---
type: REQ
profile-version: 1
id: "REV-REQ-0038"
title: "Sign-out destroys the session server-side"
governed-by:
  - "REV-ADR-0015"
---

# Sign-out destroys the session server-side

## Statement

Sign-out MUST destroy the session server-side, so that a session cookie presented after sign-out names no session and
the request that presents it is unauthenticated.

## Rationale

Server-side invalidation at logout is a core OWASP Session Management control; see the
[cheat sheet](https://cheatsheetseries.owasp.org/cheatsheets/Session_Management_Cheat_Sheet.html). Clearing the cookie
on the client alone would leave the session itself usable: a copy of the cookie value retained from before sign-out, or
one that never reached the client, would still authenticate. See
[REV-ADR-0015](../../../adr/0015-first-party-session-layer-on-the-tower-sessions-core.md).

## Acceptance criteria

- Sign-out with no session present succeeds without error, so the destroy operation is safe to call idempotently.
  Checked by `logout_returns_204_without_session` in `backend/src/routes/auth.rs`.
- Sign-out with a live session destroys it server-side, so a subsequently presented cookie for that session names
  nothing. Verified by inspection of `logout` in `backend/src/auth/session.rs`, which calls `Session::flush`, and of the
  session library's flush semantics, which delete the row from the store rather than only clearing local state.
