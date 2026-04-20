//! Webhook event emission stubs.
//!
//! Step 12 owns the real dispatcher.  Until that lands, we emit at
//! `tracing::info!` so operators can observe writeback completion in logs.
//! Upgrading to real webhook delivery is a one-line change inside the two
//! emitters below.

use uuid::Uuid;

/// Terminal-success event payload.  See BLUEPRINT Step 8 §Events.
pub fn emit_writeback_complete(
    manifestation_id: Uuid,
    reason: &str,
    attempt_count: i32,
    current_file_hash: &str,
) {
    tracing::info!(
        event = "writeback_complete",
        %manifestation_id,
        reason,
        attempt_count,
        current_file_hash,
        "writeback: complete"
    );
}

/// Terminal-failure event payload.
///
/// Logs at `warn!` because a writeback failure is an operator-relevant
/// anomaly — an operator filtering by severity should see these, but
/// should not see every successful writeback.
pub fn emit_writeback_failed(
    manifestation_id: Uuid,
    reason: &str,
    attempt_count: i32,
    error: &str,
) {
    tracing::warn!(
        event = "writeback_failed",
        %manifestation_id,
        reason,
        attempt_count,
        error,
        "writeback: failed"
    );
}
