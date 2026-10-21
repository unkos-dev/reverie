---
type: DESIGN
profile-version: 1
id: "REV-DESIGN-0026"
title: "OIDC sign-in"
satisfies:
  - "REV-REQ-0037"
  - "REV-REQ-0067"
  - "REV-REQ-0068"
  - "REV-REQ-0069"
  - "REV-REQ-0070"
governed-by:
  - "REV-ADR-0029"
---

# OIDC sign-in

This Design covers interactive sign-in through an operator-configured OpenID Connect provider: the authorization-code
flow with PKCE that starts at `GET /auth/oidc/login` and finishes at `GET /auth/callback`, the validation the callback
applies, the resolution of the verified identity to a Reverie account, first-login provisioning, and the refusals that
end a sign-in without establishing a new authenticated session.

## Purpose and boundaries

This subject owns the two OIDC handlers `oidc_login` and `callback` in `backend/src/routes/auth.rs`, the account
resolution and provisioning function `upsert_from_oidc` with its error type `OidcProvisionError` and the email guard
`validated_oidc_email` in `backend/src/models/user.rs`, and the identity-link queries in
`backend/src/models/user_identities.rs`. It owns OIDC initiation and consumption through the shared sign-in model
(`backend/src/models/sign_in.rs`), including the pending attempt's state digest, PKCE verifier, nonce and expiry. On the
client it owns the fixed outcome notice and "Sign in with OIDC" action in `frontend/src/routes/auth-login.tsx`. The
action appears when `GET /auth/setup/status` reports `oidc_enabled` and navigates the browser to `/auth/oidc/login`.

It does not own the provider client, discovery, endpoint policy or the HTTP client that performs the token exchange, the
Design "OIDC outbound transport" (REV-DESIGN-0025). It does not own the session a successful sign-in produces:
`auth::session::login`, the cookie policy and the idle expiry belong to the Design "Sessions" (REV-DESIGN-0011). It does
not own how a later request resolves to a caller, the Design "Request authentication" (REV-DESIGN-0010). It does not own
email-and-password sign-in, the Design "Local password sign-in" (REV-DESIGN-0012), or the first-administrator bootstrap.
It does not own minting or validating the CSRF synchronizer token, the Design "CSRF protection" (REV-DESIGN-0008), or
the theme cookie; the callback only calls them, and this Design names the call sites.

Depends on: the configured provider's authorization, token and signing-key endpoints through the runtime held in
`AppState.oidc`; the `users` and `user_identities` tables; and the session layer. Depended on by: the sign-in page.

## Structure

- `oidc_login` checks session identity, source admission and cohort state, saves the anonymous session, persists the
  authorization attempt and redirects the browser to the provider.
- `callback` consumes the attempt before provider exchange, verifies the token and claims the cohort's single win. Only
  a winner calls `upsert_from_oidc`, refuses a disabled account and calls `auth::session::login`, then inserts the CSRF
  token and explicitly saves before seeding the theme cookie. Provider errors consume state without exchange.
- `upsert_from_oidc` runs one transaction that serialises on the identity, resolves the link, and either refreshes a
  returning account's display name or creates an account and its link. `validated_oidc_email` shapes the email claim for
  the create branch only.
- `user_identities` holds one row per linked external identity: `user_id`, `provider` (`oidc`), `issuer` and `subject`,
  with `UNIQUE (issuer, subject)`. Deleting a `users` row deletes its links (`ON DELETE CASCADE`).

```text
browser              Reverie                                provider
   |  GET /auth/oidc/login  |                                    |
   |----------------------->|  save session; state digest, verifier, nonce -> attempt
   |<-- 307 authorization URL                                    |
   |------------------------------------------------------------>|  authenticate
   |<-- 307 /auth/callback?code&state ---------------------------|
   |  GET /auth/callback    |                                    |
   |----------------------->|  claim attempt, exchange code     |
   |                        |----------------------------------->|
   |                        |<-- tokens (ID token) --------------|
   |                        |  verify ID token and nonce, claim win, resolve or provision,
   |                        |  refuse disabled, login, CSRF token, theme cookie
   |<-- 307 /  (session cookie)                                  |
```

## Interfaces and dependencies

| Interface | Contract |
| --------- | -------- |
| `GET /auth/oidc/login` | 307 to provider; 303 for signed-in, throttled, unavailable or inactive starts; 404 when OIDC is not configured. |
| `GET /auth/callback` | Code/state or provider error; 307 on success, 303 for signed-in or fixed failure outcomes; 404 when OIDC is not configured. |
| `GET /auth/setup/status` | Reports `oidc_enabled`, true when `OIDC_ISSUER_URL` is non-blank; the client reads it. |

The generated [OpenAPI document](../../../../backend/openapi.json) is the canonical request and response definition for
both routes. OIDC is optional: with no `OIDC_ISSUER_URL`, `AppState.oidc` is `None` and both routes answer 404. When it
is set, `OIDC_CLIENT_ID`, `OIDC_CLIENT_SECRET` (or its file variant) and `OIDC_REDIRECT_URI` are required with it. The
redirect URI registered at the provider is `/auth/callback` on the public origin. Configuration loading rejects a
redirect URI whose path is not exactly `/auth/callback`, including a trailing slash, before provider discovery.

The authorization request asks for the `email` and `profile` scopes on top of the library's default `openid`, with a
random state, a random nonce and a SHA-256 PKCE challenge. The callback consumes the ID-token claims `iss`, `sub`,
`name` and `email`. It does not read `email_verified`, does not call the UserInfo endpoint, and accepts no
caller-supplied return target: success always redirects to `/`.

`upsert_from_oidc(pool, issuer, subject, display_name, email)` returns the account or an `OidcProvisionError`, either
`EmailCollision` or `Db`.

## Data and state

`sign_in_cohorts` has one row per anonymous session, keyed by `session_id` with a foreign key to
`tower_sessions.session`. `won_at` is null while pending; the conditional `claim_win` update gives one caller the single
authority to establish an account. Both local and OIDC sign-in use it. The cohort is never reopened.

`oidc_sign_in_attempts` has at most one row per cohort, keyed by `session_id` and cascading from the cohort. It stores a
32-byte SHA-256 state digest, the PKCE verifier and nonce as text, creation time and a fixed 15-minute expiry. A new
start replaces the attempt. `claim_oidc_attempt` atomically deletes matching session-and-state material whose expiry is
in the future, returning the verifier and nonce. The delete commits before provider I/O, including on an exchange that
fails. Raw state is not stored.

The model owns every cohort and attempt query. Start joins or creates the cohort and replaces the attempt; the callback
consumes the attempt and conditionally claims the win. The session worker deletes expired attempts in batches of 1,000.
Rotation, logout, invalid-session flushing and session expiry sweeping delete the parent session, cascading its cohort
and attempt. OIDC start's only session-map write is `sign_in_started = true`, saved before attempt insertion so the
foreign key has a parent and the middleware keeps the anonymous cookie. The marker has no decision-making role.

An account is identified by `(issuer, subject)`, where the issuer is the verified `iss` claim and not the configured
URL. `users.oidc_subject` is an unused nullable column that this subject never writes and never uses for identity. Email
is never an identity key.

On first sign-in, `upsert_from_oidc` creates a `users` row with the column defaults (role `adult`, not a child, session
version 0, no promotion) and the link row. The display name is the `name` claim in its default locale, falling back to
the subject when the claim is absent. The trimmed `email` claim passes the shared account-email validator, which applies
addr-spec syntax and length bounds alongside the frontend's common ASCII address pattern. An absent, blank or
unsupported claim stores `NULL`. `idx_users_email_lower`, a unique index over `lower(email)` for non-null emails, makes
an email unique across accounts without regard to case.

On a returning sign-in, the only write is the display name and `updated_at`, and only for an account that is not
disabled. The stored email is never rewritten, whatever the provider now releases.

## Runtime behaviour

**Initiation.** `oidc_login` applies these steps in order:

1. Require configured OIDC, else 404.
2. Call the shared `signed_in_user` rule. `SignedIn` redirects to `/` with 303. Stale, deleted and disabled identities
   are flushed and continue anonymously.
3. Apply the independent OIDC per-source limiter. An over-limit start redirects to `/login?outcome=throttled` with 303
   and writes no attempt.
4. Read the current provider snapshot. When discovery is pending, redirect to `/login?outcome=unavailable` with 303,
   without provider I/O, saving a session or writing a cohort/attempt. A ready start uses that snapshot's client.
5. Join a live anonymous session's cohort. `Won` redirects to `/login?outcome=expired` with 303 without changing the
   session or its attempt.
6. If an incoming session id was cleared during rehydration, rotate the cached record id with `cycle_id()`. A failure
   returns an internal error. Insert the marker and explicitly save the session.
7. Generate the PKCE pair, state and nonce, persist the attempt, and redirect to the provider with 307.

`state.oidc_limiter` has its own source allowance with the configured `login_rate_per_min` quota. Password/setup/
registration and recovery use separate governor instances; exhausting either leaves OIDC's allowance intact. The hourly
sweep uses governor's `retain_recent` to reclaim fully replenished keys. The callback uses its one-shot attempt
protection without spending password source allowance. The identity check precedes limiting and attempt lookup, so a
signed-in browser starts no provider work.

**Callback.** OIDC not configured answers 404. Otherwise `callback` applies these steps:

1. Read session identity. A signed-in browser goes 303 to `/` before query validation and without provider I/O.
2. Parse either code/state success or error with optional description/state. Reject mixed, duplicate, empty or
   incomplete parameters as `expired`. Discard provider descriptions without logging or display.
3. Consume the matching live attempt. Missing, mismatched, expired or used state goes to `expired` before provider I/O.
4. For a provider error, return `denied` only for `access_denied`, or `unavailable` for another error. No code is
   exchanged.
5. For a code response, require a ready snapshot, exchange once with the claimed PKCE verifier and verify the ID token
   and nonce. Apply the fixed failure table below. Refresh policy belongs to the outbound transport subject.
6. Claim the cohort's win. A loser goes to `expired` without provisioning or attaching identity.
7. Resolve or provision the account, refuse a disabled account, rotate the session and attach identity through
   `auth::session::login`. Insert the CSRF token and call `session.save()` explicitly. Every failed completion after the
   win attempts `session.flush()` before returning the mapped outcome. A failed flush logs a fixed cause.
8. Seed the theme cookie and answer 307 to `/` only after explicit save succeeds.

The runtime shares an in-flight fetch before checking its 30-second floor. The floor counts from every key-fetch
attempt's start, successful or failed. A suppressed caller verifies against the current snapshot, including after a
previous failure. An actual failed refresh redirects its waiting callback to `keys` and retains the last good set;
failed results are not reused by later callers. Ambiguous matches, bad signatures and other claim failures do not
trigger an unknown-key refresh. [OIDC outbound transport](0025-oidc-outbound-transport.md) owns fetching and
publication.

**Resolution inside `upsert_from_oidc`.** One transaction takes a per-identity advisory lock keyed on issuer and
subject, so two concurrent first sign-ins for one identity serialise and the second resolves to the row the first
created. It then looks the identity up in `user_identities`.

- A linked identity updates the display name and `updated_at` under `WHERE id = $1 AND disabled_at IS NULL`. A disabled
  account matches no row and is read back unchanged, so the refusal in step 7 happens with no write to the account.
- An unlinked identity inserts the `users` row, then the link, then reads the row back. A unique violation on
  `idx_users_email_lower` at the insert ends the transaction as `EmailCollision`; the rollback leaves no `users` row and
  no link.

A first sign-in that carries no usable email, or an email no other account holds, therefore provisions a fresh,
non-administrator account. Sign-in never attaches an identity to an existing account.

## Failure and recovery

Callback failures use 303 redirects to `/login?outcome=<outcome>`, except the 404 when OIDC is not configured and
accepted middleware save residual below. The login screen renders fixed copy and starts a fresh attempt only after
explicit activation.

| Condition | Outcome | State effect |
| --------- | ------- | ------------ |
| OIDC not configured | 404 | No attempt mutation |
| Already signed in at start or callback | 303 to `/` | No state validation or provider exchange |
| Start or valid callback while discovery is pending | `unavailable` | Start writes nothing; callback consumes its attempt |
| Start over source limit | `throttled` | No attempt written |
| Start finds a won anonymous cohort | `expired` | Session and attempt unchanged |
| Malformed/incomplete callback; absent, mismatched, expired or used state | `expired` | No exchange or account write |
| Provider `access_denied` with valid state | `denied` | Attempt consumed; no exchange |
| Other provider error with valid state | `unavailable` | Attempt consumed; no exchange |
| Provider error without valid state | `expired` | No exchange |
| Token endpoint `invalid_grant` | `expired` | Attempt consumed; no exchange retry |
| Other token error, transport/server/parse failure, missing ID token, invalid claims or signature | `unavailable` | Attempt consumed; no win |
| No matching key after permitted refresh, ambiguous key, failed key fetch | `keys` | Attempt consumed; no win |
| Runtime or callback storage failure | `unavailable` | No unauthorised account write or authenticated cookie |
| Valid callback loses arbitration | `expired` | No account write, session mutation or authenticated cookie from loser |
| Email collision or disabled account after winning | `account` | Same generic response; no forbidden account write; flush attempted |
| Provisioning database error after winning | `unavailable` | Account transaction rolls back; flush attempted |
| Rotation, identity insertion, CSRF insertion or explicit save failure | `unavailable` | Flush attempted; committed account writes remain |
| Middleware subsequent always-save failure | Empty 500 | Accepted residual after handler completion |

A consumed attempt requires a new explicit start. A successful post-win flush removes the session row and cookie,
cascading the cohort. A failed flush logs a warning; the residual cohort stays won until session expiry. An anonymous
newcomer cannot rotate or reopen that cohort. Account and identity writes committed before a later session failure are
not rolled back.

If old-session deletion fails during rotation, `cycle_id` has already cleared its current id. The later flush cannot
remove the original row, even if it returns success. The rotation error is logged at warning level as failed old-session
removal. The anonymous session row and its won cohort remain until idle expiry or the sweep. The row carries no identity
and the cohort is terminal, so it cannot authenticate or win again. The callback redirects to `unavailable` with its
cookie cleared; no explicit original-session cleanup occurs.

The callback explicitly saves after login and CSRF insertion, mapping that save's failure to `unavailable` with post-win
flush. `SessionManagerLayer` still performs its subsequent always-save. A store failure there replaces the handler's
response with an empty 500 outside the callback and its cleanup scope. No outer response boundary replaces that
middleware response.

A returning identity whose provider email changed or became invalid still signs in, and the stored email is unchanged.
Disabling an account refuses its next sign-in through the generic `account` redirect, and ends its live sessions through
the gates of the Design "Request authentication".

## Security and operations

The provider is trusted for the identity it asserts and for nothing else. Matching on `(issuer, subject)` and never on
email means a provider that releases an unverified or attacker-chosen address cannot reach an existing account: a first
sign-in whose email belongs to another account is refused rather than linked, and answers with the same redirect and
empty body as a disabled account so the response does not confirm that the address exists. There is no auto-linking and
no promotion: the first administrator exists only through bootstrap, and OIDC never produces a child account.

The callback consumes matching live state before any outbound request. PKCE binds the code to the browser's session, and
the nonce binds the ID token to this flow. Only the cohort winner reaches account provisioning. The session id is
rotated in `login` before any identity claim is attached. The email claim is validated by shape, and an invalid value is
logged by length only, never verbatim. Refusals log fixed outcome categories. Internal callback failures also log one
fixed static cause at warning level without formatting the underlying error; malformed-query rejection adds no cause
event. The request trace records the callback path without its query, and neither callback parsing nor token-error
handling formats provider descriptions or response bodies into logs.

A self-hoster registers `/auth/callback` on the public origin at the provider and sets `OIDC_REDIRECT_URI` to match. The
session cookie's `SameSite=Lax` policy is what lets the cross-site redirect back from the provider carry the session.
The flow has no linking step, so a refused collision stays refused until the other account no longer holds the email.
The fixed local outcomes distinguish recovery categories without exposing provider text or account details. The
middleware subsequent-save residual can still produce an empty 500.

## More information

- [Reference configuration](../../../../website/src/content/docs/reference/configuration.mdx): the generated `OIDC_*`
  field reference.

The client parses the outcome with Zod and never rewrites it or starts provider navigation on mount. The approved
[login screen](../../../design/screens/login.md) owns copy, layout, focus and rendered acceptance in both themes.
