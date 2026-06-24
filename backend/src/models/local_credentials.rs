//! Local password credentials — the seam for local-account login.
//!
//! This step ships the table and a read-only lookup only: no password
//! hashing, write, or verification path exists yet. One hash per user (the PK
//! is `user_id`). The hash is a SECRET: the model deliberately does not derive
//! `Serialize` (so it cannot leak through an API by accident), is never
//! logged, and the table is granted to `reverie_app` only (no readonly,
//! mirroring `device_tokens`).

use sqlx::PgPool;
use time::OffsetDateTime;
use uuid::Uuid;

/// A user's local password credential row.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct LocalCredential {
    /// Owning [`crate::models::user::User`]; also the primary key.
    pub user_id: Uuid,
    /// Argon2id PHC string. Never logged; the model is not serialisable.
    pub password_hash: String,
    /// Row insert timestamp.
    pub created_at: OffsetDateTime,
    /// `now()` of the most recent password change.
    pub updated_at: OffsetDateTime,
}

/// Fetch a user's local credential. Returns `Ok(None)` when the user has no
/// password set (OIDC-only account).
///
/// # Errors
///
/// Returns [`sqlx::Error`] from the underlying `SELECT`.
#[allow(dead_code)] // Seam consumed by the local-login step; exercised by tests
pub async fn find_by_user_id(
    pool: &PgPool,
    user_id: Uuid,
) -> Result<Option<LocalCredential>, sqlx::Error> {
    sqlx::query_as!(
        LocalCredential,
        "SELECT user_id, password_hash, created_at, updated_at \
         FROM local_credentials WHERE user_id = $1",
        user_id,
    )
    .fetch_optional(pool)
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn insert_user(pool: &PgPool) -> Uuid {
        sqlx::query_scalar!("INSERT INTO users (display_name) VALUES ('Cred Test') RETURNING id",)
            .fetch_one(pool)
            .await
            .expect("insert user")
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn find_returns_inserted_credential(pool: PgPool) {
        let user_id = insert_user(&pool).await;
        sqlx::query!(
            "INSERT INTO local_credentials (user_id, password_hash) VALUES ($1, $2)",
            user_id,
            "$argon2id$v=19$m=19456,t=2,p=1$fake$fakehash",
        )
        .execute(&pool)
        .await
        .expect("insert credential");

        let found = find_by_user_id(&pool, user_id)
            .await
            .expect("find")
            .expect("credential present");
        assert_eq!(found.user_id, user_id);
        assert!(found.password_hash.starts_with("$argon2id$"));
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn find_returns_none_for_oidc_only_user(pool: PgPool) {
        let user_id = insert_user(&pool).await;
        let found = find_by_user_id(&pool, user_id).await.expect("find");
        assert!(found.is_none(), "user with no password has no credential");
    }
}
