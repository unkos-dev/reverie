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
use crate::models::enrichment_status::EnrichmentStatus;

use super::orchestrator::{self, RunOutcome};

// Retry backoff is applied inside the `claim_next` CTE as a SQL CASE
// expression (5m, 30m, 2h, 8h, then 24h). That is the authoritative
// schedule; there is no Rust mirror to keep in sync.

/// Spawn the queue worker loop.  Returns when `cancel` fires, reverting any
/// `in_progress` row back to `pending`.
///
/// # Errors
///
/// Returns an error if any of the per-tick queue queries fail — typically a
/// `claim_next` failure during normal polling (transient DB error, pool
/// exhaustion) — or if the shutdown-time revert of `in_progress` rows to
/// `pending` fails. Both failure modes exit the worker loop; the supervisor
/// is responsible for restarts.
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
            () = cancel.cancelled() => {
                info!("enrichment queue shutting down");
                revert_in_progress(&pool).await?;
                return Ok(());
            }
            _ = interval.tick() => {
                // Drain as many pending rows as semaphore permits allow.
                loop {
                    let Ok(permit) = semaphore.clone().try_acquire_owned() else {
                        break; // fully busy; next tick
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
///
/// The claim also clears `enrichment_rerun_requested`: a rerun requested
/// before the claim is satisfied by this very run (its snapshot postdates
/// the edit). Only an edit landing after the claim must survive to the
/// completion bookkeeping, which converts the flag back into `pending`.
async fn claim_next(pool: &PgPool) -> sqlx::Result<Option<(Uuid, i32)>> {
    let row = sqlx::query!(
        r"WITH eligible AS (
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
              SET enrichment_status         = 'in_progress',
                  enrichment_attempted_at   = now(),
                  enrichment_attempt_count  = m.enrichment_attempt_count + 1,
                  enrichment_rerun_requested = FALSE
             FROM eligible
            WHERE m.id = eligible.id
           RETURNING m.id, m.enrichment_attempt_count",
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| (r.id, r.enrichment_attempt_count)))
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

/// Completion bookkeeping for a successful run. Guarded on `in_progress` so
/// only the claim holder transitions the row (a shutdown-time revert already
/// released the claim). A rerun requested mid-run (an identifier edit landed
/// after this run snapshotted its lookup keys) turns the row back into a
/// fresh, immediately eligible `pending` instead of `complete`.
async fn mark_complete(pool: &PgPool, id: Uuid) -> sqlx::Result<()> {
    sqlx::query!(
        "UPDATE manifestations \
         SET enrichment_status = CASE WHEN enrichment_rerun_requested \
                                      THEN 'pending'::enrichment_status \
                                      ELSE 'complete'::enrichment_status END, \
             enrichment_attempt_count = CASE WHEN enrichment_rerun_requested \
                                             THEN 0 ELSE enrichment_attempt_count END, \
             enrichment_attempted_at = CASE WHEN enrichment_rerun_requested \
                                            THEN NULL ELSE enrichment_attempted_at END, \
             enrichment_error = NULL, \
             enrichment_rerun_requested = FALSE \
         WHERE id = $1 AND enrichment_status = 'in_progress'",
        id,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Failure bookkeeping, guarded on `in_progress` like [`mark_complete`]. A
/// rerun requested mid-run overrides the failure transition entirely: the
/// operator's edit resets the row to a fresh `pending` with no backoff, the
/// same shape an edit on an idle failed row produces, so the rerun is never
/// deferred behind a backoff window or swallowed by `skipped`.
async fn mark_failed(
    pool: &PgPool,
    id: Uuid,
    attempt_count: i32,
    config: &Config,
    retry_after: Option<Duration>,
    error: Option<&str>,
) -> sqlx::Result<()> {
    let max = config.enrichment.max_attempts.cast_signed();
    let next_status = if attempt_count >= max {
        EnrichmentStatus::Skipped
    } else {
        EnrichmentStatus::Failed
    };

    // When rate-limited, bump `enrichment_attempted_at` forward so the
    // backoff window respects Retry-After semantics.
    if let Some(ra) = retry_after {
        let secs = i64::try_from(ra.as_secs()).unwrap_or(i64::MAX);
        // `query!` borrows the bind argument; the `String` must outlive the
        // macro expansion, so `secs.to_string()` cannot be inlined into the
        // bind list below — keep this binding distinct.
        let secs_str = secs.to_string();
        sqlx::query!(
            "UPDATE manifestations \
             SET enrichment_status = CASE WHEN enrichment_rerun_requested \
                                          THEN 'pending'::enrichment_status ELSE $1 END, \
                 enrichment_attempt_count = CASE WHEN enrichment_rerun_requested \
                                                 THEN 0 ELSE enrichment_attempt_count END, \
                 enrichment_attempted_at = CASE WHEN enrichment_rerun_requested \
                                                THEN NULL \
                                                ELSE now() + ($2 || ' seconds')::interval END, \
                 enrichment_error = CASE WHEN enrichment_rerun_requested \
                                         THEN NULL ELSE $3 END, \
                 enrichment_rerun_requested = FALSE \
             WHERE id = $4 AND enrichment_status = 'in_progress'",
            next_status as EnrichmentStatus,
            secs_str,
            error,
            id,
        )
        .execute(pool)
        .await?;
    } else {
        sqlx::query!(
            "UPDATE manifestations \
             SET enrichment_status = CASE WHEN enrichment_rerun_requested \
                                          THEN 'pending'::enrichment_status ELSE $1 END, \
                 enrichment_attempt_count = CASE WHEN enrichment_rerun_requested \
                                                 THEN 0 ELSE enrichment_attempt_count END, \
                 enrichment_attempted_at = CASE WHEN enrichment_rerun_requested \
                                                THEN NULL ELSE enrichment_attempted_at END, \
                 enrichment_error = CASE WHEN enrichment_rerun_requested \
                                         THEN NULL ELSE $2 END, \
                 enrichment_rerun_requested = FALSE \
             WHERE id = $3 AND enrichment_status = 'in_progress'",
            next_status as EnrichmentStatus,
            error,
            id,
        )
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
    let res = sqlx::query!(
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
#[expect(
    clippy::cast_possible_wrap,
    reason = "test code: casts in tests for known-small literal constants are intentional and not worth wrapping in try_from/cast_signed"
)]
mod tests {
    use super::*;
    use crate::test_support::db::ingestion_pool_for;

    // ── Task 35: queue integration tests ──────────────────────────────────
    //
    // Tests run against `reverie_ingestion` — the role with the
    // `manifestations_ingestion_full_access` RLS policy, which lets the
    // fixture INSERT queue rows with `RETURNING id`.  See the orchestrator
    // tests for the companion grant migration on `field_locks`.

    fn test_config_with_max_attempts(max_attempts: u32) -> Config {
        use crate::config::{CleanupMode, CoverConfig, EnrichmentConfig};
        use crate::models::manifestation_format::ManifestationFormat;

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
            password_breach_check_url: "https://api.pwnedpasswords.com/range".into(),
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
            writeback: crate::config::WritebackConfig {
                enabled: false,
                concurrency: 1,
                poll_idle_secs: 5,
                max_attempts: 3,
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
            openlibrary_base_url: "https://example.invalid".into(),
            googlebooks_base_url: "https://example.invalid".into(),
            googlebooks_api_key: None,
            hardcover_base_url: "https://example.invalid".into(),
            hardcover_api_token: None,
            operator_contact: None,
            ingestion_dsn_defaulted: false,
        }
    }

    /// Insert a work + manifestation pair with a given enrichment state and
    /// return `(work_id, manifestation_id, marker_path)`.  Cleanup via
    /// `cleanup_queue_fixture`.
    async fn insert_queue_fixture(
        pool: &PgPool,
        status: EnrichmentStatus,
        attempt_count: i32,
        attempted_at_offset_secs: Option<i64>,
    ) -> (Uuid, Uuid, String) {
        let marker = Uuid::new_v4().simple().to_string();
        let work_title = format!("QueueFixture-{marker}");
        let work_id = sqlx::query_scalar!(
            "INSERT INTO works (title, sort_title) VALUES ($1, $1) RETURNING id",
            work_title,
        )
        .fetch_one(pool)
        .await
        .unwrap();

        let path = format!("/tmp/queue-{marker}.epub");
        let hash = format!("queue-hash-{marker}");
        let manifestation_id = sqlx::query_scalar!(
            "INSERT INTO manifestations \
               (work_id, format, file_path, ingestion_file_hash, current_file_hash, \
                file_size_bytes, ingestion_status, validation_status, \
                enrichment_status, enrichment_attempt_count, enrichment_attempted_at) \
             VALUES ($1, 'epub'::manifestation_format, $2, $3, $3, 1000, \
                     'complete'::ingestion_status, 'clean'::validation_status, \
                     $4, $5, \
                     CASE WHEN $6::bigint IS NULL THEN NULL \
                          ELSE now() - ($6 || ' seconds')::interval END) \
             RETURNING id",
            work_id,
            path,
            hash,
            status as EnrichmentStatus,
            attempt_count,
            attempted_at_offset_secs,
        )
        .fetch_one(pool)
        .await
        .unwrap();
        (work_id, manifestation_id, path)
    }

    /// Two concurrent `claim_next` calls on the same eligible row — exactly
    /// one claims it (FOR UPDATE SKIP LOCKED serialises the claim path).
    #[sqlx::test(migrations = "./migrations")]
    async fn two_workers_race_exactly_one_claims(pool: PgPool) {
        let pool = ingestion_pool_for(&pool).await;
        let (_work_id, m_id, _path) =
            insert_queue_fixture(&pool, EnrichmentStatus::Pending, 0, None).await;

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
    }

    /// A failed row within the backoff window is NOT claimable; once the
    /// window elapses, the next claim picks it up.
    #[sqlx::test(migrations = "./migrations")]
    async fn retry_backoff_window_blocks_then_releases(pool: PgPool) {
        let pool = ingestion_pool_for(&pool).await;

        // attempt_count=1 → 5-minute backoff.  Set attempted_at to 60 seconds
        // ago (still inside the window).
        let (_work_id, m_id, _) =
            insert_queue_fixture(&pool, EnrichmentStatus::Failed, 1, Some(60)).await;
        let claim_inside = claim_next(&pool).await.unwrap();
        assert!(
            claim_inside.is_none(),
            "row inside backoff window must not be claimable, got {claim_inside:?}"
        );

        // Move attempted_at back 6 minutes (outside the window).
        sqlx::query!(
            "UPDATE manifestations \
             SET enrichment_attempted_at = now() - INTERVAL '6 minutes' WHERE id = $1",
            m_id,
        )
        .execute(&pool)
        .await
        .unwrap();

        let claim_outside = claim_next(&pool).await.unwrap();
        let (claimed_id, new_attempt) =
            claim_outside.expect("row past backoff window should be claimable");
        assert_eq!(claimed_id, m_id);
        assert_eq!(new_attempt, 2, "attempt_count should increment on claim");
    }

    /// After `max_attempts` failures the row transitions to `skipped` so the
    /// queue stops retrying it.
    #[sqlx::test(migrations = "./migrations")]
    async fn max_attempts_transitions_to_skipped(pool: PgPool) {
        let pool = ingestion_pool_for(&pool).await;
        let max_attempts: u32 = 3;
        let config = test_config_with_max_attempts(max_attempts);

        // Row is `in_progress` at the Nth attempt (as if just claimed).
        let (_work_id, m_id, _) = insert_queue_fixture(
            &pool,
            EnrichmentStatus::InProgress,
            max_attempts as i32,
            Some(10),
        )
        .await;

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

        // `enrichment_status` is `enrichment_status NOT NULL`; the
        // `AS "enrichment_status: _"` override decodes via the
        // `EnrichmentStatus` `sqlx::Type` impl so an unknown PG variant
        // would surface as a decode error rather than a silent miss.
        let status = sqlx::query_scalar!(
            "SELECT enrichment_status AS \"enrichment_status!: EnrichmentStatus\" \
             FROM manifestations WHERE id = $1",
            m_id,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            status,
            EnrichmentStatus::Skipped,
            "row should be marked skipped at max_attempts"
        );
    }

    /// `revert_in_progress` flips every `in_progress` row back to `pending`
    /// so the next worker can re-claim them after a shutdown.
    #[sqlx::test(migrations = "./migrations")]
    async fn shutdown_reverts_in_progress_to_pending(pool: PgPool) {
        let pool = ingestion_pool_for(&pool).await;

        let (_work_id_a, m_a, _) =
            insert_queue_fixture(&pool, EnrichmentStatus::InProgress, 1, Some(5)).await;
        let (_work_id_b, m_b, _) =
            insert_queue_fixture(&pool, EnrichmentStatus::InProgress, 2, Some(5)).await;
        // A `pending` row shouldn't be changed (already pending).
        let (_work_id_c, m_c, _) =
            insert_queue_fixture(&pool, EnrichmentStatus::Pending, 0, None).await;

        revert_in_progress(&pool).await.unwrap();

        for (id, expected) in [
            (m_a, EnrichmentStatus::Pending),
            (m_b, EnrichmentStatus::Pending),
            (m_c, EnrichmentStatus::Pending),
        ] {
            let s = sqlx::query_scalar!(
                "SELECT enrichment_status AS \"enrichment_status!: EnrichmentStatus\" \
                 FROM manifestations WHERE id = $1",
                id,
            )
            .fetch_one(&pool)
            .await
            .unwrap();
            assert_eq!(
                s, expected,
                "manifestation {id} status mismatch (expected {expected:?}, got {s:?})"
            );
        }
    }

    struct QueueRowState {
        status: EnrichmentStatus,
        attempt_count: i32,
        attempted_at_set: bool,
        error: Option<String>,
        rerun_requested: bool,
    }

    async fn queue_row_state(pool: &PgPool, id: Uuid) -> QueueRowState {
        let r = sqlx::query!(
            "SELECT enrichment_status AS \"status!: EnrichmentStatus\", \
                    enrichment_attempt_count, enrichment_attempted_at, \
                    enrichment_error, enrichment_rerun_requested \
             FROM manifestations WHERE id = $1",
            id,
        )
        .fetch_one(pool)
        .await
        .unwrap();
        QueueRowState {
            status: r.status,
            attempt_count: r.enrichment_attempt_count,
            attempted_at_set: r.enrichment_attempted_at.is_some(),
            error: r.enrichment_error,
            rerun_requested: r.enrichment_rerun_requested,
        }
    }

    async fn set_rerun_requested(pool: &PgPool, id: Uuid) {
        sqlx::query!(
            "UPDATE manifestations SET enrichment_rerun_requested = TRUE WHERE id = $1",
            id,
        )
        .execute(pool)
        .await
        .unwrap();
    }

    /// An identifier edit on an actively running row sets the rerun flag
    /// without releasing the claim, so a second worker must not pick the
    /// row up while the original run is still active.
    #[sqlx::test(migrations = "./migrations")]
    async fn rerun_flagged_in_progress_row_is_not_claimable(pool: PgPool) {
        let pool = ingestion_pool_for(&pool).await;
        let (_work_id, m_id, _) =
            insert_queue_fixture(&pool, EnrichmentStatus::InProgress, 1, Some(10)).await;
        set_rerun_requested(&pool, m_id).await;

        let claim = claim_next(&pool).await.unwrap();
        assert!(
            claim.is_none(),
            "an in_progress row with a rerun request must stay unclaimable, got {claim:?}"
        );
    }

    /// A rerun requested before any claim is satisfied by the claim itself
    /// (the new run's snapshot postdates the edit): the claim clears the
    /// flag, so completion lands on `complete` instead of re-queueing a
    /// redundant second run.
    #[sqlx::test(migrations = "./migrations")]
    async fn claim_clears_pre_claim_rerun_flag(pool: PgPool) {
        let pool = ingestion_pool_for(&pool).await;
        let (_work_id, m_id, _) =
            insert_queue_fixture(&pool, EnrichmentStatus::Pending, 0, None).await;
        set_rerun_requested(&pool, m_id).await;

        let (claimed_id, _) = claim_next(&pool).await.unwrap().expect("claim");
        assert_eq!(claimed_id, m_id);
        let state = queue_row_state(&pool, m_id).await;
        assert!(
            !state.rerun_requested,
            "the claim must absorb a pre-claim rerun request"
        );

        mark_complete(&pool, m_id).await.unwrap();
        let state = queue_row_state(&pool, m_id).await;
        assert_eq!(state.status, EnrichmentStatus::Complete);
    }

    /// A rerun requested while the run was active converts completion into
    /// a fresh, immediately eligible `pending` row instead of `complete`.
    #[sqlx::test(migrations = "./migrations")]
    async fn mark_complete_requeues_when_rerun_requested(pool: PgPool) {
        let pool = ingestion_pool_for(&pool).await;
        let (_work_id, m_id, _) =
            insert_queue_fixture(&pool, EnrichmentStatus::InProgress, 2, Some(10)).await;
        set_rerun_requested(&pool, m_id).await;

        mark_complete(&pool, m_id).await.unwrap();

        let state = queue_row_state(&pool, m_id).await;
        assert_eq!(state.status, EnrichmentStatus::Pending);
        assert_eq!(
            state.attempt_count, 0,
            "re-queue must clear the backoff counter"
        );
        assert!(
            !state.attempted_at_set,
            "re-queue must null the attempt timestamp"
        );
        assert!(state.error.is_none());
        assert!(
            !state.rerun_requested,
            "the flag is consumed by the re-queue"
        );

        let (claimed_id, _) = claim_next(&pool).await.unwrap().expect("re-claim");
        assert_eq!(
            claimed_id, m_id,
            "the re-queued row is immediately eligible"
        );
    }

    /// Without a rerun request completion still lands on `complete`.
    #[sqlx::test(migrations = "./migrations")]
    async fn mark_complete_without_rerun_completes(pool: PgPool) {
        let pool = ingestion_pool_for(&pool).await;
        let (_work_id, m_id, _) =
            insert_queue_fixture(&pool, EnrichmentStatus::InProgress, 1, Some(10)).await;

        mark_complete(&pool, m_id).await.unwrap();

        let state = queue_row_state(&pool, m_id).await;
        assert_eq!(state.status, EnrichmentStatus::Complete);
        assert_eq!(state.attempt_count, 1, "attempt bookkeeping is untouched");
    }

    /// A mid-run edit overrides the failure path in both branches: instead
    /// of `failed`-with-backoff (Retry-After) or `skipped` (attempt cap),
    /// the row resets to a fresh eligible `pending`.
    #[sqlx::test(migrations = "./migrations")]
    async fn mark_failed_requeues_when_rerun_requested(pool: PgPool) {
        let pool = ingestion_pool_for(&pool).await;
        let max_attempts: u32 = 3;
        let config = test_config_with_max_attempts(max_attempts);

        // Attempt cap reached — without the flag this row would go `skipped`.
        let (_work_id, m_capped, _) = insert_queue_fixture(
            &pool,
            EnrichmentStatus::InProgress,
            max_attempts as i32,
            Some(10),
        )
        .await;
        set_rerun_requested(&pool, m_capped).await;
        mark_failed(
            &pool,
            m_capped,
            max_attempts as i32,
            &config,
            None,
            Some("boom"),
        )
        .await
        .unwrap();

        // Rate-limited — without the flag this row would sit in a
        // Retry-After backoff window.
        let (_work_id, m_limited, _) =
            insert_queue_fixture(&pool, EnrichmentStatus::InProgress, 1, Some(10)).await;
        set_rerun_requested(&pool, m_limited).await;
        mark_failed(
            &pool,
            m_limited,
            1,
            &config,
            Some(Duration::from_mins(2)),
            Some("rate limited"),
        )
        .await
        .unwrap();

        for id in [m_capped, m_limited] {
            let state = queue_row_state(&pool, id).await;
            assert_eq!(state.status, EnrichmentStatus::Pending, "row {id}");
            assert_eq!(state.attempt_count, 0, "row {id}: backoff counter cleared");
            assert!(
                !state.attempted_at_set,
                "row {id}: attempt timestamp nulled"
            );
            assert!(state.error.is_none(), "row {id}: error cleared");
            assert!(!state.rerun_requested, "row {id}: flag consumed");
        }
    }
}
