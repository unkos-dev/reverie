//! Account-bound, single-use authority to set a password.

use base64ct::{Base64UrlUnpadded, Encoding as _};
use chrono::{DateTime, TimeDelta, Utc};
use sha2::{Digest as _, Sha256};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use crate::models::account_email_proof::{self, ProofPurpose};
use crate::models::mail_job::{self, AuthorityKind};

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

/// Replace the account's link, returning its clear token exactly once. Mail
/// queued for the replaced link is cancelled and its payload removed.
///
/// # Errors
/// Returns database errors without persisting the clear token.
pub async fn issue(
    conn: &mut PgConnection,
    user_id: Uuid,
    lifetime: TimeDelta,
) -> Result<(String, DateTime<Utc>), sqlx::Error> {
    let mut bytes = [0_u8; 32];
    rand::fill(&mut bytes);
    let token = Base64UrlUnpadded::encode_string(&bytes);
    let token_hash = Sha256::digest(token.as_bytes()).to_vec();
    mail_job::revoke_for_authority(&mut *conn, user_id, AuthorityKind::CredentialLink).await?;
    // THREAT: A database disclosure must not reveal redeemable link tokens.
    let expires_at = sqlx::query_scalar!(
        "INSERT INTO credential_links (user_id, token_hash, expires_at) \
         VALUES ($1, $2, clock_timestamp() + $3::bigint * interval '1 second') \
         ON CONFLICT (user_id) DO UPDATE SET token_hash = EXCLUDED.token_hash, \
         expires_at = EXCLUDED.expires_at, created_at = now() RETURNING expires_at",
        user_id,
        token_hash,
        lifetime.num_seconds(),
    )
    .fetch_one(&mut *conn)
    .await?;
    Ok((token, expires_at))
}

/// Resolve a live administrator or host link, or an eligible email recovery
/// proof, for an enabled account with an email address. Verification
/// challenges never resolve.
///
/// # Errors
/// Returns database errors from the lookup.
pub async fn lookup(pool: &PgPool, token: &str) -> Result<Option<LinkAccount>, sqlx::Error> {
    if token.len() != 43 || Base64UrlUnpadded::decode_vec(token).is_err() {
        return Ok(None);
    }
    let token_hash = Sha256::digest(token.as_bytes()).to_vec();
    let link = sqlx::query_as!(
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
    .await?;
    if link.is_some() {
        return Ok(link);
    }
    sqlx::query_as!(
        LinkAccount,
        r#"SELECT u.id AS user_id, u.email AS "email!", u.display_name,
                  TRUE AS "has_password!"
             FROM account_email_proofs p JOIN users u ON u.id = p.user_id
            WHERE p.token_hash = $1 AND p.purpose = 'recovery' AND p.expires_at > now()
              AND u.disabled_at IS NULL AND u.email_verified_at IS NOT NULL
              AND lower(u.email) = lower(p.email)
              AND EXISTS (SELECT 1 FROM local_credentials c WHERE c.user_id = u.id)"#,
        token_hash,
    )
    .fetch_optional(pool)
    .await
}

/// The expiry of the account's link when `token` is that link and it is still live.
///
/// # Errors
/// Returns database errors from the lookup.
pub async fn live_expiry(
    conn: &mut PgConnection,
    user_id: Uuid,
    token: &str,
) -> Result<Option<DateTime<Utc>>, sqlx::Error> {
    if token.len() != 43 || Base64UrlUnpadded::decode_vec(token).is_err() {
        return Ok(None);
    }
    let token_hash = Sha256::digest(token.as_bytes()).to_vec();
    sqlx::query_scalar!(
        "SELECT expires_at FROM credential_links \
          WHERE user_id = $1 AND token_hash = $2 AND expires_at > clock_timestamp()",
        user_id,
        token_hash,
    )
    .fetch_optional(conn)
    .await
}

/// Cancel the account's link, including an already expired link, and withdraw
/// its queued mail.
///
/// # Errors
/// Returns database errors from the deletion.
pub async fn cancel(conn: &mut PgConnection, user_id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query!("DELETE FROM credential_links WHERE user_id = $1", user_id)
        .execute(&mut *conn)
        .await?;
    mail_job::revoke_for_authority(&mut *conn, user_id, AuthorityKind::CredentialLink).await?;
    Ok(())
}

/// Consume once, after the caller locks the owning user row, and withdraw the
/// link's queued mail.
///
/// # Errors
/// Returns database errors from the conditional deletion.
pub async fn consume(
    conn: &mut PgConnection,
    user_id: Uuid,
    token: &str,
) -> Result<bool, sqlx::Error> {
    let token_hash = Sha256::digest(token.as_bytes()).to_vec();
    // THREAT: Transaction-start time can admit a link that expired during a lock wait.
    let consumed = sqlx::query!(
        "DELETE FROM credential_links WHERE user_id = $1 AND token_hash = $2 \
         AND expires_at > clock_timestamp()",
        user_id,
        token_hash,
    )
    .execute(&mut *conn)
    .await?
    .rows_affected()
        == 1;
    if consumed {
        mail_job::revoke_for_authority(&mut *conn, user_id, AuthorityKind::CredentialLink).await?;
    }
    Ok(consumed)
}

/// Consume the password-replacement authority `token` names and delete the rest.
///
/// The winner is an administrator or host link, or an eligible email recovery
/// proof. Every other replacement authority and its queued mail goes with it;
/// verification challenges are untouched. Returns whether `token` won.
///
/// The caller holds the owning user-row lock inside a transaction.
///
/// # Errors
/// Returns database errors from the deletions.
pub async fn consume_replacement(
    conn: &mut PgConnection,
    user_id: Uuid,
    token: &str,
) -> Result<bool, sqlx::Error> {
    if consume(&mut *conn, user_id, token).await? {
        account_email_proof::cancel(conn, user_id, ProofPurpose::Recovery).await?;
        return Ok(true);
    }
    // THREAT: A recovery proof outlives neither the verified address nor the password it protects.
    let eligible = sqlx::query_scalar!(
        r#"SELECT (u.disabled_at IS NULL AND u.email_verified_at IS NOT NULL
                   AND EXISTS (SELECT 1 FROM local_credentials c WHERE c.user_id = u.id))
                      AS "eligible!"
             FROM users u WHERE u.id = $1"#,
        user_id,
    )
    .fetch_optional(&mut *conn)
    .await?
    .unwrap_or(false);
    if !eligible {
        return Ok(false);
    }
    if account_email_proof::consume(&mut *conn, user_id, ProofPurpose::Recovery, token)
        .await?
        .is_none()
    {
        return Ok(false);
    }
    cancel(conn, user_id).await?;
    Ok(true)
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
        let (first, expiry) = issue(
            &mut pool.acquire().await.unwrap(),
            id,
            crate::models::credential_link::CREDENTIAL_LINK_TTL,
        )
        .await
        .unwrap();
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
        let (second, _) = issue(
            &mut pool.acquire().await.unwrap(),
            id,
            crate::models::credential_link::CREDENTIAL_LINK_TTL,
        )
        .await
        .unwrap();
        assert_ne!(first, second);
        assert!(lookup(&pool, &first).await.unwrap().is_none());
        assert!(lookup(&pool, &second).await.unwrap().is_some());
        cancel(&mut pool.acquire().await.unwrap(), id)
            .await
            .unwrap();
        cancel(&mut pool.acquire().await.unwrap(), id)
            .await
            .unwrap();
        assert!(lookup(&pool, &second).await.unwrap().is_none());
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn credential_link_expiry_boundary(pool: PgPool) {
        let id = account(&pool).await;
        let (token, _) = issue(
            &mut pool.acquire().await.unwrap(),
            id,
            crate::models::credential_link::CREDENTIAL_LINK_TTL,
        )
        .await
        .unwrap();
        sqlx::query!(
            "UPDATE credential_links SET expires_at = now() WHERE user_id = $1",
            id
        )
        .execute(&pool)
        .await
        .unwrap();
        assert!(lookup(&pool, &token).await.unwrap().is_none());
        assert!(
            !consume(&mut pool.acquire().await.unwrap(), id, &token)
                .await
                .unwrap()
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn credential_link_disabled_and_no_email(pool: PgPool) {
        let id = account(&pool).await;
        let (token, _) = issue(
            &mut pool.acquire().await.unwrap(),
            id,
            crate::models::credential_link::CREDENTIAL_LINK_TTL,
        )
        .await
        .unwrap();
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
        let (token, _) = issue(
            &mut pool.acquire().await.unwrap(),
            id,
            crate::models::credential_link::CREDENTIAL_LINK_TTL,
        )
        .await
        .unwrap();
        let mut first = pool.acquire().await.unwrap();
        let mut second = pool.acquire().await.unwrap();
        let (a, b) = tokio::join!(
            consume(&mut first, id, &token),
            consume(&mut second, id, &token)
        );
        assert_ne!(a.unwrap(), b.unwrap());
        assert!(lookup(&pool, &token).await.unwrap().is_none());
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn credential_link_expires_while_waiting_for_user_lock(pool: PgPool) {
        let id = account(&pool).await;
        let (token, _) = issue(
            &mut pool.acquire().await.unwrap(),
            id,
            crate::models::credential_link::CREDENTIAL_LINK_TTL,
        )
        .await
        .unwrap();
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
            let consumed = consume(&mut tx, id, &token).await.unwrap();
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
        let (token, _) = issue(
            &mut pool.acquire().await.unwrap(),
            id,
            crate::models::credential_link::CREDENTIAL_LINK_TTL,
        )
        .await
        .unwrap();
        sqlx::query!("DELETE FROM users WHERE id = $1", id)
            .execute(&pool)
            .await
            .unwrap();
        assert!(lookup(&pool, &token).await.unwrap().is_none());
        assert!(
            !consume(&mut pool.acquire().await.unwrap(), id, &token)
                .await
                .unwrap()
        );
    }
}
