---
type: DESIGN
profile-version: 1
id: "REV-DESIGN-0008"
title: "CSRF protection"
satisfies:
  - "REV-REQ-0021"
  - "REV-REQ-0022"
  - "REV-REQ-0023"
  - "REV-REQ-0024"
governed-by:
  - "REV-ADR-0011"
---

# CSRF protection

This Design covers Reverie's OWASP synchronizer-token defence against cross-site request forgery: the token minted into
the session at sign-in, the middleware that checks it on session-authenticated mutating requests, the exemption that
keys on credential type rather than token presence, and the client's cache, header injection and single
refresh-and-retry.

## Purpose and boundaries

This subject owns: the mint statements that generate the synchronizer token and write it into the session, which live
inline inside two handlers otherwise owned by neighbouring subjects (the OIDC interactive login subject,
`backend/src/routes/auth.rs::callback`, and the Local password sign-in subject,
`backend/src/routes/auth.rs::local_login`); the validating middleware itself, `csrf_required` in
`backend/src/security/csrf.rs`, and the two problem-type constants it raises
(`backend/src/error/problems.rs::CSRF_MISSING`, `CSRF_MISMATCH`); the point in `build_router_with_session_store`
(`backend/src/lib.rs`) where that middleware is layered onto the router; and, on the client, the token cache and its
hydration triggers (`frontend/src/api/csrf.ts`), and the header-injection and mismatch/missing retry logic inside
`frontend/src/api/fetch.ts`'s `apiFetch` and `sendRequest`.

It does not own the session itself: the Postgres-backed store, the cookie's `SameSite`, `Secure` and expiry attributes,
session id rotation, the `session_version` force-logout check, or the `GET /auth/me` and `POST /auth/logout` handlers
the token rides alongside, all the Sessions subject, which this subject depends on for the session key the middleware
reads and the endpoint the client cache hydrates from. It does not own how a request becomes a `CurrentUser` or the
scope and role checks a handler applies once the CSRF gate has passed (`backend/src/auth/middleware.rs`, the Request
authentication subject): the middleware's own session-user check is a lighter, independent read of the same session key,
not a call into that resolution. It does not own the Content Security Policy a CSRF rejection lacks, only the fact that
it lacks one; that is the Design "Response security headers and CSP", which records the same layering from its side. It
does not own the RFC 9457 Problem Details envelope the rejections render into, the Design "API error contract and
OpenAPI", or the ETag capture and replay `fetch.ts` also performs for a different resource family, the Design
"Conditional requests and optimistic concurrency". It does not own `frontend/src/hooks/useAuthMe.ts`, a second,
independent `/auth/me` reader that happens to parse the same `csrf_token` field into its own react-query cache; that
field is unused by every caller of the hook, so no second cache participates in the CSRF path, and the hook itself
belongs to the Sessions subject.

Depends on: the session's `user_id` claim (`backend/src/auth/session.rs::SESSION_KEY_USER_ID`) to decide whether a
caller is session-authenticated; `GET /auth/me` to carry the token to the browser; `tower_sessions::Session` as the
per-request handle onto the session store. Depended on by: every mutating request `backend/src/routes/` registers under
`/api`, `/auth` or `/opds` from a session-authenticated caller, and every client call through `apiFetch`.

## Structure

### Server: mint and enforce

- `backend/src/routes/auth.rs::callback` (the OIDC redirect target) and `backend/src/routes/auth.rs::local_login`
  (email/password sign-in) each mint the token the same way, inline, on every successful sign-in: 32 bytes from the OS
  CSPRNG (`rand::fill`), base64url-unpadded encoded (`Base64UrlUnpadded::encode_string`, 43 characters), written into
  the session under the literal key `"csrf_token"`. No shared helper produces the value; each site carries the same
  sequence. The session's `user_id` and `session_version` keys are read and written through the `SESSION_KEY_USER_ID`
  and `SESSION_KEY_SESSION_VERSION` constants in `auth/session.rs`; the CSRF key has no such constant in common.
  `csrf.rs` declares its own private `CSRF_SESSION_KEY` for the enforcement read, and the two mint sites and the
  `GET /auth/me` session read in `routes/auth.rs` (the Sessions subject) each use the literal `"csrf_token"`. Both mint
  sites overwrite unconditionally: re-running either flow on an already-authenticated session (a repeat OIDC callback, a
  re-submitted local login) replaces the prior value rather than leaving it in place.
- `backend/src/security/csrf.rs::csrf_required` is an `axum::middleware::from_fn` layer taking the request's
  `tower_sessions::Session` directly, not a `CurrentUser`. It exempts a request in two independent ways before reading
  either the header or the stored token: a safe method (`GET`, `HEAD`, `OPTIONS`, `TRACE`) returns immediately, and a
  request whose session carries no `user_id` claim (`SESSION_KEY_USER_ID`, imported from `auth::session` so this read
  shares the exact key the login helpers write) also passes straight through, covering Basic- and Bearer-authenticated
  callers and every pre-auth `/auth/*` mutation (`local_login`, `/auth/setup`, `/auth/register`, password recovery) in
  one check. A request that carries both a session naming a user and an `Authorization` header is still gated:
  `CurrentUser` resolution (the Request authentication subject) tries the session cookie first and returns it when
  valid, and this middleware reads only the session, so a present `Authorization` header does not exempt a
  session-authenticated caller. This session-user read is unconditional on the session store, independent of
  `session_version`; the middleware never asks whether the session is otherwise still valid, only whether it names a
  user.
- For a session-authenticated mutating request, the header lookup runs before the stored-token lookup: a missing
  `X-CSRF-Token` header returns `AppError::CsrfMissing` without a second session read. Only a present header goes on to
  read the session's `csrf_token`; if that read comes back empty too, the outcome is the same `CsrfMissing` rejection as
  a missing header, whether or not the caller sent one. Only when both sides are present does the middleware compare
  them, with `subtle::ConstantTimeEq` rather than `==`, and a mismatch there is the one path that returns
  `AppError::CsrfMismatch` instead. The branch that answers `428` when the header is present but the session holds no
  token exists in `csrf_required`, but no production path reaches it: the only session writer, `auth::session::login`,
  is called from the two sign-in handlers that mint the token in the same request, and setup, registration and password
  reset establish no session at all.
- `backend/src/lib.rs::build_router_with_session_store` layers `csrf_required` onto the composite `/api`, `/auth`,
  `/opds` router (`api_like`) after `api_csp_layer`, which places it outside that layer: a request the middleware
  rejects never reaches `next.run`, so it never reaches `api_csp_layer` either. `session_layer` (`SessionManagerLayer`)
  wraps the composite router, `csrf_required` included, so the `Session` extractor the middleware takes is always
  already populated by the time it runs.

The pre-authentication mutations (`POST /auth/local/login`, `/auth/setup`, `/auth/register`, forgot-password and
reset-password) sit outside the gate because no session names a user yet at the point they run. A cross-site HTML form
cannot reach them: their bodies go through `ApiJson`, which answers `415` to a form content type (the Design "API error
contract and OpenAPI" owns that extractor), and no CORS layer exists in the backend, so a cross-site script request
carrying a JSON body fails its preflight before it can reach the handler.

### Client: cache, inject, retry

- `frontend/src/api/csrf.ts` holds one module-level variable, `cachedToken`. In the app's runtime code it is written
  only by `refreshCsrfToken`, which fetches `GET /auth/me` directly (not through `apiFetch`, to avoid re-entering the
  wrapper it hydrates), parses the body with a narrow Zod shape requiring a non-empty string, and sets the cache to that
  value or to `null` on any failure: a network error, a non-OK status, a body that fails to parse, an omitted field, an
  explicit `null` (Basic-auth sessions, which carry no token), or an empty string. The same module also exports two
  test-only escape hatches, `__resetCsrfTokenForTesting` and `__seedCsrfTokenForTesting`, that assign `cachedToken`
  directly, bypassing `refreshCsrfToken`; production code never calls either. `getCsrfToken` is a synchronous,
  non-hydrating read of the cache.
- Three call sites trigger `refreshCsrfToken`, the only writer of `cachedToken` in runtime code: `apiFetch` in
  `frontend/src/api/fetch.ts`, lazily, before a mutating request's first send when the cache is still `null` (the only
  hydration path for a session that began with OIDC, which never calls `loginLocal`); `apiFetch` again, once, after a
  `403 csrf-mismatch` or `428 csrf-missing` response to a mutating request; and `loginLocal`
  (`frontend/src/api/auth.ts`), eagerly, right after a successful `POST /auth/local/login` response.
  `frontend/src/hooks/useAuthMe.ts` performs its own independent `/auth/me` fetch for identity display and parses the
  same `csrf_token` field into its own react-query cache, but nothing reads that copy back out, so it never feeds
  `cachedToken` and plays no part in this subject's behaviour.
- `sendRequest` in `fetch.ts` is the sole point that sets the `X-CSRF-Token` header, and only for `POST`, `PUT`, `PATCH`
  or `DELETE`: it reads `getCsrfToken()` and sets the header only when the cache holds a value, omitting the header
  rather than sending an empty one when the cache is `null`, so a cold cache always surfaces as the server's `428`
  rather than a client-manufactured blank header. No other module in the frontend sets this header;
  `frontend/src/lib/theme/api.ts`'s theme `PATCH` routes through `apiFetch` to inherit this behaviour rather than
  implementing it a second time.
- `apiFetch` recognises a mismatch or missing rejection by the RFC 9457 `type` field's slug suffix (`/csrf-mismatch`,
  `/csrf-missing`), not by status code alone, so a `403` from an unrelated cause (`AppError::Forbidden`) is never
  mistaken for a CSRF rejection and is never retried; a `403` or `428` on a `GET` is likewise never retried, because
  only a mutating request enters this branch. On a recognised rejection it calls `refreshCsrfToken` once, sends the same
  request once through `sendRequest` (re-capturing any `ETag` on the retried response, per the Design "Conditional
  requests and optimistic concurrency"), and returns or throws from that single retry outcome unconditionally: a second
  rejection, even a CSRF one, is not retried again and reaches the caller as an `ApiError`.

## Interfaces and dependencies

- The wire contract is the `X-CSRF-Token` request header (`CSRF_HEADER` in `csrf.rs`) against the session-carried
  `csrf_token` string, and the `csrf_token: Option<String>` field `GET /auth/me` (the Sessions subject) returns: a
  43-character base64url-unpadded string for a session-authenticated caller, or `null` for a Basic-auth OPDS session,
  per the field's OpenAPI description and a backend test that pins both the length and the character set.
- The two rejections are RFC 9457 Problem Details bodies distinguished by `type`:
  `https://reverie.example/probs/csrf-missing` at `428 Precondition Required`
  (`AppError::CsrfMissing`/`problems::CSRF_MISSING`) and `https://reverie.example/probs/csrf-mismatch` at
  `403 Forbidden` (`AppError::CsrfMismatch`/`problems::CSRF_MISMATCH`); the envelope shape is the Design "API error
  contract and OpenAPI". `AppError::Internal`, raised only when the middleware's own session read fails, renders through
  the same envelope at `500 Internal Server Error` with a generic detail message; that path is logged server-side at
  `error` and is not distinguished from any other internal failure by the client.
- `frontend/src/api/errors.ts`'s `ApiError` is the shape every rejection this subject cannot recover from (a non-retried
  retry outcome, or the initial `500`) surfaces to a caller as; it carries the response's status and the parsed
  `type`/`detail`, which is how a caller could in principle distinguish a CSRF failure from any other, though no current
  caller does so beyond `apiFetch`'s own retry.

## Data and state

- **The session-carried token.** Lives in the caller's session row (the Sessions subject) under the key `csrf_token`,
  alongside `user_id` and `session_version`. Its lifetime is the session's: it is written once per successful sign-in
  and never rewritten independently of a fresh sign-in on that same session, so it survives every request the session
  survives and disappears only when the session itself is flushed (force-logout, disabling, or explicit
  `POST /auth/logout`) or expires. Nothing in this subject rotates it on any event short of a new sign-in; in particular
  a `session_version` bump (a role change, an account disable) does not rewrite this key directly, it invalidates the
  whole session, and the token that reaches the browser next comes from the fresh sign-in that follows, not from an
  in-place rotation of the old one.
- **The client cache (`cachedToken`).** Module-level, page-lifetime JavaScript state with no persistence of its own: it
  starts `null` on every fresh load of the SPA and is populated only by `refreshCsrfToken`, as the Structure section's
  call-site list enumerates. It is not cleared on logout by any explicit call; `UserMenu.tsx`'s sign-out handler
  (`frontend/src/components/shell/UserMenu.tsx`) navigates the browser to `/login` with a full-page assignment after the
  logout request settles, and that navigation, not an explicit reset, is what discards the module state.

## Runtime behaviour

**A mutating request from a cold client cache**, the common case for the first `POST`, `PUT`, `PATCH` or `DELETE` after
an OIDC session begins:

1. `apiFetch` sees `getCsrfToken() === null` and awaits `refreshCsrfToken`, which fetches `GET /auth/me` and, on a
   successful, schema-matching response, sets `cachedToken` to the returned string.
2. `sendRequest` builds the outgoing request, reads the now-populated cache, and sets `X-CSRF-Token`.
3. `csrf_required` reads the session's `user_id` (present), then the request header (present), then the session's
   `csrf_token` (present, minted at sign-in), and compares the two in constant time; they match, so the request proceeds
   to `next.run`.
4. The route handler runs, and `api_csp_layer`, inside the CSRF layer, attaches the API Content Security Policy to
   whatever status the handler returns.

**A stale cache after a second tab re-authenticates on the same browser.** Two tabs share one cookie jar. Tab A re-runs
the OIDC callback (a re-authentication, not a fresh browser session), which mints a new token and overwrites the
session's `csrf_token`; tab B's `cachedToken`, populated before that happened, still holds the old value:

1. Tab B issues a mutating request with its stale header. `csrf_required` finds a session user (the shared cookie still
   names one), a present header, and a present but now-different stored token; the constant-time comparison fails, and
   the middleware returns `AppError::CsrfMismatch` without calling `next.run`, so the response carries no
   `Content-Security-Policy` header.
2. `apiFetch` reads the `403`'s `type` field, matches the `csrf-mismatch` suffix, and calls `refreshCsrfToken`, which
   fetches `GET /auth/me` under the now-current session and returns the fresh token.
3. `sendRequest` sends the original request again with the refreshed header. `csrf_required` finds a match this time and
   forwards it; the retried response's status is what `apiFetch` returns or throws.

**A Basic-authenticated OPDS request**, for example an e-reader client issuing `PUT` against the shelves reorder
endpoint with device-token or HTTP Basic credentials and no browser cookie: `csrf_required` reads the session's
`user_id` claim, finds none (no session cookie was presented, or the cookie names no user), and calls `next.run`
immediately. The request reaches its handler and whatever scope and role checks it applies, without ever reading a
header this client was never asked to send.

**A session-store read failure inside the middleware itself**, for example a transient database error while reading the
`user_id` or `csrf_token` session key: `csrf_required` maps the store error to `AppError::Internal`, which logs at
`error` and renders as a generic `500`. `apiFetch` only retries on `403`/`428` with a recognised CSRF slug, so a `500`
here is never retried; it reaches the caller as an `ApiError` on the first attempt.

## Failure and recovery

- **Missing token, either side.** A session-authenticated mutating request with no `X-CSRF-Token` header, or one whose
  header is present but whose session carries no `csrf_token` at all, both resolve to the identical `428` `CsrfMissing`
  rejection; the middleware does not distinguish "the client never had a token" from "the client sent one the session no
  longer recognises as a key" once the session-side value is absent. The client's retry logic treats this the same as a
  mismatch: refresh once, resend once.
- **Mismatch.** Both sides present but unequal under constant-time comparison returns `403` `CsrfMismatch`. This is the
  path where an attacker-supplied value that differs from the real token is distinguishable from a token the caller
  simply never had; the constant-time compare exists so that distinguishing them does not leak timing information about
  the real value.
- **A retry that also fails.** If `refreshCsrfToken` itself fails (network error, an expired session that `GET /auth/me`
  now answers with an auth failure), `cachedToken` is left or reset to `null`, and the retried `sendRequest` omits the
  header entirely; the second attempt then earns its own `428` from the server, which `apiFetch` does not retry again,
  so the caller sees an `ApiError` from the outcome of the retry rather than the original rejection.
- **A CSRF-rejected logout.** `POST /auth/logout` is a session-authenticated mutation like any other and is subject to
  the same gate: a missing or mismatched token on it returns `428` or `403` before the handler (the Sessions subject)
  ever runs, so the session row is not flushed. `UserMenu.tsx`'s sign-out handler catches any `apiFetch` failure, logs
  it, and navigates to `/login` regardless; the client-visible effect (the SPA lands on the sign-in screen with an empty
  token cache) is the same whether the server-side session was actually destroyed or merely left to its normal idle
  expiry.
- **No response class this middleware returns carries a Content Security Policy.** Every rejection above (`428`, `403`,
  `500`) is returned directly from `csrf_required` without calling `next.run`, and `api_csp_layer` only attaches its
  header on that inner call; the uniform headers from further out in the layer stack still apply. The Design "Response
  security headers and CSP" records this same fact from its side, as the one response class in its writer census with no
  `Content-Security-Policy` writer.

## Security and operations

`backend/src/security/mod.rs` marks every module beneath it, `csrf.rs` included, Tier 2 security-critical under the
repository's comment policy. The middleware's own module doc records the load-bearing design choice explicitly: the
exemption keys on the caller's authentication method (does the session name a user at all), never on whether a token
happens to be present. Keying on token presence instead would be a bypass, since an attacker-shaped cross-site request
carries no token and would then read as "exempt" rather than "missing".

The token itself never appears in a URL, a log line, or anywhere the codebase writes structured tracing output for this
subject; the only place it is read back to the caller is the `GET /auth/me` JSON body over the caller's own
authenticated connection. Comparison is constant-time (`subtle::ConstantTimeEq`) specifically so that a byte-by-byte
guessing attack cannot use response timing to recover the stored value.

`SameSite=Lax` on the session cookie (the Sessions subject) and the API Content Security Policy (the Design "Response
security headers and CSP") are both additional layers against the same cross-site-request class this synchronizer token
defends against directly; this subject does not depend on either being correctly configured, and neither substitutes for
the header check if it were removed. The CodeGuard deviation register records `SameSite=Lax` itself as an accepted
deviation from a stricter default, with its own compensating-controls rationale, which this subject does not restate.

Seven mutating routes take no body: `POST /auth/logout`, the enrichment trigger and dry-run `POST`s, the ingestion scan
`POST`, and the three `DELETE`s (a token, a shelf, a shelf item). The JSON-only extractor that keeps a cross-site HTML
form off the pre-authentication mutations does nothing for these, so a form-encoded cross-site `POST` reaches the
handler with whatever credential the browser attached. For the session cookie that is two layers: `SameSite=Lax`
withholds the cookie on a cross-site `POST` navigation, and this middleware refuses the request if the cookie arrives
anyway. For a cached Basic credential it is one, and it is browser behaviour rather than anything this subject checks: a
browser re-sends the credential preemptively only within the protection space where it was challenged (RFC 7617 §2.2),
Reverie challenges with `Basic` on the OPDS routes alone, no mutating route exists there, and the `/api/v1` challenge is
`Bearer`. The three `DELETE`s additionally trigger a CORS preflight from any script, which no CORS grant answers.

An operator's own deployment cannot weaken this check: there are no environment variables or settings that alter
`csrf_required`'s behaviour, and the exemption for Basic- and Bearer-authenticated OPDS clients and device-token
consumers is fixed by credential type in code, not configurable per instance.

## More information

- [CodeGuard deviation register](../../../security/codeguard/README.md), deviation 2: `SameSite=Lax` instead of
  `Strict`, the session-cookie layer alongside this subject's synchronizer token.
