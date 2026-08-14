//! OIDC transport policy, provider discovery, and runtime state for Reverie.
//!
//! This module owns two things. [`crate::auth::oidc::OidcTransport`] is the single outbound HTTP
//! policy every configured OIDC role shares: one `reqwest` connection pool,
//! bounded timeouts, no redirects, and the HTTPS constraints each endpoint kind
//! must satisfy. [`crate::auth::oidc::init_oidc_client`] performs the startup-time handshake on
//! top of it, discovering the configured issuer, validating the endpoints the
//! discovery document names, and assembling an [`crate::auth::oidc::OidcClient`] with the redirect
//! URI embedded. [`crate::auth::oidc::OidcRuntime`] pairs that client with the transport and is
//! what [`crate::state::AppState`] carries.
//!
//! ID-token verification and nonce binding happen in the OIDC callback route
//! handler, not here. This module's responsibility ends at client construction.
//!
//! # Threat model — issuer trust boundary
//!
//! The issuer URL is operator-supplied configuration. [`crate::auth::oidc::init_oidc_client`]
//! fetches the discovery document over HTTPS; TLS validation is performed by
//! the underlying `reqwest` client (system roots, no certificate override).
//! An operator pointing `OIDC_ISSUER_URL` at a malicious or compromised
//! provider can induce Reverie to trust attacker-controlled JWKS, enabling
//! ID-token forgery. This is an operator-level threat, not a user-level one;
//! the mitigation is operator key management and issuer selection.
//!
//! # Threat model — transport bounds
//!
//! Every configured OIDC egress path runs through [`crate::auth::oidc::OidcTransport`]: interactive
//! discovery, token exchange, and the resource-server JWKS fetch and its
//! discovery fallback. A client built outside it would silently reinstate the
//! `reqwest` defaults this policy exists to override, so no OIDC path may
//! construct one.
//!
//! THREAT: `reqwest` carries no default timeout, and startup discovery against
//! a provider that hangs rather than refusing never resolves, holding the boot
//! loop open indefinitely. Bounded connect and whole-request timeouts turn that
//! into a fast, diagnosable startup failure.
//!
//! THREAT: redirects are disabled. Discovery, token exchange, and JWKS
//! retrieval all carry credential or key-resolution authority, and a configured
//! endpoint that answers with a redirect would otherwise steer a
//! `client_secret`-bearing request, or the choice of signing keys, to an origin
//! the operator never configured.
//!
//! # Threat model — HTTPS-only endpoints
//!
//! OIDC Discovery requires an HTTPS issuer with no query or fragment. Reverie
//! extends that to every endpoint it will actually call: discovered
//! authorization and token endpoints keep their standards-permitted query
//! components but must be HTTPS and fragment-free, and JWKS endpoints
//! (discovered or explicitly overridden) must be HTTPS. There is no production
//! HTTP escape hatch; a cleartext IdP would expose the authorization code, the
//! `client_secret`, and the signing keys to any observer on the path.
//! Operator-selected private addresses remain valid over HTTPS, so the
//! enrichment SSRF resolver is deliberately not applied here: it blocks exactly
//! the private IdP deployments this path is expected to serve.
//!
//! # Threat model — WAF reachability
//!
//! The HTTP client used for discovery and token exchange sends an explicit
//! `User-Agent: reverie/<version>` header. `reqwest` does **not** set a
//! default `User-Agent` when one is not configured on the builder, and
//! common WAFs (e.g. Cloudflare's default scanner blocklist) drop
//! requests with an empty `User-Agent`. An empty header produces a
//! startup-time `403 Forbidden` on OIDC discovery and crashes the boot
//! loop, presenting as an availability failure rather than a security
//! one. The fixed UA also identifies the client to upstream IdP
//! operators so misbehaviour traces to a known agent. See
//! [`adr/2026-05-18-outbound-http-user-agent.md`](../../../../adr/2026-05-18-outbound-http-user-agent.md).

use std::time::Duration;

use anyhow::{Context, Result, bail};
use openidconnect::core::{CoreClient, CoreProviderMetadata};
use openidconnect::{
    ClientId, ClientSecret, EndpointMaybeSet, EndpointNotSet, EndpointSet, IssuerUrl, RedirectUrl,
};

use crate::config::Config;

/// Connect timeout shared by every configured OIDC egress path.
const OIDC_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// Whole-request timeout (connect through body) shared by every configured
/// OIDC egress path.
const OIDC_REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// Which OIDC endpoint a URL is about to be used as, selecting the constraints
/// that apply to it.
///
/// The kinds differ only in what the standards permit beyond HTTPS: an issuer
/// is a bare identifier, while authorization and token endpoints may legitimately
/// carry query components an IdP built into its URLs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OidcEndpoint {
    /// The configured issuer. HTTPS, no query, no fragment (OIDC Discovery).
    Issuer,
    /// The discovered authorization endpoint. HTTPS, no fragment; query kept.
    Authorization,
    /// The discovered token endpoint. HTTPS, no fragment; query kept.
    Token,
    /// A JWKS endpoint, discovered or explicitly overridden. HTTPS, no
    /// fragment; query kept.
    Jwks,
}

impl OidcEndpoint {
    /// Operator-facing name used in configuration-error messages.
    const fn label(self) -> &'static str {
        match self {
            Self::Issuer => "OIDC issuer URL",
            Self::Authorization => "OIDC authorization endpoint",
            Self::Token => "OIDC token endpoint",
            Self::Jwks => "OIDC JWKS endpoint",
        }
    }
}

/// Which URL schemes an OIDC endpoint may use.
///
/// Only ever [`Self::HttpsOnly`] outside `cfg(test)`. The permissive variant
/// exists so tests can drive the real startup seams against a local mock
/// provider; it is scoped to loopback hosts so that a test policy still proves
/// a remote cleartext endpoint is refused, and it has no production
/// constructor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SchemePolicy {
    HttpsOnly,
    #[cfg(test)]
    PermitLoopbackHttp,
}

impl SchemePolicy {
    /// Whether `url`'s scheme is acceptable under this policy.
    fn permits(self, url: &url::Url) -> bool {
        if url.scheme() == "https" {
            return true;
        }
        match self {
            Self::HttpsOnly => false,
            #[cfg(test)]
            Self::PermitLoopbackHttp => url.scheme() == "http" && is_loopback(url),
        }
    }
}

/// Whether `url`'s host is the local machine, the only place a `cfg(test)`
/// policy tolerates cleartext.
#[cfg(test)]
fn is_loopback(url: &url::Url) -> bool {
    match url.host() {
        Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
        Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
        Some(url::Host::Domain(name)) => name.eq_ignore_ascii_case("localhost"),
        None => false,
    }
}

/// The one HTTP client and URL policy every configured OIDC role shares.
///
/// Built once at startup when either interactive OIDC or resource-server JWT
/// validation is configured ([`transport_required`]), and cheap to clone: the
/// underlying `reqwest::Client` is `Arc`-backed, so a clone shares the pool
/// rather than opening a second one.
///
/// Two protocol wrappers sit over that single pool. [`Self::oauth_client`]
/// adapts it to the `AsyncHttpClient` interface `openidconnect` takes for
/// discovery and token exchange; [`Self::raw_client`] hands the bare client to
/// the resource-server JWKS source. They are role-specific views of the same
/// physical connections, not two clients.
#[derive(Clone, Debug)]
pub struct OidcTransport {
    http: reqwest::Client,
    scheme_policy: SchemePolicy,
}

impl OidcTransport {
    /// Build the process-wide OIDC transport under production policy.
    ///
    /// # Errors
    ///
    /// Returns an error if `reqwest` cannot initialise its TLS backend.
    pub fn new() -> Result<Self> {
        Self::build(
            SchemePolicy::HttpsOnly,
            OIDC_CONNECT_TIMEOUT,
            OIDC_REQUEST_TIMEOUT,
        )
    }

    /// Transport that tolerates the loopback `http://` mock providers the auth
    /// tests drive the real startup seams against. `cfg(test)` only: production
    /// has no HTTP escape hatch.
    #[cfg(test)]
    pub(crate) fn for_tests() -> Self {
        Self::build(
            SchemePolicy::PermitLoopbackHttp,
            OIDC_CONNECT_TIMEOUT,
            OIDC_REQUEST_TIMEOUT,
        )
        .expect("build test OIDC transport")
    }

    /// [`Self::for_tests`] with timeouts short enough for a hung-endpoint
    /// assertion to finish inside a test run.
    #[cfg(test)]
    pub(crate) fn for_tests_with_timeouts(connect: Duration, request: Duration) -> Self {
        Self::build(SchemePolicy::PermitLoopbackHttp, connect, request)
            .expect("build test OIDC transport")
    }

    fn build(
        scheme_policy: SchemePolicy,
        connect_timeout: Duration,
        request_timeout: Duration,
    ) -> Result<Self> {
        // THREAT: an empty User-Agent is matched by common WAF scanner blocklists
        // (Cloudflare, AWS WAF). Set a stable, identifiable UA so OIDC discovery
        // succeeds behind a WAF and upstream IdP operators can trace requests
        // back to a Reverie deployment.
        #[expect(
            clippy::disallowed_methods,
            reason = "sanctioned UA-setting constructor the clippy.toml ban funnels callers into; .user_agent() is set on the next line and regression-tested by transport_sends_reverie_user_agent"
        )]
        let http = reqwest::ClientBuilder::new()
            .user_agent(concat!("reverie/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(connect_timeout)
            .timeout(request_timeout)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .context("failed to build OIDC HTTP client")?;
        Ok(Self {
            http,
            scheme_policy,
        })
    }

    /// The pool adapted to the interface `openidconnect` takes for discovery
    /// and token exchange.
    pub(crate) fn oauth_client(&self) -> oauth2_reqwest::ReqwestClient {
        oauth2_reqwest::ReqwestClient::from(self.http.clone())
    }

    /// The bare pool, for the resource-server JWKS source, which drives
    /// `reqwest` directly rather than through the OAuth interface.
    pub(crate) fn raw_client(&self) -> reqwest::Client {
        self.http.clone()
    }

    /// Reject a URL Reverie is about to call as `endpoint` if it violates the
    /// constraints that endpoint kind carries.
    ///
    /// # Errors
    ///
    /// Returns an error naming the offending endpoint and constraint, so an
    /// operator configuration mistake reads as one at startup.
    pub(crate) fn check_endpoint(&self, endpoint: OidcEndpoint, url: &url::Url) -> Result<()> {
        let label = endpoint.label();
        if !self.scheme_policy.permits(url) {
            bail!("{label} must use https, got {}", url.scheme());
        }
        // An issuer is a bare identifier: OIDC Discovery derives the
        // well-known path from it, so a query would be dropped and a fragment
        // is meaningless. Authorization and token endpoints may carry query
        // components the IdP built in, but a fragment is never sent to the
        // server and would silently truncate the URL Reverie thinks it called.
        if endpoint == OidcEndpoint::Issuer && url.query().is_some() {
            bail!("{label} must not contain a query component");
        }
        if url.fragment().is_some() {
            bail!("{label} must not contain a fragment");
        }
        Ok(())
    }
}

/// Whether this process needs an OIDC transport at all.
///
/// A local-authentication-only deployment builds no client and performs no
/// discovery or JWKS request; both optional roles are absent.
pub fn transport_required(config: &Config) -> bool {
    config.oidc_configured() || config.resource_server_configured()
}

/// Fully-configured OIDC `CoreClient` with `redirect_uri` set.
///
/// The type alias spells out the endpoint state-machine parameters so that
/// callers can use [`OidcClient`] without importing the full generic form.
/// The 12th type parameter is `EndpointSet` (the auth-URL endpoint marker
/// populated by `from_provider_metadata`); the trailing two `EndpointMaybeSet`
/// markers reflect that introspection and revocation endpoints are optional in
/// the discovery document. `redirect_uri` is stored as runtime state (not
/// type-state) and is bound by `set_redirect_uri` before any call to
/// `authorize_url`.
pub type OidcClient = openidconnect::Client<
    openidconnect::EmptyAdditionalClaims,
    openidconnect::core::CoreAuthDisplay,
    openidconnect::core::CoreGenderClaim,
    openidconnect::core::CoreJweContentEncryptionAlgorithm,
    openidconnect::core::CoreJsonWebKey,
    openidconnect::core::CoreAuthPrompt,
    openidconnect::StandardErrorResponse<openidconnect::core::CoreErrorResponseType>,
    openidconnect::StandardTokenResponse<
        openidconnect::IdTokenFields<
            openidconnect::EmptyAdditionalClaims,
            openidconnect::EmptyExtraTokenFields,
            openidconnect::core::CoreGenderClaim,
            openidconnect::core::CoreJweContentEncryptionAlgorithm,
            openidconnect::core::CoreJwsSigningAlgorithm,
        >,
        openidconnect::core::CoreTokenType,
    >,
    openidconnect::StandardTokenIntrospectionResponse<
        openidconnect::EmptyExtraTokenFields,
        openidconnect::core::CoreTokenType,
    >,
    openidconnect::core::CoreRevocableToken,
    openidconnect::StandardErrorResponse<openidconnect::RevocationErrorResponseType>,
    EndpointSet,
    EndpointNotSet,
    EndpointNotSet,
    EndpointNotSet,
    EndpointMaybeSet,
    EndpointMaybeSet,
>;

/// The interactive OIDC login state [`crate::state::AppState`] carries.
///
/// Pairs the discovered client with the transport that produced it, so the
/// callback's token exchange reuses the same bounded pool instead of building
/// one per request. Clones are cheap: both fields are `Arc`-backed internally.
#[derive(Clone, Debug)]
pub struct OidcRuntime {
    client: OidcClient,
    transport: OidcTransport,
}

impl OidcRuntime {
    /// Pair a discovered client with the transport that produced it.
    pub(crate) fn new(client: OidcClient, transport: OidcTransport) -> Self {
        Self { client, transport }
    }

    /// The discovered client, for authorization-URL generation, code exchange,
    /// and ID-token verification.
    pub fn client(&self) -> &OidcClient {
        &self.client
    }

    /// The bounded transport backing this runtime.
    pub(crate) fn transport(&self) -> &OidcTransport {
        &self.transport
    }
}

/// Discover the OIDC provider and return a runtime with `redirect_uri` bound.
///
/// Performs an async HTTP GET to `{OIDC_ISSUER_URL}/.well-known/openid-configuration`
/// over `transport`, parses the provider metadata, checks the endpoints it names
/// against [`OidcTransport::check_endpoint`], and constructs an [`OidcClient`]
/// ready for authorization URL generation. Called once at startup; the result
/// is stored in [`crate::state::AppState`].
///
/// Issuer URL and redirect URI are validated before the network call; a
/// malformed or non-HTTPS URL is an operator configuration error caught at
/// startup rather than at request time.
///
/// # Threat model
///
/// TLS certificate validation is delegated to the `reqwest` default client
/// (system certificate roots). The discovery document is parsed by
/// `openidconnect::CoreProviderMetadata`; malformed documents produce an
/// error rather than a partially-constructed client. The endpoints it names
/// are checked before they can be called, so a compromised or misconfigured
/// provider cannot downgrade the authorization or token request to cleartext.
///
/// # Errors
///
/// Returns an error if `OIDC_ISSUER_URL` or `OIDC_REDIRECT_URI` is not a valid
/// URL, if the issuer violates the HTTPS/no-query/no-fragment constraint, if
/// provider discovery fails or returns an unparsable response, or if a
/// discovered endpoint violates its constraint.
pub async fn init_oidc_client(config: &Config, transport: &OidcTransport) -> Result<OidcRuntime> {
    // Validate both URLs before the network call so an operator configuration
    // error fails fast at startup rather than after a discovery round-trip that
    // would have succeeded.
    let issuer =
        IssuerUrl::new(config.oidc_issuer_url.clone()).context("invalid OIDC_ISSUER_URL")?;
    transport.check_endpoint(OidcEndpoint::Issuer, issuer.url())?;
    let redirect =
        RedirectUrl::new(config.oidc_redirect_uri.clone()).context("invalid OIDC_REDIRECT_URI")?;

    let provider_metadata = CoreProviderMetadata::discover_async(issuer, &transport.oauth_client())
        .await
        .map_err(|e| anyhow::anyhow!("OIDC discovery failed: {e}"))?;

    transport.check_endpoint(
        OidcEndpoint::Authorization,
        provider_metadata.authorization_endpoint().url(),
    )?;
    if let Some(token_endpoint) = provider_metadata.token_endpoint() {
        transport.check_endpoint(OidcEndpoint::Token, token_endpoint.url())?;
    }
    transport.check_endpoint(OidcEndpoint::Jwks, provider_metadata.jwks_uri().url())?;

    let client = CoreClient::from_provider_metadata(
        provider_metadata,
        ClientId::new(config.oidc_client_id.clone()),
        Some(ClientSecret::new(config.oidc_client_secret.clone())),
    )
    .set_redirect_uri(redirect);

    Ok(OidcRuntime::new(client, transport.clone()))
}

#[cfg(test)]
mod tests {
    use figment::Figment;

    use super::*;
    use crate::config::EnvProvider;

    fn https_only() -> OidcTransport {
        OidcTransport::new().expect("build production-policy OIDC transport")
    }

    fn config_with_overrides(overrides: &[(&str, &str)]) -> Config {
        let base: &[(&str, &str)] = &[
            ("DATABASE_URL", "postgres://test@localhost/reverie_dev"),
            (
                "DATABASE_URL_MIGRATION",
                "postgres://test@localhost/reverie_dev",
            ),
            ("OIDC_ISSUER_URL", "https://auth.example.com"),
            ("OIDC_CLIENT_ID", "test"),
            ("OIDC_CLIENT_SECRET", "secret"),
            ("OIDC_REDIRECT_URI", "http://localhost:3000/auth/callback"),
            ("REVERIE_OPDS_ENABLED", "false"),
        ];
        // base first, overrides last — later pairs win in EnvProvider's merge.
        let mut vars: Vec<(&str, &str)> = base.to_vec();
        vars.extend_from_slice(overrides);
        Config::from_figment(&Figment::from(EnvProvider::from_pairs(&vars)))
            .expect("test Config must build")
    }

    fn parse(url: &str) -> url::Url {
        url::Url::parse(url).expect("test URL parses")
    }

    /// Regression test: the OIDC HTTP client must send a non-empty
    /// `User-Agent` header. Empty UA is matched by common WAF rules
    /// (Cloudflare's default scanner-block list includes `http.user_agent
    /// eq ""`), causing OIDC discovery to 403 at startup behind such a
    /// WAF. The wiremock matcher returns 200 only on `reverie/<semver>`;
    /// any other UA — including the empty string `reqwest` sends by
    /// default when `.user_agent(...)` is not called on the builder —
    /// falls through to wiremock's default 404 and trips the assert.
    #[tokio::test]
    async fn transport_sends_reverie_user_agent() {
        use wiremock::matchers::{header_regex, method};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(header_regex("user-agent", r"^reverie/\d+\.\d+\.\d+"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        let response = OidcTransport::for_tests()
            .raw_client()
            .get(format!("{}/probe", server.uri()))
            .send()
            .await
            .expect("issue probe request");

        assert_eq!(
            response.status().as_u16(),
            200,
            "expected wiremock to match `reverie/<version>` User-Agent; \
             a missing or different UA would 404"
        );
    }

    /// THREAT: a configured endpoint must not be able to steer a
    /// credential-bearing OIDC request to another origin. The redirect target
    /// here answers 200, so a followed redirect surfaces as success.
    #[tokio::test]
    async fn transport_never_follows_redirects() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let target = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/target"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&target)
            .await;

        let redirector = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/start"))
            .respond_with(
                ResponseTemplate::new(302)
                    .insert_header("location", format!("{}/target", target.uri()).as_str()),
            )
            .mount(&redirector)
            .await;

        let response = OidcTransport::for_tests()
            .raw_client()
            .get(format!("{}/start", redirector.uri()))
            .send()
            .await
            .expect("issue redirected request");

        assert_eq!(
            response.status().as_u16(),
            302,
            "the redirect must be surfaced, not followed to an origin the \
             operator never configured"
        );
    }

    /// THREAT: an IdP that hangs rather than refusing must not hold the boot
    /// loop open. The whole-request timeout is the bound that turns it into a
    /// fast startup failure.
    #[tokio::test]
    async fn transport_fails_fast_against_hung_endpoint() {
        use std::time::Instant;

        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/hang"))
            .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(30)))
            .mount(&server)
            .await;

        let transport = OidcTransport::for_tests_with_timeouts(
            Duration::from_millis(250),
            Duration::from_millis(250),
        );

        let started = Instant::now();
        let result = transport
            .raw_client()
            .get(format!("{}/hang", server.uri()))
            .send()
            .await;

        assert!(result.is_err(), "a hung endpoint must time out");
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "the request must fail within the configured client timeout, not \
             the mock's delay"
        );
    }

    #[test]
    fn issuer_must_be_https_without_query_or_fragment() {
        let transport = https_only();

        transport
            .check_endpoint(OidcEndpoint::Issuer, &parse("https://auth.example.com"))
            .expect("a plain HTTPS issuer is valid");
        transport
            .check_endpoint(
                OidcEndpoint::Issuer,
                &parse("https://auth.example.com/realms/reverie"),
            )
            .expect("a path-bearing HTTPS issuer is valid");

        for rejected in [
            "http://auth.example.com",
            "https://auth.example.com?tenant=a",
            "https://auth.example.com#frag",
        ] {
            assert!(
                transport
                    .check_endpoint(OidcEndpoint::Issuer, &parse(rejected))
                    .is_err(),
                "issuer {rejected} must be rejected"
            );
        }
    }

    /// An operator-selected IdP on a private address is a supported deployment;
    /// only the scheme is constrained. The enrichment SSRF resolver, which
    /// blocks private addresses outright, is deliberately not applied here.
    #[test]
    fn private_address_issuer_is_valid_over_https() {
        https_only()
            .check_endpoint(
                OidcEndpoint::Issuer,
                &parse("https://10.1.2.3:8443/realms/r"),
            )
            .expect("a private-address HTTPS issuer is valid");
    }

    #[test]
    fn authorization_and_token_endpoints_keep_query_but_reject_fragment() {
        let transport = https_only();

        for endpoint in [OidcEndpoint::Authorization, OidcEndpoint::Token] {
            transport
                .check_endpoint(
                    endpoint,
                    &parse("https://auth.example.com/authorize?tenant=a"),
                )
                .expect("a standards-permitted query component is valid");

            for rejected in [
                "http://auth.example.com/authorize",
                "https://auth.example.com/authorize#frag",
            ] {
                assert!(
                    transport
                        .check_endpoint(endpoint, &parse(rejected))
                        .is_err(),
                    "{endpoint:?} {rejected} must be rejected"
                );
            }
        }
    }

    #[test]
    fn jwks_endpoint_must_be_https() {
        let transport = https_only();

        transport
            .check_endpoint(OidcEndpoint::Jwks, &parse("https://auth.example.com/jwks"))
            .expect("an HTTPS JWKS endpoint is valid");
        assert!(
            transport
                .check_endpoint(OidcEndpoint::Jwks, &parse("http://auth.example.com/jwks"))
                .is_err(),
            "a cleartext JWKS endpoint must be rejected"
        );
    }

    /// The permissive policy exists only so tests can drive the real startup
    /// seams against a local mock; it must not relax anything else.
    #[test]
    fn test_policy_permits_http_but_keeps_structural_constraints() {
        let transport = OidcTransport::for_tests();

        transport
            .check_endpoint(OidcEndpoint::Issuer, &parse("http://127.0.0.1:8080"))
            .expect("the test policy tolerates a cleartext mock issuer");
        assert!(
            transport
                .check_endpoint(OidcEndpoint::Issuer, &parse("http://127.0.0.1:8080?x=1"))
                .is_err(),
            "the test policy must still reject a query-bearing issuer"
        );
    }

    #[test]
    fn transport_is_required_only_when_an_oidc_role_is_configured() {
        // `test_config` is the local-authentication-only shape: both OIDC
        // issuer fields blank.
        let local_only = crate::test_support::test_config();
        assert!(
            !transport_required(&local_only),
            "a local-auth-only process must build no OIDC transport"
        );

        assert!(
            transport_required(&config_with_overrides(&[])),
            "interactive OIDC requires the transport"
        );

        let mut resource_server_only = crate::test_support::test_config();
        resource_server_only.resource_server_issuer = "https://auth.example.com".to_string();
        resource_server_only.resource_server_audience = "reverie-api".to_string();
        assert!(
            transport_required(&resource_server_only),
            "resource-server JWT validation alone requires the transport"
        );
    }

    /// A cleartext issuer must fail at startup before any network call, not be
    /// silently accepted the way the ASCII-only configuration check once was.
    /// The port is closed, so a regression that skipped the scheme check would
    /// surface as `OIDC discovery failed` instead.
    #[tokio::test]
    async fn init_oidc_client_rejects_http_issuer() {
        let config = config_with_overrides(&[("OIDC_ISSUER_URL", "http://127.0.0.1:1")]);

        let err = init_oidc_client(&config, &https_only())
            .await
            .expect_err("a cleartext issuer must be rejected");
        let msg = err.to_string();

        assert!(
            msg.contains("OIDC issuer URL"),
            "expected the issuer scheme rejection; got: {msg}"
        );
        assert!(
            !msg.contains("OIDC discovery failed"),
            "the issuer must be rejected before the discovery network call; got: {msg}"
        );
    }

    /// Regression test for the fail-fast validation order: a malformed
    /// `OIDC_REDIRECT_URI` must surface before any discovery network call is
    /// attempted. The issuer points at a closed port, so a regression of the
    /// validation ordering would surface as `OIDC discovery failed: ...`
    /// (connection refused) rather than the `invalid OIDC_REDIRECT_URI` we
    /// expect.
    #[tokio::test]
    async fn init_oidc_client_fails_fast_on_invalid_redirect_uri() {
        let config = config_with_overrides(&[
            ("OIDC_ISSUER_URL", "https://127.0.0.1:1"),
            ("OIDC_REDIRECT_URI", "not-a-valid-url"),
        ]);

        let err = init_oidc_client(&config, &https_only())
            .await
            .expect_err("malformed redirect URI must produce an error");
        let msg = err.to_string();

        assert!(
            msg.contains("OIDC_REDIRECT_URI"),
            "expected fail-fast on redirect URI parse before discovery; got: {msg}"
        );
        assert!(
            !msg.contains("OIDC discovery failed"),
            "redirect URI must be validated before discovery network call; got: {msg}"
        );
    }

    /// The interactive role's discovery runs over the shared transport and
    /// produces a usable client. Exercises the real startup seam end to end
    /// against the mock provider's discovery document.
    #[tokio::test]
    async fn init_oidc_client_discovers_over_the_shared_transport() {
        let mock = crate::test_support::oidc_mock::MockOidcProvider::start("test").await;
        mock.mount_discovery().await;

        let config = config_with_overrides(&[("OIDC_ISSUER_URL", mock.issuer())]);
        let runtime = init_oidc_client(&config, &OidcTransport::for_tests())
            .await
            .expect("discovery against the mock provider must succeed");

        assert!(
            runtime
                .client()
                .authorize_url(
                    openidconnect::AuthenticationFlow::<openidconnect::core::CoreResponseType>::AuthorizationCode,
                    openidconnect::CsrfToken::new_random,
                    openidconnect::Nonce::new_random,
                )
                .url()
                .0
                .as_str()
                .starts_with(mock.issuer()),
            "the discovered authorization endpoint must be used"
        );
    }

    /// A provider whose discovery document advertises a cleartext endpoint must
    /// be rejected before Reverie will send a credential to it, even though the
    /// issuer itself was HTTPS.
    #[tokio::test]
    async fn init_oidc_client_rejects_cleartext_discovered_endpoint() {
        let mock = crate::test_support::oidc_mock::MockOidcProvider::start("test").await;
        mock.mount_discovery_with_token_endpoint("http://downgraded.example.com/token")
            .await;

        let config = config_with_overrides(&[("OIDC_ISSUER_URL", mock.issuer())]);
        let err = init_oidc_client(&config, &OidcTransport::for_tests())
            .await
            .expect_err("a cleartext token endpoint must be rejected");

        assert!(
            err.to_string().contains("OIDC token endpoint"),
            "expected the token-endpoint rejection; got: {err}"
        );
    }
}
