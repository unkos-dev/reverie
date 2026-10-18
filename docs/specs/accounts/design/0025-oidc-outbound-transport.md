---
type: DESIGN
profile-version: 1
id: "REV-DESIGN-0025"
title: "OIDC outbound transport"
governed-by:
  - "REV-ADR-0007"
---

# OIDC outbound transport

The interactive OIDC client uses one HTTP connection pool for provider discovery, callback token exchange and
signing-key retrieval. This Design describes the transport policy and its interfaces.

## Purpose and boundaries

This subject owns `OidcTransport`, its endpoint policy, and `OidcRuntime` configuration, discovery, snapshots and
signing-key refresh coordination. It depends on `Config` for configured issuer and endpoint URLs, `reqwest` for HTTP and
TLS, `oauth2-reqwest` for the OAuth HTTP interface, and `openidconnect` for discovery. The interactive callback consumes
its client.

[Application runtime](../../operations/design/0024-application-runtime-startup-workers-and-shutdown.md) owns
construction order and whether OIDC sign-in is enabled. This subject does not own login state, PKCE, ID-token validation
or account provisioning. It does not own outbound enrichment clients.

## Structure

- `OidcTransport` holds a `reqwest::Client` and an endpoint scheme policy in `backend/src/auth/oidc.rs`.
- `OidcConfiguration` holds the parsed issuer and callback URI, client id and secret.
- `OidcRuntime` retains that configuration, the transport, an optional published snapshot, refresh coordination and a
  shutdown token; `AppState.oidc` shares it behind an `Arc`.
- `OidcSnapshot` holds provider metadata, its rebuilt client, a key-set epoch and the successful fetch instant.
- `DiscoveryHttpClient` wraps the existing OAuth adapter to apply the key-fetch floor to library-owned discovery JWKS
  requests. Discovery's first request retrieves metadata; its second retrieves signing keys.

Client clones share the connection pool. Constructing an OAuth adapter wraps a clone of the same client, so discovery,
code exchange, discovery-time JWKS fetching and subsequent key refreshes use the same client configuration.

## Interfaces and dependencies

[`OidcTransport`](../../../../backend/src/auth/oidc.rs) offers `oauth_client()` to `openidconnect`. Its test-only
`raw_client()` method supports transport tests. `check_endpoint()` takes an endpoint kind, its configuration or
discovery source, and a parsed URL; failures identify that source.

| Endpoint kind              | Scheme | Query     | Fragment |
| -------------------------- | ------ | --------- | -------- |
| Issuer                     | HTTPS  | Rejected  | Rejected |
| Authorization, token, JWKS | HTTPS  | Permitted | Rejected |

The authorization URL is a browser destination, not an HTTP request made by this transport. Its discovery validation
applies the endpoint policy before login can use it. The application callback URI is parsed separately and is not
subject to the provider endpoint policy.

The upstream OAuth adapter is exact-pinned. Its dependency constraint and lift condition are recorded in the
[adapter debt entry](../../../../debt/2026-08-14-oauth2-reqwest-prerelease-pin.md).

## Data and state

The runtime retains the pool and parsed configuration. The snapshot starts absent; discovery publishes the first
complete snapshot. An `RwLock<Option<Arc<OidcSnapshot>>>` protects publication, and readers clone the `Arc` before
releasing the lock. Provider I/O and waits for shared work never hold the snapshot lock.

The epoch starts at one and increments on every successful publication, including unchanged keys. Successful fetch time
uses `tokio::time::Instant`. A failed fetch changes neither the snapshot, its epoch nor that instant. The coordinator
holds the last key-fetch attempt's start instant and an optional watch receiver for the current shared completion; it
holds no attempt-generation counter or completed failure cache.

| State | Writers |
| ----- | ------- |
| Snapshot, epoch and successful fetch instant | Initial discovery and detached refresh, through one publication method |
| Last attempt start | Discovery JWKS adapter and detached refresh owner |
| Current refresh flight | Admission elects it; detached completion releases it |
| Shutdown token | Discovery worker on cancellation |

Initial discovery and demand refresh cannot publish concurrently: demand refresh requires a snapshot, and discovery
stops after publishing the first one. The shutdown token closes admission before the worker drains an active refresh.

Each request uses a 5-second connect timeout, a 10-second total timeout through response-body completion, no redirects,
and `reverie/<package-version>` as its User-Agent. Production clients enforce HTTPS at the HTTP layer. Test-only
constructors permit loopback HTTP endpoint validation and adjustable timeouts; these constructors are absent from
production builds.

## Runtime behaviour

Synchronous construction parses and checks `OIDC_ISSUER_URL`, then parses `OIDC_REDIRECT_URI`, without provider I/O. The
supervised discovery worker calls `CoreProviderMetadata::discover_async`; the library retrieves metadata and JWKS
through `DiscoveryHttpClient`, which delegates HTTP to the shared OAuth adapter. Before the JWKS request, the wrapper
checks the floor and records its start. The underlying client enforces HTTPS, timeouts and redirect refusal during both
requests. After discovery, authorization, token and JWKS endpoint checks run before publication.

A callback rechecks a set older than ten minutes before verification. An eligible unknown-key error can also request a
refresh. Every key-fetch attempt, successful or failed, starts a 30-second floor when it starts. Callers join an active
refresh before testing that floor. A suppressed caller uses the current snapshot, including after an earlier failure; it
does not reuse a failed result. These timing rules are application policy.

An elected refresh runs in its own Tokio task. It records the attempt start, calls `CoreJsonWebKeySet::fetch_async` with
the retained `jwks_uri` and shared adapter, then rebuilds the client from retained metadata with `set_jwks` and
`CoreClient::from_provider_metadata`. It restores client credentials and redirect URI and preserves the advertised
signing algorithms. It does not rediscover endpoints or merge old keys into a replacement set. The library owns key
parsing and selection, including unspecified algorithms and ambiguous matches.

Dropping any or all waiting requests leaves the fetch running. Its owner publishes success, logs the result and settles
current waiters; completion releases the shared flight. A panic becomes a typed task failure, and a lost completion
releases ownership for a later attempt. Request handlers do not write attempt timestamps or publish snapshots.

The [callback](../../../../backend/src/routes/auth.rs) obtains the adapter from `AppState.oidc` for code exchange. It
retains the same transport used at discovery; the callback's PKCE and ID-token verification remain separate operations.

## Failure and recovery

Client construction and invalid configured URLs prevent startup. Provider discovery, discovered endpoint rejection and
the initial JWKS fetch do not stop local service: the discovery worker retries with capped exponential waits, as
specified by the application runtime Design. The key-fetch floor can suppress a JWKS request during a discovery retry;
that attempt leaves the snapshot absent.

A failed refresh retains the last good snapshot and returns an error to its current waiters. A later caller suppressed
by the floor verifies against that snapshot; a later eligible attempt can fetch again. No failed result is cached for
subsequent callers. A successful empty or partially understood set follows the library's normal parsing and verification
rules. The ten-minute age is a recheck threshold, not deletion of retained keys.

Redirect responses are returned to the library without following their destination. Timeouts terminate stalled HTTP
requests. Callback exchange and verification failures become generic internal responses. An authorization code is never
exchanged twice as part of key recovery.

## Security and operations

The configured issuer is an operator trust input. Scheme enforcement remains active during the library-owned JWKS
request, before initialisation can inspect endpoint components in the returned metadata.

Private HTTPS providers are supported. TLS verification uses the platform trust store through
`rustls-platform-verifier`; a private CA must be installed in the host or container trust store. There is no certificate
validation override. Provider endpoints must be final URLs because redirects are disabled.

Each completed refresh emits one structured event with `epoch`, parsed key count `keys`, and a fixed `outcome`. Failure
events describe the retained set; discovery failures use epoch and count zero until a snapshot exists. Provider bodies,
raw error chains, keys, tokens, codes, state, nonce, PKCE and client secrets are not logged by these operations.
Discovery failures are error-level events; refresh failures are warnings.

OIDC provider responses have no response-size cap. The ten-second timeout bounds HTTP duration through response-body
completion, not response bytes, parser allocations or memory use. This accepted limit and its compensating controls are
recorded in the [CodeGuard deviation register](../../../security/codeguard/README.md).
