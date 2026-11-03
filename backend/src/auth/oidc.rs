//! Shared outbound OIDC transport and startup provider discovery.
//!
//! Discovery, signing-key retrieval and token exchange share one bounded
//! HTTPS client. Operator-selected private HTTPS providers remain supported.
//! TLS uses the platform trust store; there is no certificate override.
//!
//! THREAT: the operator-selected issuer controls trusted signing keys.
//! THREAT: disabled redirects prevent an endpoint from forwarding credentials or key resolution.
//! THREAT: timeouts bound stalled provider requests, including signing-key retrieval.
//! THREAT: detached fetch ownership prevents request cancellation from stranding a shared refresh.
//! THREAT: fixed discovery outcomes exclude provider-controlled errors and endpoint values from events.

use secrecy::ExposeSecret;
use std::sync::Arc;
use std::time::Duration;

use futures::FutureExt;
use tokio::sync::{Mutex, RwLock, watch};
use tokio::time::Instant;
use tracing::instrument::WithSubscriber;

use anyhow::{Context, Result, bail};
use openidconnect::core::{CoreClient, CoreProviderMetadata};
use openidconnect::{
    AsyncHttpClient, ClientId, ClientSecret, EndpointMaybeSet, EndpointNotSet, EndpointSet,
    IssuerUrl, RedirectUrl,
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

/// One connection pool for interactive OIDC sign-in.
///
/// Clones share the underlying pool and transport policy.
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

    #[cfg(test)]
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

/// Whether interactive OIDC sign-in needs the transport.
#[must_use]
pub fn transport_required(config: &Config) -> bool {
    config.oidc_configured()
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

#[derive(Debug, thiserror::Error)]
enum DiscoveryRequestError {
    #[error("OIDC key fetch is cooling down")]
    Cooldown,
    #[error(transparent)]
    Http(#[from] openidconnect::HttpClientError<reqwest::Error>),
}

#[derive(Clone, Copy, Debug)]
enum DiscoveryFailure {
    Fetch(OidcRefreshError),
    Endpoint(&'static str),
    Cooldown,
}

impl DiscoveryFailure {
    const fn outcome(self) -> &'static str {
        match self {
            Self::Fetch(error) => error.outcome(),
            Self::Endpoint(_) => "endpoint_rejected",
            Self::Cooldown => "cooldown",
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[error("{error}")]
struct OidcDiscoveryError {
    failure: DiscoveryFailure,
    #[source]
    error: anyhow::Error,
}

#[derive(Clone, Debug)]
pub(crate) struct OidcConfiguration {
    issuer: IssuerUrl,
    redirect: RedirectUrl,
    client_id: ClientId,
    client_secret: ClientSecret,
}

impl OidcConfiguration {
    pub(crate) fn parse(config: &Config, transport: &OidcTransport) -> Result<Self> {
        let issuer =
            IssuerUrl::new(config.oidc_issuer_url.clone()).context("invalid OIDC_ISSUER_URL")?;
        transport.check_endpoint(OidcEndpoint::Issuer, "OIDC_ISSUER_URL", issuer.url())?;
        let redirect = RedirectUrl::new(config.oidc_redirect_uri.clone())
            .context("invalid OIDC_REDIRECT_URI")?;
        Ok(Self {
            issuer,
            redirect,
            client_id: ClientId::new(config.oidc_client_id.clone()),
            client_secret: ClientSecret::new(config.oidc_client_secret.expose_secret().to_owned()),
        })
    }

    pub(crate) fn client(&self, metadata: CoreProviderMetadata) -> OidcClient {
        CoreClient::from_provider_metadata(
            metadata,
            self.client_id.clone(),
            Some(self.client_secret.clone()),
        )
        .set_redirect_uri(self.redirect.clone())
    }

    async fn discover(
        &self,
        transport: &OidcTransport,
        coordinator: Option<Arc<Mutex<RefreshCoordinator>>>,
    ) -> Result<CoreProviderMetadata, OidcDiscoveryError> {
        let discovery_client = DiscoveryHttpClient {
            http: transport.oauth_client(),
            coordinator,
            first_request: std::sync::atomic::AtomicBool::new(true),
        };
        let metadata = CoreProviderMetadata::discover_async(self.issuer.clone(), &discovery_client)
            .await
            .map_err(|error| {
                let failure = match &error {
                    openidconnect::DiscoveryError::Request(DiscoveryRequestError::Cooldown) => {
                        DiscoveryFailure::Cooldown
                    }
                    openidconnect::DiscoveryError::Request(DiscoveryRequestError::Http(
                        openidconnect::HttpClientError::Reqwest(error),
                    )) if error.url().is_some_and(|url| {
                        transport
                            .check_endpoint(OidcEndpoint::Jwks, "discovery document jwks_uri", url)
                            .is_err()
                    }) =>
                    {
                        DiscoveryFailure::Endpoint("jwks_uri")
                    }
                    openidconnect::DiscoveryError::Request(_) => {
                        DiscoveryFailure::Fetch(OidcRefreshError::Request)
                    }
                    openidconnect::DiscoveryError::Parse(_) => {
                        DiscoveryFailure::Fetch(OidcRefreshError::Parse)
                    }
                    _ => DiscoveryFailure::Fetch(OidcRefreshError::Response),
                };
                let error = match error {
                    openidconnect::DiscoveryError::Request(DiscoveryRequestError::Http(error)) => {
                        transport.discovery_error(openidconnect::DiscoveryError::Request(error))
                    }
                    error => anyhow::Error::new(error),
                };
                OidcDiscoveryError {
                    failure,
                    error: error.context("OIDC discovery failed"),
                }
            })?;
        transport
            .check_endpoint(
                OidcEndpoint::Authorization,
                "discovery document authorization_endpoint",
                metadata.authorization_endpoint().url(),
            )
            .map_err(|error| OidcDiscoveryError {
                failure: DiscoveryFailure::Endpoint("authorization_endpoint"),
                error,
            })?;
        if let Some(endpoint) = metadata.token_endpoint() {
            transport
                .check_endpoint(
                    OidcEndpoint::Token,
                    "discovery document token_endpoint",
                    endpoint.url(),
                )
                .map_err(|error| OidcDiscoveryError {
                    failure: DiscoveryFailure::Endpoint("token_endpoint"),
                    error,
                })?;
        }
        transport
            .check_endpoint(
                OidcEndpoint::Jwks,
                "discovery document jwks_uri",
                metadata.jwks_uri().url(),
            )
            .map_err(|error| OidcDiscoveryError {
                failure: DiscoveryFailure::Endpoint("jwks_uri"),
                error,
            })?;
        Ok(metadata)
    }
}

struct DiscoveryHttpClient {
    http: oauth2_reqwest::ReqwestClient,
    coordinator: Option<Arc<Mutex<RefreshCoordinator>>>,
    first_request: std::sync::atomic::AtomicBool,
}

impl<'c> AsyncHttpClient<'c> for DiscoveryHttpClient {
    type Error = DiscoveryRequestError;
    type Future = std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<openidconnect::HttpResponse, Self::Error>>
                + Send
                + 'c,
        >,
    >;

    fn call(&'c self, request: openidconnect::HttpRequest) -> Self::Future {
        // THREAT: discovery fetches trusted signing keys in its second request before returning metadata.
        let is_jwks = !self
            .first_request
            .swap(false, std::sync::atomic::Ordering::Relaxed);
        Box::pin(async move {
            if is_jwks && let Some(coordinator) = &self.coordinator {
                let mut state = coordinator.lock().await;
                if state
                    .last_attempt
                    .is_some_and(|start| start.elapsed() < KEY_REFRESH_FLOOR)
                {
                    return Err(DiscoveryRequestError::Cooldown);
                }
                state.last_attempt = Some(Instant::now());
            }
            self.http
                .call(request)
                .await
                .map_err(DiscoveryRequestError::Http)
        })
    }
}

const KEY_REFRESH_FLOOR: Duration = Duration::from_secs(30);
const KEY_MAX_AGE: Duration = Duration::from_secs(600);

#[derive(Debug)]
pub(crate) struct OidcSnapshot {
    pub(crate) client: OidcClient,
    metadata: CoreProviderMetadata,
    epoch: u64,
    fetched_at: Instant,
}

#[derive(Clone, Copy, Debug, thiserror::Error)]
pub(crate) enum OidcRefreshError {
    #[error("OIDC discovery is pending")]
    Unavailable,
    #[error("OIDC key request failed")]
    Request,
    #[error("OIDC key response was rejected")]
    Response,
    #[error("OIDC key response could not be parsed")]
    Parse,
    #[error("OIDC key fetch task failed")]
    Task,
}

impl OidcRefreshError {
    const fn outcome(self) -> &'static str {
        match self {
            Self::Unavailable => "unavailable",
            Self::Request => "request_failed",
            Self::Response => "response_rejected",
            Self::Parse => "parse_failed",
            Self::Task => "task_failed",
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum OidcVerificationError {
    #[error(transparent)]
    Refresh(#[from] OidcRefreshError),
    #[error("OIDC ID-token validation failed")]
    Claims(#[from] openidconnect::ClaimsVerificationError),
}

type RefreshCompletion = watch::Receiver<Option<Result<(), OidcRefreshError>>>;

#[derive(Debug, Default)]
struct RefreshCoordinator {
    last_attempt: Option<Instant>,
    flight: Option<RefreshCompletion>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum RefreshCheck {
    NotRequired,
    Requested,
}

/// Optional discovered provider snapshot and its shared bounded transport.
///
/// Clones of the application state share discovery, published keys and one refresh flight.
#[derive(Debug)]
pub struct OidcRuntime {
    configuration: OidcConfiguration,
    snapshot: RwLock<Option<Arc<OidcSnapshot>>>,
    refresh: Arc<Mutex<RefreshCoordinator>>,
    transport: OidcTransport,
    stopping: tokio_util::sync::CancellationToken,
    #[cfg(test)]
    fail_fetch: std::sync::atomic::AtomicBool,
}

impl OidcRuntime {
    #[cfg(test)]
    pub(crate) fn new(
        configuration: OidcConfiguration,
        metadata: CoreProviderMetadata,
        transport: OidcTransport,
    ) -> Self {
        let client = configuration.client(metadata.clone());
        let now = Instant::now();
        Self {
            configuration,
            snapshot: RwLock::new(Some(Arc::new(OidcSnapshot {
                client,
                metadata,
                epoch: 1,
                fetched_at: now,
            }))),
            refresh: Arc::new(Mutex::new(RefreshCoordinator {
                last_attempt: Some(now),
                flight: None,
            })),
            transport,
            stopping: tokio_util::sync::CancellationToken::new(),
            #[cfg(test)]
            fail_fetch: std::sync::atomic::AtomicBool::new(false),
        }
    }

    pub(crate) fn configured(config: &Config, transport: OidcTransport) -> Result<Self> {
        Ok(Self {
            configuration: OidcConfiguration::parse(config, &transport)?,
            snapshot: RwLock::new(None),
            refresh: Arc::new(Mutex::new(RefreshCoordinator::default())),
            transport,
            stopping: tokio_util::sync::CancellationToken::new(),
            #[cfg(test)]
            fail_fetch: std::sync::atomic::AtomicBool::new(false),
        })
    }

    pub(crate) async fn run_discovery(
        self: Arc<Self>,
        cancel: tokio_util::sync::CancellationToken,
    ) -> Result<()> {
        let mut delay = Duration::from_secs(1);
        while self.snapshot().await.is_none() && !cancel.is_cancelled() {
            let runtime = Arc::clone(&self);
            let mut fetch = tokio::spawn(
                async move { runtime.discover_once().await }.with_current_subscriber(),
            );
            let result = tokio::select! {
                result = &mut fetch => result,
                () = cancel.cancelled() => {
                    self.stopping.cancel();
                    (&mut fetch).await
                }
            }
            .context("OIDC discovery task failed")?;
            match result {
                Ok(()) => break,
                Err(error) => {
                    let outcome = error.failure.outcome();
                    match error.failure {
                        DiscoveryFailure::Cooldown => {
                            tracing::warn!(epoch = 0, keys = 0, outcome, "OIDC discovery failed");
                        }
                        DiscoveryFailure::Endpoint(source) => {
                            tracing::error!(
                                epoch = 0,
                                keys = 0,
                                outcome,
                                source,
                                "OIDC discovery failed"
                            );
                        }
                        DiscoveryFailure::Fetch(_) => {
                            tracing::error!(epoch = 0, keys = 0, outcome, "OIDC discovery failed");
                        }
                    }
                }
            }
            tokio::select! {
                () = cancel.cancelled() => break,
                () = tokio::time::sleep(delay) => {}
            }
            delay = (delay * 2).min(Duration::from_secs(60));
        }
        cancel.cancelled().await;
        self.stopping.cancel();
        let flight = self.refresh.lock().await.flight.clone();
        if let Some(mut flight) = flight {
            loop {
                if flight.borrow_and_update().is_some() {
                    break;
                }
                flight
                    .changed()
                    .await
                    .context("OIDC refresh completion lost during shutdown")?;
            }
        }
        Ok(())
    }

    async fn discover_once(&self) -> Result<(), OidcDiscoveryError> {
        let metadata = self
            .configuration
            .discover(&self.transport, Some(Arc::clone(&self.refresh)))
            .await?;
        let keys = metadata.jwks().keys().len();
        self.publish(metadata).await;
        tracing::info!(
            epoch = 1,
            keys,
            outcome = "success",
            "OIDC discovery completed"
        );
        Ok(())
    }

    pub(crate) async fn snapshot(&self) -> Option<Arc<OidcSnapshot>> {
        self.snapshot.read().await.clone()
    }

    pub(crate) const fn transport(&self) -> &OidcTransport {
        &self.transport
    }

    pub(crate) async fn verification_snapshot(
        self: &Arc<Self>,
    ) -> Result<(Arc<OidcSnapshot>, RefreshCheck), OidcRefreshError> {
        let snapshot = self.snapshot().await.ok_or(OidcRefreshError::Unavailable)?;
        if snapshot.fetched_at.elapsed() > KEY_MAX_AGE {
            Ok((self.refresh_keys().await?, RefreshCheck::Requested))
        } else {
            Ok((snapshot, RefreshCheck::NotRequired))
        }
    }

    pub(crate) async fn verify<'a>(
        self: &Arc<Self>,
        token: &'a openidconnect::core::CoreIdToken,
        nonce: &openidconnect::Nonce,
    ) -> Result<&'a openidconnect::core::CoreIdTokenClaims, OidcVerificationError> {
        let (snapshot, refresh) = self.verification_snapshot().await?;
        match token.claims(&snapshot.client.id_token_verifier(), nonce) {
            Err(openidconnect::ClaimsVerificationError::SignatureVerification(
                openidconnect::SignatureVerificationError::NoMatchingKey,
            )) if refresh == RefreshCheck::NotRequired => {
                let snapshot = self.refresh_keys().await?;
                Ok(token.claims(&snapshot.client.id_token_verifier(), nonce)?)
            }
            result => Ok(result?),
        }
    }

    async fn refresh_keys(self: &Arc<Self>) -> Result<Arc<OidcSnapshot>, OidcRefreshError> {
        let snapshot = self.snapshot().await.ok_or(OidcRefreshError::Unavailable)?;
        let mut coordinator = self.refresh.lock().await;
        if self.stopping.is_cancelled() {
            return Err(OidcRefreshError::Unavailable);
        }
        let mut completion = if let Some(flight) = &coordinator.flight {
            flight.clone()
        } else {
            if coordinator
                .last_attempt
                .is_some_and(|start| start.elapsed() < KEY_REFRESH_FLOOR)
            {
                drop(coordinator);
                return self.snapshot().await.ok_or(OidcRefreshError::Unavailable);
            }
            let (sender, receiver) = watch::channel(None);
            coordinator.flight = Some(receiver.clone());
            let runtime = Arc::clone(self);
            let _fetch = tokio::spawn(
                async move {
                    runtime.run_refresh(snapshot, sender).await;
                }
                .with_current_subscriber(),
            );
            receiver
        };
        drop(coordinator);
        loop {
            let result = *completion.borrow_and_update();
            if let Some(result) = result {
                result?;
                return self.snapshot().await.ok_or(OidcRefreshError::Unavailable);
            }
            if completion.changed().await.is_err() {
                let mut coordinator = self.refresh.lock().await;
                if coordinator
                    .flight
                    .as_ref()
                    .is_some_and(|flight| flight.same_channel(&completion))
                {
                    coordinator.flight = None;
                    drop(coordinator);
                    tracing::warn!(
                        outcome = "task_failed",
                        "OIDC key fetch task lost completion"
                    );
                }
                return Err(OidcRefreshError::Task);
            }
        }
    }

    async fn run_refresh(
        self: Arc<Self>,
        snapshot: Arc<OidcSnapshot>,
        completion: watch::Sender<Option<Result<(), OidcRefreshError>>>,
    ) {
        self.refresh.lock().await.last_attempt = Some(Instant::now());
        let result = std::panic::AssertUnwindSafe(self.fetch_keys(&snapshot))
            .catch_unwind()
            .await
            .unwrap_or(Err(OidcRefreshError::Task));
        let result = match result {
            Ok(metadata) => {
                self.publish(metadata).await;
                Ok(())
            }
            Err(error) => Err(error),
        };
        let current = self.snapshot().await;
        let epoch = current.as_ref().map_or(0, |value| value.epoch);
        let keys = current
            .as_ref()
            .map_or(0, |value| value.metadata.jwks().keys().len());
        match result {
            Ok(()) => tracing::info!(epoch, keys, outcome = "success", "OIDC key fetch completed"),
            Err(error) => tracing::warn!(
                epoch,
                keys,
                outcome = error.outcome(),
                "OIDC key fetch completed"
            ),
        }
        self.refresh.lock().await.flight = None;
        completion.send_replace(Some(result));
    }

    async fn fetch_keys(
        &self,
        snapshot: &OidcSnapshot,
    ) -> Result<CoreProviderMetadata, OidcRefreshError> {
        #[cfg(test)]
        if self
            .fail_fetch
            .swap(false, std::sync::atomic::Ordering::SeqCst)
        {
            panic!("test fetch failure");
        }
        let keys = openidconnect::core::CoreJsonWebKeySet::fetch_async(
            snapshot.metadata.jwks_uri(),
            &self.transport.oauth_client(),
        )
        .await
        .map_err(|error| match error {
            openidconnect::DiscoveryError::Request(_) => OidcRefreshError::Request,
            openidconnect::DiscoveryError::Parse(_) => OidcRefreshError::Parse,
            _ => OidcRefreshError::Response,
        })?;
        Ok(snapshot.metadata.clone().set_jwks(keys))
    }

    async fn publish(&self, metadata: CoreProviderMetadata) {
        let client = self.configuration.client(metadata.clone());
        let mut current = self.snapshot.write().await;
        let epoch = current.as_ref().map_or(1, |snapshot| snapshot.epoch + 1);
        *current = Some(Arc::new(OidcSnapshot {
            client,
            metadata,
            epoch,
            fetched_at: Instant::now(),
        }));
    }

    #[cfg(test)]
    pub(crate) async fn set_keys_for_tests(&self, keys: openidconnect::core::CoreJsonWebKeySet) {
        let snapshot = self.snapshot().await.unwrap();
        self.publish(snapshot.metadata.clone().set_jwks(keys)).await;
    }

    #[cfg(test)]
    pub(crate) async fn clear_snapshot_for_tests(&self) {
        *self.snapshot.write().await = None;
    }

    #[cfg(test)]
    pub(crate) async fn age_for_tests(&self, age: Duration) {
        let mut snapshot = self.snapshot.write().await;
        if let Some(current) = snapshot.take() {
            *snapshot = Some(Arc::new(OidcSnapshot {
                client: current.client.clone(),
                metadata: current.metadata.clone(),
                epoch: current.epoch,
                fetched_at: Instant::now() - age,
            }));
        }
        drop(snapshot);
        self.refresh.lock().await.last_attempt = Some(Instant::now() - age);
    }
}

#[cfg(test)]
mod tests {
    use openidconnect::{AsyncHttpClient, http};
    use wiremock::matchers::{header, method};
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
    fn oidc_key_config_diagnostics_omit_credentials() {
        let mut invalid = config("not-a-url");
        invalid.oidc_client_secret = "credential-marker".into();
        let error = OidcConfiguration::parse(&invalid, &production()).unwrap_err();
        assert!(error.to_string().contains("OIDC_ISSUER_URL"));
        assert!(!format!("{error:?}").contains("credential-marker"));
        invalid.oidc_issuer_url = "https://example.com".into();
        assert!(
            !format!(
                "{:?}",
                OidcConfiguration::parse(&invalid, &production()).unwrap()
            )
            .contains("credential-marker")
        );
    }

    fn mock_runtime(mock: &MockOidcProvider) -> Arc<OidcRuntime> {
        Arc::new(mock.runtime("http://localhost:3000/auth/callback"))
    }

    #[tokio::test]
    async fn oidc_key_refresh_logs_once_without_material() {
        let logs = crate::test_support::CapturedLogs::default();
        let _subscriber = tracing::subscriber::set_default(logs.subscriber());
        let mut mock = MockOidcProvider::start("test").await;
        let runtime = mock_runtime(&mock);
        runtime.age_for_tests(KEY_REFRESH_FLOOR).await;
        runtime.refresh_keys().await.unwrap();
        runtime.age_for_tests(KEY_REFRESH_FLOOR).await;
        mock.set_jwks_response(
            ResponseTemplate::new(500).set_body_string("provider-body-marker"),
            1,
        )
        .await;
        assert!(runtime.refresh_keys().await.is_err());
        let text = logs.text();
        assert_eq!(text.matches("OIDC key fetch completed").count(), 2);
        assert_eq!(text.matches("epoch=2 keys=1").count(), 2);
        assert!(text.contains(r#"outcome="success""#));
        assert!(text.contains("response_rejected"));
        for marker in [
            "provider-body-marker",
            "test-secret",
            "test-access-token",
            "BEGIN RSA",
            "test-kid",
        ] {
            assert!(!text.contains(marker));
        }
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
    async fn discovery_rejects_a_cleartext_token_endpoint() {
        let mock = MockOidcProvider::start("test").await;
        mock.mount_discovery_with_token_endpoint("http://remote.example.com/token")
            .await;
        let transport = OidcTransport::for_tests();
        let configuration = OidcConfiguration::parse(&config(mock.issuer()), &transport).unwrap();
        let err = configuration.discover(&transport, None).await.unwrap_err();
        assert!(
            err.to_string()
                .contains("discovery document token_endpoint")
        );
    }
}
