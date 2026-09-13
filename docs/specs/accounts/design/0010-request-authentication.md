---
type: DESIGN
profile-version: 1
id: "REV-DESIGN-0010"
title: "Request authentication"
satisfies:
  - "REV-REQ-0029"
  - "REV-REQ-0030"
  - "REV-REQ-0031"
  - "REV-REQ-0032"
  - "REV-REQ-0033"
governed-by:
  - "REV-ADR-0028"
---

# Request authentication

This Design covers how an inbound request becomes a `CurrentUser`: the session-cookie, HTTP Basic, and Bearer resolution
legs `CurrentUser`'s `FromRequestParts` implementation tries in order; the device-token lookup Basic and Bearer share;
the Bearer leg's dispatch between a device token and an RFC 9068 resource-server JWT; the force-logout and
soft-disabled-account gates each leg enforces; and the `require_admin`, `require_not_child`, `require_scope`, and
`may_grant_scope` assertion methods every gated handler in the application calls once it holds a `CurrentUser`.

## Purpose and boundaries

This subject owns `backend/src/auth/middleware.rs` in full: the `CurrentUser` struct and its private `role`, `is_child`,
and `scopes` fields; the `FromRequestParts<AppState>` implementation that tries the session-cookie leg, then
`verify_basic`, then `verify_bearer`, returning on the first that resolves an identity; `resolve_device_token`, the
single indexed lookup Basic and the device-token half of Bearer both call; `resolve_jwt` and `resolve_jwt_scopes`, the
JWT half of Bearer and its role-ceiling clamp; and the four assertion methods. It also owns `BasicOnly`
(`backend/src/auth/basic_only.rs`), a thin wrapper that calls this subject's own `verify_basic` and substitutes an RFC
7617 challenge for the generic one, because that wrapper has no logic of its own beyond the challenge substitution.

It does not own: session persistence, the cookie policy, or the `SESSION_KEY_USER_ID` / `SESSION_KEY_SESSION_VERSION`
values this subject reads back, the Design "Sessions", which this subject depends on for the `tower_sessions::Session`
extractor and the two session keys. It does not own local-password verification, breach-checking, or per-account
throttling, the Design "Local password sign-in", whose successful outcome is what reaches the session-cookie leg's write
side (`auth::session::login`, in "Sessions") for a `POST /auth/local/login` caller. It does not own JWT signature,
issuer, audience, or JWKS validation (`backend/src/auth/jwt.rs`) or interactive OIDC login and account provisioning
(`backend/src/auth/oidc.rs`), the JWT validation subject and the OIDC login subject; this subject only calls
`JwtValidator::validate` once a Bearer credential has been classified as non-device-token, and reads the
`user_identities` row that OIDC login alone writes. It does not own the scope hierarchy, the role-to-scope ceiling, the
deny-by-default matrix, or the RLS-vs-handler-predicate ownership map, the Design "Authorization axes". This subject
only carries the `scopes` field the assertion methods read and derives or clamps it on each of the three resolution
legs. It does not own device-token minting, the per-user cap, or revocation (`backend/src/models/device_token.rs`,
`backend/src/routes/tokens.rs`), the device-token subject, or CSRF synchronizer-token validation
(`backend/src/security/csrf.rs`), the Design "CSRF protection".

Depends on: the `users` row's `role`, `is_child`, `session_version`, and `disabled_at` columns
(`crate::models::user::find_by_id`, `find_by_oidc_identity`); the `device_tokens` row's `scopes`, `token_hash`, and
expiry/revocation state (`crate::models::device_token::find_by_id`); the `SESSION_KEY_USER_ID` and
`SESSION_KEY_SESSION_VERSION` session keys the Design "Sessions" writes at login; `AppState::jwt_validator`, which gates
whether the JWT half of Bearer is even attempted; and `crate::auth::token::TOKEN_PREFIX` and `verify_device_token`, the
credential-format constant and constant-time hash comparison REV-ADR-0028 fixed for both transports.

Depended on by: every handler that declares `current_user: CurrentUser` as a parameter. There is no router-level
authentication layer, so a handler opts in by naming the extractor, and axum resolves it before the handler body runs.
Also depended on by: `BasicOnly`, and through it the OPDS-protocol handlers in `backend/src/routes/opds/download.rs`,
`covers.rs`, `root.rs`, `library.rs`, `shelves.rs`, and `opensearch.rs` (`covers.rs` also registers a separate,
always-mounted `/api/v1` cover route gated by `CurrentUser` directly, for the web UI, which this subject reaches the
ordinary way); `backend/src/routes/tokens.rs::create_token`, which reads `may_grant_scope` to bound a requested token's
scopes to the caller's role; and `backend/src/authz_matrix.rs`, which mints scoped tokens and drives this subject's
resolution path across every `/api/v1` operation to prove the scope gate behind each one.

## Structure

- `CurrentUser` (`backend/src/auth/middleware.rs`) holds `user_id` public and `role`, `is_child`, `scopes` private,
  reachable only through the four assertion methods. The fields are private specifically so a handler cannot match on
  `role` or scan `scopes` directly and bypass the single enforcement point; the "Authorization axes" Design covers what
  those methods enforce, this Design covers how the fields they read are populated.
- `impl FromRequestParts<AppState> for CurrentUser` is the one entry point every extraction goes through. It tries the
  session-cookie leg first: reads `Session::from_request_parts`, and if a `user_id` claim is present, loads the user
  row, checks `disabled_at`, then compares the session's stored `session_version` claim against the live row's. A
  disabled account returns `AppError::Unauthorized` immediately, flushing the session first and never falling through to
  Basic or Bearer on this request. A version mismatch or a deleted user instead flushes the session and falls through,
  the function has no `return` on that branch, so the same request gets a chance to authenticate via Basic or Bearer if
  either header is also present; each of those legs independently re-checks `disabled_at`, so this fallthrough does not
  bypass the gate, it only gives a stale session's request a second credential to try. When no session claims an
  identity at all, the leg is skipped with no database read. Only after the session leg neither returns nor is
  applicable does the function try `verify_basic`, then `verify_bearer`, returning on the first `Some`; if neither
  resolves anything, it returns `AppError::Unauthorized`.
- `resolve_device_token(state, prefixed_id, secret)` is the function `verify_basic` and the device-token half of
  `verify_bearer` both call, resolving the `{prefix}{token_id}` / `{prefix}{token_id}.{secret}` credential format
  REV-ADR-0028 fixed for both transports. It does one indexed `device_token::find_by_id` lookup by the token's own row
  id, not a scan over the claimed user's other tokens; the query predicate of that lookup excludes revoked and expired
  rows, so an id belonging to a revoked or expired token resolves the same way as an unknown id, no row at all. The
  function looks up the owning user separately, rejects a soft-disabled owner before comparing the secret, compares the
  secret in constant time (`subtle::ConstantTimeEq`, inside `token::verify_device_token`), and rejects an empty `scopes`
  array as a defence-in-depth floor check (mint already refuses to create one). On success it schedules a
  fire-and-forget `device_token::update_last_used` write via `tokio::spawn`, logging a failure without propagating it.
- `verify_basic(state, parts)` reads the `Authorization: Basic` header, base64-decodes it, splits on the first `:`, and
  hands the two halves to `resolve_device_token`. It returns `Ok(None)` whenever there is no usable Basic credential to
  try, the header is absent, not valid UTF-8, or does not start with `Basic `, so the caller can try the next leg
  without treating any of those as a rejection.
- `verify_bearer(state, parts)` reads `Authorization: Bearer`, and dispatches on whether the credential starts with
  `token::TOKEN_PREFIX`: a match splits on the first `.` and calls `resolve_device_token` with the same shared function
  Basic uses; anything else is handed to `resolve_jwt`. Like `verify_basic`, an absent header returns `Ok(None)`.
- `resolve_jwt(state, token)` returns `Ok(None)` outright when `state.jwt_validator` is `None`, the resource-server
  feature is inert, so a non-device-token Bearer value is indistinguishable from no credential at all rather than a
  rejection. When a validator is configured, it calls `JwtValidator::validate` (owned by the JWT validation subject),
  looks the resulting `(iss, sub)` up read-only via `user::find_by_oidc_identity`, an unlinked identity is rejected,
  never provisioned; only the OIDC callback provisions a `user_identities` row, rejects a disabled owner, and resolves
  the effective scope set via `resolve_jwt_scopes`.
- `resolve_jwt_scopes(claimed, role)` intersects the JWT `scope` claim against `Scope::for_role(role)` (the
  "Authorization axes" ceiling): an absent claim carries the full ceiling (parity with how a session derives its
  scopes); unknown literals in a present claim are dropped, never treated as an error; and a claim that intersects to
  nothing the role permits returns `None`, which `resolve_jwt` maps to a rejected credential rather than silently
  substituting the full role-derived set.
- The four assertion methods (`require_admin`, `require_not_child`, `require_scope`, `may_grant_scope`) are plain
  comparisons against the private fields: `require_scope` does a floor check against `Scope`'s derived `Ord`
  (`self.scopes.iter().any(|held| *held >= needed)`) and logs a `tracing::warn!` naming the caller and the scope needed
  on rejection; the other three are direct field comparisons with no logging of their own.
- `BasicOnly` (`backend/src/auth/basic_only.rs`) wraps a `CurrentUser` resolved exclusively via `verify_basic`: it calls
  that function directly, and maps every rejection (`Ok(None)`, `AppError::Unauthorized`, or
  `AppError::InvalidCredential`) to `AppError::BasicAuthRequired`, so a caller that speaks only Basic (the OPDS readers
  this extractor serves) always gets the RFC 7617 challenge rather than the generic Bearer one `InvalidCredential`'s own
  response would otherwise carry.

## Interfaces and dependencies

- `CurrentUser: FromRequestParts<AppState>` is the interface every gated handler uses, naming `CurrentUser` (or
  `BasicOnly`, for the OPDS-only variant) as a parameter's type, for example `current_user: CurrentUser`. Nothing wraps
  routes in an authentication middleware layer; the router composed in `backend/src/lib.rs` attaches no auth-specific
  layer, so an unguarded handler (one that declares neither extractor) is reachable by anyone, the deny-by-default
  guarantee the "Authorization axes" Design's `every_api_v1_op_declares_a_scope` test checks is a property of the
  generated OpenAPI spec, not of this subject's own wiring.
- `pub async fn verify_basic` and `pub async fn verify_bearer` are the two functions this subject exposes beyond the
  extractor itself, so a caller that needs one transport without the cookie-first precedence (`BasicOnly`) can reach it
  directly rather than re-implementing header parsing.
- `AppState::pool` is read on every leg (user and device-token lookups);
  `AppState::jwt_validator: Option<Arc<JwtValidator>>` is read once, at the top of `resolve_jwt`, and its presence is
  what turns the JWT half of Bearer on or off.
- `tower_sessions::Session` (via `Session::from_request_parts`) and the `SESSION_KEY_USER_ID` /
  `SESSION_KEY_SESSION_VERSION` constants are the read side of a contract the Design "Sessions" owns the write side of
  (`auth::session::login`); this subject never writes either key, only reads them back and calls `session.flush()` to
  tear a rejected session down.
- `crate::error::AppError` (`backend/src/error/mod.rs`) is the shared error type every leg returns through, but not the
  sole production source of `AppError::Unauthorized`: `routes/auth.rs`'s `/auth/callback` handler returns it for its own
  OIDC anti-forgery, PKCE, and nonce checks and for a disabled account caught after upsert, and both `routes/auth.rs`'s
  `me` handler and `routes/users/mod.rs`'s `change_own_password` return it when the session's `user_id` no longer
  resolves to a row, none of these share this subject's resolution path. This subject is the only production source of
  `AppError::InvalidCredential` (a credential presented and rejected; `BasicOnly` re-maps both variants into
  `AppError::BasicAuthRequired`). That module's `IntoResponse` impl is what attaches the `WWW-Authenticate` challenge
  each variant carries; this subject only chooses which variant to return.

## Data and state

`CurrentUser` itself holds no state beyond the lifetime of one request: it is constructed fresh by the extractor on
every extraction, from a database read taken at that moment, and is dropped with the request. The two mutations this
subject's resolution path can trigger are both delegated to a neighbouring subject's storage, not owned here:

- `session.flush()`, called when the session leg finds a disabled account or a stale `session_version`, deletes the
  session row, the write authority for `tower_sessions.session` belongs to the Design "Sessions".
- The `tokio::spawn`ed `device_token::update_last_used` call inside `resolve_device_token`, the write authority for
  `device_tokens.last_used_at` belongs to the neighbouring device-token model.

`session_version` and `disabled_at`, the two `users` columns this subject's session leg compares and every leg's
disabled check reads, are read-only from this subject's side; every writer found is elsewhere, administrative or
self-service:

| Column bumped/set | Handler (file) |
| ----------------- | -------------- |
| `session_version` | `update_role` (`routes/users/mod.rs`) |
| `session_version` | `update_child_status` (`routes/users/mod.rs`) |
| `session_version`, `disabled_at` | `update_account_status`/`disable_account` (same file, + `models/user.rs`) |
| `session_version` | `admin_reset_password` (`routes/users/mod.rs`) |
| `session_version` | `reset_password`, recovery PIN (`routes/auth.rs`) |
| `session_version` | `change_own_password` (`routes/users/mod.rs`) |
| `disabled_at` cleared only | `update_account_status` → `enable_account` (`routes/users/mod.rs`, `models/user.rs`) |

`disable_account` bumps `session_version` in the same statement that sets `disabled_at`
(`backend/src/models/user.rs::disable_account`); `enable_account` only clears `disabled_at` and never bumps
`session_version` back down or forward. A session that predates the disable stored the pre-bump version, so the version
comparison in `CurrentUser::from_request_parts` never matches again once that bump has happened, re-enable or not: the
the `session_version` increment the disable performs, not the `disabled_at` flag, is what forces the logout, and it is
permanent. `enable_account`'s job is only to lift the `disabled_at` gate for a fresh sign-in; it cannot and does not
restore a pre-disable session's validity, so no live session skips re-authentication across a disable/enable cycle.

## Runtime behaviour

**A session-authenticated request with no `Authorization` header**, the common browser case:

1. `Session::from_request_parts` returns a session carrying `SESSION_KEY_USER_ID`.
2. `user::find_by_id` loads the row. `disabled_at` is `None`, so the disabled check does not fire.
3. The stored `SESSION_KEY_SESSION_VERSION` claim equals `user.session_version`, so the function returns
   `Ok(CurrentUser { user_id, role: user.role, is_child: user.is_child, scopes: Scope::for_role(user.role).to_vec() })`
   without ever evaluating `verify_basic` or `verify_bearer`.

**Force-logout after a role change**, continuing from an already-authenticated session:

1. An admin calls `update_role`, which bumps the target's `session_version` in the same transaction as the role write.
2. The target's browser, still holding its old session cookie, makes another request. The session leg loads the (now
   different-role) user row; `disabled_at` is still `None`.
3. The stored claim (captured at the earlier login) no longer equals the live `session_version`, so neither success
   branch is taken. `session.flush()` runs, deleting the session row, and the function falls through, no `Authorization`
   header is present on this request, so `verify_basic` and `verify_bearer` both return `Ok(None)`, and the extractor
   returns `AppError::Unauthorized`. The target must sign in again to obtain a session carrying the new
   `session_version`.

**A soft-disabled account presenting a device token over Basic**, an OPDS reader whose owning account an administrator
has just disabled:

1. `verify_basic` decodes the header and calls `resolve_device_token` with the parsed id and secret (no session cookie
   is in play for a Basic-only reader app).
2. `device_token::find_by_id` finds the active (non-revoked, non-expired) row, and a separate `user::find_by_id` call
   looks up its owner.
3. `u.disabled_at.is_some()` is now true, so the function returns `AppError::InvalidCredential` before the secret is
   ever compared, the reader's stored credential is inert regardless of whether it still types the correct secret.

**A Bearer credential with a resource-server JWT validator configured**, dispatching between the two Bearer shapes:

1. `verify_bearer` reads the credential after `Bearer `. If it starts with `token::TOKEN_PREFIX`, it is routed to the
   same `resolve_device_token` the Basic leg uses, regardless of whether a `JwtValidator` is configured, the
   device-token path is never attempted as a JWT.
2. Anything else is handed to `resolve_jwt`. If no validator is configured, `resolve_jwt` returns `Ok(None)`
   immediately, and `verify_bearer` in turn returns `Ok(None)`, the extractor's final fallback then returns the same
   `AppError::Unauthorized` (bare `Bearer` challenge) it would for a request with no `Authorization` header at all,
   never `InvalidCredential`, because the feature that would reject it outright is not even active.
3. With a validator configured, `JwtValidator::validate` checks the token's signature, expiry, issuer, and audience
   (owned by the JWT validation subject). A valid token's `(iss, sub)` is looked up via `find_by_oidc_identity`; an
   identity with no linked `user_identities` row is rejected, the lookup never creates one.
4. `resolve_jwt_scopes` clamps the token's `scope` claim to the caller's current role ceiling. A claim of `["read"]` on
   an admin-role caller yields `scopes: [Read]`, so a `require_scope(Scope::Write)` check on this request fails even
   though `require_admin()` would pass; the claimed scope, not the role, decides a mutation's fate for this credential.

## Failure and recovery

- **No credential presented at all.** `AppError::Unauthorized`, `401`, with a bare `WWW-Authenticate: Bearer` challenge
  (RFC 6750 §3). This is the terminal case of the `FromRequestParts` impl when every leg returns `Ok(None)`, and also
  what a disabled-Bearer-feature request lands on (see the Bearer walkthrough above).
- **A credential was presented and rejected.** `AppError::InvalidCredential`, `401`, identical body to `Unauthorized`
  but with `WWW-Authenticate: Bearer error="invalid_token"` (RFC 6750 §3.1), so a client can tell "no credential yet"
  from "that credential doesn't work" without parsing the response body. Every rejection branch inside
  `resolve_device_token` and `resolve_jwt`, unknown id, wrong secret, disabled owner, empty or fully-narrowed scopes,
  malformed header shape, collapses to this one variant by construction: the variant itself carries no payload, so no
  rejection branch has a detail to attach even if one wanted to, and the response gives an attacker no oracle; only the
  two error variants' distinct challenge headers are observable.
- **A malformed `Authorization` header.** A non-UTF8 or non-base64 Basic value, a Basic value with no `:` separator, or
  a Bearer value that starts with `TOKEN_PREFIX` but has no `.` separator, all map to `InvalidCredential` at the parsing
  step, before any database lookup.
- **`BasicOnly`'s own remapping.** Every rejection `verify_basic` can produce (`Ok(None)`, `Unauthorized`,
  `InvalidCredential`) is caught and replaced with `AppError::BasicAuthRequired { realm }`, which emits the RFC 7617
  challenge instead, the generic `Bearer` challenge `InvalidCredential` carries would otherwise reach an OPDS client
  that only prompts on a `Basic` challenge. Any other error variant (a database failure, mapped to `Internal`) passes
  through unchanged.
- **A database error on any lookup.** Every `sqlx::Error` from a user, device-token, or identity lookup is wrapped into
  `AppError::Internal`, which logs the cause at `tracing::error!` and returns a fixed, non-leaking `500` detail; no
  lookup failure here is treated as "no credential."
- **A session-store flush failure.** Both flush sites (disabled account, stale/deleted-user session) log a
  `tracing::warn!` on failure and proceed regardless, the disabled branch still returns `Unauthorized`, and the
  stale-version branch still falls through to Basic/Bearer, because the flush is a best-effort cleanup of a session row
  that has already failed to authenticate on this request either way; a session that fails to flush is caught again on
  its next presentation by the same `disabled_at` or `session_version` comparison.

## Security and operations

Every leg checks `disabled_at` independently, against the live `users` row, rather than inheriting a rejection from
another leg: the session leg's own check does not run again inside `resolve_device_token` or `resolve_jwt`, but both of
those carry the identical check (each with its own THREAT-annotated comment and its own regression test), so a disabled
account is rejected whichever of the three legs a given request happens to reach, not only the one that was tried first.
The session leg's disabled check runs before the `session_version` comparison and returns immediately without falling
through to Basic or Bearer on that same request; the `session_version` mismatch and deleted-user branches fall through
instead, but that fallthrough cannot re-admit a disabled account, because whichever of Basic or Bearer the request then
tries re-checks `disabled_at` on its own against the current row.

Every valid credential clears a `read`-scoped gate, the hierarchy's floor (owned as a value by "Authorization axes";
enforced here at the point of resolution): `require_scope`'s check is `any(|held| *held >= needed)`, so a credential
whose `scopes` holds only `Write` or `Admin` already satisfies a `Read` requirement under `Scope`'s derived `Ord`,
without `Read` itself appearing in the array. A session always derives the full role-ceiling set, so it can never narrow
to empty. A device token's scopes are mint-time fixed and non-empty, `create_token` rejects an empty list, not
specifically an omitted `read`, and `resolve_device_token` repeats the empty check as defence in depth against a row
written some other way (legacy data, a direct database write) reaching authentication regardless of the mint-time guard.
The scope a JWT claims is clamped by `resolve_jwt_scopes`, which rejects outright, returns `None`, mapped to
`InvalidCredential`, rather than silently substituting the full role-derived set, whenever the claim narrows to nothing
the caller's role permits; an absent claim, by contrast, is read as "the issuer delegated everything the role allows"
and carries the full ceiling.

The device-token secret comparison (`token::verify_device_token`, called from `resolve_device_token`) is constant-time
(`subtle::ConstantTimeEq`), so the stored hash cannot leak through response timing. `find_by_id` addresses a token row
directly by primary key, so no scan of the claimed user's other tokens exists whose position a timing difference could
reveal; the constant-time comparison's remaining job is solely to protect the digest itself, not a scan position.

`role`, `is_child`, and `scopes` on `CurrentUser` are private fields; every read outside this module goes through
`require_admin`, `require_not_child`, `require_scope`, or `may_grant_scope`. This is a structural guarantee, not a
convention: a handler cannot compile code that matches on `self.role` directly, so the "Authorization axes" enforcement
model this subject feeds cannot be bypassed by a handler reaching around the assertion methods.

## More information

- [Reverie security guidance](../../../security/claude-security-guidance.md), "OIDC & sessions": records the same
  `require_admin()` / `require_not_child()` single-enforcement-point rule as a reviewer-facing checklist entry, and
  notes that Bearer access tokens are validated by `JwtValidator` under REV-ADR-0028.
