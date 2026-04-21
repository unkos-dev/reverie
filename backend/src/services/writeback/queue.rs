//! Background writeback queue worker.
//!
//! Mirrors `services::enrichment::queue` with one change: at most one job
//! per manifestation is ever `in_progress` at the same time, so two
//! workers can never race writebacks on the same on-disk EPUB.
//!
//! The guarantee is enforced in two layers:
//!   1. A partial UNIQUE index (`idx_writeback_jobs_in_progress_unique`)
//!      on `(manifestation_id) WHERE status = 'in_progress'` — the
//!      load-bearing correctness gate.  Two workers racing the same
//!      manifestation both survive `NOT EXISTS` under READ COMMITTED
//!      (which can't see a peer's uncommitted UPDATE), but when the
//!      second worker's UPDATE would create a duplicate in_progress
//!      tuple, Postgres waits on the first worker's uncommitted index
//!      entry, then fails with SQLSTATE 23505.  `claim_next` translates
//!      that into `Ok(None)`.
//!   2. A `NOT EXISTS` clause inside the claim CTE — a cheap soft filter
//!      that avoids the unique-violation round-trip on the common path
//!      where a sibling job already holds the `in_progress` slot.

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
/// claimed.  The partial UNIQUE index on
/// `(manifestation_id) WHERE status = 'in_progress'` is the load-bearing
/// serialisation primitive; the `NOT EXISTS` clause in the CTE is a
/// common-path optimisation that avoids a unique-violation round-trip.
pub(crate) async fn claim_next(pool: &PgPool) -> sqlx::Result<Option<(Uuid, i32)>> {
    let result = sqlx::query_as::<_, (Uuid, i32)>(
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
    .await;

    match result {
        Ok(row) => Ok(row),
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
    // same terminal event twice; Step 12's real dispatcher must dedupe on
    // event ID.  Tracked in UNK-98.
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
            let mid = match sqlx::query_scalar::<_, Uuid>(
                "SELECT manifestation_id FROM writeback_jobs WHERE id = $1",
            )
            .bind(id)
            .fetch_optional(pool)
            .await
            {
                Ok(Some(mid)) => mid,
                Ok(None) => {
                    warn!(
                        %id,
                        "writeback: job row vanished before failure webhook could be emitted; using sentinel manifestation_id"
                    );
                    Uuid::nil()
                }
                Err(lookup_err) => {
                    warn!(
                        error = %lookup_err,
                        %id,
                        "writeback: manifestation_id lookup failed; using sentinel manifestation_id"
                    );
                    Uuid::nil()
                }
            };
            events::emit_writeback_failed(mid, "unknown", attempt_count, &err_str);

            if is_job_not_found {
                mark_skipped(pool, id, &err_str).await?;
            } else {
                mark_failed(pool, id, attempt_count, config, Some(&err_str)).await?;
            }
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
    use tokio::sync::Barrier;

    use crate::test_support::db::{app_pool_for, ingestion_pool_for, writeback_pool_for};

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

        sqlx::query("UPDATE writeback_jobs SET status = 'in_progress' WHERE id = $1")
            .bind(job_a)
            .execute(&app_pool)
            .await
            .unwrap();

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
        .fetch_one(&app_pool)
        .await
        .unwrap();
        assert_eq!(
            count_eligible_siblings, 0,
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
            let res = sqlx::query(
                "UPDATE writeback_jobs SET status = 'in_progress', \
                 last_attempted_at = now(), attempt_count = attempt_count + 1 \
                 WHERE id = $1",
            )
            .bind(job_a)
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
            let res = sqlx::query(
                "UPDATE writeback_jobs SET status = 'in_progress', \
                 last_attempted_at = now(), attempt_count = attempt_count + 1 \
                 WHERE id = $1",
            )
            .bind(job_b)
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
                    unique_violations += 1
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
        let in_progress_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM writeback_jobs \
             WHERE manifestation_id = $1 AND status = 'in_progress'",
        )
        .bind(m_id)
        .fetch_one(&ing_pool)
        .await
        .unwrap();
        assert_eq!(in_progress_count, 1);
    }

    /// Jobs on distinct manifestations can run in parallel — i.e. the
    /// manifestation-aware NOT EXISTS clause does NOT cross-block them.
    /// Verified by checking that neither row appears in the other's
    /// in_progress EXISTS check at the SQL level.  Parallel-test safe.
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
        sqlx::query("UPDATE writeback_jobs SET status = 'in_progress' WHERE manifestation_id = $1")
            .bind(m_a)
            .execute(&app_pool)
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
        .fetch_one(&app_pool)
        .await
        .unwrap();
        assert!(
            m_b_eligible,
            "m_b's job must remain eligible when m_a's is in_progress"
        );
    }

    /// Retry-backoff: attempt_count=2 → 30 minute window.  Verified via
    /// a SELECT mirroring the CTE's eligibility predicate, so parallel
    /// tests do not steal the claim.
    #[sqlx::test(migrations = "./migrations")]
    async fn retry_backoff_honoured(pool: PgPool) {
        let app_pool = app_pool_for(&pool).await;
        let ing_pool = ingestion_pool_for(&pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (_work_id, m_id) = insert_fixture(&ing_pool, &marker).await;

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
        .fetch_one(&app_pool)
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
        .execute(&app_pool)
        .await
        .unwrap();

        let eligible_outside: bool = sqlx::query_scalar(
            "SELECT (last_attempted_at < now() - INTERVAL '30 minutes') \
             FROM writeback_jobs WHERE id = $1",
        )
        .bind(job_id)
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
        let job_id: Uuid = sqlx::query_scalar(
            "INSERT INTO writeback_jobs (manifestation_id, reason, status) \
             VALUES ($1, 'metadata', 'in_progress') RETURNING id",
        )
        .bind(m_id)
        .fetch_one(&ing_pool)
        .await
        .unwrap();

        revert_in_progress(&app_pool).await.unwrap();

        let status: String =
            sqlx::query_scalar("SELECT status::text FROM writeback_jobs WHERE id = $1")
                .bind(job_id)
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

        let config = test_config_with_max_attempts(3);
        mark_failed(&app_pool, job_id, 3, &config, Some("final"))
            .await
            .unwrap();

        let status: String =
            sqlx::query_scalar("SELECT status::text FROM writeback_jobs WHERE id = $1")
                .bind(job_id)
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
        let job_id: Uuid = sqlx::query_scalar(
            "INSERT INTO writeback_jobs (manifestation_id, reason, status) \
             VALUES ($1, 'metadata', 'in_progress') RETURNING id",
        )
        .bind(m_id)
        .fetch_one(&ing_pool)
        .await
        .unwrap();

        let pool_for_spawn = app_pool.clone();
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
    }

    /// `finish(Ok(Skipped))` transitions to `skipped` and records the
    /// skip reason in `error`.  Skipped bypasses retry regardless of
    /// attempt_count.
    #[sqlx::test(migrations = "./migrations")]
    async fn finish_marks_skipped_on_skipped_outcome(pool: PgPool) {
        let app_pool = app_pool_for(&pool).await;
        let ing_pool = ingestion_pool_for(&pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (_work_id, m_id) = insert_fixture(&ing_pool, &marker).await;
        let job_id = insert_job(&ing_pool, m_id, "metadata").await;

        finish(
            &app_pool,
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

        let res = sqlx::query("UPDATE manifestations SET current_file_hash = $1 WHERE id = $2")
            .bind("system-context-hash")
            .bind(m_id)
            .execute(&wb_pool)
            .await
            .unwrap();
        assert_eq!(
            res.rows_affected(),
            1,
            "writeback pool must be able to UPDATE manifestations"
        );
    }

    /// UNK-99: a `reverie_app` connection without `app.system_context` set
    /// AND without `app.current_user_id` set matches zero policies and is
    /// denied.  This is the failure mode UNK-99 prevents: a future Axum
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
        let visible: Option<Uuid> =
            sqlx::query_scalar("SELECT id FROM manifestations WHERE id = $1")
                .bind(m_id)
                .fetch_optional(&app_pool)
                .await
                .unwrap();
        assert!(
            visible.is_none(),
            "a reverie_app session with neither app.current_user_id nor app.system_context must NOT see manifestations rows"
        );

        // UPDATE with no user context, no system context → must affect 0 rows.
        let res = sqlx::query("UPDATE manifestations SET current_file_hash = $1 WHERE id = $2")
            .bind("should-not-apply")
            .bind(m_id)
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

        let mut conn = app_pool.acquire().await.unwrap();
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
            &test_config_with_max_attempts(3),
            job_id,
            1,
            Err(super::super::error::WritebackError::Persist(
                "transient-disk-error".into(),
            )),
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
        assert_eq!(
            status, "skipped",
            "JobNotFound must route to skipped (retry budget would be wasted on a vanished row)"
        );
        let err_text = error.expect("error column should record the JobNotFound error");
        assert!(
            err_text.contains("not found"),
            "error should describe the JobNotFound: {err_text}"
        );
    }
}
