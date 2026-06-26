//! Local password credentials: the seam for local-account login.
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
///
/// `Debug` is implemented by hand to redact `password_hash`: deriving it would
/// emit the Argon2id PHC through any `?value` tracing span (CWE-532).
#[derive(Clone, sqlx::FromRow)]
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

impl std::fmt::Debug for LocalCredential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LocalCredential")
            .field("user_id", &self.user_id)
            .field("password_hash", &"[REDACTED]")
            .field("created_at", &self.created_at)
            .field("updated_at", &self.updated_at)
            .finish()
    }
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

/// Insert or replace a user's local password credential. Used by first-run
/// setup, the headless env seed, and password reset. The argument is an Argon2id
/// PHC string (see [`crate::auth::password`]); this layer never sees a clear
/// password. On replace, the `trg_local_credentials_updated_at` trigger bumps
/// `updated_at`.
///
/// Takes an executor so the caller can bind it to a transaction (the
/// password-reset flow writes the credential in the same transaction that
/// consumes the PIN and bumps `session_version`) or run it against a pool (the
/// headless env seed). The bootstrap path writes its own transactional insert
/// at the call site alongside the `users` row and the `instance_bootstrap`
/// marker.
///
/// # Errors
///
/// Returns [`sqlx::Error`] from the `INSERT ... ON CONFLICT`.
#[allow(dead_code)] // Consumed by setup/reset/seed in this PR
pub async fn set_password(
    executor: impl sqlx::PgExecutor<'_>,
    user_id: Uuid,
    password_hash: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO local_credentials (user_id, password_hash) VALUES ($1, $2) \
         ON CONFLICT (user_id) DO UPDATE SET password_hash = EXCLUDED.password_hash",
        user_id,
        password_hash,
    )
    .execute(executor)
    .await
    .map(|_| ())
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

    #[sqlx::test(migrations = "./migrations")]
    async fn set_password_inserts_first_credential(pool: PgPool) {
        let user_id = insert_user(&pool).await;
        let hash = "$argon2id$v=19$m=19456,t=2,p=1$first$firsthash";
        set_password(&pool, user_id, hash)
            .await
            .expect("first set_password");

        let found = find_by_user_id(&pool, user_id)
            .await
            .expect("find")
            .expect("credential present after first set_password");
        assert_eq!(found.user_id, user_id);
        assert_eq!(found.password_hash, hash);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn set_password_replaces_on_conflict(pool: PgPool) {
        let user_id = insert_user(&pool).await;
        set_password(&pool, user_id, "$argon2id$v=19$m=19456,t=2,p=1$old$oldhash")
            .await
            .expect("seed credential");
        let before = find_by_user_id(&pool, user_id)
            .await
            .expect("find before")
            .expect("credential present");

        // The updated_at trigger fires on UPDATE; sleep so the replace lands at a
        // strictly later instant and the assertion below proves the bump.
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;

        let new_hash = "$argon2id$v=19$m=19456,t=2,p=1$new$newhash";
        set_password(&pool, user_id, new_hash)
            .await
            .expect("replace credential");

        let after = find_by_user_id(&pool, user_id)
            .await
            .expect("find after")
            .expect("credential present");
        assert_eq!(
            after.password_hash, new_hash,
            "ON CONFLICT replaces the stored hash"
        );
        assert_eq!(
            after.created_at, before.created_at,
            "created_at is preserved across a replace"
        );
        assert!(
            after.updated_at > before.updated_at,
            "the trigger advances updated_at on replace (before={}, after={})",
            before.updated_at,
            after.updated_at
        );
    }
}
