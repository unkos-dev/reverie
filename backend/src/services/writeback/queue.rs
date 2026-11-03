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
    use crate::config::{CoverConfig, EnrichmentConfig, WritebackConfig};
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
            database_url: String::new().into(),
            library_path: storage_config.library_path,
            ingestion_path: storage_config.ingestion_path,
            log_level: "info".into(),
            db_max_connections: 5,
            oidc_issuer_url: String::new(),
            oidc_client_id: String::new(),
            oidc_client_secret: String::new().into(),
            oidc_redirect_uri: String::new(),
            local_auth_enabled: true,
            login_rate_per_min: 10,
            auth_hash_concurrency: 1,
            password_min_length: 8,
            password_max_length: 256,
            password_min_zxcvbn_score: 2,
            password_breach_check_enabled: true,
            self_registration_enabled: false,
            trusted_client_ip_header: None,
            migration_database_url: None,
            auto_migrate: false,
            ingestion_database_url: String::new().into(),
            accepted_formats: vec![ManifestationFormat::Epub],
            cleanup_imported: false,
            cleanup_duplicates: false,
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
            public_url: None,
            mail: crate::config::MailConfig::default(),
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
}
