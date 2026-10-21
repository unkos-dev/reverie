//! Account-bound, single-use authority to set a password.

use base64ct::{Base64UrlUnpadded, Encoding as _};
use chrono::{DateTime, TimeDelta, Utc};
use sha2::{Digest as _, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

/// Lifetime of an administrator-issued credential link.
pub const CREDENTIAL_LINK_TTL: TimeDelta = TimeDelta::hours(72);

/// Account context exposed only after a valid link lookup.
pub struct LinkAccount {
    /// Account that owns the link.
    pub user_id: Uuid,
    /// Current sign-in address.
    pub email: String,
    /// Password-policy context.
    pub display_name: String,
    /// Whether redemption replaces an existing password.
    pub has_password: bool,
}

/// Replace the account's link, returning its clear token exactly once.
///
/// # Errors
/// Returns database errors without persisting the clear token.
pub async fn issue(
    executor: impl sqlx::PgExecutor<'_>,
    user_id: Uuid,
) -> Result<(String, DateTime<Utc>), sqlx::Error> {
    let mut bytes = [0_u8; 32];
    rand::fill(&mut bytes);
    let token = Base64UrlUnpadded::encode_string(&bytes);
    let token_hash = Sha256::digest(token.as_bytes()).to_vec();
    let expires_at = Utc::now() + CREDENTIAL_LINK_TTL;
    // THREAT: A database disclosure must not reveal redeemable link tokens.
    let expires_at = sqlx::query_scalar!(
        "INSERT INTO credential_links (user_id, token_hash, expires_at) VALUES ($1, $2, $3) \
         ON CONFLICT (user_id) DO UPDATE SET token_hash = EXCLUDED.token_hash, \
         expires_at = EXCLUDED.expires_at, created_at = now() RETURNING expires_at",
        user_id,
        token_hash,
        expires_at,
    )
    .fetch_one(executor)
    .await?;
    Ok((token, expires_at))
}

/// Resolve only a live link for an enabled account with an email address.
///
/// # Errors
/// Returns database errors from the lookup.
pub async fn lookup(pool: &PgPool, token: &str) -> Result<Option<LinkAccount>, sqlx::Error> {
    if token.len() != 43 || Base64UrlUnpadded::decode_vec(token).is_err() {
        return Ok(None);
    }
    let token_hash = Sha256::digest(token.as_bytes()).to_vec();
    sqlx::query_as!(
        LinkAccount,
        r#"SELECT u.id AS user_id, u.email AS "email!", u.display_name,
                  (c.user_id IS NOT NULL) AS "has_password!"
             FROM credential_links l JOIN users u ON u.id = l.user_id
             LEFT JOIN local_credentials c ON c.user_id = u.id
            WHERE l.token_hash = $1 AND l.expires_at > now()
              AND u.disabled_at IS NULL AND u.email IS NOT NULL"#,
        token_hash,
    )
    .fetch_optional(pool)
    .await
}

/// Cancel the account's link, including an already expired link.
///
/// # Errors
/// Returns database errors from the deletion.
pub async fn cancel(executor: impl sqlx::PgExecutor<'_>, user_id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query!("DELETE FROM credential_links WHERE user_id = $1", user_id)
        .execute(executor)
        .await?;
    Ok(())
}

/// Consume once, after the caller locks the owning user row.
///
/// # Errors
/// Returns database errors from the conditional deletion.
pub async fn consume(
    executor: impl sqlx::PgExecutor<'_>,
    user_id: Uuid,
    token: &str,
) -> Result<bool, sqlx::Error> {
    let token_hash = Sha256::digest(token.as_bytes()).to_vec();
    // THREAT: Transaction-start time can admit a link that expired during a lock wait.
    Ok(sqlx::query!(
        "DELETE FROM credential_links WHERE user_id = $1 AND token_hash = $2 \
         AND expires_at > clock_timestamp()",
        user_id,
        token_hash,
    )
    .execute(executor)
    .await?
    .rows_affected()
        == 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn account(pool: &PgPool) -> Uuid {
        sqlx::query_scalar!("INSERT INTO users (display_name, email) VALUES ('Link User', 'link@example.test') RETURNING id")
            .fetch_one(pool).await.unwrap()
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn credential_link_issue_lookup_and_reissue(pool: PgPool) {
        let id = account(&pool).await;
        let (first, expiry) = issue(&pool, id).await.unwrap();
        assert_eq!(first.len(), 43);
        assert!(
            (expiry - Utc::now() - CREDENTIAL_LINK_TTL)
                .num_seconds()
                .abs()
                < 2
        );
        let found = lookup(&pool, &first).await.unwrap().unwrap();
        assert_eq!(found.user_id, id);
        assert_eq!(found.email, "link@example.test");
        assert!(!found.has_password);
        let (second, _) = issue(&pool, id).await.unwrap();
        assert_ne!(first, second);
        assert!(lookup(&pool, &first).await.unwrap().is_none());
        assert!(lookup(&pool, &second).await.unwrap().is_some());
        cancel(&pool, id).await.unwrap();
        cancel(&pool, id).await.unwrap();
        assert!(lookup(&pool, &second).await.unwrap().is_none());
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn credential_link_expiry_boundary(pool: PgPool) {
        let id = account(&pool).await;
        let (token, _) = issue(&pool, id).await.unwrap();
        sqlx::query!(
            "UPDATE credential_links SET expires_at = now() WHERE user_id = $1",
            id
        )
        .execute(&pool)
        .await
        .unwrap();
        assert!(lookup(&pool, &token).await.unwrap().is_none());
        assert!(!consume(&pool, id, &token).await.unwrap());
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn credential_link_disabled_and_no_email(pool: PgPool) {
        let id = account(&pool).await;
        let (token, _) = issue(&pool, id).await.unwrap();
        sqlx::query!("UPDATE users SET disabled_at = now() WHERE id = $1", id)
            .execute(&pool)
            .await
            .unwrap();
        assert!(lookup(&pool, &token).await.unwrap().is_none());
        sqlx::query!(
            "UPDATE users SET disabled_at = NULL, email = NULL WHERE id = $1",
            id
        )
        .execute(&pool)
        .await
        .unwrap();
        assert!(lookup(&pool, &token).await.unwrap().is_none());
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn credential_link_concurrent_consume(pool: PgPool) {
        let id = account(&pool).await;
        let (token, _) = issue(&pool, id).await.unwrap();
        let (a, b) = tokio::join!(consume(&pool, id, &token), consume(&pool, id, &token));
        assert_ne!(a.unwrap(), b.unwrap());
        assert!(lookup(&pool, &token).await.unwrap().is_none());
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn credential_link_expires_while_waiting_for_user_lock(pool: PgPool) {
        let id = account(&pool).await;
        let (token, _) = issue(&pool, id).await.unwrap();
        let mut holder = pool.begin().await.unwrap();
        sqlx::query!("SELECT id FROM users WHERE id = $1 FOR NO KEY UPDATE", id)
            .fetch_one(&mut *holder)
            .await
            .unwrap();
        let other = pool.clone();
        let waiter = tokio::spawn(async move {
            let mut tx = other.begin().await.unwrap();
            sqlx::query!("SELECT id FROM users WHERE id = $1 FOR NO KEY UPDATE", id)
                .fetch_one(&mut *tx)
                .await
                .unwrap();
            let consumed = consume(&mut *tx, id, &token).await.unwrap();
            tx.commit().await.unwrap();
            consumed
        });
        let mut blocked = false;
        for _ in 0..200 {
            if sqlx::query_scalar!(
                r#"SELECT EXISTS (SELECT 1 FROM pg_locks WHERE NOT granted) AS "blocked!""#
            )
            .fetch_one(&pool)
            .await
            .unwrap()
            {
                blocked = true;
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        assert!(blocked);
        sqlx::query!(
            "UPDATE credential_links SET expires_at = clock_timestamp() WHERE user_id = $1",
            id
        )
        .execute(&mut *holder)
        .await
        .unwrap();
        holder.commit().await.unwrap();
        assert!(!waiter.await.unwrap());
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn credential_link_cascades_on_user_delete(pool: PgPool) {
        let id = account(&pool).await;
        let (token, _) = issue(&pool, id).await.unwrap();
        sqlx::query!("DELETE FROM users WHERE id = $1", id)
            .execute(&pool)
            .await
            .unwrap();
        assert!(lookup(&pool, &token).await.unwrap().is_none());
        assert!(!consume(&pool, id, &token).await.unwrap());
    }
}
