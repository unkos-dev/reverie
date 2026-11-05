---
type: REQ
profile-version: 1
id: "REV-REQ-0078"
title: "A timed-out enrichment run releases its claim safely"
governed-by:
  - "REV-ADR-0068"
---

# A timed-out enrichment run releases its claim safely

## Statement

WHEN a queued enrichment attempt reaches its configured whole-run deadline, the worker MUST stop admitting further work
for that attempt and, after its owned in-flight work has resolved and PostgreSQL acknowledges recovery, release its
active claim through the retry outcome without requiring a process restart. A claim still held twice that deadline after
it was taken MUST likewise be released through failure handling without a restart. Recovery MUST NOT make another
attempt eligible while the expired attempt can still change catalogue state, and a released attempt MUST NOT change the
manifestation's enrichment or catalogue state afterwards.

## Rationale

A stalled attempt can consume the limited enrichment capacity indefinitely and prevent later catalogue work from
running. An attempt that fails while recording its outcome leaves the book shown as in progress and outside the failure
dashboard until something releases it. Releasing a claim while its writes can still land instead lets overlapping
attempts change the same catalogue state.

## Acceptance criteria

- A positive configured budget governs the whole queued attempt, including a wait outside provider fan-out. A run
  completed before its deadline follows normal completion handling. Zero is rejected rather than disabling recovery.
  Checked by `enrichment_run_budget_configuration` in `backend/src/config/mod.rs` and
  `enrichment_timeout_recovers_claim_and_continues` in `backend/src/services/enrichment/queue.rs`.
- An attempt stalled outside provider fan-out expires, leaves `in_progress` through failure handling and releases its
  worker slot so another eligible manifestation completes. Its ordinary retry remains subject to the existing backoff.
  Checked by `enrichment_timeout_recovers_claim_and_continues` in `backend/src/services/enrichment/queue.rs`.
- Expiry at the configured attempt cap leaves the row `skipped`. A rerun requested during the expired attempt instead
  leaves it immediately eligible `pending` with cleared attempt and failure state. Checked by
  `enrichment_timeout_recovers_claim_and_continues` in `backend/src/services/enrichment/queue.rs`.
- Before commit starts, expiry leaves no committed journal, canonical, pointer, rating or writeback change from the
  abandoned catalogue transaction. Recovery waits for that transaction to resolve. Checked by
  `enrichment_timeout_rolls_back_owned_writes` in `backend/src/services/enrichment/orchestrator.rs`.
- When expiry occurs after commit starts, ownership remains held until commit resolves. Any committed catalogue change
  and its writeback enqueue remain atomic; recovery never represents an uncertain commit as proven rollback. Checked by
  `enrichment_timeout_waits_for_commit` for ownership and `enrichment_timeout_rolls_back_owned_writes` for database
  atomicity, both in `backend/src/services/enrichment/orchestrator.rs`.
- A claim still `in_progress` more than twice the configured budget after it was taken leaves `in_progress` through
  failure handling, class `internal` with no source, on the next poll. A younger claim, and a claim whose catalogue
  transaction holds the manifestation row, stay `in_progress`. Checked by
  `stale_claims_are_rescued_and_their_attempts_fenced` in `backend/src/services/enrichment/queue.rs`.
- After its claim is released, an attempt's catalogue transaction is refused before any write and its outcome
  bookkeeping leaves the row unchanged, including when a rerun has reset the attempt count so the next claim carries the
  same attempt number. Checked by `stale_claims_are_rescued_and_their_attempts_fenced` in
  `backend/src/services/enrichment/queue.rs`.
