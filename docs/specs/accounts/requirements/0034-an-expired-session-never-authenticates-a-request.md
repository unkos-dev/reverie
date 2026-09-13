---
type: REQ
profile-version: 1
id: "REV-REQ-0034"
title: "An expired session never authenticates a request"
governed-by:
  - "REV-ADR-0015"
---

# An expired session never authenticates a request

## Statement

A request presenting a session cookie whose session has passed its expiry MUST be treated as unauthenticated. An expired
session never authenticates a request, whether or not the expired row has already been swept from the session store.

## Rationale

Expiry is enforced at the point of use, not by a background cleanup process; see the
[OWASP Session Management cheat sheet](https://cheatsheetseries.owasp.org/cheatsheets/Session_Management_Cheat_Sheet.html)
on session expiration. A periodic sweep of expired rows is availability hardening for the store's table size, a separate
concern from access control: if access control depended on the sweep having already run, a delayed or missed sweep tick
would leave an expired session briefly authenticating requests it must not. See
[REV-ADR-0015](../../../adr/0015-first-party-session-layer-on-the-tower-sessions-core.md).

## Acceptance criteria

- A request presenting a session cookie whose row's expiry has passed never resolves to a live session, regardless of
  whether that row has been swept. Verified by inspection of the load query's expiry predicate in
  `backend/src/auth/store.rs` (`PostgresStore::load`, `WHERE id = $1 AND expiry_date > now()`); no test presents an
  expired cookie to a live route, since the predicate excludes the row at the store layer before any route runs.
- A background sweep deletes expired rows without affecting which sessions authenticate. Covered by
  `sweep_deletes_expired_and_keeps_live` in `backend/src/services/session_sweep.rs`, which is a separate path from the
  expiry check above.

## More information

The sweep and the expiry check are independent mechanisms that happen to target the same condition: the sweep bounds
table growth, and the load-time predicate is what actually gates authentication. A missed or failed sweep tick degrades
table size, never access control.
