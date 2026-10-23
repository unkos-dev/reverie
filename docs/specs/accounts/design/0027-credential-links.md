---
type: DESIGN
profile-version: 1
id: "REV-DESIGN-0027"
title: "Credential links"
satisfies:
  - "REV-REQ-0029"
  - "REV-REQ-0030"
governed-by:
  - "REV-ADR-0061"
---

# Credential links

Credential links let an administrator create an account or initiate a password reset while the account holder chooses
the password. A link binds one password-setting operation to one enabled account with an email.

## Purpose and boundaries

This subject owns link issuance, replacement, cancellation, lookup and redemption, the `credential_links` model, and the
Users and Set password screens. It depends on request authentication for browser-session and administrator checks, the
shared password policy and hash limiter, and the user model's transaction and session-version operations.

First-run setup, self-registration, own-password changes, device-token lifecycle and session persistence belong to their
respective subjects. [Mailbox verification and email recovery](0028-account-mailbox-verification-and-email-recovery.md)
owns emailed reset proofs and security notices;
[authentication mail delivery](../../operations/design/0029-authentication-mail-delivery.md) sends links an
administrator chooses to email.

## Structure

`backend/src/models/credential_link.rs` stores and consumes links. The users routes create accounts and issue or cancel
links; the auth routes provide public lookup and redemption. `frontend/src/api/users.ts` and `auth.ts` parse their
contracts. `UsersPage.tsx` displays link actions and results; `routes/set-password.tsx` renders redemption in
`AuthShell`. The [Users](../../../design/screens/users.md) and [Set password](../../../design/screens/set-password.md)
screen specs hold layout, copy, states and accessibility acceptance.

## Interfaces and dependencies

The [OpenAPI contract](../../../../backend/openapi.json) defines request and response schemas:

- `POST /api/v1/users` creates a passwordless account and returns its user data and link with 201.
- `POST /api/v1/users/{id}/credential-links` returns a new link with 201, replacing any prior link for that account.
- `DELETE /api/v1/users/{id}/credential-links` cancels the link with an idempotent 204.
- `POST /api/v1/users/{id}/credential-links/emails` accepts the live token in the body, queues it for the account's
  stored address and returns the job ID and status with 202, or a fixed 503 when mail is not configured.
- `GET /api/v1/users/{id}/credential-links/emails/{job_id}` returns that job's queued, accepted, failed or inactive
  status. A delivery is inactive once its token no longer matches the account's usable, unexpired link, including
  deliveries the relay previously accepted or refused.
- `POST /auth/credential-links/lookups` accepts a token and returns the account email and password-presence flag.
- `POST /auth/credential-links/redemptions` accepts a token and password and returns 204 without creating a session.

Issuance and lookup responses carry `Cache-Control: no-store` to prevent caching of tokens and account metadata.

The five administrator operations require an administrator, a non-child account, Admin scope and browser-session
authentication. Device tokens over Basic or Bearer receive 403. All seven operations return 404 when local
authentication is disabled. User responses expose `has_password` and an unexpired `credential_link_expires_at`, or null.

Account email inputs use the shared backend validator, which applies addr-spec syntax and length bounds alongside the
frontend's common ASCII address pattern. Local parts use letters, digits, apostrophes, plus signs, hyphens, underscores
and single separating dots, and end in a letter, digit, plus sign, hyphen or underscore. Domains contain at least one
dot and end in an alphabetic suffix of at least two letters. Quoted local parts, single-label domains, other local-part
punctuation and Unicode addresses are unsupported. Admin creation and edits reject unsupported addresses with 422.
Frontend forms and lookup responses enforce the common pattern through their Zod email validators.

## Data and state

`credential_links` has one row per user: a cascading user primary key, a unique SHA-256 `token_hash`, `expires_at` and
`created_at` with timestamp decode-range checks. `reverie_app` alone has SELECT, INSERT, UPDATE and DELETE grants.
Issuance takes an explicit lifetime: 72 hours for administrator creation and reset, ten minutes for the host
`reset-password` command. Tokens contain 32 random bytes encoded as 43 canonical base64url characters without padding.
Only issuance returns the raw token; lookup hashes the submitted value.

Issuance upserts the account's row. Cancellation deletes it. Redemption conditionally deletes it inside the password
transaction. User deletion cascades. Expired rows remain inert until replaced, redeemed or cascaded; their count cannot
exceed the user count. There is no sweep.

The `/set-password` and `/reset-password` routes share one page and loader. Copied and emailed links use `/set-password`
for accounts without a password and `/reset-password` for accounts with one, selected at issuance or enqueue. Email
recovery always uses `/reset-password`. Both routes render their form from the lookup response’s `has_password`.

The shared route loader captures the fragment token and removes it from browser history with `replaceState` before
mounting the page or starting lookup. The router retains the token in memory for that route. React Query owns lookup,
password-policy loading and mutation state. The Users dialogs own the once-displayed result and clear it when closed.
Token-bearing queries and mutations use zero cache retention after the page or dialog closes. Delivery polling belongs
to the open result dialog, runs every three seconds while queued and stops at a final outcome or close. The
administrator's browser builds the link from `window.location.origin`. The host command and emailed links use
`REVERIE_PUBLIC_URL`. The backend never uses Host or forwarded headers to construct a link.

## Runtime behaviour

Account creation inserts the user and issues its link in one transaction. Issuance for an existing account refuses a
disabled account or one without an email. Issuing another link invalidates the previous token without changing the
password. Cancellation also leaves the password unchanged.

Lookup accepts a matching administrator or host link, or an eligible email recovery proof, whose expiry is after
database `now()` and whose account is enabled and has an email. Verification challenges never resolve here. Both public
operations spend the recovery source allowance before token-format validation, lookup or policy work. Malformed token
lengths, alphabet or encoding return the same generic invalid-link response without a database lookup.

Redemption looks up the account without a lock, applies password policy with its email and display name as context, and
hashes through the shared limiter. A transaction then locks the user with `FOR NO KEY UPDATE`, consumes the link only if
its digest still matches and `expires_at > clock_timestamp()`, rechecks account status and email, deletes every other
password-replacement proof and cancels its pending mail, writes the password, increments `session_version`, enqueues the
password notice when mail is enabled and the address is verified, and commits. The wall-clock comparison catches expiry
while waiting for the user lock. Concurrent redemption has one winner. No database handle or user lock is held while
hashing.

The create and reset flows retain one dialog through the link result; React Aria owns focus containment and restoration.
Pending actions retain focus while React Aria blocks repeated presses and implicit form submission.

The form shows new-account or reset copy from `has_password`. Native React Aria field validation applies the existing
client policy to the new password and equality with it to the confirmation, which is checked again when either changes.
Each error shows beneath its field, and a blocked submission focuses the first invalid field and sends nothing. Server
errors render as a general alert. A temporary lookup failure offers an explicit retry that retains the token in memory
and disables its action while fetching. The component owns retry intent; React Query owns request state. Reconnect does
not repeat lookup or discard form values. Success offers an explicit Sign in link and is terminal: lookup stops and late
lookup errors cannot replace it. Redemption creates no session and does not reset the login throttle. Device tokens
remain unchanged.

Redemption emits a structured authentication event after success or failure. Events include an outcome, a generated
request ID, the account ID when known, the resolved source IP and a bounded, control-free user agent. Events contain
neither link tokens nor passwords, email addresses or raw error details. Success is recorded only after commit.

## Failure and recovery

Unknown, expired, consumed, disabled-account and no-email links all return 422 with `invalid or expired link`. A failed
transaction rolls back link consumption and password writes. Policy rejection leaves the link usable. Recovery source
exhaustion returns 429 with the shared positive `Retry-After` delay; hash-capacity exhaustion returns the existing 503
detail without Retry-After. Database errors use the generic internal-error response.

Password-policy loading failures offer an in-page retry while retaining the token and entered values. The retry action
is disabled while fetching, and submission remains disabled until the policy is ready.

A redemption network failure retains the entered values and explains that the password may have changed, with an
explicit sign-in link. The browser does not retry redemption automatically or infer success from a consumed token.

Issuing to a disabled account returns 422 with `the account is disabled`; a missing email returns 422 with
`the account has no email address`. The Users screen explains how to correct the latter. A lost link requires another
link. The result is shown once and can be copied or dismissed. If clipboard access is unavailable or denied, the
existing error toast directs the administrator to select and copy the displayed link manually.

## Security and operations

A link is a bearer credential. Administrators copy it for manual delivery or explicitly request email; forwarding it
delegates the ability to set the bound account's password. Expiry, cancellation and replacement limit exposure. Neither
digests nor raw tokens belong in logs. Fragment removal limits accidental retention in navigation history; fragments are
not sent in HTTP requests. Session-version invalidation forces existing sessions to sign in again after successful
redemption.

Recovery notifications reach only a verified address on an instance with mail configured. Device-token revocation is
outside this flow. The host `reset-password` command prints its link to the operator's terminal after commit and never
to logs. It refuses an unknown or disabled account and a missing or invalid `REVERIE_PUBLIC_URL` before changing
anything, and it does not read mail settings.
