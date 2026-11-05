//! Background enrichment queue worker.
//!
//! Claims manifestations from the `manifestations` table using an atomic
//! `FOR UPDATE SKIP LOCKED` CTE so multiple workers can race without double
//! processing.  Applies an exponential-ish retry backoff and marks rows as
//! `skipped` after `max_attempts`.  On startup and shutdown, reverts any
//! `in_progress` rows back to `pending` so a fresh worker can re-claim them.

use std::future::Future;
use std::panic::AssertUnwindSafe;
use std::sync::Arc;
use std::time::Duration;

use futures::FutureExt;
use sqlx::PgPool;
use tokio::sync::Semaphore;
use tokio::time::{Instant, Interval};
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};
use uuid::Uuid;

use crate::config::Config;
use crate::models::enrichment_failure::{FailureClass, FailureEntry};
use crate::models::enrichment_status::EnrichmentStatus;

use super::orchestrator::{self, RunOutcome};

// Retry backoff is applied inside the `claim_next` CTE as a SQL CASE
// expression (5m, 30m, 2h, 8h, then 24h). That is the authoritative
// schedule; there is no Rust mirror to keep in sync.

/// Spawn the queue worker loop.  Returns when `cancel` fires, reverting any
/// `in_progress` row back to `pending`.
///
/// Rows left `in_progress` by a prior process are reverted before the first
/// poll so an abrupt process exit cannot strand work permanently.
///
/// # Errors
///
/// Returns an error if startup recovery, a per-tick queue query, or shutdown
/// recovery fails. Any failure exits the worker loop; the supervisor is
/// responsible for restarts.
pub async fn spawn_queue(
    pool: PgPool,
    config: Config,
    cancel: CancellationToken,
) -> anyhow::Result<()> {
    queue_with_runner(
        pool,
        config,
        cancel,
        |pool, config, id, claim_seq, deadline| async move {
            orchestrator::run_once(&pool, &config, id, claim_seq, deadline).await
        },
    )
    .await
}

async fn queue_with_runner<R, F>(
    pool: PgPool,
    config: Config,
    cancel: CancellationToken,
    run: R,
) -> anyhow::Result<()>
where
    R: Fn(PgPool, Config, Uuid, i64, Instant) -> F + Clone + Send + Sync + 'static,
    F: Future<Output = anyhow::Result<RunOutcome>> + Send,
{
    if !config.enrichment.enabled {
        info!("enrichment queue disabled by config");
        cancel.cancelled().await;
        return Ok(());
    }

    revert_in_progress(&pool).await?;

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

    let mut jobs = tokio::task::JoinSet::new();
    let outcome: anyhow::Result<()> = 'queue: loop {
        tokio::select! {
            () = cancel.cancelled() => {
                info!("enrichment queue shutting down");
                break Ok(());
            }
            result = jobs.join_next(), if !jobs.is_empty() => {
                if let Some(Err(error)) = result {
                    warn!(%error, "enrichment job task failed");
                }
            }
            _ = interval.tick() => {
                if let Err(error) = rescue_stale_claims(&pool, &config).await {
                    break 'queue Err(error.into());
                }
                // Drain as many pending rows as semaphore permits allow.
                loop {
                    let Ok(permit) = semaphore.clone().try_acquire_owned() else {
                        break; // fully busy; next tick
                    };
                    if cancel.is_cancelled() {
                        break;
                    }
                    let claim = match claim_next(&pool).await {
                        Ok(claim) => claim,
                        Err(error) => break 'queue Err(error.into()),
                    };
                    let Some((id, attempt_count, claim_seq)) = claim else {
                        drop(permit);
                        break;
                    };
                    let deadline = Instant::now() + Duration::from_secs(u64::from(config.enrichment.run_timeout_secs));
                    let pool = pool.clone();
                    let cfg = config.clone();
                    let run = run.clone();
                    jobs.spawn(async move {
                        let _p = permit;
                        let result = AssertUnwindSafe(async {
                            run(pool.clone(), cfg.clone(), id, claim_seq, deadline).await
                        })
                        .catch_unwind()
                        .await
                        .unwrap_or_else(|payload| {
                            warn!(%id, panic_message = ?panic_message(payload.as_ref()), "enrichment task panicked");
                            Err(anyhow::anyhow!("enrichment task panicked"))
                        });
                        if let Err(error) = &result {
                            if error.is::<orchestrator::CleanupFailure>() {
                                warn!(%error, %id, "enrichment: unresolved transaction; leaving claim for rescue");
                                return;
                            }
                            if error.is::<orchestrator::ClaimLost>() {
                                warn!(%id, claim_seq, "enrichment: claim rescued before catalogue writes; discarding attempt");
                                return;
                            }
                            if error.is::<orchestrator::RunTimeout>() {
                                warn!(%id, run_timeout_secs = cfg.enrichment.run_timeout_secs, "enrichment run exceeded time budget");
                            }
                        }
                        if let Err(e) = finish(&pool, &cfg, id, attempt_count, claim_seq, result).await {
                            warn!(error = %e, %id, "queue: finish bookkeeping failed");
                        }
                    });
                }
            }
        }
    };
    while let Some(result) = jobs.join_next().await {
        if let Err(error) = result {
            warn!(%error, "enrichment job task failed during drain");
        }
    }
    outcome?;
    revert_in_progress(&pool).await?;
    Ok(())
}

fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
        .unwrap_or("non-string panic payload")
        .chars()
        .filter(|character| !character.is_control())
        .collect()
}

/// Atomic claim: pick the oldest eligible row and flip it to `in_progress`.
///
/// Returns `Some((id, new_attempt_count))` when a row was claimed; `None`
/// when the queue is empty (or every row is still in its backoff window or
/// deferred by a provider's `Retry-After`).
///
/// The claim also clears `enrichment_rerun_requested`: a rerun requested
/// before the claim is satisfied by this very run (its snapshot postdates
/// the edit). Only an edit landing after the claim must survive to the
/// completion bookkeeping, which converts the flag back into `pending`.
async fn claim_next(pool: &PgPool) -> sqlx::Result<Option<(Uuid, i32, i64)>> {
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
               AND (
                 enrichment_retry_not_before IS NULL
                 OR enrichment_retry_not_before <= now()
               )
             ORDER BY enrichment_attempted_at NULLS FIRST, id
             LIMIT 1
             FOR UPDATE SKIP LOCKED
           )
           UPDATE manifestations m
              SET enrichment_status         = 'in_progress',
                  enrichment_attempted_at   = now(),
                  enrichment_attempt_count  = m.enrichment_attempt_count + 1,
                  enrichment_rerun_requested = FALSE,
                  enrichment_claim_seq      = m.enrichment_claim_seq + 1
             FROM eligible
            WHERE m.id = eligible.id
           RETURNING m.id, m.enrichment_attempt_count, m.enrichment_claim_seq",
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| (r.id, r.enrichment_attempt_count, r.enrichment_claim_seq)))
}

/// Fail claims still `in_progress` after twice the run budget. Every write an
/// attempt makes is fenced on its `enrichment_claim_seq`, so a stale attempt
/// that later resumes cannot change the row or the catalogue.
async fn rescue_stale_claims(pool: &PgPool, config: &Config) -> sqlx::Result<()> {
    let stale_after = f64::from(config.enrichment.run_timeout_secs) * 2.0;
    let mut tx = pool.begin().await?;
    let stale = sqlx::query!(
        "SELECT id, enrichment_claim_seq FROM manifestations
         WHERE enrichment_status = 'in_progress'
           AND enrichment_attempted_at < now() - make_interval(secs => $1)
         ORDER BY enrichment_attempted_at
         LIMIT 100
         FOR UPDATE SKIP LOCKED",
        stale_after,
    )
    .fetch_all(&mut *tx)
    .await?;
    if stale.is_empty() {
        return tx.commit().await;
    }
    let (ids, seqs): (Vec<Uuid>, Vec<i64>) =
        stale.iter().map(|r| (r.id, r.enrichment_claim_seq)).unzip();
    let failures = serde_json::to_value([FailureEntry {
        source: None,
        class: FailureClass::Internal,
    }])
    .map_err(|e| sqlx::Error::Encode(Box::new(e)))?;
    let rescued = fail_claims(
        &mut *tx,
        &ids,
        &seqs,
        config,
        Some("enrichment claim expired"),
        &failures,
    )
    .await?;
    tx.commit().await?;
    warn!(rescued, ?ids, "enrichment: rescued stale claims");
    Ok(())
}

/// Book-keeping after a `run_once` call.  Transitions rows to
/// `complete` / `failed` / `skipped` depending on the outcome.
async fn finish(
    pool: &PgPool,
    config: &Config,
    id: Uuid,
    attempt_count: i32,
    claim_seq: i64,
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
                    claim_seq,
                    config,
                    retry_after,
                    Some("transient source failures"),
                    &failure_entries(&outcome.source_failures),
                )
                .await?;
            } else {
                mark_complete(pool, id, claim_seq).await?;
            }
        }
        Err(e) => {
            warn!(error = %e, %id, "enrichment run_once failed");
            let internal = [FailureEntry {
                source: None,
                class: FailureClass::Internal,
            }];
            mark_failed(
                pool,
                id,
                attempt_count,
                claim_seq,
                config,
                None,
                Some(&e.to_string()),
                &internal,
            )
            .await?;
        }
    }
    Ok(())
}

/// One entry per failing source, ordered by source key. When a source failed
/// on several lookup keys in one run, the last failure stands.
fn failure_entries(failures: &[orchestrator::SourceFailure]) -> Vec<FailureEntry> {
    let by_source: std::collections::BTreeMap<&str, FailureClass> = failures
        .iter()
        .map(|failure| (failure.source_id.as_str(), failure.class))
        .collect();
    by_source
        .into_iter()
        .map(|(source, class)| FailureEntry {
            source: Some(source.to_owned()),
            class,
        })
        .collect()
}

/// Completion bookkeeping for a successful run. Guarded on `in_progress` so
/// only the claim holder transitions the row (a shutdown-time revert already
/// released the claim). A rerun requested mid-run (an identifier edit landed
/// after this run snapshotted its lookup keys) turns the row back into a
/// fresh, immediately eligible `pending` instead of `complete`.
async fn mark_complete(
    executor: impl sqlx::PgExecutor<'_>,
    id: Uuid,
    claim_seq: i64,
) -> sqlx::Result<()> {
    sqlx::query!(
        "UPDATE manifestations \
         SET enrichment_status = CASE WHEN enrichment_rerun_requested \
                                      THEN 'pending'::enrichment_status \
                                      ELSE 'complete'::enrichment_status END, \
             enrichment_attempt_count = CASE WHEN enrichment_rerun_requested \
                                             THEN 0 ELSE enrichment_attempt_count END, \
             enrichment_attempted_at = CASE WHEN enrichment_rerun_requested \
                                            THEN NULL ELSE enrichment_attempted_at END, \
             enrichment_retry_not_before = NULL, \
             enrichment_error = NULL, \
             enrichment_failures = '[]'::jsonb, \
             enrichment_rerun_requested = FALSE \
         WHERE id = $1 AND enrichment_status = 'in_progress' AND enrichment_claim_seq = $2",
        id,
        claim_seq,
    )
    .execute(executor)
    .await?;
    Ok(())
}

/// Failure bookkeeping, guarded on `in_progress` like [`mark_complete`]. A
/// rerun requested mid-run overrides the failure transition entirely: the
/// operator's edit resets the row to a fresh `pending` with no backoff, the
/// same shape an edit on an idle failed row produces, so the rerun is never
/// deferred behind a backoff window or swallowed by `skipped`.
async fn mark_failed(
    executor: impl sqlx::PgExecutor<'_>,
    id: Uuid,
    attempt_count: i32,
    claim_seq: i64,
    config: &Config,
    retry_after: Option<Duration>,
    error: Option<&str>,
    failures: &[FailureEntry],
) -> sqlx::Result<()> {
    let failures = serde_json::to_value(failures).map_err(|e| sqlx::Error::Encode(Box::new(e)))?;
    let max = config.enrichment.max_attempts.cast_signed();
    let next_status = if attempt_count >= max {
        EnrichmentStatus::Skipped
    } else {
        EnrichmentStatus::Failed
    };

    // A rate-limited row that can still retry waits out the provider's
    // Retry-After in `enrichment_retry_not_before`; a skipped row never retries.
    if let Some(ra) = retry_after.filter(|_| next_status == EnrichmentStatus::Failed) {
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
                                                THEN NULL ELSE enrichment_attempted_at END, \
                 enrichment_retry_not_before = CASE WHEN enrichment_rerun_requested \
                                                    THEN NULL \
                                                    ELSE now() + ($2 || ' seconds')::interval END, \
                 enrichment_error = CASE WHEN enrichment_rerun_requested \
                                         THEN NULL ELSE $3 END, \
                 enrichment_failures = CASE WHEN enrichment_rerun_requested \
                                            THEN '[]'::jsonb ELSE $4 END, \
                 enrichment_rerun_requested = FALSE \
             WHERE id = $5 AND enrichment_status = 'in_progress' AND enrichment_claim_seq = $6",
            next_status as EnrichmentStatus,
            secs_str,
            error,
            failures,
            id,
            claim_seq,
        )
        .execute(executor)
        .await?;
    } else {
        fail_claims(executor, &[id], &[claim_seq], config, error, &failures).await?;
    }

    // Backoff is applied inside the `claim_next` CTE via a CASE expression
    // that compares `attempt_count` against the row's last-attempt timestamp.
    // Nothing to compute here.
    Ok(())
}

async fn fail_claims(
    executor: impl sqlx::PgExecutor<'_>,
    ids: &[Uuid],
    claim_seqs: &[i64],
    config: &Config,
    error: Option<&str>,
    failures: &serde_json::Value,
) -> sqlx::Result<u64> {
    let max = config.enrichment.max_attempts.cast_signed();
    let rows = sqlx::query!(
        "UPDATE manifestations m \
         SET enrichment_status = CASE WHEN m.enrichment_rerun_requested \
                                      THEN 'pending'::enrichment_status \
                                      WHEN m.enrichment_attempt_count >= $3 \
                                      THEN 'skipped'::enrichment_status \
                                      ELSE 'failed'::enrichment_status END, \
             enrichment_attempt_count = CASE WHEN m.enrichment_rerun_requested \
                                             THEN 0 ELSE m.enrichment_attempt_count END, \
             enrichment_attempted_at = CASE WHEN m.enrichment_rerun_requested \
                                            THEN NULL ELSE m.enrichment_attempted_at END, \
             enrichment_retry_not_before = NULL, \
             enrichment_error = CASE WHEN m.enrichment_rerun_requested \
                                     THEN NULL ELSE $4 END, \
             enrichment_failures = CASE WHEN m.enrichment_rerun_requested \
                                        THEN '[]'::jsonb ELSE $5 END, \
             enrichment_rerun_requested = FALSE \
         FROM UNNEST($1::uuid[], $2::bigint[]) AS c(id, claim_seq) \
         WHERE m.id = c.id AND m.enrichment_status = 'in_progress' \
           AND m.enrichment_claim_seq = c.claim_seq",
        ids,
        claim_seqs,
        max,
        error,
        failures,
    )
    .execute(executor)
    .await?;
    Ok(rows.rows_affected())
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
mod tests {
    use super::*;
    use crate::test_support::db::ingestion_pool_for;

    // ── Task 35: queue integration tests ──────────────────────────────────
    //
    // Tests run against `reverie_ingestion` — the role with the
    // `manifestations_ingestion_full_access` RLS policy, which lets the
    // fixture INSERT queue rows with `RETURNING id`.  See the orchestrator
    // tests for the companion grant migration on `field_locks`.

    fn test_config_with_max_attempts(
        max_attempts: u32,
    ) -> (Config, crate::services::files::LibraryFiles) {
        use crate::config::{CoverConfig, EnrichmentConfig};
        use crate::models::manifestation_format::ManifestationFormat;

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
                enabled: true,
                concurrency: 2,
                poll_idle_secs: 30,
                fetch_budget_secs: 15,
                run_timeout_secs: 120,
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

        let path = format!("fixtures/queue-{marker}.epub");
        let hash = format!("queue-hash-{marker}");
        let manifestation_id = sqlx::query_scalar!(
            "WITH inserted AS (INSERT INTO manifestations \
               (library_id, work_id, format, file_path, ingestion_file_hash, current_file_hash, \
                file_size_bytes, ingestion_status, validation_status, \
                enrichment_status, enrichment_attempt_count, enrichment_attempted_at) \
             VALUES ((SELECT id FROM libraries WHERE configuration_key = 'default'), $1, 'epub'::manifestation_format, $2, $3, $3, 1000, \
                     'complete'::ingestion_status, 'clean'::validation_status, \
                     $4, $5, \
                     CASE WHEN $6::bigint IS NULL THEN NULL \
                          ELSE now() - ($6 || ' seconds')::interval END) \
             RETURNING *), claimed AS (INSERT INTO library_path_claims (library_id, path, manifestation_id) SELECT library_id, file_path, id FROM inserted) SELECT id AS \"id!\" FROM inserted",
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

    async fn stored_failures(pool: &PgPool, id: Uuid) -> serde_json::Value {
        sqlx::query_scalar!(
            "SELECT enrichment_failures AS \"enrichment_failures!\" FROM manifestations WHERE id = $1",
            id,
        )
        .fetch_one(pool)
        .await
        .unwrap()
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn enrichment_timeout_recovers_claim_and_continues(owner: PgPool) {
        // REV-REQ-0078, criteria 1-3.
        let pool = ingestion_pool_for(&owner).await;
        for (max_attempts, rerun) in [(3, false), (1, false), (1, true)] {
            let (mut config, _files) = test_config_with_max_attempts(max_attempts);
            config.enrichment.concurrency = 1;
            config.enrichment.run_timeout_secs = 1;
            config.enrichment.poll_idle_secs = 2;
            let (_, id, _) = insert_queue_fixture(&pool, EnrichmentStatus::Pending, 0, None).await;
            sqlx::query!(
                "UPDATE manifestations SET isbn_13 = '9780451524935' WHERE id = $1",
                id
            )
            .execute(&pool)
            .await
            .unwrap();
            let started = Arc::new(tokio::sync::Notify::new());
            let resume = Arc::new(tokio::sync::Notify::new());
            let expired = Arc::new(tokio::sync::Notify::new());
            let cancel = CancellationToken::new();
            let worker = tokio::spawn(queue_with_runner(pool.clone(), config, cancel.clone(), {
                let started = started.clone();
                let resume = resume.clone();
                let expired = expired.clone();
                move |pool, config, current, claim_seq, deadline| {
                    let started = started.clone();
                    let resume = resume.clone();
                    let expired = expired.clone();
                    async move {
                        if current == id {
                            started.notify_one();
                            resume.notified().await;
                        }
                        let result = orchestrator::run_queued_with_sources(
                            &pool,
                            &config,
                            &[],
                            current,
                            claim_seq,
                            deadline,
                        )
                        .await;
                        if current == id {
                            assert!(
                                result
                                    .as_ref()
                                    .unwrap_err()
                                    .is::<orchestrator::RunTimeout>()
                            );
                            expired.notify_one();
                        } else {
                            assert!(result.is_ok());
                        }
                        result
                    }
                }
            }));
            tokio::time::timeout(Duration::from_secs(5), started.notified())
                .await
                .unwrap();
            let mut blocker = owner.begin().await.unwrap();
            sqlx::query!("SELECT id FROM manifestations WHERE id = $1 FOR UPDATE", id)
                .fetch_one(&mut *blocker)
                .await
                .unwrap();
            if rerun {
                super::super::reset_retries(
                    &mut blocker,
                    &[id],
                    super::super::RetryTiming::ClearProviderDelay,
                )
                .await
                .unwrap();
            }
            resume.notify_one();
            tokio::time::timeout(Duration::from_secs(5), expired.notified())
                .await
                .unwrap();
            let active = sqlx::query_scalar!(r#"SELECT enrichment_status AS "status!: EnrichmentStatus" FROM manifestations WHERE id = $1"#, id)
                .fetch_one(&pool).await.unwrap();
            assert_eq!(active, EnrichmentStatus::InProgress);
            blocker.commit().await.unwrap();
            let state = tokio::time::timeout(Duration::from_secs(5), async {
                loop {
                    let row = sqlx::query!("SELECT enrichment_status::text AS status, enrichment_attempt_count, enrichment_attempted_at, enrichment_retry_not_before, enrichment_error, enrichment_failures, enrichment_rerun_requested FROM manifestations WHERE id = $1", id)
                        .fetch_one(&pool).await.unwrap();
                    if row.status.as_deref() != Some("in_progress") { break row; }
                    tokio::task::yield_now().await;
                }
            }).await.unwrap();
            assert_eq!(
                state.status.as_deref(),
                Some(if rerun {
                    "pending"
                } else if max_attempts == 1 {
                    "skipped"
                } else {
                    "failed"
                })
            );
            assert_eq!(state.enrichment_attempt_count, i32::from(!rerun));
            assert_eq!(state.enrichment_attempted_at.is_none(), rerun);
            assert!(state.enrichment_retry_not_before.is_none());
            assert!(!state.enrichment_rerun_requested);
            if rerun {
                assert!(state.enrichment_error.is_none());
                assert_eq!(state.enrichment_failures, serde_json::json!([]));
                let (claimed, attempt, claim_seq) = claim_next(&pool).await.unwrap().unwrap();
                assert_eq!((claimed, attempt), (id, 1));
                mark_complete(&pool, id, claim_seq).await.unwrap();
            } else {
                assert_eq!(
                    state.enrichment_error.as_deref(),
                    Some("enrichment run exceeded time budget")
                );
                assert_eq!(
                    state.enrichment_failures,
                    serde_json::json!([{"source": null, "class": "internal"}])
                );
                assert_eq!(claim_next(&pool).await.unwrap(), None);
            }
            let (_, next, _) =
                insert_queue_fixture(&pool, EnrichmentStatus::Pending, 0, None).await;
            tokio::time::timeout(Duration::from_secs(5), async {
                loop {
                    let state = sqlx::query_scalar!(r#"SELECT enrichment_status AS "status!: EnrichmentStatus" FROM manifestations WHERE id = $1"#, next)
                        .fetch_one(&pool).await.unwrap();
                    if state == EnrichmentStatus::Complete { break; }
                    tokio::task::yield_now().await;
                }
            }).await.unwrap();
            cancel.cancel();
            worker.await.unwrap().unwrap();
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn finish_waits_for_a_row_lock_held_past_the_run_budget(owner: PgPool) {
        let pool = ingestion_pool_for(&owner).await;
        let (mut config, _files) = test_config_with_max_attempts(3);
        config.enrichment.run_timeout_secs = 1;
        let (_, id, _) =
            insert_queue_fixture(&pool, EnrichmentStatus::InProgress, 1, Some(0)).await;
        let mut blocker = owner.begin().await.unwrap();
        sqlx::query!("SELECT id FROM manifestations WHERE id = $1 FOR UPDATE", id)
            .fetch_one(&mut *blocker)
            .await
            .unwrap();
        let outcome = RunOutcome {
            manifestation_id: id,
            applied: 0,
            staged: 0,
            skipped_locked: 0,
            source_failures: Vec::new(),
            duplicate_suspected: false,
        };
        let bookkeeping = tokio::spawn({
            let pool = pool.clone();
            async move { finish(&pool, &config, id, 1, 0, Ok(outcome)).await }
        });
        tokio::time::sleep(Duration::from_secs(2)).await;
        blocker.commit().await.unwrap();
        bookkeeping.await.unwrap().unwrap();
        let status = sqlx::query_scalar!(r#"SELECT enrichment_status AS "status!: EnrichmentStatus" FROM manifestations WHERE id = $1"#, id)
            .fetch_one(&pool).await.unwrap();
        assert_eq!(status, EnrichmentStatus::Complete);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn stale_claims_are_rescued_and_their_attempts_fenced(owner: PgPool) {
        // REV-REQ-0078, criteria 6-7.
        let pool = ingestion_pool_for(&owner).await;
        let (config, _files) = test_config_with_max_attempts(3);
        let (_, stale, _) =
            insert_queue_fixture(&pool, EnrichmentStatus::InProgress, 1, Some(300)).await;
        let (_, fresh, _) =
            insert_queue_fixture(&pool, EnrichmentStatus::InProgress, 1, Some(60)).await;
        let (_, open, _) =
            insert_queue_fixture(&pool, EnrichmentStatus::InProgress, 1, Some(300)).await;
        sqlx::query!(
            "UPDATE manifestations SET isbn_13 = '9780451524935' WHERE id = $1",
            stale
        )
        .execute(&pool)
        .await
        .unwrap();
        let mut catalogue = owner.begin().await.unwrap();
        sqlx::query!(
            "SELECT id FROM manifestations WHERE id = $1 FOR UPDATE",
            open
        )
        .fetch_one(&mut *catalogue)
        .await
        .unwrap();
        rescue_stale_claims(&pool, &config).await.unwrap();
        catalogue.rollback().await.unwrap();
        for (id, expected) in [
            (stale, EnrichmentStatus::Failed),
            (fresh, EnrichmentStatus::InProgress),
            (open, EnrichmentStatus::InProgress),
        ] {
            let status = sqlx::query_scalar!(r#"SELECT enrichment_status AS "status!: EnrichmentStatus" FROM manifestations WHERE id = $1"#, id)
                .fetch_one(&pool).await.unwrap();
            assert_eq!(status, expected);
        }
        assert_eq!(
            stored_failures(&pool, stale).await,
            serde_json::json!([{"source": null, "class": "internal"}])
        );

        let mut connection = pool.acquire().await.unwrap();
        super::super::reset_retries(
            &mut connection,
            &[stale],
            super::super::RetryTiming::ClearProviderDelay,
        )
        .await
        .unwrap();
        drop(connection);
        let (claimed, attempt, claim_seq) = claim_next(&pool).await.unwrap().unwrap();
        assert_eq!((claimed, attempt), (stale, 1));
        let deadline = Instant::now() + Duration::from_secs(5);
        let resumed =
            orchestrator::run_queued_with_sources(&pool, &config, &[], stale, 0, deadline).await;
        assert!(resumed.unwrap_err().is::<orchestrator::ClaimLost>());
        let outcome = RunOutcome {
            manifestation_id: stale,
            applied: 0,
            staged: 0,
            skipped_locked: 0,
            source_failures: Vec::new(),
            duplicate_suspected: false,
        };
        finish(&pool, &config, stale, 1, 0, Ok(outcome.clone()))
            .await
            .unwrap();
        let status = sqlx::query_scalar!(r#"SELECT enrichment_status AS "status!: EnrichmentStatus" FROM manifestations WHERE id = $1"#, stale)
            .fetch_one(&pool).await.unwrap();
        assert_eq!(status, EnrichmentStatus::InProgress);
        finish(&pool, &config, stale, 1, claim_seq, Ok(outcome))
            .await
            .unwrap();
        let status = sqlx::query_scalar!(r#"SELECT enrichment_status AS "status!: EnrichmentStatus" FROM manifestations WHERE id = $1"#, stale)
            .fetch_one(&pool).await.unwrap();
        assert_eq!(status, EnrichmentStatus::Complete);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn finish_records_a_run_error_as_an_internal_class_without_its_text(pool: PgPool) {
        let pool = ingestion_pool_for(&pool).await;
        let (config, _files) = test_config_with_max_attempts(5);
        let (_work_id, id, _) =
            insert_queue_fixture(&pool, EnrichmentStatus::InProgress, 1, Some(10)).await;

        let error = Err(anyhow::anyhow!("sqlx: password authentication failed"));
        finish(&pool, &config, id, 1, 0, error).await.unwrap();

        let stored = stored_failures(&pool, id).await;
        assert_eq!(
            stored,
            serde_json::json!([{"source": null, "class": "internal"}])
        );
        assert!(!stored.to_string().contains("password"));
    }
}
