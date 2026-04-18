# Implementation Report: Metadata Enrichment Pipeline — Phase C

**Branch:** feat/metadata-enrichment-pipeline
**Date:** 2026-04-17
**Plan:** `.claude/PRPs/plans/metadata-enrichment-pipeline.plan.md` (Phase C slice only — NOT archived)

## Summary

Phase C completes the wiring half of Step 7 (BLUEPRINT): per-source adapters,
orchestrator, queue worker, dry-run, field locks, REST routes, config plumbing,
and main.rs startup. Phase A (migration + journal shape) and Phase B (pure-logic
modules + SSRF-safe cover download) shipped earlier. **Phase D (integration
tests 33–37)** is deferred to a follow-up commit/PR.

## Tasks Completed

| # | Task | Status | Notes |
|---|---|---|---|
| 17 | `sources/mod.rs` trait | Complete | `MetadataSource`, `LookupKey`, `LookupCtx`, `SourceResult`, `SourceError` |
| 18 | Open Library adapter | Complete | ISBN + search; wiremock tests for 200/404/429/500 |
| 19 | Google Books adapter | Complete | ISBN + title/author; wiremock tests for 200/empty/429 |
| 20 | Hardcover adapter | Complete | GraphQL; auto-disables without token; wiremock tests |
| 21 | `enrichment/orchestrator.rs` | Complete | Per-manifestation fan-out, quorum, rematch hook |
| 22 | `queue.rs` background worker | Complete | FOR UPDATE SKIP LOCKED claim CTE, retry backoff, shutdown revert |
| 23 | `dry_run.rs` | Complete | Preview diff; writes `api_cache` only |
| 24 | `field_lock.rs` | Complete | CRUD helpers + `EntityType` enum |
| 25 | `routes/enrichment.rs` | Complete | trigger/dry-run/status endpoints |
| 26 | `routes/metadata.rs` | Complete | GET + accept/reject/revert/lock/unlock |
| 27 | Config + `.env.example` | Complete | 14 new env vars + defaults + range validation |
| 28 | Wire queue into `main.rs` | Complete | Uses `state.pool` (tome_app) |
| 37-partial | SSRF initial-URL hardening | Complete | Replaced `#[cfg(not(test))]` gate with `DownloadConfig::allow_private_hosts` flag; added loopback-blocked unit test |

## Deviations From Plan

| Item | WHAT changed | WHY |
|---|---|---|
| Cover download in orchestrator | Not fetched in MVP orchestrator | Deferred to Step 11 (library health). Sources still surface cover URLs in journal. |
| Webhook events | Logged via `tracing::info!/warn!` only | `webhook_deliveries.webhook_id NOT NULL` requires a registered webhook; event plumbing is Step 12 |
| `match_type` in orchestrator confidence calc | Reads from DB after upsert | Initial draft hardcoded `"isbn"`; fixed post-advisor review to use the authoritative stored value |
| Task 4 `find_or_create` signature | Split into `match_existing` / `create_stub` / `upgrade_stub` | Plan's unified signature breaks FK cycle (pre-Phase A; documented in `models/work.rs` preamble) |

## Validation Results

| Level | Status | Notes |
|---|---|---|
| `cargo fmt --check` | PASS | 0 diffs |
| `cargo clippy -D warnings` | PASS (new code) | 2 pre-existing errors in `db.rs` + `cleanup.rs` (unrelated to this PR) |
| `cargo build --all-targets` | PASS | 0 errors |
| `cargo test` (lib, no DB) | PASS | 188 passed, 24 ignored, 0 failed |
| `cargo audit` | DEFER | No new deps beyond Phase A/B (already vetted) |

## Tests Written (new in Phase C)

| Module | Tests | Coverage |
|---|---|---|
| `sources/open_library.rs` | 4 | happy ISBN, 404, 429 with Retry-After, 500 |
| `sources/google_books.rs` | 3 | happy ISBN, empty items, 429 with Retry-After |
| `sources/hardcover.rs` | 4 | disabled-without-token, empty-without-token, graphql happy, graphql errors |
| `cover_download.rs` | +1 (new) | Task 37 SSRF: loopback blocked when `allow_private_hosts=false` |
| `field_lock.rs` | 1 (ignored, DB) | lock/unlock round-trip + idempotency |
| `queue.rs` | 1 | backoff schedule is monotonic |
| `routes/enrichment.rs` | 3 | auth-required smoke |
| `routes/metadata.rs` | 2 | auth-required smoke |

## Deferred to Phase D

Integration coverage for the full pipeline:
- **Task 33**: orchestrator multi-source agreement / disagreement / empty-canonical auto-fill / locked-field rejection / dry-run unchanged-journal
- **Task 34**: rematch auto-merge / suspected-on-multiple-manifestations / suspected-on-manual-drafts / noop
- **Task 35**: queue two-worker race, backoff window, max-attempts → skipped, shutdown reverts in_progress
- **Task 36**: cache per-kind TTL dedupe, ISBN-10/13 key convergence
- **Task 37**: cover download byte-cap mid-stream abort, content-type mismatch, sub-threshold (happy path already covered in Phase B)

Per-node project CLAUDE.md flags this as needing coverage before merge; the follow-up PR will ship those tests and the full `--ignored` run against docker-compose postgres.

## Files Changed

### Created (8)
- `backend/src/services/enrichment/sources/mod.rs` (trait)
- `backend/src/services/enrichment/sources/open_library.rs`
- `backend/src/services/enrichment/sources/google_books.rs`
- `backend/src/services/enrichment/sources/hardcover.rs`
- `backend/src/services/enrichment/orchestrator.rs`
- `backend/src/services/enrichment/queue.rs`
- `backend/src/services/enrichment/dry_run.rs`
- `backend/src/services/enrichment/field_lock.rs`
- `backend/src/routes/enrichment.rs`
- `backend/src/routes/metadata.rs`

### Modified (7)
- `backend/src/config.rs` — EnrichmentConfig + CoverConfig + 14 env vars
- `backend/src/main.rs` — spawn `spawn_queue` alongside watcher
- `backend/src/routes/mod.rs` — register new modules
- `backend/src/services/enrichment/mod.rs` — add new submodules
- `backend/src/services/enrichment/cover_download.rs` — `allow_private_hosts` flag; remove `#[cfg(not(test))]`
- `backend/src/services/ingestion/orchestrator.rs` — extend test Config literal
- `backend/src/test_support.rs` — extend test Config literal
- `.env.example` — document all 14 new env vars

## Next Steps

- [ ] Phase D PR: integration tests 33–37 against docker-compose postgres
- [ ] Manual smoke per BLUEPRINT §Verification
- [ ] `/code-review` on this branch before merging
