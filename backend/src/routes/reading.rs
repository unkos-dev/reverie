//! `/api/v1/books/{id}/reading` — per-user reading state: status, rating,
//! notes, progress, and reading dates.
//!
//! THREAT: unlike `metadata` and `shelves`, these handlers do NOT call
//! `require_not_child()`. Reading state is self-scoped personal data (a
//! child recording their own status/rating/notes), not shared-library
//! curation, so child accounts may read and write their own row. RLS still
//! confines every read/write to rows the caller owns and to manifestations
//! visible under the caller's adult/child policy.

use axum::extract::{Path, State};
use axum::response::IntoResponse;
use serde::Deserialize;
use time::OffsetDateTime;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::auth::middleware::CurrentUser;
use crate::auth::scope::Scope;
use crate::db;
use crate::error::AppError;
use crate::models::reading_state::ReadingState;
use crate::models::reading_status::ReadingStatus;
use crate::state::AppState;

/// Build the `/api/v1/books/{id}/reading` router as an [`OpenApiRouter`].
/// Merged into `crate::openapi::pilot_router`.
pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(get_reading, patch_reading))
}

/// Decode target for a `reading_state` row (shared by the GET read and the
/// PATCH pre-write lock). `Default` gives the all-null "unread" shape for a
/// missing row.
#[derive(Debug, Default, Clone)]
struct ReadingStateRow {
    status: Option<ReadingStatus>,
    rating: Option<i16>,
    notes: Option<String>,
    progress_pct: Option<f32>,
    started_at: Option<OffsetDateTime>,
    finished_at: Option<OffsetDateTime>,
    last_read_at: Option<OffsetDateTime>,
}

impl From<ReadingStateRow> for ReadingState {
    fn from(r: ReadingStateRow) -> Self {
        Self {
            status: r.status,
            rating: r.rating,
            notes: r.notes,
            progress_pct: r.progress_pct,
            started_at: r.started_at,
            finished_at: r.finished_at,
            last_read_at: r.last_read_at,
        }
    }
}

/// `GET /api/v1/books/{id}/reading` — the caller's reading state for one
/// book.
///
/// # Errors
/// - [`AppError::NotFound`] when the manifestation is missing or hidden by
///   RLS for the current user (existence-not-leaked).
/// - [`AppError::Internal`] on database errors.
#[utoipa::path(
    get,
    path = "/api/v1/books/{id}/reading",
    tag = "reading",
    security(("session_cookie" = ["read"]), ("device_token_bearer" = ["read"]), ("oidc_jwt_bearer" = ["read"]), ("opds_basic" = ["read"])),
    params(("id" = Uuid, Path, description = "Manifestation id")),
    responses(
        (status = 200, description = "Caller's reading state; all-null fields mean unread (no row yet)", body = ReadingState),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails),
        (status = 404, description = "Manifestation missing or RLS-hidden (existence-not-leaked)", body = crate::openapi::ProblemDetails)
    )
)]
async fn get_reading(
    current_user: CurrentUser,
    State(state): State<AppState>,
    Path(manifestation_id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    current_user.require_scope(Scope::Read)?;

    let mut tx = db::acquire_with_rls(&state.pool, current_user.user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let visible: Option<Uuid> = sqlx::query_scalar!(
        "SELECT id FROM manifestations WHERE id = $1",
        manifestation_id,
    )
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    if visible.is_none() {
        return Err(AppError::NotFound);
    }

    let row = sqlx::query_as!(
        ReadingStateRow,
        r#"SELECT status AS "status?: ReadingStatus", rating, notes, progress_pct,
                  started_at, finished_at, last_read_at
             FROM reading_state WHERE manifestation_id = $1"#,
        manifestation_id,
    )
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    tx.commit()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    Ok(axum::Json(ReadingState::from(row.unwrap_or_default())))
}

/// Body for `PATCH /api/v1/books/{id}/reading`. RFC 7396 JSON Merge Patch:
/// an absent key leaves the field unchanged, an explicit `null` clears it.
#[allow(
    clippy::option_option,
    reason = "RFC 7396 sparse-update encoding — outer Option distinguishes absent (None) from present-and-null (Some(None))"
)]
#[derive(Debug, Deserialize, utoipa::ToSchema)]
struct UpdateReadingRequest {
    /// New status. Absent = unchanged; `null` clears (and skips every
    /// transition stamp below, since none of them fire without a `status`
    /// of `reading` or `finished`).
    #[serde(default, with = "::serde_with::rust::double_option")]
    #[schema(value_type = Option<ReadingStatus>)]
    status: Option<Option<ReadingStatus>>,
    /// New rating, 1-5. Absent = unchanged; `null` clears.
    #[serde(default, with = "::serde_with::rust::double_option")]
    #[schema(value_type = Option<i16>)]
    rating: Option<Option<i16>>,
    /// New free-text notes. Absent = unchanged; `null` clears.
    #[serde(default, with = "::serde_with::rust::double_option")]
    #[schema(value_type = Option<String>)]
    notes: Option<Option<String>>,
}

/// Merge a patch onto the fetched-or-default row and apply transition
/// stamps. Pure function (no I/O) so `patch_reading` stays a thin
/// fetch/merge/write pipeline — see the handler doc for the stamp rules.
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

    let (progress_pct, started_at, finished_at, last_read_at) = match status {
        Some(ReadingStatus::Reading) => (
            existing.progress_pct,
            Some(existing.started_at.unwrap_or_else(OffsetDateTime::now_utc)),
            existing.finished_at,
            existing.last_read_at,
        ),
        Some(ReadingStatus::Finished) => {
            let now = OffsetDateTime::now_utc();
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

/// `PATCH /api/v1/books/{id}/reading` — upsert the caller's reading state.
///
/// Transition stamps, computed against the fetched-or-default state and
/// applied based on the resulting `status` (not a before/after diff, so
/// repeating the same status re-applies its stamp):
/// - resulting `status = reading` → `started_at := COALESCE(started_at, now())`
/// - resulting `status = finished` → `finished_at := now()`,
///   `progress_pct := 100`, `last_read_at := now()`
/// - any other resulting `status` (including an explicit `null` clear) →
///   no stamps; `progress_pct` / `started_at` / `finished_at` /
///   `last_read_at` carry over unchanged
///
/// # Errors
/// - [`AppError::Validation`] when the body has no populated fields, or
///   `rating` is outside `1..=5`.
/// - [`AppError::NotFound`] when the manifestation is missing or hidden by
///   RLS for the current user (existence-not-leaked).
/// - [`AppError::Internal`] on database errors.
#[utoipa::path(
    patch,
    path = "/api/v1/books/{id}/reading",
    tag = "reading",
    security(("session_cookie" = ["write"]), ("device_token_bearer" = ["write"]), ("oidc_jwt_bearer" = ["write"]), ("opds_basic" = ["write"])),
    params(("id" = Uuid, Path, description = "Manifestation id")),
    request_body(content = UpdateReadingRequest, description = "RFC 7396 JSON Merge Patch: absent fields are unchanged, `null` clears"),
    responses(
        (status = 200, description = "Reading state after the patch and any transition stamps", body = ReadingState),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails),
        (status = 404, description = "Manifestation missing or RLS-hidden (existence-not-leaked)", body = crate::openapi::ProblemDetails),
        (status = 422, description = "Empty patch, or rating outside 1-5", body = crate::openapi::ProblemDetails)
    )
)]
async fn patch_reading(
    current_user: CurrentUser,
    State(state): State<AppState>,
    Path(manifestation_id): Path<Uuid>,
    body: Result<axum::Json<UpdateReadingRequest>, axum::extract::rejection::JsonRejection>,
) -> Result<impl IntoResponse, AppError> {
    current_user.require_scope(Scope::Write)?;
    let axum::Json(req) = body.map_err(|e| AppError::Validation(e.body_text()))?;

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

    let mut tx = db::acquire_with_rls(&state.pool, current_user.user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let visible: Option<Uuid> = sqlx::query_scalar!(
        "SELECT id FROM manifestations WHERE id = $1",
        manifestation_id,
    )
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    if visible.is_none() {
        return Err(AppError::NotFound);
    }

    let existing = sqlx::query_as!(
        ReadingStateRow,
        r#"SELECT status AS "status?: ReadingStatus", rating, notes, progress_pct,
                  started_at, finished_at, last_read_at
             FROM reading_state WHERE manifestation_id = $1 FOR UPDATE"#,
        manifestation_id,
    )
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?
    .unwrap_or_default();

    let merged = apply_patch(existing, &req);

    let row = sqlx::query_as!(
        ReadingStateRow,
        r#"
        INSERT INTO reading_state
            (user_id, manifestation_id, status, rating, notes, progress_pct,
             started_at, finished_at, last_read_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        ON CONFLICT (user_id, manifestation_id) DO UPDATE SET
            status = EXCLUDED.status,
            rating = EXCLUDED.rating,
            notes = EXCLUDED.notes,
            progress_pct = EXCLUDED.progress_pct,
            started_at = EXCLUDED.started_at,
            finished_at = EXCLUDED.finished_at,
            last_read_at = EXCLUDED.last_read_at
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

    Ok(axum::Json(ReadingState::from(row)))
}
