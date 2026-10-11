---
type: DESIGN
profile-version: 1
id: "REV-DESIGN-0026"
title: "OIDC sign-in"
satisfies:
  - "REV-REQ-0032"
  - "REV-REQ-0037"
governed-by:
  - "REV-ADR-0029"
---

# OIDC sign-in

This Design covers interactive sign-in through an operator-configured OpenID Connect provider: the authorization-code
flow with PKCE that starts at `GET /auth/oidc/login` and finishes at `GET /auth/callback`, the validation the callback
applies, the resolution of the verified identity to a Reverie account, first-login provisioning, and the refusals that
end a sign-in without a session.

## Purpose and boundaries

This subject owns the two OIDC handlers `oidc_login` and `callback` in `backend/src/routes/auth.rs`, the account
resolution and provisioning function `upsert_from_oidc` with its error type `OidcProvisionError` and the email guard
`validated_oidc_email` in `backend/src/models/user.rs`, and the identity-link queries in
`backend/src/models/user_identities.rs`. It owns the three transient session keys the flow writes and clears:
`pkce_verifier`, `oidc_csrf_state` and `nonce`. On the client it owns only the "Sign in with OIDC" action in
`frontend/src/routes/auth-login.tsx` and the provider-aware redirect in `frontend/src/App.tsx`, both of which navigate
the browser to `/auth/oidc/login` when `GET /auth/setup/status` reports `oidc_enabled`.

It does not own the provider client, discovery, endpoint policy or the HTTP client that performs the token exchange, the
Design "OIDC outbound transport" (REV-DESIGN-0025). It does not own the session a successful sign-in produces:
`auth::session::login`, the cookie policy and the idle expiry belong to the Design "Sessions" (REV-DESIGN-0011). It does
not own how a later request resolves to a caller, including the read-only lookup of a bearer token's `(issuer, subject)`
against the links this subject writes, the Design "Request authentication" (REV-DESIGN-0010), or resource-server token
validation (`backend/src/auth/jwt.rs`). It does not own email-and-password sign-in, the Design "Local password sign-in"
(REV-DESIGN-0012), the first-administrator bootstrap, or the validation of the CSRF synchronizer token, the Design "CSRF
protection" (REV-DESIGN-0008).

Depends on: the configured provider's authorization, token and signing-key endpoints through the runtime held in
`AppState.oidc`; the `users` and `user_identities` tables; and the session layer. Depended on by: the sign-in page, and
by Bearer resolution, which can resolve only the identities this subject has linked.

ADR-0029 fixes OIDC as a co-equal sign-in mode that resolves to the same `users` row and session as local sign-in, with
accounts matched on issuer and subject only; this Design is the mechanism that realises that decision on the OIDC side.

## Structure

- `oidc_login` builds the authorization request from the discovered client, stores the flow state in the session and
  redirects the browser to the provider.
- `callback` runs the validation chain, calls `upsert_from_oidc`, refuses a disabled account, then hands the resolved
  user to `auth::session::login` and finishes with the CSRF token and the theme cookie.
- `upsert_from_oidc` runs one transaction that serialises on the identity, resolves the link, and either refreshes a
  returning account's display name or creates an account and its link. `validated_oidc_email` shapes the email claim for
  the create branch only.
- `user_identities` holds one row per linked external identity: `user_id`, `provider` (`oidc`), `issuer` and `subject`,
  with `UNIQUE (issuer, subject)`. Deleting a `users` row deletes its links (`ON DELETE CASCADE`).

```text
browser              Reverie                                provider
   |  GET /auth/oidc/login  |                                    |
   |----------------------->|  state, PKCE verifier, nonce -> session
   |<-- 307 authorization URL                                    |
   |------------------------------------------------------------>|  authenticate
   |<-- 307 /auth/callback?code&state ---------------------------|
   |  GET /auth/callback    |                                    |
   |----------------------->|  validate state, exchange code     |
   |                        |----------------------------------->|
   |                        |<-- tokens (ID token) --------------|
   |                        |  verify ID token and nonce, resolve or provision,
   |                        |  refuse disabled, login, CSRF token, theme cookie
   |<-- 307 /  (session cookie)                                  |
```

## Interfaces and dependencies

| Interface                | Contract                                                                               |
| ------------------------ | -------------------------------------------------------------------------------------- |
| `GET /auth/oidc/login`   | Unauthenticated. 307 to the provider, or 404 when OIDC is not configured.              |
| `GET /auth/callback`     | Unauthenticated. Query `code` and `state`, both required. 307 to `/` on success.       |
| `GET /auth/setup/status` | Reports `oidc_enabled`, true when `OIDC_ISSUER_URL` is non-blank; the client reads it. |

The generated [OpenAPI document](../../../../backend/openapi.json) is the canonical request and response definition for
both routes. OIDC is optional: with no `OIDC_ISSUER_URL`, `AppState.oidc` is `None` and both routes answer 404. When it
is set, `OIDC_CLIENT_ID`, `OIDC_CLIENT_SECRET` (or its file variant) and `OIDC_REDIRECT_URI` are required with it. The
redirect URI registered at the provider is `/auth/callback` on the public origin.

The authorization request asks for the `email` and `profile` scopes on top of the library's default `openid`, with a
random state, a random nonce and a SHA-256 PKCE challenge. The callback consumes the ID-token claims `iss`, `sub`,
`name` and `email`. It does not read `email_verified`, does not call the UserInfo endpoint, and accepts no
caller-supplied return target: success always redirects to `/`.

`upsert_from_oidc(pool, issuer, subject, display_name, email)` returns the account or an `OidcProvisionError`, either
`EmailCollision` or `Db`.

## Data and state

The flow state lives in the anonymous session row that `oidc_login` creates, under three keys: `oidc_csrf_state` (the
state the provider echoes back), `pkce_verifier` and `nonce`. They are distinct from the long-lived `csrf_token`
synchronizer key, so a transient value never overwrites it. A second `GET /auth/oidc/login` in the same session
overwrites all three. A successful callback removes them; no failing path does, so they remain until a later login
overwrites them or the session expires.

An account is identified by `(issuer, subject)`, where the issuer is the verified `iss` claim and not the configured
URL. `users.oidc_subject` is a legacy nullable column that this subject neither reads nor writes. Email is never an
identity key.

On first sign-in, `upsert_from_oidc` creates a `users` row with the column defaults (role `adult`, not a child, session
version 0, no promotion) and the link row. The display name is the `name` claim in its default locale, falling back to
the subject when the claim is absent. The email is the trimmed `email` claim when it parses as an RFC 5322 addr-spec
with display-name and domain-literal forms disallowed; an absent, blank or invalid claim stores `NULL`.
`idx_users_email_lower`, a unique index over `lower(email)` for non-null emails, makes an email unique across accounts
without regard to case.

On a returning sign-in, the only write is the display name and `updated_at`, and only for an account that is not
disabled. The stored email is never rewritten, whatever the provider now releases.

## Runtime behaviour

**Initiation.** `oidc_login` returns 404 when OIDC is not configured. Otherwise it generates the PKCE pair, state and
nonce, writes the three session keys, and answers 307 to the provider's authorization URL.

**Callback.** `callback` applies these steps in order and stops at the first that fails:

1. Parse `code` and `state` from the query. A missing or malformed parameter is a 400.
2. Require a configured OIDC runtime, else 404.
3. Require a stored `oidc_csrf_state` that equals the echoed `state`, then a stored `pkce_verifier` and `nonce`. Any
   absence or mismatch is a 401, and nothing is sent to the provider.
4. Exchange the code for tokens at the provider's token endpoint with the stored verifier, through the shared transport.
5. Require an ID token and verify it against the discovered client: signature, issuer, audience, expiry and the stored
   nonce.
6. Call `upsert_from_oidc` with the verified `iss`, `sub`, display name and email claim.
7. Refuse the account when `disabled_at` is set.
8. Call `auth::session::login`, which rotates the session id before it attaches `user_id` and `session_version`.
9. Remove the three transient keys, logging a warning for each removal that fails and continuing.
10. Mint a 32-byte random `csrf_token` into the session, seed the theme cookie from the account's preference, and answer
    307 to `/`.

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

| Condition                                                       | Outcome | Written                         |
| --------------------------------------------------------------- | ------- | ------------------------------- |
| OIDC not configured                                             | 404     | Nothing                         |
| `code` or `state` missing or malformed                          | 400     | Nothing                         |
| Stored state, verifier or nonce absent; echoed state differs    | 401     | Nothing                         |
| Token exchange fails, ID token absent, or ID token fails checks | 500     | Nothing                         |
| First sign-in email held by a different account                 | 401     | Nothing; warning logged         |
| Account disabled                                                | 401     | Nothing                         |
| Database error during resolution or provisioning                | 500     | Nothing; transaction rolls back |
| Session rotation or `csrf_token` write fails                    | 500     | Session store only              |

A provider that redirects back with an error and no `code` lands in the 400 row. The 401 refusals share one body, with a
`WWW-Authenticate: Bearer` challenge, and reveal neither which check failed nor whether an email exists. A refused
sign-in leaves the transient keys in the session; the user recovers by starting again at `/auth/oidc/login`, which
replaces them. Nothing retries automatically.

A returning identity whose provider email changed or became invalid still signs in, and the stored email is unchanged.
Disabling an account refuses its next sign-in through the 401 above, and ends its live sessions through the gates of the
Design "Request authentication".

## Security and operations

The provider is trusted for the identity it asserts and for nothing else. Matching on `(issuer, subject)` and never on
email means a provider that releases an unverified or attacker-chosen address cannot reach an existing account: a first
sign-in whose email belongs to another account is refused rather than linked, and answers with the same body as a
disabled account so the response does not confirm that the address exists. There is no auto-linking and no promotion:
the first administrator exists only through bootstrap, and OIDC never produces a child account.

The callback checks the state before any outbound request, PKCE binds the code to this browser's session, and the nonce
binds the ID token to this flow. The session id is rotated in `login` before any identity claim is attached. The email
claim is validated by shape, and an invalid value is logged by length only, never verbatim. The collision refusal logs
`outcome = "email_collision"` at warning level without the address; the disabled-account refusal logs nothing.

A self-hoster registers `/auth/callback` on the public origin at the provider and sets `OIDC_REDIRECT_URI` to match. The
session cookie's `SameSite=Lax` policy is what lets the cross-site redirect back from the provider carry the session.
The flow has no linking step, so a refused collision stays refused until the other account no longer holds the email. A
500 from the callback points at the provider's reachability or ID token; the server log carries the token-exchange or
ID-token error.

## More information

- [Reference configuration](../../../../website/src/content/docs/reference/configuration.mdx): the generated `OIDC_*`
  field reference.
