//! Queues authentication mail from account transactions.
//!
//! Every method that takes a connection expects the caller to hold the owning
//! user-row lock ([`crate::models::account_email_proof::lock_account`]) inside
//! a transaction, so a failed enqueue rolls back with the account change.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use super::queue::MailRuntime;
use super::render::{self, Context, LinkKind, Notice};
use crate::models::account_email_proof::{self, LockedAccount, ProofPurpose};
use crate::models::mail_job::{self, Admission, EnqueueError, MailPayload, MailPurpose, NewJob};
use crate::models::user;

/// Why mail could not be queued.
#[derive(Debug, thiserror::Error)]
pub enum DispatchError {
    /// Mail is enabled but no public origin is configured to build links from.
    #[error("the public URL is not configured")]
    MissingOrigin,
    /// The job could not be sealed or stored.
    #[error(transparent)]
    Enqueue(#[from] EnqueueError),
    /// A query failed.
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}

/// What a proof request did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Requested {
    /// A delivery was queued.
    Queued(Uuid),
    /// A pending delivery, the interval or an allowance absorbed the request.
    Absorbed,
}

/// The mail runtime together with the origin and support address templates need.
#[derive(Clone)]
pub struct Outbox {
    runtime: Arc<MailRuntime>,
    context: Context,
}

impl Outbox {
    /// The outbox for a running mail runtime, or `None` while mail is disabled.
    ///
    /// # Errors
    /// Returns [`DispatchError::MissingOrigin`] when mail is enabled without a
    /// public URL.
    pub fn new(
        runtime: Option<&Arc<MailRuntime>>,
        public_url: Option<&url::Url>,
    ) -> Result<Option<Self>, DispatchError> {
        let Some(runtime) = runtime else {
            return Ok(None);
        };
        let url = public_url.ok_or(DispatchError::MissingOrigin)?;
        let support = runtime.transport().reply_to.email.to_string();
        Ok(Some(Self {
            runtime: Arc::clone(runtime),
            context: Context {
                origin: url.as_str().trim_end_matches('/').to_owned(),
                support,
            },
        }))
    }

    async fn enqueue_notice(
        &self,
        conn: &mut PgConnection,
        user_id: Uuid,
        event: Notice,
        account_email: &str,
        recipient: &str,
    ) -> Result<Uuid, DispatchError> {
        let message = render::notice(&self.context, event, account_email, Utc::now());
        Ok(mail_job::enqueue(
            conn,
            self.runtime.key(),
            &NewJob {
                user_id,
                purpose: event.purpose(),
                proof_digest: None,
                expires_at: None,
                payload: &MailPayload {
                    recipient: recipient.to_owned(),
                    subject: message.subject,
                    body: message.body,
                },
            },
        )
        .await?)
    }

    /// Queue `event` for the account's current address, but only when Reverie has
    /// verified that address. Returns the job when one was queued.
    ///
    /// # Errors
    /// Returns [`DispatchError`] when sealing or storing the job fails.
    pub async fn notice_to_verified_address(
        &self,
        conn: &mut PgConnection,
        user_id: Uuid,
        account: &LockedAccount,
        event: Notice,
    ) -> Result<Option<Uuid>, DispatchError> {
        let Some(address) = account
            .email
            .as_deref()
            .filter(|_| account.email_verified_at.is_some())
        else {
            return Ok(None);
        };
        self.enqueue_notice(conn, user_id, event, address, address)
            .await
            .map(Some)
    }

    /// Queue a notice for `address`, fixing the recipient now. The caller has
    /// established that the account held and controlled `address` at the event.
    ///
    /// # Errors
    /// Returns [`DispatchError`] when sealing or storing the job fails.
    pub async fn notice_to_address(
        &self,
        conn: &mut PgConnection,
        user_id: Uuid,
        event: Notice,
        address: &str,
    ) -> Result<Uuid, DispatchError> {
        self.enqueue_notice(conn, user_id, event, address, address)
            .await
    }

    /// Queue the administrator's link email to the account's stored address.
    /// `link_expires_at` is the link's own expiry. A delivery of this link that
    /// is already waiting is returned instead of queueing a second.
    ///
    /// # Errors
    /// Returns [`DispatchError`] when sealing or storing the job fails.
    pub async fn admin_link(
        &self,
        conn: &mut PgConnection,
        user_id: Uuid,
        recipient: &str,
        has_password: bool,
        token: &str,
        link_expires_at: DateTime<Utc>,
    ) -> Result<Uuid, DispatchError> {
        let digest = account_email_proof::digest(token);
        if let Some(existing) = mail_job::live_delivery(conn, user_id, &digest).await? {
            return Ok(existing);
        }
        let kind = if has_password {
            LinkKind::Reset
        } else {
            LinkKind::Set
        };
        let message = render::admin_link(&self.context, kind, recipient, token, link_expires_at);
        Ok(mail_job::enqueue(
            conn,
            self.runtime.key(),
            &NewJob {
                user_id,
                purpose: MailPurpose::AdminLink,
                proof_digest: Some(&digest),
                expires_at: Some(link_expires_at),
                payload: &MailPayload {
                    recipient: recipient.to_owned(),
                    subject: message.subject,
                    body: message.body,
                },
            },
        )
        .await?)
    }

    /// Queue a delivery of the account's `purpose` proof for `address`.
    ///
    /// A pending delivery, the 60-second interval and the recovery allowances
    /// absorb the request without a change. Otherwise the live proof is
    /// delivered again from its retained payload, or a new proof is issued.
    ///
    /// # Errors
    /// Returns [`DispatchError`] when the proof, allowance or job cannot be
    /// read or written.
    pub async fn request_proof(
        &self,
        conn: &mut PgConnection,
        user_id: Uuid,
        purpose: ProofPurpose,
        address: &str,
    ) -> Result<Requested, DispatchError> {
        let live = account_email_proof::live(conn, user_id, purpose)
            .await?
            .filter(|proof| proof.address.eq_ignore_ascii_case(address));
        let digest = live.as_ref().map(|proof| proof.token_hash.as_slice());
        let admission = mail_job::admission(conn, user_id, purpose.mail_purpose(), digest).await?;
        if admission != Admission::Allowed {
            return Ok(Requested::Absorbed);
        }
        if let Some(live) = &live
            && let Some(job) =
                mail_job::resend(conn, self.runtime.key(), user_id, &live.token_hash).await?
        {
            return Ok(Requested::Queued(job));
        }
        let issued = account_email_proof::issue(conn, user_id, purpose, address).await?;
        let message = match purpose {
            ProofPurpose::Verification => {
                render::verification(&self.context, address, &issued.token, issued.expires_at)
            }
            ProofPurpose::Recovery => {
                render::recovery(&self.context, address, &issued.token, issued.expires_at)
            }
        };
        let job = mail_job::enqueue(
            conn,
            self.runtime.key(),
            &NewJob {
                user_id,
                purpose: purpose.mail_purpose(),
                proof_digest: Some(&issued.token_hash),
                expires_at: Some(issued.expires_at),
                payload: &MailPayload {
                    recipient: address.to_owned(),
                    subject: message.subject,
                    body: message.body,
                },
            },
        )
        .await?;
        Ok(Requested::Queued(job))
    }

    /// Start a public recovery request in the background without waiting for
    /// capacity. Work refused for lack of capacity is dropped.
    pub fn spawn_recovery(&self, pool: PgPool, address: String) {
        let outbox = self.clone();
        self.runtime.try_spawn(async move {
            if let Err(error) = outbox.recover(&pool, &address).await {
                tracing::warn!(%error, "password recovery request failed");
            }
        });
    }

    /// Check eligibility under the account lock, then issue or reuse a recovery
    /// proof and queue its delivery. An ineligible address leaves no row.
    ///
    /// # Errors
    /// Returns [`DispatchError`] when a query or the enqueue fails.
    pub async fn recover(&self, pool: &PgPool, address: &str) -> Result<Requested, DispatchError> {
        if !self.runtime.local_auth_enabled() {
            return Ok(Requested::Absorbed);
        }
        let Some(found) = user::find_by_email(pool, address).await? else {
            return Ok(Requested::Absorbed);
        };
        let mut tx = pool.begin().await?;
        let Some(account) = account_email_proof::lock_account(&mut tx, found.id).await? else {
            return Ok(Requested::Absorbed);
        };
        let has_password = sqlx::query_scalar!(
            r#"SELECT EXISTS (SELECT 1 FROM local_credentials WHERE user_id = $1) AS "exists!""#,
            found.id,
        )
        .fetch_one(&mut *tx)
        .await?;
        // THREAT: Recovery must never enrol an OIDC-only account or reach an unverified address.
        let current = account
            .email
            .as_deref()
            .filter(|current| current.eq_ignore_ascii_case(address));
        let eligible = !account.disabled && account.email_verified_at.is_some() && has_password;
        let Some(current) = current.filter(|_| eligible) else {
            return Ok(Requested::Absorbed);
        };
        let outcome = self
            .request_proof(&mut tx, found.id, ProofPurpose::Recovery, current)
            .await?;
        tx.commit().await?;
        Ok(outcome)
    }
}
