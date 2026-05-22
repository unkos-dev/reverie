//! OIDC provider discovery and `OidcClient` construction for Reverie.
//!
//! This module owns the startup-time OIDC handshake: it performs provider
//! discovery against the configured issuer URL, parses the provider metadata
//! (JWKS URI, authorization endpoint, token endpoint), and assembles an
//! [`crate::auth::oidc::OidcClient`] with the redirect URI embedded. The resulting client is
//! stored in [`crate::state::AppState`] and reused for every login flow.
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

use anyhow::{Context, Result};
use openidconnect::core::{CoreClient, CoreProviderMetadata};
use openidconnect::{
    ClientId, ClientSecret, EndpointMaybeSet, EndpointNotSet, EndpointSet, IssuerUrl, RedirectUrl,
};

use crate::config::Config;

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

/// Build an HTTP client for OIDC discovery and token exchange.
///
/// The explicit `User-Agent` header is load-bearing: see the
/// "WAF reachability" section in the module-level docs. Removing it
/// reopens [UNK-255](https://linear.app/unkos/issue/UNK-255).
fn http_client() -> Result<openidconnect::reqwest::Client> {
    // THREAT: an empty User-Agent is matched by common WAF scanner blocklists
    // (Cloudflare, AWS WAF). Set a stable, identifiable UA so OIDC discovery
    // succeeds behind a WAF and upstream IdP operators can trace requests
    // back to a Reverie deployment.
    openidconnect::reqwest::ClientBuilder::new()
        .user_agent(concat!("reverie/", env!("CARGO_PKG_VERSION")))
        .build()
        .context("failed to build OIDC HTTP client")
}

/// Discover the OIDC provider and return a client with `redirect_uri` bound.
///
/// Performs an async HTTP GET to `{OIDC_ISSUER_URL}/.well-known/openid-configuration`,
/// parses the provider metadata, and constructs an [`OidcClient`] ready for
/// authorization URL generation. Called once at startup; the resulting client
/// is stored in [`crate::state::AppState`].
///
/// Issuer URL and redirect URI are validated for syntactic correctness before
/// the network call; a malformed URL is an operator configuration error caught
/// at startup rather than at request time.
///
/// # Threat model
///
/// TLS certificate validation is delegated to the `reqwest` default client
/// (system certificate roots). The discovery document is parsed by
/// `openidconnect::CoreProviderMetadata`; malformed documents produce an
/// error rather than a partially-constructed client.
///
/// # Errors
///
/// Returns an error if `OIDC_ISSUER_URL` or `OIDC_REDIRECT_URI` is not a
/// valid URL, if the HTTP client cannot be constructed, or if the provider
/// discovery request fails or returns an unparsable response.
pub async fn init_oidc_client(config: &Config) -> Result<OidcClient> {
    // Validate both URLs syntactically before the network call so an operator
    // configuration error fails fast at startup rather than after a discovery
    // round-trip that would have succeeded.
    let issuer =
        IssuerUrl::new(config.oidc_issuer_url.clone()).context("invalid OIDC_ISSUER_URL")?;
    let redirect =
        RedirectUrl::new(config.oidc_redirect_uri.clone()).context("invalid OIDC_REDIRECT_URI")?;

    let http = http_client()?;
    let provider_metadata = CoreProviderMetadata::discover_async(issuer, &http)
        .await
        .map_err(|e| anyhow::anyhow!("OIDC discovery failed: {e}"))?;

    let client = CoreClient::from_provider_metadata(
        provider_metadata,
        ClientId::new(config.oidc_client_id.clone()),
        Some(ClientSecret::new(config.oidc_client_secret.clone())),
    )
    .set_redirect_uri(redirect);

    Ok(client)
}

/// Build an HTTP client for use in the OIDC token-exchange step.
///
/// The token exchange (authorization code → tokens) requires an HTTP client
/// separate from the one used at discovery time; the `openidconnect` API
/// consumes it by value. This function constructs a fresh client with the
/// same `reqwest` defaults (system TLS roots, no certificate override) as the
/// discovery client.
///
/// Callers in the OIDC callback route use this to perform the confidential
/// client credential exchange; the `client_secret` is transmitted over TLS
/// to the provider's token endpoint.
///
/// # Errors
///
/// Returns an error if `reqwest` cannot initialise its TLS backend.
pub fn exchange_http_client() -> Result<openidconnect::reqwest::Client> {
    http_client()
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn config_with_overrides(overrides: &[(&str, &str)]) -> Config {
        let base: &[(&str, &str)] = &[
            ("DATABASE_URL", "postgres://test@localhost/reverie_dev"),
            ("OIDC_ISSUER_URL", "https://auth.example.com"),
            ("OIDC_CLIENT_ID", "test"),
            ("OIDC_CLIENT_SECRET", "secret"),
            ("OIDC_REDIRECT_URI", "http://localhost:3000/auth/callback"),
            ("REVERIE_OPDS_ENABLED", "false"),
        ];
        let mut vars: HashMap<String, String> = base
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect();
        for (k, v) in overrides {
            vars.insert((*k).to_string(), (*v).to_string());
        }
        Config::from_source(&|k| vars.get(k).cloned()).expect("test Config must build")
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
    async fn http_client_sends_reverie_user_agent() {
        use wiremock::matchers::{header_regex, method};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(header_regex("user-agent", r"^reverie/\d+\.\d+\.\d+"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        let client = http_client().expect("build OIDC HTTP client");
        let response = client
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

    /// Regression test for the fail-fast validation order: a malformed
    /// `OIDC_REDIRECT_URI` must surface before any discovery network call is
    /// attempted. The issuer URL is set to `http://127.0.0.1:1` (a closed
    /// port) so a regression of the validation ordering would surface as
    /// `OIDC discovery failed: ...` (connection refused) rather than the
    /// `invalid OIDC_REDIRECT_URI` we expect.
    #[tokio::test]
    async fn init_oidc_client_fails_fast_on_invalid_redirect_uri() {
        let config = config_with_overrides(&[
            ("OIDC_ISSUER_URL", "http://127.0.0.1:1"),
            ("OIDC_REDIRECT_URI", "not-a-valid-url"),
        ]);

        let err = init_oidc_client(&config)
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
}
