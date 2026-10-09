---
type: DESIGN
profile-version: 1
id: "REV-DESIGN-0025"
title: "OIDC outbound transport"
governed-by:
  - "REV-ADR-0007"
---

# OIDC outbound transport

The interactive OIDC client and resource-server JWT validator share one HTTP connection pool for provider discovery,
callback token exchange and signing-key retrieval. This Design describes the transport policy and the interfaces that
carry it across both authentication paths.

## Purpose and boundaries

This subject owns `OidcTransport`, its endpoint policy, and the transport references retained by `OidcRuntime` and
`ReverieJwksSource`. It depends on `Config` for configured issuer and endpoint URLs, `reqwest` for HTTP and TLS,
`oauth2-reqwest` for the OAuth HTTP interface, and `openidconnect` for discovery. The interactive callback and
resource-server key source consume its clients.

[Application runtime](../../operations/design/0024-application-runtime-startup-workers-and-shutdown.md) owns
construction order and whether either authentication role is enabled. This subject does not own login state, PKCE,
ID-token validation, account provisioning, JWT claim validation, or the signing-key cache. It does not own outbound
enrichment clients.

## Structure

- `OidcTransport` holds a `reqwest::Client` and an endpoint scheme policy in `backend/src/auth/oidc.rs`.
- `OidcRuntime` pairs the discovered interactive client with a clone of its transport; `AppState.oidc` holds the runtime
  behind an `Arc`.
- `ReverieJwksSource` in `backend/src/auth/jwt.rs` holds a raw client clone, the resolved signing-key URL, and the
  outcome of its most recent fetch.

Client clones share the connection pool. Constructing an OAuth adapter wraps a clone of the same client, so discovery,
code exchange and direct JWKS fetches use the same client configuration.

## Interfaces and dependencies

[`OidcTransport`](../../../../backend/src/auth/oidc.rs) offers `oauth_client()` to `openidconnect` and `raw_client()` to
[`ReverieJwksSource`](../../../../backend/src/auth/jwt.rs). `check_endpoint()` takes an endpoint kind, its configuration
or discovery source, and a parsed URL; failures identify that source.

| Endpoint kind              | Scheme | Query     | Fragment |
| -------------------------- | ------ | --------- | -------- |
| Issuer                     | HTTPS  | Rejected  | Rejected |
| Authorization, token, JWKS | HTTPS  | Permitted | Rejected |

The authorization URL is a browser destination, not an HTTP request made by this transport. Its startup validation
applies the endpoint policy before login can use it. The application callback URI is parsed separately and is not
subject to the provider endpoint policy.

The upstream OAuth adapter is exact-pinned. Its dependency constraint and lift condition are recorded in the
[adapter debt entry](../../../../debt/2026-08-14-oauth2-reqwest-prerelease-pin.md).

## Data and state

The pool and interactive provider metadata live for the lifetime of the configured runtime. Discovery populates the
interactive metadata once at startup; the transport does not refresh it. Resource-server key caching is owned by
`JwksClient`, outside this subject. `ReverieJwksSource` keeps the time and result of its latest fetch, success or
failure, behind an async lock.

Each request uses a 5-second connect timeout, a 10-second total timeout through response-body completion, no redirects,
and `reverie/<package-version>` as its User-Agent. Production clients enforce HTTPS at the HTTP layer. Test-only
constructors permit loopback HTTP endpoint validation and adjustable timeouts; these constructors are absent from
production builds.

## Runtime behaviour

Interactive initialisation parses and checks `OIDC_ISSUER_URL`, then parses `OIDC_REDIRECT_URI` before discovery.
`CoreProviderMetadata::discover_async` uses the OAuth adapter for both the discovery document and the JWKS fetch it
performs internally. The client enforces HTTPS, timeouts and redirect refusal during both fetches. After discovery
returns, initialisation checks the authorization endpoint, any supplied token endpoint, and the JWKS URI, then binds the
interactive client to the callback URI and transport.

The [callback](../../../../backend/src/routes/auth.rs) obtains the adapter from `AppState.oidc` for code exchange. It
retains the same transport used at discovery; the callback's PKCE and ID-token verification remain separate operations.

Resource-server initialisation checks `REVERIE_RESOURCE_SERVER_ISSUER` even when an explicit
`REVERIE_RESOURCE_SERVER_JWKS_URL` is supplied. A non-blank override supplies the key URL directly; otherwise discovery
resolves it through the OAuth adapter, including the library-owned JWKS fetch. The resolved URL is checked before
`ReverieJwksSource` retains it. Later key fetches use the raw client clone.

`JwksClient` asks its source for keys whenever a token names an unknown `kid`. `ReverieJwksSource` holds its lock across
the request, so concurrent misses share one fetch. A call arriving less than 30 seconds after the previous fetch
completed makes no request: it returns the key set from that fetch, or an error when that fetch failed. The first fetch
is never delayed.

## Failure and recovery

Client construction, URL parsing, endpoint rejection and discovery failures propagate to startup and prevent the server
from accepting requests. Endpoint errors name the configuration field or discovered metadata field. A configured role
without a transport also fails startup, as described by the application runtime Design.

Redirect responses are returned to the calling library without following their destination. Timeouts terminate stalled
requests. Callback exchange failures become `AppError::Internal`; direct JWKS request, status and JSON failures become
provider errors consumed by the validator. This transport supplies no application retry loop; recovery and cached-key
fallback belong to its consumers. A failed fetch, or one abandoned because its caller was dropped, starts the 30-second
window like a successful one, so an unreachable provider is retried at most once per window while `JwksClient` keeps
serving the keys it already holds. The error returned inside the window leaves those cached keys and their expiry
untouched.

## Security and operations

Configured issuers and explicit key URLs are operator trust inputs. Token headers cannot select a signing-key URL;
`ReverieJwksSource` only fetches its retained endpoint, so credentials with forged `kid` values cost the provider at
most one request per window. A key the provider rotates in is rejected until the window has passed since the last fetch.
Scheme enforcement remains active during the library-owned JWKS request, before initialisation can inspect endpoint
components in the returned metadata.

Private HTTPS providers are supported. TLS verification uses the platform trust store through
`rustls-platform-verifier`; a private CA must be installed in the host or container trust store. There is no certificate
validation override. Provider endpoints must be final URLs because redirects are disabled.
