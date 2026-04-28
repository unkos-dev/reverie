//! Reading state — per-`(user, manifestation)` progress and last-read timestamp.
//!
//! Schema lives in migration `20260428000001_activate_reading_state`. Step 11
//! will add the model query layer here. For now this module only carries
//! schema-level tests against the migration.

#[cfg(test)]
mod tests {
    use crate::db::acquire_with_rls;
    use crate::test_support::db::{
        app_pool_for, create_adult_and_basic_auth, ingestion_pool_for,
        insert_work_and_manifestation,
    };
    use sqlx::PgPool;
    use time::OffsetDateTime;
    use uuid::Uuid;

    /// Create one user + one manifestation, return their ids.
    /// Owner-pool inserts are fine — the schema-owner pool bypasses RLS.
    async fn fixture(pool: &PgPool, marker: &str) -> (Uuid, Uuid) {
        let ingestion = ingestion_pool_for(pool).await;
        let (_work_id, m_id) = insert_work_and_manifestation(&ingestion, marker).await;
        let app = app_pool_for(pool).await;
        let (user_id, _) = create_adult_and_basic_auth(&app, marker).await;
        (m_id, user_id)
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn both_null_is_valid_sentinel(pool: PgPool) {
        let (m_id, user_id) = fixture(&pool, "sentinel").await;
        sqlx::query("INSERT INTO reading_state (user_id, manifestation_id) VALUES ($1, $2)")
            .bind(user_id)
            .bind(m_id)
            .execute(&pool)
            .await
            .expect("both-null sentinel insert");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn both_set_is_valid(pool: PgPool) {
        let (m_id, user_id) = fixture(&pool, "both-set").await;
        sqlx::query(
            "INSERT INTO reading_state (user_id, manifestation_id, progress_pct, last_read_at) \
             VALUES ($1, $2, $3, $4)",
        )
        .bind(user_id)
        .bind(m_id)
        .bind(50.0_f32)
        .bind(OffsetDateTime::now_utc())
        .execute(&pool)
        .await
        .expect("both-set insert");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn progress_with_null_timestamp_rejected(pool: PgPool) {
        let (m_id, user_id) = fixture(&pool, "pct-null-ts").await;
        let result = sqlx::query(
            "INSERT INTO reading_state (user_id, manifestation_id, progress_pct, last_read_at) \
             VALUES ($1, $2, $3, NULL)",
        )
        .bind(user_id)
        .bind(m_id)
        .bind(50.0_f32)
        .execute(&pool)
        .await;
        assert!(
            result.is_err(),
            "(50, NULL) should violate paired-null CHECK"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn null_progress_with_timestamp_rejected(pool: PgPool) {
        let (m_id, user_id) = fixture(&pool, "null-pct-ts").await;
        let result = sqlx::query(
            "INSERT INTO reading_state (user_id, manifestation_id, progress_pct, last_read_at) \
             VALUES ($1, $2, NULL, $3)",
        )
        .bind(user_id)
        .bind(m_id)
        .bind(OffsetDateTime::now_utc())
        .execute(&pool)
        .await;
        assert!(
            result.is_err(),
            "(NULL, now()) should violate paired-null CHECK"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn progress_below_zero_rejected(pool: PgPool) {
        let (m_id, user_id) = fixture(&pool, "below-zero").await;
        let result = sqlx::query(
            "INSERT INTO reading_state (user_id, manifestation_id, progress_pct, last_read_at) \
             VALUES ($1, $2, $3, $4)",
        )
        .bind(user_id)
        .bind(m_id)
        .bind(-1.0_f32)
        .bind(OffsetDateTime::now_utc())
        .execute(&pool)
        .await;
        assert!(
            result.is_err(),
            "progress_pct = -1 should violate range CHECK"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn progress_above_hundred_rejected(pool: PgPool) {
        let (m_id, user_id) = fixture(&pool, "above-hundred").await;
        let result = sqlx::query(
            "INSERT INTO reading_state (user_id, manifestation_id, progress_pct, last_read_at) \
             VALUES ($1, $2, $3, $4)",
        )
        .bind(user_id)
        .bind(m_id)
        .bind(101.0_f32)
        .bind(OffsetDateTime::now_utc())
        .execute(&pool)
        .await;
        assert!(
            result.is_err(),
            "progress_pct = 101 should violate range CHECK"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn boundaries_zero_and_hundred_accepted(pool: PgPool) {
        let ingestion = ingestion_pool_for(&pool).await;
        let app = app_pool_for(&pool).await;
        let (_w1, m1) = insert_work_and_manifestation(&ingestion, "low-bound").await;
        let (_w2, m2) = insert_work_and_manifestation(&ingestion, "high-bound").await;
        let (user_id, _) = create_adult_and_basic_auth(&app, "boundaries").await;
        let now = OffsetDateTime::now_utc();

        sqlx::query(
            "INSERT INTO reading_state (user_id, manifestation_id, progress_pct, last_read_at) \
             VALUES ($1, $2, 0.0, $3), ($1, $4, 100.0, $3)",
        )
        .bind(user_id)
        .bind(m1)
        .bind(now)
        .bind(m2)
        .execute(&pool)
        .await
        .expect("boundary values 0 and 100 should be accepted");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn duplicate_user_manifestation_rejected(pool: PgPool) {
        let (m_id, user_id) = fixture(&pool, "dup").await;
        sqlx::query("INSERT INTO reading_state (user_id, manifestation_id) VALUES ($1, $2)")
            .bind(user_id)
            .bind(m_id)
            .execute(&pool)
            .await
            .unwrap();
        let result =
            sqlx::query("INSERT INTO reading_state (user_id, manifestation_id) VALUES ($1, $2)")
                .bind(user_id)
                .bind(m_id)
                .execute(&pool)
                .await;
        assert!(
            result.is_err(),
            "duplicate (user, manifestation) should violate PK"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn rls_isolates_users(pool: PgPool) {
        let ingestion = ingestion_pool_for(&pool).await;
        let (_w, m_id) = insert_work_and_manifestation(&ingestion, "rls").await;
        let app = app_pool_for(&pool).await;
        let (alice, _) = create_adult_and_basic_auth(&app, "alice").await;
        let (bob, _) = create_adult_and_basic_auth(&app, "bob").await;

        // Alice writes her own row.
        let mut tx = acquire_with_rls(&app, alice).await.unwrap();
        sqlx::query(
            "INSERT INTO reading_state (user_id, manifestation_id, progress_pct, last_read_at) \
             VALUES ($1, $2, $3, $4)",
        )
        .bind(alice)
        .bind(m_id)
        .bind(42.0_f32)
        .bind(OffsetDateTime::now_utc())
        .execute(&mut *tx)
        .await
        .unwrap();
        tx.commit().await.unwrap();

        // Alice sees one row.
        let mut tx = acquire_with_rls(&app, alice).await.unwrap();
        let alice_count: i64 = sqlx::query_scalar("SELECT count(*) FROM reading_state")
            .fetch_one(&mut *tx)
            .await
            .unwrap();
        assert_eq!(alice_count, 1, "alice sees her own row");
        tx.rollback().await.unwrap();

        // Bob sees zero rows.
        let mut tx = acquire_with_rls(&app, bob).await.unwrap();
        let bob_count: i64 = sqlx::query_scalar("SELECT count(*) FROM reading_state")
            .fetch_one(&mut *tx)
            .await
            .unwrap();
        assert_eq!(bob_count, 0, "bob does not see alice's row");
        tx.rollback().await.unwrap();

        // Bob writing under alice's user_id is blocked by WITH CHECK.
        let mut tx = acquire_with_rls(&app, bob).await.unwrap();
        let result = sqlx::query(
            "INSERT INTO reading_state (user_id, manifestation_id, progress_pct, last_read_at) \
             VALUES ($1, $2, $3, $4)",
        )
        .bind(alice)
        .bind(m_id)
        .bind(99.0_f32)
        .bind(OffsetDateTime::now_utc())
        .execute(&mut *tx)
        .await;
        assert!(
            result.is_err(),
            "bob writing alice's user_id should fail RLS WITH CHECK"
        );
        tx.rollback().await.unwrap();
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn user_delete_cascades(pool: PgPool) {
        let (m_id, user_id) = fixture(&pool, "user-cascade").await;
        sqlx::query(
            "INSERT INTO reading_state (user_id, manifestation_id, progress_pct, last_read_at) \
             VALUES ($1, $2, $3, $4)",
        )
        .bind(user_id)
        .bind(m_id)
        .bind(75.0_f32)
        .bind(OffsetDateTime::now_utc())
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(user_id)
            .execute(&pool)
            .await
            .unwrap();

        let count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM reading_state WHERE user_id = $1")
                .bind(user_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(count, 0, "user delete cascades into reading_state");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn manifestation_delete_cascades(pool: PgPool) {
        let (m_id, user_id) = fixture(&pool, "m-cascade").await;
        sqlx::query(
            "INSERT INTO reading_state (user_id, manifestation_id, progress_pct, last_read_at) \
             VALUES ($1, $2, $3, $4)",
        )
        .bind(user_id)
        .bind(m_id)
        .bind(75.0_f32)
        .bind(OffsetDateTime::now_utc())
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query("DELETE FROM manifestations WHERE id = $1")
            .bind(m_id)
            .execute(&pool)
            .await
            .unwrap();

        let count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM reading_state WHERE manifestation_id = $1")
                .bind(m_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(
            count, 0,
            "manifestation delete cascades into reading_state"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn updated_at_trigger_advances_on_update(pool: PgPool) {
        let (m_id, user_id) = fixture(&pool, "updated-at").await;
        sqlx::query("INSERT INTO reading_state (user_id, manifestation_id) VALUES ($1, $2)")
            .bind(user_id)
            .bind(m_id)
            .execute(&pool)
            .await
            .unwrap();

        let initial: OffsetDateTime = sqlx::query_scalar(
            "SELECT updated_at FROM reading_state WHERE user_id = $1 AND manifestation_id = $2",
        )
        .bind(user_id)
        .bind(m_id)
        .fetch_one(&pool)
        .await
        .unwrap();

        // now() == transaction_timestamp(); separate sqlx queries are separate
        // implicit transactions, but we sleep to make timestamp ordering
        // observable on fast hardware.
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;

        sqlx::query(
            "UPDATE reading_state SET progress_pct = $1, last_read_at = $2 \
             WHERE user_id = $3 AND manifestation_id = $4",
        )
        .bind(33.0_f32)
        .bind(OffsetDateTime::now_utc())
        .bind(user_id)
        .bind(m_id)
        .execute(&pool)
        .await
        .unwrap();

        let updated: OffsetDateTime = sqlx::query_scalar(
            "SELECT updated_at FROM reading_state WHERE user_id = $1 AND manifestation_id = $2",
        )
        .bind(user_id)
        .bind(m_id)
        .fetch_one(&pool)
        .await
        .unwrap();

        assert!(
            updated > initial,
            "updated_at trigger should advance on UPDATE"
        );
    }
}
