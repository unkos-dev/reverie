use serde::Serialize;
use sqlx::PgPool;
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct DeviceToken {
    pub id: Uuid,
    pub user_id: Uuid,
    pub name: String,
    #[serde(skip)]
    pub token_hash: String,
    pub last_used_at: Option<OffsetDateTime>,
    pub created_at: OffsetDateTime,
    pub revoked_at: Option<OffsetDateTime>,
}

pub async fn create(
    pool: &PgPool,
    user_id: Uuid,
    name: &str,
    token_hash: &str,
) -> Result<DeviceToken, sqlx::Error> {
    sqlx::query_as::<_, DeviceToken>(
        "INSERT INTO device_tokens (user_id, name, token_hash) \
         VALUES ($1, $2, $3) \
         RETURNING id, user_id, name, token_hash, last_used_at, created_at, revoked_at",
    )
    .bind(user_id)
    .bind(name)
    .bind(token_hash)
    .fetch_one(pool)
    .await
}

/// List active (non-revoked) tokens for a user.
pub async fn list_for_user(pool: &PgPool, user_id: Uuid) -> Result<Vec<DeviceToken>, sqlx::Error> {
    sqlx::query_as::<_, DeviceToken>(
        "SELECT id, user_id, name, token_hash, last_used_at, created_at, revoked_at \
         FROM device_tokens \
         WHERE user_id = $1 AND revoked_at IS NULL \
         ORDER BY created_at DESC",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
}

/// Revoke a token. Scoped to user_id to prevent cross-user revocation.
pub async fn revoke(pool: &PgPool, id: Uuid, user_id: Uuid) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(
        "UPDATE device_tokens SET revoked_at = now() \
         WHERE id = $1 AND user_id = $2 AND revoked_at IS NULL",
    )
    .bind(id)
    .bind(user_id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn count_active_for_user(pool: &PgPool, user_id: Uuid) -> Result<i64, sqlx::Error> {
    let row: (i64,) = sqlx::query_as(
        "SELECT count(*) FROM device_tokens WHERE user_id = $1 AND revoked_at IS NULL",
    )
    .bind(user_id)
    .fetch_one(pool)
    .await?;
    Ok(row.0)
}

pub async fn update_last_used(pool: &PgPool, id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE device_tokens SET last_used_at = now() WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore] // Requires running postgres
    async fn create_list_revoke_lifecycle() {
        let url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://tome_app:tome_app@localhost:5433/tome_dev".into());
        let pool = sqlx::PgPool::connect(&url).await.expect("connect");

        // Create a test user first
        let user: (Uuid,) = sqlx::query_as(
            "INSERT INTO users (oidc_subject, display_name) VALUES ($1, 'Token Test') RETURNING id",
        )
        .bind(format!("token-test-{}", Uuid::new_v4()))
        .fetch_one(&pool)
        .await
        .expect("create user");
        let user_id = user.0;

        // Create token
        let token = create(&pool, user_id, "My Kindle", "fake-hash")
            .await
            .expect("create token");
        assert_eq!(token.name, "My Kindle");
        assert!(token.revoked_at.is_none());

        // List shows the token
        let tokens = list_for_user(&pool, user_id).await.expect("list");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].id, token.id);

        // Revoke
        let revoked = revoke(&pool, token.id, user_id).await.expect("revoke");
        assert!(revoked);

        // List no longer shows revoked token
        let tokens = list_for_user(&pool, user_id)
            .await
            .expect("list after revoke");
        assert!(tokens.is_empty());

        // Cleanup
        sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(user_id)
            .execute(&pool)
            .await
            .expect("cleanup");
    }
}
