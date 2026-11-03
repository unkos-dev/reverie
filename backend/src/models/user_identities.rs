//! External-provider identity links for the canonical `users` row.
//!
//! Identity is keyed on `(issuer, subject)`: an OIDC `sub` is unique only
//! within its issuer (OIDC Core), so the issuer namespaces the subject. The
//! OIDC provisioning path in [`crate::models::user`] resolves through here
//! instead of `users.oidc_subject`. Mirrors the private-`Row`/public-type
//! split used in [`crate::models::user`].

use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use crate::models::identity_provider::IdentityProvider;

/// Raw row from the database. Use `UserIdentity::from` for the public type.
#[derive(Debug, Clone, sqlx::FromRow)]
struct UserIdentityRow {
    id: Uuid,
    user_id: Uuid,
    provider: IdentityProvider,
    issuer: String,
    subject: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

/// One external-provider identity link bound to a [`crate::models::user::User`].
#[derive(Debug, Clone, Serialize)]
pub struct UserIdentity {
    /// Primary key.
    pub id: Uuid,
    /// Owning canonical user.
    pub user_id: Uuid,
    /// Mechanism backing this link; see [`IdentityProvider`].
    pub provider: IdentityProvider,
    /// Trusted OIDC issuer (`iss`); namespaces `subject`.
    pub issuer: String,
    /// Provider-asserted subject (`sub`), unique only within `issuer`.
    pub subject: String,
    /// Row insert timestamp.
    pub created_at: DateTime<Utc>,
    /// `now()` of the most recent change to this link.
    pub updated_at: DateTime<Utc>,
}

impl From<UserIdentityRow> for UserIdentity {
    fn from(row: UserIdentityRow) -> Self {
        Self {
            id: row.id,
            user_id: row.user_id,
            provider: row.provider,
            issuer: row.issuer,
            subject: row.subject,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

/// Resolve the canonical `user_id` for an OIDC `(issuer, subject)` identity.
/// Returns `Ok(None)` when no link exists yet (a first login).
///
/// Takes an executor so the OIDC upsert can call it inside its transaction,
/// serialised against concurrent same-identity provisioning.
///
/// # Errors
///
/// Returns [`sqlx::Error`] from the underlying `SELECT`.
pub async fn find_user_id_by_oidc(
    executor: impl sqlx::PgExecutor<'_>,
    issuer: &str,
    subject: &str,
) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar!(
        "SELECT user_id FROM user_identities \
         WHERE issuer = $1 AND subject = $2 \
           AND provider = 'oidc'::public.identity_provider",
        issuer,
        subject,
    )
    .fetch_optional(executor)
    .await
}

/// Insert an OIDC identity link for `user_id`.
///
/// # Errors
///
/// Returns [`sqlx::Error`] from the underlying `INSERT`; a duplicate
/// `(issuer, subject)` violates `user_identities_issuer_subject_key`.
pub async fn insert_oidc(
    executor: impl sqlx::PgExecutor<'_>,
    user_id: Uuid,
    issuer: &str,
    subject: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO user_identities (user_id, provider, issuer, subject) \
         VALUES ($1, 'oidc'::public.identity_provider, $2, $3)",
        user_id,
        issuer,
        subject,
    )
    .execute(executor)
    .await
    .map(|_| ())
}

/// Fetch the full identity link for an OIDC `(issuer, subject)`.
///
/// # Errors
///
/// Returns [`sqlx::Error`] from the underlying `SELECT`.
pub async fn find_by_oidc(
    executor: impl sqlx::PgExecutor<'_>,
    issuer: &str,
    subject: &str,
) -> Result<Option<UserIdentity>, sqlx::Error> {
    sqlx::query_as!(
        UserIdentityRow,
        "SELECT id, user_id, provider AS \"provider: IdentityProvider\", \
                issuer, subject, created_at, updated_at \
         FROM user_identities \
         WHERE issuer = $1 AND subject = $2 \
           AND provider = 'oidc'::public.identity_provider",
        issuer,
        subject,
    )
    .fetch_optional(executor)
    .await
    .map(|opt| opt.map(UserIdentity::from))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::PgPool;

    async fn insert_user(pool: &PgPool, subject: &str) -> Uuid {
        sqlx::query_scalar!(
            "INSERT INTO users (display_name) VALUES ($1) RETURNING id",
            subject,
        )
        .fetch_one(pool)
        .await
        .expect("insert user")
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn duplicate_issuer_subject_is_rejected(pool: PgPool) {
        let subject = format!("dup-{}", Uuid::new_v4());
        let issuer = "https://issuer.example.com";
        let user_a = insert_user(&pool, &format!("a-{subject}")).await;
        let user_b = insert_user(&pool, &format!("b-{subject}")).await;

        insert_oidc(&pool, user_a, issuer, &subject)
            .await
            .expect("first insert");
        let err = insert_oidc(&pool, user_b, issuer, &subject)
            .await
            .expect_err("duplicate (issuer, subject) must be rejected");
        assert_eq!(
            err.as_database_error().and_then(|e| e.constraint()),
            Some("user_identities_issuer_subject_key"),
        );
    }
}
