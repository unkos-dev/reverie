//! Caller-owned reading state and conditional writes.
//!
//! THREAT: children may update only their own state for a visible manifestation.

use crate::auth::{middleware::CurrentUser, scope::Scope};
use crate::models::reading_state::{ReadingState, ReadingStateSummary};
use crate::models::reading_status::ReadingStatus;
use crate::routes::etag::{Conditional, StrongEntityTag, hash_etag};
use crate::{db, error::AppError, state::AppState};
use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Character cap for `reading_state.notes`; mirrored by the
/// `reading_state_notes_len` CHECK in the schema.
const NOTES_MAX_CHARS: usize = 10_000;

/// RLS-scoped existence probe shared by both handlers: `NotFound` when the
/// manifestation is missing or hidden for the current user
/// (existence-not-leaked).
pub async fn ensure_manifestation_visible(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    manifestation_id: Uuid,
) -> Result<(), AppError> {
    let visible: Option<Uuid> = sqlx::query_scalar!(
        "SELECT id FROM manifestations WHERE id = $1",
        manifestation_id,
    )
    .fetch_optional(&mut **tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    if visible.is_none() {
        return Err(AppError::NotFound);
    }
    Ok(())
}

/// Decode target for a `reading_state` row (shared by the GET read and the
/// PATCH pre-write lock). `Default` gives the all-null "unread" shape for a
/// missing row.
#[derive(Debug, Default, Clone)]
pub struct ReadingStateRow {
    pub status: Option<ReadingStatus>,
    pub rating: Option<i16>,
    pub notes: Option<String>,
    pub progress_pct: Option<f32>,
    pub started_at: Option<jiff_sqlx::Timestamp>,
    pub finished_at: Option<jiff_sqlx::Timestamp>,
    pub last_read_at: Option<jiff_sqlx::Timestamp>,
}

impl From<&ReadingStateRow> for ReadingStateSummary {
    fn from(r: &ReadingStateRow) -> Self {
        Self {
            status: r.status,
            rating: r.rating,
            progress_pct: r.progress_pct,
            started_at: r.started_at.map(jiff_sqlx::Timestamp::to_jiff),
            finished_at: r.finished_at.map(jiff_sqlx::Timestamp::to_jiff),
        }
    }
}

impl From<ReadingStateRow> for ReadingState {
    fn from(r: ReadingStateRow) -> Self {
        Self {
            status: r.status,
            rating: r.rating,
            notes: r.notes,
            progress_pct: r.progress_pct,
            started_at: r.started_at.map(jiff_sqlx::Timestamp::to_jiff),
            finished_at: r.finished_at.map(jiff_sqlx::Timestamp::to_jiff),
            last_read_at: r.last_read_at.map(jiff_sqlx::Timestamp::to_jiff),
        }
    }
}

/// Fixed-field-order hash input for a `reading_state` row's `ETag`. Covers
/// every field `PATCH /api/v1/books/{id}/reading` can modify. The all-`None`
/// default (no row yet) still hashes to a stable value, so a client can
/// protect a first write with `If-Match` against the "unread" `ETag` from a
/// prior GET.
#[derive(Serialize)]
struct ReadingEtagFields<'a> {
    status: Option<ReadingStatus>,
    rating: Option<i16>,
    notes: Option<&'a str>,
    progress_pct: Option<f32>,
    started_at: Option<Timestamp>,
    finished_at: Option<Timestamp>,
    last_read_at: Option<Timestamp>,
}

impl ReadingStateRow {
    pub fn etag(&self) -> Result<axum::http::HeaderValue, AppError> {
        hash_etag(&ReadingEtagFields {
            status: self.status,
            rating: self.rating,
            notes: self.notes.as_deref(),
            progress_pct: self.progress_pct,
            started_at: self.started_at.map(jiff_sqlx::Timestamp::to_jiff),
            finished_at: self.finished_at.map(jiff_sqlx::Timestamp::to_jiff),
            last_read_at: self.last_read_at.map(jiff_sqlx::Timestamp::to_jiff),
        })
    }
}

/// Body for `PATCH /api/v1/books/{id}/reading`. RFC 7396 JSON Merge Patch:
/// an absent key leaves the field unchanged, an explicit `null` clears it.
#[expect(
    clippy::option_option,
    reason = "RFC 7396 sparse-update encoding: outer Option distinguishes absent (None) from present-and-null (Some(None))"
)]
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct UpdateReadingRequest {
    /// New status. Absent = unchanged; `null` clears. Transition stamps
    /// fire only when this field is present with a non-null value, so
    /// absent and `null` both leave every timestamp untouched.
    #[serde(default, with = "::serde_with::rust::double_option")]
    #[schema(value_type = Option<ReadingStatus>)]
    pub status: Option<Option<ReadingStatus>>,
    /// New rating, 1-5. Absent = unchanged; `null` clears.
    #[serde(default, with = "::serde_with::rust::double_option")]
    #[schema(value_type = Option<i16>)]
    pub rating: Option<Option<i16>>,
    /// New free-text notes, at most 10000 characters. Absent = unchanged;
    /// `null` clears.
    #[serde(default, with = "::serde_with::rust::double_option")]
    #[schema(value_type = Option<String>, max_length = 10_000)]
    pub notes: Option<Option<String>>,
}

/// Merge a patch onto the locked row and apply transition stamps. Pure
/// function (no I/O) so `patch_reading` stays a thin fetch/merge/write
/// pipeline; see the handler doc for the stamp rules.
fn apply_patch(existing: ReadingStateRow, req: &UpdateReadingRequest) -> ReadingStateRow {
    let status = match req.status {
        None => existing.status,
        Some(None) => None,
        Some(Some(v)) => Some(v),
    };
    let rating = match req.rating {
        None => existing.rating,
        Some(None) => None,
        Some(Some(v)) => Some(v),
    };
    let notes = match &req.notes {
        None => existing.notes,
        Some(None) => None,
        Some(Some(v)) => Some(v.clone()),
    };

    let (progress_pct, started_at, finished_at, last_read_at) = match req.status.flatten() {
        Some(ReadingStatus::Reading) => (
            existing.progress_pct,
            Some(
                existing
                    .started_at
                    .unwrap_or_else(|| Timestamp::now().into()),
            ),
            existing.finished_at,
            existing.last_read_at,
        ),
        Some(ReadingStatus::Finished) => {
            let now = jiff_sqlx::Timestamp::from(Timestamp::now());
            (Some(100.0_f32), existing.started_at, Some(now), Some(now))
        }
        _ => (
            existing.progress_pct,
            existing.started_at,
            existing.finished_at,
            existing.last_read_at,
        ),
    };

    ReadingStateRow {
        status,
        rating,
        notes,
        progress_pct,
        started_at,
        finished_at,
        last_read_at,
    }
}

pub async fn patch_reading(
    current_user: &CurrentUser,
    state: &AppState,
    manifestation_id: Uuid,
    if_match: Option<StrongEntityTag>,
    req: UpdateReadingRequest,
) -> Result<Conditional<ReadingStateRow>, AppError> {
    current_user.require_scope(Scope::Write)?;
    let if_match = if_match.ok_or(AppError::IfMatchRequired)?;

    let mut tx = db::acquire_with_rls(&state.pool, current_user.user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    ensure_manifestation_visible(&mut tx, manifestation_id).await?;

    // Seed the row before locking: FOR UPDATE cannot lock an absent row, so
    // two concurrent first-writes would otherwise both merge against the
    // empty default and the later commit would silently drop the earlier
    // one's fields. After the seed, the SELECT below always has a row to
    // lock and concurrent patches serialize on it.
    sqlx::query!(
        "INSERT INTO reading_state (user_id, manifestation_id) VALUES ($1, $2)
         ON CONFLICT (user_id, manifestation_id) DO NOTHING",
        current_user.user_id,
        manifestation_id,
    )
    .execute(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    let existing = sqlx::query_as!(
        ReadingStateRow,
        r#"SELECT status AS "status?: ReadingStatus", rating, notes, progress_pct,
                  started_at, finished_at, last_read_at
             FROM reading_state WHERE manifestation_id = $1 FOR UPDATE"#,
        manifestation_id,
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    // Compared under the same row lock the patch applies against, so a
    // concurrent writer cannot slip a change in between this check and the
    // update below.
    let current = existing.etag()?;
    if !if_match.matches(&current) {
        return Ok(Conditional::Mismatch(current));
    }

    // Semantic body validation runs only once the precondition has held.
    // RFC 9110 §13.2.1 places precondition evaluation after the normal
    // request checks (auth, existence) and before the request content is
    // processed, so a caller holding a stale representation learns that
    // first and refetches, rather than being sent to fix a body that a
    // concurrent write may already have made irrelevant. Returning here
    // drops `tx`, so the seed insert above rolls back.
    if req.status.is_none() && req.rating.is_none() && req.notes.is_none() {
        return Err(AppError::Validation("no fields".into()));
    }
    if let Some(Some(rating)) = req.rating
        && !(1..=5).contains(&rating)
    {
        return Err(AppError::Validation(
            "rating must be between 1 and 5".into(),
        ));
    }
    if let Some(Some(notes)) = &req.notes
        && notes.chars().count() > NOTES_MAX_CHARS
    {
        return Err(AppError::Validation(
            "notes must be at most 10000 characters".into(),
        ));
    }

    let merged = apply_patch(existing, &req);

    let row = sqlx::query_as!(
        ReadingStateRow,
        r#"
        UPDATE reading_state SET
            status = $3,
            rating = $4,
            notes = $5,
            progress_pct = $6,
            started_at = $7,
            finished_at = $8,
            last_read_at = $9
        WHERE user_id = $1 AND manifestation_id = $2
        RETURNING status AS "status?: ReadingStatus", rating, notes, progress_pct,
                  started_at, finished_at, last_read_at
        "#,
        current_user.user_id,
        manifestation_id,
        merged.status as Option<ReadingStatus>,
        merged.rating,
        merged.notes,
        merged.progress_pct,
        merged.started_at,
        merged.finished_at,
        merged.last_read_at,
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    tx.commit()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    Ok(Conditional::Matched(row))
}
