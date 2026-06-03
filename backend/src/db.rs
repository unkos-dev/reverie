//! Database pool factories and the RLS-context acquisition helper.
//!
//! Connection-pool flavours:
//!
//! * [`init_pool`] — vanilla `PgPool` for the primary application
//!   (`reverie_app`) and the ingestion pipeline (`reverie_ingestion`).
//!   No per-connection setup; RLS context is set transaction-locally
//!   by [`acquire_with_rls`] from the user-facing handlers.
//! * [`init_writeback_pool`] — dedicated pool for the writeback worker
//!   that sets `app.system_context = 'writeback'` once at connect time.
//!   The `manifestations_*_system` RLS policies match only on this GUC,
//!   so user-facing handlers (which never set it) cannot cross into the
//!   system policies even by accident.
//! * [`run_migrations`] — ephemeral single-connection pool that applies
//!   pending migrations as the least-privilege `reverie_migrator` role.
//!   Invoked out-of-band by the `reverie migrate` subcommand (the shipped
//!   default), or in-process at startup only when `REVERIE_AUTO_MIGRATE` is
//!   set. The pool is dropped before any runtime pool is created.
//! * [`verify_schema_current`] — read-only schema-divergence check run on
//!   the application pool at startup when `REVERIE_AUTO_MIGRATE` is off.
//!   Holds no migration credential; fail-closed on ahead/behind/never-migrated.
//! * [`acquire_with_rls`] — user-scoped transaction wrapper that injects
//!   `app.current_user_id` via `SET LOCAL`-equivalent
//!   `set_config(..., true)`. Auto-resets on commit/rollback so a
//!   recycled connection does not leak the caller's identity to the next
//!   borrower.

use std::time::Instant;

use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

/// Build a vanilla `PgPool` against `database_url` capped at
/// `max_connections`.
///
/// No per-connection initialisation is run — callers needing RLS context
/// must wrap each user-scoped transaction in [`acquire_with_rls`]; the
/// writeback worker uses [`init_writeback_pool`] to set the
/// `app.system_context` GUC instead.
///
/// # Errors
///
/// Returns the underlying `sqlx::Error` when the pool cannot be opened
/// (DSN parse failure, TLS handshake failure, authentication failure,
/// connection refused, etc.).
pub async fn init_pool(database_url: &str, max_connections: u32) -> Result<PgPool, sqlx::Error> {
    PgPoolOptions::new()
        .max_connections(max_connections)
        .connect(database_url)
        .await
}

/// Build a `reverie_app` pool dedicated to the writeback worker.
///
/// Every connection opened by this pool runs
/// `SELECT set_config('app.system_context', 'writeback', false)` once at
/// connect time, marking it as a system-context caller for the duration
/// of the connection.  The `manifestations_*_system` RLS policies match
/// only when this GUC is set to `'writeback'`, so no other code path
/// (in particular, no user-facing handler that forgets `SET LOCAL
/// app.current_user_id`) can reach those policies.
///
/// # Errors
///
/// Returns the underlying `sqlx::Error` when the pool cannot be opened
/// (DSN parse, TLS handshake, authentication, connection refused) or
/// when the per-connection
/// `SELECT set_config('app.system_context', 'writeback', false)` call
/// fails during the after-connect handshake — typically a transport-
/// level error, since `set_config` is a Postgres builtin requiring
/// no schema objects or elevated permissions.
pub async fn init_writeback_pool(
    database_url: &str,
    max_connections: u32,
) -> Result<PgPool, sqlx::Error> {
    PgPoolOptions::new()
        .max_connections(max_connections)
        .after_connect(|conn, _meta| {
            Box::pin(async move {
                sqlx::query("SELECT set_config('app.system_context', 'writeback', false)")
                    .execute(conn)
                    .await?;
                Ok(())
            })
        })
        .connect(database_url)
        .await
}

/// Acquire a transaction with RLS context set for the given user.
///
/// Uses `set_config('app.current_user_id', ..., true)` where the third
/// argument `true` means "local to current transaction" (equivalent to
/// `SET LOCAL`). The value auto-resets on commit/rollback — safe with
/// connection pools.
///
/// Every user-facing handler that touches RLS-gated tables (works,
/// manifestations, shelves, …) MUST acquire its transaction through this
/// helper. A bare `pool.acquire()` runs without a `current_user_id` and
/// the corresponding RLS policies will reject the read.
///
/// # Errors
///
/// Returns the underlying `sqlx::Error` when the transaction cannot be
/// begun or when the `set_config` call fails (rare — this is the seam
/// the rest of the request relies on, so failures here typically signal
/// pool exhaustion or a connectivity blip).
pub async fn acquire_with_rls(
    pool: &PgPool,
    user_id: uuid::Uuid,
) -> Result<sqlx::Transaction<'_, sqlx::Postgres>, sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT set_config('app.current_user_id', $1::text, true)")
        .bind(user_id.to_string())
        .execute(&mut *tx)
        .await?;
    Ok(tx)
}

// ---------------------------------------------------------------------------
// Migration runner + read-only schema verification
// ---------------------------------------------------------------------------

const LOCK_TIMEOUT_SQL: &str = "SET lock_timeout = '30s'";
const LOCK_RETRY_ATTEMPTS: u32 = 10;
const LOCK_RETRY_INTERVAL: std::time::Duration = std::time::Duration::from_secs(3);
const LOCK_MAGIC: i64 = 0x3d32_ad9e;

/// Outcome of a migration run.
#[derive(Debug)]
pub struct MigrationReport {
    /// Number of migrations applied in this run (0 = already up to date).
    pub applied: usize,
    /// Wall-clock time for the entire migration pass, in milliseconds.
    pub elapsed_ms: u128,
}

/// All failure modes for the migration runner.
///
/// Variants carry enough context for operator-actionable error messages.
/// Five variants wrap the underlying `sqlx::Error` as `#[source]` (not
/// `#[from]`) because the runner converts different `sqlx::Error` sites
/// into semantically distinct variants with different recovery guidance.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum MigrationError {
    /// Ephemeral migration pool could not connect (bad DSN, auth failure,
    /// unreachable host).
    #[error("failed to connect to migration database: {0}")]
    Connection(#[source] sqlx::Error),

    /// Post-connect session initialisation failed (`SET lock_timeout`,
    /// `SELECT current_database`, or advisory-lock acquisition query).
    /// The database is untouched — no migrations were attempted.
    #[error("failed to initialize migration session: {0}")]
    SessionSetup(#[source] sqlx::Error),

    /// A transactional migration failed inside the batch `BEGIN`/`COMMIT`.
    /// The entire batch has been rolled back — database is untouched.
    #[error(
        "migration batch failed: {0} — pin the previous image tag to restore service \
         (database is untouched), then fix forward with a new release"
    )]
    BatchFailed(#[source] sqlx::Error),

    /// A `-- no-transaction` migration's SQL failed after the batch
    /// committed. The migration did NOT apply. Transactional migrations
    /// are already committed — operator must fix the failing SQL and
    /// re-deploy.
    #[error(
        "no-transaction migration failed: {version} ({name}) — transactional migrations \
         already committed; pinning the old image alone does not restore the database. \
         Fix the failing migration SQL, then re-deploy"
    )]
    NoTxFailed {
        /// Timestamp-prefix version of the failed migration.
        version: i64,
        /// Human-readable migration name (filename suffix).
        name: String,
        /// Underlying database error.
        #[source]
        source: sqlx::Error,
    },

    /// A `-- no-transaction` migration's SQL executed successfully but
    /// the tracking INSERT into `_sqlx_migrations` failed. The migration
    /// IS applied — do NOT revert it. The operator must manually insert
    /// the tracking row or the next startup will re-attempt the migration
    /// and likely fail on duplicate objects.
    #[error(
        "no-transaction migration {version} ({name}) applied successfully but tracking \
         record failed to write: {source} — the migration IS applied; do NOT revert it. \
         Manually insert the tracking row or the next startup will re-attempt and fail"
    )]
    NoTxTrackingFailed {
        /// Timestamp-prefix version of the applied migration.
        version: i64,
        /// Human-readable migration name (filename suffix).
        name: String,
        /// Underlying database error from the tracking INSERT.
        #[source]
        source: sqlx::Error,
    },

    /// Database contains a migration version unknown to this binary.
    #[error(
        "database schema (migration {version}) is newer than this application version \
         — upgrade the image or roll back the database manually"
    )]
    SchemaAhead {
        /// The unknown migration version found in `_sqlx_migrations`.
        version: i64,
    },

    /// Database is missing a migration this binary knows about — the schema
    /// is older than the binary (the "forgot to run `reverie migrate`"
    /// case). Surfaced only by the read-only startup verification; the
    /// migration runner applies pending migrations instead of erroring.
    #[error(
        "database schema is behind this application (migration {version} is not applied) \
         — run `reverie migrate` (or set REVERIE_AUTO_MIGRATE=true) before starting the server"
    )]
    SchemaBehind {
        /// The earliest embedded migration version not present in the
        /// database.
        version: i64,
    },

    /// The database has never been migrated — the `_sqlx_migrations`
    /// tracking table does not exist. Distinguished from a divergent schema
    /// so a fresh deployment gets an actionable message instead of a raw
    /// missing-relation SQL error.
    #[error("database is not initialized (no migration history) — run `reverie migrate` first")]
    NotInitialized,

    /// Stored checksum for an applied migration differs from the embedded
    /// migration file. The file was modified after initial application.
    #[error(
        "checksum mismatch for migration {version} ({name}) \
         — migration file was modified after application"
    )]
    ChecksumMismatch {
        /// Version of the mismatched migration.
        version: i64,
        /// Human-readable migration name.
        name: String,
    },

    /// Advisory lock not acquired within the retry budget.
    #[error(
        "failed to acquire migration lock after {attempts} attempts \
         — another instance may be running migrations"
    )]
    LockTimeout {
        /// Number of lock acquisition attempts made.
        attempts: u32,
    },
}

fn generate_lock_id(database_name: &str) -> i64 {
    const CRC_IEEE: crc::Crc<u32> = crc::Crc::<u32>::new(&crc::CRC_32_ISO_HDLC);
    LOCK_MAGIC * i64::from(CRC_IEEE.checksum(database_name.as_bytes()))
}

/// Apply all pending migrations against `url` using an ephemeral
/// single-connection pool. The pool is dropped before this function
/// returns.
///
/// # Errors
///
/// Returns [`MigrationError`] on connection failure, lock timeout,
/// schema-ahead detection, checksum mismatch, or SQL execution failure.
pub async fn run_migrations(url: &str) -> Result<MigrationReport, MigrationError> {
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(url)
        .await
        .map_err(MigrationError::Connection)?;

    let result = run_migrations_inner(&pool).await;
    pool.close().await;
    result
}

/// Verify the database schema matches this binary's embedded migrations,
/// read-only, without holding any migration credential.
///
/// This is the default server-startup gate when `REVERIE_AUTO_MIGRATE` is
/// off: migrations run out-of-band via `reverie migrate`, and the long-lived
/// application process only confirms the schema is current using its own
/// (least-privilege) app pool — which has `SELECT` on `_sqlx_migrations` but
/// no schema-management rights. Fail-closed in BOTH directions:
/// [`MigrationError::SchemaAhead`] when the DB is newer than the binary, and
/// [`MigrationError::SchemaBehind`] when the DB is older (the common
/// "forgot to migrate" case). Refusing on schema-behind is deliberate: the
/// migrate-then-app topology has no legitimate window where the app runs
/// newer than the DB, and the bare-docker path has no compose gating, so
/// this check is the only backstop against silent runtime SQL failures.
///
/// # Errors
///
/// - [`MigrationError::NotInitialized`] if `_sqlx_migrations` does not exist
///   (a never-migrated database).
/// - [`MigrationError::SchemaAhead`] / [`MigrationError::SchemaBehind`] on
///   version divergence.
/// - [`MigrationError::Connection`] if the read fails (auth, connectivity).
pub async fn verify_schema_current(pool: &PgPool) -> Result<(), MigrationError> {
    // Catalog probe FIRST. A never-migrated database has no
    // `_sqlx_migrations` table; without this probe the version SELECT below
    // surfaces a cryptic "relation _sqlx_migrations does not exist" instead
    // of the legible NotInitialized message, defeating the fail-closed goal.
    let table_exists: bool =
        sqlx::query_scalar("SELECT to_regclass('public._sqlx_migrations') IS NOT NULL")
            .fetch_one(pool)
            .await
            .map_err(MigrationError::Connection)?;
    if !table_exists {
        return Err(MigrationError::NotInitialized);
    }

    // Mirror the runner's `WHERE success = true` filter (the runner only
    // ever writes success=true rows, but stay consistent defensively).
    let applied_versions: std::collections::HashSet<i64> =
        sqlx::query_scalar("SELECT version FROM _sqlx_migrations WHERE success = true")
            .fetch_all(pool)
            .await
            .map_err(MigrationError::Connection)?
            .into_iter()
            .collect();

    let migrator = sqlx::migrate!("./migrations");
    let embedded_versions: std::collections::HashSet<i64> = migrator
        .iter()
        .filter(|m| m.migration_type.is_up_migration())
        .map(|m| m.version)
        .collect();

    let (ahead, behind) = diverged_versions(&applied_versions, &embedded_versions);
    if let Some(&version) = ahead.first() {
        return Err(MigrationError::SchemaAhead { version });
    }
    if let Some(&version) = behind.first() {
        return Err(MigrationError::SchemaBehind { version });
    }
    Ok(())
}

/// Pure set comparison of applied vs embedded migration versions. Returns
/// `(ahead, behind)` where `ahead` = applied versions the binary does not
/// know (DB newer) and `behind` = embedded versions not yet applied (DB
/// older). Both are sorted ascending so callers report a deterministic
/// version. This is plain set math — it raises no errors; each caller
/// decides which direction is fatal (the runner applies `behind`, the
/// verifier refuses it).
fn diverged_versions(
    applied: &std::collections::HashSet<i64>,
    embedded: &std::collections::HashSet<i64>,
) -> (Vec<i64>, Vec<i64>) {
    let mut ahead: Vec<i64> = applied.difference(embedded).copied().collect();
    let mut behind: Vec<i64> = embedded.difference(applied).copied().collect();
    ahead.sort_unstable();
    behind.sort_unstable();
    (ahead, behind)
}

/// Inner implementation operating on an already-connected pool.
#[allow(
    clippy::too_many_lines,
    reason = "migration runner threads lock acquisition, schema checks, batch tx, and no-tx passes in sequence; splitting would obscure the linear flow"
)]
async fn run_migrations_inner(pool: &PgPool) -> Result<MigrationReport, MigrationError> {
    let start = Instant::now();
    let migrator = sqlx::migrate!("./migrations");

    let mut conn = pool.acquire().await.map_err(MigrationError::Connection)?;

    // Set lock_timeout to bound DDL waits.
    sqlx::query(LOCK_TIMEOUT_SQL)
        .execute(&mut *conn)
        .await
        .map_err(MigrationError::SessionSetup)?;

    // Acquire advisory lock (bounded retry, not blocking pg_advisory_lock).
    let db_name: String = sqlx::query_scalar("SELECT current_database()::text")
        .fetch_one(&mut *conn)
        .await
        .map_err(MigrationError::SessionSetup)?;
    let lock_id = generate_lock_id(&db_name);

    let mut locked = false;
    for attempt in 0..LOCK_RETRY_ATTEMPTS {
        let acquired: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock($1)")
            .bind(lock_id)
            .fetch_one(&mut *conn)
            .await
            .map_err(MigrationError::SessionSetup)?;
        if acquired {
            locked = true;
            break;
        }
        if attempt + 1 < LOCK_RETRY_ATTEMPTS {
            tokio::time::sleep(LOCK_RETRY_INTERVAL).await;
        }
    }
    if !locked {
        return Err(MigrationError::LockTimeout {
            attempts: LOCK_RETRY_ATTEMPTS,
        });
    }

    // All post-lock work delegates to run_locked so that
    // release_lock runs on every exit path — success or error.
    let result = run_locked(&mut conn, &migrator).await;
    release_lock(&mut conn, lock_id).await;
    result.map(|applied| MigrationReport {
        applied,
        elapsed_ms: start.elapsed().as_millis(),
    })
}

#[allow(
    clippy::too_many_lines,
    reason = "linear migration pipeline: DDL check → schema checks → batch tx → no-tx pass; splitting would fragment the sequence"
)]
async fn run_locked(
    conn: &mut sqlx::pool::PoolConnection<sqlx::Postgres>,
    migrator: &sqlx::migrate::Migrator,
) -> Result<usize, MigrationError> {
    // Ensure _sqlx_migrations table exists (matches sqlx-postgres DDL exactly).
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS _sqlx_migrations (
            version BIGINT PRIMARY KEY,
            description TEXT NOT NULL,
            installed_on TIMESTAMPTZ NOT NULL DEFAULT now(),
            success BOOLEAN NOT NULL,
            checksum BYTEA NOT NULL,
            execution_time BIGINT NOT NULL
        )",
    )
    .execute(&mut **conn)
    .await
    .map_err(MigrationError::BatchFailed)?;

    // Load applied migrations from DB.
    let applied_rows: Vec<(i64, Vec<u8>)> = sqlx::query_as(
        "SELECT version, checksum FROM _sqlx_migrations WHERE success = true ORDER BY version",
    )
    .fetch_all(&mut **conn)
    .await
    .map_err(MigrationError::BatchFailed)?;

    let applied_map: std::collections::HashMap<i64, Vec<u8>> = applied_rows.into_iter().collect();

    // Collect embedded up-migrations.
    let embedded: Vec<_> = migrator
        .iter()
        .filter(|m| m.migration_type.is_up_migration())
        .collect();
    let embedded_versions: std::collections::HashSet<i64> =
        embedded.iter().map(|m| m.version).collect();

    // Schema-ahead detection (shares the pure set comparison with
    // verify_schema_current): any applied version not in the embedded set.
    // The runner only errors on `ahead` — `behind` is exactly the pending
    // set it applies below, so it is intentionally ignored here.
    let applied_versions: std::collections::HashSet<i64> = applied_map.keys().copied().collect();
    let (ahead, _behind) = diverged_versions(&applied_versions, &embedded_versions);
    if let Some(&version) = ahead.first() {
        return Err(MigrationError::SchemaAhead { version });
    }

    // Checksum verification for applied migrations.
    for m in &embedded {
        if let Some(stored_checksum) = applied_map.get(&m.version)
            && stored_checksum.as_slice() != m.checksum.as_ref()
        {
            return Err(MigrationError::ChecksumMismatch {
                version: m.version,
                name: m.description.to_string(),
            });
        }
    }

    // Partition pending migrations.
    let mut tx_pending = Vec::new();
    let mut no_tx_pending = Vec::new();
    for m in &embedded {
        if applied_map.contains_key(&m.version) {
            continue;
        }
        if m.no_tx {
            no_tx_pending.push(m);
        } else {
            tx_pending.push(m);
        }
    }

    let mut applied_count = 0;

    // Batch transaction on the same connection (manual BEGIN/COMMIT so
    // the advisory lock, lock_timeout, and DDL all share one session).
    if !tx_pending.is_empty() {
        sqlx::query("BEGIN")
            .execute(&mut **conn)
            .await
            .map_err(MigrationError::BatchFailed)?;

        let batch_result = async {
            for m in &tx_pending {
                let migration_start = Instant::now();

                tracing::debug!(
                    version = m.version,
                    name = %m.description,
                    "applying migration"
                );

                sqlx::raw_sql(m.sql.as_ref())
                    .execute(&mut **conn)
                    .await
                    .map_err(|e| {
                        tracing::error!(
                            version = m.version,
                            name = %m.description,
                            error = %e,
                            "migration SQL failed"
                        );
                        MigrationError::BatchFailed(e)
                    })?;

                #[allow(clippy::cast_possible_truncation)]
                let elapsed_nanos = migration_start.elapsed().as_nanos() as i64;

                sqlx::query(
                    "INSERT INTO _sqlx_migrations (version, description, installed_on, success, checksum, execution_time)
                     VALUES ($1, $2, now(), true, $3, $4)",
                )
                .bind(m.version)
                .bind(m.description.as_ref())
                .bind(m.checksum.as_ref())
                .bind(elapsed_nanos)
                .execute(&mut **conn)
                .await
                .map_err(MigrationError::BatchFailed)?;

                applied_count += 1;
            }
            Ok::<(), MigrationError>(())
        }
        .await;

        if let Err(e) = batch_result {
            if let Err(rb) = sqlx::query("ROLLBACK").execute(&mut **conn).await {
                tracing::warn!(error = %rb, "ROLLBACK after batch failure also failed");
            }
            return Err(e);
        }

        sqlx::query("COMMIT")
            .execute(&mut **conn)
            .await
            .map_err(MigrationError::BatchFailed)?;
    }

    // No-transaction migrations run individually after the batch.
    for m in &no_tx_pending {
        let migration_start = Instant::now();

        tracing::debug!(
            version = m.version,
            name = %m.description,
            "applying no-transaction migration"
        );

        sqlx::raw_sql(m.sql.as_ref())
            .execute(&mut **conn)
            .await
            .map_err(|e| MigrationError::NoTxFailed {
                version: m.version,
                name: m.description.to_string(),
                source: e,
            })?;

        #[allow(clippy::cast_possible_truncation)]
        let elapsed_nanos = migration_start.elapsed().as_nanos() as i64;

        sqlx::query(
            "INSERT INTO _sqlx_migrations (version, description, installed_on, success, checksum, execution_time)
             VALUES ($1, $2, now(), true, $3, $4)",
        )
        .bind(m.version)
        .bind(m.description.as_ref())
        .bind(m.checksum.as_ref())
        .bind(elapsed_nanos)
        .execute(&mut **conn)
        .await
        .map_err(|e| MigrationError::NoTxTrackingFailed {
            version: m.version,
            name: m.description.to_string(),
            source: e,
        })?;

        applied_count += 1;
    }

    Ok(applied_count)
}

async fn release_lock(conn: &mut sqlx::pool::PoolConnection<sqlx::Postgres>, lock_id: i64) {
    if let Err(e) = sqlx::query("SELECT pg_advisory_unlock($1)")
        .bind(lock_id)
        .execute(&mut **conn)
        .await
    {
        tracing::warn!(error = %e, "failed to release migration advisory lock");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[sqlx::test(migrations = "./migrations")]
    async fn acquire_with_rls_sets_session_variable(pool: PgPool) {
        let user_id = uuid::Uuid::new_v4();
        let mut tx = acquire_with_rls(&pool, user_id).await.unwrap();

        let row: (String,) = sqlx::query_as("SELECT current_setting('app.current_user_id')")
            .fetch_one(&mut *tx)
            .await
            .unwrap();

        assert_eq!(row.0, user_id.to_string());
        tx.rollback().await.unwrap();
    }

    // --- Unit tests ---

    #[test]
    fn generate_lock_id_matches_known_value() {
        // Independently computed: CRC_32_ISO_HDLC("reverie_dev") = 0x9937d124
        // 0x3d32_ad9e * 0x9937_d124 = 0x24a0_a1a5_b5d0_6838 (i64)
        let expected: i64 = 0x24a0_a1a5_b5d0_6838;
        assert_eq!(
            generate_lock_id("reverie_dev"),
            expected,
            "lock ID must match sqlx's CRC_32_ISO_HDLC * 0x3d32ad9e formula"
        );
    }

    #[test]
    fn embedded_checksum_is_sha384_of_sql() {
        use sha2::{Digest, Sha384};
        let migrator = sqlx::migrate!("./migrations");
        let first_up = migrator
            .iter()
            .find(|m| m.migration_type.is_up_migration())
            .expect("at least one up migration");
        let computed = Sha384::digest(first_up.sql.as_bytes());
        assert_eq!(
            first_up.checksum.as_ref(),
            computed.as_slice(),
            "sqlx embedded checksum must equal SHA-384 of migration SQL"
        );
    }

    #[test]
    fn migration_error_display() {
        let err = MigrationError::SchemaAhead { version: 99999 };
        let msg = err.to_string();
        assert!(msg.contains("99999"), "should contain version: {msg}");
        assert!(msg.contains("newer"), "should mention newer: {msg}");

        let err = MigrationError::ChecksumMismatch {
            version: 12345,
            name: "test_migration".into(),
        };
        let msg = err.to_string();
        assert!(msg.contains("12345"), "should contain version: {msg}");
        assert!(msg.contains("test_migration"), "should contain name: {msg}");

        let err = MigrationError::LockTimeout { attempts: 10 };
        let msg = err.to_string();
        assert!(msg.contains("10"), "should contain attempts: {msg}");
    }

    #[test]
    fn no_tx_tracking_error_distinguishes_from_no_tx_failed() {
        let failed = MigrationError::NoTxFailed {
            version: 1,
            name: "test".into(),
            source: sqlx::Error::RowNotFound,
        };
        let tracking = MigrationError::NoTxTrackingFailed {
            version: 1,
            name: "test".into(),
            source: sqlx::Error::RowNotFound,
        };
        assert!(
            failed.to_string().contains("Fix the failing migration SQL"),
            "NoTxFailed should advise fixing SQL: {failed}"
        );
        assert!(
            tracking.to_string().contains("do NOT revert"),
            "NoTxTrackingFailed should warn against reverting: {tracking}"
        );
    }

    // --- Integration tests ---

    #[sqlx::test(migrations = false)]
    async fn fresh_database_applies_all_migrations(pool: PgPool) {
        let report = run_migrations_inner(&pool).await.unwrap();
        let migrator = sqlx::migrate!("./migrations");
        let expected_up = migrator
            .iter()
            .filter(|m| m.migration_type.is_up_migration())
            .count();
        assert_eq!(report.applied, expected_up);
        assert!(report.applied > 0, "should have at least one migration");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn up_to_date_database_is_noop(pool: PgPool) {
        let report = run_migrations_inner(&pool).await.unwrap();
        assert_eq!(report.applied, 0);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn schema_ahead_detection(pool: PgPool) {
        sqlx::query(
            "INSERT INTO _sqlx_migrations (version, description, installed_on, success, checksum, execution_time)
             VALUES (99_999_999_999_999, 'future_migration', now(), true, '\\x00', 0)",
        )
        .execute(&pool)
        .await
        .unwrap();

        let err = run_migrations_inner(&pool).await.unwrap_err();
        assert!(
            matches!(
                err,
                MigrationError::SchemaAhead {
                    version: 99_999_999_999_999
                }
            ),
            "expected SchemaAhead, got: {err}"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn checksum_mismatch_detected(pool: PgPool) {
        sqlx::query("UPDATE _sqlx_migrations SET checksum = '\\xdeadbeef' WHERE version = (SELECT min(version) FROM _sqlx_migrations)")
            .execute(&pool)
            .await
            .unwrap();

        let err = run_migrations_inner(&pool).await.unwrap_err();
        assert!(
            matches!(err, MigrationError::ChecksumMismatch { .. }),
            "expected ChecksumMismatch, got: {err}"
        );
    }

    #[tokio::test]
    async fn invalid_credentials_clear_error() {
        let err = run_migrations("postgres://bad:bad@localhost:5433/nonexistent")
            .await
            .unwrap_err();
        assert!(
            matches!(err, MigrationError::Connection(_)),
            "expected Connection error, got: {err}"
        );
        let msg = err.to_string();
        assert!(
            !msg.contains("bad:bad"),
            "DSN credentials must not appear in error message: {msg}"
        );
    }

    #[sqlx::test(migrations = false)]
    async fn lock_timeout_applied_to_session(pool: PgPool) {
        // Verify the SQL constant sets the expected session value when
        // executed on a held connection (mirrors what run_migrations_inner
        // does on its acquired connection).
        let mut conn = pool.acquire().await.unwrap();
        sqlx::query(LOCK_TIMEOUT_SQL)
            .execute(&mut *conn)
            .await
            .unwrap();
        let timeout: String = sqlx::query_scalar("SHOW lock_timeout")
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        assert_eq!(timeout, "30s");
        drop(conn);

        // Also confirm the full migration run succeeds.
        run_migrations_inner(&pool).await.unwrap();
    }

    #[sqlx::test(migrations = false)]
    async fn concurrent_starts_serialize(pool: PgPool) {
        let pool2 = pool.clone();
        let (r1, r2) = tokio::join!(run_migrations_inner(&pool), run_migrations_inner(&pool2),);

        let report1 = r1.unwrap();
        let report2 = r2.unwrap();

        let total_applied = report1.applied + report2.applied;
        let migrator = sqlx::migrate!("./migrations");
        let expected = migrator
            .iter()
            .filter(|m| m.migration_type.is_up_migration())
            .count();
        assert_eq!(
            total_applied, expected,
            "exactly one runner should apply all migrations"
        );

        let row_count: (i64,) =
            sqlx::query_as("SELECT count(*) FROM _sqlx_migrations WHERE success = true")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(
            usize::try_from(row_count.0).unwrap(),
            expected,
            "no duplicate migration rows"
        );
    }

    #[sqlx::test(migrations = false)]
    async fn batch_failure_rolls_back_tracking_rows(pool: PgPool) {
        // Apply all migrations, then wipe tracking rows. Schema objects
        // still exist so the re-run's DDL collides → BatchFailed. The
        // batch transaction guarantees that tracking inserts from the
        // failed batch are also rolled back (Postgres transactional DDL
        // rolls back the DDL changes in the same tx as well).
        run_migrations_inner(&pool).await.unwrap();

        sqlx::query("DELETE FROM _sqlx_migrations")
            .execute(&pool)
            .await
            .unwrap();

        let row_count_before: (i64,) = sqlx::query_as("SELECT count(*) FROM _sqlx_migrations")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(row_count_before.0, 0, "precondition: 0 rows after delete");

        let err = run_migrations_inner(&pool).await.unwrap_err();
        assert!(
            matches!(err, MigrationError::BatchFailed(_)),
            "expected BatchFailed, got: {err}"
        );

        let row_count_after: (i64,) = sqlx::query_as("SELECT count(*) FROM _sqlx_migrations")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(
            row_count_after.0, 0,
            "batch rollback should leave zero tracking rows"
        );
    }

    #[sqlx::test(migrations = false)]
    async fn rerun_after_success_is_stable(pool: PgPool) {
        let first = run_migrations_inner(&pool).await.unwrap();
        assert!(first.applied > 0, "should apply at least one migration");

        let row_count_after_first: (i64,) =
            sqlx::query_as("SELECT count(*) FROM _sqlx_migrations WHERE success = true")
                .fetch_one(&pool)
                .await
                .unwrap();

        let second = run_migrations_inner(&pool).await.unwrap();
        assert_eq!(second.applied, 0, "second run should be noop");

        let row_count_after_second: (i64,) =
            sqlx::query_as("SELECT count(*) FROM _sqlx_migrations WHERE success = true")
                .fetch_one(&pool)
                .await
                .unwrap();

        assert_eq!(
            row_count_after_first.0, row_count_after_second.0,
            "row count should be stable across runs"
        );
    }

    // --- verify_schema_current (read-only startup check, default server path) ---

    #[sqlx::test(migrations = "./migrations")]
    async fn verify_schema_current_ok_when_up_to_date(pool: PgPool) {
        // Exercised through the app role (reverie_app) to prove the
        // GRANT SELECT ON _sqlx_migrations is in place: the default server
        // path runs this check on the app pool, never the migrator DSN.
        let app_pool = crate::test_support::db::app_pool_for(&pool).await;
        verify_schema_current(&app_pool)
            .await
            .expect("up-to-date schema should verify Ok via the app pool");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn verify_schema_current_detects_ahead(pool: PgPool) {
        // Inject a future migration row via the owner pool (app role has
        // SELECT only on _sqlx_migrations and could not write this).
        sqlx::query(
            "INSERT INTO _sqlx_migrations (version, description, installed_on, success, checksum, execution_time)
             VALUES (99_999_999_999_999, 'future_migration', now(), true, '\\x00', 0)",
        )
        .execute(&pool)
        .await
        .unwrap();

        let app_pool = crate::test_support::db::app_pool_for(&pool).await;
        let err = verify_schema_current(&app_pool).await.unwrap_err();
        assert!(
            matches!(
                err,
                MigrationError::SchemaAhead {
                    version: 99_999_999_999_999
                }
            ),
            "expected SchemaAhead, got: {err}"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn verify_schema_current_detects_behind(pool: PgPool) {
        // Simulate a DB behind the binary: delete the most recent applied
        // row so the embedded migrator knows a version the DB has not
        // applied. The table stays present (this is NOT the table-absent
        // path). Fail-closed: a stale app must refuse to serve.
        let removed: i64 =
            sqlx::query_scalar("DELETE FROM _sqlx_migrations WHERE version = (SELECT max(version) FROM _sqlx_migrations) RETURNING version")
                .fetch_one(&pool)
                .await
                .unwrap();

        let app_pool = crate::test_support::db::app_pool_for(&pool).await;
        let err = verify_schema_current(&app_pool).await.unwrap_err();
        assert!(
            matches!(err, MigrationError::SchemaBehind { version } if version == removed),
            "expected SchemaBehind {{ version: {removed} }}, got: {err}"
        );
    }

    #[sqlx::test(migrations = false)]
    async fn verify_schema_current_table_absent_is_not_initialized(pool: PgPool) {
        // Fresh DB, never migrated — _sqlx_migrations does not exist. The
        // catalog probe must precede the version SELECT so the operator sees
        // a legible "run `reverie migrate`" message, NOT a raw
        // "relation _sqlx_migrations does not exist" SQL error. This is
        // UNK-331's first-boot state.
        let err = verify_schema_current(&pool).await.unwrap_err();
        assert!(
            matches!(err, MigrationError::NotInitialized),
            "expected NotInitialized, got: {err}"
        );
        assert!(
            err.to_string().contains("reverie migrate"),
            "NotInitialized message must point operator at `reverie migrate`: {err}"
        );
    }

    // --- Least-privilege migration identity (ADR Confirmation invariants) ---

    #[sqlx::test(migrations = "./migrations")]
    async fn migrator_role_is_least_privilege(pool: PgPool) {
        // The dedicated migration identity must hold no cluster-wide
        // authority — it owns only the objects it creates and runs under RLS.
        let (is_super, can_createrole, can_bypassrls): (bool, bool, bool) = sqlx::query_as(
            "SELECT rolsuper, rolcreaterole, rolbypassrls FROM pg_roles WHERE rolname = 'reverie_migrator'",
        )
        .fetch_one(&pool)
        .await
        .expect("reverie_migrator must exist (docker/init-roles.sql seeds it)");
        assert!(!is_super, "reverie_migrator must not be SUPERUSER");
        assert!(!can_createrole, "reverie_migrator must not have CREATEROLE");
        assert!(!can_bypassrls, "reverie_migrator must not have BYPASSRLS");
    }

    #[test]
    fn migration_set_has_no_superuser_only_operations() {
        // The migration set must be applicable by the non-superuser
        // reverie_migrator. Scan the embedded SQL for operations that require
        // superuser. CREATE EXTENSION is allowed only for the two trusted
        // extensions (pg_trgm, pgcrypto), which a non-superuser with CREATE on
        // the target schema may install; any other extension needs superuser.
        let migrator = sqlx::migrate!("./migrations");
        for m in migrator
            .iter()
            .filter(|m| m.migration_type.is_up_migration())
        {
            for line in m.sql.lines() {
                let upper = line.to_uppercase();
                assert!(
                    !upper.contains("ALTER SYSTEM"),
                    "migration {} contains ALTER SYSTEM (superuser-only): {line}",
                    m.version
                );
                assert!(
                    !upper.contains("CREATE ROLE") && !upper.contains("CREATE USER"),
                    "migration {} provisions roles (superuser/createrole-only; roles belong in docker/init-roles.sql): {line}",
                    m.version
                );
                assert!(
                    !upper.contains("CREATE EVENT TRIGGER"),
                    "migration {} creates an event trigger (superuser-only): {line}",
                    m.version
                );
                if upper.contains("CREATE EXTENSION") {
                    assert!(
                        upper.contains("PG_TRGM") || upper.contains("PGCRYPTO"),
                        "migration {} installs a non-allowlisted extension (only trusted pg_trgm/pgcrypto are installable by the non-superuser migrator): {line}",
                        m.version
                    );
                }
            }
        }
    }
}
