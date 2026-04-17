//! Background enrichment queue worker.
//!
//! Claims manifestations from the `manifestations` table using an atomic
//! `FOR UPDATE SKIP LOCKED` CTE so multiple workers can race without double
//! processing.  Applies an exponential-ish retry backoff and marks rows as
//! `skipped` after `max_attempts`.  On shutdown, reverts any `in_progress`
//! rows back to `pending` so a fresh worker can re-claim them.

#![allow(dead_code)] // wired via main.rs in Phase C Task 28

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

/// Retry backoff schedule (attempts are 1-indexed).  Keep in sync with the
/// plan's Task 22 design; values are in minutes.
const BACKOFF_MINUTES: [i64; 10] = [
    5,    // attempt 1
    30,   // attempt 2
    120,  // attempt 3
    480,  // attempt 4
    1440, // attempt 5
    1440, // attempt 6
    1440, // attempt 7
    1440, // attempt 8
    1440, // attempt 9
    1440, // attempt 10
];

fn backoff(attempt_count: i64) -> time::Duration {
    // `attempt_count` is already incremented by the CTE when we claim; use
    // (attempt_count - 1) as the index for computing the *next* delay.
    let idx = attempt_count
        .saturating_sub(1)
        .clamp(0, (BACKOFF_MINUTES.len() - 1) as i64);
    time::Duration::minutes(BACKOFF_MINUTES[idx as usize])
}

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

    let _ = backoff(i64::from(attempt_count));
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

    #[test]
    fn backoff_schedule_monotonic() {
        // 5m, 30m, 2h, 8h, then 24h for every subsequent attempt.
        assert_eq!(backoff(1), time::Duration::minutes(5));
        assert_eq!(backoff(2), time::Duration::minutes(30));
        assert_eq!(backoff(3), time::Duration::minutes(120));
        assert_eq!(backoff(4), time::Duration::minutes(480));
        assert_eq!(backoff(5), time::Duration::minutes(1440));
        assert_eq!(backoff(999), time::Duration::minutes(1440));
    }
}
