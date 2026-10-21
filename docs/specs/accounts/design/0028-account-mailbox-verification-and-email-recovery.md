---
type: DESIGN
profile-version: 1
id: "REV-DESIGN-0028"
title: "Account mailbox verification and email recovery"
satisfies:
  - "REV-REQ-0029"
  - "REV-REQ-0030"
governed-by:
  - "REV-ADR-0063"
---

# Account mailbox verification and email recovery

An account holder proves control of the email address stored on their account, then can reset a forgotten password by
email. Account events that affect credentials or that address send security notices to a verified address.

## Purpose and boundaries

This subject owns `users.email_verified_at`, the `account_email_proofs` model, mailbox verification, public password
reset by email, the security-notice events and which recipient each event uses. It also owns the Email address, Verify
email and Forgot password screens.

It depends on [credential links](0027-credential-links.md) for the lookup, redemption and Set password experience that
emailed reset links reuse, on
[authentication mail delivery](../../operations/design/0029-authentication-mail-delivery.md) for the mail queue and
submission, and on request authentication for session and CSRF checks. Administrator links, the host `reset-password`
command, own-password change and session versioning keep their own subjects. OIDC sign-in never sets or reads
verification: identity matching stays on issuer and subject.

## Structure

`backend/src/models/account_email_proof.rs` issues, reuses, consumes and cancels proofs. The account routes serve
verification; the auth routes serve public reset requests and extend credential-link lookup and redemption to email
recovery proofs. Account transactions that change a password or address enqueue notices through the mail queue.
`frontend/src/routes/account-email.tsx`, `verify-email.tsx` and `auth-forgot-password.tsx` render the
[Email verification](../../../design/screens/email-verification.md) and
[Forgot password](../../../design/screens/forgot-password.md) screen specs.

## Interfaces and dependencies

The [OpenAPI contract](../../../../backend/openapi.json) defines the schemas:

- `GET /api/v1/account/email-verifications` returns the current address, whether it is verified, and `mail_enabled`.
- `POST /api/v1/account/email-verifications` queues or reuses a challenge and returns 202.
- `POST /api/v1/account/email-verifications/confirmations` accepts a token, consumes the matching challenge and returns
  204.
- `POST /auth/forgot-password` accepts an email and returns the same generic 200 for every valid request.
- `GET /auth/setup-status` carries `email_recovery_enabled`.

The verification operations need a browser session; the two POST operations also need CSRF. Basic and device tokens
receive 403. Unavailable mail returns a fixed 503 from the verification request. Forgot password returns 404 when local
authentication is disabled. Tokens use the canonical 43-character credential-link format.

## Data and state

`users.email_verified_at` is null for every newly created account, whatever created it. Successful mailbox confirmation
sets it. An actual change or removal of the address clears it.

`account_email_proofs` holds at most one row per `(user_id, purpose)`, with purpose `verification` or `recovery`. A row
stores the bound address, the SHA-256 digest of its token, its creation time and its expiry; never the token itself.
Verification proofs last 24 hours and recovery proofs 15 minutes. Deleting the account deletes its proofs. Expired
proofs are inert at once and removed by the hourly session sweep.

Every writer takes the user-row lock first and then reads or writes proofs and mail jobs in the same transaction. Expiry
and allowance checks use `clock_timestamp()` after the lock is acquired.

## Runtime behaviour

**Verification.** The signed-in holder requests a challenge for the current address. Within 60 seconds of the previous
request, or while a challenge is live, the request reuses it. Confirmation needs the same account's session, CSRF and a
challenge bound to the current address. It consumes the challenge, sets `email_verified_at` and enqueues the
address-verified notice in one transaction. It changes no password or address and signs nobody in. OIDC-only accounts
verify the same way.

**Public reset.** Forgot password applies its existing input validation and recovery source allowance, then does no
account lookup. It tries to take one of four background permits without waiting. With a permit, a tracked task bounded
to five seconds checks eligibility and issues or reuses a recovery proof and its mail job. Without one, the request is
dropped. The response is the same either way and never waits for the task. Eligibility means local authentication
enabled, account enabled, a verified current address and an existing local password.

Under the account lock, a pending delivery for the live proof absorbs the request. Otherwise the request is allowed only
if the account's last recovery mail was queued at least 60 seconds earlier, fewer than five were queued in the rolling
24 hours, and the live proof has had fewer than three deliveries. All three counts come from retained mail-job rows,
including failed and cancelled ones. A new proof does not reset the rolling count. A refused request changes nothing.

**Redemption.** Credential-link lookup and redemption accept an emailed recovery token as well as an administrator or
host link; verification tokens never work there. Redemption rechecks eligibility under the account lock, consumes the
winning proof, deletes every other password-replacement proof and administrator or host link, cancels their pending
mail, writes the password, increments `session_version` and enqueues the password-changed notice in one transaction. It
creates no session and leaves login throttling alone.

**Own-password change** keeps its existing session invalidation. It leaves live administrator or host links and email
recovery proofs, with their pending mail, in place. Verification challenges survive every password change.

**Address change.** When an administrator changes or removes an address, the transaction clears `email_verified_at`,
deletes both proofs, cancels unsent mail bound to the old address and clears its payloads. If the old address was
verified, it receives the change notice. Notices already queued keep their recipient.

**Notices** are queued for password replacement, first-password setup, a newly verified address and a change to or
removal of a verified address, but only to an address whose control was established and only while mail is enabled. They
survive later address changes and account disablement.

## Failure and recovery

An expired, used, replaced or wrong-account challenge, or one bound to a former address, returns the generic
invalid-link response. Redemption failures keep the credential-link responses: 422 for any unusable token, 429 for
source exhaustion and 503 for hash capacity.

If mail is enabled and queuing a notice fails, the account change rolls back. Later submission failure never undoes it.
A public request lost to a full permit pool, a timeout or a restart leaves no trace; the person asks again.

With mail disabled, Forgot password shows that email reset is unavailable, verification requests return 503, and no
notice is queued. Proofs already issued keep their authority and expiry.

## Security and operations

Public requests reveal nothing about accounts: unknown, disabled, unverified and passwordless addresses all receive the
same response, and only eligible requests create rows. Tests hold background work to prove the response does not wait
for it; they do not compare timings.

A recovery proof is a bearer credential valid for 15 minutes; an account cannot enrol a first password through it.
Public requests never touch the administrator and host link slot. Verification needs both the session and the mailed
challenge, so neither a copied link nor a signed-in session alone verifies an address.

Notices use the configured support address as Reply-To and in their reporting instructions. Reverie adds no automatic
locking or rollback when someone reports a mistake. An account without a verified address receives no notice.
