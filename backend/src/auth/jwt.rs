//! Resource-server JWT (RFC 9068 access token) validation for API callers.
//!
//! [`crate::auth::jwt::JwtValidator`] is built at startup
//! ([`crate::auth::jwt::init_jwt_validator`]) when resource-server config is
//! present ([`crate::config::Config::resource_server_configured`]). It
//! validates IdP-issued Bearer access tokens end to end: `iss`/`aud`
//! enforcement, the signing algorithm pinned from the resolved JWK (never
//! the token header), a JWKS fetched only from the configured URL, and the
//! `typ` header policy documented on
//! [`crate::config::Config::resource_server_require_at_jwt`]. Resolving
//! validated claims to a canonical user is `auth::middleware::resolve_jwt`'s
//! job, not this module's: [`crate::auth::jwt::JwtValidator`] owns
//! cryptographic validation only.
//!
//! # Tier 2 — security-critical
//!
//! [`jwks_client_rs::JwksClient::decode`] (the crate's own high-level
//! helper) is deliberately NOT used: it never checks `iss`, only checks
//! `aud` when a non-empty slice is passed, and falls back to
//! `Validation::default()` (HS256) when a JWK omits `alg`, all footguns
//! for a resource server. [`crate::auth::jwt::JwtValidator::validate`]
//! instead uses [`jsonwebtoken::decode`] and
//! [`jwks_client_rs::JwksClient::get`] as building blocks, with
//! `// THREAT:` annotations marking every step whose absence would open a
//! bypass.

use std::str::FromStr;
use std::time::Duration;

use jsonwebtoken::errors::{ErrorKind, new_error};
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header};
use jwks_client_rs::source::JwksSource;
use jwks_client_rs::{JsonWebKey, JsonWebKeySet, JwksClient, JwksClientError};
use serde::Deserialize;

use crate::config::Config;

/// Claims read from a validated RFC 9068 access token.
///
/// Presence of `exp`/`iss`/`aud`/`sub` is forced by `Validation::required_spec_claims`
/// before this struct is ever populated; `iss`/`sub` are re-read here for
/// identity resolution. `scope` (RFC 8693 §4.2) is a single space-delimited
/// string on the wire, not a JSON array.
#[derive(Debug, serde::Deserialize)]
pub struct AccessTokenClaims {
    /// Trusted issuer (`iss`), matched against
    /// [`crate::config::Config::resource_server_issuer`] by the wrapper
    /// before this struct is populated. Re-read here for the `(iss, sub)`
    /// identity lookup.
    pub iss: String,
    /// Provider-asserted subject (`sub`), paired with `iss` for identity
    /// resolution.
    pub sub: String,
    /// Claimed scope names, parsed from the space-delimited wire string.
    /// `None` when the claim is absent (session-parity: full role-derived
    /// set); `Some(vec![])` when present but empty, a distinct, narrower
    /// signal that callers must not treat as "absent" (an over-narrowed
    /// grant of zero capability is not the same as no narrowing requested).
    #[serde(default, deserialize_with = "deserialize_scope")]
    pub scope: Option<Vec<String>>,
}

/// Deserialize the RFC 8693 §4.2 `scope` claim: a single space-delimited
/// string (`"read write"`), never a JSON array. Only invoked when the claim
/// is present; `#[serde(default)]` on the field handles absence (`None`)
/// without calling this.
fn deserialize_scope<'de, D>(deserializer: D) -> Result<Option<Vec<String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw = String::deserialize(deserializer)?;
    Ok(Some(raw.split_whitespace().map(String::from).collect()))
}

/// Opaque validation failure.
///
/// Every rejection reason collapses to the same
/// caller-visible outcome (`AppError::InvalidCredential`, RFC 6750 §3.1
/// `error="invalid_token"`); the specific failure mode is captured only in
/// the `tracing::warn!` emitted at the point of rejection; never in the
/// return value or the HTTP response body (no oracle, hard rule 3).
#[derive(Debug)]
pub struct JwtValidationError;

impl std::fmt::Display for JwtValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("resource-server JWT validation failed")
    }
}

impl std::error::Error for JwtValidationError {}

/// [`JwksSource`] carrying the project `User-Agent`.
/// `jwks_client_rs::source::WebSource` cannot set one, and a bare-UA fetch
/// 403s behind common WAFs (mirrors [`crate::auth::oidc::http_client`]'s
/// THREAT note). Fetches ONLY from the configured `url`; this type has no
/// `jku`/`x5u` code path at all, and its client never follows HTTP
/// redirects, so a token can never redirect key resolution to an
/// attacker-chosen JWKS endpoint.
struct ReverieJwksSource {
    client: reqwest::Client,
    url: url::Url,
}

#[async_trait::async_trait]
impl JwksSource for ReverieJwksSource {
    async fn fetch_keys(&self) -> Result<JsonWebKeySet, JwksClientError> {
        // `jsonwebtoken::errors::ErrorKind::Provider` is the crate's documented
        // escape hatch for custom providers: `JwksClientError`'s only public
        // constructor path is `From<jsonwebtoken::errors::Error>` (its own
        // `Error` type is crate-private, so we cannot build a
        // `JwksClientError` any other way from outside the crate).
        let provider_error = |context: &str, err: &dyn std::fmt::Display| -> JwksClientError {
            new_error(ErrorKind::Provider(format!("{context}: {err}"))).into()
        };
        let response = self
            .client
            .get(self.url.clone())
            .send()
            .await
            .map_err(|e| provider_error("JWKS fetch failed", &e))?
            .error_for_status()
            .map_err(|e| provider_error("JWKS endpoint returned an error status", &e))?;
        response
            .json::<JsonWebKeySet>()
            .await
            .map_err(|e| provider_error("JWKS response was not valid JSON", &e))
    }
}

/// Connect timeout for the JWKS fetch client. A bare `reqwest::Client`
/// carries NO default timeout; `jwks_client_rs::source::WebSource` (which
/// [`ReverieJwksSource`] replaces to set a `User-Agent`) sets its own, so
/// the replacement must too.
const JWKS_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// Full-request timeout for the JWKS fetch client (connect through body).
const JWKS_REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// Build the UA-carrying HTTP client used by [`ReverieJwksSource`] to fetch
/// the resource-server JWKS. Mirrors `services::enrichment::http::api_client`
/// and `auth::oidc::http_client`: an empty `User-Agent` is matched by common
/// WAF scanner blocklists (Cloudflare, AWS WAF), so every outbound client in
/// this codebase is built through a sanctioned UA-setting constructor (see
/// `docs/adr/0007-outbound-http-clients-send-an-explicit-user-agent.md`).
///
/// THREAT: the timeouts are availability defenses on the auth hot path.
/// `jwks_client_rs`'s cache serves a previously cached key only after a
/// failed refresh RESOLVES; a fetch with no timeout against an `IdP` that
/// hangs (rather than errors fast) never resolves, holding every request
/// that triggered the refresh open and defeating that warm-cache fallback.
/// Redirects are never followed: the operator-configured (or
/// discovery-resolved) JWKS URL is the final endpoint, and following a
/// redirect would let the response steer key resolution to a different one.
fn jwks_http_client(
    connect_timeout: Duration,
    request_timeout: Duration,
) -> anyhow::Result<reqwest::Client> {
    use anyhow::Context as _;

    #[expect(
        clippy::disallowed_methods,
        reason = "sanctioned UA-setting constructor the clippy.toml ban funnels callers into; .user_agent() is set on the next line"
    )]
    reqwest::ClientBuilder::new()
        .user_agent(concat!("reverie/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(connect_timeout)
        .timeout(request_timeout)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .context("failed to build resource-server JWKS HTTP client")
}

/// Build a `jsonwebtoken::DecodingKey` from a resolved JWK. Mirrors
/// `jwks_client_rs::JwksClient::decode`'s own key-construction match: its
/// `JsonWebKey` enum derives `Deserialize` only (not `Serialize`), so a
/// serde round-trip into `jsonwebtoken::jwk::Jwk` is not available; these
/// are the same `jsonwebtoken::DecodingKey` constructors the crate's own
/// high-level `decode()` uses internally.
fn decoding_key_from_jwk(jwk: &JsonWebKey) -> jsonwebtoken::errors::Result<DecodingKey> {
    match jwk {
        JsonWebKey::Rsa(rsa) => DecodingKey::from_rsa_components(rsa.modulus(), rsa.exponent()),
        JsonWebKey::Ec(ec) => DecodingKey::from_ec_components(ec.x(), ec.y()),
        JsonWebKey::Okp(okp) => DecodingKey::from_ed_components(okp.x()),
    }
}

/// Validated, JWKS-backed access-token validator built once at startup.
///
/// Built from [`Config`]'s resource-server fields ([`init_jwt_validator`]). `None` on
/// [`crate::state::AppState`] when resource-server JWT validation is not
/// configured.
pub struct JwtValidator {
    client: JwksClient<ReverieJwksSource>,
    issuer: String,
    audience: String,
    require_at_jwt: bool,
}

impl JwtValidator {
    /// Validate a Bearer credential end to end: `typ` policy,
    /// JWKS-resolved and JWK-pinned signature/algorithm, then
    /// `iss`/`aud`/`exp`/`sub` presence and correctness. Every failure is
    /// logged with its specific mode via `tracing::warn!` and collapses to
    /// [`JwtValidationError`], never echoing detail to the caller (hard
    /// rule 3; avoids a validation oracle).
    ///
    /// # Errors
    ///
    /// Returns [`JwtValidationError`] on any validation failure: malformed
    /// header, a rejected `typ`, an unknown `kid`, a JWK that omits `alg`,
    /// an unsupported JWK shape, or a `jsonwebtoken::decode` failure
    /// (signature, `exp`, `nbf`, `iss`, `aud`, or a missing required claim).
    pub async fn validate(&self, token: &str) -> Result<AccessTokenClaims, JwtValidationError> {
        // Step 1: unverified peek at kid/typ. Safe: decode_header performs
        // no signature check; it only base64/JSON-decodes the header
        // segment.
        let header = decode_header(token).map_err(|e| Self::reject("malformed header", &e))?;

        // Step 2: typ policy.
        self.check_typ(header.typ.as_deref())?;

        let kid = header
            .kid
            .ok_or_else(|| Self::reject_msg("missing kid in token header"))?;

        // Step 3: JWK from the cached, rotating key set, fetched ONLY from
        // the configured JWKS URL (ReverieJwksSource has no jku/x5u code
        // path at all).
        let jwk = self
            .client
            .get(&kid)
            .await
            .map_err(|e| Self::reject("JWKS lookup failed", &e))?;

        // Step 4: pin alg from the JWK; REJECT a keyless/alg-less JWK
        // explicitly: never fall back, and never read the token header's
        // own `alg` (the classic alg-confusion vector this wrapper exists
        // to close; jwks_client_rs's own high-level `decode()` instead falls
        // back to HS256 here, which is exactly the footgun this wrapper
        // forbids).
        let alg_str = jwk.alg().ok_or_else(|| Self::reject_msg("JWK omits alg"))?;
        let algorithm =
            Algorithm::from_str(alg_str).map_err(|e| Self::reject("unsupported JWK alg", &e))?;
        let decoding_key =
            decoding_key_from_jwk(&jwk).map_err(|e| Self::reject("unsupported JWK shape", &e))?;

        // Step 5: iss/aud/required-claims are wrapper-owned. jsonwebtoken
        // skips the iss/aud/sub checks entirely when the claim is ABSENT,
        // regardless of set_issuer/set_audience: only
        // set_required_spec_claims forces presence, so it is load-bearing
        // here, not decorative.
        let mut validation = Validation::new(algorithm);
        validation.set_issuer(&[self.issuer.as_str()]);
        validation.set_audience(&[self.audience.as_str()]);
        validation.set_required_spec_claims(&["exp", "iss", "aud", "sub"]);
        validation.validate_nbf = true;

        // Step 6.
        let data = decode::<AccessTokenClaims>(token, &decoding_key, &validation)
            .map_err(|e| Self::reject("token validation failed", &e))?;
        Ok(data.claims)
    }

    /// RFC 9068 §2.1 `typ` policy, case-insensitive on
    /// the media type: `at+jwt` / `application/at+jwt` always pass. A bare
    /// `JWT` or an absent `typ` pass only when `require_at_jwt` is `false`
    /// (the default). Any OTHER explicit `typ` (e.g. `logout+jwt`,
    /// `dpop+jwt`) is rejected in BOTH modes, closing the replay of a
    /// same-key token declared for another purpose (Authentik, for one,
    /// signs logout tokens with the same key as access tokens).
    fn check_typ(&self, typ: Option<&str>) -> Result<(), JwtValidationError> {
        let normalized = typ.map(str::to_ascii_lowercase);
        match normalized.as_deref() {
            Some("at+jwt" | "application/at+jwt") => Ok(()),
            Some("jwt") | None if !self.require_at_jwt => Ok(()),
            Some(other) => Err(Self::reject_msg(&format!("typ {other:?} rejected"))),
            None => Err(Self::reject_msg("typ absent (strict mode requires at+jwt)")),
        }
    }

    /// Log the specific failure mode and return the opaque
    /// [`JwtValidationError`] every caller-visible outcome collapses to.
    fn reject(context: &str, err: &dyn std::fmt::Display) -> JwtValidationError {
        tracing::warn!(reason = context, error = %err, "resource-server JWT rejected");
        JwtValidationError
    }

    /// [`Self::reject`] for failure modes with no underlying error value.
    fn reject_msg(context: &str) -> JwtValidationError {
        tracing::warn!(reason = context, "resource-server JWT rejected");
        JwtValidationError
    }
}

/// Build a [`JwtValidator`] from resource-server config, called once at
/// startup.
///
/// Called beside [`crate::auth::oidc::init_oidc_client`] when
/// [`Config::resource_server_configured`] is `true`. The JWKS URL is the
/// explicit `resource_server_jwks_url` override when set, otherwise it is
/// derived via OIDC discovery against `resource_server_issuer`, the same
/// discovery mechanism [`crate::auth::oidc::init_oidc_client`] uses, reusing
/// its UA-carrying HTTP client.
///
/// # Errors
///
/// Returns an error if `resource_server_issuer` or an explicit
/// `resource_server_jwks_url` is not a valid URL, or if OIDC discovery
/// against the issuer fails (no explicit override configured).
pub async fn init_jwt_validator(config: &Config) -> anyhow::Result<JwtValidator> {
    use anyhow::Context as _;

    let jwks_url = if config.resource_server_jwks_url.trim().is_empty() {
        let issuer = openidconnect::IssuerUrl::new(config.resource_server_issuer.clone())
            .context("invalid REVERIE_RESOURCE_SERVER_ISSUER")?;
        let http = crate::auth::oidc::http_client()?;
        let metadata = openidconnect::core::CoreProviderMetadata::discover_async(issuer, &http)
            .await
            .map_err(|e| anyhow::anyhow!("resource-server JWKS discovery failed: {e}"))?;
        metadata.jwks_uri().url().clone()
    } else {
        url::Url::parse(&config.resource_server_jwks_url)
            .context("invalid REVERIE_RESOURCE_SERVER_JWKS_URL")?
    };

    let source = ReverieJwksSource {
        client: jwks_http_client(JWKS_CONNECT_TIMEOUT, JWKS_REQUEST_TIMEOUT)?,
        url: jwks_url,
    };
    let client = JwksClient::builder().build(source);

    Ok(JwtValidator {
        client,
        issuer: config.resource_server_issuer.clone(),
        audience: config.resource_server_audience.clone(),
        require_at_jwt: config.resource_server_require_at_jwt,
    })
}

#[cfg(test)]
mod tests {
    use jsonwebtoken::{Algorithm, EncodingKey, Header};

    use super::*;
    use crate::test_support::oidc_mock::{MockOidcProvider, now_unix};

    const AUDIENCE: &str = "reverie-api";

    /// Build a [`JwtValidator`] pointed at `mock`'s resource-server JWKS
    /// endpoint via the explicit-override path (no OIDC discovery network
    /// call), through the real [`init_jwt_validator`] startup seam.
    async fn build_validator(mock: &MockOidcProvider, require_at_jwt: bool) -> JwtValidator {
        let mut config = crate::test_support::test_config();
        config.resource_server_issuer = mock.issuer().to_string();
        config.resource_server_audience = AUDIENCE.to_string();
        config.resource_server_jwks_url = mock.resource_server_jwks_url();
        config.resource_server_require_at_jwt = require_at_jwt;
        init_jwt_validator(&config)
            .await
            .expect("build validator against the mock JWKS")
    }

    /// Minimal valid RFC 9068 claim set: `iss`/`aud`/`sub`/`exp` all present
    /// and correct. Individual tests mutate/remove fields from this via
    /// `serde_json::Value` (not a typed struct) to construct the exact
    /// negative shapes each test needs.
    fn base_claims(iss: &str, sub: &str) -> serde_json::Value {
        let now = now_unix();
        serde_json::json!({
            "iss": iss,
            "aud": AUDIENCE,
            "sub": sub,
            "exp": now + 300,
            "iat": now,
        })
    }

    fn remove_claim(mut claims: serde_json::Value, key: &str) -> serde_json::Value {
        claims
            .as_object_mut()
            .expect("claims is a JSON object")
            .remove(key);
        claims
    }

    #[tokio::test]
    async fn accepts_valid_token() {
        let mock = MockOidcProvider::start("").await;
        mock.mount_resource_server_jwks().await;
        let validator = build_validator(&mock, false).await;

        let claims = base_claims(mock.issuer(), "user-1");
        let token = mock.sign_access_token(&claims, |_| {});

        let validated = validator
            .validate(&token)
            .await
            .expect("token must validate");
        assert_eq!(validated.iss, mock.issuer());
        assert_eq!(validated.sub, "user-1");
        assert_eq!(validated.scope, None);
    }

    /// RFC 7519 §4.1.3 permits `aud` as either a single string or an array
    /// of strings; real `IdPs` (Keycloak among them) emit the array form even
    /// for a single configured audience. `jsonwebtoken` deserializes both
    /// shapes into the same internal set for `Validation::set_audience` to
    /// check against, but that's a claim about the dependency, not this
    /// wrapper — assert it directly rather than trust it silently.
    #[tokio::test]
    async fn accepts_array_form_audience() {
        let mock = MockOidcProvider::start("").await;
        mock.mount_resource_server_jwks().await;
        let validator = build_validator(&mock, false).await;

        let mut claims = base_claims(mock.issuer(), "user-1");
        claims["aud"] = serde_json::json!([AUDIENCE, "other-service"]);
        let token = mock.sign_access_token(&claims, |_| {});

        validator
            .validate(&token)
            .await
            .expect("array-form aud containing the configured audience must validate");
    }

    #[tokio::test]
    async fn rejects_wrong_issuer() {
        let mock = MockOidcProvider::start("").await;
        mock.mount_resource_server_jwks().await;
        let validator = build_validator(&mock, false).await;

        let claims = base_claims("https://wrong-issuer.example.com", "user-1");
        let token = mock.sign_access_token(&claims, |_| {});

        assert!(validator.validate(&token).await.is_err());
    }

    #[tokio::test]
    async fn rejects_missing_issuer_claim() {
        let mock = MockOidcProvider::start("").await;
        mock.mount_resource_server_jwks().await;
        let validator = build_validator(&mock, false).await;

        let claims = remove_claim(base_claims(mock.issuer(), "user-1"), "iss");
        let token = mock.sign_access_token(&claims, |_| {});

        assert!(validator.validate(&token).await.is_err());
    }

    #[tokio::test]
    async fn rejects_wrong_audience() {
        let mock = MockOidcProvider::start("").await;
        mock.mount_resource_server_jwks().await;
        let validator = build_validator(&mock, false).await;

        let mut claims = base_claims(mock.issuer(), "user-1");
        claims["aud"] = serde_json::json!("some-other-audience");
        let token = mock.sign_access_token(&claims, |_| {});

        assert!(validator.validate(&token).await.is_err());
    }

    #[tokio::test]
    async fn rejects_missing_audience_claim() {
        let mock = MockOidcProvider::start("").await;
        mock.mount_resource_server_jwks().await;
        let validator = build_validator(&mock, false).await;

        let claims = remove_claim(base_claims(mock.issuer(), "user-1"), "aud");
        let token = mock.sign_access_token(&claims, |_| {});

        assert!(validator.validate(&token).await.is_err());
    }

    #[tokio::test]
    async fn rejects_missing_exp() {
        let mock = MockOidcProvider::start("").await;
        mock.mount_resource_server_jwks().await;
        let validator = build_validator(&mock, false).await;

        let claims = remove_claim(base_claims(mock.issuer(), "user-1"), "exp");
        let token = mock.sign_access_token(&claims, |_| {});

        assert!(validator.validate(&token).await.is_err());
    }

    #[tokio::test]
    async fn rejects_missing_sub() {
        let mock = MockOidcProvider::start("").await;
        mock.mount_resource_server_jwks().await;
        let validator = build_validator(&mock, false).await;

        let claims = remove_claim(base_claims(mock.issuer(), "user-1"), "sub");
        let token = mock.sign_access_token(&claims, |_| {});

        assert!(validator.validate(&token).await.is_err());
    }

    /// Classic alg-confusion probe: a token declaring `alg: HS256` in its
    /// header, carrying the real `kid`. The real key is RSA (`alg: RS256`
    /// in the served JWK), so the wrapper pins `Algorithm::RS256` from the
    /// JWK regardless of the token's own header — `jsonwebtoken::decode`
    /// then rejects on the alg mismatch before any signature bytes are
    /// even compared, so the HMAC secret's actual value is irrelevant.
    #[tokio::test]
    async fn rejects_hs256_signed_with_public_key() {
        let mock = MockOidcProvider::start("").await;
        mock.mount_resource_server_jwks().await;
        let validator = build_validator(&mock, false).await;

        let mut header = Header::new(Algorithm::HS256);
        header.kid = Some(mock.kid().to_string());
        header.typ = Some("at+jwt".to_string());
        let claims = base_claims(mock.issuer(), "user-1");
        let key = EncodingKey::from_secret(b"attacker-guessed-hmac-secret");
        let token = jsonwebtoken::encode(&header, &claims, &key).expect("sign HS256 probe token");

        assert!(validator.validate(&token).await.is_err());
    }

    #[tokio::test]
    async fn accepts_at_jwt_typ_in_both_modes() {
        for require_at_jwt in [false, true] {
            let mock = MockOidcProvider::start("").await;
            mock.mount_resource_server_jwks().await;
            let validator = build_validator(&mock, require_at_jwt).await;

            let claims = base_claims(mock.issuer(), "user-1");
            let token = mock.sign_access_token(&claims, |h| {
                h.typ = Some("at+jwt".to_string());
            });
            assert!(
                validator.validate(&token).await.is_ok(),
                "at+jwt must pass with require_at_jwt={require_at_jwt}"
            );

            let mock2 = MockOidcProvider::start("").await;
            mock2.mount_resource_server_jwks().await;
            let validator2 = build_validator(&mock2, require_at_jwt).await;
            let claims2 = base_claims(mock2.issuer(), "user-1");
            let token2 = mock2.sign_access_token(&claims2, |h| {
                h.typ = Some("application/at+jwt".to_string());
            });
            assert!(
                validator2.validate(&token2).await.is_ok(),
                "application/at+jwt must pass with require_at_jwt={require_at_jwt}"
            );
        }
    }

    #[tokio::test]
    async fn relaxed_accepts_typ_jwt_and_absent_typ() {
        let mock = MockOidcProvider::start("").await;
        mock.mount_resource_server_jwks().await;
        let validator = build_validator(&mock, false).await;

        let claims = base_claims(mock.issuer(), "user-1");
        let token = mock.sign_access_token(&claims, |h| h.typ = Some("JWT".to_string()));
        assert!(validator.validate(&token).await.is_ok());

        let claims_absent = base_claims(mock.issuer(), "user-1");
        let token_absent = mock.sign_access_token(&claims_absent, |h| h.typ = None);
        assert!(validator.validate(&token_absent).await.is_ok());
    }

    #[tokio::test]
    async fn strict_rejects_typ_jwt_and_absent_typ() {
        let mock = MockOidcProvider::start("").await;
        mock.mount_resource_server_jwks().await;
        let validator = build_validator(&mock, true).await;

        let claims = base_claims(mock.issuer(), "user-1");
        let token = mock.sign_access_token(&claims, |h| h.typ = Some("JWT".to_string()));
        assert!(validator.validate(&token).await.is_err());

        let claims_absent = base_claims(mock.issuer(), "user-1");
        let token_absent = mock.sign_access_token(&claims_absent, |h| h.typ = None);
        assert!(validator.validate(&token_absent).await.is_err());
    }

    #[tokio::test]
    async fn rejects_foreign_typ_in_both_modes() {
        for require_at_jwt in [false, true] {
            let mock = MockOidcProvider::start("").await;
            mock.mount_resource_server_jwks().await;
            let validator = build_validator(&mock, require_at_jwt).await;

            let claims = base_claims(mock.issuer(), "user-1");
            let token = mock.sign_access_token(&claims, |h| h.typ = Some("logout+jwt".to_string()));
            assert!(
                validator.validate(&token).await.is_err(),
                "logout+jwt must be rejected with require_at_jwt={require_at_jwt}"
            );
        }
    }

    #[tokio::test]
    async fn rejects_expired_token() {
        let mock = MockOidcProvider::start("").await;
        mock.mount_resource_server_jwks().await;
        let validator = build_validator(&mock, false).await;

        let mut claims = base_claims(mock.issuer(), "user-1");
        // Well past jsonwebtoken's default 60s leeway.
        claims["exp"] = serde_json::json!(now_unix() - 3600);
        let token = mock.sign_access_token(&claims, |_| {});

        assert!(validator.validate(&token).await.is_err());
    }

    #[tokio::test]
    async fn rejects_future_nbf() {
        let mock = MockOidcProvider::start("").await;
        mock.mount_resource_server_jwks().await;
        let validator = build_validator(&mock, false).await;

        let mut claims = base_claims(mock.issuer(), "user-1");
        claims["nbf"] = serde_json::json!(now_unix() + 3600);
        let token = mock.sign_access_token(&claims, |_| {});

        assert!(validator.validate(&token).await.is_err());
    }

    #[tokio::test]
    async fn rejects_unknown_kid() {
        let mock = MockOidcProvider::start("").await;
        mock.mount_resource_server_jwks().await;
        let validator = build_validator(&mock, false).await;

        let claims = base_claims(mock.issuer(), "user-1");
        let token =
            mock.sign_access_token(&claims, |h| h.kid = Some("totally-unknown-kid".to_string()));

        assert!(validator.validate(&token).await.is_err());
    }

    /// A `jku`/`x5u` header pointing at an attacker-controlled
    /// JWKS (served on a second wiremock, with its own distinct keypair)
    /// must never be followed. The attacker signs with their own key but
    /// reuses the real `kid`, so a validator that (incorrectly) followed
    /// `jku` would resolve the attacker's key and the signature would
    /// verify; because [`ReverieJwksSource`] fetches only from the
    /// configured URL, the wrapper instead resolves the REAL key for that
    /// `kid` and signature verification correctly fails.
    #[tokio::test]
    async fn never_follows_jku_x5u() {
        let mock = MockOidcProvider::start("").await;
        mock.mount_resource_server_jwks().await;
        let attacker = MockOidcProvider::start("").await;
        attacker.mount_resource_server_jwks().await;
        let validator = build_validator(&mock, false).await;

        let claims = base_claims(mock.issuer(), "user-1");
        let forged_token = attacker.sign_access_token(&claims, |h| {
            h.kid = Some(mock.kid().to_string());
            h.jku = Some(attacker.resource_server_jwks_url());
            h.x5u = Some(attacker.resource_server_jwks_url());
        });

        assert!(validator.validate(&forged_token).await.is_err());
    }

    /// CVE-2026-25537 regression: a string-typed `exp` must be REJECTED
    /// (type confusion silently skipped the check before jsonwebtoken
    /// 10.3.0). `serde_json::json!` with a quoted number produces exactly
    /// this wire shape.
    #[tokio::test]
    async fn rejects_string_typed_exp_nbf() {
        let mock = MockOidcProvider::start("").await;
        mock.mount_resource_server_jwks().await;
        let validator = build_validator(&mock, false).await;

        let mut claims = base_claims(mock.issuer(), "user-1");
        claims["exp"] = serde_json::json!((now_unix() + 300).to_string());
        let token = mock.sign_access_token(&claims, |_| {});

        assert!(validator.validate(&token).await.is_err());
    }

    #[tokio::test]
    async fn jwk_without_alg_is_rejected() {
        // Reuse the OIDC `/jwks` endpoint (CoreJsonWebKey::new_rsa omits
        // `alg` by construction) instead of `mount_resource_server_jwks`,
        // so the JWKS URL genuinely serves an alg-less key.
        let mock = MockOidcProvider::start("").await;
        let mut config = crate::test_support::test_config();
        config.resource_server_issuer = mock.issuer().to_string();
        config.resource_server_audience = AUDIENCE.to_string();
        config.resource_server_jwks_url = format!("{}/jwks", mock.issuer());
        let validator = init_jwt_validator(&config)
            .await
            .expect("build validator against the alg-less JWKS");

        let claims = base_claims(mock.issuer(), "user-1");
        let token = mock.sign_access_token(&claims, |_| {});

        assert!(validator.validate(&token).await.is_err());
    }

    #[tokio::test]
    async fn jwks_fetch_fails_fast_against_hung_endpoint() {
        use std::time::Instant;

        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/jwks"))
            .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(30)))
            .mount(&server)
            .await;

        let source = ReverieJwksSource {
            client: jwks_http_client(Duration::from_millis(250), Duration::from_millis(250))
                .expect("build JWKS test client"),
            url: url::Url::parse(&format!("{}/jwks", server.uri())).expect("mock JWKS url parses"),
        };

        let started = Instant::now();
        let result = source.fetch_keys().await;

        assert!(
            result.is_err(),
            "a hung JWKS endpoint must time out, not hang the fetch"
        );
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "the fetch must fail within the configured client timeout, not the mock's delay"
        );
    }

    #[tokio::test]
    async fn jwks_fetch_never_follows_redirects() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let real = MockOidcProvider::start("").await;
        real.mount_resource_server_jwks().await;

        let redirector = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/jwks"))
            .respond_with(
                ResponseTemplate::new(302)
                    .insert_header("location", real.resource_server_jwks_url().as_str()),
            )
            .mount(&redirector)
            .await;

        let source = ReverieJwksSource {
            client: jwks_http_client(JWKS_CONNECT_TIMEOUT, JWKS_REQUEST_TIMEOUT)
                .expect("build JWKS test client"),
            url: url::Url::parse(&format!("{}/jwks", redirector.uri()))
                .expect("mock JWKS url parses"),
        };

        assert!(
            source.fetch_keys().await.is_err(),
            "a redirect response must fail the fetch, not steer it to the redirect target \
             (the target here serves a fully valid JWKS, so following it would succeed)"
        );
    }
}
