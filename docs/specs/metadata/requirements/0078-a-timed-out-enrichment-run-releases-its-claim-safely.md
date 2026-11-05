---
type: REQ
profile-version: 1
id: "REV-REQ-0078"
title: "A timed-out enrichment run releases its claim safely"
governed-by:
  - "REV-ADR-0018"
---

# A timed-out enrichment run releases its claim safely

## Statement

WHEN a queued enrichment attempt reaches its configured whole-run deadline, the worker MUST stop admitting further work
for that attempt and, after its owned in-flight work has resolved and PostgreSQL acknowledges recovery, release its
active claim through the retry outcome without requiring a process restart. Recovery MUST NOT make another attempt
eligible while the expired attempt can still change catalogue state.

## Rationale

A stalled attempt can consume the limited enrichment capacity indefinitely and prevent later catalogue work from
running. Releasing its claim before its writes have resolved can instead let overlapping attempts change the same
catalogue state.

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
