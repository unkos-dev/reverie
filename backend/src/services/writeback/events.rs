//! Webhook terminal-event dispatch: stable event ids + Postgres-backed
//! dedupe (UNK-98).
//!
//! Step 12 owns the real delivery transport.  Until that lands, delivery is
//! a `tracing` emit so operators can observe writeback completion in logs;
//! upgrading to real webhook delivery is contained to the private `deliver`
//! function.
//!
//! `queue::finish` intentionally emits terminal events BEFORE the DB
//! bookkeeping `UPDATE` so a transient DB failure cannot silently drop the
//! event.  The cost is a double-dispatch risk: a DB failure on the `mark_*`
//! `UPDATE` followed by crash-recovery retry re-fires the same terminal
//! event.  [`dispatch`](crate::services::writeback::events::dispatch)
//! closes that gap by deduping on a stable event id within
//! [`DEDUPE_TTL_SECS`](crate::services::writeback::events::DEDUPE_TTL_SECS).
//! The dedupe set lives in Postgres
//! (`webhook_event_dedupe`) rather than process memory because the
//! duplicate arrives after a crash — an in-memory set would be empty
//! exactly when it is needed.

use sqlx::PgPool;
use uuid::Uuid;

/// Dedupe window in seconds (48 hours).
///
/// Must exceed the maximum claim back-off (24 hours, see
/// `queue::claim_next`): a crash-recovery re-fire only becomes eligible for
/// re-claim after the back-off window for the job's attempt count, so a
/// shorter TTL would expire before the duplicate arrives.
pub const DEDUPE_TTL_SECS: f64 = 48.0 * 3600.0;

/// Terminal outcome label — the stable component of an event id.
///
/// The same `(job_id, outcome)` tuple always produces the same event id, so
/// a re-fired terminal event collides with its original in the dedupe set
/// regardless of attempt count.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalOutcome {
    /// Writeback succeeded — delivered as `writeback_complete`.
    Complete,
    /// Job terminally skipped (unsupported format, vanished job row) —
    /// delivered as `writeback_failed`.
    Skipped,
    /// Attempt failed; the job may still retry — delivered as
    /// `writeback_failed`.
    Failed,
}

impl TerminalOutcome {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Complete => "complete",
            Self::Skipped => "skipped",
            Self::Failed => "failed",
        }
    }
}

/// Stable event id for a terminal writeback event:
/// `writeback:{job_id}:{outcome}`.
#[must_use]
pub fn event_id(job_id: Uuid, outcome: TerminalOutcome) -> String {
    format!("writeback:{job_id}:{}", outcome.as_str())
}

/// A terminal writeback event awaiting dispatch.
#[derive(Debug)]
pub struct TerminalEvent<'a> {
    /// `writeback_jobs.id` — keys the stable event id.
    pub job_id: Uuid,
    /// Terminal outcome label.
    pub outcome: TerminalOutcome,
    /// Target manifestation (sentinel `Uuid::nil()` when the job row
    /// vanished before lookup).
    pub manifestation_id: Uuid,
    /// Writeback reason (`metadata` / `cover`; `unknown` on the error path).
    pub reason: &'a str,
    /// Attempt count at dispatch time.  Payload detail only — deliberately
    /// NOT part of the event id, because a crash-recovery re-claim
    /// increments it and the re-fired event must still dedupe.
    pub attempt_count: i32,
    /// `current_file_hash` for [`TerminalOutcome::Complete`]; the error or
    /// skip reason otherwise.
    pub detail: &'a str,
}

/// Outcome of [`dispatch`].
#[derive(Debug, PartialEq, Eq)]
pub enum Dispatch {
    /// Event was delivered (and marked seen).
    Delivered,
    /// Event id was seen within the TTL window; delivery dropped.
    Deduped,
}

/// Dedupe-then-deliver a terminal writeback event.
///
/// Per UNK-98: if the event id was seen within [`DEDUPE_TTL_SECS`] the
/// event is dropped; otherwise it is delivered, then marked seen.  Repeated
/// per-attempt `failed` events for the same job within the TTL window
/// dedupe by design — downstream consumers get one notification per
/// `(job, outcome)`, not one per retry (per-attempt detail stays in
/// `writeback_jobs` and the worker logs).
///
/// Dedupe bookkeeping is fail-open: a dedupe-set read or write error is
/// logged and delivery proceeds, because the emit-before-bookkeeping
/// ordering exists precisely so a DB hiccup cannot silently drop an event.
pub async fn dispatch(pool: &PgPool, event: &TerminalEvent<'_>) -> Dispatch {
    let event_id = event_id(event.job_id, event.outcome);

    match sqlx::query_scalar!(
        "SELECT EXISTS(SELECT 1 FROM webhook_event_dedupe \
         WHERE event_id = $1 \
         AND seen_at > now() - ($2::double precision * interval '1 second')) \
         AS \"seen!\"",
        event_id,
        DEDUPE_TTL_SECS,
    )
    .fetch_one(pool)
    .await
    {
        Ok(true) => {
            tracing::debug!(
                event_id,
                "writeback: webhook event deduped (seen within TTL)"
            );
            return Dispatch::Deduped;
        }
        Ok(false) => {}
        Err(e) => {
            tracing::warn!(
                error = %e,
                event_id,
                "writeback: webhook dedupe check failed; delivering anyway"
            );
        }
    }

    deliver(&event_id, event);

    if let Err(e) = sqlx::query!(
        "INSERT INTO webhook_event_dedupe (event_id) VALUES ($1) \
         ON CONFLICT (event_id) DO UPDATE SET seen_at = now()",
        event_id,
    )
    .execute(pool)
    .await
    {
        tracing::warn!(
            error = %e,
            event_id,
            "writeback: webhook dedupe mark-seen failed; duplicate delivery possible on retry"
        );
    }

    // Opportunistic GC: entries past the TTL can never dedupe again, so
    // drop them here rather than carrying a scheduled purge job.
    if let Err(e) = sqlx::query!(
        "DELETE FROM webhook_event_dedupe \
         WHERE seen_at <= now() - ($1::double precision * interval '1 second')",
        DEDUPE_TTL_SECS,
    )
    .execute(pool)
    .await
    {
        tracing::warn!(error = %e, "writeback: webhook dedupe purge failed");
    }

    Dispatch::Delivered
}

/// Delivery stub.  Step 12's real dispatcher replaces the bodies; the
/// dedupe gate in [`dispatch`] stays in front of it.
fn deliver(event_id: &str, event: &TerminalEvent<'_>) {
    match event.outcome {
        TerminalOutcome::Complete => {
            tracing::info!(
                event = "writeback_complete",
                event_id,
                manifestation_id = %event.manifestation_id,
                reason = event.reason,
                attempt_count = event.attempt_count,
                current_file_hash = event.detail,
                "writeback: complete"
            );
        }
        // Logs at `warn!` because a writeback failure is an
        // operator-relevant anomaly — an operator filtering by severity
        // should see these, but should not see every successful writeback.
        TerminalOutcome::Skipped | TerminalOutcome::Failed => {
            tracing::warn!(
                event = "writeback_failed",
                event_id,
                manifestation_id = %event.manifestation_id,
                reason = event.reason,
                attempt_count = event.attempt_count,
                error = event.detail,
                "writeback: failed"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::db::app_pool_for;

    fn sample_event(job_id: Uuid, outcome: TerminalOutcome) -> TerminalEvent<'static> {
        TerminalEvent {
            job_id,
            outcome,
            manifestation_id: Uuid::nil(),
            reason: "metadata",
            attempt_count: 1,
            detail: "abc123",
        }
    }

    async fn dedupe_rows_for(pool: &PgPool, event_id: &str) -> i64 {
        sqlx::query_scalar!(
            "SELECT count(*) AS \"n!\" FROM webhook_event_dedupe WHERE event_id = $1",
            event_id,
        )
        .fetch_one(pool)
        .await
        .unwrap()
    }

    #[test]
    fn event_id_is_stable_per_job_and_outcome() {
        let job_id = Uuid::new_v4();
        let id = event_id(job_id, TerminalOutcome::Complete);
        assert_eq!(id, format!("writeback:{job_id}:complete"));
        assert_eq!(id, event_id(job_id, TerminalOutcome::Complete));
    }

    #[test]
    fn event_id_distinguishes_outcomes() {
        let job_id = Uuid::new_v4();
        let ids = [
            event_id(job_id, TerminalOutcome::Complete),
            event_id(job_id, TerminalOutcome::Skipped),
            event_id(job_id, TerminalOutcome::Failed),
        ];
        assert_eq!(ids[0], format!("writeback:{job_id}:complete"));
        assert_eq!(ids[1], format!("writeback:{job_id}:skipped"));
        assert_eq!(ids[2], format!("writeback:{job_id}:failed"));
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn dispatch_delivers_once_then_dedupes(pool: PgPool) {
        let app_pool = app_pool_for(&pool).await;
        let job_id = Uuid::new_v4();
        let event = sample_event(job_id, TerminalOutcome::Complete);

        assert_eq!(dispatch(&app_pool, &event).await, Dispatch::Delivered);
        assert_eq!(dispatch(&app_pool, &event).await, Dispatch::Deduped);

        let eid = event_id(job_id, TerminalOutcome::Complete);
        assert_eq!(dedupe_rows_for(&pool, &eid).await, 1);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn dispatch_dedupes_across_attempt_counts(pool: PgPool) {
        let app_pool = app_pool_for(&pool).await;
        let job_id = Uuid::new_v4();

        let first = sample_event(job_id, TerminalOutcome::Failed);
        assert_eq!(dispatch(&app_pool, &first).await, Dispatch::Delivered);

        // Crash-recovery re-claim increments attempt_count; the re-fired
        // event must still collide with the original.
        let refire = TerminalEvent {
            attempt_count: 2,
            ..sample_event(job_id, TerminalOutcome::Failed)
        };
        assert_eq!(dispatch(&app_pool, &refire).await, Dispatch::Deduped);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn dispatch_distinct_outcomes_not_deduped_against_each_other(pool: PgPool) {
        let app_pool = app_pool_for(&pool).await;
        let job_id = Uuid::new_v4();

        assert_eq!(
            dispatch(&app_pool, &sample_event(job_id, TerminalOutcome::Failed)).await,
            Dispatch::Delivered
        );
        assert_eq!(
            dispatch(&app_pool, &sample_event(job_id, TerminalOutcome::Skipped)).await,
            Dispatch::Delivered
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn dispatch_redelivers_after_ttl_expiry(pool: PgPool) {
        let app_pool = app_pool_for(&pool).await;
        let job_id = Uuid::new_v4();
        let event = sample_event(job_id, TerminalOutcome::Complete);
        let eid = event_id(job_id, TerminalOutcome::Complete);

        assert_eq!(dispatch(&app_pool, &event).await, Dispatch::Delivered);

        // Age the entry past the TTL window.
        sqlx::query!(
            "UPDATE webhook_event_dedupe \
             SET seen_at = now() - ($1::double precision * interval '1 second') \
             WHERE event_id = $2",
            DEDUPE_TTL_SECS + 3600.0,
            eid,
        )
        .execute(&pool)
        .await
        .unwrap();

        assert_eq!(dispatch(&app_pool, &event).await, Dispatch::Delivered);
        // Mark-seen refreshed the entry, so the next dispatch dedupes again.
        assert_eq!(dispatch(&app_pool, &event).await, Dispatch::Deduped);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn dispatch_purges_expired_entries(pool: PgPool) {
        let app_pool = app_pool_for(&pool).await;
        let stale_eid = format!("writeback:{}:complete", Uuid::new_v4());
        sqlx::query!(
            "INSERT INTO webhook_event_dedupe (event_id, seen_at) \
             VALUES ($1, now() - ($2::double precision * interval '1 second'))",
            stale_eid,
            DEDUPE_TTL_SECS + 3600.0,
        )
        .execute(&pool)
        .await
        .unwrap();

        let job_id = Uuid::new_v4();
        assert_eq!(
            dispatch(&app_pool, &sample_event(job_id, TerminalOutcome::Complete)).await,
            Dispatch::Delivered
        );

        assert_eq!(dedupe_rows_for(&pool, &stale_eid).await, 0);
        let fresh_eid = event_id(job_id, TerminalOutcome::Complete);
        assert_eq!(dedupe_rows_for(&pool, &fresh_eid).await, 1);
    }
}
