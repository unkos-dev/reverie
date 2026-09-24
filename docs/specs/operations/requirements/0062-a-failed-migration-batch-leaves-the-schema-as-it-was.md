---
type: REQ
profile-version: 1
id: "REV-REQ-0062"
title: "A failed migration batch leaves the schema as it was"
governed-by:
  - "REV-ADR-0014"
---

# A failed migration batch leaves the schema as it was

## Statement

WHEN a migration in a batch fails, Reverie MUST leave both the database's schema objects and its record of applied
migrations exactly as they stood before the batch began. A batch is the set of pending transactional migrations applied
together; a migration marked `-- no-transaction` runs on its own after the batch commits and lies outside this
obligation.

## Rationale

An operator recovers from a failed upgrade by pinning the previous image and restarting, and that recovery works only
while the database is untouched. A batch that applied three of five migrations leaves a schema that neither the previous
image nor the new one runs against, and the way out is hand-written SQL against the operator's own live data. The party
who depends on this is the operator whose upgrade has just failed, at the moment they are least equipped to write that
SQL and least certain which migrations landed.

## Acceptance criteria

- After a batch in which one migration fails, the applied-migration record holds no row for any migration in that batch,
  including the ones whose SQL succeeded before the failure. Determined by inducing a failure after at least one
  migration in the batch has succeeded and reading the applied-migration record; no automated test does this, since
  `batch_failure_rolls_back_tracking_rows` fails the batch at its first migration, before any tracking row is written.
- After that same failure, none of the schema objects the batch would have created exists. Determined by inducing a
  failure partway through a batch and confirming that none of the objects created by the batch's earlier migrations
  exists; no automated test does this, since `batch_failure_rolls_back_tracking_rows` asserts only the applied-migration
  record.
