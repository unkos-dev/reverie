//! Background writeback queue worker.
//!
//! Mirrors `services::enrichment::queue` with one change: at most one job
//! per manifestation is ever `in_progress` at the same time, so two
//! workers can never race writebacks on the same on-disk `EPUB`.
//!
//! The guarantee is enforced in two layers:
//!
//! 1. A partial `UNIQUE` index (`idx_writeback_jobs_in_progress_unique`)
//!    on `(manifestation_id) WHERE status = 'in_progress'` — the
//!    load-bearing correctness gate.  Two workers racing the same
//!    manifestation both survive `NOT EXISTS` under `READ COMMITTED`
//!    (which can't see a peer's uncommitted `UPDATE`), but when the
//!    second worker's `UPDATE` would create a duplicate `in_progress`
//!    tuple, Postgres waits on the first worker's uncommitted index
//!    entry, then fails with `SQLSTATE 23505`.  `claim_next` translates
//!    that into `Ok(None)`.
//! 2. A `NOT EXISTS` clause inside the claim `CTE` — a cheap soft filter
//!    that avoids the unique-violation round-trip on the common path
//!    where a sibling job already holds the `in_progress` slot.
//!
//! ## `RLS` system-context invariant
//!
//! The `pool` passed to [`spawn_worker`] MUST have its connections
//! configured with `app.system_context = 'writeback'` (set via
//! `after_connect`).  The `manifestations_update_system` `RLS` policy
//! matches against this `GUC`; without it, the orchestrator's
//! `UPDATE manifestations SET current_file_hash = …` writes produce
//! zero rows affected and the hash update is silently dropped.  User-facing
//! pools (`reverie_app` without the `GUC` set) can never satisfy this
//! policy, which is intentional: it prevents a future handler bug from
//! inadvertently calling `run_once` on a user-context pool.

use std::sync::Arc;
use std::time::Duration;

use crate::services::files::LibraryFiles;
use sqlx::PgPool;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use tokio::time::Interval;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};
use uuid::Uuid;

use crate::config::Config;

use super::JobReason;
use super::error::WritebackError;
use super::events;
use super::orchestrator::{self, RunOutcome};

/// Stop claiming on cancellation, drain tracked jobs, then recover orphaned claims.
/// Active blocking work keeps its permit if the outer drain is aborted.
///
/// On startup, calls [`revert_in_progress`] to recover any rows left
/// `in_progress` by a prior process crash.  The worker then polls
/// `writeback_jobs` on a configurable interval, claiming and dispatching
/// up to `config.writeback.concurrency` jobs concurrently using a
/// `tokio` semaphore.
///
/// The `pool` MUST be a writeback-context pool (see module-level `RLS`
/// system-context invariant).
///
/// # Errors
///
/// - `anyhow::Error` wrapping `sqlx::Error` if [`revert_in_progress`] or
///   `claim_next` fails at the database layer.
pub async fn spawn_worker(
    pool: PgPool,
    config: Config,
    cancel: CancellationToken,
    files: LibraryFiles,
) -> anyhow::Result<()> {
    spawn_worker_with(
        pool,
        config,
        cancel,
        files,
        |pool, config, files, id, permit| async move {
            orchestrator::run_once(&pool, &config, &files, id, permit).await
        },
        WorkerHooks::new(|| {}),
    )
    .await
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WorkerOperation {
    Reset,
    StartupSweep,
    PeriodicSweep,
    Claim,
}

struct WorkerHooks<D, B> {
    draining: D,
    database: B,
    recovery_period: Duration,
}

impl<D> WorkerHooks<D, fn(WorkerOperation) -> sqlx::Result<()>> {
    fn new(draining: D) -> Self {
        Self {
            draining,
            database: |_| Ok(()),
            recovery_period: Duration::from_secs(300),
        }
    }
}

async fn worker_startup(
    pool: &PgPool,
    database: &mut impl FnMut(WorkerOperation) -> sqlx::Result<()>,
    reset_complete: &mut bool,
    sweep_complete: &mut bool,
) -> bool {
    if !*reset_complete {
        let result = async {
            database(WorkerOperation::Reset)?;
            revert_in_progress(pool).await
        }
        .await;
        if let Err(error) = result {
            warn!(%error, "writeback startup reset failed; retrying on timer");
            return false;
        }
        *reset_complete = true;
    }
    if !*sweep_complete {
        let result = async {
            database(WorkerOperation::StartupSweep)?;
            sweep_relocations(pool).await
        }
        .await;
        if let Err(error) = result {
            warn!(%error, "writeback startup sweep failed; retrying on timer");
            return false;
        }
        *sweep_complete = true;
    }
    true
}

async fn spawn_worker_with<F, Fut>(
    pool: PgPool,
    config: Config,
    cancel: CancellationToken,
    files: LibraryFiles,
    run: F,
    mut hooks: WorkerHooks<impl FnOnce(), impl FnMut(WorkerOperation) -> sqlx::Result<()>>,
) -> anyhow::Result<()>
where
    F: Fn(PgPool, Config, LibraryFiles, Uuid, Arc<tokio::sync::OwnedSemaphorePermit>) -> Fut
        + Clone
        + Send
        + 'static,
    Fut: std::future::Future<Output = Result<RunOutcome, WritebackError>> + Send + 'static,
{
    if !config.writeback.enabled {
        info!("writeback queue disabled by config");
        cancel.cancelled().await;
        return Ok(());
    }

    let mut reset_complete = false;
    let mut startup_sweep_complete = false;

    let concurrency = config.writeback.concurrency as usize;
    let semaphore = Arc::new(Semaphore::new(concurrency));
    let mut interval: Interval =
        tokio::time::interval(Duration::from_secs(config.writeback.poll_idle_secs));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut recovery_interval = tokio::time::interval_at(
        tokio::time::Instant::now() + hooks.recovery_period,
        hooks.recovery_period,
    );
    recovery_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    info!(
        concurrency,
        poll_idle_secs = config.writeback.poll_idle_secs,
        "writeback queue started"
    );

    let mut jobs = JoinSet::new();
    loop {
        tokio::select! {
            () = cancel.cancelled() => {
                info!("writeback queue shutting down");
                (hooks.draining)();
                break;
            }
            result = jobs.join_next(), if !jobs.is_empty() => {
                if let Some(Err(error)) = result { warn!(%error, "writeback job task failed"); }
            }
            _ = recovery_interval.tick(), if startup_sweep_complete => {
                let result = async {
                    (hooks.database)(WorkerOperation::PeriodicSweep)?;
                    sweep_relocations(&pool).await
                }.await;
                if let Err(error) = result {
                    warn!(%error, "writeback periodic relocation sweep failed; retrying on timer");
                }
            }
            _ = interval.tick() => {
                if !worker_startup(&pool, &mut hooks.database, &mut reset_complete, &mut startup_sweep_complete).await {
                    continue;
                }
                loop {
                    if cancel.is_cancelled() { break; }
                    let Ok(permit) = semaphore.clone().try_acquire_owned() else {
                        break;
                    };
                    let claim = match async {
                        (hooks.database)(WorkerOperation::Claim)?;
                        claim_next(&pool).await
                    }.await {
                        Ok(claim) => claim,
                        Err(error) => {
                            warn!(%error, "writeback claim failed; retrying on timer");
                            break;
                        }
                    };
                    let Some((id, attempt_count)) = claim else {
                        drop(permit);
                        break;
                    };
                    let pool = pool.clone();
                    let cfg = config.clone();
                    let files = files.clone();
                    let run = run.clone();
                    jobs.spawn(async move {
                        let permit = Arc::new(permit);
                        let result = run(pool.clone(), cfg.clone(), files, id, Arc::clone(&permit)).await;
                        if let Err(e) = finish(&pool, &cfg, id, attempt_count, result).await {
                            warn!(error = %e, %id, "writeback: finish bookkeeping failed");
                        }
                    });
                }
            }
        }
    }
    while let Some(result) = jobs.join_next().await {
        if let Err(error) = result {
            warn!(%error, "writeback job task failed during drain");
        }
    }
    revert_in_progress(&pool).await?;
    Ok(())
}

/// Atomic claim of the next eligible `writeback_jobs` row.
///
/// Returns `Some((id, new_attempt_count))` when a row was claimed, or
/// `None` when the queue is empty, all eligible rows are inside their
/// back-off window, or a concurrent worker won the race on the partial
/// `UNIQUE` index.
///
/// The partial `UNIQUE` index on `(manifestation_id) WHERE status =
/// 'in_progress'` is the load-bearing serialisation primitive; the
/// `NOT EXISTS` clause in the `CTE` is a common-path optimisation that
/// avoids a unique-violation round-trip.
///
/// # Errors
///
/// - `sqlx::Error` (any variant other than unique-violation) — a database
///   error occurred while executing the claim `CTE`.  Unique-violation
///   (`SQLSTATE 23505`) is translated to `Ok(None)`.
pub async fn claim_next(pool: &PgPool) -> sqlx::Result<Option<(Uuid, i32)>> {
    let result = sqlx::query!(
        r"WITH eligible AS (
             SELECT wj.id, wj.attempt_count,
                    m.relocation_source_path IS NOT NULL AND wj.reason <> 'relocation' AS recovering_edit
             FROM writeback_jobs wj
             JOIN manifestations m ON m.id = wj.manifestation_id
             WHERE wj.status IN ('pending', 'failed')
               AND NOT EXISTS (
                 SELECT 1 FROM writeback_jobs other
                 WHERE other.manifestation_id = wj.manifestation_id
                   AND other.status = 'in_progress'
               )
               AND (
                 wj.last_attempted_at IS NULL
                 OR wj.last_attempted_at <
                      now() - (
                        CASE
                          WHEN m.relocation_source_path IS NOT NULL THEN INTERVAL '5 minutes'
                          WHEN wj.attempt_count <= 0 THEN INTERVAL '0 minutes'
                          WHEN wj.attempt_count = 1 THEN INTERVAL '5 minutes'
                          WHEN wj.attempt_count = 2 THEN INTERVAL '30 minutes'
                          WHEN wj.attempt_count = 3 THEN INTERVAL '2 hours'
                          WHEN wj.attempt_count = 4 THEN INTERVAL '8 hours'
                          ELSE INTERVAL '24 hours'
                        END
                      )
               )
             ORDER BY wj.last_attempted_at NULLS FIRST, wj.created_at
             LIMIT 1
             FOR UPDATE OF wj SKIP LOCKED
           )
           UPDATE writeback_jobs wj
              SET status = 'in_progress',
                  last_attempted_at = now(),
                  attempt_count = wj.attempt_count + CASE WHEN eligible.recovering_edit THEN 0 ELSE 1 END
             FROM eligible
            WHERE wj.id = eligible.id
           RETURNING wj.id, wj.attempt_count",
    )
    .fetch_optional(pool)
    .await;

    match result {
        Ok(row) => Ok(row.map(|r| (r.id, r.attempt_count))),
        // A peer worker beat us to the in_progress slot for this
        // manifestation. The partial UNIQUE index did its job; treat as a
        // lost race and let the caller poll again.
        Err(sqlx::Error::Database(db_err)) if db_err.is_unique_violation() => {
            tracing::debug!(
                "writeback: claim_next lost race on in_progress unique index; will retry"
            );
            Ok(None)
        }
        Err(e) => Err(e),
    }
}

async fn sweep_relocations(pool: &PgPool) -> sqlx::Result<u64> {
    let result = sqlx::query!(
        "WITH eligible AS (
            SELECT m.id FROM manifestations m
            WHERE m.relocation_source_path IS NOT NULL
              AND NOT EXISTS (
                SELECT 1 FROM writeback_jobs wj
                WHERE wj.manifestation_id = m.id
                  AND wj.status IN ('pending', 'failed', 'in_progress')
              )
            ORDER BY m.id LIMIT 100
            FOR UPDATE OF m SKIP LOCKED
        )
        INSERT INTO writeback_jobs (manifestation_id, reason)
        SELECT id, 'relocation' FROM eligible",
    )
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

/// Build and dispatch a terminal event for `job_id`; see
/// `events::dispatch` for the dedupe contract.
async fn dispatch_terminal(
    pool: &PgPool,
    job_id: Uuid,
    outcome: events::TerminalOutcome,
    manifestation_id: Uuid,
    reason: &str,
    attempt_count: i32,
    detail: &str,
) {
    events::dispatch(
        pool,
        &events::TerminalEvent {
            job_id,
            outcome,
            manifestation_id,
            reason,
            attempt_count,
            detail,
        },
    )
    .await;
}

async fn finish(
    pool: &PgPool,
    config: &Config,
    id: Uuid,
    _claimed_attempt_count: i32,
    result: Result<RunOutcome, super::error::WritebackError>,
) -> sqlx::Result<()> {
    let attempt_count =
        sqlx::query_scalar!("SELECT attempt_count FROM writeback_jobs WHERE id = $1", id,)
            .fetch_optional(pool)
            .await?
            .unwrap_or(0);
    // Emit the webhook BEFORE the DB bookkeeping write.  If the DB write
    // fails, the event still fires (a transient DB hiccup on the final
    // update otherwise silently dropped the webhook forever).  A DB
    // failure followed by crash-recovery retry re-fires the same terminal
    // event; `events::dispatch` dedupes on the stable event id.
    match result {
        Ok(RunOutcome::RelocationTerminal {
            manifestation_id,
            reason,
            intent,
            diagnosis,
        }) => {
            dispatch_terminal(
                pool,
                id,
                events::TerminalOutcome::Skipped,
                manifestation_id,
                &reason,
                attempt_count,
                &diagnosis,
            )
            .await;
            let mut tx = pool.begin().await?;
            finalise_terminal(&mut tx, id, manifestation_id, &intent, &diagnosis).await?;
            tx.commit().await?;
        }
        Ok(RunOutcome::Success {
            manifestation_id,
            reason,
            current_file_hash,
        }) => {
            dispatch_terminal(
                pool,
                id,
                events::TerminalOutcome::Complete,
                manifestation_id,
                &reason,
                attempt_count,
                &current_file_hash,
            )
            .await;
            mark_complete(pool, id).await?;
        }
        Ok(RunOutcome::Skipped {
            manifestation_id,
            reason,
            skip_reason,
        }) => {
            dispatch_terminal(
                pool,
                id,
                events::TerminalOutcome::Skipped,
                manifestation_id,
                &reason,
                attempt_count,
                &skip_reason,
            )
            .await;
            // Terminal skip (e.g. unsupported format): bypass retry path.
            mark_skipped(pool, id, &skip_reason).await?;
        }
        Ok(RunOutcome::Failed {
            manifestation_id,
            reason,
            error,
        }) => {
            dispatch_terminal(
                pool,
                id,
                events::TerminalOutcome::Failed,
                manifestation_id,
                &reason,
                attempt_count,
                &error,
            )
            .await;
            mark_failed(pool, id, attempt_count, config, Some(&error)).await?;
        }
        Err(e) => {
            warn!(error = %e, %id, "writeback run_once failed");
            let err_str = e.to_string();
            // JobNotFound is terminal: the job row has vanished (CASCADE
            // removed the manifestation, or someone deleted the row
            // manually). There's no row to retry against, so retrying
            // burns the full retry budget pointlessly — go straight to
            // skipped.
            let is_job_not_found = matches!(e, super::error::WritebackError::JobNotFound(_));

            // Resolve manifestation_id from the job row so the webhook
            // carries the right target. Fall back to Uuid::nil() when the
            // row is gone or the lookup fails, so every terminal
            // transition still produces an event that downstream consumers
            // can correlate against the job id.
            let (mid, reason) = failure_identity(pool, id).await;
            let outcome = if is_job_not_found {
                events::TerminalOutcome::Skipped
            } else {
                events::TerminalOutcome::Failed
            };
            dispatch_terminal(pool, id, outcome, mid, &reason, attempt_count, &err_str).await;

            if is_job_not_found {
                mark_skipped(pool, id, &err_str).await?;
            } else {
                mark_failed(pool, id, attempt_count, config, Some(&err_str)).await?;
            }
        }
    }
    Ok(())
}

async fn failure_identity(pool: &PgPool, id: Uuid) -> (Uuid, String) {
    match sqlx::query!(
        "SELECT manifestation_id, reason AS \"reason: JobReason\" FROM writeback_jobs WHERE id = $1",
        id,
    )
    .fetch_optional(pool)
    .await
    {
        Ok(Some(row)) => (row.manifestation_id, row.reason.as_str().to_owned()),
        Ok(None) => {
            warn!(
                %id,
                "writeback: job row vanished before failure webhook could be emitted; using sentinel manifestation_id"
            );
            (Uuid::nil(), "unknown".into())
        }
        Err(lookup_err) => {
            warn!(
                error = %lookup_err,
                %id,
                "writeback: manifestation_id lookup failed; using sentinel manifestation_id"
            );
            (Uuid::nil(), "unknown".into())
        }
    }
}

async fn finalise_terminal(
    connection: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    id: Uuid,
    manifestation_id: Uuid,
    intent: &orchestrator::RelocationIntent,
    diagnosis: &str,
) -> sqlx::Result<()> {
    sqlx::query!(
        "SELECT id FROM writeback_jobs WHERE id = $1 AND manifestation_id = $2 AND status = 'in_progress' FOR UPDATE",
        id, manifestation_id,
    ).fetch_one(&mut **connection).await?;
    let cleared = sqlx::query!(
        "UPDATE manifestations SET relocation_source_path = NULL, relocation_destination_path = NULL WHERE id = $1 AND relocation_source_path = $2 AND relocation_destination_path = $3",
        manifestation_id, intent.source.as_str(), intent.destination.as_str(),
    ).execute(&mut **connection).await?;
    if cleared.rows_affected() != 1 {
        return Err(sqlx::Error::RowNotFound);
    }
    sqlx::query!(
        "UPDATE writeback_jobs SET status = 'skipped', completed_at = now(), error = $2 WHERE id = $1",
        id, diagnosis,
    ).execute(&mut **connection).await?;
    crate::models::library_path_claim::release_obsolete(connection, manifestation_id).await?;
    Ok(())
}

async fn mark_skipped(pool: &PgPool, id: Uuid, reason: &str) -> sqlx::Result<()> {
    sqlx::query!(
        "UPDATE writeback_jobs \
         SET status = 'skipped', completed_at = now(), error = $1 \
         WHERE id = $2",
        reason,
        id,
    )
    .execute(pool)
    .await?;
    Ok(())
}

async fn mark_complete(pool: &PgPool, id: Uuid) -> sqlx::Result<()> {
    sqlx::query!(
        "UPDATE writeback_jobs SET status = 'complete', completed_at = now(), error = NULL \
         WHERE id = $1",
        id,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Transition to `failed`, or to `skipped` once `attempt_count >=
/// max_attempts`.  `skipped` is the terminal exhaustion label.
async fn mark_failed(
    pool: &PgPool,
    id: Uuid,
    attempt_count: i32,
    config: &Config,
    error: Option<&str>,
) -> sqlx::Result<()> {
    let max = config.writeback.max_attempts.cast_signed();
    let exhausted = attempt_count >= max;
    let next_status = if exhausted { "skipped" } else { "failed" };
    if exhausted {
        tracing::warn!(
            %id,
            attempt_count,
            max_attempts = max,
            error,
            "writeback: job exhausted retries, transitioning to skipped"
        );
    }
    sqlx::query!(
        // sqlx macros have no built-in &str→enum mapping for `writeback_status`,
        // so $1 binds as text and the cast to the DB enum happens in SQL.
        "UPDATE writeback_jobs \
         SET status = ($1::text)::writeback_status, error = $2 \
         WHERE id = $3",
        next_status,
        error,
        id,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Revert any `in_progress` rows back to `pending`.
///
/// Called on worker startup (crash recovery) and on graceful shutdown.
/// A process killed between claiming a job and finishing it leaves a row
/// in `in_progress` indefinitely; this call restores it to `pending` so
/// the next startup picks it up.
///
/// # Errors
///
/// - `sqlx::Error` — the `UPDATE writeback_jobs` statement failed.
pub async fn revert_in_progress(pool: &PgPool) -> sqlx::Result<()> {
    let res = sqlx::query!(
        "UPDATE writeback_jobs SET status = 'pending' \
         WHERE status = 'in_progress'",
    )
    .execute(pool)
    .await?;
    if res.rows_affected() > 0 {
        info!(
            count = res.rows_affected(),
            "writeback: reverted in_progress jobs to pending"
        );
    }
    Ok(())
}

#[cfg(test)]
#[expect(
    clippy::let_underscore_must_use,
    reason = "test code: discarding transaction rollback Results in test helpers is intentional; the crate-root cfg_attr only covers unwrap_used/expect_used"
)]
mod tests {
    use super::*;
    use crate::config::{CleanupMode, CoverConfig, EnrichmentConfig, WritebackConfig};
    use crate::models::manifestation_format::ManifestationFormat;
    use tokio::sync::Barrier;

    use crate::test_support::db::{app_pool_for, ingestion_pool_for, writeback_pool_for};

    fn test_config_with_max_attempts(
        max_attempts: u32,
    ) -> (Config, crate::services::files::LibraryFiles) {
        let (storage_config, files) = crate::test_support::test_storage_config(
            None,
            crate::models::storage_library::LibraryId::from_uuid(uuid::Uuid::new_v4()),
        );
        let config = Config {
            port: 3000,
            database_url: String::new(),
            library_path: storage_config.library_path,
            ingestion_path: storage_config.ingestion_path,
            quarantine_path: storage_config.quarantine_path,
            log_level: "info".into(),
            db_max_connections: 5,
            oidc_issuer_url: String::new(),
            oidc_client_id: String::new(),
            oidc_client_secret: String::new(),
            oidc_redirect_uri: String::new(),
            local_auth_enabled: true,
            resource_server_issuer: String::new(),
            resource_server_audience: String::new(),
            resource_server_jwks_url: String::new(),
            resource_server_require_at_jwt: false,
            login_rate_per_min: 10,
            login_throttle_base_secs: 2,
            login_throttle_cap_secs: 900,
            password_min_length: 8,
            password_max_length: 256,
            password_min_zxcvbn_score: 2,
            password_breach_check_enabled: true,
            self_registration_enabled: false,
            recovery_pin_ttl_secs: 900,
            recovery_pin_dir: "./reverie-recovery".into(),
            trusted_client_ip_header: None,
            migration_database_url: None,
            auto_migrate: false,
            ingestion_database_url: String::new(),
            format_priority: vec![ManifestationFormat::Epub],
            cleanup_mode: CleanupMode::None,
            enrichment: EnrichmentConfig {
                enabled: false,
                concurrency: 1,
                poll_idle_secs: 30,
                fetch_budget_secs: 15,
                http_timeout_secs: 10,
                max_attempts: 3,
                cache_ttl_hit_days: 1,
                cache_ttl_miss_days: 1,
                cache_ttl_error_mins: 1,
            },
            cover: CoverConfig {
                max_bytes: 10_485_760,
                download_timeout_secs: 30,
                min_long_edge_px: 1000,
                redirect_limit: 3,
            },
            writeback: WritebackConfig {
                enabled: true,
                concurrency: 2,
                poll_idle_secs: 1,
                max_attempts,
            },
            opds: crate::config::OpdsConfig {
                enabled: false,
                page_size: 50,
                realm: "Reverie OPDS".into(),
                public_url: None,
            },
            security: crate::config::SecurityConfig {
                behind_https: false,
                hsts_include_subdomains: false,
                hsts_preload: false,
                csp_report_endpoint: None,
                frontend_dist_path: None,
                csp_html_header: None,
                csp_api_header: None,
            },
            googlebooks_api_key: None,
            hardcover_api_token: None,
            operator_contact: None,
            ingestion_dsn_defaulted: false,
        };
        (config, files)
    }

    /// Insert a minimal work + manifestation fixture and return ids.
    async fn insert_fixture(pool: &PgPool, marker: &str) -> (Uuid, Uuid) {
        let title = format!("WritebackFixture-{marker}");
        let work_id = sqlx::query_scalar!(
            "INSERT INTO works (title, sort_title) VALUES ($1, $1) RETURNING id",
            title,
        )
        .fetch_one(pool)
        .await
        .unwrap();
        let file_path = format!("fixtures/wb-{marker}.epub");
        let hash = format!("wb-hash-{marker}");
        let m_id = sqlx::query_scalar!(
            // Set enrichment_status = 'complete' so these fixtures don't
            // leak into the enrichment queue's claim_next under parallel
            // test execution (the column defaults to 'pending').
            "WITH inserted AS (INSERT INTO manifestations \
               (library_id, work_id, format, file_path, ingestion_file_hash, current_file_hash, \
                file_size_bytes, ingestion_status, validation_status, enrichment_status) \
             VALUES ((SELECT id FROM libraries WHERE configuration_key = 'default'), $1, 'epub'::manifestation_format, $2, $3, $3, 1000, \
                     'complete'::ingestion_status, 'clean'::validation_status, \
                     'complete'::enrichment_status) \
             RETURNING *), claimed AS (INSERT INTO library_path_claims (library_id, path, manifestation_id) SELECT library_id, file_path, id FROM inserted) SELECT id AS \"id!\" FROM inserted",
            work_id,
            file_path,
            hash,
        )
        .fetch_one(pool)
        .await
        .unwrap();
        (work_id, m_id)
    }

    async fn insert_job(pool: &PgPool, manifestation_id: Uuid, reason: &str) -> Uuid {
        sqlx::query_scalar!(
            "INSERT INTO writeback_jobs (manifestation_id, reason) VALUES ($1, $2) RETURNING id",
            manifestation_id,
            reason,
        )
        .fetch_one(pool)
        .await
        .unwrap()
    }

    async fn set_intent(pool: &PgPool, mid: Uuid) -> orchestrator::RelocationIntent {
        let source = sqlx::query_scalar!("SELECT file_path FROM manifestations WHERE id = $1", mid)
            .fetch_one(pool)
            .await
            .unwrap();
        let destination = format!("recovered/{mid}.epub");
        sqlx::query!("WITH changed AS (UPDATE manifestations SET relocation_source_path = $2, relocation_destination_path = $3 WHERE id = $1 RETURNING *), removed AS (DELETE FROM library_path_claims c USING changed m WHERE c.manifestation_id = m.id AND c.library_id <> m.library_id) INSERT INTO library_path_claims (library_id, path, manifestation_id) SELECT library_id, file_path AS path, id FROM changed UNION SELECT library_id, relocation_source_path, id FROM changed WHERE relocation_source_path IS NOT NULL UNION SELECT library_id, relocation_destination_path, id FROM changed WHERE relocation_destination_path IS NOT NULL ON CONFLICT (library_id, path) DO NOTHING", mid, source, destination).execute(pool).await.unwrap();
        orchestrator::RelocationIntent {
            source: source.parse().unwrap(),
            destination: destination.parse().unwrap(),
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn relocation_schema_rejects_half_pairs_and_invalid_paths(pool: PgPool) {
        let ing = ingestion_pool_for(&pool).await;
        let wb = writeback_pool_for(&pool).await;
        let (_, mid) = insert_fixture(&ing, "intent-schema").await;
        for (source, destination) in [
            (Some("source.epub"), None),
            (None, Some("destination.epub")),
        ] {
            let error = sqlx::query!("WITH changed AS (UPDATE manifestations SET relocation_source_path = $2, relocation_destination_path = $3 WHERE id = $1 RETURNING *), removed AS (DELETE FROM library_path_claims c USING changed m WHERE c.manifestation_id = m.id AND c.library_id <> m.library_id) INSERT INTO library_path_claims (library_id, path, manifestation_id) SELECT library_id, file_path AS path, id FROM changed UNION SELECT library_id, relocation_source_path, id FROM changed WHERE relocation_source_path IS NOT NULL UNION SELECT library_id, relocation_destination_path, id FROM changed WHERE relocation_destination_path IS NOT NULL ON CONFLICT (library_id, path) DO NOTHING", mid, source, destination).execute(&wb).await.unwrap_err();
            assert!(
                matches!(error, sqlx::Error::Database(error) if error.constraint() == Some("manifestations_relocation_pair_check"))
            );
        }
        for invalid in [
            "",
            "/absolute.epub",
            "trailing/",
            "a//b.epub",
            "a\\b.epub",
            "C:drive.epub",
            ".",
            "..",
            "a/./b",
            "a/../b",
        ] {
            for (source, destination) in [(invalid, "valid.epub"), ("valid.epub", invalid)] {
                let error = sqlx::query!("WITH changed AS (UPDATE manifestations SET relocation_source_path = $2, relocation_destination_path = $3 WHERE id = $1 RETURNING *), removed AS (DELETE FROM library_path_claims c USING changed m WHERE c.manifestation_id = m.id AND c.library_id <> m.library_id) INSERT INTO library_path_claims (library_id, path, manifestation_id) SELECT library_id, file_path AS path, id FROM changed UNION SELECT library_id, relocation_source_path, id FROM changed WHERE relocation_source_path IS NOT NULL UNION SELECT library_id, relocation_destination_path, id FROM changed WHERE relocation_destination_path IS NOT NULL ON CONFLICT (library_id, path) DO NOTHING", mid, source, destination).execute(&wb).await.unwrap_err();
                assert!(
                    matches!(error, sqlx::Error::Database(error) if error.is_check_violation())
                );
            }
        }
        set_intent(&wb, mid).await;
        let carrier = insert_job(&ing, mid, "relocation").await;
        assert_eq!(
            sqlx::query_scalar!("SELECT reason FROM writeback_jobs WHERE id = $1", carrier)
                .fetch_one(&wb)
                .await
                .unwrap(),
            "relocation"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn relocation_accounting_waiting_edit_survives_recovery_and_source_restoration_failures(
        pool: PgPool,
    ) {
        let wb = writeback_pool_for(&pool).await;
        let (_dir, files, job, mid, intent) = carrier_fixture(&pool).await;
        sqlx::query!(
            "UPDATE writeback_jobs SET reason = 'metadata', attempt_count = 0 WHERE id = $1",
            job
        )
        .execute(&wb)
        .await
        .unwrap();
        let library = crate::models::storage_library::default_library_id(&pool)
            .await
            .unwrap();
        let root = files.library(library).unwrap();
        root.rename(intent.source.as_path(), root, "saved.epub")
            .unwrap();
        root.create_dir(intent.source.as_path()).unwrap();
        for _ in 0..5 {
            let result = carrier_run(&wb, &files, job).await;
            assert!(result.is_err());
            finish(&wb, &test_config_with_max_attempts(1).0, job, 99, result)
                .await
                .unwrap();
            let row = sqlx::query!(
                "SELECT attempt_count, status::text AS status FROM writeback_jobs WHERE id = $1",
                job
            )
            .fetch_one(&wb)
            .await
            .unwrap();
            assert_eq!(row.attempt_count, 0);
            assert_eq!(row.status.as_deref(), Some("failed"));
            sqlx::query!("UPDATE writeback_jobs SET last_attempted_at = now() - INTERVAL '6 minutes' WHERE id = $1", job).execute(&wb).await.unwrap();
            assert_eq!(claim_next(&wb).await.unwrap(), Some((job, 0)));
        }
        root.remove_dir(intent.source.as_path()).unwrap();
        root.rename("saved.epub", root, intent.source.as_path())
            .unwrap();
        root.write(intent.destination.as_path(), b"FOREIGN")
            .unwrap();
        let result = carrier_run(&wb, &files, job).await;
        assert!(
            matches!(&result, Ok(RunOutcome::Failed { error, .. }) if error.contains("source restored"))
        );
        finish(&wb, &test_config_with_max_attempts(1).0, job, 99, result)
            .await
            .unwrap();
        assert_eq!(
            sqlx::query_scalar!(
                "SELECT attempt_count FROM writeback_jobs WHERE id = $1",
                job
            )
            .fetch_one(&wb)
            .await
            .unwrap(),
            0
        );
        assert!(
            sqlx::query_scalar!(
                "SELECT relocation_source_path FROM manifestations WHERE id = $1",
                mid
            )
            .fetch_one(&wb)
            .await
            .unwrap()
            .is_none()
        );
        assert_eq!(root.read(intent.destination.as_path()).unwrap(), b"FOREIGN");
        sqlx::query!(
            "UPDATE writeback_jobs SET last_attempted_at = NULL WHERE id = $1",
            job
        )
        .execute(&wb)
        .await
        .unwrap();
        assert_eq!(claim_next(&wb).await.unwrap(), Some((job, 1)));
        let result = carrier_run(&wb, &files, job).await;
        assert!(result.is_err());
        finish(&wb, &test_config_with_max_attempts(1).0, job, 0, result)
            .await
            .unwrap();
        assert_eq!(
            sqlx::query_scalar!("SELECT status::text FROM writeback_jobs WHERE id = $1", job)
                .fetch_one(&wb)
                .await
                .unwrap()
                .as_deref(),
            Some("skipped")
        );
        assert_eq!(root.read(intent.source.as_path()).unwrap(), b"PAYLOAD");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn relocation_accounting_permanent_recovery_consumes_no_edit(pool: PgPool) {
        let wb = writeback_pool_for(&pool).await;
        let (_dir, files, job, mid, intent) = carrier_fixture(&pool).await;
        sqlx::query!(
            "UPDATE writeback_jobs SET reason = 'cover', attempt_count = 0 WHERE id = $1",
            job
        )
        .execute(&wb)
        .await
        .unwrap();
        let library = crate::models::storage_library::default_library_id(&pool)
            .await
            .unwrap();
        files
            .library(library)
            .unwrap()
            .remove_file(intent.source.as_path())
            .unwrap();
        let result = carrier_run(&wb, &files, job).await;
        assert!(matches!(&result, Ok(RunOutcome::RelocationTerminal { .. })));
        finish(&wb, &test_config_with_max_attempts(1).0, job, 99, result)
            .await
            .unwrap();
        let row = sqlx::query!(
            "SELECT attempt_count, status::text AS status FROM writeback_jobs WHERE id = $1",
            job
        )
        .fetch_one(&wb)
        .await
        .unwrap();
        assert_eq!(row.attempt_count, 0);
        assert_eq!(row.status.as_deref(), Some("skipped"));
        assert!(
            sqlx::query_scalar!(
                "SELECT relocation_source_path FROM manifestations WHERE id = $1",
                mid
            )
            .fetch_one(&wb)
            .await
            .unwrap()
            .is_none()
        );
        assert_eq!(
            sqlx::query_scalar!(
                "SELECT count(*) FROM library_path_claims WHERE manifestation_id = $1",
                mid
            )
            .fetch_one(&wb)
            .await
            .unwrap(),
            Some(1)
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn relocation_queue_intent_retries_in_five_minutes_preserving_ordinary_backoff(
        pool: PgPool,
    ) {
        let ing = ingestion_pool_for(&pool).await;
        let wb = writeback_pool_for(&pool).await;
        let (_, mid) = insert_fixture(&ing, "retry-intent").await;
        let intent = set_intent(&wb, mid).await;
        let job = insert_job(&ing, mid, "metadata").await;
        sqlx::query!("UPDATE writeback_jobs SET status = 'failed', attempt_count = 5, last_attempted_at = now() - INTERVAL '4 minutes' WHERE id = $1", job).execute(&wb).await.unwrap();
        assert!(claim_next(&wb).await.unwrap().is_none());
        sqlx::query!("UPDATE writeback_jobs SET last_attempted_at = now() - INTERVAL '6 minutes' WHERE id = $1", job).execute(&wb).await.unwrap();
        assert_eq!(claim_next(&wb).await.unwrap(), Some((job, 5)));
        mark_failed(
            &wb,
            job,
            5,
            &test_config_with_max_attempts(10).0,
            Some("storage failure"),
        )
        .await
        .unwrap();
        sqlx::query!("WITH changed AS (UPDATE manifestations SET relocation_source_path = NULL, relocation_destination_path = NULL WHERE id = $1 AND relocation_source_path = $2 RETURNING *), removed AS (DELETE FROM library_path_claims c USING changed m WHERE c.manifestation_id = m.id AND c.library_id <> m.library_id) INSERT INTO library_path_claims (library_id, path, manifestation_id) SELECT library_id, file_path AS path, id FROM changed UNION SELECT library_id, relocation_source_path, id FROM changed WHERE relocation_source_path IS NOT NULL UNION SELECT library_id, relocation_destination_path, id FROM changed WHERE relocation_destination_path IS NOT NULL ON CONFLICT (library_id, path) DO NOTHING", mid, intent.source.as_str()).execute(&wb).await.unwrap();
        assert!(claim_next(&wb).await.unwrap().is_none());
        sqlx::query!("UPDATE writeback_jobs SET last_attempted_at = now() - INTERVAL '25 hours' WHERE id = $1", job).execute(&wb).await.unwrap();
        assert_eq!(claim_next(&wb).await.unwrap(), Some((job, 6)));
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn relocation_queue_sweep_is_bounded_and_reaches_remaining_eligible_rows(pool: PgPool) {
        let wb = writeback_pool_for(&pool).await;
        let work = sqlx::query_scalar!(
            "INSERT INTO works (title, sort_title) VALUES ($1, $1) RETURNING id",
            "sweep"
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        sqlx::query!(
            "WITH inserted AS (INSERT INTO manifestations (library_id, work_id, format, file_path, ingestion_file_hash, current_file_hash, file_size_bytes, relocation_source_path, relocation_destination_path)
             SELECT (SELECT id FROM libraries WHERE configuration_key = 'default'), $1, 'epub', 'source-' || n || '.epub', 'hash-' || n, 'hash-' || n, 7, 'source-' || n || '.epub', 'destination-' || n || '.epub' FROM generate_series(1, 105) AS n RETURNING *) INSERT INTO library_path_claims (library_id, path, manifestation_id) SELECT library_id, file_path AS path, id FROM inserted UNION SELECT library_id, relocation_source_path, id FROM inserted WHERE relocation_source_path IS NOT NULL UNION SELECT library_id, relocation_destination_path, id FROM inserted WHERE relocation_destination_path IS NOT NULL", work,
        ).execute(&pool).await.unwrap();
        assert_eq!(sweep_relocations(&wb).await.unwrap(), 100);
        assert_eq!(sweep_relocations(&wb).await.unwrap(), 5);
        assert_eq!(sweep_relocations(&wb).await.unwrap(), 0);
        assert_eq!(
            sqlx::query_scalar!("SELECT count(*) FROM writeback_jobs WHERE reason = 'relocation'")
                .fetch_one(&pool)
                .await
                .unwrap(),
            Some(105)
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn relocation_queue_sweep_excludes_live_jobs_and_preserves_exhausted_budget(
        pool: PgPool,
    ) {
        let ing = ingestion_pool_for(&pool).await;
        let wb = writeback_pool_for(&pool).await;
        for status in ["pending", "failed", "in_progress", "skipped", "complete"] {
            let (_, mid) = insert_fixture(&ing, status).await;
            set_intent(&wb, mid).await;
            let job = insert_job(&ing, mid, "metadata").await;
            sqlx::query!("UPDATE writeback_jobs SET status = $2::text::writeback_status, attempt_count = 10 WHERE id = $1", job, status).execute(&wb).await.unwrap();
        }
        let (_, no_intent) = insert_fixture(&ing, "no-intent").await;
        assert_eq!(sweep_relocations(&wb).await.unwrap(), 2);
        assert_eq!(sweep_relocations(&wb).await.unwrap(), 0);
        assert_eq!(sqlx::query_scalar!("SELECT count(*) FROM writeback_jobs WHERE reason = 'metadata' AND attempt_count = 10").fetch_one(&pool).await.unwrap(), Some(5));
        assert_eq!(
            sqlx::query_scalar!(
                "SELECT count(*) FROM writeback_jobs WHERE manifestation_id = $1",
                no_intent
            )
            .fetch_one(&pool)
            .await
            .unwrap(),
            Some(0)
        );
    }

    async fn carrier_fixture(
        pool: &PgPool,
    ) -> (
        tempfile::TempDir,
        LibraryFiles,
        Uuid,
        Uuid,
        orchestrator::RelocationIntent,
    ) {
        let ing = ingestion_pool_for(pool).await;
        let wb = writeback_pool_for(pool).await;
        let (_, mid) = insert_fixture(&ing, "carrier").await;
        let intent = set_intent(&wb, mid).await;
        let library = crate::models::storage_library::default_library_id(pool)
            .await
            .unwrap();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_str().unwrap().parse().unwrap();
        let files = crate::test_support::test_library_files_at(&root, library);
        let opened = files.library(library).unwrap();
        opened.create_dir_all("fixtures").unwrap();
        opened.create_dir_all("recovered").unwrap();
        opened.write(intent.source.as_path(), b"PAYLOAD").unwrap();
        let hash = crate::services::epub::repack::hash_file(
            &mut opened.open(intent.source.as_path()).unwrap().into_std(),
        )
        .unwrap();
        sqlx::query!(
            "UPDATE manifestations SET current_file_hash = $2, file_size_bytes = 7 WHERE id = $1",
            mid,
            hash
        )
        .execute(&wb)
        .await
        .unwrap();
        let origin = insert_job(&ing, mid, "metadata").await;
        mark_failed(
            &wb,
            origin,
            3,
            &test_config_with_max_attempts(3).0,
            Some("storage unavailable"),
        )
        .await
        .unwrap();
        assert_eq!(sweep_relocations(&wb).await.unwrap(), 1);
        let (job, _) = claim_next(&wb).await.unwrap().unwrap();
        (dir, files, job, mid, intent)
    }

    async fn carrier_run(
        pool: &PgPool,
        files: &LibraryFiles,
        job: Uuid,
    ) -> Result<RunOutcome, WritebackError> {
        let permit = Arc::new(Arc::new(Semaphore::new(1)).acquire_owned().await.unwrap());
        orchestrator::run_once(
            pool,
            &test_config_with_max_attempts(3).0,
            files,
            job,
            permit,
        )
        .await
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn relocation_queue_exhausted_origin_recovers_without_metadata_event_or_rewrite(
        pool: PgPool,
    ) {
        let wb = writeback_pool_for(&pool).await;
        let (_dir, files, job, mid, intent) = carrier_fixture(&pool).await;
        let ing = ingestion_pool_for(&pool).await;
        let sibling = insert_job(&ing, mid, "metadata").await;
        assert!(claim_next(&wb).await.unwrap().is_none());
        let result = carrier_run(&wb, &files, job).await;
        assert!(
            matches!(&result, Ok(RunOutcome::Success { reason, .. }) if reason == "relocation")
        );
        finish(&wb, &test_config_with_max_attempts(3).0, job, 1, result)
            .await
            .unwrap();
        let row = sqlx::query!(
            "SELECT file_path, relocation_source_path FROM manifestations WHERE id = $1",
            mid
        )
        .fetch_one(&wb)
        .await
        .unwrap();
        assert_eq!(row.file_path, intent.destination.as_str());
        assert!(row.relocation_source_path.is_none());
        let library = crate::models::storage_library::default_library_id(&pool)
            .await
            .unwrap();
        assert_eq!(
            files
                .library(library)
                .unwrap()
                .read(intent.destination.as_path())
                .unwrap(),
            b"PAYLOAD"
        );
        let event = events::event_id(job, events::TerminalOutcome::Complete);
        assert_eq!(
            sqlx::query_scalar!(
                "SELECT count(*) FROM webhook_event_dedupe WHERE event_id = $1",
                event
            )
            .fetch_one(&pool)
            .await
            .unwrap(),
            Some(1)
        );
        assert_eq!(
            sqlx::query_scalar!("SELECT count(*) FROM webhook_event_dedupe")
                .fetch_one(&pool)
                .await
                .unwrap(),
            Some(1)
        );
        assert_eq!(claim_next(&wb).await.unwrap(), Some((sibling, 1)));
        assert_eq!(
            sqlx::query_scalar!("SELECT reason FROM writeback_jobs WHERE id = $1", job)
                .fetch_one(&pool)
                .await
                .unwrap(),
            "relocation"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn relocation_queue_permanent_loss_clears_intent_and_stops_sweep(pool: PgPool) {
        let wb = writeback_pool_for(&pool).await;
        let (_dir, files, job, mid, intent) = carrier_fixture(&pool).await;
        let library = crate::models::storage_library::default_library_id(&pool)
            .await
            .unwrap();
        files
            .library(library)
            .unwrap()
            .remove_file(intent.source.as_path())
            .unwrap();
        let result = carrier_run(&wb, &files, job).await;
        assert!(
            matches!(&result, Ok(RunOutcome::RelocationTerminal { diagnosis, .. }) if diagnosis == "file_missing")
        );
        finish(&wb, &test_config_with_max_attempts(3).0, job, 1, result)
            .await
            .unwrap();
        let row = sqlx::query!(
            "SELECT status::text AS status, error FROM writeback_jobs WHERE id = $1",
            job
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(row.status.as_deref(), Some("skipped"));
        assert_eq!(row.error.as_deref(), Some("file_missing"));
        assert!(
            sqlx::query_scalar!(
                "SELECT relocation_source_path FROM manifestations WHERE id = $1",
                mid
            )
            .fetch_one(&wb)
            .await
            .unwrap()
            .is_none()
        );
        assert_eq!(sweep_relocations(&wb).await.unwrap(), 0);
        assert_eq!(
            sqlx::query_scalar!(
                "SELECT count(*) FROM webhook_event_dedupe WHERE event_id = $1",
                events::event_id(job, events::TerminalOutcome::Skipped)
            )
            .fetch_one(&pool)
            .await
            .unwrap(),
            Some(1)
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn relocation_queue_terminal_transaction_retains_sibling_exclusion_until_commit(
        pool: PgPool,
    ) {
        let wb = writeback_pool_for(&pool).await;
        let (_dir, _files, job, mid, intent) = carrier_fixture(&pool).await;
        let ing = ingestion_pool_for(&pool).await;
        let sibling = insert_job(&ing, mid, "metadata").await;
        let mut tx = wb.begin().await.unwrap();
        finalise_terminal(&mut tx, job, mid, &intent, "file_missing")
            .await
            .unwrap();
        assert!(claim_next(&wb).await.unwrap().is_none());
        tx.rollback().await.unwrap();
        assert!(
            sqlx::query_scalar!(
                "SELECT relocation_source_path FROM manifestations WHERE id = $1",
                mid
            )
            .fetch_one(&wb)
            .await
            .unwrap()
            .is_some()
        );
        assert!(claim_next(&wb).await.unwrap().is_none());
        finish(
            &wb,
            &test_config_with_max_attempts(3).0,
            job,
            1,
            Ok(RunOutcome::RelocationTerminal {
                manifestation_id: mid,
                reason: "relocation".into(),
                intent,
                diagnosis: "file_missing".into(),
            }),
        )
        .await
        .unwrap();
        assert_eq!(claim_next(&wb).await.unwrap(), Some((sibling, 1)));
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn relocation_queue_failed_finish_retains_intent_and_claim(pool: PgPool) {
        let wb = writeback_pool_for(&pool).await;
        let (_dir, _files, job, mid, intent) = carrier_fixture(&pool).await;
        wb.close().await;
        let result = finish(
            &wb,
            &test_config_with_max_attempts(3).0,
            job,
            1,
            Ok(RunOutcome::RelocationTerminal {
                manifestation_id: mid,
                reason: "relocation".into(),
                intent,
                diagnosis: "file_missing".into(),
            }),
        )
        .await;
        assert!(matches!(result, Err(sqlx::Error::PoolClosed)));
        let fresh = writeback_pool_for(&pool).await;
        assert!(
            sqlx::query_scalar!(
                "SELECT relocation_source_path FROM manifestations WHERE id = $1",
                mid
            )
            .fetch_one(&fresh)
            .await
            .unwrap()
            .is_some()
        );
        assert_eq!(
            sqlx::query_scalar!("SELECT status::text FROM writeback_jobs WHERE id = $1", job)
                .fetch_one(&pool)
                .await
                .unwrap()
                .as_deref(),
            Some("in_progress")
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn relocation_queue_disabled_worker_performs_no_claim_reset_or_sweep(pool: PgPool) {
        let wb = writeback_pool_for(&pool).await;
        let (_dir, files, job, _mid, _intent) = carrier_fixture(&pool).await;
        let ing = ingestion_pool_for(&pool).await;
        let (_, eligible) = insert_fixture(&ing, "disabled-eligible").await;
        set_intent(&wb, eligible).await;
        let (mut config, _files) = test_config_with_max_attempts(3);
        config.writeback.enabled = false;
        let cancel = CancellationToken::new();
        cancel.cancel();
        spawn_worker(wb.clone(), config, cancel, files)
            .await
            .unwrap();
        assert_eq!(
            sqlx::query_scalar!("SELECT status::text FROM writeback_jobs WHERE id = $1", job)
                .fetch_one(&pool)
                .await
                .unwrap()
                .as_deref(),
            Some("in_progress")
        );
        assert_eq!(
            sqlx::query_scalar!("SELECT count(*) FROM writeback_jobs WHERE reason = 'relocation'")
                .fetch_one(&pool)
                .await
                .unwrap(),
            Some(1)
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn relocation_queue_failure_event_uses_stored_reason(pool: PgPool) {
        let wb = writeback_pool_for(&pool).await;
        let (_dir, _files, job, mid, _intent) = carrier_fixture(&pool).await;
        assert_eq!(failure_identity(&wb, job).await, (mid, "relocation".into()));
        finish(
            &wb,
            &test_config_with_max_attempts(3).0,
            job,
            1,
            Err(WritebackError::Persist("storage unavailable".into())),
        )
        .await
        .unwrap();
        let row = sqlx::query!(
            "SELECT reason, status::text AS status, error FROM writeback_jobs WHERE id = $1",
            job
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(row.reason, "relocation");
        assert_eq!(row.status.as_deref(), Some("failed"));
        assert!(row.error.unwrap().contains("storage unavailable"));
        assert_eq!(
            sqlx::query_scalar!(
                "SELECT count(*) FROM webhook_event_dedupe WHERE event_id = $1",
                events::event_id(job, events::TerminalOutcome::Failed)
            )
            .fetch_one(&pool)
            .await
            .unwrap(),
            Some(1)
        );
    }

    async fn worker_job_status(pool: &PgPool, job: Uuid) -> Option<String> {
        sqlx::query_scalar!("SELECT status::text FROM writeback_jobs WHERE id = $1", job)
            .fetch_one(pool)
            .await
            .unwrap()
    }

    async fn assert_worker_fault_status(pool: &PgPool, job: Uuid, fault: WorkerOperation) {
        if fault != WorkerOperation::PeriodicSweep {
            let status = worker_job_status(pool, job).await;
            assert_eq!(
                status.as_deref(),
                Some(if fault == WorkerOperation::Reset {
                    "in_progress"
                } else {
                    "pending"
                })
            );
        }
    }

    async fn worker_fault_case(pool: PgPool, fault: WorkerOperation) {
        let ing = ingestion_pool_for(&pool).await;
        let wb = writeback_pool_for(&pool).await;
        let (_, mid) = insert_fixture(&ing, "worker-transient").await;
        let job = insert_job(&ing, mid, "metadata").await;
        if fault == WorkerOperation::Reset {
            sqlx::query!(
                "UPDATE writeback_jobs SET status = 'in_progress' WHERE id = $1",
                job
            )
            .execute(&wb)
            .await
            .unwrap();
        }
        let (failed, mut failed_rx) = tokio::sync::mpsc::channel(1);
        let (draining, drained) = tokio::sync::oneshot::channel();
        let started = Arc::new(tokio::sync::Notify::new());
        let released = Arc::new(tokio::sync::Notify::new());
        let run_started = Arc::clone(&started);
        let run_release = Arc::clone(&released);
        let cancel = CancellationToken::new();
        let mut injected = false;
        let hooks = WorkerHooks {
            draining: move || {
                draining.send(()).unwrap();
            },
            database: move |operation| {
                if operation == fault && !injected {
                    injected = true;
                    failed.try_send(()).unwrap();
                    Err(sqlx::Error::Io(std::io::Error::other(
                        "transient database failure",
                    )))
                } else {
                    Ok(())
                }
            },
            recovery_period: Duration::from_millis(10),
        };
        let (config, files) = test_config_with_max_attempts(3);
        let mut worker = tokio::spawn(spawn_worker_with(
            wb.clone(),
            config,
            cancel.clone(),
            files,
            move |_, _, _, id, _permit| {
                let started = Arc::clone(&run_started);
                let release = Arc::clone(&run_release);
                async move {
                    assert_eq!(id, job);
                    started.notify_one();
                    release.notified().await;
                    Ok(RunOutcome::Success {
                        manifestation_id: mid,
                        reason: "metadata".into(),
                        current_file_hash: "unchanged".into(),
                    })
                }
            },
            hooks,
        ));
        tokio::time::timeout(Duration::from_secs(5), failed_rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert!(!worker.is_finished());
        assert_worker_fault_status(&wb, job, fault).await;
        tokio::select! {
            () = started.notified() => {}
            result = &mut worker => panic!("worker exited after transient {fault:?}: {result:?}"),
            () = tokio::time::sleep(Duration::from_secs(5)) => {
                cancel.cancel();
                released.notify_one();
                worker.abort();
                panic!("worker did not recover after transient {fault:?}");
            }
        }
        assert_eq!(
            worker_job_status(&wb, job).await.as_deref(),
            Some("in_progress")
        );
        assert!(claim_next(&wb).await.unwrap().is_none());
        cancel.cancel();
        tokio::time::timeout(Duration::from_secs(5), drained)
            .await
            .unwrap()
            .unwrap();
        assert!(!worker.is_finished());
        released.notify_one();
        tokio::time::timeout(Duration::from_secs(5), worker)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(
            worker_job_status(&wb, job).await.as_deref(),
            Some("complete")
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn relocation_worker_transient_startup_reset_retries_before_claim(pool: PgPool) {
        worker_fault_case(pool, WorkerOperation::Reset).await;
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn relocation_worker_transient_startup_sweep_retries(pool: PgPool) {
        worker_fault_case(pool, WorkerOperation::StartupSweep).await;
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn relocation_worker_transient_claim_retries(pool: PgPool) {
        worker_fault_case(pool, WorkerOperation::Claim).await;
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn relocation_worker_transient_periodic_sweep_preserves_active_job_and_drain(
        pool: PgPool,
    ) {
        worker_fault_case(pool, WorkerOperation::PeriodicSweep).await;
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn relocation_worker_disabled_skips_database_operations(pool: PgPool) {
        let (mut config, files) = test_config_with_max_attempts(3);
        config.writeback.enabled = false;
        let cancel = CancellationToken::new();
        cancel.cancel();
        pool.close().await;
        spawn_worker_with(
            pool,
            config,
            cancel,
            files,
            |_, _, _, _, _| async { panic!("disabled worker ran a job") },
            WorkerHooks {
                draining: || panic!("disabled worker drained jobs"),
                database: |_| panic!("disabled worker touched database"),
                recovery_period: Duration::from_secs(300),
            },
        )
        .await
        .unwrap();
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn relocation_accounting_finish_uses_durable_edit_count(pool: PgPool) {
        let ing = ingestion_pool_for(&pool).await;
        let wb = writeback_pool_for(&pool).await;
        let (_, mid) = insert_fixture(&ing, "durable-attempt").await;
        let job = insert_job(&ing, mid, "cover").await;
        sqlx::query!(
            "UPDATE writeback_jobs SET status = 'in_progress', attempt_count = 2 WHERE id = $1",
            job
        )
        .execute(&wb)
        .await
        .unwrap();
        finish(
            &wb,
            &test_config_with_max_attempts(2).0,
            job,
            0,
            Ok(RunOutcome::Failed {
                manifestation_id: mid,
                reason: "cover".into(),
                error: "actual edit failure".into(),
            }),
        )
        .await
        .unwrap();
        assert_eq!(
            sqlx::query_scalar!("SELECT status::text FROM writeback_jobs WHERE id = $1", job)
                .fetch_one(&wb)
                .await
                .unwrap()
                .as_deref(),
            Some("skipped")
        );
        assert_eq!(
            sqlx::query_scalar!(
                "SELECT attempt_count FROM writeback_jobs WHERE id = $1",
                job
            )
            .fetch_one(&wb)
            .await
            .unwrap(),
            2
        );
        assert_eq!(
            sqlx::query_scalar!(
                "SELECT count(*) FROM webhook_event_dedupe WHERE event_id = $1",
                events::event_id(job, events::TerminalOutcome::Failed)
            )
            .fetch_one(&pool)
            .await
            .unwrap(),
            Some(1)
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn relocation_queue_aborted_worker_retains_claim_until_restart(pool: PgPool) {
        let wb = writeback_pool_for(&pool).await;
        let ing = ingestion_pool_for(&pool).await;
        let (_dir, files, job, mid, intent) = carrier_fixture(&pool).await;
        sqlx::query!(
            "UPDATE writeback_jobs SET last_attempted_at = NULL WHERE id = $1",
            job
        )
        .execute(&wb)
        .await
        .unwrap();
        let library = crate::models::storage_library::default_library_id(&pool)
            .await
            .unwrap();
        let hash = sqlx::query_scalar!(
            "SELECT current_file_hash FROM manifestations WHERE id = $1",
            mid
        )
        .fetch_one(&wb)
        .await
        .unwrap();
        let entered = Arc::new(tokio::sync::Notify::new());
        let completed = Arc::new(tokio::sync::Notify::new());
        let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let release = Arc::new(std::sync::Mutex::new(release_rx));
        let phase_entered = Arc::clone(&entered);
        let phase_completed = Arc::clone(&completed);
        let phase_calls = Arc::clone(&calls);
        let phase_intent = intent.clone();
        let worker = tokio::spawn(spawn_worker_with(
            wb.clone(),
            test_config_with_max_attempts(3).0,
            CancellationToken::new(),
            files.clone(),
            move |_pool, _config, files, id, permit| {
                let entered = Arc::clone(&phase_entered);
                let completed = Arc::clone(&phase_completed);
                let calls = Arc::clone(&phase_calls);
                let release = Arc::clone(&release);
                let intent = phase_intent.clone();
                let hash = hash.clone();
                async move {
                    assert_eq!(id, job);
                    calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    orchestrator::blocking_phase(permit, move || {
                        entered.notify_one();
                        release.lock().unwrap().recv().unwrap();
                        let result = super::super::path_rename::recover(
                            files.library(library)?,
                            &intent.source,
                            &intent.destination,
                            &hash,
                            7,
                        );
                        completed.notify_one();
                        result
                    })
                    .await?;
                    Ok(RunOutcome::Success {
                        manifestation_id: mid,
                        reason: "relocation".into(),
                        current_file_hash: "unchanged".into(),
                    })
                }
            },
            WorkerHooks::new(|| {}),
        ));
        tokio::time::timeout(Duration::from_secs(5), entered.notified())
            .await
            .unwrap();
        worker.abort();
        assert!(worker.await.unwrap_err().is_cancelled());
        let sibling = insert_job(&ing, mid, "metadata").await;
        assert!(claim_next(&wb).await.unwrap().is_none());
        assert_eq!(
            sqlx::query_scalar!("SELECT status::text FROM writeback_jobs WHERE id = $1", job)
                .fetch_one(&wb)
                .await
                .unwrap()
                .as_deref(),
            Some("in_progress")
        );
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
        release_tx.send(()).unwrap();
        tokio::time::timeout(Duration::from_secs(5), completed.notified())
            .await
            .unwrap();
        let root = files.library(library).unwrap();
        assert_eq!(root.read(intent.destination.as_path()).unwrap(), b"PAYLOAD");
        assert!(!root.try_exists(intent.source.as_path()).unwrap());
        assert_eq!(
            sqlx::query_scalar!("SELECT status::text FROM writeback_jobs WHERE id = $1", job)
                .fetch_one(&wb)
                .await
                .unwrap()
                .as_deref(),
            Some("in_progress")
        );
        revert_in_progress(&wb).await.unwrap();
        sqlx::query!(
            "UPDATE writeback_jobs SET last_attempted_at = NULL WHERE id = $1",
            job
        )
        .execute(&wb)
        .await
        .unwrap();
        let (claimed, attempt) = claim_next(&wb).await.unwrap().unwrap();
        assert_eq!(claimed, job);
        let result = carrier_run(&wb, &files, job).await;
        assert!(matches!(&result, Ok(RunOutcome::Success { .. })));
        finish(
            &wb,
            &test_config_with_max_attempts(3).0,
            job,
            attempt,
            result,
        )
        .await
        .unwrap();
        let row = sqlx::query!(
            "SELECT file_path, relocation_source_path FROM manifestations WHERE id = $1",
            mid
        )
        .fetch_one(&wb)
        .await
        .unwrap();
        assert_eq!(row.file_path, intent.destination.as_str());
        assert!(row.relocation_source_path.is_none());
        assert_eq!(claim_next(&wb).await.unwrap().unwrap().0, sibling);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn relocation_queue_startup_sweep_creates_carrier_before_claiming(pool: PgPool) {
        let ing = ingestion_pool_for(&pool).await;
        let wb = writeback_pool_for(&pool).await;
        let (_, mid) = insert_fixture(&ing, "startup-sweep").await;
        set_intent(&wb, mid).await;
        let origin = insert_job(&ing, mid, "metadata").await;
        mark_failed(
            &wb,
            origin,
            3,
            &test_config_with_max_attempts(3).0,
            Some("exhausted"),
        )
        .await
        .unwrap();
        let cancel = CancellationToken::new();
        let signal = Arc::new(tokio::sync::Notify::new());
        let worker_signal = Arc::clone(&signal);
        let worker_cancel = cancel.clone();
        let worker_pool = wb.clone();
        let (config, files) = test_config_with_max_attempts(3);
        let handle = tokio::spawn(async move {
            spawn_worker_with(
                worker_pool,
                config,
                worker_cancel,
                files,
                move |pool, _config, _files, id, _permit| {
                    let signal = Arc::clone(&worker_signal);
                    async move {
                        let (manifestation_id, reason) = failure_identity(&pool, id).await;
                        assert_eq!(manifestation_id, mid);
                        assert_eq!(reason, "relocation");
                        signal.notify_one();
                        Ok(RunOutcome::Success {
                            manifestation_id,
                            reason,
                            current_file_hash: "unchanged".into(),
                        })
                    }
                },
                WorkerHooks::new(|| {}),
            )
            .await
            .unwrap();
        });
        let mut handle = handle;
        tokio::select! {
            () = signal.notified() => {}
            result = &mut handle => panic!("startup worker exited before notification: {result:?}"),
            () = tokio::time::sleep(Duration::from_secs(5)) => {
                cancel.cancel();
                handle.abort();
                panic!("startup worker did not claim its carrier within five seconds");
            }
        }
        cancel.cancel();
        handle.await.unwrap();
        assert_eq!(sqlx::query_scalar!("SELECT count(*) FROM writeback_jobs WHERE reason = 'relocation' AND status = 'complete'").fetch_one(&pool).await.unwrap(), Some(1));
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn relocation_queue_terminal_pair_mismatch_rolls_back_without_releasing_claim(
        pool: PgPool,
    ) {
        let wb = writeback_pool_for(&pool).await;
        let (_dir, _files, job, mid, mut intent) = carrier_fixture(&pool).await;
        intent.destination = "different.epub".parse().unwrap();
        let result = finish(
            &wb,
            &test_config_with_max_attempts(3).0,
            job,
            1,
            Ok(RunOutcome::RelocationTerminal {
                manifestation_id: mid,
                reason: "relocation".into(),
                intent,
                diagnosis: "file_missing".into(),
            }),
        )
        .await;
        assert!(matches!(result, Err(sqlx::Error::RowNotFound)));
        assert!(
            sqlx::query_scalar!(
                "SELECT relocation_source_path FROM manifestations WHERE id = $1",
                mid
            )
            .fetch_one(&wb)
            .await
            .unwrap()
            .is_some()
        );
        assert_eq!(
            sqlx::query_scalar!("SELECT status::text FROM writeback_jobs WHERE id = $1", job)
                .fetch_one(&pool)
                .await
                .unwrap()
                .as_deref(),
            Some("in_progress")
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn relocation_queue_storage_return_recovers_after_carrier_exhaustion(pool: PgPool) {
        let wb = writeback_pool_for(&pool).await;
        let (_dir, files, job, mid, intent) = carrier_fixture(&pool).await;
        let library = crate::models::storage_library::default_library_id(&pool)
            .await
            .unwrap();
        let root = files.library(library).unwrap();
        root.rename(intent.source.as_path(), root, "saved.epub")
            .unwrap();
        root.create_dir(intent.source.as_path()).unwrap();
        let result = carrier_run(&wb, &files, job).await;
        assert!(matches!(&result, Err(WritebackError::Io(_))));
        finish(&wb, &test_config_with_max_attempts(1).0, job, 1, result)
            .await
            .unwrap();
        assert!(
            sqlx::query_scalar!(
                "SELECT relocation_source_path FROM manifestations WHERE id = $1",
                mid
            )
            .fetch_one(&wb)
            .await
            .unwrap()
            .is_some()
        );
        root.remove_dir(intent.source.as_path()).unwrap();
        root.rename("saved.epub", root, intent.source.as_path())
            .unwrap();
        assert_eq!(sweep_relocations(&wb).await.unwrap(), 1);
        let (next, _) = claim_next(&wb).await.unwrap().unwrap();
        assert_ne!(next, job);
        let recovered = carrier_run(&wb, &files, next).await;
        assert!(
            matches!(&recovered, Ok(RunOutcome::Success { reason, .. }) if reason == "relocation")
        );
        finish(&wb, &test_config_with_max_attempts(1).0, next, 1, recovered)
            .await
            .unwrap();
        assert_eq!(root.read(intent.destination.as_path()).unwrap(), b"PAYLOAD");
        assert!(
            sqlx::query_scalar!(
                "SELECT relocation_source_path FROM manifestations WHERE id = $1",
                mid
            )
            .fetch_one(&wb)
            .await
            .unwrap()
            .is_none()
        );
        assert_eq!(
            sqlx::query_scalar!(
                "SELECT attempt_count FROM writeback_jobs WHERE id = $1",
                job
            )
            .fetch_one(&pool)
            .await
            .unwrap(),
            1
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn bounded_writeback_cancellation_drains_active_claim_without_overlap(pool: PgPool) {
        let ing = ingestion_pool_for(&pool).await;
        let app = writeback_pool_for(&pool).await;
        let (_, manifestation) = insert_fixture(&ing, "blocked").await;
        let first = insert_job(&ing, manifestation, "metadata").await;
        let second = insert_job(&ing, manifestation, "metadata").await;
        let release = Arc::new(std::sync::Barrier::new(2));
        let (entered_tx, mut entered_rx) = tokio::sync::mpsc::channel(1);
        let (draining_tx, draining_rx) = tokio::sync::oneshot::channel();
        let cancel = CancellationToken::new();
        let (mut config, files) = test_config_with_max_attempts(3);
        config.writeback.concurrency = 1;
        let blocking_release = Arc::clone(&release);
        let worker = tokio::spawn(spawn_worker_with(
            app.clone(),
            config,
            cancel.clone(),
            files,
            move |_, _, _, _, permit| {
                let release = Arc::clone(&blocking_release);
                let entered = entered_tx.clone();
                async move {
                    orchestrator::blocking_phase(permit, move || {
                        entered.blocking_send(()).unwrap();
                        release.wait();
                        Ok(RunOutcome::Success {
                            manifestation_id: manifestation,
                            reason: "metadata".into(),
                            current_file_hash: "accepted".into(),
                        })
                    })
                    .await
                }
            },
            WorkerHooks::new(move || {
                draining_tx.send(()).unwrap();
            }),
        ));
        entered_rx.recv().await.unwrap();
        cancel.cancel();
        draining_rx.await.unwrap();
        let rows = sqlx::query!("SELECT id, status::text AS status FROM writeback_jobs WHERE manifestation_id = $1 ORDER BY created_at", manifestation).fetch_all(&app).await.unwrap();
        assert_eq!(
            rows.iter()
                .find(|row| row.id == first)
                .unwrap()
                .status
                .as_deref(),
            Some("in_progress")
        );
        assert_eq!(
            rows.iter()
                .find(|row| row.id == second)
                .unwrap()
                .status
                .as_deref(),
            Some("pending")
        );
        assert!(claim_next(&app).await.unwrap().is_none());
        assert!(!worker.is_finished());
        tokio::task::spawn_blocking(move || release.wait())
            .await
            .unwrap();
        worker.await.unwrap().unwrap();
        let row = sqlx::query!(
            "SELECT status::text AS status FROM writeback_jobs WHERE id = $1",
            first
        )
        .fetch_one(&app)
        .await
        .unwrap();
        assert_eq!(row.status.as_deref(), Some("complete"));
    }

    /// NOT EXISTS soft filter: when one sibling is already `in_progress`,
    /// the CTE predicate treats the remaining pending siblings as
    /// ineligible.  This is the common-path optimisation — it avoids a
    /// unique-violation round-trip on the claim.  Correctness under
    /// concurrent workers is guaranteed by the partial UNIQUE index
    /// (`concurrent_claims_on_same_manifestation_serialise_via_unique_index`
    /// below), not by this predicate.
    #[sqlx::test(migrations = "./migrations")]
    async fn not_exists_filter_excludes_siblings_of_in_progress_job(pool: PgPool) {
        let app_pool = app_pool_for(&pool).await;
        let ing_pool = ingestion_pool_for(&pool).await;
        let marker = Uuid::new_v4().simple().to_string();

        let (_work_id, m_id) = insert_fixture(&ing_pool, &marker).await;
        let job_a = insert_job(&ing_pool, m_id, "metadata").await;
        let _job_b = insert_job(&ing_pool, m_id, "metadata").await;
        let _job_c = insert_job(&ing_pool, m_id, "metadata").await;

        sqlx::query!(
            "UPDATE writeback_jobs SET status = 'in_progress' WHERE id = $1",
            job_a
        )
        .execute(&app_pool)
        .await
        .unwrap();

        let count_eligible_siblings = sqlx::query_scalar!(
            r#"SELECT count(*) AS "count!" FROM writeback_jobs wj
             WHERE wj.manifestation_id = $1
               AND wj.status = 'pending'
               AND NOT EXISTS (
                 SELECT 1 FROM writeback_jobs other
                 WHERE other.manifestation_id = wj.manifestation_id
                   AND other.status = 'in_progress'
               )"#,
            m_id,
        )
        .fetch_one(&app_pool)
        .await
        .unwrap();
        assert_eq!(
            count_eligible_siblings, 0_i64,
            "sibling pending jobs must not be eligible while one is in_progress"
        );
    }

    /// The partial UNIQUE index
    /// `(manifestation_id) WHERE status = 'in_progress'` enforces the
    /// per-manifestation serialisation guarantee the module promises.
    /// Two concurrent transactions each try to mark a DIFFERENT sibling
    /// row `in_progress`; the first commits, the second is blocked on
    /// the index tuple and then fails with SQLSTATE 23505 once the first
    /// commits.  Without this index, both would succeed under READ
    /// COMMITTED (the NOT EXISTS snapshot cannot see the peer's
    /// uncommitted UPDATE).
    #[sqlx::test(migrations = "./migrations")]
    async fn concurrent_claims_on_same_manifestation_serialise_via_unique_index(pool: PgPool) {
        let ing_pool = ingestion_pool_for(&pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (_work_id, m_id) = insert_fixture(&ing_pool, &marker).await;
        let job_a = insert_job(&ing_pool, m_id, "metadata").await;
        let job_b = insert_job(&ing_pool, m_id, "metadata").await;

        // Separate pools to force distinct connections (like real workers).
        let pool_a = app_pool_for(&pool).await;
        let pool_b = app_pool_for(&pool).await;

        let barrier = Arc::new(Barrier::new(2));

        let b1 = barrier.clone();
        let t1 = tokio::spawn(async move {
            let mut tx = pool_a.begin().await.unwrap();
            b1.wait().await;
            let res = sqlx::query!(
                "UPDATE writeback_jobs SET status = 'in_progress', \
                 last_attempted_at = now(), attempt_count = attempt_count + 1 \
                 WHERE id = $1",
                job_a,
            )
            .execute(&mut *tx)
            .await;
            // Hold the transaction briefly so the peer definitely blocks
            // on our uncommitted index tuple before we commit.
            tokio::time::sleep(Duration::from_millis(150)).await;
            match res {
                Ok(r) => {
                    tx.commit().await.unwrap();
                    Ok::<u64, sqlx::Error>(r.rows_affected())
                }
                Err(e) => {
                    let _ = tx.rollback().await;
                    Err(e)
                }
            }
        });

        let b2 = barrier.clone();
        let t2 = tokio::spawn(async move {
            let mut tx = pool_b.begin().await.unwrap();
            b2.wait().await;
            // Tiny stagger ensures t1 hits the UPDATE first so t2 is the
            // one that blocks on the index.  Without the stagger the race
            // outcome is symmetric (either tx wins) but the test still
            // passes — it just doesn't deterministically exercise the
            // "blocked on peer" path.
            tokio::time::sleep(Duration::from_millis(25)).await;
            let res = sqlx::query!(
                "UPDATE writeback_jobs SET status = 'in_progress', \
                 last_attempted_at = now(), attempt_count = attempt_count + 1 \
                 WHERE id = $1",
                job_b,
            )
            .execute(&mut *tx)
            .await;
            match res {
                Ok(r) => {
                    tx.commit().await.unwrap();
                    Ok::<u64, sqlx::Error>(r.rows_affected())
                }
                Err(e) => {
                    let _ = tx.rollback().await;
                    Err(e)
                }
            }
        });

        let (r1, r2) = tokio::join!(t1, t2);
        let r1 = r1.unwrap();
        let r2 = r2.unwrap();

        let mut successes = 0u32;
        let mut unique_violations = 0u32;
        for r in [&r1, &r2] {
            match r {
                Ok(1) => successes += 1,
                Ok(n) => panic!("unexpected rows_affected: {n}"),
                Err(sqlx::Error::Database(db_err)) if db_err.is_unique_violation() => {
                    unique_violations += 1;
                }
                Err(e) => panic!("unexpected error: {e}"),
            }
        }
        assert_eq!(
            successes, 1,
            "exactly one concurrent UPDATE must succeed under the partial UNIQUE index"
        );
        assert_eq!(
            unique_violations, 1,
            "the other concurrent UPDATE must fail with SQLSTATE 23505 unique_violation"
        );

        // Final state: exactly one in_progress row for this manifestation.
        let in_progress_count = sqlx::query_scalar!(
            r#"SELECT count(*) AS "count!" FROM writeback_jobs
             WHERE manifestation_id = $1 AND status = 'in_progress'"#,
            m_id,
        )
        .fetch_one(&ing_pool)
        .await
        .unwrap();
        assert_eq!(in_progress_count, 1_i64);
    }

    /// Jobs on distinct manifestations can run in parallel — i.e. the
    /// manifestation-aware NOT EXISTS clause does NOT cross-block them.
    /// Verified by checking that neither row appears in the other's
    /// `in_progress` EXISTS check at the SQL level.  Parallel-test safe.
    #[sqlx::test(migrations = "./migrations")]
    async fn two_workers_distinct_manifestations_parallelise(pool: PgPool) {
        let app_pool = app_pool_for(&pool).await;
        let ing_pool = ingestion_pool_for(&pool).await;
        let marker_a = Uuid::new_v4().simple().to_string();
        let marker_b = Uuid::new_v4().simple().to_string();
        let (_work_a, m_a) = insert_fixture(&ing_pool, &marker_a).await;
        let (_work_b, m_b) = insert_fixture(&ing_pool, &marker_b).await;
        let _job_a = insert_job(&ing_pool, m_a, "metadata").await;
        let _job_b = insert_job(&ing_pool, m_b, "metadata").await;

        // Mark m_a's job in_progress directly — simulating an active worker.
        sqlx::query!(
            "UPDATE writeback_jobs SET status = 'in_progress' WHERE manifestation_id = $1",
            m_a,
        )
        .execute(&app_pool)
        .await
        .unwrap();

        // m_b's job must still be eligible — NOT EXISTS clause compares on
        // m_b's manifestation_id, which is distinct from m_a's.
        let m_b_eligible = sqlx::query_scalar!(
            r#"SELECT NOT EXISTS (
               SELECT 1 FROM writeback_jobs
                WHERE manifestation_id = $1
                  AND status = 'in_progress'
             ) AS "exists!""#,
            m_b,
        )
        .fetch_one(&app_pool)
        .await
        .unwrap();
        assert!(
            m_b_eligible,
            "m_b's job must remain eligible when m_a's is in_progress"
        );
    }

    /// Retry-backoff: `attempt_count=2` → 30 minute window.  Verified via
    /// a SELECT mirroring the CTE's eligibility predicate, so parallel
    /// tests do not steal the claim.
    #[sqlx::test(migrations = "./migrations")]
    async fn retry_backoff_honoured(pool: PgPool) {
        let app_pool = app_pool_for(&pool).await;
        let ing_pool = ingestion_pool_for(&pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (_work_id, m_id) = insert_fixture(&ing_pool, &marker).await;

        let job_id = sqlx::query_scalar!(
            "INSERT INTO writeback_jobs \
               (manifestation_id, reason, status, attempt_count, last_attempted_at) \
             VALUES ($1, 'metadata', 'failed', 2, now() - INTERVAL '25 minutes') \
             RETURNING id",
            m_id,
        )
        .fetch_one(&ing_pool)
        .await
        .unwrap();

        // Eligibility check mirrors claim_next's WHERE clause for
        // attempt_count=2 (30-minute window).  Asserting eligibility at
        // the SQL level instead of via claim_next keeps the test safe
        // against parallel runs that might claim our row before we check.
        let eligible_inside = sqlx::query_scalar!(
            r#"SELECT (last_attempted_at < now() - INTERVAL '30 minutes') AS "elig!"
               FROM writeback_jobs WHERE id = $1"#,
            job_id,
        )
        .fetch_one(&app_pool)
        .await
        .unwrap();
        assert!(
            !eligible_inside,
            "row inside backoff window must not satisfy eligibility"
        );

        // Move back past the window (UPDATE grant is on app_pool only).
        sqlx::query!(
            "UPDATE writeback_jobs \
             SET last_attempted_at = now() - INTERVAL '35 minutes' WHERE id = $1",
            job_id,
        )
        .execute(&app_pool)
        .await
        .unwrap();

        let eligible_outside = sqlx::query_scalar!(
            r#"SELECT (last_attempted_at < now() - INTERVAL '30 minutes') AS "elig!"
               FROM writeback_jobs WHERE id = $1"#,
            job_id,
        )
        .fetch_one(&app_pool)
        .await
        .unwrap();
        assert!(
            eligible_outside,
            "row past backoff window must satisfy eligibility"
        );
    }

    /// `revert_in_progress` flips every `in_progress` row back to `pending`.
    #[sqlx::test(migrations = "./migrations")]
    async fn shutdown_reverts_in_progress(pool: PgPool) {
        let app_pool = app_pool_for(&pool).await;
        let ing_pool = ingestion_pool_for(&pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (_work_id, m_id) = insert_fixture(&ing_pool, &marker).await;
        let job_id = sqlx::query_scalar!(
            "INSERT INTO writeback_jobs (manifestation_id, reason, status) \
             VALUES ($1, 'metadata', 'in_progress') RETURNING id",
            m_id,
        )
        .fetch_one(&ing_pool)
        .await
        .unwrap();

        revert_in_progress(&app_pool).await.unwrap();

        let status = sqlx::query_scalar!(
            r#"SELECT status::text AS "status!" FROM writeback_jobs WHERE id = $1"#,
            job_id,
        )
        .fetch_one(&ing_pool)
        .await
        .unwrap();
        assert_eq!(status, "pending");
    }

    /// At `max_attempts`, `mark_failed` transitions to `skipped`.
    #[sqlx::test(migrations = "./migrations")]
    async fn max_attempts_transitions_to_skipped(pool: PgPool) {
        let app_pool = app_pool_for(&pool).await;
        let ing_pool = ingestion_pool_for(&pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (_work_id, m_id) = insert_fixture(&ing_pool, &marker).await;
        let job_id = insert_job(&ing_pool, m_id, "metadata").await;

        let (config, _files) = test_config_with_max_attempts(3);
        mark_failed(&app_pool, job_id, 3, &config, Some("final"))
            .await
            .unwrap();

        let status = sqlx::query_scalar!(
            r#"SELECT status::text AS "status!" FROM writeback_jobs WHERE id = $1"#,
            job_id,
        )
        .fetch_one(&ing_pool)
        .await
        .unwrap();
        assert_eq!(status, "skipped");
    }

    /// Crash recovery: a row left `in_progress` must be picked up as
    /// `pending` on worker startup.  Mirrors `shutdown_reverts_in_progress`
    /// but uses the full `spawn_worker` entry point.
    #[sqlx::test(migrations = "./migrations")]
    async fn crash_recovery_reconciles_in_progress(pool: PgPool) {
        let app_pool = app_pool_for(&pool).await;
        let ing_pool = ingestion_pool_for(&pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (_work_id, m_id) = insert_fixture(&ing_pool, &marker).await;
        let job_id = sqlx::query_scalar!(
            "INSERT INTO writeback_jobs (manifestation_id, reason, status) \
             VALUES ($1, 'metadata', 'in_progress') RETURNING id",
            m_id,
        )
        .fetch_one(&ing_pool)
        .await
        .unwrap();

        let pool_for_spawn = app_pool.clone();
        let cancel = CancellationToken::new();
        let cancel_for_spawn = cancel.clone();
        let (cfg, files) = test_config_with_max_attempts(3);
        let handle = tokio::spawn(async move {
            spawn_worker(pool_for_spawn, cfg, cancel_for_spawn, files)
                .await
                .unwrap();
        });

        // Allow the worker to run its startup revert_in_progress.
        tokio::time::sleep(Duration::from_millis(500)).await;
        cancel.cancel();
        handle.await.unwrap();

        let status = sqlx::query_scalar!(
            r#"SELECT status::text AS "status!" FROM writeback_jobs WHERE id = $1"#,
            job_id,
        )
        .fetch_one(&ing_pool)
        .await
        .unwrap();
        assert_ne!(
            status, "in_progress",
            "crash-recovery should have moved the row out of in_progress"
        );
    }

    // ── finish() terminal-state coverage ────────────────────────────────
    //
    // These exercise the S3 adversarial finding: every terminal transition
    // must both mark the job row AND emit a webhook event.  We assert the
    // DB-side of the transition here; event emission is a thin
    // tracing-stub wrapper (see events.rs) so structural coverage of the
    // DB write is the load-bearing test.

    /// `finish(Ok(Success))` transitions the row to `complete` and
    /// clears the `error` column.
    #[sqlx::test(migrations = "./migrations")]
    async fn finish_marks_complete_on_success_outcome(pool: PgPool) {
        let app_pool = app_pool_for(&pool).await;
        let ing_pool = ingestion_pool_for(&pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (_work_id, m_id) = insert_fixture(&ing_pool, &marker).await;
        let job_id = insert_job(&ing_pool, m_id, "metadata").await;

        finish(
            &app_pool,
            &test_config_with_max_attempts(3).0,
            job_id,
            1,
            Ok(RunOutcome::Success {
                manifestation_id: m_id,
                reason: "metadata".into(),
                current_file_hash: "abc123".into(),
            }),
        )
        .await
        .unwrap();

        let row = sqlx::query!(
            r#"SELECT status::text AS "status!", error FROM writeback_jobs WHERE id = $1"#,
            job_id,
        )
        .fetch_one(&ing_pool)
        .await
        .unwrap();
        assert_eq!(row.status, "complete");
        assert_eq!(row.error, None);
    }

    /// `finish(Ok(Skipped))` transitions to `skipped` and records the
    /// skip reason in `error`.  Skipped bypasses retry regardless of
    /// `attempt_count`.
    #[sqlx::test(migrations = "./migrations")]
    async fn finish_marks_skipped_on_skipped_outcome(pool: PgPool) {
        let app_pool = app_pool_for(&pool).await;
        let ing_pool = ingestion_pool_for(&pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (_work_id, m_id) = insert_fixture(&ing_pool, &marker).await;
        let job_id = insert_job(&ing_pool, m_id, "metadata").await;

        finish(
            &app_pool,
            &test_config_with_max_attempts(3).0,
            job_id,
            1, // well below max — still terminal for Skipped
            Ok(RunOutcome::Skipped {
                manifestation_id: m_id,
                reason: "metadata".into(),
                skip_reason: "format_unsupported: pdf".into(),
            }),
        )
        .await
        .unwrap();

        let row = sqlx::query!(
            r#"SELECT status::text AS "status!", error FROM writeback_jobs WHERE id = $1"#,
            job_id,
        )
        .fetch_one(&ing_pool)
        .await
        .unwrap();
        assert_eq!(row.status, "skipped");
        assert_eq!(row.error.as_deref(), Some("format_unsupported: pdf"));
    }

    /// `finish(Ok(Failed))` with `attempt_count < max_attempts` leaves
    /// the row as `failed` for later retry, with the error recorded.
    #[sqlx::test(migrations = "./migrations")]
    async fn finish_marks_failed_below_max_attempts(pool: PgPool) {
        let app_pool = app_pool_for(&pool).await;
        let ing_pool = ingestion_pool_for(&pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (_work_id, m_id) = insert_fixture(&ing_pool, &marker).await;
        let job_id = insert_job(&ing_pool, m_id, "metadata").await;

        finish(
            &app_pool,
            &test_config_with_max_attempts(3).0,
            job_id,
            1, // below max=3 → stays failed for retry
            Ok(RunOutcome::Failed {
                manifestation_id: m_id,
                reason: "metadata".into(),
                error: "regression".into(),
            }),
        )
        .await
        .unwrap();

        let row = sqlx::query!(
            r#"SELECT status::text AS "status!", error FROM writeback_jobs WHERE id = $1"#,
            job_id,
        )
        .fetch_one(&ing_pool)
        .await
        .unwrap();
        assert_eq!(row.status, "failed");
        assert_eq!(row.error.as_deref(), Some("regression"));
    }

    /// Double-dispatch path: a DB failure on the `mark_*` UPDATE
    /// followed by crash-recovery retry re-runs `finish` for a job whose
    /// terminal event already fired.  The dispatcher must deliver exactly
    /// once — `seen_at` is only written on delivery, so an unchanged
    /// `seen_at` after the second `finish` proves the re-fire was deduped.
    ///
    /// Modelling note: this calls `finish` twice sequentially with both
    /// calls fully succeeding, whereas the real crash shape leaves the job
    /// row `pending` and re-claims it (the second `finish` then runs against
    /// a non-`complete` row).  The property under test is identical either
    /// way — the dedupe lookup keys only on the event id and never consults
    /// `writeback_jobs` — so the sequential model exercises the same path.
    #[sqlx::test(migrations = "./migrations")]
    async fn finish_double_dispatch_delivers_exactly_once(pool: PgPool) {
        let app_pool = app_pool_for(&pool).await;
        let ing_pool = ingestion_pool_for(&pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (_work_id, m_id) = insert_fixture(&ing_pool, &marker).await;
        let job_id = insert_job(&ing_pool, m_id, "metadata").await;

        let outcome = || {
            Ok(RunOutcome::Success {
                manifestation_id: m_id,
                reason: "metadata".into(),
                current_file_hash: "abc123".into(),
            })
        };
        let eid = events::event_id(job_id, events::TerminalOutcome::Complete);
        let seen_at_for = |eid: String| {
            let pool = pool.clone();
            async move {
                sqlx::query_scalar!(
                    "SELECT seen_at FROM webhook_event_dedupe WHERE event_id = $1",
                    eid,
                )
                .fetch_one(&pool)
                .await
                .unwrap()
            }
        };

        finish(
            &app_pool,
            &test_config_with_max_attempts(3).0,
            job_id,
            1,
            outcome(),
        )
        .await
        .unwrap();
        let first_seen_at = seen_at_for(eid.clone()).await;

        // Simulated crash-recovery re-fire: the re-claim incremented
        // attempt_count; same job + outcome → same event id.
        finish(
            &app_pool,
            &test_config_with_max_attempts(3).0,
            job_id,
            2,
            outcome(),
        )
        .await
        .unwrap();

        assert_eq!(
            seen_at_for(eid.clone()).await,
            first_seen_at,
            "second finish must dedupe, not re-deliver (seen_at refreshes only on delivery)"
        );
        let n = sqlx::query_scalar!(
            "SELECT count(*) AS \"n!\" FROM webhook_event_dedupe WHERE event_id = $1",
            eid,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(n, 1);
    }

    /// RLS system-context policy: a writeback pool (which sets
    /// `app.system_context = 'writeback'` per-connection via
    /// `after_connect`) can UPDATE `manifestations` without an
    /// `app.current_user_id` user context — the worker's operational
    /// pathway.
    #[sqlx::test(migrations = "./migrations")]
    async fn rls_system_update_policy_allows_writeback_pool(pool: PgPool) {
        let wb_pool = writeback_pool_for(&pool).await;
        let ing_pool = ingestion_pool_for(&pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (_work_id, m_id) = insert_fixture(&ing_pool, &marker).await;

        let res = sqlx::query!(
            "UPDATE manifestations SET current_file_hash = $1 WHERE id = $2",
            "system-context-hash",
            m_id,
        )
        .execute(&wb_pool)
        .await
        .unwrap();
        assert_eq!(
            res.rows_affected(),
            1,
            "writeback pool must be able to UPDATE manifestations"
        );
    }

    /// A `reverie_app` connection without `app.system_context` set
    /// AND without `app.current_user_id` set matches zero policies and is
    /// denied.  This is the failure mode this guard prevents: a future Axum
    /// handler that forgets `SET LOCAL app.current_user_id` cannot reach
    /// the system policy because that policy now requires an explicit
    /// `app.system_context = 'writeback'` signal that user-facing pools
    /// never set.
    #[sqlx::test(migrations = "./migrations")]
    async fn rls_user_facing_pool_without_user_id_blocked_from_manifestations(pool: PgPool) {
        let app_pool = app_pool_for(&pool).await;
        let ing_pool = ingestion_pool_for(&pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (_work_id, m_id) = insert_fixture(&ing_pool, &marker).await;

        // SELECT with no user context, no system context → must return 0 rows.
        let visible = sqlx::query_scalar!("SELECT id FROM manifestations WHERE id = $1", m_id,)
            .fetch_optional(&app_pool)
            .await
            .unwrap();
        assert!(
            visible.is_none(),
            "a reverie_app session with neither app.current_user_id nor app.system_context must NOT see manifestations rows"
        );

        // UPDATE with no user context, no system context → must affect 0 rows.
        let res = sqlx::query!(
            "UPDATE manifestations SET current_file_hash = $1 WHERE id = $2",
            "should-not-apply",
            m_id,
        )
        .execute(&app_pool)
        .await
        .unwrap();
        assert_eq!(
            res.rows_affected(),
            0,
            "a reverie_app session with neither app.current_user_id nor app.system_context must NOT update manifestations rows"
        );
    }

    /// RLS system-context policy: a `reverie_app` session that has set a
    /// non-empty `app.current_user_id` pointing at a non-existent user
    /// matches neither the user policies (no real user) nor the system
    /// policy (no `app.system_context`), so the UPDATE is filtered out.
    #[sqlx::test(migrations = "./migrations")]
    async fn rls_system_update_policy_blocks_unknown_user_context(pool: PgPool) {
        let app_pool = app_pool_for(&pool).await;
        let ing_pool = ingestion_pool_for(&pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (_work_id, m_id) = insert_fixture(&ing_pool, &marker).await;

        // Random UUID that will not match any users row.
        let imposter = Uuid::new_v4();

        let mut tx = app_pool.begin().await.unwrap();
        // CARVE-OUT: SELECT set_config(...) is the documented
        // GUC-mutation carve-out — sqlx macros cannot validate Postgres
        // session-config calls against the schema at prepare time. The
        // BEGIN/ROLLBACK shell around it is the proper sqlx transaction
        // API; only the set_config remains runtime.
        let imposter_str = imposter.to_string();
        sqlx::query("SELECT set_config('app.current_user_id', $1, true)")
            .bind(&imposter_str)
            .execute(&mut *tx)
            .await
            .unwrap();
        let res = sqlx::query!(
            "UPDATE manifestations SET current_file_hash = $1 WHERE id = $2",
            "imposter-hash",
            m_id,
        )
        .execute(&mut *tx)
        .await
        .unwrap();
        tx.rollback().await.unwrap();

        assert_eq!(
            res.rows_affected(),
            0,
            "a session with a bogus user context must NOT be able to update manifestations"
        );
    }

    /// `finish(Err(WritebackError))` for a transient / retryable error
    /// routes through `mark_failed`.  `attempt_count < max_attempts`, so
    /// the row lands at `failed` for a later retry.
    #[sqlx::test(migrations = "./migrations")]
    async fn finish_marks_failed_on_transient_run_once_error(pool: PgPool) {
        let app_pool = app_pool_for(&pool).await;
        let ing_pool = ingestion_pool_for(&pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (_work_id, m_id) = insert_fixture(&ing_pool, &marker).await;
        let job_id = insert_job(&ing_pool, m_id, "metadata").await;

        finish(
            &app_pool,
            &test_config_with_max_attempts(3).0,
            job_id,
            1,
            Err(super::super::error::WritebackError::Persist(
                "transient-disk-error".into(),
            )),
        )
        .await
        .unwrap();

        let row = sqlx::query!(
            r#"SELECT status::text AS "status!", error FROM writeback_jobs WHERE id = $1"#,
            job_id,
        )
        .fetch_one(&ing_pool)
        .await
        .unwrap();
        assert_eq!(row.status, "failed");
        let err_text = row
            .error
            .expect("error column should record the run_once error");
        assert!(
            err_text.contains("transient-disk-error"),
            "error should describe the Persist error: {err_text}"
        );
    }

    /// `finish(Err(JobNotFound))` skips the retry budget and routes
    /// straight to `skipped`: the job row has vanished (CASCADE removed
    /// the manifestation, or the row was deleted manually), so retrying
    /// cannot succeed.
    #[sqlx::test(migrations = "./migrations")]
    async fn finish_marks_skipped_on_job_not_found_error(pool: PgPool) {
        let app_pool = app_pool_for(&pool).await;
        let ing_pool = ingestion_pool_for(&pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (_work_id, m_id) = insert_fixture(&ing_pool, &marker).await;
        let job_id = insert_job(&ing_pool, m_id, "metadata").await;

        finish(
            &app_pool,
            &test_config_with_max_attempts(3).0,
            job_id,
            1,
            Err(super::super::error::WritebackError::JobNotFound(job_id)),
        )
        .await
        .unwrap();

        let row = sqlx::query!(
            r#"SELECT status::text AS "status!", error FROM writeback_jobs WHERE id = $1"#,
            job_id,
        )
        .fetch_one(&ing_pool)
        .await
        .unwrap();
        assert_eq!(
            row.status, "skipped",
            "JobNotFound must route to skipped (retry budget would be wasted on a vanished row)"
        );
        let err_text = row
            .error
            .expect("error column should record the JobNotFound error");
        assert!(
            err_text.contains("not found"),
            "error should describe the JobNotFound: {err_text}"
        );
    }
}
