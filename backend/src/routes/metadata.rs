//! Metadata review endpoints.
//!
//! All routes require an authenticated non-child user.  Write paths open a
//! transaction, `SELECT ... FOR UPDATE` on the owning entity, apply the change,
//! and commit.

use crate::auth::{middleware::CurrentUser, scope::Scope};
use crate::extract::{ApiJson, ApiPath};
use crate::routes::etag::{Conditional, if_match_mismatch, parse_if_match};
use crate::services::metadata::editing::{
    self, BookMetadata, LockPayload, RevertPayload, UpdateMetadataFields, UpdateMetadataResponse,
    VersionPayload,
};
#[cfg(test)]
use crate::services::metadata::editing::{
    VocabularyField, apply_version, clear_field, insert_vocabulary_rows, lock_visible_manifestation,
};
use crate::{db, error::AppError, state::AppState};
use axum::extract::State;
use axum::http::header::{CACHE_CONTROL, ETAG};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use serde_json::Value;
use utoipa_axum::{router::OpenApiRouter, routes};
use uuid::Uuid;

/// Build the metadata-review router.
///
/// Mounts the `GET` views over manifestation/work metadata and the
/// `POST` accept/reject/revert/lock/unlock mutators on `AppState`.
///
/// # Invariants
/// - Every handler requires an authenticated non-child user
///   (`CurrentUser::require_not_child`).
/// - RLS transactions establish `app.current_user_id` for the caller.
/// - Accept and reject acquire the same transaction advisory lock before
///   accessing a version, using Read Committed for the subsequent eligibility read.
/// - Canonical writes retain manifestation/work row locks.
pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(get_manifestation_metadata))
        .routes(routes!(get_work_metadata))
        .routes(routes!(accept_manifestation))
        .routes(routes!(reject_manifestation))
        .routes(routes!(revert_manifestation))
        .routes(routes!(lock_field))
        .routes(routes!(unlock_field))
        .routes(routes!(get_book_metadata, update_book_metadata))
}

/// One `metadata_versions` row in the review queue view.
#[derive(Debug, Serialize, utoipa::ToSchema)]
struct MetadataRow {
    /// Version row id — the handle for accept/reject/revert.
    id: Uuid,
    /// Canonical field name (e.g. `title`, `isbn_13`).
    field_name: String,
    /// Source that produced this value (e.g. `openlibrary`, `manual`).
    source: String,
    /// Proposed value in raw JSON form.
    new_value: Value,
    /// Review status (`pending`, `applied`, `rejected`, …).
    status: String,
    /// Source-reported confidence in the value.
    confidence_score: f32,
    /// How the source matched the manifestation (e.g. `isbn`, `title`).
    match_type: String,
    /// How many enrichment passes observed this value.
    observation_count: i32,
}

/// `GET /api/v1/manifestations/{id}/metadata` — review queue for one
/// manifestation, newest first.
///
/// # Errors
/// - [`AppError::Forbidden`] when the caller is a child account.
/// - [`AppError::Internal`] on database errors.
#[utoipa::path(
    get,
    path = "/api/v1/manifestations/{id}/metadata",
    summary = "List a manifestation's metadata versions",
    description = "Returns the metadata review queue for one manifestation, newest first; the list is empty when the manifestation is missing or hidden from the caller. Available to adult accounts only.",
    tag = "metadata",
    security(("session_cookie" = ["read"]), ("device_token_bearer" = ["read"]), ("opds_basic" = ["read"])),
    params(("id" = Uuid, Path, max_length = 45, description = "Manifestation id")),
    responses(
        (status = 200, description = "Metadata version rows for the manifestation, newest first (empty when the manifestation is missing or RLS-hidden)", body = [MetadataRow]),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 403, description = "Caller is a child account", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
    )
)]
async fn get_manifestation_metadata(
    current_user: CurrentUser,
    State(state): State<AppState>,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    current_user.require_not_child()?;
    let mut tx = db::acquire_with_rls(&state.pool, current_user.user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    let rows = load_versions(&mut tx, id).await?;
    Ok(axum::Json(rows))
}

/// `GET /api/v1/works/{id}/metadata` — review queue across every
/// manifestation of a work, newest first.
///
/// # Errors
/// - [`AppError::Forbidden`] when the caller is a child account.
/// - [`AppError::Internal`] on database errors.
#[utoipa::path(
    get,
    path = "/api/v1/works/{id}/metadata",
    summary = "List a work's metadata versions",
    description = "Returns the metadata review queue across every manifestation of a work, newest first; the list is empty when the work is missing or hidden from the caller. Available to adult accounts only.",
    tag = "metadata",
    security(("session_cookie" = ["read"]), ("device_token_bearer" = ["read"]), ("opds_basic" = ["read"])),
    params(("id" = Uuid, Path, max_length = 45, description = "Work id")),
    responses(
        (status = 200, description = "Metadata version rows across the work's manifestations, newest first (empty when the work is missing or RLS-hidden)", body = [MetadataRow]),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 403, description = "Caller is a child account", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
    )
)]
async fn get_work_metadata(
    current_user: CurrentUser,
    State(state): State<AppState>,
    ApiPath(work_id): ApiPath<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    current_user.require_not_child()?;

    let mut tx = db::acquire_with_rls(&state.pool, current_user.user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let rows = sqlx::query!(
        "SELECT mv.id, mv.field_name, mv.source, \
                mv.new_value AS \"new_value!\", \
                mv.status::text AS \"status!\", \
                mv.confidence_score AS \"confidence_score!\", \
                mv.match_type, mv.observation_count \
         FROM metadata_versions mv \
         JOIN manifestations m ON m.id = mv.manifestation_id \
         WHERE m.work_id = $1 \
         ORDER BY mv.last_seen_at DESC",
        work_id,
    )
    .fetch_all(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    let rows: Vec<MetadataRow> = rows
        .into_iter()
        .map(|r| MetadataRow {
            id: r.id,
            field_name: r.field_name,
            source: r.source,
            new_value: r.new_value,
            status: r.status,
            confidence_score: r.confidence_score,
            match_type: r.match_type,
            observation_count: r.observation_count,
        })
        .collect();
    Ok(axum::Json(rows))
}

async fn load_versions(
    tx: &mut sqlx::PgConnection,
    manifestation_id: Uuid,
) -> Result<Vec<MetadataRow>, AppError> {
    let rows = sqlx::query!(
        "SELECT id, field_name, source, \
                new_value AS \"new_value!\", \
                status::text AS \"status!\", \
                confidence_score AS \"confidence_score!\", \
                match_type, observation_count \
         FROM metadata_versions \
         WHERE manifestation_id = $1 \
         ORDER BY last_seen_at DESC",
        manifestation_id,
    )
    .fetch_all(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    Ok(rows
        .into_iter()
        .map(|r| MetadataRow {
            id: r.id,
            field_name: r.field_name,
            source: r.source,
            new_value: r.new_value,
            status: r.status,
            confidence_score: r.confidence_score,
            match_type: r.match_type,
            observation_count: r.observation_count,
        })
        .collect())
}

/// `POST /api/v1/manifestations/{id}/metadata/accept` — promote a pending
/// metadata version to canonical.
///
/// # Errors
/// - [`AppError::Forbidden`] when the caller is a child account.
/// - [`AppError::NotFound`] when the version row does not belong to the
///   manifestation, is already rejected, or is RLS-hidden.
/// - [`AppError::Validation`] when the stored value fails field parsing.
/// - [`AppError::Internal`] on database errors.
#[utoipa::path(
    post,
    path = "/api/v1/manifestations/{id}/metadata/accept",
    summary = "Accept a metadata version",
    description = "Promotes a pending metadata version to canonical for a manifestation. Acceptance leaves review status pending; repeated acceptance is allowed and enqueues another writeback job for file-backed fields. Accepting an ISBN change may trigger a re-match against other works. Available to adult accounts only. Returns 404 when the version is missing, belongs to another manifestation, or is already rejected; returns 422 if the stored value fails field parsing.",
    tag = "metadata",
    security(("session_cookie" = ["write"]), ("device_token_bearer" = ["write"]), ("opds_basic" = ["write"])),
    params(("id" = Uuid, Path, max_length = 45, description = "Manifestation id")),
    request_body = VersionPayload,
    responses(
        (status = 409, description = "A requested metadata field is locked on this edition; locked_fields lists the refused keys and no changes are applied", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 200, description = "Version promoted to canonical; accepted ISBN changes refresh the possible-duplicate pointer"),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 403, description = "Caller is a child account", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 404, description = "Version not found for this manifestation, already rejected, or RLS-hidden", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 422, description = "Stored value fails field parsing", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
    )
)]
async fn accept_manifestation(
    current_user: CurrentUser,
    State(state): State<AppState>,
    ApiPath(manifestation_id): ApiPath<Uuid>,
    ApiJson(payload): ApiJson<VersionPayload>,
) -> Result<impl IntoResponse, AppError> {
    editing::accept_manifestation(&current_user, &state, manifestation_id, payload, None).await?;
    Ok(StatusCode::OK)
}

/// `POST /api/v1/manifestations/{id}/metadata/reject` — mark a pending
/// metadata version as rejected.
///
/// # Errors
/// - [`AppError::Forbidden`] when the caller is a child account.
/// - [`AppError::NotFound`] when the version row does not belong to the
///   manifestation or is RLS-hidden.
/// - [`AppError::Internal`] on database errors.
#[utoipa::path(
    post,
    path = "/api/v1/manifestations/{id}/metadata/reject",
    summary = "Reject a metadata version",
    description = "Marks a pending metadata version as rejected. Available to adult accounts only.",
    tag = "metadata",
    security(("session_cookie" = ["write"]), ("device_token_bearer" = ["write"]), ("opds_basic" = ["write"])),
    params(("id" = Uuid, Path, max_length = 45, description = "Manifestation id")),
    request_body = VersionPayload,
    responses(
        (status = 200, description = "Version marked rejected"),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 403, description = "Caller is a child account", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 404, description = "Version not found for this manifestation, or RLS-hidden", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
    )
)]
async fn reject_manifestation(
    current_user: CurrentUser,
    State(state): State<AppState>,
    ApiPath(manifestation_id): ApiPath<Uuid>,
    ApiJson(payload): ApiJson<VersionPayload>,
) -> Result<impl IntoResponse, AppError> {
    editing::reject_manifestation(&current_user, &state, manifestation_id, payload, None).await?;
    Ok(StatusCode::OK)
}

/// `POST /api/v1/manifestations/{id}/metadata/revert` — restore a prior
/// version as canonical, or clear the field entirely.
///
/// # Errors
/// - [`AppError::Forbidden`] when the caller is a child account.
/// - [`AppError::NotFound`] when the manifestation or version is missing
///   or RLS-hidden.
/// - [`AppError::Validation`] when clearing a non-clearable field (e.g.
///   `title`) or the stored value fails parsing.
/// - [`AppError::Internal`] on database errors.
#[utoipa::path(
    post,
    path = "/api/v1/manifestations/{id}/metadata/revert",
    summary = "Revert a metadata field",
    description = "Restores a prior metadata version as canonical for a field, or clears the field entirely when no version is given. Available to adult accounts only. Returns 422 if the field cannot be cleared or the stored value fails parsing.",
    tag = "metadata",
    security(("session_cookie" = ["write"]), ("device_token_bearer" = ["write"]), ("opds_basic" = ["write"])),
    params(("id" = Uuid, Path, max_length = 45, description = "Manifestation id")),
    request_body = RevertPayload,
    responses(
        (status = 409, description = "A requested metadata field is locked on this edition; locked_fields lists the refused keys and no changes are applied", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 200, description = "Field reverted to the given version, or cleared when version_id is null"),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 403, description = "Caller is a child account", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 404, description = "Manifestation or version missing, or RLS-hidden", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 422, description = "Field cannot be cleared or stored value fails parsing", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
    )
)]
async fn revert_manifestation(
    current_user: CurrentUser,
    State(state): State<AppState>,
    ApiPath(manifestation_id): ApiPath<Uuid>,
    ApiJson(payload): ApiJson<RevertPayload>,
) -> Result<impl IntoResponse, AppError> {
    editing::revert_manifestation(&current_user, &state, manifestation_id, payload, None).await?;
    Ok(StatusCode::OK)
}

/// `POST /api/v1/manifestations/{id}/metadata/lock` — lock a field
/// against manual canonical and automated enrichment writes.
///
/// # Errors
/// - [`AppError::Forbidden`] when the caller is a child account.
/// - [`AppError::NotFound`] when the manifestation is missing or hidden by
///   RLS for the current user.
/// - [`AppError::Validation`] when `entity_type` is not `work` /
///   `manifestation`.
/// - [`AppError::Internal`] on database errors.
#[utoipa::path(
    post,
    path = "/api/v1/manifestations/{id}/metadata/lock",
    summary = "Lock a metadata field",
    description = "Locks an edition-specific metadata field against manual canonical and automated enrichment writes; locking an already-locked field is a no-op. Available to adult accounts only.",
    tag = "metadata",
    security(("session_cookie" = ["write"]), ("device_token_bearer" = ["write"]), ("opds_basic" = ["write"])),
    params(("id" = Uuid, Path, max_length = 45, description = "Manifestation id")),
    request_body = LockPayload,
    responses(
        (status = 201, description = "Lock recorded (idempotent)"),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 403, description = "Caller is a child account", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 404, description = "Manifestation missing or RLS-hidden (existence-not-leaked)", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 422, description = "Unknown entity_type", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
    )
)]
async fn lock_field(
    current_user: CurrentUser,
    State(state): State<AppState>,
    ApiPath(manifestation_id): ApiPath<Uuid>,
    ApiJson(payload): ApiJson<LockPayload>,
) -> Result<impl IntoResponse, AppError> {
    editing::lock_field(&current_user, &state, manifestation_id, payload, None).await?;
    Ok(StatusCode::CREATED)
}

/// `POST /api/v1/manifestations/{id}/metadata/unlock` — remove a field
/// lock.
///
/// # Errors
/// - [`AppError::Forbidden`] when the caller is a child account.
/// - [`AppError::NotFound`] when the manifestation is missing or hidden by
///   RLS for the current user, or when no matching lock exists.
/// - [`AppError::Validation`] when `entity_type` is not `work` /
///   `manifestation`.
/// - [`AppError::Internal`] on database errors.
#[utoipa::path(
    post,
    path = "/api/v1/manifestations/{id}/metadata/unlock",
    summary = "Unlock a metadata field",
    description = "Removes a lock on a metadata field, allowing manual canonical and automated enrichment writes to resume. Available to adult accounts only.",
    tag = "metadata",
    security(("session_cookie" = ["write"]), ("device_token_bearer" = ["write"]), ("opds_basic" = ["write"])),
    params(("id" = Uuid, Path, max_length = 45, description = "Manifestation id")),
    request_body = LockPayload,
    responses(
        (status = 200, description = "Lock removed"),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 403, description = "Caller is a child account", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 404, description = "Manifestation missing or RLS-hidden, or no matching lock", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 422, description = "Unknown entity_type", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
    )
)]
async fn unlock_field(
    current_user: CurrentUser,
    State(state): State<AppState>,
    ApiPath(manifestation_id): ApiPath<Uuid>,
    ApiJson(payload): ApiJson<LockPayload>,
) -> Result<impl IntoResponse, AppError> {
    editing::unlock_field(&current_user, &state, manifestation_id, payload, None).await?;
    Ok(StatusCode::OK)
}

/// The editable metadata span for one manifestation.
///
/// This is the matched-pair read for [`update_book_metadata`]: same URI and
/// id key, so a read-modify-write flow targets one resource.
///
/// # Errors
/// - [`AppError::Forbidden`] when the caller is a child account.
/// - [`AppError::NotFound`] when the manifestation is missing or hidden by
///   RLS for the current user (existence-not-leaked).
/// - [`AppError::Internal`] on database errors.
#[utoipa::path(
    get,
    path = "/api/v1/books/{id}/metadata",
    summary = "Get a book's editable metadata",
    description = "Returns the editable metadata span for one manifestation, matching what the corresponding PATCH accepts. Available to adult accounts only.",
    tag = "metadata",
    security(("session_cookie" = ["read"]), ("device_token_bearer" = ["read"]), ("opds_basic" = ["read"])),
    params(("id" = Uuid, Path, max_length = 45, description = "Manifestation id")),
    responses(
        (status = 200, description = "The editable metadata span, matching what PATCH accepts", body = BookMetadata,
         headers(("ETag" = String, description = "Strong entity-tag covering work and edition identities and their metadata revisions. Echo as If-Match on PATCH /api/v1/books/{id}/metadata"))),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 403, description = "Caller is a child account", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 404, description = "Manifestation missing or RLS-hidden (existence-not-leaked)", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
    )
)]
async fn get_book_metadata(
    current_user: CurrentUser,
    State(state): State<AppState>,
    ApiPath(manifestation_id): ApiPath<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    let snapshot = editing::get_book_metadata(&current_user, &state, manifestation_id).await?;
    let mut headers = HeaderMap::new();
    headers.insert(ETAG, snapshot.revision.etag()?);
    Ok((headers, axum::Json(snapshot.values)))
}

/// `PATCH /api/v1/books/{id}/metadata` — manual operator edit.
///
/// Bare RFC 7396 JSON Merge Patch body (no envelope): absent keys are
/// unchanged, `null` clears (except `title`). Accepts both
/// `application/json` and `application/merge-patch+json`: axum's `Json`
/// extractor already treats any `+json`-suffixed content type as JSON.
///
/// For each touched field the handler inserts a `metadata_versions` row
/// with `source = 'manual'` and then either promotes that row to canonical
/// via [`apply_version`] (when a value is supplied) or clears the
/// canonical column via [`clear_field`] (when the value is `null`). The
/// pending AI/OPF drafts on the same field stay `status = 'pending'`, so the
/// operator can revert to one of them later. The response carries the
/// complete canonical values, their matching validator and independent exact field operations.
///
/// # Errors
/// - [`AppError::Validation`] when the body has no populated fields,
///   when an ISBN/date value fails parsing, when a vocabulary list
///   (`genres`/`moods`/`tags`) contains an empty, duplicate, or
///   over-length term, or when the operator tries to clear `title`
///   (canonical title is `NOT NULL` on `works`). Reached only once
///   `If-Match` has matched, so a stale tag yields 412 rather than a
///   body error.
/// - [`AppError::MalformedHeader`] when `If-Match` violates the RFC 9110
///   §8.8.3 entity-tag grammar, or carries a form this API refuses by
///   policy: the `*` wildcard, an entity-tag list, a weak validator, or
///   more than one instance of the field.
/// - [`AppError::NotFound`] when the manifestation is missing or hidden
///   by RLS for the current user (existence-not-leaked).
/// - [`AppError::Forbidden`] when the caller is a child account.
/// - [`AppError::IfMatchRequired`] (428) when `If-Match` is absent.
/// - [`AppError::IfMatchMismatch`] (412) when `If-Match` does not match the
///   manifestation's current metadata `ETag`; the response carries the
///   current `ETag` so the caller can resync in one round trip.
/// - [`AppError::Internal`] on database errors.
#[utoipa::path(
    patch,
    path = "/api/v1/books/{id}/metadata",
    summary = "Update a book's metadata",
    description = "Applies a manual operator edit to a manifestation's metadata; each touched field is recorded as a new metadata version. Requires an `If-Match` header; fails with 428 when it is absent, 412 when it does not match, and 400 when it is malformed or refused by policy. Available to adult accounts only.",
    tag = "metadata",
    security(("session_cookie" = ["write"]), ("device_token_bearer" = ["write"]), ("opds_basic" = ["write"])),
    params(
        ("id" = Uuid, Path, max_length = 45, description = "Manifestation id"),
        ("If-Match" = String, Header, description = "Exactly one quoted strong entity-tag, as returned in a prior GET or PATCH response's ETag header. Opaque content follows the full RFC 9110 8.8.3 grammar, obs-text included. The * wildcard, entity-tag lists, weak tags, and repeated instances of this field are refused with 400. Required: absent means 428; unequal means 412")
    ),
    request_body(content = UpdateMetadataFields, description = "RFC 7396 JSON Merge Patch: absent fields are unchanged, `null` clears (except `title`)"),
    responses(
        (status = 409, description = "A requested metadata field is locked on this edition; locked_fields lists the refused keys and no changes are applied", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 200, description = "Manual edit committed with complete canonical values and matching ETag", body = UpdateMetadataResponse,
         headers(("ETag" = String, description = "Strong entity-tag covering work and edition identities and their metadata revisions after this write"))),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 403, description = "Caller is a child account", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 404, description = "Manifestation missing or RLS-hidden (existence-not-leaked)", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 412, description = "If-Match does not match the manifestation's current metadata ETag", body = crate::openapi::ProblemDetails, content_type = "application/problem+json",
         headers(("ETag" = String, description = "Current entity-tag, so the caller can resync without a follow-up GET"))),
        (status = 400, description = "If-Match is malformed, or carries a form this API refuses by policy: the * wildcard, an entity-tag list, a weak tag, or a repeated header instance, the request body is not valid JSON, or a path parameter is malformed", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 422, description = "No populated fields, ISBN/date parse failure, or attempt to clear title. Evaluated only after If-Match has matched, so a stale tag returns 412 instead", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 428, description = "If-Match header absent", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
    )
)]
async fn update_book_metadata(
    current_user: CurrentUser,
    State(state): State<AppState>,
    ApiPath(manifestation_id): ApiPath<Uuid>,
    headers_in: HeaderMap,
    ApiJson(req_fields): ApiJson<UpdateMetadataFields>,
) -> Result<Response, AppError> {
    current_user.require_scope(Scope::Write)?;
    current_user.require_not_child()?;
    let supplied = parse_if_match(&headers_in)?;
    match editing::update_book_metadata(
        &current_user,
        &state,
        manifestation_id,
        supplied,
        req_fields,
        &editing::SnapshotSelection::Complete,
    )
    .await?
    {
        Conditional::Mismatch(tag) => Ok(if_match_mismatch(&tag)),
        Conditional::Matched(saved) => {
            let mut headers = HeaderMap::new();
            headers.insert(ETAG, saved.etag);
            headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
            Ok((
                StatusCode::OK,
                headers,
                axum::Json(UpdateMetadataResponse {
                    values: saved.snapshot.values,
                }),
            )
                .into_response())
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::error::problems;
    use crate::test_support;
    use axum::http::StatusCode;
    use uuid::Uuid;

    async fn metadata_identity(pool: &sqlx::PgPool, edition: Uuid) -> serde_json::Value {
        sqlx::query_scalar!(
            "SELECT jsonb_build_object(\
                 'work', w.metadata_revision, \
                 'manifestation', m.metadata_revision\
             ) AS \"identity!\" FROM manifestations m JOIN works w ON w.id = m.work_id WHERE m.id = $1",
            edition,
        )
        .fetch_one(pool)
        .await
        .unwrap()
    }

    fn assert_metadata_revisions_advanced(
        before: &serde_json::Value,
        after: &serde_json::Value,
        owners: &[&str],
    ) {
        for owner in owners {
            assert!(
                after[owner].as_i64().unwrap() > before[owner].as_i64().unwrap(),
                "{owner}: {before} -> {after}"
            );
        }
    }

    // Data loss boundary: canonical and referenced-attribution writers advance their owner's validator.
    #[sqlx::test(migrations = "./migrations")]
    async fn metadata_revisions_follow_representation_writers(pool: sqlx::PgPool) {
        let app = test_support::db::app_pool_for(&pool).await;
        let ing = test_support::db::ingestion_pool_for(&pool).await;
        let (user, basic) = test_support::db::create_admin_and_basic_auth(&app).await;
        let (work, edition) =
            test_support::db::insert_work_and_manifestation(&ing, "revisions").await;
        let sibling = insert_sibling_manifestation(&ing, work).await;
        let server = test_support::db::server_with_real_pools(&app, &ing);
        let url = format!("/api/v1/books/{edition}/metadata");
        let initial = server
            .get(&url)
            .add_header(AUTHORIZATION, basic.clone())
            .await;
        let before = metadata_identity(&pool, edition).await;
        assert_eq!(before["work"], 0);
        assert_eq!(before["manifestation"], 0);
        let sibling_initial = server
            .get(&format!("/api/v1/books/{sibling}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
            .await;
        assert_eq!(
            initial.json::<serde_json::Value>(),
            sibling_initial.json::<serde_json::Value>()
        );
        assert_ne!(
            initial.header(axum::http::header::ETAG),
            sibling_initial.header(axum::http::header::ETAG)
        );
        let (other_work, other_edition) =
            test_support::db::insert_work_and_manifestation(&ing, "other-owner").await;
        sqlx::query!(
            "UPDATE manifestations SET work_id = $2 WHERE id = $1",
            edition,
            other_work
        )
        .execute(&ing)
        .await
        .unwrap();
        let reparented = server
            .get(&url)
            .add_header(AUTHORIZATION, basic.clone())
            .await;
        assert_eq!(
            initial.json::<serde_json::Value>(),
            reparented.json::<serde_json::Value>()
        );
        assert_ne!(
            initial.header(axum::http::header::ETAG),
            reparented.header(axum::http::header::ETAG)
        );
        assert_metadata_revisions_advanced(
            &before,
            &metadata_identity(&pool, edition).await,
            &["manifestation"],
        );
        sqlx::query!(
            "UPDATE manifestations SET work_id = $2 WHERE id = $1",
            edition,
            work
        )
        .execute(&ing)
        .await
        .unwrap();
        let initial = server
            .get(&url)
            .add_header(AUTHORIZATION, basic.clone())
            .await;
        let before = metadata_identity(&pool, edition).await;
        let patch = server.patch(&url)
            .add_header(AUTHORIZATION, basic.clone())
            .add_header(axum::http::header::IF_MATCH, initial.header(axum::http::header::ETAG))
            .json(&serde_json::json!({
                "title": "Revision title", "subtitle": "Original subtitle", "description": "Description",
                "language": "en", "publisher": "Publisher", "pub_date": "2026-11",
                "isbn_10": "0306406152", "isbn_13": "9780306406157", "pages": 123,
                "content_rating": "everyone", "genres": ["Revision genre"], "moods": ["Revision mood"],
                "tags": ["Revision tag"],
                "contributors": {"author": ["Revision author"], "editor": ["Revision editor"], "translator": ["Revision translator"]},
                "work_identifiers": {"openlibrary": "OL123W"},
                "manifestation_identifiers": {"asin": "B004GXAX8C"}
            })).await;
        assert_eq!(patch.status_code(), StatusCode::OK, "{}", patch.text());
        let confirmed = server
            .get(&url)
            .add_header(AUTHORIZATION, basic.clone())
            .await;
        assert_eq!(
            patch.header(axum::http::header::ETAG),
            confirmed.header(axum::http::header::ETAG)
        );
        assert_eq!(
            patch.json::<serde_json::Value>()["values"],
            confirmed.json::<serde_json::Value>()
        );

        let after = metadata_identity(&pool, edition).await;
        assert_metadata_revisions_advanced(&before, &after, &["work", "manifestation"]);

        sqlx::query!(
            "UPDATE works SET subtitle = 'Intervening subtitle' WHERE id = $1",
            work
        )
        .execute(&ing)
        .await
        .unwrap();
        sqlx::query!(
            "UPDATE works SET subtitle = 'Original subtitle' WHERE id = $1",
            work
        )
        .execute(&ing)
        .await
        .unwrap();
        let restored = server
            .get(&url)
            .add_header(AUTHORIZATION, basic.clone())
            .await;
        assert_eq!(
            restored.json::<serde_json::Value>(),
            confirmed.json::<serde_json::Value>()
        );
        assert_ne!(
            restored.header(axum::http::header::ETAG),
            confirmed.header(axum::http::header::ETAG)
        );
        assert_metadata_revisions_advanced(
            &after,
            &metadata_identity(&pool, edition).await,
            &["work"],
        );

        let version =
            insert_version(&ing, edition, "title", serde_json::json!("Revision title")).await;
        let before = metadata_identity(&pool, edition).await;
        sqlx::query!(
            "UPDATE works SET title_version_id = $2 WHERE id = $1",
            work,
            version
        )
        .execute(&ing)
        .await
        .unwrap();
        sqlx::query!(
            "UPDATE manifestations SET publisher_version_id = $2 WHERE id = $1",
            edition,
            version
        )
        .execute(&ing)
        .await
        .unwrap();
        sqlx::query!(
            "UPDATE work_authors SET source_version_id = $2 WHERE work_id = $1",
            work,
            version
        )
        .execute(&ing)
        .await
        .unwrap();
        sqlx::query!(
            "UPDATE work_external_identifiers SET source_version_id = $2 WHERE work_id = $1",
            work,
            version
        )
        .execute(&ing)
        .await
        .unwrap();
        sqlx::query!("UPDATE manifestation_external_identifiers SET source_version_id = $2 WHERE manifestation_id = $1", edition, version).execute(&ing).await.unwrap();
        sqlx::query!(
            "UPDATE manifestation_genres SET source_version_id = $2 WHERE manifestation_id = $1",
            edition,
            version
        )
        .execute(&ing)
        .await
        .unwrap();
        sqlx::query!(
            "UPDATE manifestation_moods SET source_version_id = $2 WHERE manifestation_id = $1",
            edition,
            version
        )
        .execute(&ing)
        .await
        .unwrap();
        sqlx::query!(
            "UPDATE manifestation_tags SET source_version_id = $2 WHERE manifestation_id = $1",
            edition,
            version
        )
        .execute(&ing)
        .await
        .unwrap();

        let after = metadata_identity(&pool, edition).await;
        assert_metadata_revisions_advanced(&before, &after, &["work", "manifestation"]);
        let before = after;
        let before_values = server
            .get(&url)
            .add_header(AUTHORIZATION, basic.clone())
            .await;
        assert_eq!(
            before_values.json::<serde_json::Value>(),
            restored.json::<serde_json::Value>()
        );
        assert_ne!(
            before_values.header(axum::http::header::ETAG),
            restored.header(axum::http::header::ETAG)
        );
        sqlx::query!(
            "UPDATE metadata_versions SET confidence_score = 0.8 WHERE id = $1",
            version
        )
        .execute(&ing)
        .await
        .unwrap();
        let after = metadata_identity(&pool, edition).await;
        assert_metadata_revisions_advanced(&before, &after, &["work", "manifestation"]);
        let after_values = server
            .get(&url)
            .add_header(AUTHORIZATION, basic.clone())
            .await;
        assert_eq!(
            before_values.json::<serde_json::Value>(),
            after_values.json::<serde_json::Value>()
        );
        assert_ne!(
            before_values.header(axum::http::header::ETAG),
            after_values.header(axum::http::header::ETAG)
        );

        let before = metadata_identity(&pool, edition).await;
        sqlx::query!("DELETE FROM metadata_versions WHERE id = $1", version)
            .execute(&ing)
            .await
            .unwrap();
        assert_metadata_revisions_advanced(
            &before,
            &metadata_identity(&pool, edition).await,
            &["work", "manifestation"],
        );
        let cleared_attribution = server
            .get(&url)
            .add_header(AUTHORIZATION, basic.clone())
            .await;
        assert_eq!(
            cleared_attribution.json::<serde_json::Value>(),
            before_values.json::<serde_json::Value>()
        );

        let other_url = format!("/api/v1/books/{other_edition}/metadata");
        let other = server
            .get(&other_url)
            .add_header(AUTHORIZATION, basic.clone())
            .await;
        let other_patch = server.patch(&other_url)
            .add_header(AUTHORIZATION, basic.clone())
            .add_header(axum::http::header::IF_MATCH, other.header(axum::http::header::ETAG))
            .json(&serde_json::json!({
                "contributors": {"author": ["Revision author"], "editor": ["Revision editor"], "translator": ["Revision translator"]},
                "genres": ["Revision genre"], "moods": ["Revision mood"], "tags": ["Revision tag"],
                "work_identifiers": {"openlibrary": "OL456W"},
                "manifestation_identifiers": {"asin": "B004GXAX8D"}
            })).await;
        assert_eq!(
            other_patch.status_code(),
            StatusCode::OK,
            "{}",
            other_patch.text()
        );
        let other_before = metadata_identity(&pool, other_edition).await;
        let before = metadata_identity(&pool, edition).await;
        sqlx::query!("UPDATE authors SET name = name || ' renamed' WHERE id IN (SELECT author_id FROM work_authors WHERE work_id = $1)", work).execute(&ing).await.unwrap();
        sqlx::query!("UPDATE genres SET name = name || ' renamed' WHERE id IN (SELECT genre_id FROM manifestation_genres WHERE manifestation_id = $1)", edition).execute(&ing).await.unwrap();
        sqlx::query!("UPDATE moods SET name = name || ' renamed' WHERE id IN (SELECT mood_id FROM manifestation_moods WHERE manifestation_id = $1)", edition).execute(&ing).await.unwrap();
        sqlx::query!("UPDATE tags SET name = name || ' renamed' WHERE id IN (SELECT tag_id FROM manifestation_tags WHERE manifestation_id = $1)", edition).execute(&ing).await.unwrap();
        sqlx::query!("UPDATE identifier_schemes SET display_name = display_name || ' renamed' WHERE id IN ('openlibrary', 'asin')").execute(&pool).await.unwrap();
        assert_metadata_revisions_advanced(
            &before,
            &metadata_identity(&pool, edition).await,
            &["work", "manifestation"],
        );

        assert_metadata_revisions_advanced(
            &other_before,
            &metadata_identity(&pool, other_edition).await,
            &["work", "manifestation"],
        );

        let before = metadata_identity(&pool, edition).await;
        sqlx::query!("UPDATE genres SET name = name WHERE id IN (SELECT genre_id FROM manifestation_genres WHERE manifestation_id = $1)", edition).execute(&ing).await.unwrap();
        sqlx::query!("UPDATE moods SET name = name WHERE id IN (SELECT mood_id FROM manifestation_moods WHERE manifestation_id = $1)", edition).execute(&ing).await.unwrap();
        sqlx::query!("UPDATE tags SET name = name WHERE id IN (SELECT tag_id FROM manifestation_tags WHERE manifestation_id = $1)", edition).execute(&ing).await.unwrap();
        sqlx::query!("UPDATE manifestations SET current_file_hash = 'writeback-only', enrichment_claim_seq = enrichment_claim_seq + 1, has_embedded_cover = TRUE WHERE id = $1", edition).execute(&ing).await.unwrap();
        sqlx::query!("UPDATE metadata_versions SET observation_count = observation_count + 1, last_seen_at = now(), status = 'rejected' WHERE manifestation_id = $1", edition).execute(&ing).await.unwrap();
        assert_eq!(before, metadata_identity(&pool, edition).await);

        let before = metadata_identity(&pool, edition).await;
        sqlx::query!("DELETE FROM authors WHERE id IN (SELECT author_id FROM work_authors WHERE work_id = $1 AND role = 'editor')", work).execute(&ing).await.unwrap();
        sqlx::query!("DELETE FROM genres WHERE id IN (SELECT genre_id FROM manifestation_genres WHERE manifestation_id = $1)", edition).execute(&ing).await.unwrap();
        sqlx::query!("DELETE FROM moods WHERE id IN (SELECT mood_id FROM manifestation_moods WHERE manifestation_id = $1)", edition).execute(&ing).await.unwrap();
        sqlx::query!("DELETE FROM tags WHERE id IN (SELECT tag_id FROM manifestation_tags WHERE manifestation_id = $1)", edition).execute(&ing).await.unwrap();
        sqlx::query!(
            "DELETE FROM work_external_identifiers WHERE work_id = $1",
            work
        )
        .execute(&ing)
        .await
        .unwrap();
        sqlx::query!(
            "DELETE FROM manifestation_external_identifiers WHERE manifestation_id = $1",
            edition
        )
        .execute(&ing)
        .await
        .unwrap();
        assert_metadata_revisions_advanced(
            &before,
            &metadata_identity(&pool, edition).await,
            &["work", "manifestation"],
        );
        let sibling_read = server
            .get(&format!("/api/v1/books/{sibling}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
            .await;
        assert_eq!(
            sibling_read.json::<serde_json::Value>()["title"],
            "Revision title"
        );

        let mut tx = crate::db::acquire_with_rls(&app, user).await.unwrap();
        let forgery = sqlx::query!(
            "UPDATE manifestations SET metadata_revision = 0 WHERE id = $1",
            edition
        )
        .execute(&mut *tx)
        .await;
        assert!(forgery.is_err());
        tx.rollback().await.unwrap();
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn manual_metadata_locks_are_atomic_and_edition_scoped(pool: sqlx::PgPool) {
        let app = test_support::db::app_pool_for(&pool).await;
        let ing = test_support::db::ingestion_pool_for(&pool).await;
        let (_, basic) = test_support::db::create_admin_and_basic_auth(&app).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (work, edition) = test_support::db::insert_work_and_manifestation(&ing, &marker).await;
        let sibling = insert_sibling_manifestation(&ing, work).await;
        let version =
            insert_version(&ing, edition, "subtitle", serde_json::json!("Proposal")).await;
        let server = test_support::db::server_with_real_pools(&app, &ing);
        let locked = [
            ("work", "subtitle"),
            ("manifestation", "publisher"),
            ("manifestation", "genres"),
            ("manifestation", "moods"),
            ("manifestation", "tags"),
            ("work", "contributors.editor"),
            ("work", "identifiers.work.openlibrary"),
            ("manifestation", "identifiers.manifestation.asin"),
        ];
        for (entity, field) in locked {
            let response = server
                .post(&format!("/api/v1/manifestations/{edition}/metadata/lock"))
                .add_header(AUTHORIZATION, basic.clone())
                .json(&serde_json::json!({"entity_type": entity, "field_name": field}))
                .await;
            assert_eq!(response.status_code(), StatusCode::CREATED);
        }
        let before = review_mutation_state(&pool, edition).await;
        let initial = server
            .get(&format!("/api/v1/books/{edition}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
            .await;
        let etag = etag_value(initial.headers());
        let response = server
            .patch(&format!("/api/v1/books/{edition}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
            .add_header(
                axum::http::header::IF_MATCH,
                axum::http::HeaderValue::from_str(&etag).unwrap(),
            )
            .json(&serde_json::json!({
                "title": "Must not change",
                "subtitle": "Blocked",
                "publisher": "Blocked",
                "genres": ["Blocked"],
                "moods": ["Blocked"],
                "tags": ["Blocked"],
                "contributors": {"editor": ["Blocked"]},
                "work_identifiers": {"openlibrary": "OL123W"},
                "manifestation_identifiers": {"asin": "B004GXAX8C"},
            }))
            .await;
        assert_eq!(
            response.status_code(),
            StatusCode::CONFLICT,
            "{}",
            response.text()
        );
        test_support::assert_problem(&response, "field-locked", StatusCode::CONFLICT);
        let body: serde_json::Value = response.json();
        let mut expected: Vec<_> = locked.into_iter().map(|(_, field)| field).collect();
        expected.sort_unstable();
        assert_eq!(body["locked_fields"], serde_json::json!(expected));
        assert_eq!(review_mutation_state(&pool, edition).await, before);

        let response = server
            .patch(&format!("/api/v1/books/{edition}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
            .add_header(
                axum::http::header::IF_MATCH,
                axum::http::HeaderValue::from_str(&etag).unwrap(),
            )
            .json(&serde_json::json!({"contributors": null}))
            .await;
        test_support::assert_problem(&response, "field-locked", StatusCode::CONFLICT);
        assert_eq!(
            response.json::<serde_json::Value>()["locked_fields"],
            serde_json::json!(["contributors.editor"])
        );
        assert_eq!(review_mutation_state(&pool, edition).await, before);

        for (action, payload) in [
            ("accept", serde_json::json!({"version_id": version})),
            (
                "revert",
                serde_json::json!({"field_name": "subtitle", "version_id": version}),
            ),
            (
                "revert",
                serde_json::json!({"field_name": "subtitle", "version_id": null}),
            ),
        ] {
            let response = server
                .post(&format!(
                    "/api/v1/manifestations/{edition}/metadata/{action}"
                ))
                .add_header(AUTHORIZATION, basic.clone())
                .json(&payload)
                .await;
            test_support::assert_problem(&response, "field-locked", StatusCode::CONFLICT);
            assert_eq!(
                response.json::<serde_json::Value>()["locked_fields"],
                serde_json::json!(["subtitle"])
            );
            assert_eq!(review_mutation_state(&pool, edition).await, before);
        }
        let response = server
            .post(&format!("/api/v1/manifestations/{edition}/metadata/reject"))
            .add_header(AUTHORIZATION, basic.clone())
            .json(&serde_json::json!({"version_id": version}))
            .await;
        assert_eq!(response.status_code(), StatusCode::OK);
        let after_reject = review_mutation_state(&pool, edition).await;
        assert_eq!(after_reject["work"], before["work"]);
        assert_eq!(after_reject["jobs"], before["jobs"]);

        let initial = server
            .get(&format!("/api/v1/books/{sibling}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
            .await;
        let response = server
            .patch(&format!("/api/v1/books/{sibling}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
            .add_header(
                axum::http::header::IF_MATCH,
                axum::http::HeaderValue::from_str(&etag_value(initial.headers())).unwrap(),
            )
            .json(&serde_json::json!({"subtitle": "Sibling edit"}))
            .await;
        assert_eq!(response.status_code(), StatusCode::OK);
        assert_eq!(
            review_mutation_state(&pool, sibling).await["work"]["subtitle"],
            "Sibling edit"
        );

        let initial = server
            .get(&format!("/api/v1/books/{sibling}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
            .await;
        let etag = etag_value(initial.headers());
        let before = review_mutation_state(&pool, sibling).await;
        let mut barrier = pool.begin().await.unwrap();
        super::lock_visible_manifestation(&mut barrier, sibling)
            .await
            .unwrap();
        let lock_server = test_support::db::server_with_real_pools(&app, &ing);
        let lock_basic = basic.clone();
        let mut locking = tokio::spawn(async move {
            lock_server
                .post(&format!("/api/v1/manifestations/{sibling}/metadata/lock"))
                .add_header(AUTHORIZATION, lock_basic)
                .json(&serde_json::json!({"entity_type": "work", "field_name": "subtitle"}))
                .await
        });
        tokio::select! {
            response = &mut locking => panic!("lock bypassed barrier: {:?}", response.unwrap().status_code()),
            () = test_support::wait_for_blocked_query(&pool, "SELECT id FROM manifestations WHERE id = $1 FOR UPDATE", 1) => {}
        }
        let mut saving = tokio::spawn(async move {
            server
                .patch(&format!("/api/v1/books/{sibling}/metadata"))
                .add_header(AUTHORIZATION, basic)
                .add_header(
                    axum::http::header::IF_MATCH,
                    axum::http::HeaderValue::from_str(&etag).unwrap(),
                )
                .json(&serde_json::json!({"subtitle": "Racing edit"}))
                .await
        });
        tokio::select! {
            response = &mut saving => panic!("save bypassed barrier: {:?}", response.unwrap().status_code()),
            () = test_support::wait_for_blocked_query(&pool, "SELECT work_id FROM manifestations WHERE id = $1 FOR UPDATE", 1) => {}
        }
        barrier.commit().await.unwrap();
        assert_eq!(locking.await.unwrap().status_code(), StatusCode::CREATED);
        let response = saving.await.unwrap();
        test_support::assert_problem(&response, "field-locked", StatusCode::CONFLICT);
        let after = review_mutation_state(&pool, sibling).await;
        for key in ["work", "manifestation", "versions", "jobs"] {
            assert_eq!(after[key], before[key]);
        }
        assert_eq!(after["locks"].as_array().unwrap().len(), 1);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn shared_vocabulary_edit_preserves_other_books(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let server = test_support::db::server_with_real_pools(&app_pool, &pool);
        let marker = Uuid::new_v4().simple().to_string();
        let (_, original) = test_support::db::insert_work_and_manifestation(&pool, &marker).await;
        let (_, edited) =
            test_support::db::insert_work_and_manifestation(&pool, &format!("{marker}-edited"))
                .await;

        for field in [
            super::VocabularyField::Genres,
            super::VocabularyField::Moods,
            super::VocabularyField::Tags,
        ] {
            let stored = format!("Original-{marker}");
            let added = format!("New-{marker}");
            let original_version = insert_version(
                &pool,
                original,
                field.field_name(),
                serde_json::json!([stored]),
            )
            .await;
            let edited_version = insert_version(
                &pool,
                edited,
                field.field_name(),
                serde_json::json!([stored.to_uppercase(), added]),
            )
            .await;
            let mut tx = pool.begin().await.unwrap();
            super::insert_vocabulary_rows(
                &mut tx,
                original,
                field,
                std::slice::from_ref(&stored),
                original_version,
            )
            .await
            .unwrap();
            super::insert_vocabulary_rows(
                &mut tx,
                edited,
                field,
                &[stored.to_uppercase(), added.clone(), stored.to_lowercase()],
                edited_version,
            )
            .await
            .unwrap();

            let rows = match field {
                super::VocabularyField::Genres => sqlx::query!(
                    "SELECT j.manifestation_id, j.genre_id AS term_id, t.name, j.source_version_id \
                     FROM manifestation_genres j JOIN genres t ON t.id = j.genre_id \
                     WHERE j.manifestation_id = ANY($1)",
                    &[original, edited],
                )
                .fetch_all(&mut *tx)
                .await
                .unwrap()
                .into_iter()
                .map(|r| (r.manifestation_id, r.term_id, r.name, r.source_version_id))
                .collect::<Vec<_>>(),
                super::VocabularyField::Moods => sqlx::query!(
                    "SELECT j.manifestation_id, j.mood_id AS term_id, t.name, j.source_version_id \
                     FROM manifestation_moods j JOIN moods t ON t.id = j.mood_id \
                     WHERE j.manifestation_id = ANY($1)",
                    &[original, edited],
                )
                .fetch_all(&mut *tx)
                .await
                .unwrap()
                .into_iter()
                .map(|r| (r.manifestation_id, r.term_id, r.name, r.source_version_id))
                .collect::<Vec<_>>(),
                super::VocabularyField::Tags => sqlx::query!(
                    "SELECT j.manifestation_id, j.tag_id AS term_id, t.name, j.source_version_id \
                     FROM manifestation_tags j JOIN tags t ON t.id = j.tag_id \
                     WHERE j.manifestation_id = ANY($1)",
                    &[original, edited],
                )
                .fetch_all(&mut *tx)
                .await
                .unwrap()
                .into_iter()
                .map(|r| (r.manifestation_id, r.term_id, r.name, r.source_version_id))
                .collect::<Vec<_>>(),
            };
            let original_rows: Vec<_> = rows.iter().filter(|r| r.0 == original).collect();
            assert_eq!(original_rows.len(), 1);
            let original_row = original_rows[0];
            assert_eq!(original_row.2, stored);
            assert_eq!(original_row.3, Some(original_version));
            let edited_rows: Vec<_> = rows.iter().filter(|r| r.0 == edited).collect();
            assert_eq!(edited_rows.len(), 2);
            assert!(
                edited_rows
                    .iter()
                    .any(|r| r.1 == original_row.1 && r.2 == stored)
            );
            assert!(edited_rows.iter().any(|r| r.2 == added));
            assert!(edited_rows.iter().all(|r| r.3 == Some(edited_version)));
            tx.commit().await.unwrap();

            let initial = server
                .get(&format!("/api/v1/books/{edited}/metadata"))
                .add_header(AUTHORIZATION, basic.clone())
                .await;
            assert_eq!(initial.status_code(), StatusCode::OK);
            let response = server
                .patch(&format!("/api/v1/books/{edited}/metadata"))
                .add_header(AUTHORIZATION, basic.clone())
                .add_header(
                    axum::http::header::IF_MATCH,
                    axum::http::HeaderValue::from_str(&etag_value(initial.headers())).unwrap(),
                )
                .json(&serde_json::json!({
                    (field.field_name()): [stored.to_uppercase(), added],
                }))
                .await;
            assert_eq!(response.status_code(), StatusCode::OK);
            let body: serde_json::Value = response.json();
            assert_eq!(
                body["values"][field.field_name()],
                serde_json::json!([added, stored]),
            );
        }
    }

    // REV-REQ-0072 AC1-4; REV-REQ-0073 AC2-4.
    #[sqlx::test(migrations = "./migrations")]
    async fn isbn_contract_invalid_review(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let server = test_support::db::server_with_real_pools(&app_pool, &pool);
        let marker = Uuid::new_v4().simple().to_string();
        let (work_id, m_id) = test_support::db::insert_work_and_manifestation(&pool, &marker).await;
        for (field, raw) in [
            ("isbn_13", "4006381333931"),
            ("isbn_10", "0306406153"),
            ("isbn_13", "9780306406150"),
        ] {
            let raw = serde_json::json!(raw);
            let version = insert_version(&pool, m_id, field, raw.clone()).await;
            let response = server
                .post(&format!("/api/v1/manifestations/{m_id}/metadata/accept"))
                .add_header(AUTHORIZATION, basic.clone())
                .json(&serde_json::json!({"version_id": version}))
                .await;
            assert_eq!(response.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
            let problem: serde_json::Value = response.json();
            assert!(problem["errors"][field].is_string());
            let journal = sqlx::query_scalar!(
                "SELECT new_value FROM metadata_versions WHERE id = $1",
                version
            )
            .fetch_one(&pool)
            .await
            .unwrap();
            assert_eq!(journal, raw);
            let row = sqlx::query!(
                "SELECT isbn_10, isbn_10_version_id, isbn_13, isbn_13_version_id FROM manifestations WHERE id = $1", m_id
            ).fetch_one(&pool).await.unwrap();
            assert!(row.isbn_10.is_none() && row.isbn_10_version_id.is_none());
            assert!(row.isbn_13.is_none() && row.isbn_13_version_id.is_none());
            let jobs = sqlx::query_scalar!(
                "SELECT COUNT(*) AS \"count!\" FROM writeback_jobs WHERE manifestation_id = $1",
                m_id
            )
            .fetch_one(&pool)
            .await
            .unwrap();
            assert_eq!(jobs, 0);
        }
        for (field, raw, ten, thirteen) in [
            ("isbn_10", Some("0 8044-2957-X"), Some("080442957X"), None),
            (
                "isbn_13",
                Some("979 10-90636-07-1"),
                Some("080442957X"),
                Some("9791090636071"),
            ),
            ("isbn_10", None, None, Some("9791090636071")),
            ("isbn_13", None, None, None),
        ] {
            let version = if let Some(raw) = raw {
                Some(insert_version(&pool, m_id, field, serde_json::json!(raw)).await)
            } else {
                None
            };
            let mut tx = pool.begin().await.unwrap();
            if let Some(version) = version {
                super::apply_version(
                    &mut tx,
                    field,
                    &serde_json::json!(raw.unwrap()),
                    version,
                    m_id,
                    work_id,
                )
                .await
                .unwrap();
            } else {
                super::clear_field(&mut tx, field, m_id, work_id)
                    .await
                    .unwrap();
            }
            tx.commit().await.unwrap();
            let row = sqlx::query!(
                "SELECT isbn_10, isbn_13 FROM manifestations WHERE id = $1",
                m_id
            )
            .fetch_one(&pool)
            .await
            .unwrap();
            assert_eq!(
                (row.isbn_10.as_deref(), row.isbn_13.as_deref()),
                (ten, thirteen)
            );
        }
    }

    // REV-REQ-0072 AC2-4.
    #[sqlx::test(migrations = "./migrations")]
    async fn patch_isbn13_bad_check_digit_returns_422(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ing_pool);
        let marker = Uuid::new_v4().simple().to_string();
        let (_work, m) = test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;

        let initial = server
            .get(&format!("/api/v1/books/{m}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
            .await;
        let etag = etag_value(initial.headers());

        for raw in ["9780306406150", "4006381333931"] {
            let response = server
                .patch(&format!("/api/v1/books/{m}/metadata"))
                .add_header(AUTHORIZATION, basic.clone())
                .add_header(
                    axum::http::header::IF_MATCH,
                    axum::http::HeaderValue::from_str(&etag).unwrap(),
                )
                .json(&serde_json::json!({"isbn_13": raw}))
                .await;
            assert_eq!(
                response.status_code(),
                StatusCode::UNPROCESSABLE_ENTITY,
                "body = {}",
                response.text()
            );
            let row = sqlx::query!(
                "SELECT isbn_10, isbn_13 FROM manifestations WHERE id = $1",
                m
            )
            .fetch_one(&ing_pool)
            .await
            .unwrap();
            assert!(row.isbn_10.is_none() && row.isbn_13.is_none());
        }
    }

    // REV-REQ-0072 AC2,4.
    #[sqlx::test(migrations = "./migrations")]
    async fn patch_isbn10_wrong_length_returns_422(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ing_pool);
        let marker = Uuid::new_v4().simple().to_string();
        let (_work, m) = test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;

        let initial = server
            .get(&format!("/api/v1/books/{m}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
            .await;
        let etag = etag_value(initial.headers());

        let response = server
            .patch(&format!("/api/v1/books/{m}/metadata"))
            .add_header(AUTHORIZATION, basic)
            .add_header(
                axum::http::header::IF_MATCH,
                axum::http::HeaderValue::from_str(&etag).unwrap(),
            )
            .json(&serde_json::json!({"isbn_10": "12345"}))
            .await;
        assert_eq!(
            response.status_code(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "body = {}",
            response.text()
        );
    }

    // REV-REQ-0072 AC2,4.
    #[sqlx::test(migrations = "./migrations")]
    async fn patch_isbn13_non_numeric_returns_422(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ing_pool);
        let marker = Uuid::new_v4().simple().to_string();
        let (_work, m) = test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;

        let initial = server
            .get(&format!("/api/v1/books/{m}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
            .await;
        let etag = etag_value(initial.headers());

        // 13 chars, but a letter where a digit must be.
        let response = server
            .patch(&format!("/api/v1/books/{m}/metadata"))
            .add_header(AUTHORIZATION, basic)
            .add_header(
                axum::http::header::IF_MATCH,
                axum::http::HeaderValue::from_str(&etag).unwrap(),
            )
            .json(&serde_json::json!({"isbn_13": "978030640615X"}))
            .await;
        assert_eq!(
            response.status_code(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "body = {}",
            response.text()
        );
    }

    // REV-REQ-0072 AC1,4; REV-REQ-0073 AC3-4.
    #[sqlx::test(migrations = "./migrations")]
    async fn patch_valid_isbn10_accepted(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ing_pool);
        let marker = Uuid::new_v4().simple().to_string();
        let (_work, m) = test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;

        let initial = server
            .get(&format!("/api/v1/books/{m}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
            .await;
        let etag = etag_value(initial.headers());
        let response = server
            .patch(&format!("/api/v1/books/{m}/metadata"))
            .add_header(AUTHORIZATION, basic)
            .add_header(
                axum::http::header::IF_MATCH,
                axum::http::HeaderValue::from_str(&etag).unwrap(),
            )
            .json(&serde_json::json!({"isbn_10": "0 8044-2957-X"}))
            .await;
        assert_eq!(
            response.status_code(),
            StatusCode::OK,
            "body = {}",
            response.text()
        );
        let row = sqlx::query!(
            "SELECT isbn_10, isbn_13 FROM manifestations WHERE id = $1",
            m
        )
        .fetch_one(&ing_pool)
        .await
        .unwrap();
        assert_eq!(row.isbn_10.as_deref(), Some("080442957X"));
        assert!(row.isbn_13.is_none());
    }

    // REV-REQ-0072 AC1,4; REV-REQ-0073 AC3-4.
    #[sqlx::test(migrations = "./migrations")]
    async fn patch_hyphenated_isbn13_stored_normalized(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ing_pool);
        let marker = Uuid::new_v4().simple().to_string();
        let (_work, m) = test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;

        let initial = server
            .get(&format!("/api/v1/books/{m}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
            .await;
        let etag = etag_value(initial.headers());
        let response = server
            .patch(&format!("/api/v1/books/{m}/metadata"))
            .add_header(AUTHORIZATION, basic)
            .add_header(
                axum::http::header::IF_MATCH,
                axum::http::HeaderValue::from_str(&etag).unwrap(),
            )
            .json(&serde_json::json!({"isbn_13": "978-0-306-40615-7"}))
            .await;
        assert_eq!(
            response.status_code(),
            StatusCode::OK,
            "body = {}",
            response.text()
        );
        let body: serde_json::Value = response.json();
        assert_eq!(
            body["values"]["isbn_13"],
            serde_json::json!("9780306406157"),
            "response must echo the normalized value, not the raw hyphenated input"
        );

        // Stored canonical value is digits-only, matching the ingestion
        // surface so rematch exact-equality can find ingested twins.
        let stored = sqlx::query!(
            "SELECT isbn_10, isbn_13 FROM manifestations WHERE id = $1",
            m
        )
        .fetch_one(&ing_pool)
        .await
        .expect("fetch isbn_13");
        assert!(stored.isbn_10.is_none());
        assert_eq!(stored.isbn_13.as_deref(), Some("9780306406157"));
    }

    // ── Admin-authenticated success tests (X3) ────────────────────────────
    //
    // These exercise the C2 fix: route handlers now open their tx via
    // `acquire_with_rls`, so the manifestations RLS policies see a real
    // `app.current_user_id` and an admin user satisfies the
    // `role IN ('admin','adult')` clause.  Without C2 these tests would
    // 404 on the initial SELECT.

    use axum::http::header::AUTHORIZATION;

    async fn insert_sibling_manifestation(ingestion_pool: &sqlx::PgPool, work_id: Uuid) -> Uuid {
        let marker = Uuid::new_v4().simple().to_string();
        let file_path = format!("fixtures/admin-test-sibling-{marker}.epub");
        let file_hash = format!("admin-test-sibling-hash-{marker}");
        sqlx::query_scalar!(
            "WITH inserted AS (INSERT INTO manifestations \
                (library_id, work_id, format, file_path, ingestion_file_hash, current_file_hash, \
                 file_size_bytes, ingestion_status, validation_status) \
             VALUES ((SELECT id FROM libraries WHERE configuration_key = 'default'), $1, 'epub'::manifestation_format, $2, $3, $3, 1000, \
                     'complete'::ingestion_status, 'clean'::validation_status) \
             RETURNING *), claimed AS (INSERT INTO library_path_claims (library_id, path, manifestation_id) SELECT library_id, file_path, id FROM inserted) SELECT id AS \"id!\" FROM inserted",
            work_id,
            file_path,
            file_hash,
        )
        .fetch_one(ingestion_pool)
        .await
        .expect("insert sibling manifestation")
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn revert_pages_rejects_version_journaled_under_sibling_manifestation(
        pool: sqlx::PgPool,
    ) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (work_id, m_a) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;
        let m_b = insert_sibling_manifestation(&ing_pool, work_id).await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ing_pool);

        // Patch pages (manifestation-scoped) via A.
        let initial_a = server
            .get(&format!("/api/v1/books/{m_a}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
            .await;
        let etag_a = etag_value(initial_a.headers());
        let patch_a = server
            .patch(&format!("/api/v1/books/{m_a}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
            .add_header(
                axum::http::header::IF_MATCH,
                axum::http::HeaderValue::from_str(&etag_a).unwrap(),
            )
            .json(&serde_json::json!({"pages": 250}))
            .await;
        assert_eq!(patch_a.status_code(), StatusCode::OK);
        let version_a = sqlx::query_scalar!(
            "SELECT pages_version_id FROM manifestations WHERE id = $1",
            m_a
        )
        .fetch_one(&ing_pool)
        .await
        .unwrap()
        .unwrap();

        // Revert on B with A's version id must 404: pages is
        // manifestation-scoped, so a sibling's journal row must not be
        // applicable to another edition's columns.
        let revert = server
            .post(&format!("/api/v1/manifestations/{m_b}/metadata/revert"))
            .add_header(AUTHORIZATION, basic)
            .json(&serde_json::json!({
                "field_name": "pages",
                "version_id": version_a,
            }))
            .await;
        assert_eq!(
            revert.status_code(),
            StatusCode::NOT_FOUND,
            "manifestation-scoped fields must keep the strict same-manifestation \
             match; body = {}",
            revert.text()
        );
    }

    /// Insert a `metadata_versions` row for `(manifestation_id, field_name)`
    /// via the ingestion pool, returning its id.
    async fn insert_version(
        ingestion_pool: &sqlx::PgPool,
        manifestation_id: Uuid,
        field: &str,
        value: serde_json::Value,
    ) -> Uuid {
        let hash = format!("hash-{}", Uuid::new_v4()).into_bytes();
        sqlx::query_scalar!(
            "INSERT INTO metadata_versions \
                (manifestation_id, source, field_name, new_value, value_hash, \
                 match_type, confidence_score, status) \
             VALUES ($1, 'openlibrary', $2, $3, $4, 'isbn', 0.96, 'pending'::metadata_review_status) \
             RETURNING id",
            manifestation_id,
            field,
            value,
            hash,
        )
        .fetch_one(ingestion_pool)
        .await
        .expect("insert metadata_versions")
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn accept_admin_writes_canonical_title(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (work_id, m_id) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;

        let new_title = format!("Canon Title {marker}");
        let version_id =
            insert_version(&ing_pool, m_id, "title", serde_json::json!(new_title)).await;

        let server = test_support::db::server_with_real_pools(&app_pool, &ing_pool);
        let response = server
            .post(&format!("/api/v1/manifestations/{m_id}/metadata/accept"))
            .add_header(AUTHORIZATION, basic)
            .json(&serde_json::json!({"version_id": version_id}))
            .await;
        assert_eq!(
            response.status_code(),
            StatusCode::OK,
            "body = {}",
            response.text()
        );

        let title: String = sqlx::query_scalar!("SELECT title FROM works WHERE id = $1", work_id)
            .fetch_one(&app_pool)
            .await
            .expect("fetch title");
        assert_eq!(title, new_title, "canonical title not written");

        let pointer: Option<Uuid> =
            sqlx::query_scalar!("SELECT title_version_id FROM works WHERE id = $1", work_id)
                .fetch_one(&app_pool)
                .await
                .expect("fetch title_version_id");
        assert_eq!(pointer, Some(version_id), "version pointer not wired");

        let status: String = sqlx::query_scalar!(
            "SELECT status::text AS \"status!\" FROM metadata_versions WHERE id = $1",
            version_id,
        )
        .fetch_one(&app_pool)
        .await
        .expect("fetch version status");
        assert_eq!(status, "pending", "accepted rows retain pending status");

        // Accept must have enqueued exactly one writeback_jobs row.
        let job_count: i64 = sqlx::query_scalar!(
            "SELECT count(*) AS \"count!\" FROM writeback_jobs WHERE manifestation_id = $1",
            m_id,
        )
        .fetch_one(&app_pool)
        .await
        .expect("fetch job count");
        assert_eq!(
            job_count, 1,
            "accept must enqueue exactly one writeback job; got {job_count}"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn accept_waits_for_concurrent_rejection(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (work_id, m_id) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;
        let version_id =
            insert_version(&ing_pool, m_id, "title", serde_json::json!("Rejected")).await;
        let before = sqlx::query!(
            "SELECT title, title_version_id FROM works WHERE id = $1",
            work_id
        )
        .fetch_one(&app_pool)
        .await
        .unwrap();
        let mut barrier = pool.begin().await.unwrap();
        sqlx::query!("LOCK TABLE metadata_versions IN SHARE MODE")
            .execute(&mut *barrier)
            .await
            .unwrap();
        let server = test_support::db::server_with_real_pools(&app_pool, &ing_pool);
        let reject_basic = basic.clone();
        let mut rejection = tokio::spawn(async move {
            server
                .post(&format!("/api/v1/manifestations/{m_id}/metadata/reject"))
                .add_header(AUTHORIZATION, reject_basic)
                .json(&serde_json::json!({"version_id": version_id}))
                .await
        });
        tokio::select! {
            response = &mut rejection => panic!("reject bypassed barrier: {:?}", response.unwrap().status_code()),
            () = test_support::wait_for_blocked_query(&pool, "UPDATE metadata_versions SET status = 'rejected'%", 1) => {}
        }
        let server = test_support::db::server_with_real_pools(&app_pool, &ing_pool);
        let mut acceptance = tokio::spawn(async move {
            server
                .post(&format!("/api/v1/manifestations/{m_id}/metadata/accept"))
                .add_header(AUTHORIZATION, basic)
                .json(&serde_json::json!({"version_id": version_id}))
                .await
        });
        tokio::select! {
            response = &mut acceptance => panic!("accept finished before rejection committed: {:?}", response.unwrap().status_code()),
            () = test_support::wait_for_blocked_query(&pool, "SELECT pg_advisory_xact_lock%", 1) => {}
        }
        barrier.commit().await.unwrap();
        assert_eq!(rejection.await.unwrap().status_code(), StatusCode::OK);
        let response = acceptance.await.unwrap();
        test_support::assert_problem(&response, problems::NOT_FOUND, StatusCode::NOT_FOUND);
        let after = sqlx::query!(
            "SELECT title, title_version_id FROM works WHERE id = $1",
            work_id
        )
        .fetch_one(&app_pool)
        .await
        .unwrap();
        assert_eq!(after.title, before.title);
        assert_eq!(after.title_version_id, before.title_version_id);
        let status = sqlx::query_scalar!(
            "SELECT status::text AS \"status!\" FROM metadata_versions WHERE id = $1",
            version_id,
        )
        .fetch_one(&app_pool)
        .await
        .unwrap();
        assert_eq!(status, "rejected");
        let jobs = sqlx::query_scalar!(
            "SELECT count(*) AS \"count!\" FROM writeback_jobs WHERE manifestation_id = $1",
            m_id,
        )
        .fetch_one(&app_pool)
        .await
        .unwrap();
        assert_eq!(jobs, 0);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn revert_admin_clears_field_to_null(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (work_id, m_id) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;

        // Pre-set a description on the work + a version pointer; revert with
        // version_id=null should clear both.
        let initial = format!("To Be Cleared {marker}");
        let version_id =
            insert_version(&ing_pool, m_id, "description", serde_json::json!(&initial)).await;
        sqlx::query!(
            "UPDATE works SET description = $1, description_version_id = $2 WHERE id = $3",
            initial,
            version_id,
            work_id,
        )
        .execute(&ing_pool)
        .await
        .expect("seed description");

        let server = test_support::db::server_with_real_pools(&app_pool, &ing_pool);
        let response = server
            .post(&format!("/api/v1/manifestations/{m_id}/metadata/revert"))
            .add_header(AUTHORIZATION, basic)
            .json(&serde_json::json!({
                "field_name": "description",
                "version_id": serde_json::Value::Null,
            }))
            .await;
        assert_eq!(
            response.status_code(),
            StatusCode::OK,
            "body = {}",
            response.text()
        );

        let row = sqlx::query!(
            "SELECT description, description_version_id FROM works WHERE id = $1",
            work_id,
        )
        .fetch_one(&app_pool)
        .await
        .expect("fetch work");
        assert_eq!(row.description, None, "description should be cleared");
        assert_eq!(
            row.description_version_id, None,
            "version pointer should be cleared"
        );

        // Revert must enqueue exactly one writeback_jobs row — the OPF
        // still needs the field cleared on disk.
        let job_count: i64 = sqlx::query_scalar!(
            "SELECT count(*) AS \"count!\" FROM writeback_jobs WHERE manifestation_id = $1",
            m_id,
        )
        .fetch_one(&app_pool)
        .await
        .expect("fetch job count");
        assert_eq!(
            job_count, 1,
            "revert must enqueue exactly one writeback job; got {job_count}"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn double_accept_enqueues_two_jobs(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (_work_id, m_id) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;

        let v1 = insert_version(
            &ing_pool,
            m_id,
            "title",
            serde_json::json!(format!("First {marker}")),
        )
        .await;
        let v2 = insert_version(
            &ing_pool,
            m_id,
            "title",
            serde_json::json!(format!("Second {marker}")),
        )
        .await;

        let server = test_support::db::server_with_real_pools(&app_pool, &ing_pool);
        for vid in [v1, v2] {
            let response = server
                .post(&format!("/api/v1/manifestations/{m_id}/metadata/accept"))
                .add_header(AUTHORIZATION, basic.clone())
                .json(&serde_json::json!({"version_id": vid}))
                .await;
            assert_eq!(response.status_code(), StatusCode::OK);
        }

        // Emitter does NOT dedup; worker does.  Two accepts → two rows.
        let job_count: i64 = sqlx::query_scalar!(
            "SELECT count(*) AS \"count!\" FROM writeback_jobs WHERE manifestation_id = $1",
            m_id,
        )
        .fetch_one(&app_pool)
        .await
        .expect("fetch job count");
        assert_eq!(job_count, 2, "two accepts must enqueue two rows (no dedup)");
    }

    // ── PATCH /api/v1/books/{id}/metadata (manual edit, RFC 7396) ───────────

    #[sqlx::test(migrations = "./migrations")]
    async fn patch_sets_title_and_writes_canonical(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (work_id, m_id) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;

        let new_title = format!("Manual Title {marker}");
        let server = test_support::db::server_with_real_pools(&app_pool, &ing_pool);
        let initial = server
            .get(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
            .await;
        let etag = etag_value(initial.headers());
        let response = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic)
            .add_header(
                axum::http::header::IF_MATCH,
                axum::http::HeaderValue::from_str(&etag).unwrap(),
            )
            .json(&serde_json::json!({"title": new_title}))
            .await;
        assert_eq!(
            response.status_code(),
            StatusCode::OK,
            "body = {}",
            response.text()
        );
        let body: serde_json::Value = response.json();

        let row = sqlx::query!(
            "SELECT title, title_version_id FROM works WHERE id = $1",
            work_id,
        )
        .fetch_one(&app_pool)
        .await
        .expect("fetch work");
        assert_eq!(row.title, new_title, "canonical title not written");

        let version_id = row.title_version_id.expect("title_version_id wired");
        assert_eq!(
            body["values"]["title"],
            serde_json::json!(new_title),
            "response must echo the applied title"
        );

        let v = sqlx::query!(
            "SELECT source, status::text AS \"status!\", new_value AS \"new_value!\" \
             FROM metadata_versions WHERE id = $1",
            version_id,
        )
        .fetch_one(&app_pool)
        .await
        .expect("fetch version");
        assert_eq!(v.source, "manual");
        assert_eq!(v.status, "pending");
        assert_eq!(v.new_value, serde_json::json!(new_title));

        let job_count: i64 = sqlx::query_scalar!(
            "SELECT count(*) AS \"count!\" FROM writeback_jobs WHERE manifestation_id = $1",
            m_id,
        )
        .fetch_one(&app_pool)
        .await
        .expect("fetch job count");
        assert_eq!(job_count, 1, "manual edit must enqueue one writeback");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn patch_two_fields_enqueues_one_writeback_per_field(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (_work_id, m_id) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;

        let server = test_support::db::server_with_real_pools(&app_pool, &ing_pool);
        let initial = server
            .get(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
            .await;
        let etag = etag_value(initial.headers());
        let response = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic)
            .add_header(
                axum::http::header::IF_MATCH,
                axum::http::HeaderValue::from_str(&etag).unwrap(),
            )
            .json(&serde_json::json!({
                "title": format!("Two Field Title {marker}"),
                "publisher": format!("Two Field Publisher {marker}"),
            }))
            .await;
        assert_eq!(
            response.status_code(),
            StatusCode::OK,
            "body = {}",
            response.text()
        );

        let job_count: i64 = sqlx::query_scalar!(
            "SELECT count(*) AS \"count!\" FROM writeback_jobs WHERE manifestation_id = $1",
            m_id,
        )
        .fetch_one(&app_pool)
        .await
        .expect("fetch job count");
        assert_eq!(
            job_count, 2,
            "a two-field edit enqueues one writeback per changed field"
        );
    }

    async fn review_mutation_state(pool: &sqlx::PgPool, m_id: Uuid) -> serde_json::Value {
        sqlx::query_scalar!(
            "SELECT jsonb_build_object( \
                'work', to_jsonb(w), 'manifestation', to_jsonb(m), \
                'versions', (SELECT coalesce(jsonb_agg(to_jsonb(v) ORDER BY v.id), '[]') \
                             FROM metadata_versions v WHERE v.manifestation_id = m.id), \
                'locks', (SELECT coalesce(jsonb_agg(to_jsonb(l) ORDER BY l.entity_type, l.field_name), '[]') \
                          FROM field_locks l WHERE l.manifestation_id = m.id), \
                'jobs', (SELECT coalesce(jsonb_agg(to_jsonb(j) ORDER BY j.id), '[]') \
                         FROM writeback_jobs j WHERE j.manifestation_id = m.id) \
             ) AS \"state!\" \
             FROM manifestations m JOIN works w ON w.id = m.work_id WHERE m.id = $1",
            m_id,
        )
        .fetch_one(pool)
        .await
        .unwrap()
    }

    async fn child_review_request_preserves_state(pool: &sqlx::PgPool, action: &str) {
        let app = test_support::db::app_pool_for(pool).await;
        let ing = test_support::db::ingestion_pool_for(pool).await;
        let (child_id, child_auth) =
            test_support::db::create_child_user_and_basic_auth(&app, action).await;
        let (adult_id, adult_auth) =
            test_support::db::create_adult_and_basic_auth(&app, action).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (work_id, m_id) = test_support::db::insert_work_and_manifestation(&ing, &marker).await;
        let shelf = test_support::db::create_shelf(&app, child_id, action).await;
        test_support::db::add_to_shelf(&app, shelf, m_id).await;
        let prior =
            insert_version(&ing, m_id, "subtitle", serde_json::json!("Prior subtitle")).await;
        let proposal = insert_version(
            &ing,
            m_id,
            "subtitle",
            serde_json::json!("Proposed subtitle"),
        )
        .await;
        sqlx::query!(
            "UPDATE works SET subtitle = 'Prior subtitle', subtitle_version_id = $1 WHERE id = $2",
            prior,
            work_id
        )
        .execute(&ing)
        .await
        .unwrap();
        let mut tx = app.begin().await.unwrap();
        crate::services::enrichment::field_lock::lock_tx(
            &mut tx,
            m_id,
            crate::services::enrichment::field_lock::EntityType::Work,
            "subtitle",
            adult_id,
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        let payload = match action {
            "accept" | "reject" => serde_json::json!({"version_id": proposal}),
            "revert" => serde_json::json!({"field_name": "subtitle", "version_id": null}),
            "lock" => {
                serde_json::json!({"entity_type": "manifestation", "field_name": "publisher"})
            }
            "unlock" => serde_json::json!({"entity_type": "work", "field_name": "subtitle"}),
            _ => panic!("unknown review action"),
        };
        let before = review_mutation_state(pool, m_id).await;
        let server = test_support::db::server_with_real_pools(&app, &ing);
        let url = format!("/api/v1/manifestations/{m_id}/metadata/{action}");
        let response = server
            .post(&url)
            .add_header(AUTHORIZATION, child_auth.clone())
            .json(&payload)
            .await;
        assert_eq!(
            response.status_code(),
            StatusCode::FORBIDDEN,
            "{}",
            response.text()
        );
        assert_eq!(review_mutation_state(pool, m_id).await, before);
        if action == "revert" {
            let response = server
                .post(&url)
                .add_header(AUTHORIZATION, child_auth)
                .json(&serde_json::json!({"field_name": "subtitle", "version_id": proposal}))
                .await;
            assert_eq!(response.status_code(), StatusCode::FORBIDDEN);
            assert_eq!(review_mutation_state(pool, m_id).await, before);
        }
        if matches!(action, "accept" | "revert") {
            let response = server
                .post(&format!("/api/v1/manifestations/{m_id}/metadata/unlock"))
                .add_header(AUTHORIZATION, adult_auth.clone())
                .json(&serde_json::json!({"entity_type": "work", "field_name": "subtitle"}))
                .await;
            assert_eq!(response.status_code(), StatusCode::OK);
        }
        let before = review_mutation_state(pool, m_id).await;
        let response = server
            .post(&url)
            .add_header(AUTHORIZATION, adult_auth)
            .json(&payload)
            .await;
        assert_eq!(
            response.status_code(),
            if action == "lock" {
                StatusCode::CREATED
            } else {
                StatusCode::OK
            },
            "{}",
            response.text()
        );
        let after = review_mutation_state(pool, m_id).await;
        assert_adult_review_change(action, &before, &after, proposal, adult_id);
    }

    fn assert_adult_review_change(
        action: &str,
        before: &serde_json::Value,
        after: &serde_json::Value,
        proposal: Uuid,
        adult_id: Uuid,
    ) {
        match action {
            "accept" => {
                assert_eq!(after["work"]["subtitle"], "Proposed subtitle");
                assert_eq!(after["work"]["subtitle_version_id"], proposal.to_string());
                assert_eq!(after["jobs"].as_array().unwrap().len(), 1);
            }
            "reject" => {
                let version = after["versions"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|v| v["id"] == proposal.to_string())
                    .unwrap();
                assert_eq!(version["status"], "rejected");
                assert_eq!(version["resolved_by"], adult_id.to_string());
                assert!(!version["resolved_at"].is_null());
                assert_eq!(after["work"], before["work"]);
                assert_eq!(after["jobs"], before["jobs"]);
            }
            "revert" => {
                assert!(after["work"]["subtitle"].is_null());
                assert!(after["work"]["subtitle_version_id"].is_null());
                assert_eq!(after["jobs"].as_array().unwrap().len(), 1);
            }
            "lock" => assert_eq!(after["locks"].as_array().unwrap().len(), 2),
            "unlock" => assert_eq!(after["locks"], serde_json::json!([])),
            _ => unreachable!(),
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn accept_child_account_forbidden_without_writes(pool: sqlx::PgPool) {
        child_review_request_preserves_state(&pool, "accept").await;
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn reject_child_account_forbidden_without_writes(pool: sqlx::PgPool) {
        child_review_request_preserves_state(&pool, "reject").await;
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn revert_child_account_forbidden_without_writes(pool: sqlx::PgPool) {
        child_review_request_preserves_state(&pool, "revert").await;
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn lock_child_account_forbidden_without_writes(pool: sqlx::PgPool) {
        child_review_request_preserves_state(&pool, "lock").await;
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn unlock_child_account_forbidden_without_writes(pool: sqlx::PgPool) {
        child_review_request_preserves_state(&pool, "unlock").await;
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn patch_child_account_forbidden(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_child_id, basic) =
            test_support::db::create_child_user_and_basic_auth(&app_pool, "patch").await;
        let marker = Uuid::new_v4().simple().to_string();
        let (_work_id, m_id) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;

        let server = test_support::db::server_with_real_pools(&app_pool, &ing_pool);
        let response = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic)
            .json(&serde_json::json!({"title": "Nope"}))
            .await;
        assert_eq!(response.status_code(), StatusCode::FORBIDDEN);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn patch_null_clear_does_not_surface_as_pending_draft(pool: sqlx::PgPool) {
        // Regression: PATCH-clear inserts an audit row with
        // new_value='null'::jsonb. Without the filter in
        // load_pending_versions, that row would surface in
        // BookDetail.metadata_versions as a draft with "(null)" value
        // and Accept/Reject buttons.
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (work_id, m_id) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;
        let initial = format!("seed prose {marker}");
        let seed_v =
            insert_version(&ing_pool, m_id, "description", serde_json::json!(&initial)).await;
        sqlx::query!(
            "UPDATE works SET description = $1, description_version_id = $2 WHERE id = $3",
            initial,
            seed_v,
            work_id,
        )
        .execute(&ing_pool)
        .await
        .expect("seed description");

        let server = test_support::db::server_with_real_pools(&app_pool, &ing_pool);
        let initial = server
            .get(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
            .await;
        let etag = etag_value(initial.headers());
        let response = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
            .add_header(
                axum::http::header::IF_MATCH,
                axum::http::HeaderValue::from_str(&etag).unwrap(),
            )
            .json(&serde_json::json!({"description": serde_json::Value::Null}))
            .await;
        assert_eq!(response.status_code(), StatusCode::OK);

        // Now GET the detail and confirm the audit row is absent.
        let detail = server
            .get(&format!("/api/v1/books/{m_id}"))
            .add_header(AUTHORIZATION, basic)
            .await;
        assert_eq!(detail.status_code(), StatusCode::OK);
        let body: serde_json::Value = detail.json();
        let versions = body
            .get("metadata_versions")
            .and_then(|v| v.as_array())
            .expect("metadata_versions array present");
        for v in versions {
            assert_ne!(
                v.get("new_value"),
                Some(&serde_json::Value::Null),
                "null-value audit row leaked into Versions tab: {v}"
            );
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn accept_rejects_null_value_audit_row(pool: sqlx::PgPool) {
        // Regression for the null-accept corruption path: PATCH-clear
        // inserts a `new_value = 'null'::jsonb` audit row; the Versions
        // tab filters it, but POST /accept used to fetch any row by id
        // and run `apply_version`, which fell through to writing the
        // literal string "null" into the canonical column. The handler
        // must reject that promotion with 422.
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (work_id, m_id) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;
        let seed = format!("seed prose {marker}");
        let seed_v = insert_version(&ing_pool, m_id, "description", serde_json::json!(&seed)).await;
        sqlx::query!(
            "UPDATE works SET description = $1, description_version_id = $2 WHERE id = $3",
            seed,
            seed_v,
            work_id,
        )
        .execute(&ing_pool)
        .await
        .expect("seed description");

        let server = test_support::db::server_with_real_pools(&app_pool, &ing_pool);
        let initial = server
            .get(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
            .await;
        let etag = etag_value(initial.headers());
        // PATCH description -> null. Creates a `new_value = 'null'`
        // audit row + clears the canonical column.
        let r = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
            .add_header(
                axum::http::header::IF_MATCH,
                axum::http::HeaderValue::from_str(&etag).unwrap(),
            )
            .json(&serde_json::json!({"description": serde_json::Value::Null}))
            .await;
        assert_eq!(r.status_code(), StatusCode::OK);

        // Fetch the null-value row id directly. The Versions tab filter
        // hides it, so we read it from the table.
        let null_row_id: Uuid = sqlx::query_scalar!(
            "SELECT id FROM metadata_versions \
             WHERE manifestation_id = $1 AND field_name = 'description' \
             AND new_value = 'null'::jsonb AND source = 'manual' \
             ORDER BY created_at DESC LIMIT 1",
            m_id,
        )
        .fetch_one(&app_pool)
        .await
        .expect("null audit row exists");

        let accept = server
            .post(&format!("/api/v1/manifestations/{m_id}/metadata/accept"))
            .add_header(AUTHORIZATION, basic)
            .json(&serde_json::json!({"version_id": null_row_id}))
            .await;
        assert_eq!(
            accept.status_code(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "accept of null-value row must 422; got body = {}",
            accept.text()
        );

        // Canonical column stays NULL (the PATCH-clear committed it).
        // No literal-"null" string corruption.
        let description: Option<String> =
            sqlx::query_scalar!("SELECT description FROM works WHERE id = $1", work_id)
                .fetch_one(&app_pool)
                .await
                .expect("fetch description");
        assert_eq!(
            description, None,
            "canonical description must stay NULL, not be overwritten with literal \"null\""
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn publication_date_roundtrip_preserves_precision(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (_work_id, m_id) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ing_pool);
        let path = format!("/api/v1/books/{m_id}/metadata");
        let initial = server
            .get(&path)
            .add_header(AUTHORIZATION, basic.clone())
            .await;
        let mut etag = etag_value(initial.headers());
        let values = [
            "2024",
            "2024-06",
            "2024-01-15",
            "2024-01-15T00:00Z",
            "2024-01-15T00:00:00Z",
            "2024-01-15T00:00:00.123456789123+05:30",
            "2024-01-15T00:00:00Z",
        ];
        for pub_date in values {
            let response = server
                .patch(&path)
                .add_header(AUTHORIZATION, basic.clone())
                .add_header(
                    axum::http::header::IF_MATCH,
                    axum::http::HeaderValue::from_str(&etag).unwrap(),
                )
                .json(&serde_json::json!({"pub_date": pub_date}))
                .await;
            assert_eq!(
                response.status_code(),
                StatusCode::OK,
                "{}",
                response.text()
            );
            let current = server
                .get(&path)
                .add_header(AUTHORIZATION, basic.clone())
                .await;
            assert_eq!(current.status_code(), StatusCode::OK);
            let body: serde_json::Value = current.json();
            assert_eq!(body["pub_date"], pub_date);
            etag = etag_value(current.headers());
        }
        let rows = sqlx::query!(
            "SELECT observation_count FROM metadata_versions \
             WHERE manifestation_id = $1 AND source = 'manual' AND field_name = 'pub_date'",
            m_id,
        )
        .fetch_all(&app_pool)
        .await
        .expect("fetch manual pub_date rows");
        assert_eq!(rows.len(), 6);
        assert_eq!(
            rows.iter().filter(|row| row.observation_count == 2).count(),
            1
        );
        for (patch, expected) in [
            (
                serde_json::json!({"publisher": "Precision Press"}),
                serde_json::json!(values[6]),
            ),
            (
                serde_json::json!({"pub_date": null}),
                serde_json::Value::Null,
            ),
        ] {
            let response = server
                .patch(&path)
                .add_header(AUTHORIZATION, basic.clone())
                .add_header(
                    axum::http::header::IF_MATCH,
                    axum::http::HeaderValue::from_str(&etag).unwrap(),
                )
                .json(&patch)
                .await;
            assert_eq!(
                response.status_code(),
                StatusCode::OK,
                "{}",
                response.text()
            );
            let current = server
                .get(&path)
                .add_header(AUTHORIZATION, basic.clone())
                .await;
            let body: serde_json::Value = current.json();
            assert_eq!(body["pub_date"], expected);
            etag = etag_value(current.headers());
        }
        let before = server
            .get(&path)
            .add_header(AUTHORIZATION, basic.clone())
            .await;
        let before_body: serde_json::Value = before.json();
        let snapshot = || async {
            sqlx::query!(
                "SELECT (SELECT count(*) FROM metadata_versions WHERE manifestation_id = $1) AS journal_count, (SELECT sum(observation_count)::bigint FROM metadata_versions WHERE manifestation_id = $1) AS observations, (SELECT count(*) FROM writeback_jobs WHERE manifestation_id = $1) AS job_count",
                m_id,
            ).fetch_one(&app_pool).await.unwrap()
        };
        let before_state = snapshot().await;
        let refused = server
            .patch(&path)
            .add_header(AUTHORIZATION, basic.clone())
            .add_header(
                axum::http::header::IF_MATCH,
                axum::http::HeaderValue::from_str(&etag).unwrap(),
            )
            .json(&serde_json::json!({"publisher": "Refused Press", "pub_date": "unknown"}))
            .await;
        assert_eq!(refused.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
        let problem: serde_json::Value = refused.json();
        assert!(
            problem["errors"]["pub_date"]
                .as_str()
                .is_some_and(|message| !message.is_empty())
        );
        let after = server
            .get(&path)
            .add_header(AUTHORIZATION, basic.clone())
            .await;
        assert_eq!(after.json::<serde_json::Value>(), before_body);
        assert_eq!(etag_value(after.headers()), etag);
        let after_state = snapshot().await;
        assert_eq!(
            (
                after_state.journal_count,
                after_state.observations,
                after_state.job_count
            ),
            (
                before_state.journal_count,
                before_state.observations,
                before_state.job_count
            ),
        );
    }

    // ── PATCH contributors / subtitle / pages ──────────────────────────────

    #[sqlx::test(migrations = "./migrations")]
    async fn patch_contributors_replace_sets_names_in_order(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (work_id, m_id) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;

        let name_a = format!("Alpha Author {marker}");
        let name_b = format!("Beta Author {marker}");
        let server = test_support::db::server_with_real_pools(&app_pool, &ing_pool);
        let initial = server
            .get(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
            .await;
        let etag = etag_value(initial.headers());
        let response = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic)
            .add_header(
                axum::http::header::IF_MATCH,
                axum::http::HeaderValue::from_str(&etag).unwrap(),
            )
            .json(&serde_json::json!({"contributors": {"author": [name_a, name_b], "editor": ["Editor"], "translator": ["Translator"]}}))
            .await;
        assert_eq!(
            response.status_code(),
            StatusCode::OK,
            "body = {}",
            response.text()
        );

        let rows = sqlx::query!(
            "SELECT a.name, wa.position FROM work_authors wa \
             JOIN authors a ON a.id = wa.author_id \
             WHERE wa.work_id = $1 AND wa.role = 'author' ORDER BY wa.position",
            work_id,
        )
        .fetch_all(&app_pool)
        .await
        .expect("fetch work_authors");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].name, name_a);
        assert_eq!(rows[0].position, 0);
        assert_eq!(rows[1].name, name_b);
        assert_eq!(rows[1].position, 1);

        let sort_name = sqlx::query_scalar!(
            "SELECT first_author_sort_name FROM works WHERE id = $1",
            work_id,
        )
        .fetch_one(&app_pool)
        .await
        .expect("fetch sort name");
        assert_eq!(
            sort_name.as_deref(),
            Some(format!("{marker}, Alpha Author").as_str()),
            "sort form moves the final name token to the front; \
             refresh_first_author_sort must have run"
        );

        let journal_value: serde_json::Value = sqlx::query_scalar!(
            "SELECT new_value FROM metadata_versions \
             WHERE manifestation_id = $1 AND field_name = 'contributors.author' \
             ORDER BY created_at DESC LIMIT 1",
            m_id,
        )
        .fetch_one(&app_pool)
        .await
        .expect("fetch journal row");
        assert_eq!(journal_value, serde_json::json!([name_a, name_b]));

        let jobs: i64 = sqlx::query_scalar!(
            "SELECT count(*) AS \"count!\" FROM writeback_jobs WHERE manifestation_id = $1",
            m_id,
        )
        .fetch_one(&app_pool)
        .await
        .expect("count writeback jobs");
        assert_eq!(
            jobs, 1,
            "contributor edit re-renders the library path; exactly one \
             writeback job must ride the same transaction"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn patch_contributors_empty_object_enqueues_no_writeback(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (_work_id, m_id) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;

        let server = test_support::db::server_with_real_pools(&app_pool, &ing_pool);
        let initial = server
            .get(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
            .await;
        let etag = etag_value(initial.headers());
        let response = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic)
            .add_header(
                axum::http::header::IF_MATCH,
                axum::http::HeaderValue::from_str(&etag).unwrap(),
            )
            .json(&serde_json::json!({"contributors": {}}))
            .await;
        assert_eq!(
            response.status_code(),
            StatusCode::OK,
            "body = {}",
            response.text()
        );

        let jobs: i64 = sqlx::query_scalar!(
            "SELECT count(*) AS \"count!\" FROM writeback_jobs WHERE manifestation_id = $1",
            m_id,
        )
        .fetch_one(&app_pool)
        .await
        .expect("count writeback jobs");
        assert_eq!(jobs, 0, "role-free patch touches nothing; no writeback");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn patch_contributors_editor_only_leaves_authors_untouched(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (work_id, m_id) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;
        let author_name = format!("Untouched Author {marker}");
        test_support::db::insert_contributor(&ing_pool, work_id, &author_name, "author", 0).await;

        let editor_name = format!("New Editor {marker}");
        let server = test_support::db::server_with_real_pools(&app_pool, &ing_pool);
        let initial = server
            .get(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
            .await;
        let etag = etag_value(initial.headers());
        let response = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic)
            .add_header(
                axum::http::header::IF_MATCH,
                axum::http::HeaderValue::from_str(&etag).unwrap(),
            )
            .json(&serde_json::json!({"contributors": {"editor": [editor_name]}}))
            .await;
        assert_eq!(
            response.status_code(),
            StatusCode::OK,
            "body = {}",
            response.text()
        );

        let author_count = sqlx::query_scalar!(
            "SELECT COUNT(*) AS \"count!\" FROM work_authors \
             WHERE work_id = $1 AND role = 'author'",
            work_id,
        )
        .fetch_one(&app_pool)
        .await
        .expect("count authors");
        assert_eq!(author_count, 1, "editor-only patch must not touch authors");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn patch_contributors_top_level_null_on_authored_work_returns_422(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (work_id, m_id) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;
        test_support::db::insert_contributor(
            &ing_pool,
            work_id,
            &format!("Sole Author {marker}"),
            "author",
            0,
        )
        .await;

        let server = test_support::db::server_with_real_pools(&app_pool, &ing_pool);
        let initial = server
            .get(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
            .await;
        let etag = etag_value(initial.headers());
        let response = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic)
            .add_header(
                axum::http::header::IF_MATCH,
                axum::http::HeaderValue::from_str(&etag).unwrap(),
            )
            .json(&serde_json::json!({"contributors": serde_json::Value::Null}))
            .await;
        assert_eq!(
            response.status_code(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "clearing the sole author via top-level null must 422, body = {}",
            response.text()
        );

        let author_count = sqlx::query_scalar!(
            "SELECT COUNT(*) AS \"count!\" FROM work_authors \
             WHERE work_id = $1 AND role = 'author'",
            work_id,
        )
        .fetch_one(&app_pool)
        .await
        .expect("count authors");
        assert_eq!(
            author_count, 1,
            "rejected patch must roll back, not partially apply"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn patch_contributors_top_level_null_on_authorless_stub_succeeds(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (work_id, m_id) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;
        test_support::db::insert_contributor(
            &ing_pool,
            work_id,
            &format!("Editor Only {marker}"),
            "editor",
            0,
        )
        .await;

        let server = test_support::db::server_with_real_pools(&app_pool, &ing_pool);
        let initial = server
            .get(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
            .await;
        let etag = etag_value(initial.headers());
        let response = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic)
            .add_header(
                axum::http::header::IF_MATCH,
                axum::http::HeaderValue::from_str(&etag).unwrap(),
            )
            .json(&serde_json::json!({"contributors": serde_json::Value::Null}))
            .await;
        assert_eq!(
            response.status_code(),
            StatusCode::OK,
            "authorless stub has nothing to violate the last-author guard; body = {}",
            response.text()
        );

        let editor_count = sqlx::query_scalar!(
            "SELECT COUNT(*) AS \"count!\" FROM work_authors \
             WHERE work_id = $1 AND role = 'editor'",
            work_id,
        )
        .fetch_one(&app_pool)
        .await
        .expect("count editors");
        assert_eq!(editor_count, 0);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn patch_contributors_clear_authors_via_empty_array_on_stub_is_noop_ok(
        pool: sqlx::PgPool,
    ) {
        // No pre-existing authors: `author: []` touches the author key but
        // pre/post counts are both zero, so the last-author guard must not fire.
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (_work_id, m_id) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;

        let server = test_support::db::server_with_real_pools(&app_pool, &ing_pool);
        let initial = server
            .get(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
            .await;
        let etag = etag_value(initial.headers());
        let response = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic)
            .add_header(
                axum::http::header::IF_MATCH,
                axum::http::HeaderValue::from_str(&etag).unwrap(),
            )
            .json(&serde_json::json!({"contributors": {"author": []}}))
            .await;
        assert_eq!(
            response.status_code(),
            StatusCode::OK,
            "body = {}",
            response.text()
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn patch_mixed_valid_invalid_fields_rolls_back_all(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (work_id, m_id) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;

        let server = test_support::db::server_with_real_pools(&app_pool, &ing_pool);
        // description is applied before isbn_13 in field order, so its
        // write has already been issued inside the transaction when the
        // invalid ISBN rejects the patch: this pins genuine rollback of an
        // in-flight write, not just fail-fast ordering.
        let initial = server
            .get(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
            .await;
        let etag = etag_value(initial.headers());
        let response = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic)
            .add_header(
                axum::http::header::IF_MATCH,
                axum::http::HeaderValue::from_str(&etag).unwrap(),
            )
            .json(&serde_json::json!({"description": "New", "isbn_13": "not-an-isbn"}))
            .await;
        assert_eq!(
            response.status_code(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "body = {}",
            response.text()
        );

        let row = sqlx::query!("SELECT description FROM works WHERE id = $1", work_id)
            .fetch_one(&app_pool)
            .await
            .expect("fetch work after rejected patch");
        assert!(row.description.is_none());
    }

    async fn writeback_job_count(pool: &sqlx::PgPool, m_id: Uuid) -> i64 {
        sqlx::query_scalar!(
            "SELECT count(*) AS \"count!\" FROM writeback_jobs WHERE manifestation_id = $1",
            m_id,
        )
        .fetch_one(pool)
        .await
        .expect("count writeback jobs")
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn patch_clears_identifier_slot_without_writeback(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (_work_id, m_id) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ing_pool);

        let initial = server
            .get(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
            .await;
        let etag = etag_value(initial.headers());
        let set = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
            .add_header(
                axum::http::header::IF_MATCH,
                axum::http::HeaderValue::from_str(&etag).unwrap(),
            )
            .json(&serde_json::json!({"manifestation_identifiers": {"asin": "B004GXAX8C"}}))
            .await;
        assert_eq!(set.status_code(), StatusCode::OK);
        let etag = etag_value(set.headers());

        let clear = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic)
            .add_header(
                axum::http::header::IF_MATCH,
                axum::http::HeaderValue::from_str(&etag).unwrap(),
            )
            .json(&serde_json::json!({"manifestation_identifiers": {"asin": null}}))
            .await;
        assert_eq!(
            clear.status_code(),
            StatusCode::OK,
            "body = {}",
            clear.text()
        );
        let body: serde_json::Value = clear.json();
        assert_eq!(
            body["values"]["manifestation_identifiers"]["asin"],
            serde_json::Value::Null
        );

        let remaining: i64 = sqlx::query_scalar!(
            "SELECT count(*) AS \"count!\" FROM manifestation_external_identifiers \
             WHERE manifestation_id = $1 AND scheme = 'asin'",
            m_id,
        )
        .fetch_one(&pool)
        .await
        .expect("count");
        assert_eq!(remaining, 0, "clear must delete the slot");

        // Accountability row for the clear, mirroring the scalar clear path.
        let audit: i64 = sqlx::query_scalar!(
            "SELECT count(*) AS \"count!\" FROM metadata_versions \
             WHERE manifestation_id = $1 \
               AND field_name = 'identifiers.manifestation.asin' \
               AND new_value = 'null'::jsonb",
            m_id,
        )
        .fetch_one(&pool)
        .await
        .expect("count audit rows");
        assert_eq!(audit, 1, "clear must journal an accountability row");

        assert_eq!(
            writeback_job_count(&app_pool, m_id).await,
            0,
            "identifier set + clear must enqueue no writeback"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn accept_staged_identifier_writes_registry_without_writeback(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (_work_id, m_id) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;

        let version_id = insert_version(
            &ing_pool,
            m_id,
            "identifiers.manifestation.googlebooks",
            serde_json::json!("zyTZAAAAYAAJ"),
        )
        .await;

        let server = test_support::db::server_with_real_pools(&app_pool, &ing_pool);
        let response = server
            .post(&format!("/api/v1/manifestations/{m_id}/metadata/accept"))
            .add_header(AUTHORIZATION, basic)
            .json(&serde_json::json!({"version_id": version_id}))
            .await;
        assert_eq!(
            response.status_code(),
            StatusCode::OK,
            "accept of a staged identifier must round-trip; body = {}",
            response.text()
        );

        let row = sqlx::query!(
            "SELECT external_id, source_version_id \
             FROM manifestation_external_identifiers \
             WHERE manifestation_id = $1 AND scheme = 'googlebooks'",
            m_id,
        )
        .fetch_one(&pool)
        .await
        .expect("registry row");
        assert_eq!(row.external_id, "zyTZAAAAYAAJ");
        assert_eq!(row.source_version_id, Some(version_id));

        assert_eq!(
            writeback_job_count(&app_pool, m_id).await,
            0,
            "identifier accept must not enqueue a writeback"
        );
    }

    // ── ETag / If-Match ───────────────────────────────────────────────────

    fn etag_value(headers: &axum::http::HeaderMap) -> String {
        headers
            .get(axum::http::header::ETAG)
            .expect("ETag header present")
            .to_str()
            .expect("ETag ascii")
            .to_owned()
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn patch_metadata_without_if_match_returns_428(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (_work_id, m_id) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ing_pool);
        let before = server
            .get(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
            .await;

        let r = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
            .json(&serde_json::json!({"title": format!("No If-Match {marker}")}))
            .await;
        test_support::assert_problem(
            &r,
            problems::IF_MATCH_REQUIRED,
            StatusCode::PRECONDITION_REQUIRED,
        );
        let after = server
            .get(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic)
            .await;
        assert_eq!(etag_value(after.headers()), etag_value(before.headers()));
        assert_eq!(after.text(), before.text());
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn patch_metadata_with_matching_if_match_succeeds(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (_work_id, m_id) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ing_pool);

        let initial = server
            .get(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
            .await;
        let etag = etag_value(initial.headers());

        let r = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic)
            .add_header(
                axum::http::header::IF_MATCH,
                axum::http::HeaderValue::from_str(&etag).unwrap(),
            )
            .json(&serde_json::json!({"title": format!("Matched {marker}")}))
            .await;
        assert_eq!(r.status_code(), StatusCode::OK, "body: {}", r.text());
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn patch_metadata_with_stale_if_match_returns_412_with_current_etag(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (_work_id, m_id) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ing_pool);

        let initial = server
            .get(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
            .await;
        let stale_etag = etag_value(initial.headers());

        // Someone else's write lands first, invalidating the captured tag.
        server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
            .add_header(
                axum::http::header::IF_MATCH,
                axum::http::HeaderValue::from_str(&stale_etag).unwrap(),
            )
            .json(&serde_json::json!({"title": format!("Raced {marker}")}))
            .await;

        let r = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic)
            .add_header(
                axum::http::header::IF_MATCH,
                axum::http::HeaderValue::from_str(&stale_etag).unwrap(),
            )
            .json(&serde_json::json!({"title": format!("Should not land {marker}")}))
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

    /// RLS coverage for the taxonomy junction tables (`manifestation_genres`,
    /// `manifestation_moods`, `manifestation_tags`): a direct read or write on
    /// these tables must be scoped through `manifestations` visibility the
    /// same way the identifier registry is, so a caller that forgets the join
    /// cannot see or change associations outside its authorization.
    mod junction_rls {
        use crate::db::acquire_with_rls;
        use crate::test_support::db::{
            add_to_shelf, app_pool_for, create_adult_and_basic_auth,
            create_child_user_and_basic_auth, create_shelf, ingestion_pool_for,
            insert_work_and_manifestation, readonly_pool_for,
        };
        use sqlx::PgPool;
        use uuid::Uuid;

        fn is_rls_denied(err: &sqlx::Error) -> bool {
            // 42501 = insufficient_privilege: the clean RLS policy miss.
            err.as_database_error()
                .and_then(sqlx::error::DatabaseError::code)
                .as_deref()
                == Some("42501")
        }

        // ---- manifestation_genres ----

        async fn seed_genre(ingestion: &PgPool, name: &str) -> Uuid {
            sqlx::query_scalar!("INSERT INTO genres (name) VALUES ($1) RETURNING id", name)
                .fetch_one(ingestion)
                .await
                .expect("insert genre")
        }

        async fn link_genre(ingestion: &PgPool, manifestation_id: Uuid, genre_id: Uuid) {
            sqlx::query!(
                "INSERT INTO manifestation_genres (manifestation_id, genre_id) VALUES ($1, $2)",
                manifestation_id,
                genre_id,
            )
            .execute(ingestion)
            .await
            .expect("insert manifestation_genres");
        }

        #[sqlx::test(migrations = "./migrations")]
        async fn genres_child_reads_only_shelf_visible(pool: PgPool) {
            let ingestion = ingestion_pool_for(&pool).await;
            let app = app_pool_for(&pool).await;
            let (_vw, visible_m) = insert_work_and_manifestation(&ingestion, "genre-vis").await;
            let (_hw, hidden_m) = insert_work_and_manifestation(&ingestion, "genre-hid").await;
            let genre = seed_genre(&ingestion, "genre-child-scope").await;
            link_genre(&ingestion, visible_m, genre).await;
            link_genre(&ingestion, hidden_m, genre).await;

            let (child_id, _) = create_child_user_and_basic_auth(&app, "genre-child").await;
            let shelf = create_shelf(&app, child_id, "kids").await;
            add_to_shelf(&app, shelf, visible_m).await;

            let mut tx = acquire_with_rls(&app, child_id).await.expect("rls tx");
            let rows: Vec<Uuid> = sqlx::query_scalar!(
                "SELECT manifestation_id FROM manifestation_genres ORDER BY manifestation_id",
            )
            .fetch_all(&mut *tx)
            .await
            .expect("child select");
            assert_eq!(
                rows,
                vec![visible_m],
                "child sees only the shelf-visible manifestation's genre link"
            );
        }

        #[sqlx::test(migrations = "./migrations")]
        async fn genres_readonly_reads_scoped_and_lacks_write_grant(pool: PgPool) {
            let ingestion = ingestion_pool_for(&pool).await;
            let app = app_pool_for(&pool).await;
            let readonly = readonly_pool_for(&pool).await;
            let (_w, m) = insert_work_and_manifestation(&ingestion, "genre-ro").await;
            let genre = seed_genre(&ingestion, "genre-ro-scope").await;
            link_genre(&ingestion, m, genre).await;

            let (adult_id, _) = create_adult_and_basic_auth(&app, "genre-ro").await;
            let mut tx = acquire_with_rls(&readonly, adult_id).await.expect("rls tx");
            let count: i64 = sqlx::query_scalar!(
                "SELECT count(*) AS \"c!\" FROM manifestation_genres WHERE manifestation_id = $1",
                m,
            )
            .fetch_one(&mut *tx)
            .await
            .expect("readonly select");
            assert_eq!(count, 1, "readonly reads the same scoped rows");

            let err = sqlx::query!(
                "INSERT INTO manifestation_genres (manifestation_id, genre_id) VALUES ($1, $2)",
                m,
                genre,
            )
            .execute(&mut *tx)
            .await
            .expect_err("readonly insert denied");
            assert!(is_rls_denied(&err), "expected 42501, got {err:?}");
        }

        // ---- manifestation_moods ----

        async fn seed_mood(ingestion: &PgPool, name: &str) -> Uuid {
            sqlx::query_scalar!("INSERT INTO moods (name) VALUES ($1) RETURNING id", name)
                .fetch_one(ingestion)
                .await
                .expect("insert mood")
        }

        async fn link_mood(ingestion: &PgPool, manifestation_id: Uuid, mood_id: Uuid) {
            sqlx::query!(
                "INSERT INTO manifestation_moods (manifestation_id, mood_id) VALUES ($1, $2)",
                manifestation_id,
                mood_id,
            )
            .execute(ingestion)
            .await
            .expect("insert manifestation_moods");
        }

        #[sqlx::test(migrations = "./migrations")]
        async fn moods_child_reads_only_shelf_visible(pool: PgPool) {
            let ingestion = ingestion_pool_for(&pool).await;
            let app = app_pool_for(&pool).await;
            let (_vw, visible_m) = insert_work_and_manifestation(&ingestion, "mood-vis").await;
            let (_hw, hidden_m) = insert_work_and_manifestation(&ingestion, "mood-hid").await;
            let mood = seed_mood(&ingestion, "mood-child-scope").await;
            link_mood(&ingestion, visible_m, mood).await;
            link_mood(&ingestion, hidden_m, mood).await;

            let (child_id, _) = create_child_user_and_basic_auth(&app, "mood-child").await;
            let shelf = create_shelf(&app, child_id, "kids").await;
            add_to_shelf(&app, shelf, visible_m).await;

            let mut tx = acquire_with_rls(&app, child_id).await.expect("rls tx");
            let rows: Vec<Uuid> = sqlx::query_scalar!(
                "SELECT manifestation_id FROM manifestation_moods ORDER BY manifestation_id",
            )
            .fetch_all(&mut *tx)
            .await
            .expect("child select");
            assert_eq!(
                rows,
                vec![visible_m],
                "child sees only the shelf-visible manifestation's mood link"
            );
        }

        #[sqlx::test(migrations = "./migrations")]
        async fn moods_readonly_reads_scoped_and_lacks_write_grant(pool: PgPool) {
            let ingestion = ingestion_pool_for(&pool).await;
            let app = app_pool_for(&pool).await;
            let readonly = readonly_pool_for(&pool).await;
            let (_w, m) = insert_work_and_manifestation(&ingestion, "mood-ro").await;
            let mood = seed_mood(&ingestion, "mood-ro-scope").await;
            link_mood(&ingestion, m, mood).await;

            let (adult_id, _) = create_adult_and_basic_auth(&app, "mood-ro").await;
            let mut tx = acquire_with_rls(&readonly, adult_id).await.expect("rls tx");
            let count: i64 = sqlx::query_scalar!(
                "SELECT count(*) AS \"c!\" FROM manifestation_moods WHERE manifestation_id = $1",
                m,
            )
            .fetch_one(&mut *tx)
            .await
            .expect("readonly select");
            assert_eq!(count, 1, "readonly reads the same scoped rows");

            let err = sqlx::query!(
                "INSERT INTO manifestation_moods (manifestation_id, mood_id) VALUES ($1, $2)",
                m,
                mood,
            )
            .execute(&mut *tx)
            .await
            .expect_err("readonly insert denied");
            assert!(is_rls_denied(&err), "expected 42501, got {err:?}");
        }

        // ---- manifestation_tags ----

        async fn seed_tag(ingestion: &PgPool, name: &str) -> Uuid {
            sqlx::query_scalar!("INSERT INTO tags (name) VALUES ($1) RETURNING id", name)
                .fetch_one(ingestion)
                .await
                .expect("insert tag")
        }

        async fn link_tag(ingestion: &PgPool, manifestation_id: Uuid, tag_id: Uuid) {
            sqlx::query!(
                "INSERT INTO manifestation_tags (manifestation_id, tag_id) VALUES ($1, $2)",
                manifestation_id,
                tag_id,
            )
            .execute(ingestion)
            .await
            .expect("insert manifestation_tags");
        }

        #[sqlx::test(migrations = "./migrations")]
        async fn tags_child_reads_only_shelf_visible(pool: PgPool) {
            let ingestion = ingestion_pool_for(&pool).await;
            let app = app_pool_for(&pool).await;
            let (_vw, visible_m) = insert_work_and_manifestation(&ingestion, "tag-vis").await;
            let (_hw, hidden_m) = insert_work_and_manifestation(&ingestion, "tag-hid").await;
            let tag = seed_tag(&ingestion, "tag-child-scope").await;
            link_tag(&ingestion, visible_m, tag).await;
            link_tag(&ingestion, hidden_m, tag).await;

            let (child_id, _) = create_child_user_and_basic_auth(&app, "tag-child").await;
            let shelf = create_shelf(&app, child_id, "kids").await;
            add_to_shelf(&app, shelf, visible_m).await;

            let mut tx = acquire_with_rls(&app, child_id).await.expect("rls tx");
            let rows: Vec<Uuid> = sqlx::query_scalar!(
                "SELECT manifestation_id FROM manifestation_tags ORDER BY manifestation_id",
            )
            .fetch_all(&mut *tx)
            .await
            .expect("child select");
            assert_eq!(
                rows,
                vec![visible_m],
                "child sees only the shelf-visible manifestation's tag link"
            );
        }

        #[sqlx::test(migrations = "./migrations")]
        async fn tags_readonly_reads_scoped_and_lacks_write_grant(pool: PgPool) {
            let ingestion = ingestion_pool_for(&pool).await;
            let app = app_pool_for(&pool).await;
            let readonly = readonly_pool_for(&pool).await;
            let (_w, m) = insert_work_and_manifestation(&ingestion, "tag-ro").await;
            let tag = seed_tag(&ingestion, "tag-ro-scope").await;
            link_tag(&ingestion, m, tag).await;

            let (adult_id, _) = create_adult_and_basic_auth(&app, "tag-ro").await;
            let mut tx = acquire_with_rls(&readonly, adult_id).await.expect("rls tx");
            let count: i64 = sqlx::query_scalar!(
                "SELECT count(*) AS \"c!\" FROM manifestation_tags WHERE manifestation_id = $1",
                m,
            )
            .fetch_one(&mut *tx)
            .await
            .expect("readonly select");
            assert_eq!(count, 1, "readonly reads the same scoped rows");

            let err = sqlx::query!(
                "INSERT INTO manifestation_tags (manifestation_id, tag_id) VALUES ($1, $2)",
                m,
                tag,
            )
            .execute(&mut *tx)
            .await
            .expect_err("readonly insert denied");
            assert!(is_rls_denied(&err), "expected 42501, got {err:?}");
        }
    }
}
