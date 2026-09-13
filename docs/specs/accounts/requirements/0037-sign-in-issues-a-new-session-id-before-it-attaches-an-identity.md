---
type: REQ
profile-version: 1
id: "REV-REQ-0037"
title: "Sign-in issues a new session id before it attaches an identity"
governed-by:
  - "REV-ADR-0015"
---

# Sign-in issues a new session id before it attaches an identity

## Statement

A successful sign-in MUST issue a new session id before attaching any identity to the session, so that a session id
present before authentication never becomes an authenticated session.

## Rationale

This defends against session fixation: an attacker who plants a known session id on a victim's browser before
authentication must not have that id become authenticated once the victim signs in. See the
[OWASP Session Management cheat sheet](https://cheatsheetseries.owasp.org/cheatsheets/Session_Management_Cheat_Sheet.html)
on renewing the session id after any change in privilege level. See
[REV-ADR-0015](../../../adr/0015-first-party-session-layer-on-the-tower-sessions-core.md).

## Acceptance criteria

- The session id changes across a successful sign-in, so a pre-authentication id never authenticates. Checked by
  `callback_succeeds_first_user_not_promoted` in `backend/src/routes/auth.rs`, which asserts the session cookie's value
  after `/auth/callback` differs from the value issued before it.
- The new id is issued before any identity claim is attached, not merely by the time the response is sent. Verified by
  inspection of `login` in `backend/src/auth/session.rs`, which calls `session.cycle_id()` before either identity claim
  is inserted.
