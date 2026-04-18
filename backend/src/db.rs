use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

pub async fn init_pool(database_url: &str, max_connections: u32) -> Result<PgPool, sqlx::Error> {
    PgPoolOptions::new()
        .max_connections(max_connections)
        .connect(database_url)
        .await
}

/// Acquire a transaction with RLS context set for the given user.
///
/// Uses `set_config('app.current_user_id', ..., true)` where the third
/// argument `true` means "local to current transaction" (equivalent to
/// `SET LOCAL`). The value auto-resets on commit/rollback — safe with
/// connection pools.
pub async fn acquire_with_rls(
    pool: &PgPool,
    user_id: uuid::Uuid,
) -> Result<sqlx::Transaction<'_, sqlx::Postgres>, sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT set_config('app.current_user_id', $1::text, true)")
        .bind(user_id.to_string())
        .execute(&mut *tx)
        .await?;
    Ok(tx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore] // Requires running postgres: cargo test -- --ignored
    async fn acquire_with_rls_sets_session_variable() {
        // config::tests mutate DATABASE_URL (setting it to a non-existent
        // "test" host), so serialize the env read against them via ENV_LOCK.
        // Scope the guard so it drops before the first await — once the URL
        // is captured, env mutations can't affect this test.
        let url = {
            let _env_guard = crate::test_support::ENV_LOCK.lock().unwrap();
            std::env::var("DATABASE_URL")
                .unwrap_or_else(|_| "postgres://tome_app:tome_app@localhost:5433/tome_dev".into())
        };
        let pool = init_pool(&url, 2).await.expect("failed to connect");

        let user_id = uuid::Uuid::new_v4();
        let mut tx = acquire_with_rls(&pool, user_id).await.unwrap();

        let row: (String,) = sqlx::query_as("SELECT current_setting('app.current_user_id')")
            .fetch_one(&mut *tx)
            .await
            .unwrap();

        assert_eq!(row.0, user_id.to_string());
        tx.rollback().await.unwrap();
    }
}
