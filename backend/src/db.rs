use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

pub async fn init_pool(database_url: &str, max_connections: u32) -> Result<PgPool, sqlx::Error> {
    PgPoolOptions::new()
        .max_connections(max_connections)
        .connect(database_url)
        .await
}

/// Build a `reverie_app` pool dedicated to the writeback worker.
///
/// Every connection opened by this pool runs
/// `SELECT set_config('app.system_context', 'writeback', false)` once at
/// connect time, marking it as a system-context caller for the duration
/// of the connection.  The `manifestations_*_system` RLS policies match
/// only when this GUC is set to `'writeback'`, so no other code path
/// (in particular, no user-facing handler that forgets `SET LOCAL
/// app.current_user_id`) can reach those policies.
pub async fn init_writeback_pool(
    database_url: &str,
    max_connections: u32,
) -> Result<PgPool, sqlx::Error> {
    PgPoolOptions::new()
        .max_connections(max_connections)
        .after_connect(|conn, _meta| {
            Box::pin(async move {
                sqlx::query("SELECT set_config('app.system_context', 'writeback', false)")
                    .execute(conn)
                    .await?;
                Ok(())
            })
        })
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

    #[sqlx::test(migrations = "./migrations")]
    async fn acquire_with_rls_sets_session_variable(pool: PgPool) {
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
