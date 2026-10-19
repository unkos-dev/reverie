//! Hourly reclamation for sessions, sign-in attempts and password-admission state.
//!
//! THREAT: requests enforce expiry independently; cleanup skips locked account rows and retains active protection.
//! Reclamation is eventual, not a storage bound under an unbounded arrival rate.

use std::sync::Arc;

use crate::auth::rate_limit::LoginLimiter;
use crate::models::login_throttle;
use sqlx::PgPool;
use tokio_util::sync::CancellationToken;
use tower_sessions::session_store::{self, ExpiredDeletion};

use crate::auth::store::PostgresStore;
use crate::models::sign_in;

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

/// Reclaims authentication state every `SWEEP_INTERVAL` until cancellation.
///
/// Sweep failures are logged at `warn` and swallowed: a transient database
/// hiccup must not take the process down, and the next tick recovers. The
/// first interval tick fires immediately and is skipped so startup is not
/// bursty — the first real sweep lands one interval in.
pub async fn run_sweep(
    store: PostgresStore,
    pool: PgPool,
    limiters: [Arc<LoginLimiter>; 3],
    cancel: CancellationToken,
) {
    let mut interval = tokio::time::interval(SWEEP_INTERVAL);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    interval.tick().await;

    loop {
        tokio::select! {
            biased;
            () = cancel.cancelled() => return,
            _ = interval.tick() => sweep_pass(&store, &pool, &limiters, &cancel).await,

        }
    }
}

async fn sweep_pass(
    store: &PostgresStore,
    pool: &PgPool,
    limiters: &[Arc<LoginLimiter>; 3],
    cancel: &CancellationToken,
) {
    for limiter in limiters {
        limiter.retain_recent();
    }
    match sweep_once(store).await {
        Ok(()) => tracing::info!("session sweep completed"),
        Err(e) => {
            tracing::warn!(error = %e, "session sweep failed; retrying next tick");
        }
    }
    while !cancel.is_cancelled() {
        match sign_in::delete_expired_attempts(pool, 1000).await {
            Ok(deleted) if deleted < 1000 => break,
            Ok(_) => {}
            Err(e) => {
                tracing::warn!(error = %e, "sign-in attempt sweep failed; retrying next tick");
                break;
            }
        }
    }
    sweep_throttles(pool, cancel).await;
}

async fn sweep_throttles(pool: &PgPool, cancel: &CancellationToken) {
    while !cancel.is_cancelled() {
        match login_throttle::delete_inactive(pool, 1000).await {
            Ok(deleted) if deleted < 1000 => break,
            Ok(_) => {}
            Err(e) => {
                tracing::warn!(error = %e, "password throttle sweep failed; retrying next tick");
                break;
            }
        }
        #[cfg(test)]
        if let Ok(cancel) = tests::CANCEL_AFTER_BATCH.try_with(Clone::clone) {
            cancel.cancel();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, TimeDelta, Utc};
    use sqlx::PgPool;
    use tracing::instrument::WithSubscriber;

    tokio::task_local! {
        pub(super) static CANCEL_AFTER_BATCH: CancellationToken;
    }

    fn test_limiters() -> [Arc<LoginLimiter>; 3] {
        std::array::from_fn(|_| crate::test_support::test_login_limiter())
    }

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
    async fn throttle_reclamation_worker_cancels_between_batches(pool: PgPool) {
        sqlx::query!("INSERT INTO local_login_throttle (email_lower) SELECT 'cancel-' || number FROM generate_series(1, 1005) AS number")
            .execute(&pool).await.unwrap();
        crate::models::login_throttle::tests::seed(&pool, "protected", &[Utc::now()], None).await;
        let cancel = CancellationToken::new();
        CANCEL_AFTER_BATCH
            .scope(cancel.clone(), sweep_throttles(&pool, &cancel))
            .await;
        assert!(cancel.is_cancelled());
        assert_eq!(
            sqlx::query_scalar!("SELECT count(*) FROM local_login_throttle")
                .fetch_one(&pool)
                .await
                .unwrap(),
            Some(6)
        );
        sweep_throttles(&pool, &cancel).await;
        assert_eq!(
            sqlx::query_scalar!("SELECT count(*) FROM local_login_throttle")
                .fetch_one(&pool)
                .await
                .unwrap(),
            Some(6)
        );
        sweep_throttles(&pool, &CancellationToken::new()).await;
        assert_eq!(
            sqlx::query_scalar!("SELECT count(*) FROM local_login_throttle")
                .fetch_one(&pool)
                .await
                .unwrap(),
            Some(1)
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn throttle_reclamation_worker_recovers_after_failure_and_always_reclaims_governors(
        pool: PgPool,
    ) {
        let now = Utc::now();
        crate::models::login_throttle::tests::seed(&pool, "eligible", &[], None).await;
        crate::models::login_throttle::tests::seed(
            &pool,
            "protected",
            &[now],
            Some(now + TimeDelta::minutes(15)),
        )
        .await;
        let readonly = crate::test_support::db::readonly_pool_for(&pool).await;
        let store = PostgresStore::new(readonly.clone());
        let limiters: [Arc<LoginLimiter>; 3] = std::array::from_fn(|_| {
            Arc::new(governor::RateLimiter::keyed(
                governor::Quota::with_period(std::time::Duration::from_nanos(1)).unwrap(),
            ))
        });
        let ip = "192.0.2.1".parse().unwrap();
        for limiter in &limiters {
            limiter.check_key(&ip).unwrap();
            assert_eq!(limiter.len(), 1);
        }
        let logs = crate::test_support::CapturedLogs::default();
        let writer = logs.clone();
        let subscriber = tracing_subscriber::fmt()
            .with_writer(move || writer.clone())
            .without_time()
            .finish();
        sweep_pass(&store, &readonly, &limiters, &CancellationToken::new())
            .with_subscriber(subscriber)
            .await;
        assert!(logs.text().contains("password throttle sweep failed"));
        for limiter in &limiters {
            assert!(limiter.is_empty());
        }
        assert_eq!(
            sqlx::query_scalar!("SELECT count(*) FROM local_login_throttle")
                .fetch_one(&pool)
                .await
                .unwrap(),
            Some(2)
        );
        let app_pool = crate::test_support::db::app_pool_for(&pool).await;
        sweep_pass(
            &PostgresStore::new(app_pool.clone()),
            &app_pool,
            &limiters,
            &CancellationToken::new(),
        )
        .await;
        assert_eq!(
            sqlx::query_scalar!("SELECT count(*) FROM local_login_throttle")
                .fetch_one(&pool)
                .await
                .unwrap(),
            Some(1)
        );
        assert!(matches!(
            login_throttle::admit(&app_pool, "protected").await.unwrap(),
            login_throttle::Admission::Blocked(_)
        ));
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
        sweep_pass(&store, &pool, &test_limiters(), &CancellationToken::new()).await;
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
        let handle = tokio::spawn(run_sweep(store, pool, test_limiters(), cancel.clone()));

        cancel.cancel();

        tokio::time::timeout(std::time::Duration::from_secs(5), handle)
            .await
            .expect("run_sweep must observe cancellation and return, not hang until the next tick")
            .expect("run_sweep task must not panic");
    }
}
