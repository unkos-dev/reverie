//! Webhook terminal-event dispatch: stable event ids + Postgres-backed
//! dedupe.
//!
//! A real delivery transport is not yet implemented; delivery is
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
    /// `writeback_failed`.  Within the TTL window a re-fired `failed`
    /// event for the same job dedupes — consumers get at most one
    /// `writeback_failed` per job per window, not one per retry; see
    /// [`dispatch`] for the rationale.
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
    /// Stored reason (`metadata`, `cover` or `relocation`); `unknown` only when lookup fails.
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
/// If the event id was seen within [`DEDUPE_TTL_SECS`] the
/// event is dropped; otherwise it is delivered, then marked seen.  Repeated
/// per-attempt `failed` events for the same job within the TTL window
/// dedupe by design — downstream consumers get one notification per
/// `(job, outcome)`, not one per retry (per-attempt detail stays in
/// `writeback_jobs` and the worker logs).
///
/// Dedupe bookkeeping is fail-open: a dedupe-set read or write error is
/// logged and delivery proceeds, because the emit-before-bookkeeping
/// ordering exists precisely so a DB hiccup cannot silently drop an event.
///
/// Concurrency: the check → deliver → mark sequence is not atomic, but two
/// concurrent dispatches of the same event id are structurally impossible —
/// an event id is keyed by `(job_id, outcome)`, a job only reaches `finish`
/// while it holds the `in_progress` slot, and the partial UNIQUE index
/// `idx_writeback_jobs_in_progress_unique` serialises that slot across
/// workers and replicas.  Re-fires are therefore always sequential, which
/// is the case this set dedupes.
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
    // drop them here rather than carrying a scheduled purge job.  Bounded
    // per the no-unbounded-queries invariant: steady-state expiry rate
    // equals the delivery rate, so a 100-row cap per delivered event keeps
    // up and caps statement cost even after a backlog.
    if let Err(e) = sqlx::query!(
        "DELETE FROM webhook_event_dedupe \
         WHERE event_id IN (SELECT event_id FROM webhook_event_dedupe \
         WHERE seen_at <= now() - ($1::double precision * interval '1 second') \
         LIMIT 100)",
        DEDUPE_TTL_SECS,
    )
    .execute(pool)
    .await
    {
        tracing::warn!(error = %e, "writeback: webhook dedupe purge failed");
    }

    Dispatch::Delivered
}

/// Delivery stub.  A real dispatcher will replace the bodies; the
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
