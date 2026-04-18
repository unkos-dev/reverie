//! Background enrichment queue worker.
//!
//! Claims manifestations from the `manifestations` table using an atomic
//! `FOR UPDATE SKIP LOCKED` CTE so multiple workers can race without double
//! processing.  Applies an exponential-ish retry backoff and marks rows as
//! `skipped` after `max_attempts`.  On shutdown, reverts any `in_progress`
//! rows back to `pending` so a fresh worker can re-claim them.

use std::sync::Arc;
use std::time::Duration;

use sqlx::PgPool;
use tokio::sync::Semaphore;
use tokio::time::Interval;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};
use uuid::Uuid;

use crate::config::Config;

use super::orchestrator::{self, RunOutcome};

// Retry backoff is applied inside the `claim_next` CTE as a SQL CASE
// expression (5m, 30m, 2h, 8h, then 24h). That is the authoritative
// schedule; there is no Rust mirror to keep in sync.

/// Spawn the queue worker loop.  Returns when `cancel` fires, reverting any
/// `in_progress` row back to `pending`.
pub async fn spawn_queue(
    pool: PgPool,
    config: Config,
    cancel: CancellationToken,
) -> anyhow::Result<()> {
    if !config.enrichment.enabled {
        info!("enrichment queue disabled by config");
        cancel.cancelled().await;
        return Ok(());
    }

    let concurrency = config.enrichment.concurrency as usize;
    let semaphore = Arc::new(Semaphore::new(concurrency));
    let mut interval: Interval =
        tokio::time::interval(Duration::from_secs(config.enrichment.poll_idle_secs));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    info!(
        concurrency,
        poll_idle_secs = config.enrichment.poll_idle_secs,
        "enrichment queue started"
    );

    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                info!("enrichment queue shutting down");
                revert_in_progress(&pool).await?;
                return Ok(());
            }
            _ = interval.tick() => {
                // Drain as many pending rows as semaphore permits allow.
                loop {
                    let permit = match semaphore.clone().try_acquire_owned() {
                        Ok(p) => p,
                        Err(_) => break, // fully busy; next tick
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
                            warn!(error = %e, %id, "queue: finish bookkeeping failed");
                        }
                    });
                }
            }
        }
    }
}

/// Atomic claim: pick the oldest eligible row and flip it to `in_progress`.
///
/// Returns `Some((id, new_attempt_count))` when a row was claimed; `None`
/// when the queue is empty (or every row is still in its backoff window).
async fn claim_next(pool: &PgPool) -> sqlx::Result<Option<(Uuid, i32)>> {
    let row: Option<(Uuid, i32)> = sqlx::query_as(
        r#"WITH eligible AS (
             SELECT id, enrichment_attempt_count
             FROM manifestations
             WHERE enrichment_status IN ('pending', 'failed')
               AND (
                 enrichment_attempted_at IS NULL
                 OR enrichment_attempted_at <
                      now() - (
                        CASE
                          WHEN enrichment_attempt_count <= 0 THEN INTERVAL '0 minutes'
                          WHEN enrichment_attempt_count = 1 THEN INTERVAL '5 minutes'
                          WHEN enrichment_attempt_count = 2 THEN INTERVAL '30 minutes'
                          WHEN enrichment_attempt_count = 3 THEN INTERVAL '2 hours'
                          WHEN enrichment_attempt_count = 4 THEN INTERVAL '8 hours'
                          ELSE INTERVAL '24 hours'
                        END
                      )
               )
             ORDER BY enrichment_attempted_at NULLS FIRST, id
             LIMIT 1
             FOR UPDATE SKIP LOCKED
           )
           UPDATE manifestations m
              SET enrichment_status        = 'in_progress',
                  enrichment_attempted_at  = now(),
                  enrichment_attempt_count = m.enrichment_attempt_count + 1
             FROM eligible
            WHERE m.id = eligible.id
           RETURNING m.id, m.enrichment_attempt_count"#,
    )
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// Book-keeping after a `run_once` call.  Transitions rows to
/// `complete` / `failed` / `skipped` depending on the outcome.
async fn finish(
    pool: &PgPool,
    config: &Config,
    id: Uuid,
    attempt_count: i32,
    result: anyhow::Result<RunOutcome>,
) -> sqlx::Result<()> {
    match result {
        Ok(outcome) => {
            // If every source failed with non-terminal errors, treat the row
            // as `failed` so we retry; otherwise mark complete.
            let enabled_tried = !outcome.source_failures.is_empty();
            let any_terminal = outcome.source_failures.iter().any(|f| f.terminal);
            let applied_or_staged = outcome.applied + outcome.staged > 0;

            if enabled_tried && !applied_or_staged && !any_terminal {
                // Surface the longest retry_after among rate-limited failures.
                let retry_after = outcome
                    .source_failures
                    .iter()
                    .filter_map(|f| f.retry_after)
                    .max();
                mark_failed(
                    pool,
                    id,
                    attempt_count,
                    config,
                    retry_after,
                    Some("transient source failures"),
                )
                .await?;
            } else {
                mark_complete(pool, id).await?;
            }
        }
        Err(e) => {
            warn!(error = %e, %id, "enrichment run_once failed");
            mark_failed(pool, id, attempt_count, config, None, Some(&e.to_string())).await?;
        }
    }
    Ok(())
}

async fn mark_complete(pool: &PgPool, id: Uuid) -> sqlx::Result<()> {
    sqlx::query(
        "UPDATE manifestations \
         SET enrichment_status = 'complete', enrichment_error = NULL \
         WHERE id = $1",
    )
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

async fn mark_failed(
    pool: &PgPool,
    id: Uuid,
    attempt_count: i32,
    config: &Config,
    retry_after: Option<Duration>,
    error: Option<&str>,
) -> sqlx::Result<()> {
    let max = config.enrichment.max_attempts as i32;
    let next_status = if attempt_count >= max {
        "skipped"
    } else {
        "failed"
    };

    // When rate-limited, bump `enrichment_attempted_at` forward so the
    // backoff window respects Retry-After semantics.
    if let Some(ra) = retry_after {
        let secs = i64::try_from(ra.as_secs()).unwrap_or(i64::MAX);
        sqlx::query(
            "UPDATE manifestations \
             SET enrichment_status = $1::enrichment_status, \
                 enrichment_attempted_at = now() + ($2 || ' seconds')::interval, \
                 enrichment_error = $3 \
             WHERE id = $4",
        )
        .bind(next_status)
        .bind(secs.to_string())
        .bind(error)
        .bind(id)
        .execute(pool)
        .await?;
    } else {
        sqlx::query(
            "UPDATE manifestations \
             SET enrichment_status = $1::enrichment_status, \
                 enrichment_error = $2 \
             WHERE id = $3",
        )
        .bind(next_status)
        .bind(error)
        .bind(id)
        .execute(pool)
        .await?;
    }

    // Backoff is applied inside the `claim_next` CTE via a CASE expression
    // that compares `attempt_count` against the row's last-attempt timestamp.
    // Nothing to compute here.
    Ok(())
}

/// Revert any rows that were mid-run at shutdown back to `pending` so the
/// next worker can pick them up.
async fn revert_in_progress(pool: &PgPool) -> sqlx::Result<()> {
    let res = sqlx::query(
        "UPDATE manifestations \
         SET enrichment_status = 'pending' \
         WHERE enrichment_status = 'in_progress'",
    )
    .execute(pool)
    .await?;
    if res.rows_affected() > 0 {
        info!(
            count = res.rows_affected(),
            "reverted in_progress rows to pending"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Task 35: queue integration tests ──────────────────────────────────

    /// Tests run against `reverie_ingestion` — the role with the
    /// `manifestations_ingestion_full_access` RLS policy, which lets the
    /// fixture INSERT queue rows with `RETURNING id`.  See the orchestrator
    /// tests for the companion grant migration on `field_locks`.
    fn db_url() -> String {
        std::env::var("DATABASE_URL_INGESTION").unwrap_or_else(|_| {
            "postgres://reverie_ingestion:reverie_ingestion@localhost:5433/reverie_dev".into()
        })
    }

    /// Defuse any stale eligible rows left behind by prior tests (panics,
    /// unexpected errors).  Leaves only rows this test created as claim
    /// candidates.
    async fn quiesce_queue(pool: &PgPool) {
        let _ = sqlx::query(
            "UPDATE manifestations SET enrichment_status = 'complete' \
             WHERE enrichment_status IN ('pending', 'failed', 'in_progress')",
        )
        .execute(pool)
        .await;
    }

    fn test_config_with_max_attempts(max_attempts: u32) -> Config {
        use crate::config::{CleanupMode, CoverConfig, EnrichmentConfig};

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
                enabled: true,
                concurrency: 2,
                poll_idle_secs: 30,
                fetch_budget_secs: 15,
                http_timeout_secs: 10,
                max_attempts,
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
            openlibrary_base_url: "https://example.invalid".into(),
            googlebooks_base_url: "https://example.invalid".into(),
            googlebooks_api_key: None,
            hardcover_base_url: "https://example.invalid".into(),
            hardcover_api_token: None,
            operator_contact: None,
        }
    }

    /// Insert a work + manifestation pair with a given enrichment state and
    /// return `(work_id, manifestation_id, marker_path)`.  Cleanup via
    /// `cleanup_queue_fixture`.
    async fn insert_queue_fixture(
        pool: &PgPool,
        status: &str,
        attempt_count: i32,
        attempted_at_offset_secs: Option<i64>,
    ) -> (Uuid, Uuid, String) {
        let marker = Uuid::new_v4().simple().to_string();
        let work_id: Uuid = sqlx::query_scalar(
            "INSERT INTO works (title, sort_title) VALUES ($1, $1) RETURNING id",
        )
        .bind(format!("QueueFixture-{marker}"))
        .fetch_one(pool)
        .await
        .unwrap();

        let path = format!("/tmp/queue-{marker}.epub");
        let manifestation_id: Uuid = sqlx::query_scalar(
            "INSERT INTO manifestations \
               (work_id, format, file_path, file_hash, file_size_bytes, \
                ingestion_status, validation_status, \
                enrichment_status, enrichment_attempt_count, enrichment_attempted_at) \
             VALUES ($1, 'epub'::manifestation_format, $2, $3, 1000, \
                     'complete'::ingestion_status, 'valid'::validation_status, \
                     $4::enrichment_status, $5, \
                     CASE WHEN $6::bigint IS NULL THEN NULL \
                          ELSE now() - ($6 || ' seconds')::interval END) \
             RETURNING id",
        )
        .bind(work_id)
        .bind(&path)
        .bind(format!("queue-hash-{marker}"))
        .bind(status)
        .bind(attempt_count)
        .bind(attempted_at_offset_secs)
        .fetch_one(pool)
        .await
        .unwrap();
        (work_id, manifestation_id, path)
    }

    async fn cleanup_queue_fixture(pool: &PgPool, work_id: Uuid) {
        let _ = sqlx::query("DELETE FROM manifestations WHERE work_id = $1")
            .bind(work_id)
            .execute(pool)
            .await;
        let _ = sqlx::query("DELETE FROM works WHERE id = $1")
            .bind(work_id)
            .execute(pool)
            .await;
    }

    /// Two concurrent `claim_next` calls on the same eligible row — exactly
    /// one claims it (FOR UPDATE SKIP LOCKED serialises the claim path).
    #[tokio::test]
    #[ignore] // Requires running postgres with applied migrations
    async fn two_workers_race_exactly_one_claims() {
        let pool = PgPool::connect(&db_url()).await.unwrap();
        quiesce_queue(&pool).await;
        let (work_id, m_id, _path) = insert_queue_fixture(&pool, "pending", 0, None).await;

        // Race two claims.  Each call acquires its own connection from the pool.
        let (a, b) = tokio::join!(claim_next(&pool), claim_next(&pool));
        let a = a.unwrap();
        let b = b.unwrap();
        let claimed: Vec<(Uuid, i32)> = [a, b].into_iter().flatten().collect();
        assert_eq!(
            claimed.len(),
            1,
            "expected exactly one successful claim, got {}",
            claimed.len()
        );
        assert_eq!(claimed[0].0, m_id);

        cleanup_queue_fixture(&pool, work_id).await;
    }

    /// A failed row within the backoff window is NOT claimable; once the
    /// window elapses, the next claim picks it up.
    #[tokio::test]
    #[ignore] // Requires running postgres with applied migrations
    async fn retry_backoff_window_blocks_then_releases() {
        let pool = PgPool::connect(&db_url()).await.unwrap();
        quiesce_queue(&pool).await;

        // attempt_count=1 → 5-minute backoff.  Set attempted_at to 60 seconds
        // ago (still inside the window).
        let (work_id, m_id, _) = insert_queue_fixture(&pool, "failed", 1, Some(60)).await;
        let claim_inside = claim_next(&pool).await.unwrap();
        assert!(
            claim_inside.is_none(),
            "row inside backoff window must not be claimable, got {claim_inside:?}"
        );

        // Move attempted_at back 6 minutes (outside the window).
        sqlx::query(
            "UPDATE manifestations \
             SET enrichment_attempted_at = now() - INTERVAL '6 minutes' WHERE id = $1",
        )
        .bind(m_id)
        .execute(&pool)
        .await
        .unwrap();

        let claim_outside = claim_next(&pool).await.unwrap();
        let (claimed_id, new_attempt) =
            claim_outside.expect("row past backoff window should be claimable");
        assert_eq!(claimed_id, m_id);
        assert_eq!(new_attempt, 2, "attempt_count should increment on claim");

        cleanup_queue_fixture(&pool, work_id).await;
    }

    /// After `max_attempts` failures the row transitions to `skipped` so the
    /// queue stops retrying it.
    #[tokio::test]
    #[ignore] // Requires running postgres with applied migrations
    async fn max_attempts_transitions_to_skipped() {
        let pool = PgPool::connect(&db_url()).await.unwrap();
        quiesce_queue(&pool).await;
        let max_attempts: u32 = 3;
        let config = test_config_with_max_attempts(max_attempts);

        // Row is `in_progress` at the Nth attempt (as if just claimed).
        let (work_id, m_id, _) =
            insert_queue_fixture(&pool, "in_progress", max_attempts as i32, Some(10)).await;

        // Simulate a failed run — final attempt, no retry_after.
        mark_failed(
            &pool,
            m_id,
            max_attempts as i32,
            &config,
            None,
            Some("simulated final failure"),
        )
        .await
        .unwrap();

        let status: String =
            sqlx::query_scalar("SELECT enrichment_status::text FROM manifestations WHERE id = $1")
                .bind(m_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(
            status, "skipped",
            "row should be marked skipped at max_attempts"
        );

        cleanup_queue_fixture(&pool, work_id).await;
    }

    /// `revert_in_progress` flips every `in_progress` row back to `pending`
    /// so the next worker can re-claim them after a shutdown.
    #[tokio::test]
    #[ignore] // Requires running postgres with applied migrations
    async fn shutdown_reverts_in_progress_to_pending() {
        let pool = PgPool::connect(&db_url()).await.unwrap();
        quiesce_queue(&pool).await;

        let (work_id_a, m_a, _) = insert_queue_fixture(&pool, "in_progress", 1, Some(5)).await;
        let (work_id_b, m_b, _) = insert_queue_fixture(&pool, "in_progress", 2, Some(5)).await;
        // A `pending` row shouldn't be changed (already pending).
        let (work_id_c, m_c, _) = insert_queue_fixture(&pool, "pending", 0, None).await;

        revert_in_progress(&pool).await.unwrap();

        for (id, expected) in [(m_a, "pending"), (m_b, "pending"), (m_c, "pending")] {
            let s: String = sqlx::query_scalar(
                "SELECT enrichment_status::text FROM manifestations WHERE id = $1",
            )
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
            assert_eq!(
                s, expected,
                "manifestation {id} status mismatch (expected {expected}, got {s})"
            );
        }

        cleanup_queue_fixture(&pool, work_id_a).await;
        cleanup_queue_fixture(&pool, work_id_b).await;
        cleanup_queue_fixture(&pool, work_id_c).await;
    }
}
