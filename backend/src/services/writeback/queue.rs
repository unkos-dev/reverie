//! Background writeback queue worker.
//!
//! Mirrors `services::enrichment::queue` with one change: the claim CTE
//! adds a manifestation-aware `NOT EXISTS` clause so two jobs for the same
//! manifestation never run in parallel (on-disk file state must serialise).

use std::sync::Arc;
use std::time::Duration;

use sqlx::PgPool;
use tokio::sync::Semaphore;
use tokio::time::Interval;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};
use uuid::Uuid;

use crate::config::Config;

use super::events;
use super::orchestrator::{self, RunOutcome};

/// Spawn the writeback queue worker loop.  Returns when `cancel` fires,
/// reverting any `in_progress` row back to `pending`.
pub async fn spawn_worker(
    pool: PgPool,
    config: Config,
    cancel: CancellationToken,
) -> anyhow::Result<()> {
    if !config.writeback.enabled {
        info!("writeback queue disabled by config");
        cancel.cancelled().await;
        return Ok(());
    }

    // Crash-recovery: any row left in_progress from a previous process
    // transitions back to pending before we start polling.  Shared with
    // the shutdown path below.
    revert_in_progress(&pool).await?;

    let concurrency = config.writeback.concurrency as usize;
    let semaphore = Arc::new(Semaphore::new(concurrency));
    let mut interval: Interval =
        tokio::time::interval(Duration::from_secs(config.writeback.poll_idle_secs));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    info!(
        concurrency,
        poll_idle_secs = config.writeback.poll_idle_secs,
        "writeback queue started"
    );

    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                info!("writeback queue shutting down");
                revert_in_progress(&pool).await?;
                return Ok(());
            }
            _ = interval.tick() => {
                loop {
                    let permit = match semaphore.clone().try_acquire_owned() {
                        Ok(p) => p,
                        Err(_) => break,
                    };
                    let claim = claim_next(&pool).await?;
                    let Some((id, attempt_count)) = claim else {
                        drop(permit);
                        break;
                    };
                    let pool = pool.clone();
                    let cfg = config.clone();
                    tokio::spawn(async move {
                        let _p = permit;
                        let result = orchestrator::run_once(&pool, &cfg, id).await;
                        if let Err(e) = finish(&pool, &cfg, id, attempt_count, result).await {
                            warn!(error = %e, %id, "writeback: finish bookkeeping failed");
                        }
                    });
                }
            }
        }
    }
}

/// Atomic claim.  Returns `Some((id, new_attempt_count))` when a row was
/// claimed.  Manifestation-aware `NOT EXISTS` clause ensures only one job
/// per manifestation is `in_progress` at any time.
pub(crate) async fn claim_next(pool: &PgPool) -> sqlx::Result<Option<(Uuid, i32)>> {
    let row: Option<(Uuid, i32)> = sqlx::query_as(
        r#"WITH eligible AS (
             SELECT wj.id, wj.attempt_count
             FROM writeback_jobs wj
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
             FOR UPDATE SKIP LOCKED
           )
           UPDATE writeback_jobs wj
              SET status = 'in_progress',
                  last_attempted_at = now(),
                  attempt_count = wj.attempt_count + 1
             FROM eligible
            WHERE wj.id = eligible.id
           RETURNING wj.id, wj.attempt_count"#,
    )
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

async fn finish(
    pool: &PgPool,
    config: &Config,
    id: Uuid,
    attempt_count: i32,
    result: Result<RunOutcome, super::error::WritebackError>,
) -> sqlx::Result<()> {
    // Emit the webhook BEFORE the DB bookkeeping write.  If the DB write
    // fails, the event still fires (a transient DB hiccup on the final
    // update otherwise silently dropped the webhook forever).  The cost
    // is that a DB failure followed by crash-recovery retry can emit the
    // same terminal event twice; Step 12's real dispatcher must dedupe.
    match result {
        Ok(RunOutcome::Success {
            manifestation_id,
            reason,
            current_file_hash,
        }) => {
            events::emit_writeback_complete(
                manifestation_id,
                &reason,
                attempt_count,
                &current_file_hash,
            );
            mark_complete(pool, id).await?;
        }
        Ok(RunOutcome::Skipped {
            manifestation_id,
            reason,
            skip_reason,
        }) => {
            events::emit_writeback_failed(manifestation_id, &reason, attempt_count, &skip_reason);
            // Terminal skip (e.g. unsupported format): bypass retry path.
            mark_skipped(pool, id, &skip_reason).await?;
        }
        Ok(RunOutcome::Failed {
            manifestation_id,
            reason,
            error,
        }) => {
            events::emit_writeback_failed(manifestation_id, &reason, attempt_count, &error);
            mark_failed(pool, id, attempt_count, config, Some(&error)).await?;
        }
        Err(e) => {
            warn!(error = %e, %id, "writeback run_once failed");
            let err_str = e.to_string();
            // run_once failed before producing a RunOutcome (e.g. snapshot
            // load).  Resolve manifestation_id from the job row so the
            // webhook carries the right target.  Each outcome is logged
            // explicitly so an observability gap here can't silently
            // swallow the failure event.
            match sqlx::query_scalar::<_, Uuid>(
                "SELECT manifestation_id FROM writeback_jobs WHERE id = $1",
            )
            .bind(id)
            .fetch_optional(pool)
            .await
            {
                Ok(Some(mid)) => {
                    events::emit_writeback_failed(mid, "unknown", attempt_count, &err_str);
                }
                Ok(None) => {
                    warn!(
                        %id,
                        "writeback: job row vanished before failure webhook could be emitted"
                    );
                }
                Err(lookup_err) => {
                    warn!(
                        error = %lookup_err,
                        %id,
                        "writeback: manifestation_id lookup failed; failure webhook not emitted"
                    );
                }
            }
            mark_failed(pool, id, attempt_count, config, Some(&err_str)).await?;
        }
    }
    Ok(())
}

async fn mark_skipped(pool: &PgPool, id: Uuid, reason: &str) -> sqlx::Result<()> {
    sqlx::query(
        "UPDATE writeback_jobs \
         SET status = 'skipped', completed_at = now(), error = $1 \
         WHERE id = $2",
    )
    .bind(reason)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

async fn mark_complete(pool: &PgPool, id: Uuid) -> sqlx::Result<()> {
    sqlx::query(
        "UPDATE writeback_jobs SET status = 'complete', completed_at = now(), error = NULL \
         WHERE id = $1",
    )
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Transition to `failed`, or to `skipped` once `attempt_count >=
/// max_attempts`.  `skipped` is the terminal exhaustion label; mirrors
/// Step 7's semantic.
async fn mark_failed(
    pool: &PgPool,
    id: Uuid,
    attempt_count: i32,
    config: &Config,
    error: Option<&str>,
) -> sqlx::Result<()> {
    let max = config.writeback.max_attempts as i32;
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
    sqlx::query(
        "UPDATE writeback_jobs \
         SET status = $1::writeback_status, error = $2 \
         WHERE id = $3",
    )
    .bind(next_status)
    .bind(error)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Revert any `in_progress` rows back to `pending`.  Called on shutdown
/// AND on worker startup (crash recovery).
pub(crate) async fn revert_in_progress(pool: &PgPool) -> sqlx::Result<()> {
    let res = sqlx::query(
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
mod tests {
    use super::*;
    use crate::config::{CleanupMode, CoverConfig, EnrichmentConfig, WritebackConfig};

    fn db_url() -> String {
        std::env::var("DATABASE_URL_INGESTION").unwrap_or_else(|_| {
            "postgres://reverie_ingestion:reverie_ingestion@localhost:5433/reverie_dev".into()
        })
    }

    fn app_url() -> String {
        std::env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgres://reverie_app:reverie_app@localhost:5433/reverie_dev".into()
        })
    }

    fn test_config_with_max_attempts(max_attempts: u32) -> Config {
        Config {
            port: 3000,
            database_url: String::new(),
            library_path: String::new(),
            ingestion_path: String::new(),
            quarantine_path: String::new(),
            log_level: "info".into(),
            db_max_connections: 5,
            oidc_issuer_url: String::new(),
            oidc_client_id: String::new(),
            oidc_client_secret: String::new(),
            oidc_redirect_uri: String::new(),
            ingestion_database_url: String::new(),
            format_priority: vec!["epub".into()],
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
            openlibrary_base_url: "https://example.invalid".into(),
            googlebooks_base_url: "https://example.invalid".into(),
            googlebooks_api_key: None,
            hardcover_base_url: "https://example.invalid".into(),
            hardcover_api_token: None,
            operator_contact: None,
        }
    }

    /// Defuse any stale rows from prior runs so claim_next only sees rows
    /// this test inserted.
    async fn quiesce_writeback_jobs(pool: &PgPool) {
        let _ = sqlx::query(
            "UPDATE writeback_jobs SET status = 'complete' \
             WHERE status IN ('pending', 'failed', 'in_progress')",
        )
        .execute(pool)
        .await;
    }

    /// Insert a minimal work + manifestation fixture and return ids.
    async fn insert_fixture(pool: &PgPool, marker: &str) -> (Uuid, Uuid) {
        let work_id: Uuid = sqlx::query_scalar(
            "INSERT INTO works (title, sort_title) VALUES ($1, $1) RETURNING id",
        )
        .bind(format!("WritebackFixture-{marker}"))
        .fetch_one(pool)
        .await
        .unwrap();
        let m_id: Uuid = sqlx::query_scalar(
            // Set enrichment_status = 'complete' so these fixtures don't
            // leak into the enrichment queue's claim_next under parallel
            // test execution (the column defaults to 'pending').
            "INSERT INTO manifestations \
               (work_id, format, file_path, ingestion_file_hash, current_file_hash, \
                file_size_bytes, ingestion_status, validation_status, enrichment_status) \
             VALUES ($1, 'epub'::manifestation_format, $2, $3, $3, 1000, \
                     'complete'::ingestion_status, 'valid'::validation_status, \
                     'complete'::enrichment_status) \
             RETURNING id",
        )
        .bind(work_id)
        .bind(format!("/tmp/wb-{marker}.epub"))
        .bind(format!("wb-hash-{marker}"))
        .fetch_one(pool)
        .await
        .unwrap();
        (work_id, m_id)
    }

    async fn insert_job(pool: &PgPool, manifestation_id: Uuid, reason: &str) -> Uuid {
        sqlx::query_scalar(
            "INSERT INTO writeback_jobs (manifestation_id, reason) VALUES ($1, $2) RETURNING id",
        )
        .bind(manifestation_id)
        .bind(reason)
        .fetch_one(pool)
        .await
        .unwrap()
    }

    /// Cleanup uses `app_pool` for writeback_jobs (reverie_ingestion has no
    /// DELETE grant) and `ing_pool` for manifestations/works (reverie_app
    /// has RLS + no FK cascade from here).
    async fn cleanup(app_pool: &PgPool, ing_pool: &PgPool, work_id: Uuid, manifestation_id: Uuid) {
        let _ = sqlx::query("DELETE FROM writeback_jobs WHERE manifestation_id = $1")
            .bind(manifestation_id)
            .execute(app_pool)
            .await;
        let _ = sqlx::query("DELETE FROM manifestations WHERE id = $1")
            .bind(manifestation_id)
            .execute(ing_pool)
            .await;
        let _ = sqlx::query("DELETE FROM works WHERE id = $1")
            .bind(work_id)
            .execute(ing_pool)
            .await;
    }

    /// Two jobs on the same manifestation MUST serialise — only one can be
    /// in_progress at any time.  Asserts the manifestation-aware NOT EXISTS
    /// clause excludes sibling-pending rows when one is in_progress.
    /// Parallel-test safe (no reliance on global claim_next order).
    #[tokio::test]
    #[ignore]
    async fn two_workers_same_manifestation_serialise() {
        let pool = PgPool::connect(&app_url()).await.unwrap();
        let marker = Uuid::new_v4().simple().to_string();

        let ing_pool = PgPool::connect(&db_url()).await.unwrap();
        let (work_id, m_id) = insert_fixture(&ing_pool, &marker).await;
        let job_a = insert_job(&ing_pool, m_id, "metadata").await;
        let _job_b = insert_job(&ing_pool, m_id, "metadata").await;
        let _job_c = insert_job(&ing_pool, m_id, "metadata").await;

        // Simulate one claim — flip job_a to in_progress.
        sqlx::query("UPDATE writeback_jobs SET status = 'in_progress' WHERE id = $1")
            .bind(job_a)
            .execute(&pool)
            .await
            .unwrap();

        // The remaining two sibling pending jobs must now fail the
        // NOT EXISTS (sibling in_progress for same manifestation) clause,
        // i.e. be ineligible.
        let count_eligible_siblings: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM writeback_jobs wj \
             WHERE wj.manifestation_id = $1 \
               AND wj.status = 'pending' \
               AND NOT EXISTS ( \
                 SELECT 1 FROM writeback_jobs other \
                 WHERE other.manifestation_id = wj.manifestation_id \
                   AND other.status = 'in_progress' \
               )",
        )
        .bind(m_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            count_eligible_siblings, 0,
            "sibling pending jobs must not be eligible while one is in_progress"
        );

        cleanup(&pool, &ing_pool, work_id, m_id).await;
    }

    /// Jobs on distinct manifestations can run in parallel — i.e. the
    /// manifestation-aware NOT EXISTS clause does NOT cross-block them.
    /// Verified by checking that neither row appears in the other's
    /// in_progress EXISTS check at the SQL level.  Parallel-test safe.
    #[tokio::test]
    #[ignore]
    async fn two_workers_distinct_manifestations_parallelise() {
        let pool = PgPool::connect(&app_url()).await.unwrap();
        let ing_pool = PgPool::connect(&db_url()).await.unwrap();
        let marker_a = Uuid::new_v4().simple().to_string();
        let marker_b = Uuid::new_v4().simple().to_string();
        let (work_a, m_a) = insert_fixture(&ing_pool, &marker_a).await;
        let (work_b, m_b) = insert_fixture(&ing_pool, &marker_b).await;
        let _job_a = insert_job(&ing_pool, m_a, "metadata").await;
        let _job_b = insert_job(&ing_pool, m_b, "metadata").await;

        // Mark m_a's job in_progress directly — simulating an active worker.
        sqlx::query("UPDATE writeback_jobs SET status = 'in_progress' WHERE manifestation_id = $1")
            .bind(m_a)
            .execute(&pool)
            .await
            .unwrap();

        // m_b's job must still be eligible — NOT EXISTS clause compares on
        // m_b's manifestation_id, which is distinct from m_a's.
        let m_b_eligible: bool = sqlx::query_scalar(
            "SELECT NOT EXISTS (
               SELECT 1 FROM writeback_jobs
                WHERE manifestation_id = $1
                  AND status = 'in_progress'
             )",
        )
        .bind(m_b)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(
            m_b_eligible,
            "m_b's job must remain eligible when m_a's is in_progress"
        );

        cleanup(&pool, &ing_pool, work_a, m_a).await;
        cleanup(&pool, &ing_pool, work_b, m_b).await;
    }

    /// Retry-backoff: attempt_count=2 → 30 minute window.  Verified via
    /// a SELECT mirroring the CTE's eligibility predicate, so parallel
    /// tests do not steal the claim.
    #[tokio::test]
    #[ignore]
    async fn retry_backoff_honoured() {
        let pool = PgPool::connect(&app_url()).await.unwrap();
        let ing_pool = PgPool::connect(&db_url()).await.unwrap();
        let marker = Uuid::new_v4().simple().to_string();
        let (work_id, m_id) = insert_fixture(&ing_pool, &marker).await;

        let job_id: Uuid = sqlx::query_scalar(
            "INSERT INTO writeback_jobs \
               (manifestation_id, reason, status, attempt_count, last_attempted_at) \
             VALUES ($1, 'metadata', 'failed', 2, now() - INTERVAL '25 minutes') \
             RETURNING id",
        )
        .bind(m_id)
        .fetch_one(&ing_pool)
        .await
        .unwrap();

        // Eligibility check mirrors claim_next's WHERE clause for
        // attempt_count=2 (30-minute window).  Asserting eligibility at
        // the SQL level instead of via claim_next keeps the test safe
        // against parallel runs that might claim our row before we check.
        let eligible_inside: bool = sqlx::query_scalar(
            "SELECT (last_attempted_at < now() - INTERVAL '30 minutes') \
             FROM writeback_jobs WHERE id = $1",
        )
        .bind(job_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(
            !eligible_inside,
            "row inside backoff window must not satisfy eligibility"
        );

        // Move back past the window (UPDATE grant is on app_pool only).
        sqlx::query(
            "UPDATE writeback_jobs \
             SET last_attempted_at = now() - INTERVAL '35 minutes' WHERE id = $1",
        )
        .bind(job_id)
        .execute(&pool)
        .await
        .unwrap();

        let eligible_outside: bool = sqlx::query_scalar(
            "SELECT (last_attempted_at < now() - INTERVAL '30 minutes') \
             FROM writeback_jobs WHERE id = $1",
        )
        .bind(job_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(
            eligible_outside,
            "row past backoff window must satisfy eligibility"
        );

        cleanup(&pool, &ing_pool, work_id, m_id).await;
    }

    /// `revert_in_progress` flips every `in_progress` row back to `pending`.
    #[tokio::test]
    #[ignore]
    async fn shutdown_reverts_in_progress() {
        let pool = PgPool::connect(&app_url()).await.unwrap();
        let ing_pool = PgPool::connect(&db_url()).await.unwrap();
        quiesce_writeback_jobs(&pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (work_id, m_id) = insert_fixture(&ing_pool, &marker).await;
        let job_id: Uuid = sqlx::query_scalar(
            "INSERT INTO writeback_jobs (manifestation_id, reason, status) \
             VALUES ($1, 'metadata', 'in_progress') RETURNING id",
        )
        .bind(m_id)
        .fetch_one(&ing_pool)
        .await
        .unwrap();

        revert_in_progress(&pool).await.unwrap();

        let status: String =
            sqlx::query_scalar("SELECT status::text FROM writeback_jobs WHERE id = $1")
                .bind(job_id)
                .fetch_one(&ing_pool)
                .await
                .unwrap();
        assert_eq!(status, "pending");

        cleanup(&pool, &ing_pool, work_id, m_id).await;
    }

    /// At `max_attempts`, `mark_failed` transitions to `skipped`.
    #[tokio::test]
    #[ignore]
    async fn max_attempts_transitions_to_skipped() {
        let pool = PgPool::connect(&app_url()).await.unwrap();
        let ing_pool = PgPool::connect(&db_url()).await.unwrap();
        quiesce_writeback_jobs(&pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (work_id, m_id) = insert_fixture(&ing_pool, &marker).await;
        let job_id = insert_job(&ing_pool, m_id, "metadata").await;

        let config = test_config_with_max_attempts(3);
        mark_failed(&pool, job_id, 3, &config, Some("final"))
            .await
            .unwrap();

        let status: String =
            sqlx::query_scalar("SELECT status::text FROM writeback_jobs WHERE id = $1")
                .bind(job_id)
                .fetch_one(&ing_pool)
                .await
                .unwrap();
        assert_eq!(status, "skipped");

        cleanup(&pool, &ing_pool, work_id, m_id).await;
    }

    /// Crash recovery: a row left `in_progress` must be picked up as
    /// `pending` on worker startup.  Mirrors `shutdown_reverts_in_progress`
    /// but uses the full `spawn_worker` entry point.
    #[tokio::test]
    #[ignore]
    async fn crash_recovery_reconciles_in_progress() {
        let ing_pool = PgPool::connect(&db_url()).await.unwrap();
        let app_for_quiesce = PgPool::connect(&app_url()).await.unwrap();
        quiesce_writeback_jobs(&app_for_quiesce).await;
        drop(app_for_quiesce);
        let marker = Uuid::new_v4().simple().to_string();
        let (work_id, m_id) = insert_fixture(&ing_pool, &marker).await;
        let job_id: Uuid = sqlx::query_scalar(
            "INSERT INTO writeback_jobs (manifestation_id, reason, status) \
             VALUES ($1, 'metadata', 'in_progress') RETURNING id",
        )
        .bind(m_id)
        .fetch_one(&ing_pool)
        .await
        .unwrap();

        let pool = PgPool::connect(&app_url()).await.unwrap();
        let pool_for_spawn = pool.clone();
        let cancel = CancellationToken::new();
        let cancel_for_spawn = cancel.clone();
        let cfg = test_config_with_max_attempts(3);
        let handle = tokio::spawn(async move {
            spawn_worker(pool_for_spawn, cfg, cancel_for_spawn)
                .await
                .unwrap();
        });

        // Allow the worker to run its startup revert_in_progress.
        tokio::time::sleep(Duration::from_millis(500)).await;
        cancel.cancel();
        handle.await.unwrap();

        let status: String =
            sqlx::query_scalar("SELECT status::text FROM writeback_jobs WHERE id = $1")
                .bind(job_id)
                .fetch_one(&ing_pool)
                .await
                .unwrap();
        assert_ne!(
            status, "in_progress",
            "crash-recovery should have moved the row out of in_progress"
        );

        cleanup(&pool, &ing_pool, work_id, m_id).await;
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
    #[tokio::test]
    #[ignore]
    async fn finish_marks_complete_on_success_outcome() {
        let pool = PgPool::connect(&app_url()).await.unwrap();
        let ing_pool = PgPool::connect(&db_url()).await.unwrap();
        let marker = Uuid::new_v4().simple().to_string();
        let (work_id, m_id) = insert_fixture(&ing_pool, &marker).await;
        let job_id = insert_job(&ing_pool, m_id, "metadata").await;

        finish(
            &pool,
            &test_config_with_max_attempts(3),
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

        let (status, error): (String, Option<String>) =
            sqlx::query_as("SELECT status::text, error FROM writeback_jobs WHERE id = $1")
                .bind(job_id)
                .fetch_one(&ing_pool)
                .await
                .unwrap();
        assert_eq!(status, "complete");
        assert_eq!(error, None);

        cleanup(&pool, &ing_pool, work_id, m_id).await;
    }

    /// `finish(Ok(Skipped))` transitions to `skipped` and records the
    /// skip reason in `error`.  Skipped bypasses retry regardless of
    /// attempt_count.
    #[tokio::test]
    #[ignore]
    async fn finish_marks_skipped_on_skipped_outcome() {
        let pool = PgPool::connect(&app_url()).await.unwrap();
        let ing_pool = PgPool::connect(&db_url()).await.unwrap();
        let marker = Uuid::new_v4().simple().to_string();
        let (work_id, m_id) = insert_fixture(&ing_pool, &marker).await;
        let job_id = insert_job(&ing_pool, m_id, "metadata").await;

        finish(
            &pool,
            &test_config_with_max_attempts(3),
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

        let (status, error): (String, Option<String>) =
            sqlx::query_as("SELECT status::text, error FROM writeback_jobs WHERE id = $1")
                .bind(job_id)
                .fetch_one(&ing_pool)
                .await
                .unwrap();
        assert_eq!(status, "skipped");
        assert_eq!(error.as_deref(), Some("format_unsupported: pdf"));

        cleanup(&pool, &ing_pool, work_id, m_id).await;
    }

    /// `finish(Ok(Failed))` with `attempt_count < max_attempts` leaves
    /// the row as `failed` for later retry, with the error recorded.
    #[tokio::test]
    #[ignore]
    async fn finish_marks_failed_below_max_attempts() {
        let pool = PgPool::connect(&app_url()).await.unwrap();
        let ing_pool = PgPool::connect(&db_url()).await.unwrap();
        let marker = Uuid::new_v4().simple().to_string();
        let (work_id, m_id) = insert_fixture(&ing_pool, &marker).await;
        let job_id = insert_job(&ing_pool, m_id, "metadata").await;

        finish(
            &pool,
            &test_config_with_max_attempts(3),
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

        let (status, error): (String, Option<String>) =
            sqlx::query_as("SELECT status::text, error FROM writeback_jobs WHERE id = $1")
                .bind(job_id)
                .fetch_one(&ing_pool)
                .await
                .unwrap();
        assert_eq!(status, "failed");
        assert_eq!(error.as_deref(), Some("regression"));

        cleanup(&pool, &ing_pool, work_id, m_id).await;
    }

    /// RLS system-context policy: with no user context set (NULL
    /// `app.current_user_id`), `reverie_app` can UPDATE `manifestations`
    /// — the worker's operational pathway.  Matches how the real worker
    /// connects: it never calls `SET LOCAL app.current_user_id`, so the
    /// setting stays at its session default of NULL.
    #[tokio::test]
    #[ignore]
    async fn rls_system_update_policy_allows_writeback_worker_without_user_context() {
        let pool = PgPool::connect(&app_url()).await.unwrap();
        let ing_pool = PgPool::connect(&db_url()).await.unwrap();
        let marker = Uuid::new_v4().simple().to_string();
        let (work_id, m_id) = insert_fixture(&ing_pool, &marker).await;

        // No SET LOCAL — default is NULL, which matches the real worker.
        let res = sqlx::query("UPDATE manifestations SET current_file_hash = $1 WHERE id = $2")
            .bind("system-context-hash")
            .bind(m_id)
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(
            res.rows_affected(),
            1,
            "system-context worker must be able to UPDATE manifestations"
        );
        cleanup(&pool, &ing_pool, work_id, m_id).await;
    }

    /// RLS system-context policy: with a non-empty `app.current_user_id`
    /// pointing at a non-existent user, neither the system policy nor the
    /// user-role policies match, so the UPDATE is filtered out.  This
    /// guards the worker's isolation: a misconfigured worker session
    /// that inherits a user context does NOT quietly succeed under the
    /// system policy.
    #[tokio::test]
    #[ignore]
    async fn rls_system_update_policy_blocks_unknown_user_context() {
        let pool = PgPool::connect(&app_url()).await.unwrap();
        let ing_pool = PgPool::connect(&db_url()).await.unwrap();
        let marker = Uuid::new_v4().simple().to_string();
        let (work_id, m_id) = insert_fixture(&ing_pool, &marker).await;

        // Random UUID that will not match any users row.
        let imposter = Uuid::new_v4();

        let mut conn = pool.acquire().await.unwrap();
        sqlx::query("BEGIN").execute(&mut *conn).await.unwrap();
        sqlx::query("SELECT set_config('app.current_user_id', $1, true)")
            .bind(imposter.to_string())
            .execute(&mut *conn)
            .await
            .unwrap();
        let res = sqlx::query("UPDATE manifestations SET current_file_hash = $1 WHERE id = $2")
            .bind("imposter-hash")
            .bind(m_id)
            .execute(&mut *conn)
            .await
            .unwrap();
        sqlx::query("ROLLBACK").execute(&mut *conn).await.unwrap();

        assert_eq!(
            res.rows_affected(),
            0,
            "a session with a bogus user context must NOT be able to update manifestations via the system policy"
        );
        drop(conn);
        cleanup(&pool, &ing_pool, work_id, m_id).await;
    }

    /// `finish(Err(WritebackError))` records `mark_failed` with the
    /// error string.  This is the S3 Err arm: `run_once` failed before
    /// producing a `RunOutcome`.
    #[tokio::test]
    #[ignore]
    async fn finish_marks_failed_on_run_once_error() {
        let pool = PgPool::connect(&app_url()).await.unwrap();
        let ing_pool = PgPool::connect(&db_url()).await.unwrap();
        let marker = Uuid::new_v4().simple().to_string();
        let (work_id, m_id) = insert_fixture(&ing_pool, &marker).await;
        let job_id = insert_job(&ing_pool, m_id, "metadata").await;

        finish(
            &pool,
            &test_config_with_max_attempts(3),
            job_id,
            1,
            Err(super::super::error::WritebackError::JobNotFound(job_id)),
        )
        .await
        .unwrap();

        let (status, error): (String, Option<String>) =
            sqlx::query_as("SELECT status::text, error FROM writeback_jobs WHERE id = $1")
                .bind(job_id)
                .fetch_one(&ing_pool)
                .await
                .unwrap();
        assert_eq!(status, "failed");
        let err_text = error.expect("error column should record the run_once error");
        assert!(
            err_text.contains("not found"),
            "error should describe the JobNotFound: {err_text}"
        );

        cleanup(&pool, &ing_pool, work_id, m_id).await;
    }
}
