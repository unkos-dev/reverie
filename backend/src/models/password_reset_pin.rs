//! Password-reset PIN records: hashed, single-use, short-lived recovery tokens.
//!
//! # Tier 2 — security-critical
//!
//! The clear PIN is never stored here; only its Argon2id hash, an expiry, and a
//! consumed marker. A row is single-use (`consumed_at`) and short-lived
//! (`expires_at`). At most one row stays active per user: a new request
//! supersedes prior unconsumed rows. The struct deliberately does not derive
//! `Serialize` so the hash cannot leak through an API by accident.

use sqlx::PgPool;
use time::OffsetDateTime;
use uuid::Uuid;

/// An active (unconsumed, unexpired) password-reset PIN, as needed to verify a
/// reset attempt. Holds a SECRET (`pin_hash`); not serialisable, never logged.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PasswordResetPin {
    /// Primary key, used to [`consume`] the row after a successful verify.
    pub id: Uuid,
    /// Owning user.
    pub user_id: Uuid,
    /// Argon2id PHC of the clear PIN.
    pub pin_hash: String,
    /// When this PIN stops being valid.
    pub expires_at: OffsetDateTime,
}

/// Delete any unconsumed PIN rows for a user so at most one stays live. Call
/// before [`insert`] on each new forgot-password request: a re-request
/// invalidates the prior PIN (codeguard #2: at most one active PIN per user).
///
/// # Errors
///
/// Returns [`sqlx::Error`] from the `DELETE`.
#[allow(dead_code)] // Consumed by the forgot-password route in this PR
pub async fn supersede_active(pool: &PgPool, user_id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "DELETE FROM password_reset_pins WHERE user_id = $1 AND consumed_at IS NULL",
        user_id,
    )
    .execute(pool)
    .await
    .map(|_| ())
}

/// Insert a new PIN row (hash only) and return its id.
///
/// # Errors
///
/// Returns [`sqlx::Error`] from the `INSERT`.
#[allow(dead_code)] // Consumed by the forgot-password route in this PR
pub async fn insert(
    pool: &PgPool,
    user_id: Uuid,
    pin_hash: &str,
    expires_at: OffsetDateTime,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO password_reset_pins (user_id, pin_hash, expires_at) \
         VALUES ($1, $2, $3) RETURNING id",
        user_id,
        pin_hash,
        expires_at,
    )
    .fetch_one(pool)
    .await
}

/// Fetch the single active (unconsumed, unexpired) PIN for a user, newest first
/// if more than one somehow exists. Returns `Ok(None)` when none is active.
///
/// # Errors
///
/// Returns [`sqlx::Error`] from the `SELECT`.
#[allow(dead_code)] // Consumed by the reset-password route in this PR
pub async fn find_active_by_user(
    pool: &PgPool,
    user_id: Uuid,
) -> Result<Option<PasswordResetPin>, sqlx::Error> {
    sqlx::query_as!(
        PasswordResetPin,
        "SELECT id, user_id, pin_hash, expires_at FROM password_reset_pins \
         WHERE user_id = $1 AND consumed_at IS NULL AND expires_at > now() \
         ORDER BY created_at DESC LIMIT 1",
        user_id,
    )
    .fetch_optional(pool)
    .await
}

/// Mark a PIN consumed, but only if it is still unconsumed. Returns `true` iff
/// this call performed the consumption.
///
/// THREAT (single-use under concurrency): the guarded `WHERE consumed_at IS
/// NULL` makes consumption atomic at the row level, so two concurrent resets
/// presenting the same PIN cannot both succeed. The caller MUST treat `false`
/// as a failed reset (the PIN was already used).
///
/// # Errors
///
/// Returns [`sqlx::Error`] from the `UPDATE`.
#[allow(dead_code)] // Consumed by the reset-password route in this PR
pub async fn consume(pool: &PgPool, id: Uuid) -> Result<bool, sqlx::Error> {
    let result = sqlx::query!(
        "UPDATE password_reset_pins SET consumed_at = now() \
         WHERE id = $1 AND consumed_at IS NULL",
        id,
    )
    .execute(pool)
    .await?;
    Ok(result.rows_affected() == 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::Duration;

    async fn insert_user(pool: &PgPool) -> Uuid {
        sqlx::query_scalar!("INSERT INTO users (display_name) VALUES ('PIN Test') RETURNING id")
            .fetch_one(pool)
            .await
            .expect("insert user")
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn active_then_consumed_is_inactive(pool: PgPool) {
        let user_id = insert_user(&pool).await;
        let expires = OffsetDateTime::now_utc() + Duration::minutes(15);
        let id = insert(&pool, user_id, "$argon2id$hash", expires)
            .await
            .expect("insert pin");

        assert!(
            find_active_by_user(&pool, user_id)
                .await
                .expect("find")
                .is_some(),
            "freshly inserted PIN is active"
        );
        assert!(
            consume(&pool, id).await.expect("consume"),
            "first consume succeeds"
        );
        assert!(
            !consume(&pool, id).await.expect("second consume"),
            "second consume is a no-op (single-use)"
        );
        assert!(
            find_active_by_user(&pool, user_id)
                .await
                .expect("find")
                .is_none(),
            "consumed PIN is no longer active"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn expired_pin_is_not_active(pool: PgPool) {
        let user_id = insert_user(&pool).await;
        let expired = OffsetDateTime::now_utc() - Duration::minutes(1);
        insert(&pool, user_id, "$argon2id$hash", expired)
            .await
            .expect("insert pin");
        assert!(
            find_active_by_user(&pool, user_id)
                .await
                .expect("find")
                .is_none(),
            "an expired PIN is not active"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn supersede_leaves_at_most_one_active(pool: PgPool) {
        let user_id = insert_user(&pool).await;
        let expires = OffsetDateTime::now_utc() + Duration::minutes(15);
        insert(&pool, user_id, "$argon2id$first", expires)
            .await
            .expect("first");

        supersede_active(&pool, user_id).await.expect("supersede");
        let second = insert(&pool, user_id, "$argon2id$second", expires)
            .await
            .expect("second");

        let active = find_active_by_user(&pool, user_id)
            .await
            .expect("find")
            .expect("one active remains");
        assert_eq!(
            active.id, second,
            "only the newest PIN is active after supersede"
        );
    }
}
