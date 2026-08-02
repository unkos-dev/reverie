//! Reading state: per-`(user, manifestation)` status, rating, notes,
//! progress, and reading dates.
//!
//! Schema lives in migration `20260428000001_activate_reading_state`
//! (progress/last-read) extended by `20260703120000_reading_domain`
//! (status/rating/notes/started_at/finished_at). Queries live in
//! `routes::reading` and `routes::library`, not here; this module carries
//! the wire DTOs, plus schema-level tests against the migrations.
//!
//! Wire-format conventions follow the JSON-API conventions ADR
//! (`adr/2026-05-22-json-api-conventions.md`): snake_case field names,
//! `Option<T>` for nullable (never `skip_serializing_if`), RFC 3339
//! timestamps via `time`.

use serde::Serialize;
use time::OffsetDateTime;

use crate::models::reading_status::ReadingStatus;

/// `GET /api/v1/books/{id}/reading` response: the caller's full reading
/// state for one book. A missing `reading_state` row (never written) decodes
/// to all-`None` fields; that all-null shape IS the "unread" domain state,
/// not an error.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
#[non_exhaustive]
pub struct ReadingState {
    /// `reading_state.status`; `None` means unread (no row, or row with a
    /// null status).
    pub status: Option<ReadingStatus>,
    /// `reading_state.rating`, 1-5; `None` means unrated.
    pub rating: Option<i16>,
    /// `reading_state.notes`; free-text, caller-authored.
    pub notes: Option<String>,
    /// `reading_state.progress_pct`, 0-100.
    pub progress_pct: Option<f32>,
    /// `reading_state.started_at`: stamped when a patch first sets `status`
    /// to [`ReadingStatus::Reading`]; not re-stamped on later re-entries.
    #[serde(with = "time::serde::rfc3339::option")]
    pub started_at: Option<OffsetDateTime>,
    /// `reading_state.finished_at`: stamped each time a patch sets `status`
    /// to [`ReadingStatus::Finished`].
    #[serde(with = "time::serde::rfc3339::option")]
    pub finished_at: Option<OffsetDateTime>,
    /// `reading_state.last_read_at`.
    #[serde(with = "time::serde::rfc3339::option")]
    pub last_read_at: Option<OffsetDateTime>,
}

/// Reading-state slice embedded in each `GET /api/v1/books` list row.
/// Batch-loaded alongside the page (see `routes::library::load_authors_for_works`
/// for the sibling batch-load pattern); `None` when the caller has no
/// `reading_state` row for that book (unread).
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
#[non_exhaustive]
pub struct ReadingStateSummary {
    /// `reading_state.status`.
    pub status: Option<ReadingStatus>,
    /// `reading_state.rating`, 1-5.
    pub rating: Option<i16>,
    /// `reading_state.notes`; free-text, caller-authored.
    pub notes: Option<String>,
    /// `reading_state.progress_pct`, 0-100.
    pub progress_pct: Option<f32>,
    /// `reading_state.started_at`; see [`ReadingState::started_at`].
    #[serde(with = "time::serde::rfc3339::option")]
    pub started_at: Option<OffsetDateTime>,
    /// `reading_state.finished_at`; see [`ReadingState::finished_at`].
    #[serde(with = "time::serde::rfc3339::option")]
    pub finished_at: Option<OffsetDateTime>,
}

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
        sqlx::query!(
            "INSERT INTO reading_state (user_id, manifestation_id) VALUES ($1, $2)",
            user_id,
            m_id,
        )
        .execute(&pool)
        .await
        .expect("both-null sentinel insert");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn both_set_is_valid(pool: PgPool) {
        let (m_id, user_id) = fixture(&pool, "both-set").await;
        sqlx::query!(
            "INSERT INTO reading_state (user_id, manifestation_id, progress_pct, last_read_at) \
             VALUES ($1, $2, $3, $4)",
            user_id,
            m_id,
            50.0_f32,
            OffsetDateTime::now_utc(),
        )
        .execute(&pool)
        .await
        .expect("both-set insert");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn progress_with_null_timestamp_rejected(pool: PgPool) {
        let (m_id, user_id) = fixture(&pool, "pct-null-ts").await;
        let result = sqlx::query!(
            "INSERT INTO reading_state (user_id, manifestation_id, progress_pct, last_read_at) \
             VALUES ($1, $2, $3, NULL)",
            user_id,
            m_id,
            50.0_f32,
        )
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
        let result = sqlx::query!(
            "INSERT INTO reading_state (user_id, manifestation_id, progress_pct, last_read_at) \
             VALUES ($1, $2, NULL, $3)",
            user_id,
            m_id,
            OffsetDateTime::now_utc(),
        )
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
        let result = sqlx::query!(
            "INSERT INTO reading_state (user_id, manifestation_id, progress_pct, last_read_at) \
             VALUES ($1, $2, $3, $4)",
            user_id,
            m_id,
            -1.0_f32,
            OffsetDateTime::now_utc(),
        )
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
        let result = sqlx::query!(
            "INSERT INTO reading_state (user_id, manifestation_id, progress_pct, last_read_at) \
             VALUES ($1, $2, $3, $4)",
            user_id,
            m_id,
            101.0_f32,
            OffsetDateTime::now_utc(),
        )
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

        sqlx::query!(
            "INSERT INTO reading_state (user_id, manifestation_id, progress_pct, last_read_at) \
             VALUES ($1, $2, 0.0, $3), ($1, $4, 100.0, $3)",
            user_id,
            m1,
            now,
            m2,
        )
        .execute(&pool)
        .await
        .expect("boundary values 0 and 100 should be accepted");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn duplicate_user_manifestation_rejected(pool: PgPool) {
        let (m_id, user_id) = fixture(&pool, "dup").await;
        sqlx::query!(
            "INSERT INTO reading_state (user_id, manifestation_id) VALUES ($1, $2)",
            user_id,
            m_id,
        )
        .execute(&pool)
        .await
        .unwrap();
        let result = sqlx::query!(
            "INSERT INTO reading_state (user_id, manifestation_id) VALUES ($1, $2)",
            user_id,
            m_id,
        )
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
        sqlx::query!(
            "INSERT INTO reading_state (user_id, manifestation_id, progress_pct, last_read_at) \
             VALUES ($1, $2, $3, $4)",
            alice,
            m_id,
            42.0_f32,
            OffsetDateTime::now_utc(),
        )
        .execute(&mut *tx)
        .await
        .unwrap();
        tx.commit().await.unwrap();

        // Alice sees one row.
        let mut tx = acquire_with_rls(&app, alice).await.unwrap();
        let alice_count = sqlx::query_scalar!("SELECT count(*) AS \"count!\" FROM reading_state")
            .fetch_one(&mut *tx)
            .await
            .unwrap();
        assert_eq!(alice_count, 1, "alice sees her own row");
        tx.rollback().await.unwrap();

        // Bob sees zero rows.
        let mut tx = acquire_with_rls(&app, bob).await.unwrap();
        let bob_count = sqlx::query_scalar!("SELECT count(*) AS \"count!\" FROM reading_state")
            .fetch_one(&mut *tx)
            .await
            .unwrap();
        assert_eq!(bob_count, 0, "bob does not see alice's row");
        tx.rollback().await.unwrap();

        // Bob writing under alice's user_id is blocked by WITH CHECK.
        let mut tx = acquire_with_rls(&app, bob).await.unwrap();
        let result = sqlx::query!(
            "INSERT INTO reading_state (user_id, manifestation_id, progress_pct, last_read_at) \
             VALUES ($1, $2, $3, $4)",
            alice,
            m_id,
            99.0_f32,
            OffsetDateTime::now_utc(),
        )
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
        sqlx::query!(
            "INSERT INTO reading_state (user_id, manifestation_id, progress_pct, last_read_at) \
             VALUES ($1, $2, $3, $4)",
            user_id,
            m_id,
            75.0_f32,
            OffsetDateTime::now_utc(),
        )
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query!("DELETE FROM users WHERE id = $1", user_id)
            .execute(&pool)
            .await
            .unwrap();

        let count = sqlx::query_scalar!(
            "SELECT count(*) AS \"count!\" FROM reading_state WHERE user_id = $1",
            user_id,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(count, 0, "user delete cascades into reading_state");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn manifestation_delete_cascades(pool: PgPool) {
        let (m_id, user_id) = fixture(&pool, "m-cascade").await;
        sqlx::query!(
            "INSERT INTO reading_state (user_id, manifestation_id, progress_pct, last_read_at) \
             VALUES ($1, $2, $3, $4)",
            user_id,
            m_id,
            75.0_f32,
            OffsetDateTime::now_utc(),
        )
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query!("DELETE FROM manifestations WHERE id = $1", m_id)
            .execute(&pool)
            .await
            .unwrap();

        let count = sqlx::query_scalar!(
            "SELECT count(*) AS \"count!\" FROM reading_state WHERE manifestation_id = $1",
            m_id,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(count, 0, "manifestation delete cascades into reading_state");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn updated_at_trigger_advances_on_update(pool: PgPool) {
        let (m_id, user_id) = fixture(&pool, "updated-at").await;
        sqlx::query!(
            "INSERT INTO reading_state (user_id, manifestation_id) VALUES ($1, $2)",
            user_id,
            m_id,
        )
        .execute(&pool)
        .await
        .unwrap();

        let initial = sqlx::query_scalar!(
            "SELECT updated_at AS \"updated_at!\" FROM reading_state \
             WHERE user_id = $1 AND manifestation_id = $2",
            user_id,
            m_id,
        )
        .fetch_one(&pool)
        .await
        .unwrap();

        // now() == transaction_timestamp(); separate sqlx queries are separate
        // implicit transactions, but we sleep to make timestamp ordering
        // observable on fast hardware.
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;

        sqlx::query!(
            "UPDATE reading_state SET progress_pct = $1, last_read_at = $2 \
             WHERE user_id = $3 AND manifestation_id = $4",
            33.0_f32,
            OffsetDateTime::now_utc(),
            user_id,
            m_id,
        )
        .execute(&pool)
        .await
        .unwrap();

        let updated = sqlx::query_scalar!(
            "SELECT updated_at AS \"updated_at!\" FROM reading_state \
             WHERE user_id = $1 AND manifestation_id = $2",
            user_id,
            m_id,
        )
        .fetch_one(&pool)
        .await
        .unwrap();

        assert!(
            updated > initial,
            "updated_at trigger should advance on UPDATE"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn rating_below_range_rejected(pool: PgPool) {
        let (m_id, user_id) = fixture(&pool, "rating-low").await;
        let result = sqlx::query!(
            "INSERT INTO reading_state (user_id, manifestation_id, rating) VALUES ($1, $2, $3)",
            user_id,
            m_id,
            0_i16,
        )
        .execute(&pool)
        .await;
        assert!(result.is_err(), "rating = 0 should violate range CHECK");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn rating_above_range_rejected(pool: PgPool) {
        let (m_id, user_id) = fixture(&pool, "rating-high").await;
        let result = sqlx::query!(
            "INSERT INTO reading_state (user_id, manifestation_id, rating) VALUES ($1, $2, $3)",
            user_id,
            m_id,
            6_i16,
        )
        .execute(&pool)
        .await;
        assert!(result.is_err(), "rating = 6 should violate range CHECK");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn rating_boundaries_accepted(pool: PgPool) {
        let ingestion = ingestion_pool_for(&pool).await;
        let app = app_pool_for(&pool).await;
        let (_w1, m1) = insert_work_and_manifestation(&ingestion, "rating-low-bound").await;
        let (_w2, m2) = insert_work_and_manifestation(&ingestion, "rating-high-bound").await;
        let (user_id, _) = create_adult_and_basic_auth(&app, "rating-boundaries").await;

        sqlx::query!(
            "INSERT INTO reading_state (user_id, manifestation_id, rating) \
             VALUES ($1, $2, 1), ($1, $3, 5)",
            user_id,
            m1,
            m2,
        )
        .execute(&pool)
        .await
        .expect("boundary ratings 1 and 5 should be accepted");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn notes_over_cap_rejected(pool: PgPool) {
        let (m_id, user_id) = fixture(&pool, "notes-boundary").await;
        sqlx::query!(
            "INSERT INTO reading_state (user_id, manifestation_id, notes) VALUES ($1, $2, $3)",
            user_id,
            m_id,
            "n".repeat(10_000),
        )
        .execute(&pool)
        .await
        .expect("10000-char notes should be accepted");

        let err = sqlx::query!(
            "UPDATE reading_state SET notes = $3 WHERE user_id = $1 AND manifestation_id = $2",
            user_id,
            m_id,
            "n".repeat(10_001),
        )
        .execute(&pool)
        .await
        .expect_err("10001-char notes must violate the notes length CHECK");
        assert_eq!(
            err.as_database_error().and_then(|e| e.constraint()),
            Some("reading_state_notes_len"),
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn started_at_decode_range_rejects_infinity(pool: PgPool) {
        let (m_id, user_id) = fixture(&pool, "started-at-infinity").await;
        let err = sqlx::query!(
            "INSERT INTO reading_state (user_id, manifestation_id, started_at) \
             VALUES ($1, $2, 'infinity')",
            user_id,
            m_id,
        )
        .execute(&pool)
        .await
        .expect_err("infinity must violate the started_at decode-range CHECK");
        assert_eq!(
            err.as_database_error().and_then(|e| e.constraint()),
            Some("reading_state_started_at_ts_decode_range"),
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn finished_at_decode_range_rejects_infinity(pool: PgPool) {
        let (m_id, user_id) = fixture(&pool, "finished-at-infinity").await;
        let err = sqlx::query!(
            "INSERT INTO reading_state (user_id, manifestation_id, finished_at) \
             VALUES ($1, $2, 'infinity')",
            user_id,
            m_id,
        )
        .execute(&pool)
        .await
        .expect_err("infinity must violate the finished_at decode-range CHECK");
        assert_eq!(
            err.as_database_error().and_then(|e| e.constraint()),
            Some("reading_state_finished_at_ts_decode_range"),
        );
    }
}
