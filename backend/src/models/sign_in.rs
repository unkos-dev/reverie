//! One-shot sign-in attempts and single-winner cohorts.
//!
//! # Tier 2: security-critical
//!
//! THREAT: atomic consumption prevents callback replay across replicas.
//! THREAT: only a cohort winner may replace an anonymous session with an identity.

use sha2::{Digest, Sha256};
use sqlx::PgPool;

/// Whether a browser's sign-in cohort can still acquire a winner.
#[derive(Debug, PartialEq, Eq)]
pub enum CohortState {
    /// No sign-in has won.
    Pending,
    /// A sign-in has acquired the cohort.
    Won,
}

/// Provider exchange material consumed by one callback; never logged or serialised.
pub struct ClaimedAttempt {
    /// Secret proving possession of the authorization request.
    pub pkce_verifier: String,
    /// Nonce binding the ID token to this attempt.
    pub nonce: String,
}

/// Join the session's cohort without reopening a winner.
///
/// # Errors
/// Returns database errors from inserting or reading the cohort.
pub async fn join_pending(pool: &PgPool, session_id: &str) -> Result<CohortState, sqlx::Error> {
    sqlx::query!(
        "INSERT INTO sign_in_cohorts (session_id) VALUES ($1) ON CONFLICT (session_id) DO NOTHING",
        session_id
    )
    .execute(pool)
    .await?;
    let won_at = sqlx::query_scalar!(
        "SELECT won_at FROM sign_in_cohorts WHERE session_id = $1",
        session_id
    )
    .fetch_one(pool)
    .await?;
    Ok(if won_at.is_some() {
        CohortState::Won
    } else {
        CohortState::Pending
    })
}

/// Replace the session's pending OIDC attempt with a fixed fifteen-minute lifetime.
///
/// # Errors
/// Returns database errors; the cohort and attempt commit together.
pub async fn start_oidc_attempt(
    pool: &PgPool,
    session_id: &str,
    state: &str,
    pkce_verifier: &str,
    nonce: &str,
) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query!(
        "INSERT INTO sign_in_cohorts (session_id) VALUES ($1) ON CONFLICT (session_id) DO NOTHING",
        session_id
    )
    .execute(&mut *tx)
    .await?;
    let digest = Sha256::digest(state.as_bytes());
    sqlx::query!("INSERT INTO oidc_sign_in_attempts (session_id, state_digest, pkce_verifier, nonce, expires_at) VALUES ($1, $2, $3, $4, now() + interval '15 minutes') ON CONFLICT (session_id) DO UPDATE SET state_digest = EXCLUDED.state_digest, pkce_verifier = EXCLUDED.pkce_verifier, nonce = EXCLUDED.nonce, expires_at = EXCLUDED.expires_at", session_id, digest.as_slice(), pkce_verifier, nonce)
        .execute(&mut *tx).await?;
    tx.commit().await
}

/// Consume matching live state before any provider request.
///
/// # Errors
/// Returns database errors from the atomic delete.
pub async fn claim_oidc_attempt(
    pool: &PgPool,
    session_id: &str,
    state: &str,
) -> Result<Option<ClaimedAttempt>, sqlx::Error> {
    let digest = Sha256::digest(state.as_bytes());
    let row = sqlx::query!("DELETE FROM oidc_sign_in_attempts WHERE session_id = $1 AND state_digest = $2 AND expires_at > now() RETURNING pkce_verifier, nonce", session_id, digest.as_slice())
        .fetch_optional(pool).await?;
    Ok(row.map(|row| ClaimedAttempt {
        pkce_verifier: row.pkce_verifier,
        nonce: row.nonce,
    }))
}

/// Acquire the sole authority to establish an authenticated session.
///
/// # Errors
/// Returns database errors from the conditional update.
pub async fn claim_win(pool: &PgPool, session_id: &str) -> Result<bool, sqlx::Error> {
    Ok(sqlx::query!(
        "UPDATE sign_in_cohorts SET won_at = now() WHERE session_id = $1 AND won_at IS NULL",
        session_id
    )
    .execute(pool)
    .await?
    .rows_affected()
        == 1)
}

/// Remove at most `batch` expired attempts, retaining live attempts.
///
/// # Errors
/// Returns database errors from the bounded delete.
pub async fn delete_expired_attempts(pool: &PgPool, batch: i64) -> Result<u64, sqlx::Error> {
    Ok(sqlx::query!("DELETE FROM oidc_sign_in_attempts WHERE session_id IN (SELECT session_id FROM oidc_sign_in_attempts WHERE expires_at <= now() ORDER BY expires_at LIMIT $1)", batch)
        .execute(pool).await?.rows_affected())
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn start(pool: &PgPool, id: &str, state: &str) {
        sqlx::query!("INSERT INTO tower_sessions.session (id, data, expiry_date) VALUES ($1, $2, now() + interval '1 hour') ON CONFLICT (id) DO NOTHING", id, &b"payload"[..])
            .execute(pool).await.unwrap();
        start_oidc_attempt(pool, id, state, "verifier", "nonce")
            .await
            .unwrap();
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn sign_in_attempt_claim_is_one_shot(pool: PgPool) {
        start(&pool, "one", "state").await;
        let claimed = claim_oidc_attempt(&pool, "one", "state")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(claimed.pkce_verifier, "verifier");
        assert_eq!(claimed.nonce, "nonce");
        assert!(
            claim_oidc_attempt(&pool, "one", "state")
                .await
                .unwrap()
                .is_none()
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn sign_in_attempt_claim_rejects_expired(pool: PgPool) {
        start(&pool, "one", "state").await;
        sqlx::query!("UPDATE oidc_sign_in_attempts SET expires_at = now() - interval '1 second'")
            .execute(&pool)
            .await
            .unwrap();
        assert!(
            claim_oidc_attempt(&pool, "one", "state")
                .await
                .unwrap()
                .is_none()
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn sign_in_attempt_claim_rejects_wrong_state_or_session(pool: PgPool) {
        start(&pool, "one", "state").await;
        assert!(
            claim_oidc_attempt(&pool, "one", "wrong")
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            claim_oidc_attempt(&pool, "other", "state")
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            claim_oidc_attempt(&pool, "one", "state")
                .await
                .unwrap()
                .is_some()
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn sign_in_attempt_start_replaces_previous(pool: PgPool) {
        start(&pool, "one", "first").await;
        start_oidc_attempt(&pool, "one", "second", "replacement", "new-nonce")
            .await
            .unwrap();
        assert!(
            claim_oidc_attempt(&pool, "one", "first")
                .await
                .unwrap()
                .is_none()
        );
        let attempt = claim_oidc_attempt(&pool, "one", "second")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(attempt.pkce_verifier, "replacement");
        assert_eq!(attempt.nonce, "new-nonce");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn sign_in_cohort_win_is_exclusive(pool: PgPool) {
        start(&pool, "one", "state").await;
        assert_eq!(
            join_pending(&pool, "one").await.unwrap(),
            CohortState::Pending
        );
        let (first, second) = tokio::join!(claim_win(&pool, "one"), claim_win(&pool, "one"));
        assert_ne!(first.unwrap(), second.unwrap());
        assert_eq!(join_pending(&pool, "one").await.unwrap(), CohortState::Won);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn sign_in_cohort_rows_cascade_with_session(pool: PgPool) {
        start(&pool, "one", "state").await;
        sqlx::query!("DELETE FROM tower_sessions.session WHERE id = 'one'")
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(
            sqlx::query_scalar!("SELECT count(*) FROM sign_in_cohorts")
                .fetch_one(&pool)
                .await
                .unwrap(),
            Some(0)
        );
        assert!(
            claim_oidc_attempt(&pool, "one", "state")
                .await
                .unwrap()
                .is_none()
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn sign_in_attempt_sweep_deletes_only_expired_in_batches(pool: PgPool) {
        for id in ["one", "two", "three", "live"] {
            start(&pool, id, "state").await;
        }
        sqlx::query!("UPDATE oidc_sign_in_attempts SET expires_at = now() - interval '1 second' WHERE session_id <> 'live'").execute(&pool).await.unwrap();
        assert_eq!(delete_expired_attempts(&pool, 2).await.unwrap(), 2);
        assert_eq!(delete_expired_attempts(&pool, 2).await.unwrap(), 1);
        assert!(
            claim_oidc_attempt(&pool, "live", "state")
                .await
                .unwrap()
                .is_some()
        );
    }
}
