//! Periodic reaper for expired `tower_sessions` rows.
//!
//! THREAT: this sweep is availability hardening, not access control. Expired
//! rows are never authenticated against — `PostgresStore::load` already
//! filters `WHERE expiry_date > now()`, so a stale row can never resurrect a
//! session. The sweep exists solely to bound `tower_sessions.session` growth;
//! a missed or failed sweep degrades to unbounded table size, never to
//! session reuse.
//!
//! `PostgresStore` persists sessions to `tower_sessions.session` but never
//! deletes expired rows on its own — without a scheduled sweep they
//! accumulate until reaped manually. This worker drives the store's
//! [`ExpiredDeletion`] trait on a fixed cadence and drains cleanly when its
//! [`CancellationToken`] fires, sharing the cooperative graceful-shutdown
//! shape of the other background workers (ingestion, enrichment, writeback,
//! settings listener) rather than the library's abort-based deletion task.

use tokio_util::sync::CancellationToken;
use tower_sessions::session_store::{self, ExpiredDeletion};
use tower_sessions_sqlx_store::PostgresStore;

/// Sweep cadence.
///
/// Sessions expire on 24h inactivity (`Expiry::OnInactivity` in
/// `build_router_with_session_store`), so hourly reaping keeps the table
/// bounded at negligible cost. Hardcoded rather than config-driven: a tunable
/// cadence is overkill for a single-instance v0.x deployment — lift to config
/// when a second deployment shape needs it.
const SWEEP_INTERVAL: std::time::Duration = std::time::Duration::from_hours(1);

/// Deletes all expired session rows once, via the store's [`ExpiredDeletion`]
/// trait.
///
/// # Errors
///
/// Returns the store's [`session_store::Error`] if the underlying
/// `DELETE … WHERE expiry_date < now()` fails. [`run_sweep`] treats this as
/// non-fatal; direct callers (tests) may handle it as they wish.
pub async fn sweep_once(store: &PostgresStore) -> Result<(), session_store::Error> {
    store.delete_expired().await
}

/// Runs [`sweep_once`] every [`SWEEP_INTERVAL`] until `cancel` is triggered.
///
/// Sweep failures are logged at `warn` and swallowed: a transient database
/// hiccup must not take the process down, and the next tick recovers. The
/// first interval tick fires immediately and is skipped so startup is not
/// bursty — the first real sweep lands one interval in.
pub async fn run_sweep(store: PostgresStore, cancel: CancellationToken) {
    let mut interval = tokio::time::interval(SWEEP_INTERVAL);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    interval.tick().await;

    loop {
        tokio::select! {
            biased;
            () = cancel.cancelled() => return,
            _ = interval.tick() => match sweep_once(&store).await {
                Ok(()) => tracing::info!("session sweep completed"),
                Err(e) => {
                    tracing::warn!(error = %e, "session sweep failed; retrying next tick");
                }
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::PgPool;
    use time::{Duration, OffsetDateTime};

    async fn session_count(pool: &PgPool) -> i64 {
        sqlx::query_scalar!(r#"SELECT count(*) AS "count!" FROM tower_sessions.session"#)
            .fetch_one(pool)
            .await
            .unwrap()
    }

    async fn insert_session(pool: &PgPool, id: &str, expiry: OffsetDateTime) {
        sqlx::query!(
            "INSERT INTO tower_sessions.session (id, data, expiry_date) VALUES ($1, $2, $3)",
            id,
            &b"payload"[..],
            expiry,
        )
        .execute(pool)
        .await
        .unwrap();
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn sweep_deletes_expired_and_keeps_live(pool: PgPool) {
        let now = OffsetDateTime::now_utc();
        insert_session(&pool, "expired", now - Duration::hours(1)).await;
        insert_session(&pool, "live", now + Duration::hours(1)).await;
        assert_eq!(session_count(&pool).await, 2);

        let store = PostgresStore::new(pool.clone());
        sweep_once(&store).await.expect("sweep should succeed");

        assert_eq!(
            session_count(&pool).await,
            1,
            "expired row reaped, live row retained"
        );
        let live_remaining = sqlx::query_scalar!(
            r#"SELECT count(*) AS "count!" FROM tower_sessions.session WHERE id = 'live'"#
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(live_remaining, 1);
    }
}
