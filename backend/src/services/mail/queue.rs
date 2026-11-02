//! The durable mail queue: the runtime handle, claiming, outcome recording and
//! the single supervised submission worker.
//!
//! The worker owns delivery state. Claim transactions are single statements
//! that end before any account is read or SMTP is attempted, and no
//! transaction or lock is held across a submission.

use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use lettre::{Address, message::Mailbox};
use sqlx::PgPool;
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::FailureCategory;
use super::crypto::{self, QueueKey};
use super::transport::{self, OutgoingMail};
use crate::config::{Config, ConfigError, MailTransportConfig};
use crate::models::mail_job::{AuthorityKind, MAX_ATTEMPTS, MailPayload, MailPurpose};

/// Public recovery tasks that may run at once.
const ADMISSION_PERMITS: u32 = 4;
/// Time budget for one background task, database waits included.
pub const BACKGROUND_BUDGET: Duration = Duration::from_secs(5);
/// How long the worker sleeps when nothing is due.
pub const POLL_INTERVAL: Duration = Duration::from_secs(5);
/// Seconds to wait after the first, second, third and fourth failed attempt.
const RETRY_DELAY_SECS: [i32; 4] = [30, 120, 300, 900];
const EXPIRY_BATCH: i64 = 100;

/// Mail configuration resolved for the running process: the queue key, the
/// transport settings and the bounded capacity for background recovery work.
pub struct MailRuntime {
    key: QueueKey,
    transport: MailTransportConfig,
    local_auth_enabled: bool,
    admission: Arc<Semaphore>,
    shutdown: CancellationToken,
}

impl MailRuntime {
    /// Build the runtime from validated configuration; `None` when mail is disabled.
    ///
    /// # Errors
    /// Returns [`ConfigError`] naming the setting when enabled mail is incomplete.
    pub fn from_config(config: &Config) -> Result<Option<Self>, ConfigError> {
        if !config.mail.enabled {
            return Ok(None);
        }
        Ok(Some(Self::new(
            config.mail.queue_key()?,
            config.mail.transport()?,
            config.local_auth_enabled,
        )))
    }

    /// Assemble a runtime from already validated parts.
    #[must_use]
    pub fn new(key: QueueKey, transport: MailTransportConfig, local_auth_enabled: bool) -> Self {
        Self {
            key,
            transport,
            local_auth_enabled,
            admission: Arc::new(Semaphore::new(ADMISSION_PERMITS as usize)),
            shutdown: CancellationToken::new(),
        }
    }

    /// The key payloads are sealed under.
    #[must_use]
    pub const fn key(&self) -> &QueueKey {
        &self.key
    }

    /// Whether password recovery by email is currently available.
    #[must_use]
    pub const fn local_auth_enabled(&self) -> bool {
        self.local_auth_enabled
    }

    /// The relay, sender and support settings.
    #[must_use]
    pub const fn transport(&self) -> &MailTransportConfig {
        &self.transport
    }

    /// Run `work` in the background if one of the four permits is free, never
    /// waiting for one. The task is dropped after [`BACKGROUND_BUDGET`] or at
    /// shutdown. Returns whether the work was admitted; refused work is not retried.
    pub fn try_spawn<F>(&self, work: F) -> bool
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        if self.shutdown.is_cancelled() {
            return false;
        }
        let Ok(permit) = Arc::clone(&self.admission).try_acquire_owned() else {
            return false;
        };
        let stop = self.shutdown.clone();
        tokio::spawn(async move {
            tokio::select! {
                () = stop.cancelled() => tracing::debug!("background mail task stopped at shutdown"),
                result = tokio::time::timeout(BACKGROUND_BUDGET, work) => {
                    if result.is_err() {
                        tracing::warn!("background mail task exceeded its time budget");
                    }
                }
            }
            drop(permit);
        });
        true
    }

    /// Refuse further background work, cancel what is running and wait for it
    /// to finish, bounded by [`BACKGROUND_BUDGET`].
    pub async fn shutdown(&self) {
        self.shutdown.cancel();
        let drained = tokio::time::timeout(
            BACKGROUND_BUDGET,
            self.admission.acquire_many(ADMISSION_PERMITS),
        )
        .await;
        if drained.is_err() {
            tracing::warn!("background mail tasks did not finish before shutdown");
        }
    }
}

/// A job the worker owns for one attempt. `Debug` shows only its identity.
pub struct ClaimedJob {
    /// Job UUID, also the source of the Message-ID.
    pub id: Uuid,
    /// Owning account.
    pub user_id: Uuid,
    /// Why the message is sent.
    pub purpose: MailPurpose,
    /// Digest of the bearer token the message carries.
    pub proof_digest: Option<Vec<u8>>,
    /// This attempt's number, already persisted.
    pub attempt: i32,
    /// When the job stops being useful.
    pub expires_at: DateTime<Utc>,
    nonce: Vec<u8>,
    payload: Vec<u8>,
}

impl std::fmt::Debug for ClaimedJob {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClaimedJob")
            .field("id", &self.id)
            .field("purpose", &self.purpose)
            .field("attempt", &self.attempt)
            .finish_non_exhaustive()
    }
}

/// Move jobs whose lifetime ended before acceptance to `expired`, clearing their payloads.
///
/// # Errors
/// Returns database errors from the update.
pub async fn expire_due(pool: &PgPool) -> Result<u64, sqlx::Error> {
    Ok(sqlx::query!(
        "UPDATE mail_jobs SET state = 'expired', terminal_at = clock_timestamp(), \
                nonce = NULL, payload = NULL \
          WHERE id IN (SELECT id FROM mail_jobs \
                        WHERE state = 'pending' AND expires_at <= clock_timestamp() \
                        ORDER BY expires_at LIMIT $1 FOR UPDATE SKIP LOCKED)",
        EXPIRY_BATCH,
    )
    .execute(pool)
    .await?
    .rows_affected())
}

/// Claim the next due job and persist the attempt in the same statement.
/// Notices come before other mail, then enqueue time, then job UUID.
///
/// # Errors
/// Returns database errors from the claim.
pub async fn claim_next(pool: &PgPool) -> Result<Option<ClaimedJob>, sqlx::Error> {
    let Some(row) = sqlx::query!(
        r#"WITH next AS (
               SELECT id FROM mail_jobs
                WHERE state = 'pending' AND next_attempt_at <= clock_timestamp()
                  AND expires_at > clock_timestamp()
                ORDER BY (proof_digest IS NULL) DESC, enqueued_at, id
                LIMIT 1 FOR UPDATE SKIP LOCKED)
           UPDATE mail_jobs j SET state = 'in_progress', attempt_count = j.attempt_count + 1
             FROM next WHERE j.id = next.id
           RETURNING j.id, j.user_id, j.purpose, j.proof_digest, j.attempt_count, j.expires_at,
                     j.nonce AS "nonce!", j.payload AS "payload!""#,
    )
    .fetch_optional(pool)
    .await?
    else {
        return Ok(None);
    };
    let purpose = MailPurpose::parse(&row.purpose)
        .ok_or_else(|| sqlx::Error::Decode("unknown mail purpose".into()))?;
    Ok(Some(ClaimedJob {
        id: row.id,
        user_id: row.user_id,
        purpose,
        proof_digest: row.proof_digest,
        attempt: row.attempt_count,
        expires_at: row.expires_at,
        nonce: row.nonce,
        payload: row.payload,
    }))
}

/// Return jobs a stopped worker left `in_progress` to the queue. The
/// interrupted attempt stays counted, so a job on its last attempt fails
/// instead. Returns how many jobs were touched.
///
/// # Errors
/// Returns database errors from the updates.
pub async fn reclaim_in_progress(pool: &PgPool) -> Result<u64, sqlx::Error> {
    let exhausted = sqlx::query!(
        "UPDATE mail_jobs SET state = 'failed', terminal_at = clock_timestamp(), \
                failure_category = COALESCE(failure_category, 'timeout'), \
                nonce = CASE WHEN authority_kind IN ('verification', 'recovery') THEN nonce END, \
                payload = CASE WHEN authority_kind IN ('verification', 'recovery') THEN payload END \
          WHERE state = 'in_progress' AND attempt_count >= $1",
        MAX_ATTEMPTS,
    )
    .execute(pool)
    .await?
    .rows_affected();
    let requeued = sqlx::query!(
        "UPDATE mail_jobs SET state = 'pending', next_attempt_at = clock_timestamp() \
          WHERE state = 'in_progress'",
    )
    .execute(pool)
    .await?
    .rows_affected();
    Ok(exhausted + requeued)
}

struct Standing {
    unexpired: bool,
    current_email: Option<String>,
    usable: bool,
    recoverable: bool,
}

async fn standing(pool: &PgPool, job: &ClaimedJob) -> Result<Option<Standing>, sqlx::Error> {
    sqlx::query_as!(
        Standing,
        r#"SELECT ($4::timestamptz > clock_timestamp()) AS "unexpired!",
                  u.email AS current_email,
                  (u.email_verified_at IS NOT NULL
                   AND EXISTS (SELECT 1 FROM local_credentials c WHERE c.user_id = u.id))
                      AS "recoverable!",
                  (u.disabled_at IS NULL AND CASE $2::text
                      WHEN 'credential_link' THEN EXISTS (
                          SELECT 1 FROM credential_links l
                           WHERE l.user_id = u.id AND l.token_hash = $3::bytea
                             AND l.expires_at > clock_timestamp())
                      WHEN 'verification' THEN EXISTS (
                          SELECT 1 FROM account_email_proofs p
                           WHERE p.user_id = u.id AND p.purpose = 'verification'
                             AND p.token_hash = $3::bytea AND p.expires_at > clock_timestamp())
                      WHEN 'recovery' THEN EXISTS (
                          SELECT 1 FROM account_email_proofs p
                           WHERE p.user_id = u.id AND p.purpose = 'recovery'
                             AND p.token_hash = $3::bytea AND p.expires_at > clock_timestamp())
                      ELSE FALSE
                  END) AS "usable!"
             FROM users u WHERE u.id = $1"#,
        job.user_id,
        job.purpose.authority_kind().map(AuthorityKind::as_str),
        job.proof_digest.as_deref(),
        job.expires_at,
    )
    .fetch_optional(pool)
    .await
}

#[derive(Debug, PartialEq, Eq)]
enum Verdict {
    Send,
    Cancel,
    Expire,
}

fn verdict(purpose: MailPurpose, standing: &Standing, local_auth_enabled: bool) -> Verdict {
    if !standing.unexpired {
        return Verdict::Expire;
    }
    let Some(kind) = purpose.authority_kind() else {
        return Verdict::Send;
    };
    let eligible = standing.usable
        && (kind != AuthorityKind::Recovery || (local_auth_enabled && standing.recoverable));
    if eligible {
        Verdict::Send
    } else {
        Verdict::Cancel
    }
}

fn open_mail(key: &QueueKey, job: &ClaimedJob) -> Result<OutgoingMail, FailureCategory> {
    let plaintext = crypto::open(key, job.id, job.purpose.as_str(), &job.nonce, &job.payload)
        .map_err(crypto::CryptoError::category)?;
    let payload = serde_json::from_slice::<MailPayload>(&plaintext)
        .map_err(|_| FailureCategory::PayloadUnreadable)?;
    let address = payload
        .recipient
        .parse::<Address>()
        .map_err(|_| FailureCategory::PayloadUnreadable)?;
    Ok(OutgoingMail {
        recipient: Mailbox::new(None, address),
        subject: payload.subject,
        body: payload.body,
        job_id: Some(job.id),
    })
}

/// Whether a failed submission may be tried again. Only a permanent refusal
/// and an unreadable payload end a job early.
#[must_use]
pub const fn retryable(category: FailureCategory) -> bool {
    !matches!(
        category,
        FailureCategory::PermanentRefusal | FailureCategory::PayloadUnreadable
    )
}

enum Outcome {
    Accepted,
    Cancelled,
    Expired,
    Failed(FailureCategory),
    Retry(FailureCategory),
}

async fn record(pool: &PgPool, job: &ClaimedJob, outcome: &Outcome) -> Result<(), sqlx::Error> {
    let applied = match outcome {
        Outcome::Accepted => {
            sqlx::query!(
                "UPDATE mail_jobs SET state = 'accepted', terminal_at = clock_timestamp(), \
                        failure_category = NULL, \
                        nonce = CASE WHEN authority_kind IN ('verification', 'recovery') THEN nonce END, \
                        payload = CASE WHEN authority_kind IN ('verification', 'recovery') THEN payload END \
                  WHERE id = $1 AND state = 'in_progress' AND attempt_count = $2",
                job.id,
                job.attempt,
            )
            .execute(pool)
            .await?
        }
        Outcome::Cancelled | Outcome::Expired => {
            let state = if matches!(outcome, Outcome::Expired) {
                "expired"
            } else {
                "cancelled"
            };
            sqlx::query!(
                "UPDATE mail_jobs SET state = $3::text, terminal_at = clock_timestamp(), \
                        nonce = NULL, payload = NULL \
                  WHERE id = $1 AND state = 'in_progress' AND attempt_count = $2",
                job.id,
                job.attempt,
                state,
            )
            .execute(pool)
            .await?
        }
        Outcome::Failed(category) => {
            sqlx::query!(
                "UPDATE mail_jobs SET state = 'failed', terminal_at = clock_timestamp(), \
                        failure_category = $3::text, \
                        nonce = CASE WHEN authority_kind IN ('verification', 'recovery') \
                                      AND $3::text <> 'payload_unreadable' THEN nonce END, \
                        payload = CASE WHEN authority_kind IN ('verification', 'recovery') \
                                        AND $3::text <> 'payload_unreadable' THEN payload END \
                  WHERE id = $1 AND state = 'in_progress' AND attempt_count = $2",
                job.id,
                job.attempt,
                category.as_str(),
            )
            .execute(pool)
            .await?
        }
        Outcome::Retry(category) => {
            let index = usize::try_from(job.attempt - 1).unwrap_or(0);
            let delay = RETRY_DELAY_SECS
                .get(index)
                .copied()
                .unwrap_or(RETRY_DELAY_SECS[RETRY_DELAY_SECS.len() - 1]);
            sqlx::query!(
                "UPDATE mail_jobs SET state = 'pending', failure_category = $3::text, \
                        next_attempt_at = clock_timestamp() + make_interval(secs => $4::int) \
                  WHERE id = $1 AND state = 'in_progress' AND attempt_count = $2",
                job.id,
                job.attempt,
                category.as_str(),
                delay,
            )
            .execute(pool)
            .await?
        }
    };
    if applied.rows_affected() == 0 {
        tracing::info!(job = %job.id, purpose = job.purpose.as_str(), "stale mail result ignored");
    }
    Ok(())
}

/// Where a worker step failed, which decides whether the worker restarts.
#[derive(Debug, thiserror::Error)]
pub enum StepError {
    /// Expiring or claiming failed before any job was owned; retry on the timer.
    #[error("claiming mail failed: {0}")]
    Claim(sqlx::Error),
    /// A claimed job could not be checked or recorded; it stays `in_progress`
    /// until a restart reclaims it.
    #[error("recording a mail outcome failed: {0}")]
    Record(sqlx::Error),
}

/// Claim and deliver at most one job. Returns whether a job was claimed.
///
/// # Errors
/// Returns [`StepError`] for database failures. A cancelled submission leaves
/// its job `in_progress` for [`reclaim_in_progress`].
pub async fn process_next(
    pool: &PgPool,
    runtime: &MailRuntime,
    cancel: &CancellationToken,
) -> Result<bool, StepError> {
    expire_due(pool).await.map_err(StepError::Claim)?;
    let Some(job) = claim_next(pool).await.map_err(StepError::Claim)? else {
        return Ok(false);
    };
    deliver(pool, runtime, cancel, &job)
        .await
        .map_err(StepError::Record)?;
    Ok(true)
}

async fn deliver(
    pool: &PgPool,
    runtime: &MailRuntime,
    cancel: &CancellationToken,
    job: &ClaimedJob,
) -> Result<(), sqlx::Error> {
    let standing = standing(pool, job).await?;
    let verdict = standing.as_ref().map_or(Verdict::Cancel, |standing| {
        verdict(job.purpose, standing, runtime.local_auth_enabled)
    });
    match verdict {
        Verdict::Expire => return record(pool, job, &Outcome::Expired).await,
        Verdict::Cancel => return record(pool, job, &Outcome::Cancelled).await,
        Verdict::Send => {}
    }
    let mail = match open_mail(&runtime.key, job) {
        Ok(mail) => mail,
        Err(category) => return record(pool, job, &Outcome::Failed(category)).await,
    };
    if job.purpose.authority_kind().is_some() {
        // THREAT: A bearer link is only ever sent to the address the account holds now.
        let current = standing.and_then(|standing| standing.current_email);
        let matches = current
            .is_some_and(|current| current.eq_ignore_ascii_case(mail.recipient.email.as_ref()));
        if !matches {
            return record(pool, job, &Outcome::Cancelled).await;
        }
    }
    let result = tokio::select! {
        () = cancel.cancelled() => return Ok(()),
        result = transport::submit(&runtime.transport, &mail) => result,
    };
    let outcome = match result {
        Ok(()) => Outcome::Accepted,
        Err(category) if retryable(category) && job.attempt < MAX_ATTEMPTS => {
            Outcome::Retry(category)
        }
        Err(category) => Outcome::Failed(category),
    };
    match &outcome {
        Outcome::Accepted => {
            tracing::info!(job = %job.id, purpose = job.purpose.as_str(), "mail accepted by relay");
        }
        Outcome::Retry(category) | Outcome::Failed(category) => {
            tracing::warn!(
                job = %job.id,
                purpose = job.purpose.as_str(),
                category = category.as_str(),
                attempt = job.attempt,
                "mail submission failed"
            );
        }
        Outcome::Cancelled | Outcome::Expired => {}
    }
    record(pool, job, &outcome).await
}

/// The supervised worker: reclaim interrupted jobs, then deliver due mail one
/// message at a time until cancelled.
///
/// A failure after a job was claimed ends the run with an error so the
/// supervisor restarts it and the next run reclaims.
///
/// # Errors
/// Returns the database failure or deadline expiry that ended the run.
pub async fn run_worker(
    pool: PgPool,
    runtime: Arc<MailRuntime>,
    cancel: CancellationToken,
    idle_wait: Duration,
) -> anyhow::Result<()> {
    use anyhow::Context as _;

    let mut reclaimed = false;
    while !cancel.is_cancelled() {
        if !reclaimed {
            let result = tokio::select! {
                () = cancel.cancelled() => break,
                result = tokio::time::timeout(transport::SUBMISSION_TIMEOUT, reclaim_in_progress(&pool)) => result,
            };
            match result.context("mail startup reclaim timed out")? {
                Ok(count) => {
                    if count > 0 {
                        tracing::info!(count, "mail jobs interrupted by a stop were reclaimed");
                    }
                    reclaimed = true;
                }
                Err(error) => {
                    tracing::warn!(%error, "mail reclaim failed; retrying on timer");
                    sleep_or_cancel(idle_wait, &cancel).await;
                    continue;
                }
            }
        }
        let result = tokio::time::timeout(
            transport::SUBMISSION_TIMEOUT * 2,
            process_next(&pool, &runtime, &cancel),
        )
        .await;
        match result.context("mail worker step timed out")? {
            Ok(true) => continue,
            Ok(false) => {}
            Err(StepError::Claim(error)) => {
                tracing::warn!(%error, "mail claim failed; retrying on timer");
            }
            Err(error @ StepError::Record(_)) => {
                return Err(error).context("mail worker lost track of a claimed job");
            }
        }
        sleep_or_cancel(idle_wait, &cancel).await;
    }
    tokio::time::timeout(transport::SUBMISSION_TIMEOUT, reclaim_in_progress(&pool))
        .await
        .context("mail shutdown reclaim timed out")?
        .context("returning in-flight mail jobs at shutdown")?;
    Ok(())
}

async fn sleep_or_cancel(duration: Duration, cancel: &CancellationToken) {
    tokio::select! {
        () = cancel.cancelled() => {}
        () = tokio::time::sleep(duration) => {}
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeDelta;

    use super::*;
    use crate::models::account_email_proof::{self, ProofPurpose, lock_account};
    use crate::models::credential_link;
    use crate::test_support::db::app_pool_for;
    use crate::test_support::mail::{
        Behaviour, FakeRelay, account, enqueue_bearer, enqueue_notice, key,
        payload_present, runtime, runtime_with, state_of,
    };

    const ADDRESS: &str = "reader@example.org";

    struct Fixture {
        app: PgPool,
        user: Uuid,
        relay: FakeRelay,
        runtime: Arc<MailRuntime>,
        cancel: CancellationToken,
    }

    async fn fixture(pool: &PgPool, behaviour: Behaviour) -> Fixture {
        let relay = FakeRelay::start(behaviour).await;
        let runtime = Arc::new(runtime(relay.port));
        Fixture {
            app: app_pool_for(pool).await,
            user: account(pool, ADDRESS).await,
            relay,
            runtime,
            cancel: CancellationToken::new(),
        }
    }

    impl Fixture {
        async fn step(&self) -> bool {
            process_next(&self.app, &self.runtime, &self.cancel)
                .await
                .unwrap()
        }

        async fn link(&self) -> (String, Vec<u8>) {
            let mut tx = self.app.begin().await.unwrap();
            lock_account(&mut tx, self.user).await.unwrap();
            let (token, _) = credential_link::issue(
                &mut tx,
                self.user,
                crate::models::credential_link::CREDENTIAL_LINK_TTL,
            )
            .await
            .unwrap();
            tx.commit().await.unwrap();
            let digest = account_email_proof::digest(&token);
            (token, digest)
        }

        async fn admin_mail(&self, digest: &[u8]) -> Uuid {
            enqueue_bearer(
                &self.app,
                self.user,
                MailPurpose::AdminLink,
                digest,
                TimeDelta::hours(72),
                ADDRESS,
            )
            .await
        }

        async fn make_due(&self, pool: &PgPool, id: Uuid) {
            sqlx::query!(
                "UPDATE mail_jobs SET next_attempt_at = clock_timestamp() WHERE id = $1",
                id
            )
            .execute(pool)
            .await
            .unwrap();
        }
    }

    async fn attempts(pool: &PgPool, id: Uuid) -> (i32, Option<String>) {
        let row = sqlx::query!(
            "SELECT attempt_count, failure_category FROM mail_jobs WHERE id = $1",
            id
        )
        .fetch_one(pool)
        .await
        .unwrap();
        (row.attempt_count, row.failure_category)
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn smtp_queue_admin_link_payload_is_removed_at_every_terminal_outcome(pool: PgPool) {
        let f = fixture(&pool, Behaviour::Transient).await;
        let (_, digest) = f.link().await;

        let retried = f.admin_mail(&digest).await;
        assert!(f.step().await);
        assert_eq!(state_of(&pool, retried).await, "pending");
        assert!(
            payload_present(&pool, retried).await,
            "pending attempts keep the payload"
        );
        f.relay.set(Behaviour::Accept);
        f.make_due(&pool, retried).await;
        assert!(f.step().await);
        assert_eq!(state_of(&pool, retried).await, "accepted");
        assert!(!payload_present(&pool, retried).await);

        f.relay.set(Behaviour::Permanent);
        let refused = f.admin_mail(&digest).await;
        assert!(f.step().await);
        assert_eq!(state_of(&pool, refused).await, "failed");
        assert_eq!(
            attempts(&pool, refused).await,
            (1, Some("permanent_refusal".into()))
        );
        assert!(!payload_present(&pool, refused).await);

        f.relay.set(Behaviour::Accept);
        let explicit_retry = f.admin_mail(&digest).await;
        assert!(f.step().await);
        assert_eq!(state_of(&pool, explicit_retry).await, "accepted");
        assert!(!payload_present(&pool, explicit_retry).await);

        let expired = f.admin_mail(&digest).await;
        sqlx::query!(
            "UPDATE mail_jobs SET enqueued_at = now() - interval '2 hours', \
             next_attempt_at = now() - interval '2 hours', expires_at = now() - interval '1 hour' \
             WHERE id = $1",
            expired
        )
        .execute(&pool)
        .await
        .unwrap();
        assert!(!f.step().await);
        assert_eq!(state_of(&pool, expired).await, "expired");
        assert!(!payload_present(&pool, expired).await);

        let cancelled = f.admin_mail(&digest).await;
        let mut tx = f.app.begin().await.unwrap();
        lock_account(&mut tx, f.user).await.unwrap();
        credential_link::cancel(&mut tx, f.user).await.unwrap();
        tx.commit().await.unwrap();
        assert!(
            !f.step().await,
            "cancelling the link withdraws its mail directly"
        );
        assert_eq!(state_of(&pool, cancelled).await, "cancelled");
        assert!(!payload_present(&pool, cancelled).await);
        assert_eq!(f.relay.messages().len(), 4);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn smtp_queue_verification_payload_outlives_acceptance_until_the_proof_is_used(
        pool: PgPool,
    ) {
        let f = fixture(&pool, Behaviour::Accept).await;
        let mut tx = f.app.begin().await.unwrap();
        lock_account(&mut tx, f.user).await.unwrap();
        let proof =
            account_email_proof::issue(&mut tx, f.user, ProofPurpose::Verification, ADDRESS)
                .await
                .unwrap();
        tx.commit().await.unwrap();
        let job = enqueue_bearer(
            &f.app,
            f.user,
            MailPurpose::Verification,
            &proof.token_hash,
            TimeDelta::hours(24),
            ADDRESS,
        )
        .await;
        assert!(f.step().await);
        assert_eq!(state_of(&pool, job).await, "accepted");
        assert!(payload_present(&pool, job).await);
        let mut tx = f.app.begin().await.unwrap();
        lock_account(&mut tx, f.user).await.unwrap();
        assert!(
            account_email_proof::consume(&mut tx, f.user, ProofPurpose::Verification, &proof.token)
                .await
                .unwrap()
                .is_some()
        );
        tx.commit().await.unwrap();
        assert!(!payload_present(&pool, job).await);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn smtp_queue_stale_results_after_cancellation_or_a_newer_attempt_are_ignored(
        pool: PgPool,
    ) {
        let f = fixture(&pool, Behaviour::Accept).await;
        let (_, digest) = f.link().await;
        let job = f.admin_mail(&digest).await;
        let claimed = claim_next(&f.app).await.unwrap().unwrap();
        let mut tx = f.app.begin().await.unwrap();
        lock_account(&mut tx, f.user).await.unwrap();
        crate::models::mail_job::revoke_for_proofs(&mut tx, f.user, std::slice::from_ref(&digest))
            .await
            .unwrap();
        tx.commit().await.unwrap();
        record(&f.app, &claimed, &Outcome::Accepted).await.unwrap();
        record(&f.app, &claimed, &Outcome::Failed(FailureCategory::Timeout))
            .await
            .unwrap();
        record(&f.app, &claimed, &Outcome::Retry(FailureCategory::Timeout))
            .await
            .unwrap();
        assert_eq!(state_of(&pool, job).await, "cancelled");
        assert_eq!(attempts(&pool, job).await.1, None);

        let notice = enqueue_notice(&f.app, f.user, MailPurpose::NoticePasswordSet, ADDRESS).await;
        let old = claim_next(&f.app).await.unwrap().unwrap();
        reclaim_in_progress(&f.app).await.unwrap();
        let newer = claim_next(&f.app).await.unwrap().unwrap();
        assert_eq!((old.id, newer.attempt), (notice, 2));
        record(&f.app, &old, &Outcome::Accepted).await.unwrap();
        assert_eq!(state_of(&pool, notice).await, "in_progress");
        record(&f.app, &newer, &Outcome::Accepted).await.unwrap();
        assert_eq!(state_of(&pool, notice).await, "accepted");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn smtp_queue_notice_goes_to_the_old_address_after_a_change_and_disablement(
        pool: PgPool,
    ) {
        let f = fixture(&pool, Behaviour::Accept).await;
        let job = enqueue_notice(&f.app, f.user, MailPurpose::NoticeAddressChanged, ADDRESS).await;
        sqlx::query!(
            "UPDATE users SET email = 'new@example.org', disabled_at = now(), email_verified_at = NULL \
             WHERE id = $1",
            f.user
        )
        .execute(&pool)
        .await
        .unwrap();
        assert!(f.step().await);
        assert_eq!(state_of(&pool, job).await, "accepted");
        assert_eq!(f.relay.recipients(), ["RCPT TO:<reader@example.org>"]);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn smtp_queue_bearer_mail_is_refused_when_the_account_no_longer_qualifies(pool: PgPool) {
        let cases = [
            "address changed",
            "account disabled",
            "proof removed",
            "recipient differs from the account",
            "address not verified",
            "no password",
        ];
        let f = fixture(&pool, Behaviour::Accept).await;
        for case in cases {
            let name = format!("{}@example.org", case.replace(' ', "-"));
            let user = account(&pool, &name).await;
            let mut tx = f.app.begin().await.unwrap();
            lock_account(&mut tx, user).await.unwrap();
            let proof = account_email_proof::issue(&mut tx, user, ProofPurpose::Recovery, &name)
                .await
                .unwrap();
            tx.commit().await.unwrap();
            sqlx::query!(
                "UPDATE users SET email_verified_at = now() WHERE id = $1",
                user
            )
            .execute(&pool)
            .await
            .unwrap();
            sqlx::query!(
                "INSERT INTO local_credentials (user_id, password_hash) VALUES ($1, 'hash')",
                user
            )
            .execute(&pool)
            .await
            .unwrap();
            let recipient = if case == "recipient differs from the account" {
                "someone-else@example.org".to_owned()
            } else {
                name.clone()
            };
            let job = enqueue_bearer(
                &f.app,
                user,
                MailPurpose::EmailRecovery,
                &proof.token_hash,
                TimeDelta::minutes(15),
                &recipient,
            )
            .await;
            match case {
                "address changed" => {
                    sqlx::query!(
                        "UPDATE users SET email = 'moved@example.org' WHERE id = $1",
                        user
                    )
                    .execute(&pool)
                    .await
                    .unwrap();
                }
                "account disabled" => {
                    sqlx::query!("UPDATE users SET disabled_at = now() WHERE id = $1", user)
                        .execute(&pool)
                        .await
                        .unwrap();
                }
                "proof removed" => {
                    sqlx::query!("DELETE FROM account_email_proofs WHERE user_id = $1", user)
                        .execute(&pool)
                        .await
                        .unwrap();
                }
                "address not verified" => {
                    sqlx::query!(
                        "UPDATE users SET email_verified_at = NULL WHERE id = $1",
                        user
                    )
                    .execute(&pool)
                    .await
                    .unwrap();
                }
                "no password" => {
                    sqlx::query!("DELETE FROM local_credentials WHERE user_id = $1", user)
                        .execute(&pool)
                        .await
                        .unwrap();
                }
                _ => {}
            }
            assert!(f.step().await, "{case}");
            assert_eq!(state_of(&pool, job).await, "cancelled", "{case}");
            assert!(!payload_present(&pool, job).await, "{case}");
            assert!(f.relay.messages().is_empty(), "{case}");
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn smtp_queue_recovery_mail_is_refused_when_local_authentication_is_off(pool: PgPool) {
        let f = fixture(&pool, Behaviour::Accept).await;
        let mut tx = f.app.begin().await.unwrap();
        lock_account(&mut tx, f.user).await.unwrap();
        let proof = account_email_proof::issue(&mut tx, f.user, ProofPurpose::Recovery, ADDRESS)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        sqlx::query!(
            "UPDATE users SET email_verified_at = now() WHERE id = $1",
            f.user
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query!(
            "INSERT INTO local_credentials (user_id, password_hash) VALUES ($1, 'hash')",
            f.user
        )
        .execute(&pool)
        .await
        .unwrap();
        let job = enqueue_bearer(
            &f.app,
            f.user,
            MailPurpose::EmailRecovery,
            &proof.token_hash,
            TimeDelta::minutes(15),
            ADDRESS,
        )
        .await;
        let off = Arc::new(runtime_with(f.relay.port, key(), false));
        assert!(process_next(&f.app, &off, &f.cancel).await.unwrap());
        assert_eq!(state_of(&pool, job).await, "cancelled");

        let again = enqueue_bearer(
            &f.app,
            f.user,
            MailPurpose::EmailRecovery,
            &proof.token_hash,
            TimeDelta::minutes(15),
            ADDRESS,
        )
        .await;
        assert!(
            f.step().await,
            "an eligible account is the positive control"
        );
        assert_eq!(state_of(&pool, again).await, "accepted");
        assert_eq!(f.relay.messages().len(), 1);
    }
}
