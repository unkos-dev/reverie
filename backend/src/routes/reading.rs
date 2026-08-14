//! `/api/v1/books/{id}/reading`, the per-user reading state endpoints:
//! status, rating, notes, progress, and reading dates.
//!
//! THREAT: unlike `metadata` and `shelves`, these handlers do NOT call
//! `require_not_child()`. Reading state is self-scoped personal data (a
//! child recording their own status/rating/notes), not shared-library
//! curation, so child accounts may read and write their own row. RLS still
//! confines every read/write to rows the caller owns and to manifestations
//! visible under the caller's adult/child policy.

use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::http::header::ETAG;
use axum::response::{IntoResponse, Response};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::auth::middleware::CurrentUser;
use crate::auth::scope::Scope;
use crate::db;
use crate::error::AppError;
use crate::models::reading_state::ReadingState;
use crate::models::reading_status::ReadingStatus;
use crate::routes::etag::{hash_etag, if_match_mismatch, parse_if_match};
use crate::state::AppState;

/// Build the `/api/v1/books/{id}/reading` router as an [`OpenApiRouter`].
/// Merged into `crate::openapi::pilot_router`.
pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(get_reading, patch_reading))
}

/// Character cap for `reading_state.notes`; mirrored by the
/// `reading_state_notes_len` CHECK in the schema.
const NOTES_MAX_CHARS: usize = 10_000;

/// RLS-scoped existence probe shared by both handlers: `NotFound` when the
/// manifestation is missing or hidden for the current user
/// (existence-not-leaked).
async fn ensure_manifestation_visible(
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
struct ReadingStateRow {
    status: Option<ReadingStatus>,
    rating: Option<i16>,
    notes: Option<String>,
    progress_pct: Option<f32>,
    started_at: Option<DateTime<Utc>>,
    finished_at: Option<DateTime<Utc>>,
    last_read_at: Option<DateTime<Utc>>,
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
    started_at: Option<DateTime<Utc>>,
    finished_at: Option<DateTime<Utc>>,
    last_read_at: Option<DateTime<Utc>>,
}

impl ReadingStateRow {
    fn etag(&self) -> Result<axum::http::HeaderValue, AppError> {
        hash_etag(&ReadingEtagFields {
            status: self.status,
            rating: self.rating,
            notes: self.notes.as_deref(),
            progress_pct: self.progress_pct,
            started_at: self.started_at,
            finished_at: self.finished_at,
            last_read_at: self.last_read_at,
        })
    }
}

/// `GET /api/v1/books/{id}/reading`: the caller's reading state for one
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
        (status = 200, description = "Caller's reading state; all-null fields mean unread (no row yet)", body = ReadingState,
         headers(("ETag" = String, description = "Strong entity-tag hashing every PATCH-modifiable field; echo as If-Match on a protected write"))),
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

    ensure_manifestation_visible(&mut tx, manifestation_id).await?;

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

    let row = row.unwrap_or_default();
    let mut headers = HeaderMap::new();
    headers.insert(ETAG, row.etag()?);
    Ok((headers, axum::Json(ReadingState::from(row))))
}

/// Body for `PATCH /api/v1/books/{id}/reading`. RFC 7396 JSON Merge Patch:
/// an absent key leaves the field unchanged, an explicit `null` clears it.
#[expect(
    clippy::option_option,
    reason = "RFC 7396 sparse-update encoding: outer Option distinguishes absent (None) from present-and-null (Some(None))"
)]
#[derive(Debug, Deserialize, utoipa::ToSchema)]
struct UpdateReadingRequest {
    /// New status. Absent = unchanged; `null` clears. Transition stamps
    /// fire only when this field is present with a non-null value, so
    /// absent and `null` both leave every timestamp untouched.
    #[serde(default, with = "::serde_with::rust::double_option")]
    #[schema(value_type = Option<ReadingStatus>)]
    status: Option<Option<ReadingStatus>>,
    /// New rating, 1-5. Absent = unchanged; `null` clears.
    #[serde(default, with = "::serde_with::rust::double_option")]
    #[schema(value_type = Option<i16>)]
    rating: Option<Option<i16>>,
    /// New free-text notes, at most 10000 characters. Absent = unchanged;
    /// `null` clears.
    #[serde(default, with = "::serde_with::rust::double_option")]
    #[schema(value_type = Option<String>, max_length = 10_000)]
    notes: Option<Option<String>>,
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
            Some(existing.started_at.unwrap_or_else(Utc::now)),
            existing.finished_at,
            existing.last_read_at,
        ),
        Some(ReadingStatus::Finished) => {
            let now = Utc::now();
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

/// `PATCH /api/v1/books/{id}/reading`: upsert the caller's reading state.
///
/// Transition stamps fire only when the patch itself carries a non-null
/// `status` (no before/after diff, so repeating the current status
/// re-applies its stamp; a patch without `status` never touches stamps):
/// - patched `status = reading` → `started_at := COALESCE(started_at, now())`
/// - patched `status = finished` → `finished_at := now()`,
///   `progress_pct := 100`, `last_read_at := now()`
/// - any other patched `status`, an explicit `null` clear, or a patch
///   without `status` → no stamps; `progress_pct` / `started_at` /
///   `finished_at` / `last_read_at` carry over unchanged
///
/// # Errors
/// - [`AppError::Validation`] when the body has no populated fields,
///   `rating` is outside `1..=5`, or `notes` exceeds 10000 characters.
/// - [`AppError::MalformedHeader`] when `If-Match` violates the RFC 9110
///   §8.8.3 entity-tag grammar, or carries a form this API refuses by
///   policy: the `*` wildcard, an entity-tag list, a weak validator, or
///   more than one instance of the field.
/// - [`AppError::NotFound`] when the manifestation is missing or hidden by
///   RLS for the current user (existence-not-leaked).
/// - [`AppError::IfMatchRequired`] (428) when `If-Match` is absent.
/// - [`AppError::IfMatchMismatch`] (412) when `If-Match` does not match the
///   caller's current reading-state `ETag`; the response carries the
///   current `ETag` so the caller can resync in one round trip.
/// - [`AppError::Internal`] on database errors.
#[utoipa::path(
    patch,
    path = "/api/v1/books/{id}/reading",
    tag = "reading",
    security(("session_cookie" = ["write"]), ("device_token_bearer" = ["write"]), ("oidc_jwt_bearer" = ["write"]), ("opds_basic" = ["write"])),
    params(
        ("id" = Uuid, Path, description = "Manifestation id"),
        ("If-Match" = String, Header, description = "Exactly one quoted strong entity-tag, as returned in a prior GET or PATCH response's ETag header. Opaque content follows the full RFC 9110 8.8.3 grammar, obs-text included. The * wildcard, entity-tag lists, weak tags, and repeated instances of this field are refused with 400. Required: absent means 428; unequal means 412")
    ),
    request_body(content = UpdateReadingRequest, description = "RFC 7396 JSON Merge Patch: absent fields are unchanged, `null` clears"),
    responses(
        (status = 200, description = "Reading state after the patch and any transition stamps", body = ReadingState,
         headers(("ETag" = String, description = "Strong entity-tag hashing every PATCH-modifiable field, reflecting the state after this write"))),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails),
        (status = 404, description = "Manifestation missing or RLS-hidden (existence-not-leaked)", body = crate::openapi::ProblemDetails),
        (status = 412, description = "If-Match does not match the caller's current reading-state ETag", body = crate::openapi::ProblemDetails,
         headers(("ETag" = String, description = "Current entity-tag, so the caller can resync without a follow-up GET"))),
        (status = 400, description = "If-Match is malformed, or carries a form this API refuses by policy: the * wildcard, an entity-tag list, a weak tag, or a repeated header instance", body = crate::openapi::ProblemDetails),
        (status = 422, description = "Empty patch, rating outside 1-5, or notes over 10000 characters", body = crate::openapi::ProblemDetails),
        (status = 428, description = "If-Match header absent", body = crate::openapi::ProblemDetails)
    )
)]
async fn patch_reading(
    current_user: CurrentUser,
    State(state): State<AppState>,
    Path(manifestation_id): Path<Uuid>,
    headers_in: HeaderMap,
    body: Result<axum::Json<UpdateReadingRequest>, axum::extract::rejection::JsonRejection>,
) -> Result<Response, AppError> {
    current_user.require_scope(Scope::Write)?;
    let if_match = parse_if_match(&headers_in)?.ok_or(AppError::IfMatchRequired)?;
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
    if let Some(Some(notes)) = &req.notes
        && notes.chars().count() > NOTES_MAX_CHARS
    {
        return Err(AppError::Validation(
            "notes must be at most 10000 characters".into(),
        ));
    }

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
        return Ok(if_match_mismatch(&current));
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

    let mut headers = HeaderMap::new();
    headers.insert(ETAG, row.etag()?);
    Ok((headers, axum::Json(ReadingState::from(row))).into_response())
}

#[cfg(test)]
mod tests {
    use axum::http::{HeaderName, HeaderValue, StatusCode};
    use serde_json::json;
    use sqlx::PgPool;
    use uuid::Uuid;

    use crate::error::problems;
    use crate::test_support;

    fn auth(header: &str) -> (HeaderName, HeaderValue) {
        (
            axum::http::header::AUTHORIZATION,
            HeaderValue::from_str(header).expect("ascii auth header"),
        )
    }

    fn etag_value(headers: &axum::http::HeaderMap) -> String {
        headers
            .get(axum::http::header::ETAG)
            .expect("ETag header present")
            .to_str()
            .expect("ETag ascii")
            .to_owned()
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn get_reading_requires_auth(pool: PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
        let r = server
            .get(&format!("/api/v1/books/{}/reading", Uuid::new_v4()))
            .await;
        assert_eq!(r.status_code(), StatusCode::UNAUTHORIZED);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn get_reading_absent_row_returns_all_null_200(pool: PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_work, m_id) =
            test_support::db::insert_work_and_manifestation(&ingestion_pool, "get-absent").await;
        let (_user_id, basic) =
            test_support::db::create_adult_and_basic_auth(&app_pool, "get-absent").await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

        let r = server
            .get(&format!("/api/v1/books/{m_id}/reading"))
            .add_header(auth(&basic).0, auth(&basic).1)
            .await;
        assert_eq!(r.status_code(), StatusCode::OK, "body: {}", r.text());
        let body: serde_json::Value = r.json();
        for field in [
            "status",
            "rating",
            "notes",
            "progress_pct",
            "started_at",
            "finished_at",
            "last_read_at",
        ] {
            assert!(body[field].is_null(), "{field} should be null: {body}");
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn get_reading_unknown_book_returns_404(pool: PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_user_id, basic) =
            test_support::db::create_adult_and_basic_auth(&app_pool, "get-404").await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

        let r = server
            .get(&format!("/api/v1/books/{}/reading", Uuid::new_v4()))
            .add_header(auth(&basic).0, auth(&basic).1)
            .await;
        test_support::assert_problem(&r, problems::NOT_FOUND, StatusCode::NOT_FOUND);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn patch_reading_unknown_book_returns_404(pool: PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_user_id, basic) =
            test_support::db::create_adult_and_basic_auth(&app_pool, "patch-404").await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

        let r = server
            .patch(&format!("/api/v1/books/{}/reading", Uuid::new_v4()))
            .add_header(auth(&basic).0, auth(&basic).1)
            .add_header(
                axum::http::header::IF_MATCH,
                HeaderValue::from_static("\"dummy\""),
            )
            .json(&json!({"status": "reading"}))
            .await;
        test_support::assert_problem(&r, problems::NOT_FOUND, StatusCode::NOT_FOUND);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn patch_reading_creates_row_via_upsert(pool: PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_work, m_id) =
            test_support::db::insert_work_and_manifestation(&ingestion_pool, "upsert").await;
        let (_user_id, basic) =
            test_support::db::create_adult_and_basic_auth(&app_pool, "upsert").await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

        let initial = server
            .get(&format!("/api/v1/books/{m_id}/reading"))
            .add_header(auth(&basic).0.clone(), auth(&basic).1.clone())
            .await;
        let etag = etag_value(initial.headers());

        let r = server
            .patch(&format!("/api/v1/books/{m_id}/reading"))
            .add_header(auth(&basic).0, auth(&basic).1)
            .add_header(
                axum::http::header::IF_MATCH,
                HeaderValue::from_str(&etag).unwrap(),
            )
            .json(&json!({"status": "want_to_read", "rating": 4, "notes": "looks good"}))
            .await;
        assert_eq!(r.status_code(), StatusCode::OK, "body: {}", r.text());
        let body: serde_json::Value = r.json();
        assert_eq!(body["status"], "want_to_read");
        assert_eq!(body["rating"], 4);
        assert_eq!(body["notes"], "looks good");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn patch_reading_partial_update_leaves_other_fields_unchanged(pool: PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_work, m_id) =
            test_support::db::insert_work_and_manifestation(&ingestion_pool, "partial").await;
        let (_user_id, basic) =
            test_support::db::create_adult_and_basic_auth(&app_pool, "partial").await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

        let initial = server
            .get(&format!("/api/v1/books/{m_id}/reading"))
            .add_header(auth(&basic).0.clone(), auth(&basic).1.clone())
            .await;
        let etag = etag_value(initial.headers());

        let first = server
            .patch(&format!("/api/v1/books/{m_id}/reading"))
            .add_header(auth(&basic).0.clone(), auth(&basic).1.clone())
            .add_header(
                axum::http::header::IF_MATCH,
                HeaderValue::from_str(&etag).unwrap(),
            )
            .json(&json!({"status": "on_hold", "rating": 3, "notes": "original"}))
            .await;
        let etag = etag_value(first.headers());

        let r = server
            .patch(&format!("/api/v1/books/{m_id}/reading"))
            .add_header(auth(&basic).0, auth(&basic).1)
            .add_header(
                axum::http::header::IF_MATCH,
                HeaderValue::from_str(&etag).unwrap(),
            )
            .json(&json!({"rating": 5}))
            .await;
        assert_eq!(r.status_code(), StatusCode::OK, "body: {}", r.text());
        let body: serde_json::Value = r.json();
        assert_eq!(body["rating"], 5);
        assert_eq!(
            body["status"], "on_hold",
            "unpatched field must be untouched"
        );
        assert_eq!(
            body["notes"], "original",
            "unpatched field must be untouched"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn patch_reading_explicit_null_clears_field(pool: PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_work, m_id) =
            test_support::db::insert_work_and_manifestation(&ingestion_pool, "clear").await;
        let (_user_id, basic) =
            test_support::db::create_adult_and_basic_auth(&app_pool, "clear").await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

        let initial = server
            .get(&format!("/api/v1/books/{m_id}/reading"))
            .add_header(auth(&basic).0.clone(), auth(&basic).1.clone())
            .await;
        let etag = etag_value(initial.headers());

        let first = server
            .patch(&format!("/api/v1/books/{m_id}/reading"))
            .add_header(auth(&basic).0.clone(), auth(&basic).1.clone())
            .add_header(
                axum::http::header::IF_MATCH,
                HeaderValue::from_str(&etag).unwrap(),
            )
            .json(&json!({"notes": "temporary"}))
            .await;
        let etag = etag_value(first.headers());

        let r = server
            .patch(&format!("/api/v1/books/{m_id}/reading"))
            .add_header(auth(&basic).0, auth(&basic).1)
            .add_header(
                axum::http::header::IF_MATCH,
                HeaderValue::from_str(&etag).unwrap(),
            )
            .json(&json!({"notes": null}))
            .await;
        assert_eq!(r.status_code(), StatusCode::OK, "body: {}", r.text());
        let body: serde_json::Value = r.json();
        assert!(body["notes"].is_null());
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn patch_reading_status_reading_stamps_started_at_once(pool: PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_work, m_id) =
            test_support::db::insert_work_and_manifestation(&ingestion_pool, "started-once").await;
        let (_user_id, basic) =
            test_support::db::create_adult_and_basic_auth(&app_pool, "started-once").await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

        let initial = server
            .get(&format!("/api/v1/books/{m_id}/reading"))
            .add_header(auth(&basic).0.clone(), auth(&basic).1.clone())
            .await;
        let etag = etag_value(initial.headers());

        let first = server
            .patch(&format!("/api/v1/books/{m_id}/reading"))
            .add_header(auth(&basic).0.clone(), auth(&basic).1.clone())
            .add_header(
                axum::http::header::IF_MATCH,
                HeaderValue::from_str(&etag).unwrap(),
            )
            .json(&json!({"status": "reading"}))
            .await;
        let etag = etag_value(first.headers());
        let first_body: serde_json::Value = first.json();
        let started_at_1 = first_body["started_at"]
            .as_str()
            .expect("started_at set on first reading transition")
            .to_owned();

        tokio::time::sleep(std::time::Duration::from_millis(5)).await;

        let second = server
            .patch(&format!("/api/v1/books/{m_id}/reading"))
            .add_header(auth(&basic).0, auth(&basic).1)
            .add_header(
                axum::http::header::IF_MATCH,
                HeaderValue::from_str(&etag).unwrap(),
            )
            .json(&json!({"status": "reading"}))
            .await;
        let second_body: serde_json::Value = second.json();
        assert_eq!(
            second_body["started_at"].as_str(),
            Some(started_at_1.as_str()),
            "re-entering reading must not re-stamp started_at"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn patch_reading_status_finished_stamps_progress_and_timestamps(pool: PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_work, m_id) =
            test_support::db::insert_work_and_manifestation(&ingestion_pool, "finished").await;
        let (_user_id, basic) =
            test_support::db::create_adult_and_basic_auth(&app_pool, "finished").await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

        let initial = server
            .get(&format!("/api/v1/books/{m_id}/reading"))
            .add_header(auth(&basic).0.clone(), auth(&basic).1.clone())
            .await;
        let etag = etag_value(initial.headers());

        let r = server
            .patch(&format!("/api/v1/books/{m_id}/reading"))
            .add_header(auth(&basic).0, auth(&basic).1)
            .add_header(
                axum::http::header::IF_MATCH,
                HeaderValue::from_str(&etag).unwrap(),
            )
            .json(&json!({"status": "finished"}))
            .await;
        assert_eq!(r.status_code(), StatusCode::OK, "body: {}", r.text());
        let body: serde_json::Value = r.json();
        assert_eq!(body["progress_pct"].as_f64(), Some(100.0));
        assert!(body["finished_at"].is_string());
        assert!(body["last_read_at"].is_string());
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn patch_reading_status_null_after_finished_leaves_timestamps(pool: PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_work, m_id) =
            test_support::db::insert_work_and_manifestation(&ingestion_pool, "clear-after-finish")
                .await;
        let (_user_id, basic) =
            test_support::db::create_adult_and_basic_auth(&app_pool, "clear-after-finish").await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

        let initial = server
            .get(&format!("/api/v1/books/{m_id}/reading"))
            .add_header(auth(&basic).0.clone(), auth(&basic).1.clone())
            .await;
        let etag = etag_value(initial.headers());

        let finished = server
            .patch(&format!("/api/v1/books/{m_id}/reading"))
            .add_header(auth(&basic).0.clone(), auth(&basic).1.clone())
            .add_header(
                axum::http::header::IF_MATCH,
                HeaderValue::from_str(&etag).unwrap(),
            )
            .json(&json!({"status": "finished"}))
            .await;
        let etag = etag_value(finished.headers());
        let finished_body: serde_json::Value = finished.json();
        let finished_at = finished_body["finished_at"]
            .as_str()
            .expect("finished_at set")
            .to_owned();

        let cleared = server
            .patch(&format!("/api/v1/books/{m_id}/reading"))
            .add_header(auth(&basic).0, auth(&basic).1)
            .add_header(
                axum::http::header::IF_MATCH,
                HeaderValue::from_str(&etag).unwrap(),
            )
            .json(&json!({"status": null}))
            .await;
        assert_eq!(
            cleared.status_code(),
            StatusCode::OK,
            "body: {}",
            cleared.text()
        );
        let cleared_body: serde_json::Value = cleared.json();
        assert!(cleared_body["status"].is_null());
        assert_eq!(
            cleared_body["finished_at"].as_str(),
            Some(finished_at.as_str()),
            "clearing status must not erase prior transition timestamps"
        );
        assert_eq!(cleared_body["progress_pct"].as_f64(), Some(100.0));
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn patch_reading_requires_auth(pool: PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
        let r = server
            .patch(&format!("/api/v1/books/{}/reading", Uuid::new_v4()))
            .json(&json!({"status": "reading"}))
            .await;
        assert_eq!(r.status_code(), StatusCode::UNAUTHORIZED);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn patch_reading_unrelated_patch_preserves_finished_at(pool: PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_work, m_id) =
            test_support::db::insert_work_and_manifestation(&ingestion_pool, "stamp-guard").await;
        let (_user_id, basic) =
            test_support::db::create_adult_and_basic_auth(&app_pool, "stamp-guard").await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

        let initial = server
            .get(&format!("/api/v1/books/{m_id}/reading"))
            .add_header(auth(&basic).0.clone(), auth(&basic).1.clone())
            .await;
        let etag = etag_value(initial.headers());

        let finished = server
            .patch(&format!("/api/v1/books/{m_id}/reading"))
            .add_header(auth(&basic).0.clone(), auth(&basic).1.clone())
            .add_header(
                axum::http::header::IF_MATCH,
                HeaderValue::from_str(&etag).unwrap(),
            )
            .json(&json!({"status": "finished"}))
            .await;
        let etag = etag_value(finished.headers());
        let finished_body: serde_json::Value = finished.json();
        let finished_at = finished_body["finished_at"]
            .as_str()
            .expect("finished_at set")
            .to_owned();
        let last_read_at = finished_body["last_read_at"]
            .as_str()
            .expect("last_read_at set")
            .to_owned();

        tokio::time::sleep(std::time::Duration::from_millis(5)).await;

        let r = server
            .patch(&format!("/api/v1/books/{m_id}/reading"))
            .add_header(auth(&basic).0, auth(&basic).1)
            .add_header(
                axum::http::header::IF_MATCH,
                HeaderValue::from_str(&etag).unwrap(),
            )
            .json(&json!({"rating": 4}))
            .await;
        assert_eq!(r.status_code(), StatusCode::OK, "body: {}", r.text());
        let body: serde_json::Value = r.json();
        assert_eq!(body["status"], "finished");
        assert_eq!(body["rating"], 4);
        assert_eq!(
            body["finished_at"].as_str(),
            Some(finished_at.as_str()),
            "rating-only patch must not re-stamp finished_at"
        );
        assert_eq!(
            body["last_read_at"].as_str(),
            Some(last_read_at.as_str()),
            "rating-only patch must not re-stamp last_read_at"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn patch_reading_concurrent_first_writes_both_survive(pool: PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_work, m_id) =
            test_support::db::insert_work_and_manifestation(&ingestion_pool, "merge-race").await;
        let (_user_id, basic) =
            test_support::db::create_adult_and_basic_auth(&app_pool, "merge-race").await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

        let initial = server
            .get(&format!("/api/v1/books/{m_id}/reading"))
            .add_header(auth(&basic).0.clone(), auth(&basic).1.clone())
            .await;
        let etag = etag_value(initial.headers());

        let rating_patch = server
            .patch(&format!("/api/v1/books/{m_id}/reading"))
            .add_header(auth(&basic).0, auth(&basic).1)
            .add_header(
                axum::http::header::IF_MATCH,
                HeaderValue::from_str(&etag).unwrap(),
            )
            .json(&json!({"rating": 4}));
        let notes_patch = server
            .patch(&format!("/api/v1/books/{m_id}/reading"))
            .add_header(auth(&basic).0, auth(&basic).1)
            .add_header(
                axum::http::header::IF_MATCH,
                HeaderValue::from_str(&etag).unwrap(),
            )
            .json(&json!({"notes": "kept"}));
        let (a, b) = tokio::join!(rating_patch, notes_patch);
        // Both racers carry the same tag, so exactly one lands and the
        // other is told to re-read: the lost-update window is closed.
        let mut statuses = [a.status_code(), b.status_code()];
        statuses.sort();
        assert_eq!(
            statuses,
            [StatusCode::OK, StatusCode::PRECONDITION_FAILED],
            "bodies: {} / {}",
            a.text(),
            b.text()
        );
        let (winner, loser) = if a.status_code() == StatusCode::OK {
            (a, b)
        } else {
            (b, a)
        };
        let current = etag_value(winner.headers());
        let retry_body = if winner.json::<serde_json::Value>()["rating"] == 4 {
            json!({"notes": "kept"})
        } else {
            json!({"rating": 4})
        };
        assert_ne!(etag_value(loser.headers()), etag, "412 carries current tag");
        let retry = server
            .patch(&format!("/api/v1/books/{m_id}/reading"))
            .add_header(auth(&basic).0, auth(&basic).1)
            .add_header(
                axum::http::header::IF_MATCH,
                HeaderValue::from_str(&current).unwrap(),
            )
            .json(&retry_body)
            .await;
        assert_eq!(
            retry.status_code(),
            StatusCode::OK,
            "body: {}",
            retry.text()
        );

        let r = server
            .get(&format!("/api/v1/books/{m_id}/reading"))
            .add_header(auth(&basic).0, auth(&basic).1)
            .await;
        let body: serde_json::Value = r.json();
        assert_eq!(body["rating"], 4, "concurrent write lost: {body}");
        assert_eq!(body["notes"], "kept", "concurrent write lost: {body}");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn reading_rls_hidden_book_returns_404(pool: PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_work, m_id) =
            test_support::db::insert_work_and_manifestation(&ingestion_pool, "shelf-gate").await;
        let (_child_id, child_basic) =
            test_support::db::create_child_user_and_basic_auth(&app_pool, "shelf-gate").await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

        // The manifestation exists but sits on no shelf, so the child RLS
        // policy hides it; both verbs must 404 exactly like a missing id.
        let r = server
            .get(&format!("/api/v1/books/{m_id}/reading"))
            .add_header(auth(&child_basic).0.clone(), auth(&child_basic).1.clone())
            .await;
        test_support::assert_problem(&r, problems::NOT_FOUND, StatusCode::NOT_FOUND);

        let r = server
            .patch(&format!("/api/v1/books/{m_id}/reading"))
            .add_header(auth(&child_basic).0, auth(&child_basic).1)
            .add_header(
                axum::http::header::IF_MATCH,
                HeaderValue::from_static("\"dummy\""),
            )
            .json(&json!({"status": "reading"}))
            .await;
        test_support::assert_problem(&r, problems::NOT_FOUND, StatusCode::NOT_FOUND);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn patch_reading_notes_over_cap_rejected(pool: PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_work, m_id) =
            test_support::db::insert_work_and_manifestation(&ingestion_pool, "notes-limit").await;
        let (_user_id, basic) =
            test_support::db::create_adult_and_basic_auth(&app_pool, "notes-limit").await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

        let r = server
            .patch(&format!("/api/v1/books/{m_id}/reading"))
            .add_header(auth(&basic).0, auth(&basic).1)
            .add_header(
                axum::http::header::IF_MATCH,
                HeaderValue::from_static("\"dummy\""),
            )
            .json(&json!({"notes": "n".repeat(10_001)}))
            .await;
        test_support::assert_problem(&r, problems::VALIDATION, StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn patch_reading_rating_out_of_range_rejected(pool: PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_work, m_id) =
            test_support::db::insert_work_and_manifestation(&ingestion_pool, "rating-range").await;
        let (_user_id, basic) =
            test_support::db::create_adult_and_basic_auth(&app_pool, "rating-range").await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

        for bad_rating in [0, 6] {
            let r = server
                .patch(&format!("/api/v1/books/{m_id}/reading"))
                .add_header(auth(&basic).0.clone(), auth(&basic).1.clone())
                .add_header(
                    axum::http::header::IF_MATCH,
                    HeaderValue::from_static("\"dummy\""),
                )
                .json(&json!({"rating": bad_rating}))
                .await;
            test_support::assert_problem(
                &r,
                problems::VALIDATION,
                StatusCode::UNPROCESSABLE_ENTITY,
            );
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn patch_reading_empty_patch_rejected(pool: PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_work, m_id) =
            test_support::db::insert_work_and_manifestation(&ingestion_pool, "empty-patch").await;
        let (_user_id, basic) =
            test_support::db::create_adult_and_basic_auth(&app_pool, "empty-patch").await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

        let r = server
            .patch(&format!("/api/v1/books/{m_id}/reading"))
            .add_header(auth(&basic).0, auth(&basic).1)
            .add_header(
                axum::http::header::IF_MATCH,
                HeaderValue::from_static("\"dummy\""),
            )
            .json(&json!({}))
            .await;
        test_support::assert_problem(&r, problems::VALIDATION, StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn cross_user_reading_state_is_isolated(pool: PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_work, m_id) =
            test_support::db::insert_work_and_manifestation(&ingestion_pool, "isolation").await;
        let (_a_id, a_basic) =
            test_support::db::create_adult_and_basic_auth(&app_pool, "isolation-a").await;
        let (_b_id, b_basic) =
            test_support::db::create_adult_and_basic_auth(&app_pool, "isolation-b").await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

        let initial = server
            .get(&format!("/api/v1/books/{m_id}/reading"))
            .add_header(auth(&a_basic).0.clone(), auth(&a_basic).1.clone())
            .await;
        let etag = etag_value(initial.headers());

        server
            .patch(&format!("/api/v1/books/{m_id}/reading"))
            .add_header(auth(&a_basic).0, auth(&a_basic).1)
            .add_header(
                axum::http::header::IF_MATCH,
                HeaderValue::from_str(&etag).unwrap(),
            )
            .json(&json!({"status": "finished", "rating": 5}))
            .await;

        let r = server
            .get(&format!("/api/v1/books/{m_id}/reading"))
            .add_header(auth(&b_basic).0, auth(&b_basic).1)
            .await;
        assert_eq!(r.status_code(), StatusCode::OK, "body: {}", r.text());
        let body: serde_json::Value = r.json();
        assert!(
            body["status"].is_null(),
            "user B must not see user A's reading state: {body}"
        );
        assert!(body["rating"].is_null());
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn child_account_can_read_and_write_own_reading_state(pool: PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_work, m_id) =
            test_support::db::insert_work_and_manifestation(&ingestion_pool, "child-rw").await;
        let (child_id, child_basic) =
            test_support::db::create_child_user_and_basic_auth(&app_pool, "child-rw").await;
        // Child manifestation visibility is shelf-gated (manifestations_select_child
        // RLS policy): put the book on the child's own shelf so the RLS-scoped
        // manifestation probe in the handler sees it.
        let shelf_id = test_support::db::create_shelf(&app_pool, child_id, "Child shelf").await;
        test_support::db::add_to_shelf(&app_pool, shelf_id, m_id).await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

        let initial = server
            .get(&format!("/api/v1/books/{m_id}/reading"))
            .add_header(auth(&child_basic).0.clone(), auth(&child_basic).1.clone())
            .await;
        let etag = etag_value(initial.headers());

        let patched = server
            .patch(&format!("/api/v1/books/{m_id}/reading"))
            .add_header(auth(&child_basic).0.clone(), auth(&child_basic).1.clone())
            .add_header(
                axum::http::header::IF_MATCH,
                HeaderValue::from_str(&etag).unwrap(),
            )
            .json(&json!({"status": "reading", "rating": 5}))
            .await;
        assert_eq!(
            patched.status_code(),
            StatusCode::OK,
            "child must be able to write their own reading state: {}",
            patched.text()
        );

        let r = server
            .get(&format!("/api/v1/books/{m_id}/reading"))
            .add_header(auth(&child_basic).0, auth(&child_basic).1)
            .await;
        assert_eq!(r.status_code(), StatusCode::OK, "body: {}", r.text());
        let body: serde_json::Value = r.json();
        assert_eq!(body["status"], "reading");
        assert_eq!(body["rating"], 5);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn get_reading_emits_etag_for_unread_default(pool: PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_work, m_id) =
            test_support::db::insert_work_and_manifestation(&ingestion_pool, "etag-default").await;
        let (_user_id, basic) =
            test_support::db::create_adult_and_basic_auth(&app_pool, "etag-default").await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

        let r = server
            .get(&format!("/api/v1/books/{m_id}/reading"))
            .add_header(auth(&basic).0, auth(&basic).1)
            .await;
        assert_eq!(r.status_code(), StatusCode::OK);
        let etag = etag_value(r.headers());
        assert!(etag.starts_with('"') && etag.ends_with('"'));
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn get_reading_etag_stable_across_identical_reads(pool: PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_work, m_id) =
            test_support::db::insert_work_and_manifestation(&ingestion_pool, "etag-stable").await;
        let (_user_id, basic) =
            test_support::db::create_adult_and_basic_auth(&app_pool, "etag-stable").await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

        let initial = server
            .get(&format!("/api/v1/books/{m_id}/reading"))
            .add_header(auth(&basic).0.clone(), auth(&basic).1.clone())
            .await;
        let etag = etag_value(initial.headers());

        server
            .patch(&format!("/api/v1/books/{m_id}/reading"))
            .add_header(auth(&basic).0.clone(), auth(&basic).1.clone())
            .add_header(
                axum::http::header::IF_MATCH,
                HeaderValue::from_str(&etag).unwrap(),
            )
            .json(&json!({"rating": 3}))
            .await;

        let first = server
            .get(&format!("/api/v1/books/{m_id}/reading"))
            .add_header(auth(&basic).0.clone(), auth(&basic).1.clone())
            .await;
        let second = server
            .get(&format!("/api/v1/books/{m_id}/reading"))
            .add_header(auth(&basic).0, auth(&basic).1)
            .await;
        assert_eq!(etag_value(first.headers()), etag_value(second.headers()));
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn patch_reading_bumps_etag_after_write(pool: PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_work, m_id) =
            test_support::db::insert_work_and_manifestation(&ingestion_pool, "etag-bump").await;
        let (_user_id, basic) =
            test_support::db::create_adult_and_basic_auth(&app_pool, "etag-bump").await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

        let before = server
            .get(&format!("/api/v1/books/{m_id}/reading"))
            .add_header(auth(&basic).0.clone(), auth(&basic).1.clone())
            .await;
        let before_etag = etag_value(before.headers());

        let r = server
            .patch(&format!("/api/v1/books/{m_id}/reading"))
            .add_header(auth(&basic).0, auth(&basic).1)
            .add_header(
                axum::http::header::IF_MATCH,
                HeaderValue::from_str(&before_etag).unwrap(),
            )
            .json(&json!({"rating": 4}))
            .await;
        assert_eq!(r.status_code(), StatusCode::OK, "body: {}", r.text());
        let after_etag = etag_value(r.headers());
        assert_ne!(
            before_etag, after_etag,
            "a real field change must change the ETag"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn patch_reading_without_if_match_returns_428(pool: PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_work, m_id) =
            test_support::db::insert_work_and_manifestation(&ingestion_pool, "no-if-match").await;
        let (_user_id, basic) =
            test_support::db::create_adult_and_basic_auth(&app_pool, "no-if-match").await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

        let r = server
            .patch(&format!("/api/v1/books/{m_id}/reading"))
            .add_header(auth(&basic).0, auth(&basic).1)
            .json(&json!({"rating": 2}))
            .await;
        test_support::assert_problem(
            &r,
            problems::IF_MATCH_REQUIRED,
            StatusCode::PRECONDITION_REQUIRED,
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn patch_reading_with_matching_if_match_succeeds(pool: PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_work, m_id) =
            test_support::db::insert_work_and_manifestation(&ingestion_pool, "match-ok").await;
        let (_user_id, basic) =
            test_support::db::create_adult_and_basic_auth(&app_pool, "match-ok").await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

        // The "unread" default ETag protects a first write, before any row
        // exists.
        let initial = server
            .get(&format!("/api/v1/books/{m_id}/reading"))
            .add_header(auth(&basic).0.clone(), auth(&basic).1.clone())
            .await;
        let etag = etag_value(initial.headers());

        let r = server
            .patch(&format!("/api/v1/books/{m_id}/reading"))
            .add_header(auth(&basic).0, auth(&basic).1)
            .add_header(
                axum::http::header::IF_MATCH,
                HeaderValue::from_str(&etag).unwrap(),
            )
            .json(&json!({"status": "want_to_read"}))
            .await;
        assert_eq!(r.status_code(), StatusCode::OK, "body: {}", r.text());
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn patch_reading_with_stale_if_match_returns_412_with_current_etag(pool: PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_work, m_id) =
            test_support::db::insert_work_and_manifestation(&ingestion_pool, "stale-match").await;
        let (_user_id, basic) =
            test_support::db::create_adult_and_basic_auth(&app_pool, "stale-match").await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

        let initial = server
            .get(&format!("/api/v1/books/{m_id}/reading"))
            .add_header(auth(&basic).0.clone(), auth(&basic).1.clone())
            .await;
        let stale_etag = etag_value(initial.headers());

        // Someone else's write lands first, invalidating the captured tag.
        server
            .patch(&format!("/api/v1/books/{m_id}/reading"))
            .add_header(auth(&basic).0.clone(), auth(&basic).1.clone())
            .add_header(
                axum::http::header::IF_MATCH,
                HeaderValue::from_str(&stale_etag).unwrap(),
            )
            .json(&json!({"rating": 5}))
            .await;

        let r = server
            .patch(&format!("/api/v1/books/{m_id}/reading"))
            .add_header(auth(&basic).0, auth(&basic).1)
            .add_header(
                axum::http::header::IF_MATCH,
                HeaderValue::from_str(&stale_etag).unwrap(),
            )
            .json(&json!({"rating": 1}))
            .await;
        test_support::assert_problem(
            &r,
            problems::IF_MATCH_MISMATCH,
            StatusCode::PRECONDITION_FAILED,
        );
        let current_etag = etag_value(r.headers());
        assert_ne!(
            current_etag, stale_etag,
            "412 must carry the current, not the stale, ETag"
        );
    }

    // Reading shares the metadata parser, so it owes the same status
    // contract: refused header forms are 400, never the 412 a false
    // precondition earns.
    #[sqlx::test(migrations = "./migrations")]
    async fn patch_reading_with_refused_if_match_returns_400(pool: PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_work, m_id) =
            test_support::db::insert_work_and_manifestation(&ingestion_pool, "refused-match").await;
        let (_user_id, basic) =
            test_support::db::create_adult_and_basic_auth(&app_pool, "refused-match").await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

        for (label, if_match) in [
            ("SP is not etagc", "\"a b\""),
            ("wildcard", "*"),
            ("entity-tag list", "\"abc\", \"def\""),
            ("weak validator", "W/\"abc\""),
            ("unquoted", "abc"),
        ] {
            let r = server
                .patch(&format!("/api/v1/books/{m_id}/reading"))
                .add_header(auth(&basic).0, auth(&basic).1)
                .add_header(
                    axum::http::header::IF_MATCH,
                    HeaderValue::from_str(if_match).unwrap(),
                )
                .json(&json!({"rating": 2}))
                .await;
            assert_eq!(
                r.status_code(),
                StatusCode::BAD_REQUEST,
                "{label}: body: {}",
                r.text()
            );
            test_support::assert_problem(&r, problems::MALFORMED_HEADER, StatusCode::BAD_REQUEST);
        }
    }

    // `obs-text` is valid entity-tag content, so such a tag is compared
    // rather than refused, and compares unequal against Reverie's ASCII
    // validators.
    #[sqlx::test(migrations = "./migrations")]
    async fn patch_reading_with_obs_text_if_match_reaches_comparison(pool: PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_work, m_id) =
            test_support::db::insert_work_and_manifestation(&ingestion_pool, "obs-text").await;
        let (_user_id, basic) =
            test_support::db::create_adult_and_basic_auth(&app_pool, "obs-text").await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

        let r = server
            .patch(&format!("/api/v1/books/{m_id}/reading"))
            .add_header(auth(&basic).0, auth(&basic).1)
            .add_header(
                axum::http::header::IF_MATCH,
                HeaderValue::from_bytes(b"\"\x80obs-text\"").unwrap(),
            )
            .json(&json!({"rating": 2}))
            .await;
        test_support::assert_problem(
            &r,
            problems::IF_MATCH_MISMATCH,
            StatusCode::PRECONDITION_FAILED,
        );
    }

    // Body validation short-circuits before the precondition is compared, so
    // a body-contract failure is 422 whether the tag would have matched or
    // not. Both tags are asserted to pin that ordering: comparing the tag
    // first would turn the stale-tag case into a 412.
    #[sqlx::test(migrations = "./migrations")]
    async fn patch_reading_body_violation_returns_422_regardless_of_if_match(pool: PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_work, m_id) =
            test_support::db::insert_work_and_manifestation(&ingestion_pool, "body-422").await;
        let (_user_id, basic) =
            test_support::db::create_adult_and_basic_auth(&app_pool, "body-422").await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

        let initial = server
            .get(&format!("/api/v1/books/{m_id}/reading"))
            .add_header(auth(&basic).0.clone(), auth(&basic).1.clone())
            .await;
        let matching = etag_value(initial.headers());

        for (label, if_match) in [
            ("matching tag", matching.as_str()),
            ("valid tag that cannot match", "\"stale\""),
        ] {
            let r = server
                .patch(&format!("/api/v1/books/{m_id}/reading"))
                .add_header(auth(&basic).0, auth(&basic).1)
                .add_header(
                    axum::http::header::IF_MATCH,
                    HeaderValue::from_str(if_match).unwrap(),
                )
                .json(&json!({"rating": 9}))
                .await;
            assert_eq!(
                r.status_code(),
                StatusCode::UNPROCESSABLE_ENTITY,
                "{label}: body: {}",
                r.text()
            );
        }
    }
}
