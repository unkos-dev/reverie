//! `/api/v1/books/{id}/reading`, the per-user reading state endpoints:
//! status, rating, notes, progress, and reading dates.
//!
//! THREAT: unlike `metadata` and `shelves`, these handlers do NOT call
//! `require_not_child()`. Reading state is self-scoped personal data (a
//! child recording their own status/rating/notes), not shared-library
//! curation, so child accounts may read and write their own row. RLS still
//! confines every read/write to rows the caller owns and to manifestations
//! visible under the caller's adult/child policy.

use crate::auth::{middleware::CurrentUser, scope::Scope};
use crate::extract::{ApiJson, ApiPath};
use crate::models::reading_state::ReadingState;
use crate::routes::etag::{Conditional, if_match_mismatch, parse_if_match};
use crate::services::reading::{self, UpdateReadingRequest};
use crate::{error::AppError, state::AppState};
use axum::extract::State;
use axum::http::{HeaderMap, header::ETAG};
use axum::response::{IntoResponse, Response};
use utoipa_axum::{router::OpenApiRouter, routes};
use uuid::Uuid;

/// Build the `/api/v1/books/{id}/reading` router as an [`OpenApiRouter`].
/// Merged into `crate::openapi::pilot_router`.
pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(get_reading, patch_reading))
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
    summary = "Get reading state",
    description = "Returns the caller's reading state for one book; all-null fields mean the book is unread.",
    tag = "reading",
    security(("session_cookie" = ["read"]), ("device_token_bearer" = ["read"]), ("opds_basic" = ["read"])),
    params(("id" = Uuid, Path, max_length = 45, description = "Manifestation id")),
    responses(
        (status = 200, description = "Caller's reading state; all-null fields mean unread (no row yet)", body = ReadingState,
         headers(("ETag" = String, description = "Strong entity-tag hashing every PATCH-modifiable field; echo as If-Match on a protected write"))),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 404, description = "Manifestation missing or RLS-hidden (existence-not-leaked)", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
    )
)]
async fn get_reading(
    current_user: CurrentUser,
    State(state): State<AppState>,
    ApiPath(manifestation_id): ApiPath<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    current_user.require_scope(Scope::Read)?;

    let row =
        crate::services::reader::reading_state(&current_user, &state, manifestation_id).await?;
    let mut headers = HeaderMap::new();
    headers.insert(ETAG, row.etag()?);
    Ok((headers, axum::Json(ReadingState::from(row))))
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
///   Reached only once `If-Match` has matched, so a stale tag yields 412
///   rather than a body error.
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
    summary = "Update reading state",
    description = "Applies a partial update to the caller's reading state for one book, such as status, rating, or notes. Requires an `If-Match` header; fails with 428 when it is absent, 412 when it does not match the caller's current entity-tag, and 400 when it is malformed or refused by policy.",
    tag = "reading",
    security(("session_cookie" = ["write"]), ("device_token_bearer" = ["write"]), ("opds_basic" = ["write"])),
    params(
        ("id" = Uuid, Path, max_length = 45, description = "Manifestation id"),
        ("If-Match" = String, Header, description = "Exactly one quoted strong entity-tag, as returned in a prior GET or PATCH response's ETag header. Opaque content follows the full RFC 9110 8.8.3 grammar, obs-text included. The * wildcard, entity-tag lists, weak tags, and repeated instances of this field are refused with 400. Required: absent means 428; unequal means 412")
    ),
    request_body(content = UpdateReadingRequest, description = "RFC 7396 JSON Merge Patch: absent fields are unchanged, `null` clears"),
    responses(
        (status = 200, description = "Reading state after the patch and any transition stamps", body = ReadingState,
         headers(("ETag" = String, description = "Strong entity-tag hashing every PATCH-modifiable field, reflecting the state after this write"))),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 404, description = "Manifestation missing or RLS-hidden (existence-not-leaked)", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 412, description = "If-Match does not match the caller's current reading-state ETag", body = crate::openapi::ProblemDetails, content_type = "application/problem+json",
         headers(("ETag" = String, description = "Current entity-tag, so the caller can resync without a follow-up GET"))),
        (status = 400, description = "If-Match is malformed, or carries a form this API refuses by policy: the * wildcard, an entity-tag list, a weak tag, or a repeated header instance, the request body is not valid JSON, or a path parameter is malformed", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 422, description = "Empty patch, rating outside 1-5, or notes over 10000 characters. Evaluated only after If-Match has matched, so a stale tag returns 412 instead", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 428, description = "If-Match header absent", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
    )
)]
async fn patch_reading(
    current_user: CurrentUser,
    State(state): State<AppState>,
    ApiPath(manifestation_id): ApiPath<Uuid>,
    headers_in: HeaderMap,
    ApiJson(req): ApiJson<UpdateReadingRequest>,
) -> Result<Response, AppError> {
    current_user.require_scope(Scope::Write)?;
    let supplied = parse_if_match(&headers_in)?;
    match reading::patch_reading(&current_user, &state, manifestation_id, supplied, req).await? {
        Conditional::Mismatch(tag) => Ok(if_match_mismatch(&tag)),
        Conditional::Matched(row) => {
            let mut headers = HeaderMap::new();
            headers.insert(ETAG, row.etag()?);
            Ok((headers, axum::Json(ReadingState::from(row))).into_response())
        }
    }
}

#[cfg(test)]
mod tests {
    use axum::http::{HeaderName, HeaderValue, StatusCode};
    use serde_json::json;
    use sqlx::PgPool;

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
    async fn patch_reading_without_if_match_returns_428(pool: PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_work, m_id) =
            test_support::db::insert_work_and_manifestation(&ingestion_pool, "no-if-match").await;
        let (_user_id, basic) =
            test_support::db::create_adult_and_basic_auth(&app_pool, "no-if-match").await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
        let before = server
            .get(&format!("/api/v1/books/{m_id}/reading"))
            .add_header(auth(&basic).0, auth(&basic).1)
            .await;

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
        let after = server
            .get(&format!("/api/v1/books/{m_id}/reading"))
            .add_header(auth(&basic).0, auth(&basic).1)
            .await;
        assert_eq!(etag_value(after.headers()), etag_value(before.headers()));
        assert_eq!(after.text(), before.text());
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
}
