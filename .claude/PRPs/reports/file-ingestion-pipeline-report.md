# Implementation Report: File Ingestion Pipeline

## Summary

Implemented the core file ingestion pipeline (Blueprint Step 4): filesystem watcher,
format filtering with priority selection, atomic copy with SHA-256 verification,
path templates from filename heuristics, post-batch cleanup, quarantine with JSON
sidecar, duplicate detection, and a manual scan API endpoint. The pipeline creates
placeholder work + manifestation rows for each ingested file.

## Assessment vs Reality

| Metric | Predicted (Plan) | Actual |
|---|---|---|
| Complexity | Large | Large |
| Confidence | 8/10 | 9/10 |
| Files Changed | 16-18 | 20 (12 modified + 8 new) |

## Tasks Completed

| # | Task | Status | Notes |
|---|---|---|---|
| 0 | Add `skipped` to job_status enum | Complete | |
| 1 | Add dependencies | Complete | |
| 2 | Extend Config | Complete | |
| 3 | Extend AppState and test_support | Complete | Also fixed manual AppState in routes/tokens.rs |
| 4 | Wire main.rs | Complete | Deferred until after Task 12 (avoids referencing unwritten code) |
| 5 | Create ingestion_job model | Complete | |
| 6 | Create format_filter | Complete | |
| 7 | Create path_template | Complete | |
| 8 | Create copier | Complete | |
| 9 | Create quarantine handler | Complete | |
| 10 | Create cleanup handler | Complete | |
| 11 | Create filesystem watcher | Complete | |
| 12 | Create pipeline orchestrator | Complete | |
| 13 | Create scan API endpoint | Complete | |
| 14 | Wire routes and models | Complete | Removed #[allow(dead_code)] on Forbidden |

## Validation Results

| Level | Status | Notes |
|---|---|---|
| Static Analysis (clippy) | Pass | Zero warnings with `-D warnings` |
| Formatting (fmt) | Pass | |
| Unit Tests | Pass | 50 tests |
| Integration Tests | Pass | 6 tests (requires docker network connect workaround) |
| Full Suite | Pass | 56 tests, 0 failures |

## Files Changed

| File | Action | Lines |
|---|---|---|
| `backend/migrations/20260414100001_add_skipped_job_status.up.sql` | CREATED | +3 |
| `backend/migrations/20260414100001_add_skipped_job_status.down.sql` | CREATED | +2 |
| `backend/Cargo.toml` | UPDATED | +5 |
| `backend/Cargo.lock` | UPDATED | +191/-2 |
| `backend/src/config.rs` | UPDATED | +53 |
| `backend/src/state.rs` | UPDATED | +1 |
| `backend/src/main.rs` | UPDATED | +26/-1 |
| `backend/src/error.rs` | UPDATED | -1 (removed dead_code allow) |
| `backend/src/test_support.rs` | UPDATED | +10 |
| `backend/src/models/mod.rs` | UPDATED | +1 |
| `backend/src/models/ingestion_job.rs` | CREATED | +130 |
| `backend/src/services/mod.rs` | UPDATED | +2 |
| `backend/src/services/ingestion/mod.rs` | CREATED | +11 |
| `backend/src/services/ingestion/format_filter.rs` | CREATED | +109 |
| `backend/src/services/ingestion/path_template.rs` | CREATED | +152 |
| `backend/src/services/ingestion/copier.rs` | CREATED | +147 |
| `backend/src/services/ingestion/quarantine.rs` | CREATED | +82 |
| `backend/src/services/ingestion/cleanup.rs` | CREATED | +96 |
| `backend/src/services/ingestion/watcher.rs` | CREATED | +110 |
| `backend/src/services/ingestion/orchestrator.rs` | CREATED | +310 |
| `backend/src/routes/mod.rs` | UPDATED | +1 |
| `backend/src/routes/ingestion.rs` | CREATED | +42 |
| `backend/src/routes/tokens.rs` | UPDATED | +1 |
| `.env.example` | UPDATED | +4 |

## Deviations from Plan

1. **Task execution order**: Tasks executed as 0-3, 5-12, 4, 13-14 instead of 0-14
   sequential. This avoided referencing `run_watcher` before it was written (Task 4
   calls it, Task 12 defines it).

2. **Duplicate check before copy**: The plan specified checking for duplicates during
   the per-file loop. Implementation hashes the source file first, checks the DB for
   matching `file_hash` or `file_path`, and skips before copying. This avoids wasting
   I/O on duplicate files.

3. **Double hashing**: The source file is hashed once for duplicate detection, then
   `copy_verified` hashes it again during the integrity check. This is a known
   inefficiency — fixing it would require changing the copier API to accept a
   pre-computed hash, which isn't worth the complexity right now.

4. **NOT Building section stale**: The plan's NOT Building section still listed "Auth
   on the scan endpoint" as out of scope, but Task 13 (added during adversarial review)
   implements admin-only auth. The implementation follows Task 13, not the stale note.

## Issues Encountered

- **DooD networking**: Integration tests couldn't connect to postgres via localhost:5433.
  Resolved with `docker network connect tome_default coder-john-dev`. Created Linear
  issue UNK-87 for a durable Proxmox-hosted dev database.

## Tests Written

| Test File | Tests | Coverage |
|---|---|---|
| `config.rs` | 1 new | ingestion_database_url fallback + format_priority parsing |
| `ingestion_job.rs` | 2 (integration) | Job lifecycle, skipped + failed status |
| `format_filter.rs` | 7 | Single/multi format, case insensitive, no match, custom priority |
| `path_template.rs` | 9 | Render, sanitize, collision, heuristic parsing |
| `copier.rs` | 3 | Hash correctness, copy + verify, empty file |
| `quarantine.rs` | 2 | Move + sidecar, collision handling |
| `cleanup.rs` | 3 | Files + dirs removal, missing file, non-empty dir preserved |
| `watcher.rs` | 2 | File detection, cancel shutdown |
| `ingestion.rs` (route) | 1 | 401 without auth |

## Next Steps

- [ ] Code review via `/code-review`
- [ ] Create PR via `/prp-pr`
