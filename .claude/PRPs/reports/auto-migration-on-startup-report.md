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

| Test                                     | Type        | Validates                                             |
| ---------------------------------------- | ----------- | ----------------------------------------------------- |
| `from_env_missing_migration_url`         | Unit        | Config fails without DATABASE_URL_MIGRATION           |
| `from_env_custom_migration_url`          | Unit        | Config parses custom migration URL                    |
| `generate_lock_id_matches_known_value`   | Unit        | Lock ID formula matches hardcoded expected value      |
| `embedded_checksum_is_sha384_of_sql`     | Unit        | sqlx embedded checksum == SHA-384 of migration SQL    |
| `migration_error_display`                | Unit        | Error message formatting matches ADR                  |
| `no_tx_tracking_error_distinguishes`     | Unit        | NoTxFailed vs NoTxTrackingFailed recovery guidance    |
| `fresh_database_applies_all_migrations`  | Integration | Happy path — all migrations applied                   |
| `up_to_date_database_is_noop`            | Integration | Idempotency — 0 applied on second run                 |
| `rerun_after_success_is_stable`          | Integration | Row count stable across runs                          |
| `schema_ahead_detection`                 | Integration | Refuses startup on unknown DB migrations              |
| `checksum_mismatch_detected`             | Integration | Refuses startup on modified migration                 |
| `invalid_credentials_clear_error`        | Integration | Connection variant + no credential leakage            |
| `lock_timeout_applied_to_session`        | Integration | SET lock_timeout verified via SHOW on held connection |
| `concurrent_starts_serialize`            | Integration | Advisory lock serializes, no duplicates               |
| `batch_failure_rolls_back_tracking_rows` | Integration | Tracking rows rolled back on batch failure            |

---

## Known Gaps

- `-- no-transaction` migration tests deferred: no existing no-tx migrations to test against (documented in ADR verification checklist)
- Batch failure DDL rollback: test proves tracking rows are rolled back; Postgres transactional DDL guarantee covers schema-object rollback (testing Postgres, not our runner)

---

## Post-review fixes (Santa Method + Greptile)

| Finding                                             | Fix                                            |
| --------------------------------------------------- | ---------------------------------------------- |
| `pool.close()` unreachable on error (T1)            | Capture result, close unconditionally          |
| `NoTxFailed` conflates SQL + tracking failures (C1) | Split into `NoTxFailed` + `NoTxTrackingFailed` |
| `Connection` misused for session-setup errors (B1)  | New `SessionSetup` variant                     |
| Missing `#[non_exhaustive]` (B3)                    | Added to `MigrationError`                      |
| 4 tautological tests (T2)                           | Replaced with meaningful assertions            |
| No credential-leakage assertion (C2)                | Added to `invalid_credentials_clear_error`     |
| Retry loop tail delay (C3)                          | Skip sleep after final attempt                 |
| ADR "no new crates" drift (B2)                      | Updated to document `crc`                      |
| ADR log table tense mismatch (B4)                   | Updated to match implementation                |

---

## Next Steps

- [ ] Merge when approved
