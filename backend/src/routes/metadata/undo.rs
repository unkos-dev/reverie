use crate::auth::{middleware::CurrentUser, scope::Scope};
use crate::extract::{ApiJson, ApiPath};
use crate::routes::etag::{Conditional, if_match_mismatch, parse_if_match};
use crate::services::metadata::editing::undo::{
    self, OperationResponse, ResolveRequest, UndoRequest,
};
#[cfg(test)]
pub(super) use crate::services::metadata::editing::undo::{
    MAX_CAPTURE_BYTES, MAX_CAPTURE_ENTRIES, ResourceBudget, capture,
};
use crate::{error::AppError, state::AppState};
use axum::extract::State;
use axum::http::header::{CACHE_CONTROL, ETAG};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use tower_sessions::Session;
use uuid::Uuid;

fn operation_response(result: undo::OperationResult) -> Response {
    let mut headers = HeaderMap::new();
    headers.insert(ETAG, result.etag);
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    (
        StatusCode::OK,
        headers,
        axum::Json(OperationResponse {
            values: result.snapshot.values,
            outcome: result.outcome,
            operations: result.operations,
        }),
    )
        .into_response()
}

#[utoipa::path(
    post, path = "/api/v1/books/{id}/metadata/undo", tag = "metadata",
    summary = "Undo one saved metadata field",
    description = "Restores the exact captured field state once for its issuing browser session. Requires current aggregate If-Match and an eligible, unlocked field. Confidence changes retain field-identity invalidation. No write is replayed automatically.",
    security(("session_cookie" = ["write"])),
    params(("id" = Uuid, Path, max_length = 45, description = "Manifestation id"), ("If-Match" = String, Header, max_length = 24, description = "Current quoted 128-bit metadata entity-tag returned by a matched response. Other well-formed tags return 412")),
    request_body = UndoRequest,
    responses(
        (status = 200, description = "Exact field restoration committed", body = OperationResponse, headers(("ETag" = String, description = "Metadata validator matching the returned values"))),
        (status = 400, description = "Malformed body, path or If-Match", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 403, description = "Browser session and adult write scope required", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 404, description = "Edition missing or hidden", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 409, description = "Field locked or recorded after-state conflicts; operation not consumed", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 412, description = "Aggregate metadata validator is stale", body = crate::openapi::ProblemDetails, content_type = "application/problem+json", headers(("ETag" = String, description = "Current metadata validator"))),
        (status = 422, description = "Operation unavailable or Undo resource limit exceeded", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 428, description = "If-Match or CSRF token missing", body = crate::openapi::ProblemDetails, content_type = "application/problem+json")
    )
)]
pub(super) async fn undo_metadata(
    user: CurrentUser,
    State(state): State<AppState>,
    session: Session,
    ApiPath(edition): ApiPath<Uuid>,
    headers: HeaderMap,
    ApiJson(request): ApiJson<UndoRequest>,
) -> Result<Response, AppError> {
    user.require_scope(Scope::Write)?;
    user.require_not_child()?;
    undo::session_id(&user, &session)?;
    let supplied = parse_if_match(&headers)?;
    match undo::undo_metadata(
        &user,
        &state,
        &session,
        edition,
        supplied,
        request,
        &crate::services::metadata::editing::SnapshotSelection::Complete,
    )
    .await?
    {
        Conditional::Mismatch(tag) => Ok(if_match_mismatch(&tag)),
        Conditional::Matched(result) => Ok(operation_response(result)),
    }
}

#[utoipa::path(
    post, path = "/api/v1/books/{id}/metadata/operations/resolve", tag = "metadata",
    summary = "Resolve a metadata operation outcome",
    description = "Returns current paired metadata and session-owned committed evidence for one save request or field operation. An absent receipt means unknown, never proof of rollback. This read does not restore or replay a write.",
    security(("session_cookie" = ["write"])),
    params(("id" = Uuid, Path, max_length = 45, description = "Manifestation id")),
    request_body = ResolveRequest,
    responses(
        (status = 200, description = "Current metadata and operation evidence", body = OperationResponse, headers(("ETag" = String, description = "Metadata validator matching the returned values"))),
        (status = 400, description = "Malformed body or path", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 403, description = "Browser session and adult write scope required", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 404, description = "Edition missing or hidden", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 422, description = "Select exactly one request or operation identifier; operation evidence must fit the resource limit", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 428, description = "CSRF token missing", body = crate::openapi::ProblemDetails, content_type = "application/problem+json")
    )
)]
pub(super) async fn resolve_metadata(
    user: CurrentUser,
    State(state): State<AppState>,
    session: Session,
    ApiPath(edition): ApiPath<Uuid>,
    ApiJson(request): ApiJson<ResolveRequest>,
) -> Result<Response, AppError> {
    Ok(operation_response(
        undo::resolve_metadata(
            &user,
            &state,
            &session,
            edition,
            request,
            &crate::services::metadata::editing::SnapshotSelection::Complete,
        )
        .await?,
    ))
}
