# Implementation Report

**Plan**: `.claude/PRPs/plans/auto-migration-on-startup.plan.md`
**Branch**: `feat/auto-migration-on-startup`
**Date**: 2026-05-27
**Status**: COMPLETE

---

## Summary

Added always-on database auto-migration to Reverie's startup sequence. A custom batch runner wraps sqlx's embedded `Migrator`, executing all pending transactional migrations in a single `BEGIN`/`COMMIT` (all-or-nothing), then running `-- no-transaction` migrations individually. Includes schema-ahead detection, checksum verification, advisory lock concurrency control, and an ephemeral migration pool that drops before runtime pools are created.

---

## Assessment vs Reality

| Metric     | Predicted | Actual | Reasoning                                                                                                                                                       |
| ---------- | --------- | ------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Complexity | HIGH      | HIGH   | Multi-statement SQL needed `raw_sql`, connection management needed single-connection-throughout pattern, `#[sqlx::test]` behavior required `migrations = false` |
| Confidence | HIGH      | HIGH   | ADR spec was comprehensive; sqlx source research confirmed all API details                                                                                      |

**Deviations from plan:**

- Used `sqlx::raw_sql()` instead of `sqlx::query()` for migration SQL execution — multi-statement migrations trigger Postgres extended-query-protocol error "cannot insert multiple commands into a prepared statement"
- Used manual `BEGIN`/`COMMIT` on a single acquired connection instead of `pool.begin()` — `pool.begin()` acquires a separate connection that doesn't share the advisory lock or `lock_timeout` session setting
- Used `#[sqlx::test(migrations = false)]` instead of bare `#[sqlx::test]` for fresh-database tests — sqlx auto-applies migrations by default even without explicit `migrations` arg
- `lock_timeout` test restructured to verify constant value + successful execution rather than `SHOW lock_timeout` (session-scoped setting not observable on a different pool connection)

---

## Tasks Completed

| #   | Task                                   | File                    | Status |
| --- | -------------------------------------- | ----------------------- | ------ |
| 1   | Add `migration_database_url` to Config | `backend/src/config.rs` | Done   |
| 2   | Add MigrationError and types           | `backend/src/db.rs`     | Done   |
| 3   | Implement `run_migrations`             | `backend/src/db.rs`     | Done   |
| 4   | Wire migration into startup            | `backend/src/lib.rs`    | Done   |
| 5   | Write unit tests                       | `backend/src/db.rs`     | Done   |
| 6   | Write integration tests                | `backend/src/db.rs`     | Done   |
| 7   | Update backend/CLAUDE.md               | `backend/CLAUDE.md`     | Done   |
| 8   | Full validation suite                  | —                       | Done   |

---

## Validation Results

| Check      | Result | Details                                                                                 |
| ---------- | ------ | --------------------------------------------------------------------------------------- |
| Format     | Pass   | `cargo fmt --check` clean                                                               |
| Clippy     | Pass   | 0 errors, 0 warnings                                                                    |
| Unit tests | Pass   | 648 passed, 1 ignored                                                                   |
| Build      | Pass   | Compiled successfully                                                                   |
| sqlx cache | Pass   | No drift (runtime queries only)                                                         |
| Grep guard | Pass   | No violations (raw_sql not in guard pattern, query calls covered by db.rs file-blanket) |

---

## Files Changed

| File                                              | Action | Lines                    |
| ------------------------------------------------- | ------ | ------------------------ |
| `backend/Cargo.toml`                              | UPDATE | +1 (crc dep)             |
| `backend/Cargo.lock`                              | UPDATE | +1                       |
| `backend/src/config.rs`                           | UPDATE | +38                      |
| `backend/src/db.rs`                               | UPDATE | +525                     |
| `backend/src/lib.rs`                              | UPDATE | +13                      |
| `backend/src/auth/oidc.rs`                        | UPDATE | +4                       |
| `backend/src/test_support.rs`                     | UPDATE | +1                       |
| `backend/src/services/enrichment/orchestrator.rs` | UPDATE | +1                       |
| `backend/src/services/enrichment/queue.rs`        | UPDATE | +1                       |
| `backend/src/services/ingestion/orchestrator.rs`  | UPDATE | +1                       |
| `backend/src/services/writeback/orchestrator.rs`  | UPDATE | +1                       |
| `backend/src/services/writeback/queue.rs`         | UPDATE | +1                       |
| `backend/CLAUDE.md`                               | UPDATE | +19/-3                   |
| `adr/2026-05-26-auto-migration-on-startup.md`     | UPDATE | status proposed→accepted |
| `adr/README.md`                                   | UPDATE | status proposed→accepted |

---

## Tests Written

| Test                                    | Type        | Validates                                    |
| --------------------------------------- | ----------- | -------------------------------------------- |
| `from_env_missing_migration_url`        | Unit        | Config fails without DATABASE_URL_MIGRATION  |
| `from_env_custom_migration_url`         | Unit        | Config parses custom migration URL           |
| `generate_lock_id_matches_sqlx`         | Unit        | Lock ID formula matches sqlx-postgres v0.8.6 |
| `checksum_matches_sha384`               | Unit        | SHA-384 produces 48-byte output              |
| `migration_error_display`               | Unit        | Error message formatting matches ADR         |
| `fresh_database_applies_all_migrations` | Integration | Happy path — all migrations applied          |
| `up_to_date_database_is_noop`           | Integration | Idempotency — 0 applied on second run        |
| `rerun_after_success_is_stable`         | Integration | Row count stable across runs                 |
| `schema_ahead_detection`                | Integration | Refuses startup on unknown DB migrations     |
| `checksum_mismatch_detected`            | Integration | Refuses startup on modified migration        |
| `invalid_credentials_clear_error`       | Integration | Clear auth error (Connection variant)        |
| `lock_timeout_constant_is_30s`          | Integration | Constant value + successful execution        |
| `concurrent_starts_serialize`           | Integration | Advisory lock serializes, no duplicates      |

---

## Known Gaps

- `lock_timeout` observability: the SET runs on a private connection; test verifies constant value and successful execution but cannot `SHOW lock_timeout` externally
- `-- no-transaction` migration tests deferred: no existing no-tx migrations to test against (documented in ADR verification checklist)
- Batch failure atomicity test deferred: cannot inject bad SQL into embedded `Migrator` without constructing doc-hidden types; `rerun_after_success_is_stable` covers idempotency instead

---

## Next Steps

- [ ] Review implementation
- [ ] Create PR
- [ ] Merge when approved
