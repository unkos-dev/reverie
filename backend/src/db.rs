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
    acquire_with_rls_inner(pool, user_id, RlsIsolation::Inherited).await
}

/// Opens an RLS transaction whose statements see changes committed while waiting for locks.
///
/// # Errors
/// Returns database errors from transaction setup.
pub async fn acquire_with_rls_read_committed(
    pool: &PgPool,
    user_id: uuid::Uuid,
) -> Result<sqlx::Transaction<'_, sqlx::Postgres>, sqlx::Error> {
    acquire_with_rls_inner(pool, user_id, RlsIsolation::ReadCommitted).await
}

enum RlsIsolation {
    Inherited,
    ReadCommitted,
}

async fn acquire_with_rls_inner(
    pool: &PgPool,
    user_id: uuid::Uuid,
    isolation: RlsIsolation,
) -> Result<sqlx::Transaction<'_, sqlx::Postgres>, sqlx::Error> {
    let mut tx = pool.begin().await?;
    if matches!(isolation, RlsIsolation::ReadCommitted) {
        sqlx::query("SET TRANSACTION ISOLATION LEVEL READ COMMITTED")
            .execute(&mut *tx)
            .await?;
    }
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

/// How long [`run_migrations`] waits for its single connection before
/// reporting [`MigrationError::Connection`].
///
/// This budget covers the whole of connection establishment (TCP handshake,
/// TLS, authentication), and sqlx's pool additionally treats a refused
/// connection as retryable for its duration. It therefore doubles as the
/// startup-race tolerance for the deployment modes that have no readiness
/// gate. The shipped compose topology gates both the migrate container and
/// the app on `depends_on: { condition: service_healthy }`, but the bare
/// `docker run` and `REVERIE_AUTO_MIGRATE` paths the migration ADR supports
/// have no such sequencing, and a database still running initdb on a fresh
/// volume can take many seconds to accept its first connection.
///
/// Thirty seconds is what sqlx would default to anyway. It is stated here so
/// the tolerance is a recorded decision rather than an inherited default,
/// and so that shortening it reads as the change to deployment behaviour it
/// would be. Tests that assert the unreachable-database path pass their own
/// short budget instead of paying this one.
const MIGRATION_CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

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

    /// A read against `_sqlx_migrations` during read-only schema verification
    /// failed. Unlike [`Self::Connection`], the failing pool is the
    /// application pool (`reverie_app` / `DATABASE_URL`), NOT the migration
    /// DSN — most often `reverie_app` lacks `SELECT` on `_sqlx_migrations` (a
    /// database migrated before that grant existed), or a transport/auth
    /// failure on the app pool.
    #[error(
        "failed to read migration history during schema verification: {0} \
         — ensure reverie_app has SELECT on _sqlx_migrations (re-run `reverie migrate`) \
         and that DATABASE_URL is reachable"
    )]
    VerificationRead(#[source] sqlx::Error),

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
/// Waits up to [`MIGRATION_CONNECT_TIMEOUT`] for the connection, which is
/// also the startup-race tolerance for the deployment modes that have no
/// readiness gate.
///
/// # Errors
///
/// Returns [`MigrationError`] on connection failure, lock timeout,
/// schema-ahead detection, checksum mismatch, or SQL execution failure.
pub async fn run_migrations(url: &str) -> Result<MigrationReport, MigrationError> {
    run_migrations_with_connect_timeout(url, MIGRATION_CONNECT_TIMEOUT).await
}

/// [`run_migrations`] with an explicit connection budget.
///
/// Exists so a test can assert the unreachable-database path without
/// spending the production startup tolerance on it. Production callers go
/// through [`run_migrations`] and get [`MIGRATION_CONNECT_TIMEOUT`].
async fn run_migrations_with_connect_timeout(
    url: &str,
    connect_timeout: std::time::Duration,
) -> Result<MigrationReport, MigrationError> {
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(connect_timeout)
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
/// no schema-management rights. Fail-closed in every divergence case:
/// [`MigrationError::SchemaAhead`] when the DB is newer than the binary,
/// [`MigrationError::SchemaBehind`] when the DB is older (the common
/// "forgot to migrate" case), and [`MigrationError::NotInitialized`] when the
/// DB was never migrated at all. Refusing on schema-behind is deliberate: the
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
/// - [`MigrationError::VerificationRead`] if the read fails (app-pool auth,
///   connectivity, or missing `SELECT` on `_sqlx_migrations`).
pub async fn verify_schema_current(pool: &PgPool) -> Result<(), MigrationError> {
    // The two reads below use raw `sqlx::query_scalar` rather than the
    // compile-time-checked macro: `_sqlx_migrations` is managed by sqlx itself
    // and is absent from the user prepare cache, so the macro cannot validate
    // it. The migration runner reads the same table the same way.
    //
    // Catalog probe FIRST. A never-migrated database has no
    // `_sqlx_migrations` table; without this probe the version SELECT below
    // surfaces a cryptic "relation _sqlx_migrations does not exist" instead
    // of the legible NotInitialized message, defeating the fail-closed goal.
    let table_exists: bool =
        sqlx::query_scalar("SELECT to_regclass('public._sqlx_migrations') IS NOT NULL")
            .fetch_one(pool)
            .await
            .map_err(MigrationError::VerificationRead)?;
    if !table_exists {
        return Err(MigrationError::NotInitialized);
    }

    // Mirror the runner's `WHERE success = true` filter (the runner only
    // ever writes success=true rows, but stay consistent defensively).
    let applied_versions: std::collections::HashSet<i64> =
        sqlx::query_scalar("SELECT version FROM _sqlx_migrations WHERE success = true")
            .fetch_all(pool)
            .await
            .map_err(MigrationError::VerificationRead)?
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

#[expect(
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

                sqlx::raw_sql(m.sql.clone())
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

                #[expect(clippy::cast_possible_truncation, reason = "elapsed nanos fit i64 for any realistic duration; series positions are small whole numbers")]
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

        sqlx::raw_sql(m.sql.clone())
            .execute(&mut **conn)
            .await
            .map_err(|e| MigrationError::NoTxFailed {
                version: m.version,
                name: m.description.to_string(),
                source: e,
            })?;

        #[expect(
            clippy::cast_possible_truncation,
            reason = "elapsed nanos fit i64 for any realistic duration; series positions are small whole numbers"
        )]
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

    async fn storage_manifestation(
        pool: &PgPool,
        library_id: Option<uuid::Uuid>,
        path: &str,
        hash: &str,
    ) -> Result<(), sqlx::Error> {
        let work = sqlx::query_scalar!(
            "INSERT INTO works (title, sort_title) VALUES ('Storage', 'storage') RETURNING id"
        )
        .fetch_one(pool)
        .await?;
        sqlx::query!(
            "WITH inserted AS (INSERT INTO manifestations
             (work_id, library_id, file_path, format, ingestion_file_hash, current_file_hash, file_size_bytes)
             VALUES ($1, $2, $3, 'epub', $4, $4, 7) RETURNING *) INSERT INTO library_path_claims (library_id, path, manifestation_id) SELECT library_id, file_path, id FROM inserted",
            work,
            library_id,
            path,
            hash,
        )
        .execute(pool)
        .await?;
        Ok(())
    }

    fn storage_sqlstate(error: &sqlx::Error) -> String {
        error
            .as_database_error()
            .unwrap()
            .code()
            .unwrap()
            .into_owned()
    }

    fn claim_location(library: uuid::Uuid, path: &str) -> crate::services::files::LibraryLocation {
        crate::services::files::LibraryLocation {
            library_id: crate::models::storage_library::LibraryId::from_uuid(library),
            path: path.parse().unwrap(),
        }
    }

    async fn claim_manifestation(
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        library: uuid::Uuid,
        path: &str,
    ) -> (uuid::Uuid, uuid::Uuid) {
        let work = sqlx::query_scalar!(
            "INSERT INTO works (title, sort_title) VALUES ('Claim', 'claim') RETURNING id"
        )
        .fetch_one(&mut **tx)
        .await
        .unwrap();
        let id = uuid::Uuid::new_v4();
        sqlx::query!(
            "INSERT INTO manifestations (id, work_id, library_id, file_path, format, ingestion_file_hash, current_file_hash, file_size_bytes)
             VALUES ($1, $2, $3, $4, 'epub', $5, $5, 7)",
            id, work, library, path, id.to_string(),
        ).execute(&mut **tx).await.unwrap();
        (work, id)
    }

    async fn apply_schema_pair(pool: &PgPool, first: &'static str, second: &'static str) {
        let mut tx = pool.begin().await.unwrap();
        sqlx::raw_sql(first).execute(&mut *tx).await.unwrap();
        sqlx::raw_sql(second).execute(&mut *tx).await.unwrap();
        tx.commit().await.unwrap();
    }

    async fn populated_relocation_downgrade(pool: &PgPool, recorded_destination: bool) {
        use crate::models::library_path_claim::reserve;
        let library = crate::models::storage_library::default_library_id(pool)
            .await
            .unwrap()
            .as_uuid();
        let mut tx = pool.begin().await.unwrap();
        let (_, id) = claim_manifestation(&mut tx, library, "recorded.epub").await;
        for path in ["recorded.epub", "source.epub", "destination.epub"] {
            assert!(
                reserve(&mut tx, &claim_location(library, path), id)
                    .await
                    .unwrap()
            );
        }
        let destination = if recorded_destination {
            "recorded.epub"
        } else {
            "destination.epub"
        };
        sqlx::query!("UPDATE manifestations SET relocation_source_path = 'source.epub', relocation_destination_path = $2 WHERE id = $1", id, destination).execute(&mut *tx).await.unwrap();
        sqlx::query!("INSERT INTO writeback_jobs (manifestation_id, reason, status, attempt_count) VALUES ($1, 'relocation', 'skipped', 10), ($1, 'metadata', 'pending', 1), ($1, 'cover', 'failed', 2)", id).execute(&mut *tx).await.unwrap();
        tx.commit().await.unwrap();
        apply_schema_pair(
            pool,
            include_str!("../migrations/20261003000000_library_path_claims.down.sql"),
            include_str!("../migrations/20261001000000_writeback_relocation_intent.down.sql"),
        )
        .await;
        assert_eq!(
            sqlx::query_scalar!("SELECT count(*) FROM writeback_jobs WHERE reason = 'relocation'")
                .fetch_one(pool)
                .await
                .unwrap(),
            Some(0)
        );
        let remaining = sqlx::query!("SELECT reason, attempt_count, status::text AS status FROM writeback_jobs WHERE manifestation_id = $1 ORDER BY reason", id).fetch_all(pool).await.unwrap();
        assert_eq!(remaining.len(), 2);
        assert_eq!(
            (
                remaining[0].reason.as_str(),
                remaining[0].attempt_count,
                remaining[0].status.as_deref()
            ),
            ("cover", 2, Some("failed"))
        );
        assert_eq!(
            (
                remaining[1].reason.as_str(),
                remaining[1].attempt_count,
                remaining[1].status.as_deref()
            ),
            ("metadata", 1, Some("pending"))
        );
        assert_eq!(
            sqlx::query_scalar!("SELECT file_path FROM manifestations WHERE id = $1", id)
                .fetch_one(pool)
                .await
                .unwrap(),
            "recorded.epub"
        );
        assert_eq!(storage_sqlstate(&sqlx::query!("INSERT INTO writeback_jobs (manifestation_id, reason) VALUES ($1, 'relocation')", id).execute(pool).await.unwrap_err()), "23514");
        apply_schema_pair(
            pool,
            include_str!("../migrations/20261001000000_writeback_relocation_intent.up.sql"),
            include_str!("../migrations/20261003000000_library_path_claims.up.sql"),
        )
        .await;
        assert_eq!(
            sqlx::query_scalar!(
                "SELECT path FROM library_path_claims WHERE manifestation_id = $1",
                id
            )
            .fetch_one(pool)
            .await
            .unwrap(),
            "recorded.epub"
        );
        assert!(
            sqlx::query_scalar!(
                "SELECT relocation_source_path FROM manifestations WHERE id = $1",
                id
            )
            .fetch_one(pool)
            .await
            .unwrap()
            .is_none()
        );
        sqlx::query!(
            "INSERT INTO writeback_jobs (manifestation_id, reason) VALUES ($1, 'relocation')",
            id
        )
        .execute(pool)
        .await
        .unwrap();
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn relocation_downgrade_populated_jobs_preserve_recorded_source(pool: PgPool) {
        populated_relocation_downgrade(&pool, false).await;
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn relocation_downgrade_retained_pair_preserves_recorded_destination(pool: PgPool) {
        populated_relocation_downgrade(&pool, true).await;
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn path_claim_unique_owner_deduplication_and_library_scope(pool: PgPool) {
        use crate::models::library_path_claim::{owner, reserve};
        let library = crate::models::storage_library::default_library_id(&pool)
            .await
            .unwrap()
            .as_uuid();
        let other = sqlx::query_scalar!(
            "INSERT INTO libraries (configuration_key) VALUES ('other') RETURNING id"
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let ing = crate::test_support::db::ingestion_pool_for(&pool).await;
        let mut tx = ing.begin().await.unwrap();
        let (_, first) = claim_manifestation(&mut tx, library, "same.epub").await;
        let (_, second) = claim_manifestation(&mut tx, other, "same.epub").await;
        let location = claim_location(library, "same.epub");
        assert!(reserve(&mut tx, &location, first).await.unwrap());
        assert!(reserve(&mut tx, &location, first).await.unwrap());
        assert!(!reserve(&mut tx, &location, second).await.unwrap());
        assert_eq!(owner(&mut tx, &location).await.unwrap(), Some(first));
        assert!(
            reserve(&mut tx, &claim_location(other, "same.epub"), second)
                .await
                .unwrap()
        );
        tx.commit().await.unwrap();
        assert_eq!(
            sqlx::query_scalar!("SELECT count(*) FROM library_path_claims")
                .fetch_one(&pool)
                .await
                .unwrap(),
            Some(2)
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn path_claim_missing_wrong_owner_and_wrong_library_fail_at_commit(pool: PgPool) {
        use crate::models::library_path_claim::reserve;
        let library = crate::models::storage_library::default_library_id(&pool)
            .await
            .unwrap()
            .as_uuid();
        let other = sqlx::query_scalar!(
            "INSERT INTO libraries (configuration_key) VALUES ('other') RETURNING id"
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let mut tx = pool.begin().await.unwrap();
        let (_, owner) = claim_manifestation(&mut tx, library, "owner.epub").await;
        assert!(
            reserve(&mut tx, &claim_location(library, "owner.epub"), owner)
                .await
                .unwrap()
        );
        tx.commit().await.unwrap();
        for case in ["missing", "wrong-owner", "wrong-library"] {
            let mut tx = pool.begin().await.unwrap();
            let (_, id) = claim_manifestation(&mut tx, library, "invalid.epub").await;
            match case {
                "wrong-owner" => {
                    assert!(
                        reserve(&mut tx, &claim_location(library, "invalid.epub"), owner)
                            .await
                            .unwrap()
                    );
                }
                "wrong-library" => {
                    assert!(
                        reserve(&mut tx, &claim_location(other, "invalid.epub"), id)
                            .await
                            .unwrap()
                    );
                }
                _ => {}
            }
            assert_eq!(
                storage_sqlstate(&tx.commit().await.unwrap_err()),
                "23503",
                "{case}"
            );
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn path_claim_both_intent_references_and_direct_deletion(pool: PgPool) {
        use crate::models::library_path_claim::reserve;
        let library = crate::models::storage_library::default_library_id(&pool)
            .await
            .unwrap()
            .as_uuid();
        let mut tx = pool.begin().await.unwrap();
        let (_, id) = claim_manifestation(&mut tx, library, "recorded.epub").await;
        assert!(
            reserve(&mut tx, &claim_location(library, "recorded.epub"), id)
                .await
                .unwrap()
        );
        tx.commit().await.unwrap();
        for absent in ["source.epub", "destination.epub"] {
            let mut tx = pool.begin().await.unwrap();
            let present = if absent == "source.epub" {
                "destination.epub"
            } else {
                "source.epub"
            };
            assert!(
                reserve(&mut tx, &claim_location(library, present), id)
                    .await
                    .unwrap()
            );
            sqlx::query!("UPDATE manifestations SET relocation_source_path = 'source.epub', relocation_destination_path = 'destination.epub' WHERE id = $1", id).execute(&mut *tx).await.unwrap();
            assert_eq!(storage_sqlstate(&tx.commit().await.unwrap_err()), "23503");
        }
        let mut tx = pool.begin().await.unwrap();
        sqlx::query!(
            "DELETE FROM library_path_claims WHERE manifestation_id = $1",
            id
        )
        .execute(&mut *tx)
        .await
        .unwrap();
        assert_eq!(storage_sqlstate(&tx.commit().await.unwrap_err()), "23503");
        let mut tx = pool.begin().await.unwrap();
        for path in ["source.epub", "destination.epub"] {
            assert!(
                reserve(&mut tx, &claim_location(library, path), id)
                    .await
                    .unwrap()
            );
        }
        sqlx::query!("UPDATE manifestations SET relocation_source_path = 'source.epub', relocation_destination_path = 'destination.epub' WHERE id = $1", id).execute(&mut *tx).await.unwrap();
        tx.commit().await.unwrap();
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn path_claim_role_access_and_concurrent_reservation(pool: PgPool) {
        use crate::models::library_path_claim::{owner, reserve};
        let library = crate::models::storage_library::default_library_id(&pool)
            .await
            .unwrap()
            .as_uuid();
        let ing = crate::test_support::db::ingestion_pool_for(&pool).await;
        let wb = crate::test_support::db::writeback_pool_for(&pool).await;
        let ordinary = crate::test_support::db::app_pool_for(&pool).await;
        let mut tx = ing.begin().await.unwrap();
        let (_, first) = claim_manifestation(&mut tx, library, "first.epub").await;
        let (_, second) = claim_manifestation(&mut tx, library, "second.epub").await;
        assert!(
            reserve(&mut tx, &claim_location(library, "first.epub"), first)
                .await
                .unwrap()
        );
        assert!(
            reserve(&mut tx, &claim_location(library, "second.epub"), second)
                .await
                .unwrap()
        );
        tx.commit().await.unwrap();
        let mut tx = ordinary.begin().await.unwrap();
        assert!(
            owner(&mut tx, &claim_location(library, "first.epub"))
                .await
                .unwrap_err()
                .to_string()
                .contains("ownership evidence is unavailable")
        );
        assert_eq!(
            storage_sqlstate(
                &reserve(&mut tx, &claim_location(library, "denied.epub"), first)
                    .await
                    .unwrap_err()
            ),
            "42501"
        );
        tx.rollback().await.unwrap();
        let mut tx = wb.begin().await.unwrap();
        assert_eq!(
            owner(&mut tx, &claim_location(library, "first.epub"))
                .await
                .unwrap(),
            Some(first)
        );
        assert!(
            reserve(&mut tx, &claim_location(library, "contended.epub"), first)
                .await
                .unwrap()
        );
        let (started, reached) = tokio::sync::oneshot::channel();
        let next = tokio::spawn(async move {
            let mut tx = ing.begin().await.unwrap();
            started.send(()).unwrap();
            let reserved = reserve(&mut tx, &claim_location(library, "contended.epub"), second)
                .await
                .unwrap();
            tx.rollback().await.unwrap();
            reserved
        });
        reached.await.unwrap();
        assert!(!next.is_finished());
        tx.commit().await.unwrap();
        assert!(
            !tokio::time::timeout(std::time::Duration::from_secs(5), next)
                .await
                .unwrap()
                .unwrap()
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn publication_date_migration_refuses_loss_and_roundtrips(pool: PgPool) {
        let down = include_str!("../migrations/20261030000000_publication_date_text.down.sql");
        let up = include_str!("../migrations/20261030000000_publication_date_text.up.sql");
        let (_, id) =
            crate::test_support::db::insert_work_and_manifestation(&pool, "date-rollback").await;
        let (_, empty) =
            crate::test_support::db::insert_work_and_manifestation(&pool, "date-null").await;
        sqlx::query!(
            "UPDATE manifestations SET pub_date = '2024-01-15T12:30Z' WHERE id = $1",
            id
        )
        .execute(&pool)
        .await
        .unwrap();
        let mut tx = pool.begin().await.unwrap();
        let error = sqlx::raw_sql(down).execute(&mut *tx).await.unwrap_err();
        assert!(error.to_string().contains("complete dates"));
        tx.rollback().await.unwrap();
        assert_eq!(
            sqlx::query_scalar!("SELECT pub_date FROM manifestations WHERE id = $1", id)
                .fetch_one(&pool)
                .await
                .unwrap()
                .as_deref(),
            Some("2024-01-15T12:30Z")
        );
        sqlx::query!(
            "UPDATE manifestations SET pub_date = '2024-01-15' WHERE id = $1",
            id
        )
        .execute(&pool)
        .await
        .unwrap();
        for status in ["pending", "in_progress"] {
            let job = sqlx::query_scalar!(
                "INSERT INTO writeback_jobs (manifestation_id, reason, field_name, status) VALUES ($1, 'metadata', 'pub_date', $2::text::writeback_status) RETURNING id",
                id, status,
            ).fetch_one(&pool).await.unwrap();
            let mut tx = pool.begin().await.unwrap();
            let error = sqlx::raw_sql(down).execute(&mut *tx).await.unwrap_err();
            assert!(error.to_string().contains("jobs must drain"));
            tx.rollback().await.unwrap();
            assert_eq!(
                sqlx::query_scalar!("SELECT field_name FROM writeback_jobs WHERE id = $1", job)
                    .fetch_one(&pool)
                    .await
                    .unwrap()
                    .as_deref(),
                Some("pub_date")
            );
            sqlx::query!(
                "UPDATE writeback_jobs SET status = 'complete' WHERE id = $1",
                job
            )
            .execute(&pool)
            .await
            .unwrap();
        }
        sqlx::raw_sql(down).execute(&pool).await.unwrap();
        assert_eq!(sqlx::query_scalar!("SELECT data_type FROM information_schema.columns WHERE table_name = 'manifestations' AND column_name = 'pub_date'")
            .fetch_one(&pool).await.unwrap().as_deref(), Some("date"));
        sqlx::raw_sql(up).execute(&pool).await.unwrap();
        assert_eq!(sqlx::query_scalar!("SELECT data_type FROM information_schema.columns WHERE table_name = 'manifestations' AND column_name = 'pub_date'")
            .fetch_one(&pool).await.unwrap().as_deref(), Some("text"));
        assert_eq!(
            sqlx::query_scalar!("SELECT pub_date FROM manifestations WHERE id = $1", id)
                .fetch_one(&pool)
                .await
                .unwrap()
                .as_deref(),
            Some("2024-01-15")
        );
        assert_eq!(
            sqlx::query_scalar!("SELECT pub_date FROM manifestations WHERE id = $1", empty)
                .fetch_one(&pool)
                .await
                .unwrap(),
            None
        );
        assert_eq!(
            sqlx::query_scalar!(
                "SELECT field_name FROM writeback_jobs WHERE manifestation_id = $1 LIMIT 1",
                id
            )
            .fetch_one(&pool)
            .await
            .unwrap(),
            None
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn library_storage_schema_seeded_identity_and_required_owner(pool: PgPool) {
        use crate::models::storage_library::default_library_id;
        let id = default_library_id(&pool).await.unwrap();
        assert_eq!(id, default_library_id(&pool).await.unwrap());
        storage_manifestation(&pool, Some(id.as_uuid()), "author/book.epub", "valid")
            .await
            .unwrap();
        let error = storage_manifestation(&pool, None, "missing.epub", "missing")
            .await
            .unwrap_err();
        assert_eq!(storage_sqlstate(&error), "23502");
        let error =
            storage_manifestation(&pool, Some(uuid::Uuid::new_v4()), "unknown.epub", "unknown")
                .await
                .unwrap_err();
        assert_eq!(storage_sqlstate(&error), "23503");
        sqlx::query!("DELETE FROM manifestations")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query!("DELETE FROM libraries WHERE configuration_key = 'default'")
            .execute(&pool)
            .await
            .unwrap();
        assert!(matches!(
            default_library_id(&pool).await,
            Err(sqlx::Error::RowNotFound)
        ));
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn library_storage_schema_canonical_relative_paths(pool: PgPool) {
        let id = crate::models::storage_library::default_library_id(&pool)
            .await
            .unwrap();
        for path in [
            "",
            "/book",
            "book/",
            "a//book",
            "a\\book",
            "C:book",
            ".",
            "..",
            "./book",
            "a/../book",
            "a/./book",
        ] {
            let error = storage_manifestation(&pool, Some(id.as_uuid()), path, path)
                .await
                .unwrap_err();
            assert_eq!(storage_sqlstate(&error), "23514", "{path:?}");
        }
        storage_manifestation(
            &pool,
            Some(id.as_uuid()),
            "Author/Book: edition.epub",
            "valid",
        )
        .await
        .unwrap();
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn library_storage_schema_library_namespace_and_global_hash(pool: PgPool) {
        let first = crate::models::storage_library::default_library_id(&pool)
            .await
            .unwrap()
            .as_uuid();
        let second = sqlx::query_scalar!(
            "INSERT INTO libraries (configuration_key) VALUES ('second') RETURNING id"
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        storage_manifestation(&pool, Some(first), "book.epub", "first")
            .await
            .unwrap();
        let error = storage_manifestation(&pool, Some(first), "book.epub", "duplicate-path")
            .await
            .unwrap_err();
        assert_eq!(storage_sqlstate(&error), "23505");
        storage_manifestation(&pool, Some(second), "book.epub", "second")
            .await
            .unwrap();
        let error = storage_manifestation(&pool, Some(second), "other.epub", "first")
            .await
            .unwrap_err();
        assert_eq!(storage_sqlstate(&error), "23505");
        let error = sqlx::query!("DELETE FROM libraries WHERE id = $1", first)
            .execute(&pool)
            .await
            .unwrap_err();
        assert_eq!(storage_sqlstate(&error), "23001");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn library_storage_schema_runtime_roles_read_only(pool: PgPool) {
        let roles = [
            crate::test_support::db::app_pool_for(&pool).await,
            crate::test_support::db::ingestion_pool_for(&pool).await,
            crate::test_support::db::readonly_pool_for(&pool).await,
        ];
        for role in roles {
            crate::models::storage_library::default_library_id(&role)
                .await
                .unwrap();
            let insert =
                sqlx::query!("INSERT INTO libraries (configuration_key) VALUES ('forbidden')")
                    .execute(&role)
                    .await
                    .unwrap_err();
            assert_eq!(storage_sqlstate(&insert), "42501");
            let update = sqlx::query!("UPDATE libraries SET configuration_key = 'forbidden'")
                .execute(&role)
                .await
                .unwrap_err();
            assert_eq!(storage_sqlstate(&update), "42501");
            let delete = sqlx::query!("DELETE FROM libraries")
                .execute(&role)
                .await
                .unwrap_err();
            assert_eq!(storage_sqlstate(&delete), "42501");
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn readonly_role_is_refused_credential_tables(pool: PgPool) {
        let readonly = crate::test_support::db::readonly_pool_for(&pool).await;

        let tokens = sqlx::query_scalar!("SELECT id FROM device_tokens")
            .fetch_all(&readonly)
            .await
            .unwrap_err();
        assert_eq!(storage_sqlstate(&tokens), "42501", "{tokens:?}");

        let credentials = sqlx::query_scalar!("SELECT user_id FROM local_credentials")
            .fetch_all(&readonly)
            .await
            .unwrap_err();
        assert_eq!(storage_sqlstate(&credentials), "42501", "{credentials:?}");

        let token_hashes = sqlx::query_scalar!("SELECT token_hash FROM device_tokens")
            .fetch_all(&readonly)
            .await
            .unwrap_err();
        assert_eq!(storage_sqlstate(&token_hashes), "42501", "{token_hashes:?}");

        let password_hashes = sqlx::query_scalar!("SELECT password_hash FROM local_credentials")
            .fetch_all(&readonly)
            .await
            .unwrap_err();
        assert_eq!(
            storage_sqlstate(&password_hashes),
            "42501",
            "{password_hashes:?}"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn read_committed_rls_overrides_default_without_leaking(pool: PgPool) {
        let isolated = sqlx::postgres::PgPoolOptions::new()
            .max_connections(1)
            .after_connect(|conn, _| {
                Box::pin(async move {
                    sqlx::query("SET default_transaction_isolation = 'repeatable read'")
                        .execute(conn)
                        .await?;
                    Ok(())
                })
            })
            .connect_with((*pool.connect_options()).clone())
            .await
            .unwrap();
        let user_id = uuid::Uuid::new_v4();
        let mut tx = acquire_with_rls_read_committed(&isolated, user_id)
            .await
            .unwrap();
        let settings: (String, String) = sqlx::query_as(
            "SELECT current_setting('transaction_isolation'), current_setting('app.current_user_id')"
        ).fetch_one(&mut *tx).await.unwrap();
        assert_eq!(settings, ("read committed".to_owned(), user_id.to_string()));
        tx.rollback().await.unwrap();
        let settings: (String, String) = sqlx::query_as(
            "SELECT current_setting('transaction_isolation'), current_setting('app.current_user_id')"
        ).fetch_one(&isolated).await.unwrap();
        assert_eq!(settings, ("repeatable read".to_owned(), String::new()));
        let mut inherited = acquire_with_rls(&isolated, user_id).await.unwrap();
        let isolation: String = sqlx::query_scalar("SHOW transaction_isolation")
            .fetch_one(&mut *inherited)
            .await
            .unwrap();
        assert_eq!(isolation, "repeatable read");
        inherited.commit().await.unwrap();
    }

    #[tokio::test]
    async fn invalid_credentials_clear_error() {
        // Its own budget, not MIGRATION_CONNECT_TIMEOUT: that constant is the
        // startup-race tolerance for ungated deployments, and this test has
        // no reason to spend it. Port 5433 refuses instantly, but sqlx's pool
        // retries a refused connection for the whole budget, so an unbounded
        // run here costs the suite the full production tolerance.
        const TEST_CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);

        let started = Instant::now();
        let err = run_migrations_with_connect_timeout(
            "postgres://bad:bad@localhost:5433/nonexistent",
            TEST_CONNECT_TIMEOUT,
        )
        .await
        .unwrap_err();
        let elapsed = started.elapsed();
        assert!(
            matches!(err, MigrationError::Connection(_)),
            "expected Connection error, got: {err}"
        );
        let msg = err.to_string();
        assert!(
            !msg.contains("bad:bad"),
            "DSN credentials must not appear in error message: {msg}"
        );
        // Loose at 3x the budget so a loaded runner cannot fail it; it only
        // has to catch the budget silently ceasing to apply, which would put
        // this back on the 30s production tolerance.
        let bound = TEST_CONNECT_TIMEOUT * 3;
        assert!(
            elapsed < bound,
            "unreachable database should fail within {bound:?}, took {elapsed:?} \
             (is the connect budget still applied to the pool?)"
        );
    }

    #[sqlx::test(migrations = false)]
    async fn a_batch_failing_after_a_success_leaves_neither_record_nor_schema(pool: PgPool) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("20260101000000_creates_probe.sql"),
            "CREATE TABLE batch_probe (id integer);",
        )
        .unwrap();
        std::fs::write(dir.path().join("20260101000001_fails.sql"), "SELECT 1 / 0;").unwrap();
        let migrator = sqlx::migrate::Migrator::new(dir.path()).await.unwrap();
        let mut conn = pool.acquire().await.unwrap();

        let err = run_locked(&mut conn, &migrator).await.unwrap_err();
        assert!(
            matches!(err, MigrationError::BatchFailed(_)),
            "expected BatchFailed, got: {err}"
        );

        let recorded = sqlx::query_scalar!("SELECT count(*) AS \"count!\" FROM _sqlx_migrations")
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        assert_eq!(
            recorded, 0,
            "the succeeded migration must not stay recorded"
        );
        let probe = sqlx::query_scalar!("SELECT to_regclass('batch_probe')::text")
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        assert_eq!(
            probe, None,
            "the succeeded migration's table must not exist"
        );
    }

    // --- verify_schema_current (read-only startup check, default server path) ---

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
        // the fresh-database first-boot state.
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
}
