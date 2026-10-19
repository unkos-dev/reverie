---
type: DESIGN
profile-version: 1
id: "REV-DESIGN-0011"
title: "Sessions"
satisfies:
  - "REV-REQ-0034"
  - "REV-REQ-0035"
  - "REV-REQ-0036"
  - "REV-REQ-0037"
  - "REV-REQ-0038"
governed-by:
  - "REV-ADR-0015"
---

# Sessions

This Design covers the first-party session mechanism that carries a browser's authenticated identity across requests:
the Postgres-backed store, the cookie policy and idle-expiry window the session layer is constructed with, the id
rotation and identity claims written at sign-in, the hourly expiry sweep, the `GET /auth/me` profile endpoint, and the
client's single funnel for reacting to a lapsed session.

## Purpose and boundaries

This subject owns: the `tower_sessions::SessionStore` implementation over `tower_sessions.session`
(`backend/src/auth/store.rs`); the cookie attributes and expiry policy the session layer is constructed with
(`build_router_with_session_store` in `backend/src/lib.rs`); the login and logout helpers that rotate the session id and
write or clear the identity claims a session carries (`backend/src/auth/session.rs`); the hourly expired-row sweep
(`backend/src/services/session_sweep.rs`); the `GET /auth/me` profile endpoint and the `POST /auth/logout` handler
(`backend/src/routes/auth.rs`); and, on the client, the shared `/auth/me` query, the once-guarded funnel that reacts to
a lapsed session, and the per-device hint that scopes client caches to the last confirmed account
(`frontend/src/hooks/useAuthMe.ts`, `frontend/src/hooks/useSessionRecovery.ts`, `frontend/src/lib/query/client.ts`,
`frontend/src/lib/active-user.ts`, `frontend/src/components/AuthenticatedBoundary.tsx`).

It does not own how a session's identity claims are read back and turned into a `CurrentUser`, the disabled-account and
stale-`session_version` checks that decide whether a rehydrated session is still valid, or the Basic/Bearer credential
paths a request falls back to when no session claims an identity, all of which belong to the Design "Request
authentication". It does not own minting or validating the CSRF synchronizer token the session also carries (session key
`csrf_token`), which is the Design "CSRF protection"; this subject only carries that value and echoes it back from
`/auth/me`. `POST /auth/logout` is a session-authenticated mutating request like any other and sits under that Design's
gate: a caller with no `X-CSRF-Token` header receives `428`, and a caller whose header does not match the session's
token receives `403`, before this subject's `logout` handler runs; the handler's own doc comment records both outcomes.
It does not own the row-level-security exemption or the role grants on `tower_sessions.session`, which are the Design
"Row-level security and database context". It does not own the local password or OIDC credential flows that call into
this subject's `login` helper once a credential has already been verified, those being the Design "Local password
sign-in" and the Design "OIDC sign-in" (`backend/src/routes/auth.rs::callback`). It does not own the response security
headers a session-layer failure response lacks, only the fact that it lacks them; that is the Design "Response security
headers and CSP".

Depends on: the `users` table's `session_version` and `disabled_at` columns, which `CurrentUser` reads to decide whether
a session's claims are still valid; the CSRF token minted into the session at the end of a successful sign-in; the
cached `GET /auth/setup/status` response the client reads to choose a redirect target for a lapsed session.

Depended on by: every request whose identity resolution reaches the session-cookie leg of `CurrentUser`; the local and
OIDC sign-in handlers, which call this subject's `login` helper once a credential is verified; the authenticated
boundary (`frontend/src/components/AuthenticatedBoundary.tsx`), which mounts the 401-recovery funnel above every route
that requires a session, and the app shell (`frontend/src/App.tsx`, `frontend/src/components/shell/AppShell.tsx` and
everything it renders), which reads the shared `/auth/me` query for the signed-in identity; and the library's
first-paint display-preference mirror, which reads the per-device active-user hint this subject writes to know whose
cached values to seed before its own request resolves.

## Structure

### State-writer census

| State item | Where it lives | Single owner |
| ---------- | -------------- | ------------ |
| Session row (`id`, `data`, `expiry_date`) | `tower_sessions.session` | `PostgresStore`, via `SessionManagerLayer` |
| `user_id` / `session_version` claims | Same row, in `data` | `auth::session::login`; write-only here |
| `csrf_token` claim | Same row, in `data` | The Design "CSRF protection" (read back only, in `me`) |
| Session cookie (`id`) | Browser cookie jar | `SessionManagerLayer`, built once |
| `sign_in_started` marker | Session `data` | OIDC start |
| Cohort and `won_at` | `sign_in_cohorts` | `models::sign_in`; session deletion cascade |
| Pending OIDC attempt | `oidc_sign_in_attempts` | `models::sign_in`; cohort deletion cascade |
| 401-funnel handler + guard | `client.ts` module state | `setUnauthenticatedHandler` / `invokeUnauthenticatedHandler` |
| `reverie_last_user` (active-user hint) | `localStorage`, one key | `rememberActiveUser` / `forgetActiveUser` |

`auth::session::signed_in_user` reads `user_id` and `session_version` for `CurrentUser` and the three sign-in entry
points. It flushes a disabled, deleted or stale identity, logging a failed flush at warning level. It returns
`SignedIn(User)`, `Disabled` or `NotSignedIn`. `CurrentUser` refuses `Disabled` without credential fallthrough, while
the sign-in handlers treat it as signed out. The Design "Request authentication" owns the validity rule.

Call sites that dispatch to those owners:

- Identity claims are written by the two `auth::session::login` call sites, in the OIDC callback and `local_login`. OIDC
  start writes the anonymous marker. Logout, `signed_in_user` and post-win failure handling clear the claims and delete
  the row through the session API. The worker deletes expired rows. Other requests re-save through
  `with_always_save(true)`, renewing `expiry_date` without changing `data`.
- `unauthenticatedHandler`: replaced once, by the mount effect in `AuthenticatedBoundary`
  (`frontend/src/components/AuthenticatedBoundary.tsx`), and reset to a no-op on unmount. `redirecting` is reset by the
  same call (`setUnauthenticatedHandler`) and flipped exactly once per lapse by `invokeUnauthenticatedHandler`.
- `invokeUnauthenticatedHandler` is called from three places: `queryClient`'s `QueryCache.onError`, for any query that
  rejects with an `ApiError` whose status is 401; its `MutationCache.onError`, for any mutation that does; and
  `useSessionRecovery`'s effect, when the shared `/auth/me` query settles with `data === undefined` (a 401 or 403
  response, per `useAuthMe`).
- `rememberActiveUser`: the one call site is inside `useAuthMe`'s `queryFn`, on every successful parse of `/auth/me`,
  not only at login, so a boot with an already-live session records the hint the same way a fresh sign-in does.
- `forgetActiveUser`: two call sites, `invokeUnauthenticatedHandler` (the 401/lapsed-session path) and the sign-out
  handler inside `UserChip` in `frontend/src/components/shell/UserMenu.tsx` (explicit sign-out).

The session store owns row persistence and deletion. The sign-in model owns cohort and attempt mutations for both
providers: `join_pending` and `start_oidc_attempt` create cohorts, `start_oidc_attempt` replaces the pending attempt,
`claim_oidc_attempt` consumes it, and `claim_win` conditionally sets `won_at`. Both local sign-in and the OIDC callback
call `claim_win`; its one conditional database update is the shared ownership rule. A loser cannot provision identity or
log in. The asynchronous session worker calls `delete_expired_attempts` in batches, and foreign-key cascades remove
cohorts and attempts when rotation, logout, invalid-identity flushing or expiry sweeping deletes a session row.

OIDC start writes `sign_in_started = true` and explicitly saves before creating its attempt. This keeps the anonymous
session non-empty and gives the cohort a persisted parent. The CSRF subject owns `csrf_token`; OIDC secrets live in the
attempt table. After a win, a handler error before `session::login` succeeds also calls `session.flush()`. These calls
all use the same session-store deletion owner.

### Component relationships

- `backend/src/auth/store.rs::PostgresStore` is the only `SessionStore` implementation this subject builds. `create`
  inserts atomically (`ON CONFLICT (id) DO NOTHING RETURNING id`) and, on a `NULL` return (an id collision), draws a
  fresh `Id` and retries in a loop rather than falling back to an unconditional upsert; `save` always upserts
  (`ON CONFLICT (id) DO UPDATE`); `load` filters `WHERE expiry_date > now()`; `delete` removes the row by id. The same
  type implements `ExpiredDeletion::delete_expired` (`DELETE ... WHERE expiry_date < now()`), which
  `backend/src/services/session_sweep.rs` drives on a timer.
- `backend/src/lib.rs::build_router_with_session_store` constructs the one `SessionManagerLayer` the whole composite
  router shares: `with_http_only(true)`, `with_secure(state.config.security.behind_https)`,
  `with_same_site(SameSite::Lax)`, `with_expiry(Expiry::OnInactivity(24h))`, and `with_always_save(true)`. It is added
  to the router after the `security_headers` layer, so it wraps outside that layer (see Failure and recovery).
- `backend/src/auth/session.rs` is the write side of a session's identity claims: `login(session, user)` calls
  `Session::cycle_id` before inserting `SESSION_KEY_USER_ID` and `SESSION_KEY_SESSION_VERSION`; `logout(session)` calls
  `Session::flush`. Both constants (`SESSION_KEY_USER_ID`, `SESSION_KEY_SESSION_VERSION`) are `pub(crate)` and shared
  with `backend/src/auth/middleware.rs`'s read of the same keys, a module boundary this Design's boundary section above
  hands to the Design "Request authentication".
- `backend/src/routes/auth.rs::me` and `::logout` are this subject's two HTTP entry points once a session already
  exists: `me` requires a `CurrentUser` (so an unauthenticated or invalid session never reaches it) and echoes the
  session's `csrf_token` claim alongside the caller's profile; `logout` requires only a `Session` extractor, so it is
  idempotent for a caller with no session to destroy, and sits under the CSRF gate described in Purpose and boundaries
  for a caller that does have one.
- On the client, `frontend/src/hooks/useAuthMe.ts` is the one `/auth/me` query every other piece of this subject reads
  or reacts to: `frontend/src/hooks/useSessionRecovery.ts` observes its settled state, and
  `frontend/src/components/shell/UserMenu.tsx` reads its `data` to render the signed-in identity.
  `AuthenticatedBoundary` wires `useSessionRecovery` and the `QueryCache` and `MutationCache` handler together in a
  pathless layout route that every authenticated route sits under (the app shell route and `/account/password`), in a
  declared order it documents as load-bearing (see Failure and recovery). `frontend/src/lib/query/client.ts` is the one
  module owning the once-guarded funnel both paths call into.

## Interfaces and dependencies

- `tower_sessions::SessionStore` and `tower_sessions::session_store::ExpiredDeletion`, the two traits `PostgresStore`
  implements; `tower_sessions::Session`, the per-request extractor every handler that touches a session (including `me`,
  `logout`, and the two `login` call sites) takes as a parameter.
- `auth::session::login(session: &Session, user: &User) -> Result<(), tower_sessions::session::Error>` and
  `auth::session::logout(session: &Session) -> Result<(), tower_sessions::session::Error>`, called by the sign-in and
  sign-out handlers respectively.
- `GET /auth/me` (`MeResponse`: `id`, `display_name`, `email`, `role`, `is_child`, `has_local_password`,
  `theme_preference`, `csrf_token`) and `POST /auth/logout` (`204 No Content`), documented in the OpenAPI spec generated
  from `backend/src/routes/auth.rs`.
- `frontend/src/hooks/useAuthMe.ts` exports `useAuthMe`, returning `{ data, isLoading, isError }` over a Zod-validated
  `AuthMe`; the query function returns `null` on a `401` or `403` response, and the hook exposes that as
  `data: undefined` (`data ?? undefined`) so callers treat it as the ordinary "logged out" state rather than an
  operational error. `frontend/src/lib/query/client.ts` exports `setUnauthenticatedHandler`,
  `invokeUnauthenticatedHandler`, and the shared `queryClient` singleton; `frontend/src/lib/active-user.ts` exports
  `activeUserId`, `rememberActiveUser`, and `forgetActiveUser`.
- The operator-facing cookie table in `docs/security/content-security-policy.md` documents the session cookie's wire
  name (`id`), path (`/`), and observed `Max-Age` (24 hours, renewed on each request) as the canonical reference for
  self-hosters; this Design does not restate it as a second source.

The profile's `has_local_password` predicate uses a credential-existence query without reading the password hash,
independently of external identities. The shell shows "Change password" for local and dual-capability accounts and hides
it for OIDC-only accounts. The password-change endpoint still rejects an account without a local credential. The client
requires the capability field in its Zod schema rather than assuming it when absent.

## Data and state

- **The session row.** `tower_sessions.session` (`id text`, `data bytea`, `expiry_date timestamptz`), created by the
  initial schema migration, with a check constraint bounding `expiry_date` to the range a `timestamptz` can decode
  (`session_expiry_date_ts_decode_range`, `'0001-01-01 00:00:00+00' <= expiry_date < '10000-01-01 00:00:00+00'`). `data`
  holds the whole `tower_sessions::session::Record` (id, claims map, expiry) MessagePack-encoded; the `id` and
  `expiry_date` columns are kept alongside for lookup and the load-time filter. `reverie_app` holds
  `SELECT, INSERT, DELETE, UPDATE` on the table; `reverie_readonly` holds a column-scoped `SELECT (expiry_date)` only,
  because the `id` column is the credential the session cookie carries. The table is row-level-security-exempt, and both
  the exemption and these grants belong to the Design "Row-level security and database context", not this one.
- **Session lifetime.** `Expiry::OnInactivity(24h)` is the only expiry policy configured; no absolute session lifetime
  exists anywhere in `Config`, and the policy is the same for every account regardless of role, admin included, there is
  one `SessionManagerLayer`, built once, wrapping the whole composite router. Because `with_always_save(true)` saves
  every request that carries a session, `expiry_date` recomputes from the most recent request rather than staying fixed
  from login: a session that keeps being used never idles out, and one left alone expires 24 hours after its last
  request.
- **The cookie.** Wire name `id` (the session library's own default; Reverie sets no name), path `/` (also the library's
  default), no `Domain` attribute and no `__Host-` prefix. `HttpOnly` always; `SameSite=Lax`; `Secure` only when
  `state.config.security.behind_https` is `true` (an operator-set flag, never detected automatically), the browser
  judges `Secure` against its own leg to the edge, so a deployment behind a TLS-terminating proxy still qualifies even
  though the proxy-to-backend hop is plain HTTP. The cookie is unsigned: nothing about its contents is validated by a
  signature, because the value is only a cryptographically random session id and the claims it names live server-side.
- **The session id.** 128 bits, drawn from the `rand` crate's default CSPRNG (`rand::rng().random()` inside
  `tower_sessions::session::Id::default`), giving a session-fixation attacker no practical way to guess a live id.
- **Identity claims.** `SESSION_KEY_USER_ID` (a `Uuid`) and `SESSION_KEY_SESSION_VERSION` (an `i32`, a snapshot of
  `users.session_version` at login) are the two keys this subject writes; a session with neither key is anonymous (no
  cookie yet, or a cookie whose claims were never established, e.g. mid-OIDC-flow before `/auth/callback` completes).
- **The active-user hint.** `localStorage["reverie_last_user"]`, one key per browser rather than per account: it names
  whichever account last confirmed a session on this browser, not a set of accounts. It grants nothing on its own, every
  request still goes through the session cookie, and exists only so a per-user client cache has an id to key on before
  that request's response has arrived. The library's first-paint display-preference mirror
  (`frontend/src/pages/library/display-storage.ts`) is the one caller that reads it synchronously before any request
  resolves, to choose which account's mirrored presentation to seed on first paint; the hint itself is written only from
  a validated `/auth/me` response (see Security and operations), so a stale or attacker-written value can only misdirect
  which cached presentation a page seeds before its own request resolves, never which account a request authenticates
  as.
- **The 401 funnel's guard state.** `unauthenticatedHandler` and `redirecting` are plain module-level variables in
  `frontend/src/lib/query/client.ts`, living for the JS realm's lifetime; a full-page navigation (which every path that
  sets `redirecting` ends in) discards them along with everything else in that realm.

## Runtime behaviour

**Establishing a session at sign-in**, after any live anonymous cohort has been won, for either of the two call sites
(`local_login` or the OIDC callback in `backend/src/routes/auth.rs`), once the credential has already been verified:

1. The handler calls `auth::session::login(&session, &user)`.
2. `login` calls `session.cycle_id()`. If the request arrived with an existing (pre-auth) session id, this replaces it
   before any identity claim is attached. Deleting the old row cascades `sign_in_cohorts` and its attempt, so a slower
   contender's conditional winner update matches nothing. A pre-authentication id never becomes authenticated.
3. `login` inserts `SESSION_KEY_USER_ID` and `SESSION_KEY_SESSION_VERSION` into the session's in-memory claims map.
4. The OIDC callback inserts its CSRF token and explicitly saves before returning success. The local handler relies on
   response-time persistence. `SessionManagerLayer`'s `with_always_save(true)` also triggers a store write for a
   populated session. Because the id was just cycled, this is a fresh id from the store's point of view:
   `PostgresStore::create` runs its atomic `ON CONFLICT (id) DO NOTHING RETURNING id` insert; on the cryptographically
   improbable case of a collision with a live row, `create` draws a new `Id` and retries the same insert rather than
   falling back to an overwrite, so a concurrent creator can never clobber a session that already exists under that id.
5. The response carries a `Set-Cookie` for `id` with the attributes fixed in `build_router_with_session_store`.

Arbitration covers only requests that load the session before a winner rotates it; a request carrying an already retired
id loads without a session and is treated as a request without a cookie, still needs its own valid credentials to sign
in, and is refused by the OIDC callback.

**Every subsequent request on that session**, whether it touches session data or not:

1. The session-cookie leg of `CurrentUser` (owned by the Design "Request authentication") loads the session via
   `PostgresStore::load`, which filters `WHERE expiry_date > now()`: a row whose expiry has already passed is never
   returned, so an expired cookie can never resolve to a live session regardless of what `CurrentUser` does next.
2. At response time, `with_always_save(true)` triggers a store write via `PostgresStore::save` (an upsert), which
   recomputes `expiry_date` to 24 hours from now. A read-only request such as `GET /auth/me` renews the window exactly
   the same way a mutation does, because the save is unconditional on the layer, not on whether the handler wrote to
   `session`.

**Reacting to a lapsed session on the client**, the funnel its independent triggers share. Every authenticated route is
a descendant of the `AuthenticatedBoundary` layout route; the pre-auth screens (`/login`, `/setup`, `/forgot-password`)
are siblings of it, so no handler is wired there and a 401 from a sign-in attempt does not redirect:

1. `AuthenticatedBoundary`'s mount effect calls `setUnauthenticatedHandler` with a provider-aware redirect (the OIDC
   initiator when the cached `/auth/setup/status` reports OIDC enabled, `/login` otherwise, falling back to `/login` if
   that query itself fails) before any other effect in the tree runs, because React commits effects in declaration order
   and this effect is declared first.
2. `useSessionRecovery` (mounted immediately after) reads the shared `/auth/me` query via `useAuthMe`; when that query
   has settled (`!isLoading`) without an operational error (`!isError`) and its data is `undefined`, the shape a 401 or
   403 response takes, its own effect calls `invokeUnauthenticatedHandler()`.
3. Independently, any other query on the page that rejects with an `ApiError` whose status is 401 trips the same call
   through `queryClient`'s `QueryCache.onError`, and any mutation that does so through its `MutationCache.onError`; a
   mutation's own `onError` still runs. A lapsed session commonly trips several requests in the same tick (every
   in-flight request on the page); each one calls `invokeUnauthenticatedHandler`, but only the first to run finds
   `redirecting` still `false`, it sets the guard, clears the active-user hint via `forgetActiveUser()`, and invokes the
   wired handler (a `window.location.assign` to the resolved target); every other caller in the same tick or after finds
   the guard already set and returns without effect.
4. The full-page navigation the handler performs ends the funnel: the guard and the handler reference are discarded with
   the rest of the JS realm, and a fresh page load re-wires both from `AuthenticatedBoundary`'s mount effect.

**The hourly sweep**, driven from `run` alongside the other background workers: `run_sweep` ticks on a fixed one-hour
interval and skips its first tick. It reclaims each authentication source map through governor's `retain_recent`, then
runs `ExpiredDeletion::delete_expired` once. It deletes expired OIDC attempts in batches of 1,000 and reclaims inactive
account-throttle rows through the [local password admission model](0012-local-password-sign-in.md). Session deletion
cascades cohort rows; live attempts remain. Cancellation is checked ahead of the timer and between database batches.
Each cleanup failure logs and retries on a later tick; source-map reclamation precedes database work.

## Failure and recovery

- **A session id collision at creation.** Handled inline in the runtime walkthrough above: `create` regenerates and
  retries rather than surfacing an error to the caller. A real collision is cryptographically improbable (128-bit ids),
  so the loop is a correctness guarantee, not a path expected to iterate in practice.
- **A stale or invalid identity claim.** `signed_in_user` flushes disabled, deleted and stale-version sessions.
  `CurrentUser` rejects a disabled account with 401; a deleted or stale identity permits Basic/Bearer fallthrough.
  Sign-in handlers treat both cases as anonymous. In OIDC start, if an incoming id disappears during that read,
  `cycle_id()` rotates the cached record before the marker is inserted and saved. `flush()` retains that cached record
  id, so rotation prevents its deleted id from being recreated. Rotation errors propagate as internal errors.
- **An error after winning.** A callback failure through identity attachment, CSRF insertion or explicit persistence
  attempts session flush before redirecting to the fixed login outcome. Local sign-in flushes a failure before
  `session::login` succeeds. A successful flush removes the cookie and cascades the cohort. A failed flush logs a
  warning; any surviving cohort stays won and cannot authorise another sign-in before session expiry. If old-session
  deletion fails during rotation, `cycle_id` has already cleared its current id. The later flush cannot remove the
  original row, even if it returns success. The rotation error is logged at warning level as failed old-session removal.
  The anonymous session row and its won cohort remain until idle expiry or the sweep. The row carries no identity and
  the cohort is terminal, so it cannot authenticate or win again. The callback redirects to `unavailable` with its
  cookie cleared; no explicit original-session cleanup occurs.
- **A failed session-store save.** `SessionManagerLayer` is registered after (and so wraps outside) the
  `security_headers` layer in `build_router_with_session_store`. When a request carries a non-empty session, the
  response is not already a server error, and the store write triggered by `with_always_save(true)` fails, the session
  layer discards the handler's response and substitutes an empty `500` built from `Response::default()`. Because that
  substitution happens outside `security_headers`, the response carries none of the uniform security headers or a CSP,
  the consequence for response headers belongs to the Design "Response security headers and CSP"; this subject's part is
  that the save can fail (store I/O error) and that the layer's own response on that path bypasses every layer nested
  inside it, this store included.
- **A missed or failed sweep tick.** `sweep_once` returns its error and `run_sweep` logs it at `warn` and carries on;
  the next tick retries. This is availability hardening only: `PostgresStore::load`'s `expiry_date > now()` filter is
  what keeps an expired session from authenticating. Attempt claims independently require `expires_at > now()`. A failed
  attempt batch is also logged at warning level and retried on the next tick; backlog affects table size.
- **The `/auth/setup/status` lookup failing during redirect resolution.** `resolveLoginRedirect`
  (`AuthenticatedBoundary.tsx`) wraps the cached query in a `try`/`catch` and falls back to `/login` on any failure, so
  a lapsed session's redirect never strands the caller on a broken provider-detection request; it degrades to the
  always-valid local login form.
- **Sign-out whose `/auth/logout` request fails.** The sign-out handler inside `UserChip` in
  `frontend/src/components/shell/UserMenu.tsx` logs the error to the console and still calls `forgetActiveUser()` and
  navigates to `/login`: the client-visible outcome of sign-out does not depend on the request having reached the
  server, because the session's own 24-hour idle expiry is the backstop for a row the request failed to delete.

## Security and operations

The cookie policy trades two invariants a stricter default would hold, and both are recorded CodeGuard deviations:
`Secure` follows `behind_https` rather than always being set, because a browser never sends a `Secure` cookie over plain
HTTP and an always-set flag would make an HTTP-only deployment unable to hold a session at all; and `SameSite=Lax`
rather than `Strict`, because the OIDC authorization-code redirect back from an identity provider is a cross-site
navigation that `Strict` semantics would block. Session security in both cases rests on the cryptographic randomness of
the session id itself, not on the cookie's transport attributes.

The 24-hour idle window (`Expiry::OnInactivity`, renewed by `with_always_save` on every request) is also a recorded
deviation from a shorter-timeout default; it applies identically to every account this subject issues a session for,
with no separate, shorter, or otherwise stricter session policy for an admin-role account. Logging out, or a
force-logout via `session_version`, invalidates a session immediately rather than waiting for it to idle out; short of
either of those, a session with regular activity does not expire.

The active-user hint is a cache-scoping convenience, not an authorization boundary: it grants no request anything, and a
request's actual access is decided entirely by the server-side session and the credential paths `CurrentUser` resolves.
On the write side, `rememberActiveUser` is called only from inside `useAuthMe`'s `queryFn`, after a fetched `/auth/me`
response has been parsed against its Zod schema, so the hint always names an account the server has just confirmed. The
library's first-paint mirror reads the hint synchronously, before any request has resolved, to choose which cached
presentation to seed; a stale or attacker-writable `localStorage` value can therefore misdirect which cached
presentation a page seeds on first paint, never which account a request authenticates as, because the hint is never
consulted by the session or credential resolution itself.

Operationally, this subject introduces no configuration surface of its own beyond `behind_https` (owned by the
configuration-loading subject). The sweep cadence is fixed at one hour and is not a setting: a single-instance
deployment has no need to tune it.

## More information

- [CodeGuard deviation register](../../../security/codeguard/README.md), deviations 1 to 3: the recorded rationale and
  compensating controls for the `Secure`, `SameSite`, and 24-hour-expiry choices above.
- [Content Security Policy reference](../../../security/content-security-policy.md), "Cookies": the session cookie's
  wire name, path, and observed `Max-Age`, as the canonical operator-facing reference.
