//! Periodic reaper for expired sessions and OIDC sign-in attempts.
//!
//! THREAT: this sweep is availability hardening, not access control. Expired
//! rows are never authenticated against — `PostgresStore::load` already
//! filters `WHERE expiry_date > now()`, so a stale row can never resurrect a
//! session. Attempt expiry is enforced independently by the atomic claim.
//! The sweeps bound session and attempt table growth;
//! a missed or failed sweep degrades to unbounded table size, never to
//! session reuse.
//!
//! `PostgresStore` persists sessions to `tower_sessions.session` but never
//! deletes expired rows on its own — without a scheduled sweep they
//! accumulate until reaped manually. This worker drives the store's
//! [`ExpiredDeletion`](tower_sessions::session_store::ExpiredDeletion) trait
//! on a fixed cadence and drains cleanly when its
//! [`CancellationToken`](tokio_util::sync::CancellationToken) fires, sharing
//! the cooperative graceful-shutdown shape of the other background workers
//! (ingestion, enrichment, writeback, settings listener) rather than the
//! library's abort-based deletion task.

use sqlx::PgPool;
use tokio_util::sync::CancellationToken;
use tower_sessions::session_store::{self, ExpiredDeletion};

use crate::auth::store::PostgresStore;
use crate::models::sign_in;

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

/// Sweeps sessions and expired attempts every `SWEEP_INTERVAL` until cancellation.
///
/// Sweep failures are logged at `warn` and swallowed: a transient database
/// hiccup must not take the process down, and the next tick recovers. The
/// first interval tick fires immediately and is skipped so startup is not
/// bursty — the first real sweep lands one interval in.
pub async fn run_sweep(store: PostgresStore, pool: PgPool, cancel: CancellationToken) {
    let mut interval = tokio::time::interval(SWEEP_INTERVAL);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    interval.tick().await;

    loop {
        tokio::select! {
            biased;
            () = cancel.cancelled() => return,
            _ = interval.tick() => sweep_pass(&store, &pool).await,

        }
    }
}

async fn sweep_pass(store: &PostgresStore, pool: &PgPool) {
    match sweep_once(store).await {
        Ok(()) => tracing::info!("session sweep completed"),
        Err(e) => {
            tracing::warn!(error = %e, "session sweep failed; retrying next tick");
        }
    }
    loop {
        match sign_in::delete_expired_attempts(pool, 1000).await {
            Ok(deleted) if deleted < 1000 => break,
            Ok(_) => {}
            Err(e) => {
                tracing::warn!(error = %e, "sign-in attempt sweep failed; retrying next tick");
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, TimeDelta, Utc};
    use sqlx::PgPool;

    async fn session_count(pool: &PgPool) -> i64 {
        sqlx::query_scalar!(r#"SELECT count(*) AS "count!" FROM tower_sessions.session"#)
            .fetch_one(pool)
            .await
            .unwrap()
    }

    async fn insert_session(pool: &PgPool, id: &str, expiry: DateTime<Utc>) {
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
        let now = Utc::now();
        insert_session(&pool, "expired", now - TimeDelta::hours(1)).await;
        insert_session(&pool, "live", now + TimeDelta::hours(1)).await;
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

    #[sqlx::test(migrations = "./migrations")]
    async fn session_sweep_also_removes_expired_attempts(pool: PgPool) {
        let now = Utc::now();
        for id in ["expired", "live"] {
            insert_session(&pool, id, now + TimeDelta::hours(1)).await;
            sign_in::start_oidc_attempt(&pool, id, "state", "verifier", "nonce")
                .await
                .unwrap();
        }
        sqlx::query!("UPDATE oidc_sign_in_attempts SET expires_at = now() - interval '1 second' WHERE session_id = 'expired'")
            .execute(&pool).await.unwrap();
        let store = PostgresStore::new(pool.clone());
        sweep_pass(&store, &pool).await;
        assert_eq!(
            sqlx::query_scalar!("SELECT count(*) FROM oidc_sign_in_attempts")
                .fetch_one(&pool)
                .await
                .unwrap(),
            Some(1)
        );
        assert!(
            sign_in::claim_oidc_attempt(&pool, "live", "state")
                .await
                .unwrap()
                .is_some()
        );
        assert_eq!(session_count(&pool).await, 2);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn run_sweep_returns_promptly_on_cancel(pool: PgPool) {
        let store = PostgresStore::new(pool.clone());
        let cancel = CancellationToken::new();
        let handle = tokio::spawn(run_sweep(store, pool, cancel.clone()));

        cancel.cancel();

        tokio::time::timeout(std::time::Duration::from_secs(5), handle)
            .await
            .expect("run_sweep must observe cancellation and return, not hang until the next tick")
            .expect("run_sweep task must not panic");
    }
}
