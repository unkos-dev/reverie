//! Encrypted authentication-mail jobs and the allowances derived from them.
//!
//! Every mutating function takes the caller's connection after the caller has
//! locked the owning user row (see
//! [`crate::models::account_email_proof::lock_account`]); decisions use
//! `clock_timestamp()` so a lock wait cannot make a stale instant authoritative.

use chrono::{DateTime, TimeDelta, Utc};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use crate::services::mail::crypto::{self, CryptoError, QueueKey};

/// Minimum gap between two queued deliveries of one purpose for one account.
pub const REQUEST_INTERVAL: TimeDelta = TimeDelta::seconds(60);
/// Window over which public recovery deliveries are counted.
pub const ROLLING_WINDOW: TimeDelta = TimeDelta::hours(24);
/// Public recovery deliveries allowed per account in [`ROLLING_WINDOW`].
pub const RECOVERY_DAILY_LIMIT: i64 = 5;
/// Deliveries per recovery proof: the first plus two requested repeats.
pub const PROOF_DELIVERY_LIMIT: i64 = 3;
/// Automatic submission attempts per job.
pub const MAX_ATTEMPTS: i32 = 5;

/// Why a message is sent. The stored text is also the authenticated purpose of
/// its sealed payload, so the spellings are permanent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MailPurpose {
    /// An administrator-selected credential link.
    AdminLink,
    /// A mailbox verification challenge.
    Verification,
    /// A public password recovery link.
    EmailRecovery,
    /// A password was replaced.
    NoticePasswordChanged,
    /// A first password was set.
    NoticePasswordSet,
    /// An address was verified.
    NoticeAddressVerified,
    /// A verified address was changed.
    NoticeAddressChanged,
    /// A verified address was removed.
    NoticeAddressRemoved,
}

impl MailPurpose {
    /// The stored identifier.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AdminLink => "admin_link",
            Self::Verification => "verification",
            Self::EmailRecovery => "email_recovery",
            Self::NoticePasswordChanged => "notice_password_changed",
            Self::NoticePasswordSet => "notice_password_set",
            Self::NoticeAddressVerified => "notice_address_verified",
            Self::NoticeAddressChanged => "notice_address_changed",
            Self::NoticeAddressRemoved => "notice_address_removed",
        }
    }

    /// Parse a stored identifier.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "admin_link" => Self::AdminLink,
            "verification" => Self::Verification,
            "email_recovery" => Self::EmailRecovery,
            "notice_password_changed" => Self::NoticePasswordChanged,
            "notice_password_set" => Self::NoticePasswordSet,
            "notice_address_verified" => Self::NoticeAddressVerified,
            "notice_address_changed" => Self::NoticeAddressChanged,
            "notice_address_removed" => Self::NoticeAddressRemoved,
            _ => return None,
        })
    }

    /// The proof a bearer message carries, or `None` for a notice.
    #[must_use]
    pub const fn authority_kind(self) -> Option<AuthorityKind> {
        match self {
            Self::AdminLink => Some(AuthorityKind::CredentialLink),
            Self::Verification => Some(AuthorityKind::Verification),
            Self::EmailRecovery => Some(AuthorityKind::Recovery),
            _ => None,
        }
    }
}

/// Which table holds the proof a bearer message carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthorityKind {
    /// The administrator and host slot in `credential_links`.
    CredentialLink,
    /// A `verification` row in `account_email_proofs`.
    Verification,
    /// A `recovery` row in `account_email_proofs`.
    Recovery,
}

impl AuthorityKind {
    /// The stored identifier.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CredentialLink => "credential_link",
            Self::Verification => "verification",
            Self::Recovery => "recovery",
        }
    }

    /// Parse a stored identifier.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "credential_link" => Some(Self::CredentialLink),
            "verification" => Some(Self::Verification),
            "recovery" => Some(Self::Recovery),
            _ => None,
        }
    }

    /// Whether a resend can recover the token from a retained payload.
    #[must_use]
    pub const fn payload_outlives_delivery(self) -> bool {
        matches!(self, Self::Verification | Self::Recovery)
    }
}

/// Lifecycle of a job row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JobState {
    /// Waiting for its next attempt.
    Pending,
    /// Claimed by the worker.
    InProgress,
    /// The relay accepted the message.
    Accepted,
    /// Retries are exhausted or the relay refused permanently.
    Failed,
    /// Revoked before acceptance.
    Cancelled,
    /// Its lifetime ended before acceptance.
    Expired,
}

impl JobState {
    /// The stored identifier.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::InProgress => "in_progress",
            Self::Accepted => "accepted",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Expired => "expired",
        }
    }

    /// Parse a stored identifier.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "pending" => Self::Pending,
            "in_progress" => Self::InProgress,
            "accepted" => Self::Accepted,
            "failed" => Self::Failed,
            "cancelled" => Self::Cancelled,
            "expired" => Self::Expired,
            _ => return None,
        })
    }
}

/// The recipient and rendered message sealed into a job. `Debug` hides all
/// three fields.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct MailPayload {
    /// Envelope and header recipient.
    pub recipient: String,
    /// Subject line.
    pub subject: String,
    /// Plain-text body.
    pub body: String,
}

impl std::fmt::Debug for MailPayload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("MailPayload(<redacted>)")
    }
}

/// A delivery to queue.
pub struct NewJob<'a> {
    /// Owning account.
    pub user_id: Uuid,
    /// Why the message is sent.
    pub purpose: MailPurpose,
    /// SHA-256 digest of the bearer token the message carries; `None` for notices.
    pub proof_digest: Option<&'a [u8]>,
    /// When a bearer message stops being useful; `None` for notices, which last
    /// 24 hours from their enqueue time.
    pub expires_at: Option<DateTime<Utc>>,
    /// The recipient and rendered message.
    pub payload: &'a MailPayload,
}

/// Why a job could not be queued.
#[derive(Debug, thiserror::Error)]
pub enum EnqueueError {
    /// Bearer purposes need a digest and expiry; notices must carry neither.
    #[error("job fields do not match its purpose")]
    Shape,
    /// The payload could not be encoded.
    #[error("payload could not be encoded")]
    Encode,
    /// Sealing or opening failed.
    #[error(transparent)]
    Crypto(#[from] CryptoError),
    /// The database refused the write.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

/// Queue a job, returning its UUID.
///
/// # Errors
/// Returns [`EnqueueError`] for mismatched fields, a sealing failure, or a
/// database error, including a second live delivery of one proof.
pub async fn enqueue(
    conn: &mut PgConnection,
    key: &QueueKey,
    job: &NewJob<'_>,
) -> Result<Uuid, EnqueueError> {
    let bearer = job.purpose.authority_kind();
    let shaped = match bearer {
        Some(_) => job.proof_digest.is_some() && job.expires_at.is_some(),
        None => job.proof_digest.is_none(),
    };
    if !shaped {
        return Err(EnqueueError::Shape);
    }
    let plaintext = serde_json::to_vec(job.payload).map_err(|_| EnqueueError::Encode)?;
    insert_sealed(
        conn,
        key,
        job.user_id,
        job.purpose,
        job.proof_digest,
        job.expires_at,
        &plaintext,
    )
    .await
}

async fn insert_sealed(
    conn: &mut PgConnection,
    key: &QueueKey,
    user_id: Uuid,
    purpose: MailPurpose,
    proof_digest: Option<&[u8]>,
    expires_at: Option<DateTime<Utc>>,
    plaintext: &[u8],
) -> Result<Uuid, EnqueueError> {
    let id = Uuid::new_v4();
    let sealed = crypto::seal(key, id, purpose.as_str(), plaintext)?;
    let authority = purpose.authority_kind().map(AuthorityKind::as_str);
    sqlx::query!(
        "INSERT INTO mail_jobs (id, user_id, purpose, proof_digest, authority_kind, enqueued_at, \
         next_attempt_at, expires_at, nonce, payload) \
         SELECT $1::uuid, $2::uuid, $3::text, $4::bytea, $5::text, c.t, c.t, \
                COALESCE($6::timestamptz, c.t + interval '24 hours'), $7::bytea, $8::bytea \
           FROM (SELECT clock_timestamp() AS t) c",
        id,
        user_id,
        purpose.as_str(),
        proof_digest,
        authority,
        expires_at,
        sealed.nonce,
        sealed.ciphertext,
    )
    .execute(conn)
    .await?;
    Ok(id)
}

/// Queue another delivery of a live proof by reopening the newest retained
/// payload for it and sealing it again under a new job UUID and nonce.
/// Returns `None` when no payload is retained.
///
/// # Errors
/// Returns [`EnqueueError::Crypto`] when the retained payload cannot be
/// opened, or a database error, including a second live delivery of the proof.
pub async fn resend(
    conn: &mut PgConnection,
    key: &QueueKey,
    user_id: Uuid,
    proof_digest: &[u8],
) -> Result<Option<Uuid>, EnqueueError> {
    let Some(source) = sqlx::query!(
        "SELECT id, purpose, expires_at, nonce AS \"nonce!\", payload AS \"payload!\" \
           FROM mail_jobs \
          WHERE user_id = $1 AND proof_digest = $2 AND payload IS NOT NULL \
          ORDER BY enqueued_at DESC, id DESC LIMIT 1",
        user_id,
        proof_digest,
    )
    .fetch_optional(&mut *conn)
    .await?
    else {
        return Ok(None);
    };
    let purpose = MailPurpose::parse(&source.purpose).ok_or(CryptoError::Open)?;
    let plaintext = crypto::open(
        key,
        source.id,
        &source.purpose,
        &source.nonce,
        &source.payload,
    )?;
    insert_sealed(
        conn,
        key,
        user_id,
        purpose,
        Some(proof_digest),
        Some(source.expires_at),
        &plaintext,
    )
    .await
    .map(Some)
}

/// Cancel unsent mail and drop retained payloads for the given proofs.
/// A message already in flight cannot be recalled; its result write is ignored.
///
/// # Errors
/// Returns database errors from the update.
pub async fn revoke_for_proofs(
    conn: &mut PgConnection,
    user_id: Uuid,
    digests: &[Vec<u8>],
) -> Result<u64, sqlx::Error> {
    Ok(sqlx::query!(
        "UPDATE mail_jobs SET \
            state = CASE WHEN state IN ('pending', 'in_progress') THEN 'cancelled' ELSE state END, \
            terminal_at = CASE WHEN state IN ('pending', 'in_progress') \
                               THEN clock_timestamp() ELSE terminal_at END, \
            nonce = NULL, payload = NULL \
          WHERE user_id = $1 AND proof_digest = ANY($2) \
            AND (state IN ('pending', 'in_progress') OR payload IS NOT NULL)",
        user_id,
        digests,
    )
    .execute(conn)
    .await?
    .rows_affected())
}

/// Cancel unsent mail and drop retained payloads for every proof of one kind
/// the account holds, for example when its credential link is replaced.
///
/// # Errors
/// Returns database errors from the update.
pub async fn revoke_for_authority(
    conn: &mut PgConnection,
    user_id: Uuid,
    kind: AuthorityKind,
) -> Result<u64, sqlx::Error> {
    Ok(sqlx::query!(
        "UPDATE mail_jobs SET \
            state = CASE WHEN state IN ('pending', 'in_progress') THEN 'cancelled' ELSE state END, \
            terminal_at = CASE WHEN state IN ('pending', 'in_progress') \
                               THEN clock_timestamp() ELSE terminal_at END, \
            nonce = NULL, payload = NULL \
          WHERE user_id = $1 AND authority_kind = $2 \
            AND (state IN ('pending', 'in_progress') OR payload IS NOT NULL)",
        user_id,
        kind.as_str(),
    )
    .execute(conn)
    .await?
    .rows_affected())
}

/// The pending or in-flight delivery of a proof, if any.
///
/// # Errors
/// Returns database errors from the lookup.
pub async fn live_delivery(
    conn: &mut PgConnection,
    user_id: Uuid,
    proof_digest: &[u8],
) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar!(
        "SELECT id FROM mail_jobs \
          WHERE user_id = $1 AND proof_digest = $2 AND state IN ('pending', 'in_progress')",
        user_id,
        proof_digest,
    )
    .fetch_optional(conn)
    .await
}

/// A job's state and, once failed, its safe failure category.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobStatus {
    /// Lifecycle state.
    pub state: JobState,
    /// Why the last attempt failed, when it did.
    pub failure_category: Option<String>,
}

/// The status of an administrator-link delivery that belongs to `user_id`.
///
/// # Errors
/// Returns database errors from the lookup, or a decode error for an unknown state.
pub async fn admin_link_status(
    pool: &PgPool,
    user_id: Uuid,
    job_id: Uuid,
) -> Result<Option<JobStatus>, sqlx::Error> {
    let Some(row) = sqlx::query!(
        r#"SELECT CASE WHEN EXISTS (
                    SELECT 1 FROM credential_links l JOIN users u ON u.id = l.user_id
                     WHERE l.user_id = j.user_id AND l.token_hash = j.proof_digest
                       AND l.expires_at > clock_timestamp()
                       AND u.disabled_at IS NULL AND u.email IS NOT NULL
                 ) THEN j.state ELSE 'cancelled' END AS "state!", j.failure_category
             FROM mail_jobs j
            WHERE j.id = $1 AND j.user_id = $2 AND j.purpose = 'admin_link'"#,
        job_id,
        user_id,
    )
    .fetch_optional(pool)
    .await?
    else {
        return Ok(None);
    };
    let state = JobState::parse(&row.state)
        .ok_or_else(|| sqlx::Error::Decode("unknown mail job state".into()))?;
    Ok(Some(JobStatus {
        state,
        failure_category: match state {
            JobState::Failed => row.failure_category,
            _ => None,
        },
    }))
}

/// The outcome of an allowance check.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Admission {
    /// A delivery of this proof is already waiting or in flight.
    Coalesce,
    /// A new delivery may be queued.
    Allowed,
    /// The previous delivery was queued less than [`REQUEST_INTERVAL`] ago.
    TooSoon,
    /// [`RECOVERY_DAILY_LIMIT`] deliveries were queued within [`ROLLING_WINDOW`].
    DailyLimit,
    /// The proof has already had [`PROOF_DELIVERY_LIMIT`] deliveries.
    ProofLimit,
}

struct Usage {
    live_delivery: bool,
    last_enqueued: Option<DateTime<Utc>>,
    recent: i64,
    for_proof: i64,
}

/// The database clock, for decisions taken after the account lock.
///
/// # Errors
/// Returns database errors from the query.
pub async fn database_now(conn: &mut PgConnection) -> Result<DateTime<Utc>, sqlx::Error> {
    sqlx::query_scalar!(r#"SELECT clock_timestamp() AS "now!""#)
        .fetch_one(conn)
        .await
}

/// Decide whether a delivery of `purpose` may be queued for `proof_digest`
/// (the account's live proof, if any) using the database clock.
///
/// Only email recovery and verification are limited; other purposes are always
/// allowed. Counts come from retained job rows, including failed and cancelled
/// ones, and are unaffected by proof deletion.
///
/// # Errors
/// Returns database errors from the queries.
pub async fn admission(
    conn: &mut PgConnection,
    user_id: Uuid,
    purpose: MailPurpose,
    proof_digest: Option<&[u8]>,
) -> Result<Admission, sqlx::Error> {
    let now = database_now(conn).await?;
    admission_at(conn, user_id, purpose, proof_digest, now).await
}

/// [`admission`] against an explicit instant.
///
/// # Errors
/// Returns database errors from the query.
pub async fn admission_at(
    conn: &mut PgConnection,
    user_id: Uuid,
    purpose: MailPurpose,
    proof_digest: Option<&[u8]>,
    now: DateTime<Utc>,
) -> Result<Admission, sqlx::Error> {
    let public_recovery = purpose == MailPurpose::EmailRecovery;
    if !public_recovery && purpose != MailPurpose::Verification {
        return Ok(Admission::Allowed);
    }
    let usage = sqlx::query_as!(
        Usage,
        r#"SELECT COALESCE(bool_or(state IN ('pending', 'in_progress'))
                           FILTER (WHERE proof_digest = $3::bytea), FALSE) AS "live_delivery!",
                  max(enqueued_at) AS last_enqueued,
                  count(*) FILTER (WHERE enqueued_at > $4::timestamptz)
                      AS "recent!",
                  count(*) FILTER (WHERE proof_digest = $3::bytea) AS "for_proof!"
             FROM mail_jobs WHERE user_id = $1 AND purpose = $2"#,
        user_id,
        purpose.as_str(),
        proof_digest,
        now - ROLLING_WINDOW,
    )
    .fetch_one(conn)
    .await?;
    if usage.live_delivery {
        return Ok(Admission::Coalesce);
    }
    if usage
        .last_enqueued
        .is_some_and(|last| now - last < REQUEST_INTERVAL)
    {
        return Ok(Admission::TooSoon);
    }
    if public_recovery && usage.recent >= RECOVERY_DAILY_LIMIT {
        return Ok(Admission::DailyLimit);
    }
    if public_recovery && usage.for_proof >= PROOF_DELIVERY_LIMIT {
        return Ok(Admission::ProofLimit);
    }
    Ok(Admission::Allowed)
}

/// Accounts examined by one [`sweep_batch`] and the cursor to continue from.
pub struct SweepBatch {
    /// Last account examined; pass it as `after` for the next batch.
    pub last_user: Option<Uuid>,
    /// Accounts examined, including any skipped because their lock was held.
    pub accounts: usize,
}

/// Clean up mail state for up to `batch` accounts after `after`.
///
/// Expired proofs and payloads are removed, stale pending jobs expire and
/// terminal rows older than seven days are deleted. Account locks are taken
/// first; locked accounts are skipped and picked up by a later sweep.
///
/// # Errors
/// Returns database errors; the batch's transaction rolls back.
pub async fn sweep_batch(
    pool: &PgPool,
    after: Option<Uuid>,
    batch: i64,
) -> Result<SweepBatch, sqlx::Error> {
    let candidates = sqlx::query_scalar!(
        r#"SELECT user_id AS "user_id!" FROM (
               SELECT user_id FROM account_email_proofs WHERE expires_at <= clock_timestamp()
               UNION
               SELECT j.user_id FROM mail_jobs j
                WHERE (j.state = 'pending' AND j.expires_at <= clock_timestamp())
                   OR (j.payload IS NOT NULL AND j.state <> 'in_progress'
                       AND j.expires_at <= clock_timestamp())
                   OR j.terminal_at <= clock_timestamp() - interval '7 days'
                   OR (j.payload IS NOT NULL AND j.authority_kind IN ('verification', 'recovery')
                       AND NOT EXISTS (
                                    SELECT 1 FROM account_email_proofs p
                                     WHERE p.user_id = j.user_id AND p.token_hash = j.proof_digest
                                       AND p.expires_at > clock_timestamp()))
           ) c
            WHERE ($1::uuid IS NULL OR user_id > $1)
            ORDER BY user_id LIMIT $2"#,
        after,
        batch,
    )
    .fetch_all(pool)
    .await?;
    let Some(&last_user) = candidates.last() else {
        return Ok(SweepBatch {
            last_user: None,
            accounts: 0,
        });
    };
    let mut tx = pool.begin().await?;
    let locked = sqlx::query_scalar!(
        "SELECT id FROM users WHERE id = ANY($1) ORDER BY id FOR NO KEY UPDATE SKIP LOCKED",
        &candidates,
    )
    .fetch_all(&mut *tx)
    .await?;
    sqlx::query!(
        "DELETE FROM account_email_proofs \
          WHERE user_id = ANY($1) AND expires_at <= clock_timestamp()",
        &locked,
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query!(
        "UPDATE mail_jobs SET state = 'expired', terminal_at = clock_timestamp(), \
                nonce = NULL, payload = NULL \
          WHERE user_id = ANY($1) AND state = 'pending' AND expires_at <= clock_timestamp()",
        &locked,
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query!(
        "UPDATE mail_jobs j SET \
            state = CASE WHEN j.state IN ('pending', 'in_progress') \
                         THEN 'cancelled' ELSE j.state END, \
            terminal_at = CASE WHEN j.state IN ('pending', 'in_progress') \
                               THEN clock_timestamp() ELSE j.terminal_at END, \
            nonce = NULL, payload = NULL \
          WHERE j.user_id = ANY($1) AND j.payload IS NOT NULL \
            AND ((j.state <> 'in_progress' AND j.expires_at <= clock_timestamp()) \
                 OR (j.authority_kind IN ('verification', 'recovery') \
                     AND NOT EXISTS ( \
                                  SELECT 1 FROM account_email_proofs p \
                                   WHERE p.user_id = j.user_id AND p.token_hash = j.proof_digest \
                                     AND p.expires_at > clock_timestamp())))",
        &locked,
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query!(
        "DELETE FROM mail_jobs \
          WHERE user_id = ANY($1) AND terminal_at <= clock_timestamp() - interval '7 days'",
        &locked,
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(SweepBatch {
        last_user: Some(last_user),
        accounts: candidates.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::account_email_proof::{self, ProofPurpose, lock_account};
    use crate::test_support::db::{app_pool_for, readonly_pool_for};
    use crate::test_support::mail::{
        account, backdate, digest, enqueue_bearer, enqueue_notice, key, other_key, payload,
        payload_present, state_of,
    };

    const ADDRESS: &str = "reader@example.org";

    #[sqlx::test(migrations = "./migrations")]
    async fn smtp_queue_enqueue_seals_recipient_and_message_under_job_and_purpose(pool: PgPool) {
        let app = app_pool_for(&pool).await;
        let user = account(&pool, ADDRESS).await;
        let first = enqueue_notice(&app, user, MailPurpose::NoticePasswordSet, ADDRESS).await;
        let second = enqueue_notice(&app, user, MailPurpose::NoticePasswordSet, ADDRESS).await;
        let rows = sqlx::query!(
            r#"SELECT id, purpose, state, attempt_count, nonce AS "nonce!", payload AS "payload!",
                      EXTRACT(EPOCH FROM expires_at - enqueued_at)::float8 AS "lifetime!",
                      enqueued_at = next_attempt_at AS "due_at_enqueue!"
                 FROM mail_jobs ORDER BY enqueued_at"#
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(rows.len(), 2);
        assert_ne!(rows[0].nonce, rows[1].nonce);
        for row in &rows {
            assert_eq!(row.state, "pending");
            assert_eq!(row.attempt_count, 0);
            assert_eq!(row.nonce.len(), 12);
            assert!(row.due_at_enqueue);
            assert!((row.lifetime - 86_400.0).abs() < f64::EPSILON);
            let clear = String::from_utf8_lossy(&row.payload);
            assert!(!clear.contains(ADDRESS) && !clear.contains("Body-marker"));
            let opened =
                crypto::open(&key(), row.id, &row.purpose, &row.nonce, &row.payload).unwrap();
            let opened: MailPayload = serde_json::from_slice(&opened).unwrap();
            assert_eq!(opened.recipient, ADDRESS);
            assert_eq!(opened.body, payload(ADDRESS).body);
            assert!(crypto::open(&key(), row.id, "admin_link", &row.nonce, &row.payload).is_err());
            assert!(
                crypto::open(&other_key(), row.id, &row.purpose, &row.nonce, &row.payload).is_err()
            );
        }
        assert_ne!(first, second);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn smtp_queue_tables_belong_to_the_app_role_alone(pool: PgPool) {
        let app = app_pool_for(&pool).await;
        let readonly = readonly_pool_for(&pool).await;
        assert!(
            sqlx::query!("SELECT count(*) FROM mail_jobs")
                .fetch_one(&app)
                .await
                .is_ok()
        );
        assert!(
            sqlx::query!("SELECT count(*) FROM account_email_proofs")
                .fetch_one(&app)
                .await
                .is_ok()
        );
        assert!(
            sqlx::query!("SELECT count(*) FROM mail_jobs")
                .fetch_one(&readonly)
                .await
                .is_err()
        );
        assert!(
            sqlx::query!("SELECT count(*) FROM account_email_proofs")
                .fetch_one(&readonly)
                .await
                .is_err()
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn smtp_queue_resend_reseals_the_retained_message_under_a_new_job(pool: PgPool) {
        let app = app_pool_for(&pool).await;
        let user = account(&pool, ADDRESS).await;
        let mut tx = app.begin().await.unwrap();
        lock_account(&mut tx, user).await.unwrap();
        let proof = account_email_proof::issue(&mut tx, user, ProofPurpose::Recovery, ADDRESS)
            .await
            .unwrap();
        let first = enqueue(
            &mut tx,
            &key(),
            &NewJob {
                user_id: user,
                purpose: MailPurpose::EmailRecovery,
                proof_digest: Some(&proof.token_hash),
                expires_at: Some(proof.expires_at),
                payload: &payload(ADDRESS),
            },
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        let mut conn = app.acquire().await.unwrap();
        assert!(matches!(
            resend(&mut conn, &key(), user, &proof.token_hash).await,
            Err(EnqueueError::Database(_))
        ));
        sqlx::query!(
            "UPDATE mail_jobs SET state = 'accepted', terminal_at = now() WHERE id = $1",
            first
        )
        .execute(&pool)
        .await
        .unwrap();
        let mut conn = app.acquire().await.unwrap();
        let second = resend(&mut conn, &key(), user, &proof.token_hash)
            .await
            .unwrap()
            .unwrap();
        assert_ne!(first, second);
        let rows = sqlx::query!(
            r#"SELECT id, purpose, nonce AS "nonce!", payload AS "payload!", expires_at
                 FROM mail_jobs ORDER BY enqueued_at"#
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(rows.len(), 2);
        assert_ne!(rows[0].nonce, rows[1].nonce);
        assert_eq!(rows[0].expires_at, rows[1].expires_at);
        let plain: Vec<_> = rows
            .iter()
            .map(|row| {
                crypto::open(&key(), row.id, &row.purpose, &row.nonce, &row.payload).unwrap()
            })
            .collect();
        assert_eq!(plain[0], plain[1]);
        revoke_for_proofs(&mut conn, user, std::slice::from_ref(&proof.token_hash))
            .await
            .unwrap();
        assert!(
            resend(&mut conn, &key(), user, &proof.token_hash)
                .await
                .unwrap()
                .is_none()
        );
        assert!(!payload_present(&pool, first).await);
    }

    async fn sweep_all(app: &PgPool) {
        let mut after = None;
        loop {
            let batch = sweep_batch(app, after, 2).await.unwrap();
            if batch.accounts < 2 {
                break;
            }
            after = batch.last_user;
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn smtp_queue_sweep_cancels_mail_whose_proof_has_gone(pool: PgPool) {
        let app = app_pool_for(&pool).await;
        let user = account(&pool, ADDRESS).await;
        let mut tx = app.begin().await.unwrap();
        lock_account(&mut tx, user).await.unwrap();
        let proof = account_email_proof::issue(&mut tx, user, ProofPurpose::Verification, ADDRESS)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        let orphaned = enqueue_bearer(
            &app,
            user,
            MailPurpose::Verification,
            &proof.token_hash,
            TimeDelta::hours(24),
            ADDRESS,
        )
        .await;
        let kept = enqueue_bearer(
            &app,
            user,
            MailPurpose::EmailRecovery,
            &digest(5),
            TimeDelta::minutes(15),
            ADDRESS,
        )
        .await;
        sqlx::query!("DELETE FROM account_email_proofs WHERE user_id = $1", user)
            .execute(&pool)
            .await
            .unwrap();
        sweep_all(&app).await;
        assert_eq!(state_of(&pool, orphaned).await, "cancelled");
        assert!(!payload_present(&pool, orphaned).await);
        assert_eq!(
            state_of(&pool, kept).await,
            "cancelled",
            "no recovery proof exists for it either"
        );
    }
}
