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

PIN recovery, first-run setup, self-registration, own-password changes, device-token lifecycle and session persistence
belong to their respective subjects. There is no invitation email delivery or recovery notification channel.

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
- `POST /auth/credential-links/lookups` accepts a token and returns the account email and password-presence flag.
- `POST /auth/credential-links/redemptions` accepts a token and password and returns 204 without creating a session.

Issuance and lookup responses carry `Cache-Control: no-store` to prevent caching of tokens and account metadata.

The three administrator operations require an administrator, a non-child account, Admin scope and browser-session
authentication. Device tokens over Basic or Bearer receive 403. All five operations return 404 when local authentication
is disabled. User responses expose `has_password` and an unexpired `credential_link_expires_at`, or null.

Account email inputs use the shared backend validator, which applies addr-spec syntax and length bounds alongside the
frontend's common ASCII address pattern. Local parts use letters, digits, apostrophes, plus signs, hyphens, underscores
and single separating dots, and end in a letter, digit, plus sign, hyphen or underscore. Domains contain at least one
dot and end in an alphabetic suffix of at least two letters. Quoted local parts, single-label domains, other local-part
punctuation and Unicode addresses are unsupported. Admin creation and edits reject unsupported addresses with 422.
Frontend forms and lookup responses enforce the common pattern through their Zod email validators.

## Data and state

`credential_links` has one row per user: a cascading user primary key, a unique SHA-256 `token_hash`, `expires_at` and
`created_at` with timestamp decode-range checks. `reverie_app` alone has SELECT, INSERT, UPDATE and DELETE grants.
`CREDENTIAL_LINK_TTL` is a fixed 72 hours. Tokens contain 32 random bytes encoded as 43 canonical base64url characters
without padding. Only issuance returns the raw token; lookup hashes the submitted value.

Issuance upserts the account's row. Cancellation deletes it. Redemption conditionally deletes it inside the password
transaction. User deletion cascades. Expired rows remain inert until replaced, redeemed or cascaded; their count cannot
exceed the user count. There is no sweep.

The Set password route loader captures the fragment token and removes it from browser history with `replaceState` before
mounting the page or starting lookup. The router retains the token in memory for that route. React Query owns lookup,
password-policy loading and mutation state. The Users dialogs own the once-displayed result and clear it when closed.
The browser builds the link from `window.location.origin`; the backend never uses Host to construct it.

## Runtime behaviour

Account creation inserts the user and issues its link in one transaction. Issuance for an existing account refuses a
disabled account or one without an email. Issuing another link invalidates the previous token without changing the
password. Cancellation also leaves the password unchanged.

Lookup accepts only a matching digest whose expiry is after database `now()` and whose account is enabled and has an
email. Both public operations spend the recovery source allowance before token-format validation, lookup or policy work.
Malformed token lengths, alphabet or encoding return the same generic invalid-link response without a database lookup.

Redemption looks up the account without a lock, applies password policy with its email and display name as context, and
hashes through the shared limiter. A transaction then locks the user with `FOR NO KEY UPDATE`, consumes the link only if
its digest still matches and `expires_at > clock_timestamp()`, rechecks account status and email, writes the password,
increments `session_version` and commits. The wall-clock comparison catches expiry while waiting for the user lock.
Concurrent redemption has one winner. No database handle or user lock is held while hashing.

The form shows new-account or reset copy from `has_password`, checks confirmation and the existing client policy, and
renders server errors. A temporary lookup failure offers an explicit retry that retains the token in memory and disables
its action while fetching. The component owns retry intent; React Query owns request state. Reconnect does not repeat
lookup or discard form values. Success offers an explicit Sign in link and is terminal: lookup stops and late lookup
errors cannot replace it. Redemption creates no session and does not reset the login throttle. PIN recovery and device
tokens remain unchanged.

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

A link is a bearer credential. Administrators deliver it out of band; forwarding it delegates the ability to set the
bound account's password. Expiry, cancellation and replacement limit exposure. Neither digests nor raw tokens belong in
logs. Fragment removal limits accidental retention in navigation history; fragments are not sent in HTTP requests.
Session-version invalidation forces existing sessions to sign in again after successful redemption.

Recovery notifications are absent: NIST SP 800-63B-4's account-recovery notification requirement is unmet until a mail
channel is provided. Device-token revocation is outside this flow. Link and PIN recovery do not cancel each other.
