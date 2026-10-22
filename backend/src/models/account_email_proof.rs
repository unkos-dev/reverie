//! Mailbox proofs: one slot per account and purpose, holding only a token digest.
//!
//! Every function except [`lock_account`](crate::models::account_email_proof::lock_account)
//! expects the caller to hold the owning user-row lock and to run in one
//! transaction with its mail-job writes.
//! Expiry decisions use `clock_timestamp()` so a lock wait cannot admit a proof
//! that expired while the caller waited.

use base64ct::{Base64UrlUnpadded, Encoding as _};
use chrono::{DateTime, TimeDelta, Utc};
use sha2::{Digest as _, Sha256};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::models::mail_job::{self, AuthorityKind, MailPurpose};

/// What a proof lets its holder do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProofPurpose {
    /// Confirm control of the account's address.
    Verification,
    /// Replace a forgotten password.
    Recovery,
}

impl ProofPurpose {
    /// The stored identifier.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Verification => "verification",
            Self::Recovery => "recovery",
        }
    }

    /// How long a proof of this purpose stays live: 24 hours for verification,
    /// 15 minutes for recovery.
    #[must_use]
    pub fn ttl(self) -> TimeDelta {
        TimeDelta::seconds(i64::from(self.ttl_secs()))
    }

    const fn ttl_secs(self) -> i32 {
        match self {
            Self::Verification => 24 * 60 * 60,
            Self::Recovery => 15 * 60,
        }
    }

    /// The mail purpose that carries this proof.
    #[must_use]
    pub const fn mail_purpose(self) -> MailPurpose {
        match self {
            Self::Verification => MailPurpose::Verification,
            Self::Recovery => MailPurpose::EmailRecovery,
        }
    }

    /// The job authority kind that references this proof.
    #[must_use]
    pub const fn authority_kind(self) -> AuthorityKind {
        match self {
            Self::Verification => AuthorityKind::Verification,
            Self::Recovery => AuthorityKind::Recovery,
        }
    }
}

/// The account fields a locked mutation decides on.
#[derive(Debug)]
pub struct LockedAccount {
    /// Current sign-in address.
    pub email: Option<String>,
    /// Whether the account is disabled.
    pub disabled: bool,
    /// When Reverie verified the current address.
    pub email_verified_at: Option<DateTime<Utc>>,
}

/// Take the user-row lock every account-sensitive mutation starts with.
/// Returns `None` when the account does not exist.
///
/// # Errors
/// Returns database errors from the lock.
pub async fn lock_account(
    conn: &mut PgConnection,
    user_id: Uuid,
) -> Result<Option<LockedAccount>, sqlx::Error> {
    if sqlx::query_scalar!(
        "SELECT id FROM users WHERE id = $1 FOR NO KEY UPDATE",
        user_id,
    )
    .fetch_optional(&mut *conn)
    .await?
    .is_none()
    {
        return Ok(None);
    }
    sqlx::query_as!(
        LockedAccount,
        r#"SELECT email, (disabled_at IS NOT NULL) AS "disabled!", email_verified_at
             FROM users WHERE id = $1"#,
        user_id,
    )
    .fetch_optional(conn)
    .await
}

/// The SHA-256 digest a proof is stored and referenced by.
#[must_use]
pub fn digest(token: &str) -> Vec<u8> {
    Sha256::digest(token.as_bytes()).to_vec()
}

/// A freshly issued proof. The clear token exists only here.
pub struct IssuedProof {
    /// The 43-character bearer token.
    pub token: String,
    /// Its stored digest.
    pub token_hash: Vec<u8>,
    /// When the proof stops working.
    pub expires_at: DateTime<Utc>,
}

impl std::fmt::Debug for IssuedProof {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IssuedProof")
            .field("expires_at", &self.expires_at)
            .finish_non_exhaustive()
    }
}

/// A live proof as stored. The digest identifies it; the token cannot be
/// recovered from it.
pub struct LiveProof {
    /// Address the proof is bound to.
    pub address: String,
    /// Stored digest.
    pub token_hash: Vec<u8>,
    /// When the proof was issued.
    pub created_at: DateTime<Utc>,
    /// When the proof stops working.
    pub expires_at: DateTime<Utc>,
}

impl std::fmt::Debug for LiveProof {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LiveProof")
            .field("expires_at", &self.expires_at)
            .finish_non_exhaustive()
    }
}

/// Replace the account's proof of `purpose`, bound to `address`. The replaced
/// proof's unsent mail is cancelled and its retained payloads removed.
///
/// # Errors
/// Returns database errors; nothing is persisted with the clear token.
pub async fn issue(
    conn: &mut PgConnection,
    user_id: Uuid,
    purpose: ProofPurpose,
    address: &str,
) -> Result<IssuedProof, sqlx::Error> {
    let mut bytes = [0_u8; 32];
    rand::fill(&mut bytes);
    let token = Base64UrlUnpadded::encode_string(&bytes);
    let token_hash = digest(&token);
    if let Some(replaced) = sqlx::query_scalar!(
        "SELECT token_hash FROM account_email_proofs WHERE user_id = $1 AND purpose = $2",
        user_id,
        purpose.as_str(),
    )
    .fetch_optional(&mut *conn)
    .await?
    {
        mail_job::revoke_for_proofs(conn, user_id, &[replaced]).await?;
    }
    // THREAT: A database disclosure must not reveal redeemable proof tokens.
    let expires_at = sqlx::query_scalar!(
        "INSERT INTO account_email_proofs (user_id, purpose, email, token_hash, created_at, expires_at) \
         SELECT $1::uuid, $2::text, $3::text, $4::bytea, c.t, c.t + make_interval(secs => $5::int) \
           FROM (SELECT clock_timestamp() AS t) c \
         ON CONFLICT (user_id, purpose) DO UPDATE SET email = EXCLUDED.email, \
            token_hash = EXCLUDED.token_hash, created_at = EXCLUDED.created_at, \
            expires_at = EXCLUDED.expires_at \
         RETURNING expires_at",
        user_id,
        purpose.as_str(),
        address,
        token_hash,
        purpose.ttl_secs(),
    )
    .fetch_one(&mut *conn)
    .await?;
    Ok(IssuedProof {
        token,
        token_hash,
        expires_at,
    })
}

/// The account's live proof of `purpose`, if any.
///
/// # Errors
/// Returns database errors from the lookup.
pub async fn live(
    conn: &mut PgConnection,
    user_id: Uuid,
    purpose: ProofPurpose,
) -> Result<Option<LiveProof>, sqlx::Error> {
    sqlx::query_as!(
        LiveProof,
        "SELECT email AS address, token_hash, created_at, expires_at \
           FROM account_email_proofs \
          WHERE user_id = $1 AND purpose = $2 AND expires_at > clock_timestamp()",
        user_id,
        purpose.as_str(),
    )
    .fetch_optional(conn)
    .await
}

/// Consume the proof once and return the address it was bound to.
///
/// It must match the token, be live, belong to an enabled account and still be
/// bound to that account's current address. Its unsent mail is cancelled and
/// its retained payloads removed.
///
/// # Errors
/// Returns database errors from the deletion.
pub async fn consume(
    conn: &mut PgConnection,
    user_id: Uuid,
    purpose: ProofPurpose,
    token: &str,
) -> Result<Option<String>, sqlx::Error> {
    let token_hash = digest(token);
    // THREAT: Transaction-start time can admit a proof that expired during a lock wait.
    let consumed = sqlx::query!(
        "DELETE FROM account_email_proofs p USING users u \
          WHERE u.id = p.user_id AND p.user_id = $1 AND p.purpose = $2 AND p.token_hash = $3 \
            AND p.expires_at > clock_timestamp() AND u.disabled_at IS NULL \
            AND lower(u.email) = lower(p.email) \
          RETURNING p.email",
        user_id,
        purpose.as_str(),
        token_hash,
    )
    .fetch_optional(&mut *conn)
    .await?;
    let Some(consumed) = consumed else {
        return Ok(None);
    };
    mail_job::revoke_for_proofs(conn, user_id, &[token_hash]).await?;
    Ok(Some(consumed.email))
}

/// Delete the account's proof of `purpose` and revoke its mail, including an
/// already expired proof.
///
/// # Errors
/// Returns database errors from the deletion.
pub async fn cancel(
    conn: &mut PgConnection,
    user_id: Uuid,
    purpose: ProofPurpose,
) -> Result<(), sqlx::Error> {
    let removed = sqlx::query_scalar!(
        "DELETE FROM account_email_proofs WHERE user_id = $1 AND purpose = $2 RETURNING token_hash",
        user_id,
        purpose.as_str(),
    )
    .fetch_all(&mut *conn)
    .await?;
    mail_job::revoke_for_proofs(conn, user_id, &removed).await?;
    Ok(())
}

/// Delete every proof the account holds and revoke their mail.
///
/// # Errors
/// Returns database errors from the deletion.
pub async fn cancel_all(conn: &mut PgConnection, user_id: Uuid) -> Result<(), sqlx::Error> {
    let removed = sqlx::query_scalar!(
        "DELETE FROM account_email_proofs WHERE user_id = $1 RETURNING token_hash",
        user_id,
    )
    .fetch_all(&mut *conn)
    .await?;
    mail_job::revoke_for_proofs(conn, user_id, &removed).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::db::app_pool_for;
    use crate::test_support::mail::{account, enqueue_bearer, payload_present, state_of};
    use sqlx::PgPool;

    const ADDRESS: &str = "reader@example.org";

    async fn issue_in_tx(app: &PgPool, user: Uuid, purpose: ProofPurpose) -> IssuedProof {
        let mut tx = app.begin().await.unwrap();
        lock_account(&mut tx, user).await.unwrap().unwrap();
        let proof = issue(&mut tx, user, purpose, ADDRESS).await.unwrap();
        tx.commit().await.unwrap();
        proof
    }

    async fn consume_in_tx(
        app: &PgPool,
        user: Uuid,
        purpose: ProofPurpose,
        token: &str,
    ) -> Option<String> {
        let mut tx = app.begin().await.unwrap();
        lock_account(&mut tx, user).await.unwrap().unwrap();
        let consumed = consume(&mut tx, user, purpose, token).await.unwrap();
        tx.commit().await.unwrap();
        consumed
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn smtp_proof_issue_stores_only_a_digest_and_the_purpose_lifetime(pool: PgPool) {
        let app = app_pool_for(&pool).await;
        let user = account(&pool, ADDRESS).await;
        let verification = issue_in_tx(&app, user, ProofPurpose::Verification).await;
        let recovery = issue_in_tx(&app, user, ProofPurpose::Recovery).await;
        assert_eq!(verification.token.len(), 43);
        assert_ne!(verification.token, recovery.token);
        let rows = sqlx::query!(
            "SELECT purpose, email, token_hash, EXTRACT(EPOCH FROM expires_at - created_at)::float8 AS \"ttl!\" \
               FROM account_email_proofs ORDER BY purpose"
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(rows.len(), 2);
        for row in &rows {
            assert_eq!(row.email, ADDRESS);
            assert_eq!(row.token_hash.len(), 32);
            let (token, ttl) = if row.purpose == "recovery" {
                (&recovery.token, 15.0 * 60.0)
            } else {
                (&verification.token, 24.0 * 3_600.0)
            };
            assert_ne!(row.token_hash, token.as_bytes());
            assert_eq!(row.token_hash, digest(token));
            assert!((row.ttl - ttl).abs() < f64::EPSILON);
        }
        assert_eq!(
            ProofPurpose::Recovery.ttl(),
            TimeDelta::minutes(15),
            "recovery lifetime"
        );
        assert_eq!(ProofPurpose::Verification.ttl(), TimeDelta::hours(24));
        let mut conn = app.acquire().await.unwrap();
        let live = live(&mut conn, user, ProofPurpose::Recovery).await.unwrap();
        assert_eq!(live.unwrap().token_hash, recovery.token_hash);
        assert!(format!("{recovery:?}").find(&recovery.token).is_none());
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn smtp_proof_replacement_revokes_the_old_proof_and_its_mail(pool: PgPool) {
        let app = app_pool_for(&pool).await;
        let user = account(&pool, ADDRESS).await;
        let first = issue_in_tx(&app, user, ProofPurpose::Recovery).await;
        let job = enqueue_bearer(
            &app,
            user,
            MailPurpose::EmailRecovery,
            &first.token_hash,
            TimeDelta::minutes(15),
            ADDRESS,
        )
        .await;
        let second = issue_in_tx(&app, user, ProofPurpose::Recovery).await;
        assert_ne!(first.token, second.token);
        assert_eq!(state_of(&pool, job).await, "cancelled");
        assert!(!payload_present(&pool, job).await);
        assert_eq!(
            consume_in_tx(&app, user, ProofPurpose::Recovery, &first.token).await,
            None
        );
        assert_eq!(
            consume_in_tx(&app, user, ProofPurpose::Recovery, &second.token).await,
            Some(ADDRESS.to_owned())
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn smtp_proof_consume_is_single_use_and_purpose_bound(pool: PgPool) {
        let app = app_pool_for(&pool).await;
        let user = account(&pool, ADDRESS).await;
        let recovery = issue_in_tx(&app, user, ProofPurpose::Recovery).await;
        let verification = issue_in_tx(&app, user, ProofPurpose::Verification).await;
        assert_eq!(
            consume_in_tx(&app, user, ProofPurpose::Verification, &recovery.token).await,
            None
        );
        assert_eq!(
            consume_in_tx(&app, user, ProofPurpose::Recovery, &verification.token).await,
            None
        );
        assert_eq!(
            consume_in_tx(&app, user, ProofPurpose::Recovery, "short").await,
            None
        );
        assert!(
            consume_in_tx(&app, user, ProofPurpose::Recovery, &recovery.token)
                .await
                .is_some()
        );
        assert_eq!(
            consume_in_tx(&app, user, ProofPurpose::Recovery, &recovery.token).await,
            None
        );
        assert!(
            consume_in_tx(&app, user, ProofPurpose::Verification, &verification.token)
                .await
                .is_some(),
            "consuming one purpose leaves the other slot alone"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn smtp_proof_concurrent_consume_has_one_winner(pool: PgPool) {
        let app = app_pool_for(&pool).await;
        let user = account(&pool, ADDRESS).await;
        let proof = issue_in_tx(&app, user, ProofPurpose::Recovery).await;
        let (a, b) = tokio::join!(
            consume_in_tx(&app, user, ProofPurpose::Recovery, &proof.token),
            consume_in_tx(&app, user, ProofPurpose::Recovery, &proof.token),
        );
        assert_ne!(a.is_some(), b.is_some());
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn smtp_proof_consume_refuses_address_change_disabled_account_and_expiry(pool: PgPool) {
        let app = app_pool_for(&pool).await;
        let user = account(&pool, ADDRESS).await;

        let proof = issue_in_tx(&app, user, ProofPurpose::Recovery).await;
        sqlx::query!(
            "UPDATE users SET email = 'moved@example.org' WHERE id = $1",
            user
        )
        .execute(&pool)
        .await
        .unwrap();
        assert_eq!(
            consume_in_tx(&app, user, ProofPurpose::Recovery, &proof.token).await,
            None
        );
        sqlx::query!("UPDATE users SET email = $2 WHERE id = $1", user, ADDRESS)
            .execute(&pool)
            .await
            .unwrap();

        sqlx::query!("UPDATE users SET disabled_at = now() WHERE id = $1", user)
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(
            consume_in_tx(&app, user, ProofPurpose::Recovery, &proof.token).await,
            None
        );
        sqlx::query!("UPDATE users SET disabled_at = NULL WHERE id = $1", user)
            .execute(&pool)
            .await
            .unwrap();

        sqlx::query!(
            "UPDATE account_email_proofs SET expires_at = clock_timestamp() WHERE user_id = $1",
            user
        )
        .execute(&pool)
        .await
        .unwrap();
        assert_eq!(
            consume_in_tx(&app, user, ProofPurpose::Recovery, &proof.token).await,
            None
        );
        let mut conn = app.acquire().await.unwrap();
        assert!(
            live(&mut conn, user, ProofPurpose::Recovery)
                .await
                .unwrap()
                .is_none()
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn smtp_proof_expires_while_waiting_for_the_account_lock(pool: PgPool) {
        let app = app_pool_for(&pool).await;
        let user = account(&pool, ADDRESS).await;
        let proof = issue_in_tx(&app, user, ProofPurpose::Recovery).await;
        let mut holder = app.begin().await.unwrap();
        lock_account(&mut holder, user).await.unwrap();
        let other = app.clone();
        let token = proof.token.clone();
        let waiter = tokio::spawn(async move {
            consume_in_tx(&other, user, ProofPurpose::Recovery, &token).await
        });
        crate::test_support::wait_for_blocked_query(
            &pool,
            "SELECT id FROM users WHERE id = $1 FOR NO KEY UPDATE",
            1,
        )
        .await;
        sqlx::query!(
            "UPDATE account_email_proofs SET expires_at = clock_timestamp() WHERE user_id = $1",
            user
        )
        .execute(&mut *holder)
        .await
        .unwrap();
        holder.commit().await.unwrap();
        assert_eq!(waiter.await.unwrap(), None);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn smtp_proof_cancel_all_revokes_unsent_mail_and_keeps_metadata(pool: PgPool) {
        let app = app_pool_for(&pool).await;
        let user = account(&pool, ADDRESS).await;
        let recovery = issue_in_tx(&app, user, ProofPurpose::Recovery).await;
        let verification = issue_in_tx(&app, user, ProofPurpose::Verification).await;
        let pending = enqueue_bearer(
            &app,
            user,
            MailPurpose::EmailRecovery,
            &recovery.token_hash,
            TimeDelta::minutes(15),
            ADDRESS,
        )
        .await;
        let accepted = enqueue_bearer(
            &app,
            user,
            MailPurpose::Verification,
            &verification.token_hash,
            TimeDelta::hours(24),
            ADDRESS,
        )
        .await;
        sqlx::query!(
            "UPDATE mail_jobs SET state = 'accepted', terminal_at = now() WHERE id = $1",
            accepted
        )
        .execute(&pool)
        .await
        .unwrap();
        let notice = crate::test_support::mail::enqueue_notice(
            &app,
            user,
            MailPurpose::NoticePasswordSet,
            ADDRESS,
        )
        .await;
        let before = sqlx::query!(
            "SELECT id, user_id, purpose, proof_digest, enqueued_at FROM mail_jobs ORDER BY id"
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        let mut tx = app.begin().await.unwrap();
        lock_account(&mut tx, user).await.unwrap();
        cancel_all(&mut tx, user).await.unwrap();
        tx.commit().await.unwrap();
        assert_eq!(state_of(&pool, pending).await, "cancelled");
        assert_eq!(state_of(&pool, accepted).await, "accepted");
        assert!(!payload_present(&pool, pending).await);
        assert!(!payload_present(&pool, accepted).await);
        assert_eq!(state_of(&pool, notice).await, "pending");
        assert!(
            payload_present(&pool, notice).await,
            "notices are not bearer mail"
        );
        let after = sqlx::query!(
            "SELECT id, user_id, purpose, proof_digest, enqueued_at FROM mail_jobs ORDER BY id"
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(before.len(), after.len());
        for (a, b) in before.iter().zip(&after) {
            assert_eq!(
                (a.id, a.user_id, &a.purpose, &a.proof_digest, a.enqueued_at),
                (b.id, b.user_id, &b.purpose, &b.proof_digest, b.enqueued_at)
            );
        }
        assert_eq!(
            sqlx::query_scalar!("SELECT count(*) FROM account_email_proofs")
                .fetch_one(&pool)
                .await
                .unwrap(),
            Some(0)
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn smtp_proof_consume_clears_retained_payloads_of_accepted_mail(pool: PgPool) {
        let app = app_pool_for(&pool).await;
        let user = account(&pool, ADDRESS).await;
        let proof = issue_in_tx(&app, user, ProofPurpose::Recovery).await;
        let job = enqueue_bearer(
            &app,
            user,
            MailPurpose::EmailRecovery,
            &proof.token_hash,
            TimeDelta::minutes(15),
            ADDRESS,
        )
        .await;
        sqlx::query!(
            "UPDATE mail_jobs SET state = 'accepted', terminal_at = now() WHERE id = $1",
            job
        )
        .execute(&pool)
        .await
        .unwrap();
        assert!(payload_present(&pool, job).await);
        assert!(
            consume_in_tx(&app, user, ProofPurpose::Recovery, &proof.token)
                .await
                .is_some()
        );
        assert!(!payload_present(&pool, job).await);
        assert_eq!(state_of(&pool, job).await, "accepted");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn smtp_proof_rolled_back_issue_leaves_the_previous_proof(pool: PgPool) {
        let app = app_pool_for(&pool).await;
        let user = account(&pool, ADDRESS).await;
        let first = issue_in_tx(&app, user, ProofPurpose::Recovery).await;
        let job = enqueue_bearer(
            &app,
            user,
            MailPurpose::EmailRecovery,
            &first.token_hash,
            TimeDelta::minutes(15),
            ADDRESS,
        )
        .await;
        let mut tx = app.begin().await.unwrap();
        lock_account(&mut tx, user).await.unwrap();
        issue(&mut tx, user, ProofPurpose::Recovery, ADDRESS)
            .await
            .unwrap();
        tx.rollback().await.unwrap();
        assert_eq!(state_of(&pool, job).await, "pending");
        assert!(
            consume_in_tx(&app, user, ProofPurpose::Recovery, &first.token)
                .await
                .is_some()
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn smtp_proof_deleting_the_account_deletes_its_proofs(pool: PgPool) {
        let app = app_pool_for(&pool).await;
        let user = account(&pool, ADDRESS).await;
        issue_in_tx(&app, user, ProofPurpose::Recovery).await;
        issue_in_tx(&app, user, ProofPurpose::Verification).await;
        sqlx::query!("DELETE FROM users WHERE id = $1", user)
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(
            sqlx::query_scalar!("SELECT count(*) FROM account_email_proofs")
                .fetch_one(&pool)
                .await
                .unwrap(),
            Some(0)
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn smtp_proof_lock_account_reports_state_and_absence(pool: PgPool) {
        let app = app_pool_for(&pool).await;
        let user = account(&pool, ADDRESS).await;
        let mut conn = app.acquire().await.unwrap();
        let state = lock_account(&mut conn, user).await.unwrap().unwrap();
        assert_eq!(state.email.as_deref(), Some(ADDRESS));
        assert!(!state.disabled);
        assert!(state.email_verified_at.is_none());
        assert!(
            lock_account(&mut conn, Uuid::new_v4())
                .await
                .unwrap()
                .is_none()
        );
    }
}
