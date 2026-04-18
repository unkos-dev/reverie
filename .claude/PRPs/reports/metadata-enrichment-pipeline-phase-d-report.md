# Implementation Report: Metadata Enrichment Pipeline — Phase D

**Branch:** feat/metadata-enrichment-pipeline
**Date:** 2026-04-17
**Plan:** `.claude/PRPs/plans/metadata-enrichment-pipeline.plan.md` (Phase D — integration test coverage, finalising Step 7)

## Summary

Phase D completes Step 7 with integration-test coverage for the wiring that Phase C
shipped. Scope is tests-only — no new features, no refactors. Two production bugs
surfaced while writing the tests and were fixed under the brief's "fix bugs tests
reveal" clause; the fixes are minimal. A small grant-only migration was added so the
`tome_ingestion` role can run the enrichment orchestrator.

## Tasks Completed

| # | Task | Status | Notes |
|---|---|---|---|
| 29 | Unit tests audit | Pass (no gaps) | Every Testing-Strategy table row is already covered by existing tests in `lookup_key.rs`, `value_hash.rs`, `confidence.rs`, `policy.rs`. |
| 30 | Ingest invariant tests | 3 new | `ingest_sets_version_pointers_for_all_canonical_fields`, `ingest_without_opf_writes_heuristic_title_journal`, `ingest_sets_work_authors_source_version_id`. |
| 33 | Orchestrator integration | 5 new | multi-source agreement, disagreement, empty-canonical AutoFill, locked field, dry-run preview. |
| 34 | Rematch tests | 4 new | auto-merge, suspected-multi-manifestations, suspected-manual-draft, noop. |
| 35 | Queue tests | 4 new | two-worker race, backoff window, max-attempts → skipped, shutdown revert. |
| 36 | Cache tests | 1 new | ISBN-10/13 dedupe via `lookup_key`; per-kind TTL + expiry already covered. |
| 37 | Cover download | 1 new | DNS-rebinding redirect via `localhost` OS lookup; byte-cap + content-type + sub-threshold already covered. |

## Production Bugs Fixed (tests revealed them)

| Bug | Location | Fix |
|---|---|---|
| `SELECT DISTINCT … FOR UPDATE` is illegal in PostgreSQL (error `0A000`). The enrichment orchestrator called this path on every ISBN change; it would have errored at runtime. | `models/work.rs::rematch_on_isbn_change` | Remove `DISTINCT`, lock manifestations, dedupe work_ids in Rust via a `HashSet`. |
| `CanonicalState::is_empty_for` treated empty strings as "populated", so `AutoFill` could never fire on canonical fields that defaulted to `''` (e.g. `works.title` is `NOT NULL` and starts `''` for stubs). | `services/enrichment/orchestrator.rs::CanonicalState::is_empty_for` | Added a `blank` helper: `v.as_deref().unwrap_or("").is_empty()` applied to every string field. |

## Deviations From Plan

| Item | WHAT changed | WHY |
|---|---|---|
| Task 37 DNS-rebinding target | Uses `localhost` as the redirect destination (OS resolves to 127.0.0.1) instead of "mocking hickory-resolver". | `http::validate_hop` uses `std::net::ToSocketAddrs` (OS resolver). No injection point for hickory — a resolver refactor is out of scope. Driving the same code path via a real OS lookup proves the redirect SSRF guard rejects private IPs. |
| Added grant-only migration `20260417000002_grant_field_locks_select_ingestion` | Grants `tome_ingestion` `SELECT` on `field_locks`. | The enrichment orchestrator calls `field_lock::is_locked_tx` on every field; previously this failed with `permission denied for table field_locks` when run under the background-pipeline role. With the grant, orchestrator tests pass under `tome_ingestion`. Writes (lock/unlock) remain a `tome_app` surface. |
| Orchestrator + queue tests use `tome_ingestion` | Rather than `tome_app` as in current `main.rs` wiring. | `tome_app` requires an `app.current_user_id` session variable for the RLS `INSERT ... RETURNING` path on manifestations. Setting that up per test adds complexity beyond Phase D. `tome_ingestion` holds a `FOR ALL USING (true) WITH CHECK (true)` RLS policy on manifestations, matching the background-pipeline semantics of the queue. **Follow-up surface:** `main.rs` may want to switch the enrichment queue pool to the ingestion pool (currently uses `state.pool` to write `webhook_deliveries`). That's architectural and out of Phase D scope. |

## Validation Results

| Level | Status | Notes |
|---|---|---|
| `cargo fmt --check` | PASS | 0 diffs. |
| `cargo clippy -D warnings` (new code) | PASS | 2 pre-existing errors in `db.rs` + `cleanup.rs` resolved on this branch (commits `4c01c31`, `0e85350`). |
| `cargo test --bin tome-api` (no DB) | PASS | 201 passed, 41 ignored, 0 failed (post-adversarial-review fixes). |
| `cargo test --bin tome-api -- --ignored` (with DB) | PASS | 41 passed / 0 failed.  The Phase C `field_lock::lock_unlock_roundtrip` failure was resolved by commit `4249774` which splits the fixture across `tome_ingestion` (manifestations INSERT) and `tome_app` (field_locks writes). |

## Tests Added (18 total)

| File | Tests | Purpose |
|---|---|---|
| `services/ingestion/orchestrator.rs` | 3 `#[ignore]` | Ingest invariant: `*_version_id` pointers wired on canonical fields; heuristic-fallback journal row with `source='opf', confidence=0.2`; `work_authors.source_version_id` wired to creators draft. |
| `services/enrichment/orchestrator.rs` | 5 `#[ignore]` + wiremock helpers + `tome_app_pool()` | Full orchestrator flow through all 3 sources via wiremock. |
| `services/enrichment/queue.rs` | 4 `#[ignore]` + helpers | `claim_next` race + backoff window, `mark_failed` skipped transition, `revert_in_progress`. |
| `models/work.rs` | 4 `#[ignore]` + helpers | `rematch_on_isbn_change` branches. |
| `services/enrichment/cache.rs` | 1 `#[ignore]` | ISBN-10/13 dedupe via `lookup_key::isbn_key`. |
| `services/enrichment/cover_download.rs` | 1 (non-ignored) | DNS-rebinding redirect to hostname that OS-resolves to loopback is blocked. |

## Files Changed

### New
- `backend/migrations/20260417000002_grant_field_locks_select_ingestion.up.sql`
- `backend/migrations/20260417000002_grant_field_locks_select_ingestion.down.sql`

### Modified
- `backend/src/models/work.rs` — 4 new rematch DB tests + `insert_work` / `insert_manifestation` / `preclean_rematch_isbn` helpers. Production fix: remove `DISTINCT` from the `FOR UPDATE` match query and dedupe in Rust.
- `backend/src/services/enrichment/orchestrator.rs` — 5 new orchestrator DB tests + `config_with_mock_sources`, `insert_enrich_fixture`, `cleanup_enrich_fixture`, wiremock mount helpers, `tome_app_pool` helper. Production fix: `CanonicalState::is_empty_for` now treats empty strings as empty.
- `backend/src/services/enrichment/queue.rs` — 4 new queue DB tests + `quiesce_queue` + `insert_queue_fixture` / `cleanup_queue_fixture`.
- `backend/src/services/enrichment/cache.rs` — 1 new ISBN-dedupe test.
- `backend/src/services/enrichment/cover_download.rs` — 1 new DNS-rebinding redirect test.
- `backend/src/services/ingestion/orchestrator.rs` — 3 new ingest-invariant DB tests + `preclean_isbn` helper.

## Pre-existing Issues Surfaced (resolved on this branch)

1. `services::enrichment::field_lock::tests::lock_unlock_roundtrip` (Phase C) — RESOLVED by commit `4249774` (split fixture: `tome_ingestion` for manifestations INSERT, `tome_app` for `field_locks` writes).
2. Pre-existing clippy errors in `db.rs` and `cleanup.rs` — RESOLVED by commits `4c01c31` and `0e85350`.
3. `main.rs` wired the enrichment queue to `state.pool` (`tome_app`) — would silently no-op in production because RLS on `manifestations` requires the `app.current_user_id` session variable that the queue never sets.  RESOLVED in adversarial-review pass: `main.rs` now passes `state.ingestion_pool` to `spawn_queue` (matches the `manifestations_ingestion_full_access` policy).

## Next Steps

- [ ] `/code-review` on the Phase D diff (done — adversarial review run; findings tracked in PR notes)
- [ ] Open PR for the feature branch
