---
type: REQ
profile-version: 1
id: "REV-REQ-0007"
title: "RLS-gated rows are unreadable without a user context"
governed-by:
  - "REV-ADR-0028"
---

# RLS-gated rows are unreadable without a user context

## Statement

WHEN a database transaction has no value set for `app.current_user_id`, a read of a row-level-security-gated table in
that transaction MUST NOT return any row whose visibility depends on that value.

## Rationale

Every per-user and per-role row-level-security policy trusts `app.current_user_id` as the caller's identity, and a
pooled connection may have served a different caller, or none, just before. If a read with no user context could still
return gated rows, the whole visibility model would depend on every caller of the database layer remembering to set the
context, with nothing behind it when one forgets.

## Acceptance criteria

- On a connection that has never set `app.current_user_id`, a `SELECT` against a gated table such as `manifestations`
  returns zero rows. Checked by `rls_user_facing_pool_without_user_id_blocked_from_manifestations` in
  `backend/src/services/writeback/queue.rs`.
- On a connection where an earlier, finished transaction set `app.current_user_id` and the current transaction sets no
  new value, the same `SELECT` returns zero rows or fails with a database error, and never returns a gated row. Checked
  against a migrated database by setting the value in one committed transaction and querying in the next on the same
  connection.

## More information

Which outcome the second criterion produces depends on PostgreSQL's handling of a custom setting a connection has
already used, not on anything the application chooses. Both outcomes satisfy this obligation, which forbids disclosure,
not a particular failure. The Design that satisfies it explains the mechanism.
