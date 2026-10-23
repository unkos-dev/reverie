---
type: ADR
profile-version: 1
id: "REV-ADR-0058"
title: "One-shot sign-in attempts with a single winner"
status: "accepted"
recorded-on: "2026-10-18"
decision-makers:
  - "John Unkovich"
---

# One-shot sign-in attempts with a single winner

## Context and problem statement

Local and OIDC sign-in share one browser session. Concurrent requests can verify different credentials, and a delayed
request can finish after another has established an account. OIDC callbacks also need to consume their state even when
the provider exchange fails. Session persistence alone supplies neither atomic consumption nor arbitration across
replicas.

This record refines [REV-ADR-0029](0029-unified-identity-with-pluggable-authentication-providers.md)'s shared identity
and session model. It leaves issuer-and-subject identity matching, generic account refusals and the two co-equal sign-in
modes intact.

## Decision drivers

- Consume an authorization response once, before provider I/O.
- Give competing sign-ins from one anonymous session a single authority to establish an account.
- Keep local credential verification independent of the OIDC provider.
- Bound pending secrets and cleanup work using the existing PostgreSQL and session lifecycle.
- Preserve session regeneration and generic account refusals.

## Considered options

- Session value map: reuses persistence but cannot atomically claim one value across concurrent session saves.
- One signed cookie per attempt: binds material to the browser, but replay prevention and arbitration still need shared
  server state.
- Multiple attempts per session: permits independent pending starts, with more state to select and expire; rejected on
  17 October 2026 in favour of one pending attempt.
- PostgreSQL attempts with a single-winner cohort: shares atomic claims across replicas and cascades with session
  deletion.

## Decision outcome

Chosen option: **PostgreSQL attempts with a single-winner cohort**, because conditional database mutations give every
replica the same consumption and arbitration authority.

Persist one cohort per anonymous session and at most one OIDC attempt under it. Both cascade from the session row. Store
state as a SHA-256 digest with a fixed 15-minute attempt expiry. A new start replaces the pending attempt. Claim
matching live state in one atomic delete before exchanging the code; a failed exchange requires a new start.

A conditional transition from pending to won is the sole authority to provision or link identity and establish an
authenticated session. Rotation deletes the old session row and cascades its cohort and attempt. A won cohort is never
reopened, and an anonymous newcomer is refused without changing that session. There is no time-based recovery. Account
switching requires sign-out first.

Local sign-in joins the cohort only when a live session exists, avoiding session and cohort creation for failed attempts
without a cookie.

Every error after winning and before session login succeeds first flushes the session, removing the row and browser
cookie so an explicit next attempt starts fresh. Successful sign-in regenerates the session id before attaching
identity.

One attempt per session and single-winner arbitration are Reverie application policy, rather than behaviour OIDC
prescribes. One-time browser-bound state follows RFC 9700, while session regeneration and invalidation follow the OWASP
Session Management Cheat Sheet. Generic refusals follow the OWASP Authentication Cheat Sheet.

### Consequences

- Positive: replay cannot repeat a provider exchange, and only one verified request per cohort can establish an account.
- Positive: session deletion owns cohort cleanup.
- Negative: OIDC initiation and callbacks need database round trips, and a new start invalidates another tab's pending
  attempt.
- Negative: a crash after claiming state consumes the attempt. A database claim and provider exchange cannot commit
  atomically, so recovery requires another explicit sign-in.
- Negative: a failed post-win flush can leave an anonymous session losing until its idle expiry.
- Negative: arbitration covers only requests that load the session before a winner rotates it. A request carrying an id
  already retired by that rotation loads without a session and is treated as a request without a cookie. It still needs
  its own valid credentials to sign in, and the OIDC callback refuses it.

## More information

- [RFC 9700 §2.1](https://www.rfc-editor.org/rfc/rfc9700.html#section-2.1) and
  [§4.2.4](https://www.rfc-editor.org/rfc/rfc9700.html#section-4.2.4): browser binding and one-time state.
- [OWASP Session Management Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Session_Management_Cheat_Sheet.html):
  session regeneration and invalidation.
- [OWASP Authentication Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Authentication_Cheat_Sheet.html):
  generic authentication responses, including disabled accounts.
