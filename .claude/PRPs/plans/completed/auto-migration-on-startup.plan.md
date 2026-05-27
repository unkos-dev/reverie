# Feature: Auto-migrate database on startup

## Summary

Add always-on database auto-migration to Reverie's startup sequence. A custom batch runner wraps sqlx's embedded `Migrator`, executing all pending transactional migrations in a single `BEGIN`/`COMMIT` (all-or-nothing), then running `-- no-transaction` migrations individually. Includes schema-ahead detection, checksum verification, advisory lock concurrency control, and an ephemeral migration pool that drops before runtime pools are created.

## User Story

As a self-hosted Reverie operator
I want database migrations to apply automatically when I pull a new image and restart
So that I never need to run manual `sqlx migrate run` or suffer restart loops from schema drift

## Problem Statement

Operators must manually run `sqlx migrate run` before (re)starting Reverie. The staging instance entered a restart loop when a new image expected a `settings` table the database didn't have. Self-hosted apps in the same category (Gitea, Immich, Kavita) auto-migrate transparently.

## Solution Statement

Insert an ephemeral migration step between tracing init and runtime pool creation in `run()`. The migration runner connects via a dedicated schema-owner DSN (`DATABASE_URL_MIGRATION`), acquires an advisory lock, applies pending migrations in a batch transaction, verifies checksums and schema version, then drops the connection before any runtime pool exists.

## Metadata

| Field            | Value                                                        |
| ---------------- | ------------------------------------------------------------ |
| Type             | NEW_CAPABILITY                                               |
| Complexity       | HIGH                                                         |
| Systems Affected | backend/src/db.rs, backend/src/config.rs, backend/src/lib.rs |
| Dependencies     | sqlx 0.8.6 (already present, `migrate` feature enabled)      |
| Estimated Tasks  | 8                                                            |
| ADR              | `adr/2026-05-26-auto-migration-on-startup.md` (accepted)     |
| Linear           | Implementation PR, child of ADR work                         |

---

## UX Design

### Before State

```text
Operator pulls new image → restarts container → app crashes:
  "relation 'settings' does not exist" → restart loop

Recovery: operator must SSH in, run:
  DATABASE_URL=postgres://reverie:reverie@... sqlx migrate run
  Then restart again.
```

### After State

```text
Operator pulls new image → restarts container → startup logs:
  INFO applied 3 pending migrations (142ms)
  → app serves requests

Failure: startup logs:
  ERROR migration batch failed: ... pin the previous image tag
  → container exits, database untouched, old image works
```

---

## Mandatory Reading

**CRITICAL: Implementation agent MUST read these files before starting any task:**

| Priority | File                                          | Lines                             | Why Read This                                                           |
| -------- | --------------------------------------------- | --------------------------------- | ----------------------------------------------------------------------- |
| P0       | `adr/2026-05-26-auto-migration-on-startup.md` | all                               | The spec — every design decision, logging table, verification checklist |
| P0       | `backend/src/config.rs`                       | 35-127, 318-334, 376-400, 762-960 | Config struct, ConfigError, from_source parsing, test helpers           |
| P0       | `backend/src/db.rs`                           | all                               | Pool init patterns, acquire_with_rls, existing test                     |
| P0       | `backend/src/lib.rs`                          | 219-290                           | run() startup sequence — the insertion point                            |
| P1       | `backend/src/error/mod.rs`                    | 54-121                            | thiserror pattern for error enums                                       |
| P1       | `backend/src/services/writeback/error.rs`     | 13-44                             | Subsystem error pattern with #[from] sqlx::Error                        |
| P2       | `backend/CLAUDE.md`                           | all                               | Current migration docs to update                                        |

**External Documentation:**

| Source                                                                                                             | Section             | Why Needed                                               |
| ------------------------------------------------------------------------------------------------------------------ | ------------------- | -------------------------------------------------------- |
| [sqlx Migrator source v0.8.6](https://github.com/launchbadge/sqlx/blob/v0.8.6/sqlx-core/src/migrate/migrator.rs)   | Struct fields       | `migrations: Cow<'static, [Migration]>`, `iter()` method |
| [sqlx Migration source v0.8.6](https://github.com/launchbadge/sqlx/blob/v0.8.6/sqlx-core/src/migrate/migration.rs) | Struct fields       | `version`, `sql`, `checksum`, `no_tx`, `migration_type`  |
| [sqlx Postgres migrate impl v0.8.6](https://github.com/launchbadge/sqlx/blob/v0.8.6/sqlx-postgres/src/migrate.rs)  | Table DDL + lock ID | `_sqlx_migrations` schema, `generate_lock_id` formula    |

---

## Patterns to Mirror

**CONFIG_REQUIRED_VAR:**

```rust
// SOURCE: backend/src/config.rs:377-378
let database_url =
    get("DATABASE_URL").ok_or_else(|| ConfigError::MissingVar("DATABASE_URL".into()))?;
```

**POOL_INIT:**

```rust
// SOURCE: backend/src/db.rs:36-41
pub async fn init_pool(database_url: &str, max_connections: u32) -> Result<PgPool, sqlx::Error> {
    PgPoolOptions::new()
        .max_connections(max_connections)
        .connect(database_url)
        .await
}
```

**ERROR_PROPAGATION_IN_RUN:**

```rust
// SOURCE: backend/src/lib.rs:274-276
let pool = db::init_pool(&config.database_url, config.db_max_connections)
    .await
    .map_err(|e| anyhow::anyhow!("failed to connect to database: {e}"))?;
```

**SUBSYSTEM_ERROR_ENUM:**

```rust
// SOURCE: backend/src/services/writeback/error.rs:13-44
#[derive(Debug, thiserror::Error)]
pub enum WritebackError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("sqlx: {0}")]
    Db(#[from] sqlx::Error),
    #[error("writeback job {0} not found")]
    JobNotFound(uuid::Uuid),
}
```

**TRACING_STRUCTURED:**

```rust
// SOURCE: backend/src/lib.rs:261
tracing::warn!(error = %err, "configured log level is unparsable; ...");
// SOURCE: backend/src/services/writeback/events.rs:17-24
tracing::info!(event = "writeback_complete", %manifestation_id, reason, "writeback: complete");
```

**CONFIG_TEST_HELPERS:**

```rust
// SOURCE: backend/src/config.rs:762-820
fn env_for(vars: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> + use<> { ... }
const BASE_VARS: &[(&str, &str)] = &[("DATABASE_URL", "..."), ...];
fn with_overrides(extra: &[(&str, &str)]) -> Vec<(String, String)> { ... }
fn without_keys(keys: &[&str]) -> Vec<(String, String)> { ... }
```

**SQLX_TEST:**

```rust
// SOURCE: backend/src/db.rs:114-126
#[sqlx::test(migrations = "./migrations")]
async fn acquire_with_rls_sets_session_variable(pool: PgPool) {
    let user_id = uuid::Uuid::new_v4();
    let mut tx = acquire_with_rls(&pool, user_id).await.unwrap();
    // ...
}
```

---

## Files to Change

| File                                 | Action | Justification                                                         |
| ------------------------------------ | ------ | --------------------------------------------------------------------- |
| `backend/src/config.rs`              | UPDATE | Add `migration_database_url: String` field + parsing + tests          |
| `backend/src/db.rs`                  | UPDATE | Add `MigrationError`, `run_migrations()`, advisory lock, batch runner |
| `backend/src/lib.rs`                 | UPDATE | Wire migration call into startup between tracing init and pool init   |
| `.github/sqlx-runtime-allowlist.txt` | UPDATE | Add migration runner `sqlx::query(...)` call sites (DDL carve-out)    |
| `backend/CLAUDE.md`                  | UPDATE | Replace manual migration instructions with auto-migration docs        |

---

## NOT Building (Scope Limits)

- Exponential backoff on restart-loop (noted in ADR "More Information", explicitly out of scope)
- Opt-out env var `REVERIE_AUTO_MIGRATE` (rejected in ADR)
- Extractable crate (UNK-299 tracks future evaluation)
- Lock/timeout strategy beyond interim 30s defaults (UNK-296)
- Logging conventions beyond interim levels (UNK-297)

---

## sqlx Migration API Reference (v0.8.6)

Critical implementation details from research:

| API                              | Detail                                                                                                                  |
| -------------------------------- | ----------------------------------------------------------------------------------------------------------------------- |
| `sqlx::migrate!("./migrations")` | Returns `Migrator` with embedded migrations. Can assign to `static`.                                                    |
| `Migrator.iter()`                | Returns `slice::Iter<'_, Migration>`. Idiomatic access path.                                                            |
| `Migration.version`              | `i64` — the timestamp prefix                                                                                            |
| `Migration.description`          | `Cow<'static, str>` — the filename suffix                                                                               |
| `Migration.sql`                  | `Cow<'static, str>` — the SQL content                                                                                   |
| `Migration.checksum`             | `Cow<'static, [u8]>` — SHA-384 of SQL bytes                                                                             |
| `Migration.no_tx`                | `bool` — `true` means `-- no-transaction` marker present                                                                |
| `Migration.migration_type`       | `MigrationType` — use `.is_up_migration()` to filter forward-only                                                       |
| Lock ID formula                  | `0x3d32ad9e_i64 * (CRC_32_ISO_HDLC(db_name) as i64)`                                                                    |
| `_sqlx_migrations` columns       | `version BIGINT PK, description TEXT, installed_on TIMESTAMPTZ, success BOOLEAN, checksum BYTEA, execution_time BIGINT` |
| Checksum algo                    | `sha2::Sha384::digest(migration.sql.as_bytes())` — sha2 is transitive dep                                               |
| Advisory lock type               | Session-level: `pg_advisory_lock(id)` / `pg_advisory_unlock(id)`                                                        |

**Semver warning**: `Migrator.migrations` and all `Migration` fields are `#[doc(hidden)]` and semver-exempt in 0.8.6. Using `iter()` is slightly safer. Must verify on sqlx version bumps (noted in ADR consequences).

---

## Step-by-Step Tasks

### Task 1: UPDATE `backend/src/config.rs` — add migration_database_url

- **ACTION**: Add `migration_database_url: String` field to `Config` struct. Add parsing in `from_source`. Update `BASE_VARS` and tests.
- **IMPLEMENT**:
  - Add field at `config.rs:~88` (after `ingestion_database_url`):

    ```rust
    /// Migration DSN (`DATABASE_URL_MIGRATION`, required). Schema-owner
    /// credentials for the ephemeral migration pool. Bypasses RLS.
    pub migration_database_url: String,
    ```

  - Add parsing at `config.rs:~400` (after ingestion URL parsing):

    ```rust
    let migration_database_url = get("DATABASE_URL_MIGRATION")
        .ok_or_else(|| ConfigError::MissingVar("DATABASE_URL_MIGRATION".into()))?;
    ```

  - Add to struct initialization in `from_source` return.
  - Update `BASE_VARS` to include `("DATABASE_URL_MIGRATION", "postgres://test@localhost/reverie_dev")`.

- **MIRROR**: `config.rs:377-378` (DATABASE_URL required var pattern)
- **GOTCHA**: `BASE_VARS` is used by all config tests — adding the new required var here ensures all existing tests continue passing. Missing it will break every config test.
- **TESTS TO ADD**:
  - `from_env_missing_migration_url` — verify `ConfigError::MissingVar` when `DATABASE_URL_MIGRATION` absent
  - `from_env_custom_migration_url` — verify custom value parsed correctly
- **VALIDATE**: `cargo test -p reverie-api config::tests -- --nocapture`

### Task 2: UPDATE `backend/src/db.rs` — add MigrationError and types

- **ACTION**: Define `MigrationError` enum, `MigrationReport` struct, and constants.
- **IMPLEMENT**:

  ```rust
  #[derive(Debug, thiserror::Error)]
  pub enum MigrationError {
      #[error("failed to connect to migration database: {0}")]
      Connection(#[source] sqlx::Error),

      #[error("migration batch failed: {0}")]
      BatchFailed(#[source] sqlx::Error),

      #[error("no-transaction migration failed: {version} ({name})")]
      NoTxFailed {
          version: i64,
          name: String,
          #[source]
          source: sqlx::Error,
      },

      #[error(
          "database schema (migration {version}) is newer than this application version \
           — upgrade the image or roll back the database manually"
      )]
      SchemaAhead { version: i64 },

      #[error(
          "checksum mismatch for migration {version} ({name}) \
           — migration file was modified after application"
      )]
      ChecksumMismatch { version: i64, name: String },

      #[error(
          "failed to acquire migration lock after {attempts} attempts \
           — another instance may be running migrations"
      )]
      LockTimeout { attempts: u32 },
  }

  #[derive(Debug)]
  pub struct MigrationReport {
      pub applied: usize,
      pub elapsed_ms: u128,
  }

  const LOCK_TIMEOUT_SQL: &str = "SET lock_timeout = '30s'";
  const LOCK_RETRY_ATTEMPTS: u32 = 10;
  const LOCK_RETRY_INTERVAL: std::time::Duration = std::time::Duration::from_secs(3);
  const LOCK_MAGIC: i64 = 0x3d32ad9e;
  ```

- **MIRROR**: `backend/src/services/writeback/error.rs:13-44` (subsystem error enum)
- **VALIDATE**: `cargo check -p reverie-api`

### Task 3: UPDATE `backend/src/db.rs` — implement run_migrations

- **ACTION**: Implement the core `run_migrations` function with all ADR-specified behavior.
- **ARCHITECTURE**: Two-function split for testability:
  - `pub async fn run_migrations(url: &str) -> Result<MigrationReport, MigrationError>` — public API per ADR. Creates ephemeral pool, delegates to inner, drops pool.
  - `async fn run_migrations_inner(pool: &PgPool) -> Result<MigrationReport, MigrationError>` — private implementation. All logic lives here. Integration tests call this directly with the `#[sqlx::test]`-injected pool.
  - Expose inner via `#[cfg(test)] pub` or a `pub(crate)` test helper — either pattern works; prefer `#[cfg(test)]` to avoid leaking test-only API.
- **IMPLEMENT**: `run_migrations_inner(pool: &PgPool)`:
  1. **Set lock_timeout**: Execute `SET lock_timeout = '30s'` on the connection
  2. **Acquire advisory lock**: Query `SELECT current_database()`, compute lock ID via `crc32(db_name) * 0x3d32ad9e`, try `SELECT pg_try_advisory_lock($1)` in bounded retry loop (10 attempts × 3s)
  3. **Ensure `_sqlx_migrations` table**: `CREATE TABLE IF NOT EXISTS _sqlx_migrations (...)` — match sqlx's exact DDL
  4. **Query applied migrations**: `SELECT version, checksum FROM _sqlx_migrations WHERE success = true ORDER BY version`
  5. **Schema-ahead detection**: Check if any applied version is not in embedded migrations → `MigrationError::SchemaAhead`
  6. **Checksum verification**: For each applied migration, compare stored checksum against `sha2::Sha384::digest(migration.sql.as_bytes())` → `MigrationError::ChecksumMismatch`
  7. **Partition pending**: Split unapplied up-migrations into `tx_pending` (no_tx == false) and `no_tx_pending` (no_tx == true), both version-sorted
  8. **Batch transaction**: `BEGIN` → for each tx migration: execute SQL, insert into `_sqlx_migrations` with `success = true` → `COMMIT`. On error: rollback is automatic, return `MigrationError::BatchFailed`
  9. **No-tx migrations**: For each no_tx migration (after batch commits): execute SQL outside transaction, insert into `_sqlx_migrations`. On error: return `MigrationError::NoTxFailed` with distinct recovery guidance
  10. **Release lock**: `SELECT pg_advisory_unlock($1)`
  11. **Return**: `MigrationReport { applied, elapsed_ms }`
  - `run_migrations(url)` wrapper:
  1. **Connect**: `PgPoolOptions::new().max_connections(1).connect(url).await` → ephemeral pool
  2. **Delegate**: `run_migrations_inner(&pool).await`
  3. **Drop pool**: Pool goes out of scope on return, connections closed
- **MIRROR**: `db.rs:36-41` (pool creation), `db.rs:62-78` (after_connect for SQL execution)
- **GOTCHA**: Use `sqlx::query(...)` (runtime, not macro) for all migration SQL — DDL and meta-queries can't be compile-time checked. This is a documented carve-out class per `backend/CLAUDE.md` conventions (DDL and dynamic SQL).
- **GOTCHA**: `Migration.checksum` is raw SHA-384 bytes. DB stores as `BYTEA`. Compare directly — no hex encoding needed.
- **GOTCHA**: Record `execution_time` in nanoseconds (matching sqlx convention) as `BIGINT`.
- **GOTCHA**: Only process `migration.migration_type.is_up_migration()` — skip ReversibleDown entries.
- **ALLOWLIST**: Update `.github/sqlx-runtime-allowlist.txt` with new `sqlx::query(...)` call sites in `db.rs`. CI grep-guard will reject the PR without this. All migration SQL is DDL-class carve-out per `backend/CLAUDE.md` conventions.
- **VALIDATE**: `cargo check -p reverie-api`

### Task 4: UPDATE `backend/src/lib.rs` — wire migration into startup

- **ACTION**: Add migration call between tracing init (line 272) and pool init (line 274).
- **IMPLEMENT**: Insert after the operator contact warning block:

  ```rust
  // Auto-migrate database schema
  let migration_report = db::run_migrations(&config.migration_database_url)
      .await
      .map_err(|e| anyhow::anyhow!("database migration failed: {e}"))?;
  if migration_report.applied > 0 {
      tracing::info!(
          count = migration_report.applied,
          elapsed_ms = migration_report.elapsed_ms,
          "applied pending migrations"
      );
  } else {
      tracing::debug!("database schema is up to date");
  }
  ```

- **MIRROR**: `lib.rs:274-276` (error propagation pattern)
- **GOTCHA**: Must come AFTER tracing init (line 258) so migration logs are captured. Must come BEFORE runtime pool init (line 274) so schema is ready.
- **GOTCHA**: Add `mod migration;` or update `pub mod db` — ensure `run_migrations` is accessible. Since it's in db.rs, just call `db::run_migrations(...)`.
- **VALIDATE**: `cargo check -p reverie-api`

### Task 5: UPDATE `backend/src/db.rs` — write unit tests

- **ACTION**: Add tests for advisory lock ID generation, checksum verification logic, and error variants.
- **IMPLEMENT**:
  - `generate_lock_id_matches_sqlx` — verify lock ID formula produces expected value for known database name
  - `checksum_matches_sqlx` — verify SHA-384 digest of known SQL matches expected bytes
  - `migration_error_display` — verify error messages match ADR-specified strings
- **MIRROR**: `db.rs:114-126` (existing sqlx::test pattern)
- **VALIDATE**: `cargo test -p reverie-api db::tests`

### Task 6: UPDATE `backend/src/db.rs` — write integration tests

- **ACTION**: Add `#[sqlx::test]` integration tests covering the ADR verification checklist. All integration tests call `run_migrations_inner(&pool)` directly (the `#[cfg(test)] pub` inner function from Task 3).
- **TEST ARCHITECTURE**:
  - **Fresh database test**: Use `#[sqlx::test]` WITHOUT `migrations` arg → bare pool, no schema applied. Call `run_migrations_inner(&pool)` → verify all migrations applied and report count matches total up-migrations.
  - **Already-migrated tests**: Use `#[sqlx::test(migrations = "./migrations")]` → sqlx applies all migrations before injecting pool. Then manipulate `_sqlx_migrations` for negative tests.
  - **URL-based tests** (invalid credentials): Call the public `run_migrations(url)` with a bad URL — this is the only test that exercises the outer function.
- **IMPLEMENT**:
  - `fresh_database_applies_all_migrations` — `#[sqlx::test]` (no migrations arg), call `run_migrations_inner`, verify report.applied == expected count
  - `up_to_date_database_is_noop` — `#[sqlx::test(migrations = ...)]`, call `run_migrations_inner`, verify applied == 0
  - `batch_failure_leaves_no_partial_state` — `#[sqlx::test]` (no migrations arg), apply some migrations, then inject a bad migration SQL mid-batch. Verify `_sqlx_migrations` row count unchanged after error (the core all-or-nothing guarantee).
  - `schema_ahead_detection` — `#[sqlx::test(migrations = ...)]`, insert fake future row into `_sqlx_migrations`, verify `SchemaAhead` error
  - `checksum_mismatch_detected` — `#[sqlx::test(migrations = ...)]`, update stored checksum to wrong value, verify `ChecksumMismatch` error
  - `invalid_credentials_clear_error` — call public `run_migrations("postgres://bad:bad@localhost:5433/nonexistent")`, verify `Connection` error
  - `lock_timeout_effective` — `#[sqlx::test]` (no migrations arg), call `run_migrations_inner`, then `SHOW lock_timeout` on same pool, verify `30s`
  - `concurrent_starts_serialize` — `#[sqlx::test]` (no migrations arg), spawn two `run_migrations_inner` tasks on the SAME pool concurrently (two connections), verify both succeed, no duplicate `_sqlx_migrations` rows
- **MIRROR**: `db.rs:114` (sqlx::test pattern), `lib.rs:462` (test with real pools)
- **GOTCHA**: `#[sqlx::test]` without `migrations` arg still creates `_sqlx_migrations` table? No — sqlx only creates it during migration. The custom runner's `CREATE TABLE IF NOT EXISTS` handles this.
- **GOTCHA**: For `batch_failure_leaves_no_partial_state`, need a way to inject bad SQL. Options: (a) insert a `_sqlx_migrations` row that makes the runner think migration N is already applied, then corrupt it so migration N+1 depends on something missing; (b) more practically, test against `run_migrations_inner` with a custom `Migrator` if possible. Since `Migrator` fields are public (though doc-hidden), constructing a test `Migrator` with a bad migration is feasible.
- **VALIDATE**: `cargo test -p reverie-api db::tests`

### Task 7: UPDATE `backend/CLAUDE.md` — document auto-migration

- **ACTION**: Replace manual migration instructions with auto-migration documentation.
- **IMPLEMENT**:
  - Update line 25-26: replace `Run migrations as schema owner: DATABASE_URL=... sqlx migrate run` with auto-migration description
  - Update line 39 (postgres:18 upgrade note): replace manual migrate command
  - Add `DATABASE_URL_MIGRATION` to the roles table or a new "Migration Connection" section
  - Document the MigrationError variants and recovery guidance
  - Note that `#[sqlx::test]` still uses its own internal migrator (no change to test workflow)
- **VALIDATE**: Visual review — ensure no stale references to manual `sqlx migrate run`

### Task 8: Run validation suite

- **ACTION**: Full validation pass per ADR verification checklist.
- **VALIDATE**:

  ```bash
  cargo fmt --check
  cargo clippy --workspace --all-targets --locked -- -D warnings
  cargo test -p reverie-api
  DATABASE_URL=postgres://reverie:reverie@localhost:5433/reverie_dev cargo sqlx prepare --workspace --check
  ```

- **GOTCHA**: `cargo sqlx prepare` must be re-run if any compile-time checked queries changed. Migration runner uses runtime queries only, so the cache should be unaffected — but verify.

---

## Testing Strategy

### Tests to Write

| Test                                    | Type        | Validates                                                    |
| --------------------------------------- | ----------- | ------------------------------------------------------------ |
| `from_env_missing_migration_url`        | Unit        | Config fails without DATABASE_URL_MIGRATION                  |
| `from_env_custom_migration_url`         | Unit        | Config parses custom migration URL                           |
| `generate_lock_id_matches_sqlx`         | Unit        | Lock ID formula correctness                                  |
| `checksum_matches_sqlx`                 | Unit        | SHA-384 checksum correctness                                 |
| `migration_error_display`               | Unit        | Error message formatting                                     |
| `fresh_database_applies_all_migrations` | Integration | Happy path — all migrations applied (no `migrations` arg)    |
| `up_to_date_database_is_noop`           | Integration | Idempotency — no-op on second run                            |
| `batch_failure_leaves_no_partial_state` | Integration | All-or-nothing — row count unchanged on failure              |
| `schema_ahead_detection`                | Integration | Refuses startup on unknown DB migrations                     |
| `checksum_mismatch_detected`            | Integration | Refuses startup on modified migration                        |
| `invalid_credentials_clear_error`       | Integration | Clear auth error, not generic (uses public `run_migrations`) |
| `lock_timeout_effective`                | Integration | lock_timeout = 30s on connection                             |
| `concurrent_starts_serialize`           | Integration | Advisory lock serializes, no duplicates                      |

### Edge Cases Checklist (from ADR verification)

- [x] Fresh database — all migrations applied
- [x] Up-to-date database — noop
- [x] Migration failure — rollback, no partial state (`batch_failure_leaves_no_partial_state` test)
- [x] Schema ahead — clear error with version
- [x] Missing env var — clear error naming the var
- [x] Invalid credentials — auth error, not generic
- [x] Lock timeout — bounded, not indefinite hang
- [x] Concurrent starts — serialized, no duplicates
- [x] Ephemeral pool — dropped before runtime pools
- [x] Checksum mismatch — clear error naming the migration
- [ ] `-- no-transaction` migration — runs after batch (deferred: no existing no-tx migrations to test against; add test when first no-tx migration is introduced)
- [ ] `-- no-transaction` failure — distinct error/recovery (same deferral)

---

## Validation Commands

### Level 1: STATIC_ANALYSIS

```bash
cargo fmt --check && cargo clippy --workspace --all-targets --locked -- -D warnings
```

### Level 2: UNIT_TESTS

```bash
cargo test -p reverie-api config::tests db::tests
```

### Level 3: FULL_SUITE

```bash
cargo test -p reverie-api
```

### Level 4: DATABASE_VALIDATION

```bash
DATABASE_URL=postgres://reverie:reverie@localhost:5433/reverie_dev cargo sqlx prepare --workspace --check
```

### Level 5: MANUAL_VALIDATION

1. `docker compose down && docker volume rm reverie_pgdata && docker compose up -d` (fresh DB)
2. Run init-roles: `docker cp docker/init-roles.sql reverie-postgres:/tmp/init-roles.sql && docker exec reverie-postgres psql -U reverie -d reverie_dev -f /tmp/init-roles.sql`
3. Set `DATABASE_URL_MIGRATION=postgres://reverie:reverie@localhost:5433/reverie_dev`
4. `cargo run` — verify INFO log "applied N pending migrations"
5. `cargo run` again — verify DEBUG log "database schema is up to date"
6. Insert fake future migration row, `cargo run` — verify ERROR with schema-ahead message

---

## Acceptance Criteria

- [ ] All 15 ADR verification checklist items pass (12 tested, 2 deferred with rationale, 1 covered by static analysis)
- [ ] Level 1-4 validation commands pass with exit 0
- [ ] `MigrationError` variants match ADR-specified error messages
- [ ] Logging matches ADR logging table (level + message format)
- [ ] Ephemeral pool confirmed dropped before runtime pool creation
- [ ] No regressions in existing 248 `#[sqlx::test]` tests
- [ ] `backend/CLAUDE.md` updated with no stale manual migration references
- [ ] ADR status flip (proposed→accepted) included in PR

---

## Risks and Mitigations

| Risk                                    | Likelihood | Impact | Mitigation                                                                    |
| --------------------------------------- | ---------- | ------ | ----------------------------------------------------------------------------- |
| sqlx 0.9 breaks `#[doc(hidden)]` fields | LOW        | HIGH   | ADR documents version-bump verification; use `iter()` not direct field access |
| Advisory lock ID drift from sqlx        | LOW        | MED    | Replicate exact formula with test verifying against known value               |
| `_sqlx_migrations` DDL drift            | LOW        | MED    | CREATE TABLE IF NOT EXISTS is idempotent; test against sqlx-created table     |
| Test isolation for concurrent lock test | MED        | MED    | Use two connections to same `#[sqlx::test]` database, not separate DBs        |
| SHA-384 computation mismatch            | LOW        | HIGH   | Unit test comparing our checksum against sqlx's stored value                  |

---

## Notes

- `sha2` crate is a transitive dependency of sqlx — do NOT add it to `[dependencies]` directly. Use `sha2::Sha384` via sqlx's re-export or add the explicit dep only if the transitive path is insufficient. Verify with `cargo tree -i sha2`.
- The `crc` crate (for advisory lock ID) is also a transitive dep of sqlx but may need explicit addition to Cargo.toml — verify with `cargo tree -i crc` at implementation time.
- The no-tx migration test cases are deferred because no `-- no-transaction` migrations exist in the codebase today. The code paths will be implemented but tested when the first no-tx migration is introduced. This is documented in the ADR verification checklist.
