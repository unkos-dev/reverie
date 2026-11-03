//! Reading state: per-`(user, manifestation)` status, rating, notes,
//! progress, and reading dates.
//!
//! Queries live in `routes::reading` and `routes::library`, not here; this
//! module carries the wire DTOs, plus schema-level tests against the
//! migrations.
//!
//! Wire-format conventions follow the JSON-API conventions ADR
//! (`docs/adr/0011-json-api-conventions-for-the-browser-facing-rest-surface.md`): snake_case field names,
//! `Option<T>` for nullable (never `skip_serializing_if`), RFC 3339
//! timestamps.

use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::models::reading_status::ReadingStatus;

/// `GET /api/v1/books/{id}/reading` response: the caller's full reading
/// state for one book.
///
/// A missing `reading_state` row (never written) decodes
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
    pub started_at: Option<DateTime<Utc>>,
    /// `reading_state.finished_at`: stamped each time a patch sets `status`
    /// to [`ReadingStatus::Finished`].
    pub finished_at: Option<DateTime<Utc>>,
    /// `reading_state.last_read_at`.
    pub last_read_at: Option<DateTime<Utc>>,
}

/// Reading-state slice embedded in each `GET /api/v1/books` list row.
///
/// Batch-loaded alongside the page (see `routes::library::load_authors_for_works`
/// for the sibling batch-load pattern); `None` when the caller has no
/// `reading_state` row for that book (unread).
///
/// Deliberately excludes `notes`: it is capped at 10k characters per row,
/// which at the maximum page size would dominate the response body many
/// times over. Clients read notes through the single-book
/// `GET /api/v1/books/{id}/reading` endpoint ([`ReadingState`]).
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
#[non_exhaustive]
pub struct ReadingStateSummary {
    /// `reading_state.status`.
    pub status: Option<ReadingStatus>,
    /// `reading_state.rating`, 1-5.
    pub rating: Option<i16>,
    /// `reading_state.progress_pct`, 0-100.
    pub progress_pct: Option<f32>,
    /// `reading_state.started_at`; see [`ReadingState::started_at`].
    pub started_at: Option<DateTime<Utc>>,
    /// `reading_state.finished_at`; see [`ReadingState::finished_at`].
    pub finished_at: Option<DateTime<Utc>>,
}

#[cfg(test)]
mod tests {
    use crate::db::acquire_with_rls;
    use crate::test_support::db::{
        app_pool_for, create_adult_and_basic_auth, ingestion_pool_for,
        insert_work_and_manifestation,
    };
    use chrono::Utc;
    use sqlx::PgPool;

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
            Utc::now(),
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
            Utc::now(),
        )
        .execute(&mut *tx)
        .await;
        assert!(
            result.is_err(),
            "bob writing alice's user_id should fail RLS WITH CHECK"
        );
        tx.rollback().await.unwrap();
    }
}
