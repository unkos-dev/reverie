//! Shared outbound OIDC transport and startup provider discovery.
//!
//! Discovery, token exchange and resource-server JWKS retrieval share one bounded
//! HTTPS client. Operator-selected private HTTPS providers remain supported.
//! TLS uses the platform trust store; there is no certificate override.
//!
//! THREAT: the operator-selected issuer controls trusted signing keys.
//! THREAT: disabled redirects prevent an endpoint from forwarding credentials or key resolution.
//! THREAT: timeouts bound stalled provider requests, including JWKS cache refreshes.

use secrecy::ExposeSecret;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use openidconnect::core::{CoreClient, CoreProviderMetadata};
use openidconnect::{
    ClientId, ClientSecret, EndpointMaybeSet, EndpointNotSet, EndpointSet, IssuerUrl, RedirectUrl,
};

use crate::config::Config;

const OIDC_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const OIDC_REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// Endpoint kinds select the URL components permitted by OIDC and OAuth.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OidcEndpoint {
    /// HTTPS issuer identifier without a query or fragment.
    Issuer,
    /// HTTPS authorization endpoint; queries are permitted, fragments are not.
    Authorization,
    /// HTTPS token endpoint; queries are permitted, fragments are not.
    Token,
    /// HTTPS signing-key endpoint; queries are permitted, fragments are not.
    Jwks,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SchemePolicy {
    HttpsOnly,
    #[cfg(test)]
    PermitLoopbackHttp,
}

impl SchemePolicy {
    fn permits(self, url: &url::Url) -> bool {
        if url.scheme() == "https" {
            return true;
        }
        match self {
            Self::HttpsOnly => false,
            #[cfg(test)]
            Self::PermitLoopbackHttp => {
                let loopback = match url.host() {
                    Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
                    Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
                    Some(url::Host::Domain(name)) => name.eq_ignore_ascii_case("localhost"),
                    None => false,
                };
                url.scheme() == "http" && loopback
            }
        }
    }
}

/// One connection pool shared by configured interactive and resource-server OIDC roles.
///
/// Clones share the underlying pool. The OAuth adapter and direct JWKS client
/// use the same timeouts, redirect policy and User-Agent.
#[derive(Clone, Debug)]
pub struct OidcTransport {
    http: reqwest::Client,
    scheme_policy: SchemePolicy,
}

impl OidcTransport {
    /// Build an HTTPS-only client with 5-second connect and 10-second total timeouts.
    ///
    /// # Errors
    ///
    /// Returns an error if the HTTP client cannot be initialised.
    pub fn new() -> Result<Self> {
        Self::build(
            SchemePolicy::HttpsOnly,
            OIDC_CONNECT_TIMEOUT,
            OIDC_REQUEST_TIMEOUT,
        )
    }

    #[cfg(test)]
    pub(crate) fn for_tests() -> Self {
        Self::for_tests_with_timeouts(OIDC_CONNECT_TIMEOUT, OIDC_REQUEST_TIMEOUT)
    }

    #[cfg(test)]
    pub(crate) fn for_tests_with_timeouts(connect: Duration, request: Duration) -> Self {
        Self::build(SchemePolicy::PermitLoopbackHttp, connect, request)
            .expect("build test OIDC transport")
    }

    fn build(scheme_policy: SchemePolicy, connect: Duration, request: Duration) -> Result<Self> {
        // THREAT: an empty User-Agent caused startup discovery to fail behind a WAF.
        #[expect(
            clippy::disallowed_methods,
            reason = "sanctioned constructor sets the project User-Agent"
        )]
        let http = reqwest::ClientBuilder::new()
            .user_agent(concat!("reverie/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(connect)
            .timeout(request)
            .redirect(reqwest::redirect::Policy::none())
            // THREAT: discover_async fetches the discovered JWKS before endpoint validation can run.
            .https_only(scheme_policy == SchemePolicy::HttpsOnly)
            .build()
            .context("failed to build OIDC HTTP client")?;
        Ok(Self {
            http,
            scheme_policy,
        })
    }

    pub(crate) fn oauth_client(&self) -> oauth2_reqwest::ReqwestClient {
        oauth2_reqwest::ReqwestClient::from(self.http.clone())
    }

    pub(crate) fn raw_client(&self) -> reqwest::Client {
        self.http.clone()
    }

    pub(crate) fn discovery_error(
        &self,
        error: openidconnect::DiscoveryError<openidconnect::HttpClientError<reqwest::Error>>,
    ) -> anyhow::Error {
        match error {
            openidconnect::DiscoveryError::Request(openidconnect::HttpClientError::Reqwest(
                error,
            )) => {
                let endpoint_error = error.url().and_then(|url| {
                    self.check_endpoint(OidcEndpoint::Jwks, "discovery document jwks_uri", url)
                        .err()
                });
                let error = anyhow::Error::new(error.without_url());
                match endpoint_error {
                    Some(context) => error.context(context),
                    None => error,
                }
            }
            error => anyhow::Error::new(error),
        }
    }

    /// Validate an endpoint and name its configuration or discovery source on failure.
    ///
    /// # Errors
    ///
    /// Rejects cleartext URLs, all fragments, and queries on issuer identifiers.
    pub(crate) fn check_endpoint(
        &self,
        endpoint: OidcEndpoint,
        source: &'static str,
        url: &url::Url,
    ) -> Result<()> {
        if !self.scheme_policy.permits(url) {
            bail!("{source} must use https, got {}", url.scheme());
        }
        if endpoint == OidcEndpoint::Issuer && url.query().is_some() {
            bail!("{source} must not contain a query component");
        }
        if url.fragment().is_some() {
            bail!("{source} must not contain a fragment");
        }
        Ok(())
    }
}

/// Whether either configured OIDC role needs the shared transport.
#[must_use]
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

/// Discovered interactive client paired with the transport used for code exchange.
///
/// `AppState` holds this behind an `Arc`, sharing the provider metadata and pool.
#[derive(Debug)]
pub struct OidcRuntime {
    client: OidcClient,
    transport: OidcTransport,
}

impl OidcRuntime {
    pub(crate) const fn new(client: OidcClient, transport: OidcTransport) -> Self {
        Self { client, transport }
    }

    /// The discovered client used for authorization and ID-token verification.
    #[must_use]
    pub const fn client(&self) -> &OidcClient {
        &self.client
    }

    pub(crate) const fn transport(&self) -> &OidcTransport {
        &self.transport
    }
}

/// Discover a provider and bind its client to the callback URI and shared transport.
///
/// Issuer and callback syntax are checked before discovery. Authorization and
/// token endpoints are checked before use. The library fetches JWKS during
/// discovery, so the transport enforces HTTPS before that fetch.
///
/// # Errors
///
/// Returns an error for invalid URLs, discovery failure or a rejected endpoint.
pub async fn init_oidc_client(config: &Config, transport: &OidcTransport) -> Result<OidcRuntime> {
    let issuer =
        IssuerUrl::new(config.oidc_issuer_url.clone()).context("invalid OIDC_ISSUER_URL")?;
    transport.check_endpoint(OidcEndpoint::Issuer, "OIDC_ISSUER_URL", issuer.url())?;
    let redirect =
        RedirectUrl::new(config.oidc_redirect_uri.clone()).context("invalid OIDC_REDIRECT_URI")?;
    let metadata = CoreProviderMetadata::discover_async(issuer, &transport.oauth_client())
        .await
        .map_err(|error| transport.discovery_error(error))
        .context("OIDC discovery failed")?;
    transport.check_endpoint(
        OidcEndpoint::Authorization,
        "discovery document authorization_endpoint",
        metadata.authorization_endpoint().url(),
    )?;
    if let Some(endpoint) = metadata.token_endpoint() {
        transport.check_endpoint(
            OidcEndpoint::Token,
            "discovery document token_endpoint",
            endpoint.url(),
        )?;
    }
    transport.check_endpoint(
        OidcEndpoint::Jwks,
        "discovery document jwks_uri",
        metadata.jwks_uri().url(),
    )?;
    let client = CoreClient::from_provider_metadata(
        metadata,
        ClientId::new(config.oidc_client_id.clone()),
        Some(ClientSecret::new(
            config.oidc_client_secret.expose_secret().to_owned(),
        )),
    )
    .set_redirect_uri(redirect);
    Ok(OidcRuntime::new(client, transport.clone()))
}

#[cfg(test)]
mod tests {
    use openidconnect::{AsyncHttpClient, http};
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::test_support::oidc_mock::MockOidcProvider;

    fn production() -> OidcTransport {
        OidcTransport::new().expect("production transport")
    }

    fn config(issuer: &str) -> Config {
        let mut config = crate::test_support::test_config();
        config.oidc_issuer_url = issuer.to_owned();
        config.oidc_client_id = "test".to_owned();
        config.oidc_client_secret = "secret".into();
        config.oidc_redirect_uri = "http://localhost:3000/auth/callback".to_owned();
        config
    }

    #[test]
    fn endpoints_enforce_scheme_and_components() {
        let transport = production();
        for endpoint in [
            OidcEndpoint::Issuer,
            OidcEndpoint::Authorization,
            OidcEndpoint::Token,
            OidcEndpoint::Jwks,
        ] {
            for allowed in [
                "https://auth.example.com/realm",
                "https://10.1.2.3:8443/realm",
            ] {
                transport
                    .check_endpoint(endpoint, "source", &url::Url::parse(allowed).unwrap())
                    .unwrap();
            }
            for rejected in [
                "http://auth.example.com/realm",
                "https://auth.example.com/realm#fragment",
            ] {
                assert!(
                    transport
                        .check_endpoint(endpoint, "source", &url::Url::parse(rejected).unwrap())
                        .is_err(),
                    "{endpoint:?}: {rejected}"
                );
            }
            let query = url::Url::parse("https://auth.example.com/realm?tenant=a").unwrap();
            assert_eq!(
                transport.check_endpoint(endpoint, "source", &query).is_ok(),
                endpoint != OidcEndpoint::Issuer
            );
        }
    }

    #[test]
    fn test_policy_only_relaxes_loopback_scheme() {
        let transport = OidcTransport::for_tests();
        for host in ["127.0.0.1", "[::1]", "localhost"] {
            transport
                .check_endpoint(
                    OidcEndpoint::Issuer,
                    "source",
                    &url::Url::parse(&format!("http://{host}:8080")).unwrap(),
                )
                .unwrap();
        }
        for rejected in [
            "http://remote.example.com",
            "http://127.0.0.1?x=1",
            "http://localhost#frag",
        ] {
            assert!(
                transport
                    .check_endpoint(
                        OidcEndpoint::Issuer,
                        "source",
                        &url::Url::parse(rejected).unwrap()
                    )
                    .is_err()
            );
        }
    }

    #[test]
    fn transport_is_required_only_for_configured_roles() {
        let mut local = crate::test_support::test_config();
        assert!(!transport_required(&local));
        assert!(transport_required(&config("https://auth.example.com")));
        local.resource_server_issuer = "https://auth.example.com".to_owned();
        assert!(transport_required(&local));
        local.oidc_issuer_url = "https://auth.example.com".to_owned();
        assert!(transport_required(&local));
    }

    #[tokio::test]
    async fn adapter_preserves_user_agent_and_refuses_redirects() {
        let target = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200))
            .expect(0)
            .mount(&target)
            .await;
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(header(
                "user-agent",
                concat!("reverie/", env!("CARGO_PKG_VERSION")),
            ))
            .respond_with(
                ResponseTemplate::new(307).insert_header("location", target.uri().as_str()),
            )
            .expect(1)
            .mount(&server)
            .await;
        let request = http::Request::builder()
            .method("POST")
            .uri(server.uri())
            .body(b"code=test".to_vec())
            .unwrap();
        let response = OidcTransport::for_tests()
            .oauth_client()
            .call(request)
            .await
            .unwrap();
        assert_eq!(response.status(), 307);
    }

    #[tokio::test]
    async fn adapter_bounds_hung_requests() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(30)))
            .mount(&server)
            .await;
        let transport = OidcTransport::for_tests_with_timeouts(
            Duration::from_millis(250),
            Duration::from_millis(250),
        );
        let request = http::Request::builder()
            .uri(server.uri())
            .body(Vec::new())
            .unwrap();
        let result = tokio::time::timeout(
            Duration::from_secs(5),
            transport.oauth_client().call(request),
        )
        .await;
        let err = result
            .expect("transport must finish before outer bound")
            .unwrap_err();
        assert!(
            matches!(err, openidconnect::HttpClientError::Reqwest(error) if error.is_timeout())
        );
    }

    #[tokio::test]
    async fn production_adapter_refuses_cleartext_before_sending() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;
        let request = || {
            http::Request::builder()
                .uri(server.uri())
                .body(Vec::new())
                .unwrap()
        };
        assert_eq!(
            OidcTransport::for_tests()
                .oauth_client()
                .call(request())
                .await
                .unwrap()
                .status(),
            200
        );
        assert!(production().oauth_client().call(request()).await.is_err());
        assert!(
            production()
                .raw_client()
                .get(server.uri())
                .send()
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn init_rejects_issuer_and_callback_before_discovery() {
        for issuer in [
            "http://127.0.0.1:1",
            "https://127.0.0.1:1?tenant=a",
            "https://127.0.0.1:1#frag",
        ] {
            let err = init_oidc_client(&config(issuer), &production())
                .await
                .unwrap_err();
            assert!(err.to_string().contains("OIDC_ISSUER_URL"));
            assert!(!err.to_string().contains("OIDC discovery failed"));
        }
        let mut config = config("https://127.0.0.1:1");
        config.oidc_redirect_uri = "not-a-url".to_owned();
        let err = init_oidc_client(&config, &production()).await.unwrap_err();
        assert!(err.to_string().contains("OIDC_REDIRECT_URI"));
    }

    #[tokio::test]
    async fn discovery_builds_a_usable_runtime() {
        let mock = MockOidcProvider::start("test").await;
        mock.mount_discovery().await;
        let runtime = init_oidc_client(&config(mock.issuer()), &OidcTransport::for_tests())
            .await
            .unwrap();
        let (url, _, _) = runtime.client().authorize_url(
            openidconnect::AuthenticationFlow::<openidconnect::core::CoreResponseType>::AuthorizationCode,
            openidconnect::CsrfToken::new_random,
            openidconnect::Nonce::new_random,
        ).url();
        assert!(url.as_str().starts_with(mock.issuer()));
    }

    #[tokio::test]
    async fn discovery_rejects_a_cleartext_token_endpoint() {
        let mock = MockOidcProvider::start("test").await;
        mock.mount_discovery_with_token_endpoint("http://remote.example.com/token")
            .await;
        let err = init_oidc_client(&config(mock.issuer()), &OidcTransport::for_tests())
            .await
            .unwrap_err();
        assert!(
            err.to_string()
                .contains("discovery document token_endpoint")
        );
    }

    #[tokio::test]
    async fn library_owned_jwks_fetch_cannot_downgrade_to_cleartext() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/jwks"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"keys": []})))
            .expect(1)
            .mount(&server)
            .await;
        let issuer = "https://auth.example.com";
        let document = serde_json::to_vec(&serde_json::json!({
            "issuer": issuer,
            "authorization_endpoint": format!("{issuer}/auth"),
            "token_endpoint": format!("{issuer}/token"),
            "jwks_uri": format!("{}/jwks", server.uri()),
            "response_types_supported": ["code"],
            "subject_types_supported": ["public"],
            "id_token_signing_alg_values_supported": ["RS256"],
        }))
        .unwrap();
        for (transport, succeeds) in [(OidcTransport::for_tests(), true), (production(), false)] {
            let fetch = |request: openidconnect::HttpRequest| {
                let adapter = transport.oauth_client();
                let document = document.clone();
                async move {
                    if request.uri().path().contains(".well-known") {
                        Ok(http::Response::builder()
                            .status(200)
                            .header("content-type", "application/json")
                            .body(document)
                            .unwrap())
                    } else {
                        adapter.call(request).await
                    }
                }
            };
            let result = CoreProviderMetadata::discover_async(
                IssuerUrl::new(issuer.to_owned()).unwrap(),
                &fetch,
            )
            .await;
            assert_eq!(result.is_ok(), succeeds);
            if let Err(error) = result {
                let error = transport
                    .discovery_error(error)
                    .context("OIDC discovery failed")
                    .context("failed to initialize OIDC client");
                let diagnostic = format!("{error:#}");
                assert!(diagnostic.contains("discovery document jwks_uri must use https"));
                assert!(diagnostic.contains("builder error"));
                assert!(!diagnostic.contains(&server.uri()));
            }
        }
    }
}
