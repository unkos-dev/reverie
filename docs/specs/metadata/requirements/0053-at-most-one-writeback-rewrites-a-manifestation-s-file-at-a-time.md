---
type: REQ
profile-version: 1
id: "REV-REQ-0053"
title: "At most one writeback rewrites a manifestation's file at a time"
governed-by:
  - "REV-ADR-0018"
---

# At most one writeback rewrites a manifestation's file at a time

## Statement

The system MUST NOT run two writeback jobs against the same manifestation's file concurrently, and this exclusion MUST
be enforced by the database itself rather than depend solely on the behaviour of whatever process claims writeback work;
WHEN two attempts to start a writeback for the same manifestation race each other, exactly one MUST proceed and the
other MUST fail without starting.

## Rationale

A manifestation's file is one physical artefact a reader may already be reading or downloading; two writeback jobs
rewriting it at the same time would interleave two different sets of changes into the same bytes, corrupting the archive
with nothing to indicate what happened. Depending on application logic alone to keep two overlapping worker processes,
or two overlapping tasks within one process, from touching the same file at once is fragile against exactly the kind of
process crash or restart durable background work must tolerate; only a constraint the database itself enforces holds
regardless of how the workers above it behave.

## Acceptance criteria

- Two workers that race to claim writeback work for the same manifestation cannot both succeed: exactly one claim
  proceeds and the other fails. Checked by `concurrent_claims_on_same_manifestation_serialise_via_unique_index` in
  `backend/src/services/writeback/queue.rs`.
- A second job already queued for a manifestation that has a writeback in progress stays unclaimed for as long as the
  in-progress one holds that state. Checked by `not_exists_filter_excludes_siblings_of_in_progress_job` in
  `backend/src/services/writeback/queue.rs`, which asserts the claim eligibility condition directly excludes a pending
  sibling of an in-progress job.
- The exclusion is backed by a unique index scoped to in-progress rows for a manifestation, not only by the query that
  selects work to claim: the index `idx_writeback_jobs_in_progress_unique` is present in the generated schema at
  `backend/schema.sql`.
