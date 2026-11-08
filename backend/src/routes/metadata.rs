//! Metadata review endpoints.
//!
//! All routes require an authenticated non-child user.  Write paths open a
//! transaction, `SELECT ... FOR UPDATE` on the owning entity, apply the change,
//! and commit.

pub(crate) mod undo;

use std::collections::BTreeMap;

use axum::extract::State;
use axum::http::header::ETAG;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{Postgres, Transaction};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::auth::middleware::CurrentUser;
use crate::auth::scope::Scope;
use crate::db;
use crate::error::AppError;
use crate::extract::{ApiJson, ApiPath};
use crate::models::content_rating::ContentRating;
use crate::models::external_identifier::{
    IdentifierLevel, delete_manifestation_identifier, delete_work_identifier,
    upsert_manifestation_identifier, upsert_work_identifier,
};
use crate::models::work;
use crate::routes::etag::{MetadataRevision, if_match_mismatch, parse_if_match};
use crate::routes::library::{
    load_genres_for_manifestations, load_moods_for_manifestations, load_tags_for_manifestations,
};
use crate::services::enrichment::field_lock::{self, EntityType};
use crate::services::enrichment::value_hash;
use crate::services::metadata::external_id;
use crate::state::AppState;

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
        .routes(routes!(undo::undo_metadata))
        .routes(routes!(undo::resolve_metadata))
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

// ── accept / reject / revert / lock ────────────────────────────────────────

/// Body for accept / reject: the targeted `metadata_versions` row.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
struct VersionPayload {
    /// Metadata version row to act on.
    #[schema(max_length = 45)]
    version_id: Uuid,
}

/// Body for revert.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
struct RevertPayload {
    /// Canonical field to revert (e.g. `title`).
    #[schema(max_length = 38)]
    field_name: String,
    /// Version to restore; `null` clears the canonical pointer AND the
    /// canonical column.
    #[schema(max_length = 45)]
    version_id: Option<Uuid>,
}

/// Body for lock / unlock.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
struct LockPayload {
    /// Field to lock or unlock (e.g. `title`).
    field_name: String,
    /// Entity the lock applies to: `work` or `manifestation`.
    #[schema(schema_with = lock_entity_type_schema)]
    entity_type: String,
}

fn lock_entity_type_schema() -> utoipa::openapi::schema::Object {
    utoipa::openapi::schema::ObjectBuilder::new()
        .schema_type(utoipa::openapi::schema::Type::String)
        .description(Some(
            "Entity the lock applies to: `work` or `manifestation`.",
        ))
        .enum_values(Some(["work", "manifestation"]))
        .examples(["manifestation"])
        .build()
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
    current_user.require_scope(Scope::Write)?;
    current_user.require_not_child()?;

    let mut tx = db::acquire_with_rls_read_committed(&state.pool, current_user.user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    lock_version_review(&mut tx, payload.version_id).await?;

    let row = sqlx::query!(
        "SELECT mv.id, mv.field_name, \
                mv.new_value AS \"new_value!\", \
                m.work_id \
         FROM metadata_versions mv \
         JOIN manifestations m ON m.id = mv.manifestation_id \
         JOIN works w ON w.id = m.work_id \
         WHERE mv.id = $1 AND mv.manifestation_id = $2 \
           AND mv.status = 'pending'::metadata_review_status \
         FOR UPDATE OF m, w",
        payload.version_id,
        manifestation_id,
    )
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    let row = row.ok_or(AppError::NotFound)?;
    let version_id = row.id;
    let field_name = row.field_name;
    let new_value = row.new_value;
    let work_id = row.work_id;

    require_unlocked_fields(
        &mut tx,
        manifestation_id,
        current_user.user_id,
        std::slice::from_ref(&field_name),
    )
    .await?;

    // Accept promotes the version to canonical; the caller has no use for
    // the prior pointer (that's the revert/undo surface's concern).
    apply_version(
        &mut tx,
        &field_name,
        &new_value,
        version_id,
        manifestation_id,
        work_id,
    )
    .await?;

    if field_name == "isbn_10" || field_name == "isbn_13" {
        work::rematch_on_isbn_change(&mut tx, manifestation_id)
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
    }

    tx.commit()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    Ok(StatusCode::OK)
}

const VERSION_REVIEW_LOCK_NAMESPACE: i32 = 0x4d45_5441;

async fn lock_version_review(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    version_id: Uuid,
) -> Result<(), AppError> {
    let key = version_id.to_string();
    sqlx::query!(
        "SELECT pg_advisory_xact_lock($1, hashtext($2))",
        VERSION_REVIEW_LOCK_NAMESPACE,
        key,
    )
    .execute(&mut **tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    Ok(())
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
    current_user.require_scope(Scope::Write)?;
    current_user.require_not_child()?;

    let mut tx = db::acquire_with_rls_read_committed(&state.pool, current_user.user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    lock_version_review(&mut tx, payload.version_id).await?;

    let work_id: Option<Uuid> = sqlx::query_scalar!(
        "SELECT m.work_id FROM manifestations m \
         JOIN works w ON w.id = m.work_id \
         WHERE m.id = $1 FOR UPDATE OF m, w",
        manifestation_id,
    )
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    work_id.ok_or(AppError::NotFound)?;

    let rows = sqlx::query!(
        "UPDATE metadata_versions \
         SET status = 'rejected', resolved_by = $1, resolved_at = now() \
         WHERE id = $2 AND manifestation_id = $3",
        current_user.user_id,
        payload.version_id,
        manifestation_id,
    )
    .execute(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    if rows.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }

    tx.commit()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
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
    current_user.require_scope(Scope::Write)?;
    current_user.require_not_child()?;

    let mut tx = db::acquire_with_rls_read_committed(&state.pool, current_user.user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    // Lock both the manifestation row and its work row so concurrent
    // accept/revert calls on sibling manifestations of the same work
    // serialise on `works.{title,description,language}` updates.
    let work_id: Option<Uuid> = sqlx::query_scalar!(
        "SELECT m.work_id FROM manifestations m \
         JOIN works w ON w.id = m.work_id \
         WHERE m.id = $1 FOR UPDATE OF m, w",
        manifestation_id,
    )
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    let work_id = work_id.ok_or(AppError::NotFound)?;

    require_unlocked_fields(
        &mut tx,
        manifestation_id,
        current_user.user_id,
        std::slice::from_ref(&payload.field_name),
    )
    .await?;

    match payload.version_id {
        Some(vid) => {
            // Sibling manifestations of the same work may legitimately own
            // the version being restored for work-scoped fields; see
            // `is_work_scoped_field`.
            let is_work_scoped = is_work_scoped_field(&payload.field_name);
            let new_value: Option<Value> = sqlx::query_scalar!(
                "SELECT mv.new_value AS \"new_value!\" \
                 FROM metadata_versions mv \
                 JOIN manifestations vm ON vm.id = mv.manifestation_id \
                 WHERE mv.id = $1 \
                   AND (vm.id = $2 OR ($3::bool AND vm.work_id = $4))",
                vid,
                manifestation_id,
                is_work_scoped,
                work_id,
            )
            .fetch_optional(&mut *tx)
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
            let new_value = new_value.ok_or(AppError::NotFound)?;
            // Revert-to-version restores canonical state; the review UI
            // does not surface the prior pointer from this endpoint today.
            apply_version(
                &mut tx,
                &payload.field_name,
                &new_value,
                vid,
                manifestation_id,
                work_id,
            )
            .await?;
        }
        None => {
            clear_field(&mut tx, &payload.field_name, manifestation_id, work_id).await?;
        }
    }

    if matches!(payload.field_name.as_str(), "isbn_10" | "isbn_13") {
        work::rematch_on_isbn_change(&mut tx, manifestation_id)
            .await
            .map_err(|error| AppError::Internal(error.into()))?;
    }

    tx.commit()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
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
    current_user.require_scope(Scope::Write)?;
    current_user.require_not_child()?;
    let entity = parse_entity(&payload.entity_type)?;
    let mut tx = db::acquire_with_rls_read_committed(&state.pool, current_user.user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    lock_visible_manifestation(&mut tx, manifestation_id).await?;
    field_lock::lock_tx(
        &mut tx,
        manifestation_id,
        entity,
        &payload.field_name,
        current_user.user_id,
    )
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    tx.commit()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
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
    current_user.require_scope(Scope::Write)?;
    current_user.require_not_child()?;
    let entity = parse_entity(&payload.entity_type)?;
    let mut tx = db::acquire_with_rls_read_committed(&state.pool, current_user.user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    lock_visible_manifestation(&mut tx, manifestation_id).await?;
    let removed = field_lock::unlock_tx(&mut tx, manifestation_id, entity, &payload.field_name)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    if !removed {
        return Err(AppError::NotFound);
    }
    tx.commit()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    Ok(StatusCode::OK)
}

/// Resolve a manifestation through the caller's RLS context and hold its row
/// lock while a field lock mutation runs.
async fn lock_visible_manifestation(
    tx: &mut Transaction<'_, Postgres>,
    manifestation_id: Uuid,
) -> Result<(), AppError> {
    let visible = sqlx::query_scalar!(
        "SELECT id FROM manifestations WHERE id = $1 FOR UPDATE",
        manifestation_id,
    )
    .fetch_optional(&mut **tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    visible.ok_or(AppError::NotFound).map(|_| ())
}

/// Whether `field_name`'s canonical pointer lives on the shared `works`
/// row rather than on a specific `manifestations` row.
///
/// `title`/`subtitle`/`description`/`language` and every `contributors.*`
/// role write through to `works` (see `apply_version`), so a version
/// journaled under any manifestation of that work is a valid revert
/// target. `genres`/`moods`/`tags` are per-manifestation vocabulary
/// junctions despite also hanging off the work's editions, so they stay
/// out of this set and use the strict same-manifestation match.
/// `identifiers.work.*` values live in `work_external_identifiers`, keyed
/// by the shared work, so they are work-scoped too;
/// `identifiers.manifestation.*` values are per-edition and are not.
fn is_work_scoped_field(field_name: &str) -> bool {
    matches!(
        field_name,
        "title" | "subtitle" | "description" | "language"
    ) || field_name.starts_with("contributors.")
        || field_name.starts_with("identifiers.work.")
}

async fn require_unlocked_fields(
    tx: &mut Transaction<'_, Postgres>,
    manifestation_id: Uuid,
    user_id: Uuid,
    fields: &[String],
) -> Result<(), AppError> {
    // THREAT: The caller holds the edition row lock so lock changes cannot race this decision.
    let (work_fields, manifestation_fields): (Vec<_>, Vec<_>) = fields
        .iter()
        .cloned()
        .partition(|field| is_work_scoped_field(field));
    let locked =
        field_lock::locked_fields_tx(tx, manifestation_id, &work_fields, &manifestation_fields)
            .await
            .map_err(|error| AppError::Internal(error.into()))?;
    if locked.is_empty() {
        return Ok(());
    }
    tracing::info!(%user_id, %manifestation_id, "metadata write refused by field locks");
    Err(AppError::FieldLocked(locked))
}

/// Split a canonical `identifiers.<level>.<scheme>` field name into its level
/// and scheme, rejecting a malformed level segment or a scheme unknown at
/// that level. Shared by the PATCH map handler, `apply_version`, and
/// `clear_field` so every path addresses the registry identically.
fn parse_identifier_field(field: &str) -> Result<(IdentifierLevel, &str), AppError> {
    let rest = field
        .strip_prefix("identifiers.")
        .ok_or_else(|| AppError::Validation(format!("'{field}' is not an identifier field")))?;
    let (level_segment, scheme) = rest.split_once('.').ok_or_else(|| {
        AppError::Validation(format!(
            "identifier field '{field}' must be identifiers.<level>.<scheme>"
        ))
    })?;
    let level = IdentifierLevel::from_segment(level_segment).ok_or_else(|| {
        AppError::Validation(format!(
            "identifier level must be 'work' or 'manifestation', got '{level_segment}'"
        ))
    })?;
    external_id::validate_scheme_level(level, scheme).map_err(AppError::Validation)?;
    Ok((level, scheme))
}

fn parse_entity(s: &str) -> Result<EntityType, AppError> {
    match s {
        "work" => Ok(EntityType::Work),
        "manifestation" => Ok(EntityType::Manifestation),
        other => Err(AppError::Validation(format!(
            "invalid entity_type '{other}'; expected 'work' or 'manifestation'"
        ))),
    }
}

fn normalise_isbn_field(field: &str, value: &Value) -> Result<String, AppError> {
    let invalid = || AppError::FieldValidation {
        detail: format!("invalid {field}"),
        errors: BTreeMap::from([(field.into(), format!("invalid {field}"))]),
    };
    let raw = value.as_str().ok_or_else(invalid)?;
    let parsed = if field == "isbn_10" {
        raw.parse::<isbn::Isbn10>().map(|value| value.to_string())
    } else {
        raw.parse::<isbn::Isbn13>().and_then(|value| {
            let compact = value.to_string();
            if compact.starts_with("978") || compact.starts_with("979") {
                Ok(compact)
            } else {
                Err(isbn::IsbnError::InvalidGroup)
            }
        })
    };
    parsed.map_err(|_| invalid())
}

/// Apply a specific version to its canonical column + pointer.
/// Reused by `/accept` and `/revert`.
//
// Field-dispatch with one `sqlx::query!` per supported field; macro-form
// expansion pushed this just past clippy's 100-line threshold.
#[expect(
    clippy::too_many_lines,
    reason = "field dispatch with one sqlx::query! per supported field; macro-form expansion pushes this past the 100-line threshold"
)]
async fn apply_version(
    tx: &mut Transaction<'_, Postgres>,
    field: &str,
    value: &Value,
    version_id: Uuid,
    manifestation_id: Uuid,
    work_id: Uuid,
) -> Result<(), AppError> {
    // Refuse to promote a null-valued audit row to canonical. Manual
    // clears (PATCH `{field}: null`) write a `'null'::jsonb` audit row
    // with `status = 'pending'` so operators can revert to them; the
    // Versions tab filters those rows out, but `accept_manifestation`
    // and `revert_manifestation` accept any pending row by id. Without
    // this guard, `value.as_str()` returns `None`, `value.to_string()`
    // yields the literal `"null"`, and non-date canonical columns get
    // corrupted with the string `"null"`. Callers wanting to clear a
    // field must go through `clear_field` (PATCH null or revert with
    // `version_id = null`), not /accept.
    if value.is_null() {
        return Err(AppError::Validation(
            "cannot accept a null-value audit row; use revert/clear instead".into(),
        ));
    }
    let str_val = if matches!(field, "isbn_10" | "isbn_13") {
        normalise_isbn_field(field, value)?
    } else {
        value
            .as_str()
            .map_or_else(|| value.to_string(), str::to_owned)
    };
    let update = match field {
        "title" => Some(sqlx::query!(
            "UPDATE works \
             SET title = $1, sort_title = lower($1), title_version_id = $2 \
             WHERE id = $3",
            str_val,
            version_id,
            work_id,
        )),
        "description" => Some(sqlx::query!(
            "UPDATE works SET description = $1, description_version_id = $2 \
             WHERE id = $3",
            str_val,
            version_id,
            work_id,
        )),
        "language" => Some(sqlx::query!(
            "UPDATE works SET language = $1, language_version_id = $2 \
             WHERE id = $3",
            str_val,
            version_id,
            work_id,
        )),
        "subtitle" => Some(sqlx::query!(
            "UPDATE works SET subtitle = $1, subtitle_version_id = $2 \
             WHERE id = $3",
            str_val,
            version_id,
            work_id,
        )),
        "pages" => {
            let pages = value
                .as_i64()
                .and_then(|n| i32::try_from(n).ok())
                .filter(|n| *n > 0)
                .ok_or_else(|| AppError::Validation("pages must be a positive integer".into()))?;
            Some(sqlx::query!(
                "UPDATE manifestations SET pages = $1, pages_version_id = $2 \
                 WHERE id = $3",
                pages,
                version_id,
                manifestation_id,
            ))
        }
        "publisher" => Some(sqlx::query!(
            "UPDATE manifestations \
             SET publisher = $1, publisher_version_id = $2 \
             WHERE id = $3",
            str_val,
            version_id,
            manifestation_id,
        )),
        "isbn_10" => Some(sqlx::query!(
            "UPDATE manifestations \
             SET isbn_10 = $1, isbn_10_version_id = $2 \
             WHERE id = $3",
            str_val,
            version_id,
            manifestation_id,
        )),
        "isbn_13" => Some(sqlx::query!(
            "UPDATE manifestations \
             SET isbn_13 = $1, isbn_13_version_id = $2 \
             WHERE id = $3",
            str_val,
            version_id,
            manifestation_id,
        )),
        "pub_date" => {
            let date = crate::services::metadata::publication_date::normalise(&str_val)
                .and_then(|value| value.ok_or(crate::services::metadata::publication_date::PublicationDateError::Malformed))
                .map_err(|error| {
                    let message = format!("invalid publication date: {error}");
                    AppError::FieldValidation {
                        detail: message.clone(),
                        errors: std::collections::BTreeMap::from([("pub_date".into(), message)]),
                    }
                })?;
            Some(sqlx::query!(
                "UPDATE manifestations \
                 SET pub_date = $1, pub_date_version_id = $2 \
                 WHERE id = $3",
                date,
                version_id,
                manifestation_id,
            ))
        }
        "content_rating" => {
            let rating: ContentRating = serde_json::from_value(value.clone()).map_err(|_| {
                AppError::Validation(
                    "content_rating must be one of everyone, teen, mature, adult, explicit".into(),
                )
            })?;
            Some(sqlx::query!(
                "UPDATE manifestations \
                 SET content_rating = $1, content_rating_version_id = $2 \
                 WHERE id = $3",
                rating as ContentRating,
                version_id,
                manifestation_id,
            ))
        }
        "genres" | "moods" | "tags" => {
            let vocab = VocabularyField::from_field_name(field).ok_or_else(|| {
                AppError::Validation(format!("unsupported vocabulary field '{field}'"))
            })?;
            let names: Vec<String> = serde_json::from_value(value.clone()).map_err(|_| {
                AppError::Validation(format!("{field} must be an array of strings"))
            })?;
            let trimmed = validated_vocabulary_terms(field, &names)?;
            delete_vocabulary_rows(tx, manifestation_id, vocab).await?;
            insert_vocabulary_rows(tx, manifestation_id, vocab, &trimmed, version_id).await?;
            None
        }
        _ if field.starts_with("contributors.") => {
            let role = &field["contributors.".len()..];
            if !matches!(role, "author" | "editor" | "translator") {
                return Err(AppError::Validation(format!(
                    "unsupported contributor role '{role}'"
                )));
            }
            let names: Vec<String> = serde_json::from_value(value.clone()).map_err(|_| {
                AppError::Validation(format!("{field} must be an array of strings"))
            })?;
            let trimmed = validated_role_names(role, &names)?;
            delete_role_rows(tx, work_id, role).await?;
            insert_role_rows(tx, work_id, role, &trimmed, version_id).await?;
            if role == "author" {
                let post_author_count: i64 = sqlx::query_scalar!(
                    "SELECT COUNT(*) AS \"count!\" FROM work_authors \
                     WHERE work_id = $1 AND role = 'author'",
                    work_id,
                )
                .fetch_one(&mut **tx)
                .await
                .map_err(|e| AppError::Internal(e.into()))?;
                if post_author_count == 0 {
                    return Err(AppError::Validation(
                        "a work must retain at least one author".into(),
                    ));
                }
                work::refresh_first_author_sort(tx, work_id)
                    .await
                    .map_err(|e| AppError::Internal(e.into()))?;
            }
            None
        }
        _ if field.starts_with("identifiers.") => {
            let (level, scheme) = parse_identifier_field(field)?;
            let raw = value
                .as_str()
                .ok_or_else(|| AppError::Validation(format!("{field} must be a string")))?;
            // Re-run the typed parser here (not only at the PATCH boundary)
            // so accept/revert of a journaled value cannot promote a
            // malformed identifier into the registry.
            let canonical =
                external_id::parse_external_id(level, scheme, raw).map_err(AppError::Validation)?;
            match level {
                IdentifierLevel::Work => {
                    upsert_work_identifier(
                        &mut **tx,
                        work_id,
                        scheme,
                        &canonical,
                        Some(version_id),
                    )
                    .await
                    .map_err(|e| AppError::Internal(e.into()))?;
                }
                IdentifierLevel::Manifestation => {
                    upsert_manifestation_identifier(
                        &mut **tx,
                        manifestation_id,
                        scheme,
                        &canonical,
                        Some(version_id),
                    )
                    .await
                    .map_err(|e| AppError::Internal(e.into()))?;
                }
            }
            None
        }
        other => {
            return Err(AppError::Validation(format!(
                "unsupported auto-apply field '{other}' (list/complex fields must be accepted via their dedicated routes)"
            )));
        }
    };
    if let Some(update) = update {
        update
            .execute(&mut **tx)
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
    }
    // External identifiers are never written back to the source file, so an
    // identifier apply (manual set, accept, or revert) must not enqueue an
    // OPF writeback job.
    if !field.starts_with("identifiers.") {
        enqueue_writeback(tx, manifestation_id, field).await?;
    }
    Ok(())
}

/// Insert a writeback job in the caller's tx.  Shared by `apply_version`
/// (accept + revert-to-version) and `clear_field` (revert-to-null) so the
/// pointer mutation and writeback enqueue commit together.
async fn enqueue_writeback(
    tx: &mut Transaction<'_, Postgres>,
    manifestation_id: Uuid,
    field: &str,
) -> Result<(), AppError> {
    let reason = if field == "cover" || field == "cover_url" {
        "cover"
    } else {
        "metadata"
    };
    sqlx::query!(
        "INSERT INTO writeback_jobs (manifestation_id, reason, field_name) VALUES ($1, $2, $3)",
        manifestation_id,
        reason,
        field,
    )
    .execute(&mut **tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    Ok(())
}

/// Clear a field's canonical value and pointer. Reused by `/revert` (with
/// `version_id = null`) and the manual PATCH clear path.
///
/// Returns the pointer that was canonical before the clear (`None` when the
/// field was already unset, or when the field kind has no single-pointer
/// concept).
#[expect(
    clippy::too_many_lines,
    reason = "field dispatch with one sqlx::query! per supported field; macro-form expansion pushes this past the 100-line threshold"
)]
async fn clear_field(
    tx: &mut Transaction<'_, Postgres>,
    field: &str,
    manifestation_id: Uuid,
    work_id: Uuid,
) -> Result<(), AppError> {
    let update = match field {
        "title" => {
            return Err(AppError::Validation(
                "cannot clear title — revert to a specific version instead".into(),
            ));
        }
        "description" => Some(sqlx::query!(
            "UPDATE works SET description = NULL, description_version_id = NULL \
             WHERE id = $1",
            work_id,
        )),
        "language" => Some(sqlx::query!(
            "UPDATE works SET language = NULL, language_version_id = NULL \
             WHERE id = $1",
            work_id,
        )),
        "subtitle" => Some(sqlx::query!(
            "UPDATE works SET subtitle = NULL, subtitle_version_id = NULL \
             WHERE id = $1",
            work_id,
        )),
        "pages" => Some(sqlx::query!(
            "UPDATE manifestations SET pages = NULL, pages_version_id = NULL \
             WHERE id = $1",
            manifestation_id,
        )),
        "publisher" => Some(sqlx::query!(
            "UPDATE manifestations \
             SET publisher = NULL, publisher_version_id = NULL \
             WHERE id = $1",
            manifestation_id,
        )),
        "pub_date" => Some(sqlx::query!(
            "UPDATE manifestations \
             SET pub_date = NULL, pub_date_version_id = NULL \
             WHERE id = $1",
            manifestation_id,
        )),
        "isbn_10" => Some(sqlx::query!(
            "UPDATE manifestations \
             SET isbn_10 = NULL, isbn_10_version_id = NULL \
             WHERE id = $1",
            manifestation_id,
        )),
        "isbn_13" => Some(sqlx::query!(
            "UPDATE manifestations \
             SET isbn_13 = NULL, isbn_13_version_id = NULL \
             WHERE id = $1",
            manifestation_id,
        )),
        "content_rating" => Some(sqlx::query!(
            "UPDATE manifestations \
             SET content_rating = NULL, content_rating_version_id = NULL \
             WHERE id = $1",
            manifestation_id,
        )),
        "genres" | "moods" | "tags" => {
            let vocab = VocabularyField::from_field_name(field).ok_or_else(|| {
                AppError::Validation(format!("unsupported vocabulary field '{field}'"))
            })?;
            delete_vocabulary_rows(tx, manifestation_id, vocab).await?;
            None
        }
        _ if field.starts_with("contributors.") => {
            let role = &field["contributors.".len()..];
            match role {
                // Mirrors the title arm above: clearing the sole author role
                // would leave the work with zero authors, which the schema's
                // consumers (sort, display) do not expect.
                "author" => {
                    return Err(AppError::Validation(
                        "cannot clear contributors.author: a work must retain at least one author"
                            .into(),
                    ));
                }
                "editor" | "translator" => {
                    delete_role_rows(tx, work_id, role).await?;
                }
                other => {
                    return Err(AppError::Validation(format!(
                        "unsupported contributor role '{other}'"
                    )));
                }
            }
            None
        }
        _ if field.starts_with("identifiers.") => {
            let (level, scheme) = parse_identifier_field(field)?;
            match level {
                IdentifierLevel::Work => {
                    delete_work_identifier(&mut **tx, work_id, scheme)
                        .await
                        .map_err(|e| AppError::Internal(e.into()))?;
                }
                IdentifierLevel::Manifestation => {
                    delete_manifestation_identifier(&mut **tx, manifestation_id, scheme)
                        .await
                        .map_err(|e| AppError::Internal(e.into()))?;
                }
            }
            None
        }
        other => {
            return Err(AppError::Validation(format!("unsupported field '{other}'")));
        }
    };
    if let Some(update) = update {
        update
            .execute(&mut **tx)
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
    }
    // Identifier clears mirror identifier applies: no OPF writeback.
    if !field.starts_with("identifiers.") {
        enqueue_writeback(tx, manifestation_id, field).await?;
    }
    Ok(())
}

/// Maximum length (in `char`s) for a manually-entered contributor name.
const MAX_CONTRIBUTOR_NAME_CHARS: usize = 500;

/// Maximum number of names accepted per contributor role in one patch.
/// Bounds the per-name insert loop that runs under the handler's row lock.
const MAX_CONTRIBUTORS_PER_ROLE: usize = 100;

/// Validate one role's submitted names: bounded count, trimmed, non-empty,
/// length-capped, duplicate-free. Returns the trimmed list.
fn validated_role_names(role: &str, names: &[String]) -> Result<Vec<String>, AppError> {
    if names.len() > MAX_CONTRIBUTORS_PER_ROLE {
        return Err(AppError::Validation(format!(
            "{role} list exceeds {MAX_CONTRIBUTORS_PER_ROLE} names"
        )));
    }
    let mut trimmed: Vec<String> = Vec::with_capacity(names.len());
    for raw in names {
        let t = raw.trim();
        if t.is_empty() {
            return Err(AppError::Validation(format!(
                "{role} name must not be empty"
            )));
        }
        if t.chars().count() > MAX_CONTRIBUTOR_NAME_CHARS {
            return Err(AppError::Validation(format!(
                "{role} name exceeds {MAX_CONTRIBUTOR_NAME_CHARS} characters"
            )));
        }
        trimmed.push(t.to_string());
    }
    let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for t in &trimmed {
        if !seen.insert(t.as_str()) {
            return Err(AppError::Validation(format!("duplicate {role} name '{t}'")));
        }
    }
    Ok(trimmed)
}

async fn delete_role_rows(
    tx: &mut Transaction<'_, Postgres>,
    work_id: Uuid,
    role: &str,
) -> Result<(), AppError> {
    sqlx::query!(
        "DELETE FROM work_authors WHERE work_id = $1 AND role = ($2::text)::author_role",
        work_id,
        role,
    )
    .execute(&mut **tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    Ok(())
}

async fn insert_role_rows(
    tx: &mut Transaction<'_, Postgres>,
    work_id: Uuid,
    role: &str,
    names: &[String],
    version_id: Uuid,
) -> Result<(), AppError> {
    for (i, name) in names.iter().enumerate() {
        let sort_name = crate::services::metadata::extractor::generate_sort_name(name);
        let author_id = work::find_or_create_author(tx, name, &sort_name)
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
        let position = i32::try_from(i).unwrap_or(i32::MAX);
        sqlx::query!(
            "INSERT INTO work_authors (work_id, author_id, role, position, source_version_id) \
             VALUES ($1, $2, ($3::text)::author_role, $4, $5) \
             ON CONFLICT (work_id, author_id, role) DO NOTHING",
            work_id,
            author_id,
            role,
            position,
            version_id,
        )
        .execute(&mut **tx)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    }
    Ok(())
}

/// Apply a `contributors` merge-patch: per role, replace/clear/reorder
/// `work_authors` rows and journal the change under `contributors.<role>`.
///
/// `patch = None` means the top-level `"contributors": null` was sent —
/// pure RFC 7396 member removal, treated as clearing all three editable
/// roles. An empty array is equivalent to `null` for a given role.
///
/// # Errors
/// - [`AppError::Validation`] on an empty/duplicate/over-length name, or
///   when clearing/reassigning authors would leave a previously-authored
///   work with zero authors (editor/translator-only edits on an
///   authorless stub are legal; the invariant only fires when the
///   `author` key was touched).
/// - [`AppError::Internal`] on database errors.
async fn apply_contributors_patch(
    tx: &mut Transaction<'_, Postgres>,
    manifestation_id: Uuid,
    work_id: Uuid,
    user_id: Uuid,
    patch: Option<ContributorsPatch>,
) -> Result<(), AppError> {
    let (author, editor, translator) = match patch {
        None => (Some(None), Some(None), Some(None)),
        Some(p) => (p.author, p.editor, p.translator),
    };

    // Captured before any mutation so the last-author guard below compares
    // against the pre-patch state, not a role this same call already cleared.
    let pre_author_count: i64 = sqlx::query_scalar!(
        "SELECT COUNT(*) AS \"count!\" FROM work_authors WHERE work_id = $1 AND role = 'author'",
        work_id,
    )
    .fetch_one(&mut **tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    let mut author_touched = false;
    let mut touched = false;

    for (role, maybe_names) in [
        ("author", author),
        ("editor", editor),
        ("translator", translator),
    ] {
        let Some(names) = maybe_names else {
            continue; // absent from the patch: this role is untouched.
        };
        touched = true;
        if role == "author" {
            author_touched = true;
        }
        // Empty array ≡ null: both clear the role.
        let names = names.filter(|v| !v.is_empty());

        let key = format!("contributors.{role}");
        match names {
            None => {
                insert_manual_version(tx, manifestation_id, user_id, &key, &Value::Null).await?;
                delete_role_rows(tx, work_id, role).await?;
            }
            Some(names) => {
                let trimmed = validated_role_names(role, &names)?;
                let json =
                    serde_json::to_value(&trimmed).map_err(|e| AppError::Internal(e.into()))?;
                let version_id =
                    insert_manual_version(tx, manifestation_id, user_id, &key, &json).await?;
                delete_role_rows(tx, work_id, role).await?;
                insert_role_rows(tx, work_id, role, &trimmed, version_id).await?;
            }
        }
    }

    if author_touched {
        let post_author_count: i64 = sqlx::query_scalar!(
            "SELECT COUNT(*) AS \"count!\" FROM work_authors WHERE work_id = $1 AND role = 'author'",
            work_id,
        )
        .fetch_one(&mut **tx)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
        if post_author_count == 0 && pre_author_count > 0 {
            return Err(AppError::Validation(
                "a work must retain at least one author".into(),
            ));
        }
    }

    // The denormalized sort column derives from the author role only, so
    // editor/translator-only patches skip the refresh (and its updated_at
    // trigger bump).
    if author_touched {
        work::refresh_first_author_sort(tx, work_id)
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
    }

    // The library path renders from the primary author, so a contributor
    // change must enqueue writeback like every scalar field in this handler.
    if touched {
        enqueue_writeback(tx, manifestation_id, "contributors").await?;
    }
    Ok(())
}

// ── vocabulary junction fields (genres / moods / tags) ────────────────────

/// Maximum length (in `char`s) for a manually-entered vocabulary term.
const MAX_VOCABULARY_TERM_CHARS: usize = 100;

/// Maximum number of terms accepted per vocabulary field in one patch.
const MAX_VOCABULARY_TERMS_PER_FIELD: usize = 50;

/// Vocabulary junction targets editable from the manual PATCH surface and
/// the accept/revert machinery. Each maps to a vocabulary table plus a
/// junction table mirroring `manifestation_tags`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VocabularyField {
    Genres,
    Moods,
    Tags,
}

impl VocabularyField {
    const fn field_name(self) -> &'static str {
        match self {
            Self::Genres => "genres",
            Self::Moods => "moods",
            Self::Tags => "tags",
        }
    }

    fn from_field_name(field: &str) -> Option<Self> {
        match field {
            "genres" => Some(Self::Genres),
            "moods" => Some(Self::Moods),
            "tags" => Some(Self::Tags),
            _ => None,
        }
    }
}

/// Validate one vocabulary field's submitted terms: bounded count, trimmed,
/// non-empty, length-capped, duplicate-free. Duplicates are checked
/// case-insensitively because the vocabulary tables are unique on
/// `lower(name)`. Returns the trimmed list in submission order.
fn validated_vocabulary_terms(field: &str, names: &[String]) -> Result<Vec<String>, AppError> {
    if names.len() > MAX_VOCABULARY_TERMS_PER_FIELD {
        return Err(AppError::Validation(format!(
            "{field} list exceeds {MAX_VOCABULARY_TERMS_PER_FIELD} terms"
        )));
    }
    let mut trimmed: Vec<String> = Vec::with_capacity(names.len());
    for raw in names {
        let t = raw.trim();
        if t.is_empty() {
            return Err(AppError::Validation(format!(
                "{field} term must not be empty"
            )));
        }
        if t.chars().count() > MAX_VOCABULARY_TERM_CHARS {
            return Err(AppError::Validation(format!(
                "{field} term exceeds {MAX_VOCABULARY_TERM_CHARS} characters"
            )));
        }
        trimmed.push(t.to_string());
    }
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for t in &trimmed {
        if !seen.insert(t.to_lowercase()) {
            return Err(AppError::Validation(format!(
                "duplicate {field} term '{t}'"
            )));
        }
    }
    Ok(trimmed)
}

async fn delete_vocabulary_rows(
    tx: &mut Transaction<'_, Postgres>,
    manifestation_id: Uuid,
    field: VocabularyField,
) -> Result<(), AppError> {
    match field {
        VocabularyField::Genres => {
            sqlx::query!(
                "DELETE FROM manifestation_genres WHERE manifestation_id = $1",
                manifestation_id,
            )
            .execute(&mut **tx)
            .await
        }
        VocabularyField::Moods => {
            sqlx::query!(
                "DELETE FROM manifestation_moods WHERE manifestation_id = $1",
                manifestation_id,
            )
            .execute(&mut **tx)
            .await
        }
        VocabularyField::Tags => {
            sqlx::query!(
                "DELETE FROM manifestation_tags WHERE manifestation_id = $1",
                manifestation_id,
            )
            .execute(&mut **tx)
            .await
        }
    }
    .map_err(|e| AppError::Internal(e.into()))?;
    Ok(())
}

/// Find-or-create every term and link it to the manifestation, carrying the
/// journal pointer onto each junction row. Set-based: one statement per call.
/// The `ON CONFLICT ((lower(name))) DO UPDATE` leg returns ids for existing
/// terms too, preserving their stored display spelling.
///
/// The `DISTINCT ON (lower(name)) .. ORDER BY lower(name)` source does two
/// jobs: it guards the single-statement upsert against case collisions the
/// Rust-side validation missed (Postgres `lower()` can fold pairs that
/// `str::to_lowercase` keeps distinct, and upserting the same row twice in
/// one statement is an error), and it fixes the row-lock acquisition order
/// so concurrent patches upserting overlapping terms cannot deadlock.
async fn insert_vocabulary_rows(
    tx: &mut Transaction<'_, Postgres>,
    manifestation_id: Uuid,
    field: VocabularyField,
    names: &[String],
    version_id: Uuid,
) -> Result<(), AppError> {
    match field {
        VocabularyField::Genres => {
            sqlx::query!(
                "WITH terms AS ( \
                     INSERT INTO genres (name) \
                     SELECT DISTINCT ON (lower(name)) name \
                       FROM unnest($2::text[]) AS t(name) \
                      ORDER BY lower(name) \
                     ON CONFLICT ((lower(name))) DO UPDATE SET name = genres.name \
                     RETURNING id \
                 ) \
                 INSERT INTO manifestation_genres (manifestation_id, genre_id, source_version_id) \
                 SELECT $1, id, $3 FROM terms \
                 ON CONFLICT (manifestation_id, genre_id) DO NOTHING",
                manifestation_id,
                names,
                version_id,
            )
            .execute(&mut **tx)
            .await
        }
        VocabularyField::Moods => {
            sqlx::query!(
                "WITH terms AS ( \
                     INSERT INTO moods (name) \
                     SELECT DISTINCT ON (lower(name)) name \
                       FROM unnest($2::text[]) AS t(name) \
                      ORDER BY lower(name) \
                     ON CONFLICT ((lower(name))) DO UPDATE SET name = moods.name \
                     RETURNING id \
                 ) \
                 INSERT INTO manifestation_moods (manifestation_id, mood_id, source_version_id) \
                 SELECT $1, id, $3 FROM terms \
                 ON CONFLICT (manifestation_id, mood_id) DO NOTHING",
                manifestation_id,
                names,
                version_id,
            )
            .execute(&mut **tx)
            .await
        }
        VocabularyField::Tags => {
            sqlx::query!(
                "WITH terms AS ( \
                     INSERT INTO tags (name) \
                     SELECT DISTINCT ON (lower(name)) name \
                       FROM unnest($2::text[]) AS t(name) \
                      ORDER BY lower(name) \
                     ON CONFLICT ((lower(name))) DO UPDATE SET name = tags.name \
                     RETURNING id \
                 ) \
                 INSERT INTO manifestation_tags (manifestation_id, tag_id, source_version_id) \
                 SELECT $1, id, $3 FROM terms \
                 ON CONFLICT (manifestation_id, tag_id) DO NOTHING",
                manifestation_id,
                names,
                version_id,
            )
            .execute(&mut **tx)
            .await
        }
    }
    .map_err(|e| AppError::Internal(e.into()))?;
    Ok(())
}

/// Apply one vocabulary field's merge-patch: journal the full replacement
/// list under the field name and rebuild the junction rows with the new
/// journal pointer.
///
/// `None` (top-level `null`) and an empty array both clear the field; the
/// clear is journaled as a null-valued accountability row, mirroring the
/// scalar clear path.
///
/// # Errors
/// - [`AppError::Validation`] on an empty/duplicate/over-length term or an
///   over-long list.
/// - [`AppError::Internal`] on database errors.
async fn apply_vocabulary_patch(
    tx: &mut Transaction<'_, Postgres>,
    manifestation_id: Uuid,
    user_id: Uuid,
    field: VocabularyField,
    maybe_names: Option<Vec<String>>,
) -> Result<(), AppError> {
    let key = field.field_name();
    // Empty array ≡ null: both clear the field.
    let names = maybe_names.filter(|v| !v.is_empty());
    match names {
        None => {
            insert_manual_version(tx, manifestation_id, user_id, key, &Value::Null).await?;
            delete_vocabulary_rows(tx, manifestation_id, field).await?;
        }
        Some(names) => {
            let trimmed = validated_vocabulary_terms(key, &names)?;
            let json = serde_json::to_value(&trimmed).map_err(|e| AppError::Internal(e.into()))?;
            let version_id =
                insert_manual_version(tx, manifestation_id, user_id, key, &json).await?;
            delete_vocabulary_rows(tx, manifestation_id, field).await?;
            insert_vocabulary_rows(tx, manifestation_id, field, &trimmed, version_id).await?;
        }
    }
    enqueue_writeback(tx, manifestation_id, key).await?;
    Ok(())
}

// ── manual metadata edit (RFC 7396 JSON Merge Patch) ──────────────────────

/// Per-field sparse update body for `PATCH /api/v1/books/{id}/metadata`.
///
/// Encodes RFC 7396 JSON Merge Patch semantics via
/// `serde_with::rust::double_option` (per
/// `docs/adr/0009-backend-auxiliary-crates-axum-extra-serde-with-and-subtle.md`):
/// * absent key → `None`         → leave field unchanged.
/// * key = `null` → `Some(None)` → clear the canonical value.
/// * key = value → `Some(Some(v))` → set the canonical value.
///
/// `cover` is not yet operator-editable from this endpoint — cover
/// promotion happens via a separate file-upload path.
#[expect(
    clippy::option_option,
    reason = "RFC 7396 sparse-update encoding — outer Option distinguishes absent (None) from present-and-null (Some(None))"
)]
#[derive(Debug, Default, Deserialize, utoipa::ToSchema)]
struct UpdateMetadataFields {
    /// Client-generated save identifier for resolution; an already recorded identifier is refused.
    #[schema(max_length = 45)]
    request_id: Option<Uuid>,
    /// New title. Absent = unchanged; `null` is rejected (canonical title
    /// is NOT NULL).
    #[serde(default, with = "::serde_with::rust::double_option")]
    #[schema(value_type = Option<String>)]
    title: Option<Option<String>>,
    /// New description. Absent = unchanged; `null` clears.
    #[serde(default, with = "::serde_with::rust::double_option")]
    #[schema(value_type = Option<String>)]
    description: Option<Option<String>>,
    /// New language. Absent = unchanged; `null` clears.
    #[serde(default, with = "::serde_with::rust::double_option")]
    #[schema(value_type = Option<String>)]
    language: Option<Option<String>>,
    /// New publisher. Absent = unchanged; `null` clears.
    #[serde(default, with = "::serde_with::rust::double_option")]
    #[schema(value_type = Option<String>)]
    publisher: Option<Option<String>>,
    /// New publication text at its supplied precision. Absent = unchanged;
    /// `null` clears.
    #[serde(default, with = "::serde_with::rust::double_option")]
    #[schema(value_type = Option<String>)]
    pub_date: Option<Option<String>>,
    /// New ISBN-10. Absent = unchanged; `null` clears.
    #[serde(default, with = "::serde_with::rust::double_option")]
    #[schema(value_type = Option<String>)]
    isbn_10: Option<Option<String>>,
    /// New ISBN-13. Absent = unchanged; `null` clears.
    #[serde(default, with = "::serde_with::rust::double_option")]
    #[schema(value_type = Option<String>)]
    isbn_13: Option<Option<String>>,
    /// New subtitle. Absent = unchanged; `null` clears.
    #[serde(default, with = "::serde_with::rust::double_option")]
    #[schema(value_type = Option<String>)]
    subtitle: Option<Option<String>>,
    /// New page count. Absent = unchanged; `null` clears; must be positive.
    #[serde(default, with = "::serde_with::rust::double_option")]
    #[schema(value_type = Option<i32>, minimum = 1)]
    pages: Option<Option<i32>>,
    /// New content rating tier. Absent = unchanged; `null` clears.
    #[serde(default, with = "::serde_with::rust::double_option")]
    #[schema(value_type = Option<ContentRating>)]
    content_rating: Option<Option<ContentRating>>,
    /// Full replacement genre list. Absent = unchanged; `null` or `[]`
    /// clears. Handled separately from the scalar fields; not part of
    /// [`Self::populated`].
    #[serde(default, with = "::serde_with::rust::double_option")]
    #[schema(value_type = Option<Vec<String>>, max_items = 50)]
    genres: Option<Option<Vec<String>>>,
    /// Full replacement mood list. Absent = unchanged; `null` or `[]`
    /// clears. Handled separately from the scalar fields; not part of
    /// [`Self::populated`].
    #[serde(default, with = "::serde_with::rust::double_option")]
    #[schema(value_type = Option<Vec<String>>, max_items = 50)]
    moods: Option<Option<Vec<String>>>,
    /// Full replacement tag list. Absent = unchanged; `null` or `[]`
    /// clears. Handled separately from the scalar fields; not part of
    /// [`Self::populated`].
    #[serde(default, with = "::serde_with::rust::double_option")]
    #[schema(value_type = Option<Vec<String>>, max_items = 50)]
    tags: Option<Option<Vec<String>>>,
    /// Per-role contributor replace/clear. Absent = unchanged; `null` clears
    /// author, editor, and translator together (pure RFC 7396 member removal).
    /// Handled separately from the other fields — not part of [`Self::populated`].
    #[serde(default, with = "::serde_with::rust::double_option")]
    #[schema(value_type = Option<ContributorsPatch>)]
    contributors: Option<Option<ContributorsPatch>>,
    /// Work-level external identifiers keyed by scheme (e.g.
    /// `{"openlibrary": "OL45804W"}`). Each entry independently sets its
    /// scheme's single slot; a `null` entry clears it. Absent (or `null`)
    /// leaves all work-level identifiers unchanged. Handled separately from
    /// the scalar fields — not part of [`Self::populated`].
    #[serde(default)]
    #[schema(schema_with = update_metadata_work_identifiers_schema)]
    work_identifiers: Option<BTreeMap<String, Option<String>>>,
    /// Manifestation-level external identifiers keyed by scheme (e.g.
    /// `{"googlebooks": "zyTZAAAAYAAJ"}`). Same per-entry set/clear semantics
    /// as `work_identifiers`.
    #[serde(default)]
    #[schema(schema_with = update_metadata_manifestation_identifiers_schema)]
    manifestation_identifiers: Option<BTreeMap<String, Option<String>>>,
}

/// utoipa 5.5's `#[schema(...)]` derive attribute has no `max_properties`
/// (`openapi::schema::Object` does), so the bounded identifier/response map
/// fields below build their schema by hand via `schema_with`.
fn nullable_string_value_schema() -> utoipa::openapi::schema::Object {
    use utoipa::openapi::schema::{ObjectBuilder, SchemaType, Type};
    ObjectBuilder::new()
        .schema_type(SchemaType::from_iter([Type::String, Type::Null]))
        .build()
}

fn string_property_names() -> utoipa::openapi::schema::Object {
    utoipa::openapi::schema::ObjectBuilder::new()
        .schema_type(utoipa::openapi::schema::Type::String)
        .build()
}

fn update_metadata_work_identifiers_schema() -> utoipa::openapi::schema::Object {
    update_metadata_identifiers_schema(
        "Work-level external identifiers keyed by scheme (e.g.\n\
             `{\"openlibrary\": \"OL45804W\"}`). Each entry independently sets its\n\
             scheme's single slot; a `null` entry clears it. Absent (or `null`)\n\
             leaves all work-level identifiers unchanged. Handled separately from\n\
             the scalar fields — not part of [`Self::populated`].",
    )
}

fn update_metadata_manifestation_identifiers_schema() -> utoipa::openapi::schema::Object {
    update_metadata_identifiers_schema(
        "Manifestation-level external identifiers keyed by scheme (e.g.\n\
             `{\"googlebooks\": \"zyTZAAAAYAAJ\"}`). Same per-entry set/clear semantics\n\
             as `work_identifiers`.",
    )
}

fn update_metadata_identifiers_schema(description: &str) -> utoipa::openapi::schema::Object {
    use utoipa::openapi::schema::{ObjectBuilder, SchemaType, Type};
    ObjectBuilder::new()
        .schema_type(SchemaType::from_iter([Type::Object, Type::Null]))
        .description(Some(description))
        .additional_properties(Some(utoipa::openapi::Schema::Object(
            nullable_string_value_schema(),
        )))
        .property_names(Some(
            ObjectBuilder::new()
                .schema_type(Type::String)
                .max_length(Some(12))
                .build(),
        ))
        .max_properties(Some(MAX_IDENTIFIERS_PER_LEVEL))
        .build()
}

/// Per-role contributor replace/clear, nested under `UpdateMetadataFields::contributors`.
///
/// Each role independently follows RFC 7396 double-`Option` semantics:
/// absent = untouched, `null` (or an empty array) clears the role, a
/// non-empty array replaces it wholesale in the given order. `narrator` is
/// deliberately not accepted here (reserved, not yet operator-writable);
/// any unrecognised key 422s via `deny_unknown_fields`.
#[expect(
    clippy::option_option,
    reason = "see UpdateMetadataFields struct attribute"
)]
#[derive(Debug, Default, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
struct ContributorsPatch {
    /// Ordered author names. `null` or `[]` clears the role.
    #[serde(default, with = "::serde_with::rust::double_option")]
    #[schema(value_type = Option<Vec<String>>, max_items = 100)]
    author: Option<Option<Vec<String>>>,
    /// Ordered editor names. `null` or `[]` clears the role.
    #[serde(default, with = "::serde_with::rust::double_option")]
    #[schema(value_type = Option<Vec<String>>, max_items = 100)]
    editor: Option<Option<Vec<String>>>,
    /// Ordered translator names. `null` or `[]` clears the role.
    #[serde(default, with = "::serde_with::rust::double_option")]
    #[schema(value_type = Option<Vec<String>>, max_items = 100)]
    translator: Option<Option<Vec<String>>>,
}

/// Response body for `PATCH /api/v1/books/{id}/metadata`.
#[derive(Debug, Serialize, utoipa::ToSchema)]
struct UpdateMetadataResponse {
    /// Save identifier for session-bound outcome resolution; absent for non-browser credentials.
    request_id: Option<Uuid>,
    /// Independent exact field operations issued to the authenticated browser session.
    #[schema(max_items = 144)]
    operations: Vec<undo::UndoOperation>,
    /// Complete canonical metadata paired with the response `ETag`.
    values: BookMetadata,
}

impl UpdateMetadataFields {
    /// Collect every populated entry as `(field_name, optional value)`
    /// — `None` means "clear this field", `Some(_)` means "set to this".
    ///
    /// `None` for the outer Option means the key was absent from the
    /// JSON body and is therefore not iterated; that case never appears
    /// in the returned vec.
    ///
    /// `pages` widens this to `Value` (rather than adding a parallel
    /// int-typed path) so every scalar field, string or numeric, flows
    /// through the same apply/clear plumbing.
    fn populated(self) -> Vec<(&'static str, Option<Value>)> {
        let mut out: Vec<(&'static str, Option<Value>)> = Vec::new();
        if let Some(v) = self.title {
            out.push(("title", v.map(Value::String)));
        }
        if let Some(v) = self.description {
            out.push(("description", v.map(Value::String)));
        }
        if let Some(v) = self.language {
            out.push(("language", v.map(Value::String)));
        }
        if let Some(v) = self.publisher {
            out.push(("publisher", v.map(Value::String)));
        }
        if let Some(v) = self.pub_date {
            out.push(("pub_date", v.map(Value::String)));
        }
        if let Some(v) = self.isbn_10 {
            out.push(("isbn_10", v.map(Value::String)));
        }
        if let Some(v) = self.isbn_13 {
            out.push(("isbn_13", v.map(Value::String)));
        }
        if let Some(v) = self.subtitle {
            out.push(("subtitle", v.map(Value::String)));
        }
        if let Some(v) = self.pages {
            out.push(("pages", v.map(Value::from)));
        }
        if let Some(v) = self.content_rating {
            out.push((
                "content_rating",
                v.map(|r| Value::String(r.as_str().to_owned())),
            ));
        }
        out
    }
}

/// Contributor names by role, mirroring [`ContributorsPatch`]'s roles for
/// the matched-GET response.
#[derive(Debug, Serialize, utoipa::ToSchema)]
struct MetadataContributors {
    /// Ordered author names.
    author: Vec<String>,
    /// Ordered editor names.
    editor: Vec<String>,
    /// Ordered translator names.
    translator: Vec<String>,
}

/// Response body for `GET /api/v1/books/{id}/metadata` — exactly the span
/// [`UpdateMetadataFields`] can write, field for field.
#[derive(Debug, Serialize, utoipa::ToSchema)]
struct BookMetadata {
    /// Canonical title (work-level, `NOT NULL`).
    title: String,
    /// Subtitle; `null` when unset.
    subtitle: Option<String>,
    /// Description; `null` when unset.
    description: Option<String>,
    /// Language; `null` when unset.
    language: Option<String>,
    /// Publisher; `null` when unset.
    publisher: Option<String>,
    /// Publication text at its supplied precision; `null` when unset.
    pub_date: Option<String>,
    /// ISBN-10; `null` when unset.
    isbn_10: Option<String>,
    /// ISBN-13; `null` when unset.
    isbn_13: Option<String>,
    /// Whether the saved ISBN pair contains valid, non-equivalent numbers.
    isbn_mismatch: bool,
    /// Page count; `null` when unset.
    pages: Option<i32>,
    /// Content rating tier; `null` when unset.
    content_rating: Option<ContentRating>,
    /// Genre names, alphabetical; empty when none are set.
    genres: Vec<String>,
    /// Mood names, alphabetical; empty when none are set.
    moods: Vec<String>,
    /// Tag names, alphabetical; empty when none are set.
    tags: Vec<String>,
    /// Author/editor/translator names, each in stored position order.
    contributors: MetadataContributors,
    /// Work-level external identifiers keyed by scheme.
    #[schema(schema_with = book_metadata_work_identifiers_schema)]
    work_identifiers: BTreeMap<String, String>,
    /// Manifestation-level external identifiers keyed by scheme.
    #[schema(schema_with = book_metadata_manifestation_identifiers_schema)]
    manifestation_identifiers: BTreeMap<String, String>,
}

fn book_metadata_work_identifiers_schema() -> utoipa::openapi::schema::Object {
    use utoipa::openapi::schema::{ObjectBuilder, Type};
    ObjectBuilder::new()
        .schema_type(Type::Object)
        .description(Some("Work-level external identifiers keyed by scheme."))
        .additional_properties(Some(ObjectBuilder::new().schema_type(Type::String)))
        .property_names(Some(string_property_names()))
        .max_properties(Some(MAX_IDENTIFIERS_PER_LEVEL))
        .build()
}

fn book_metadata_manifestation_identifiers_schema() -> utoipa::openapi::schema::Object {
    use utoipa::openapi::schema::{ObjectBuilder, Type};
    ObjectBuilder::new()
        .schema_type(Type::Object)
        .description(Some(
            "Manifestation-level external identifiers keyed by scheme.",
        ))
        .additional_properties(Some(ObjectBuilder::new().schema_type(Type::String)))
        .property_names(Some(string_property_names()))
        .max_properties(Some(MAX_IDENTIFIERS_PER_LEVEL))
        .build()
}

struct MetadataSnapshot {
    values: BookMetadata,
    revision: MetadataRevision,
}

/// Loads editable values and their owner revisions within the caller's snapshot or parent locks.
async fn load_book_metadata(
    tx: &mut Transaction<'_, Postgres>,
    manifestation_id: Uuid,
) -> Result<Option<MetadataSnapshot>, AppError> {
    let Some(core) = sqlx::query!(
        r#"
        SELECT w.id             AS "work_id!",
               w.metadata_revision AS "work_revision!",
               m.metadata_revision AS "manifestation_revision!",
               w.title          AS "title!",
               w.subtitle       AS subtitle,
               w.description    AS description,
               w.language       AS language,
               m.publisher      AS publisher,
               m.pub_date       AS pub_date,
               m.isbn_10        AS isbn_10,
               m.isbn_13        AS isbn_13,
               m.pages          AS pages,
               m.content_rating AS "content_rating: ContentRating"
          FROM manifestations m
          JOIN works w ON w.id = m.work_id
         WHERE m.id = $1
        "#,
        manifestation_id,
    )
    .fetch_optional(&mut **tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?
    else {
        return Ok(None);
    };

    let ids = std::slice::from_ref(&manifestation_id);
    let mut genres = load_genres_for_manifestations(tx, ids).await?;
    let mut moods = load_moods_for_manifestations(tx, ids).await?;
    let mut tags = load_tags_for_manifestations(tx, ids).await?;
    let contributors = load_contributors_for_work(tx, core.work_id).await?;
    let work_identifiers = load_work_identifiers(tx, core.work_id).await?;
    let manifestation_identifiers = load_manifestation_identifiers(tx, manifestation_id).await?;

    Ok(Some(MetadataSnapshot {
        revision: MetadataRevision {
            work_id: core.work_id,
            manifestation_id,
            work_revision: core.work_revision,
            manifestation_revision: core.manifestation_revision,
        },
        values: BookMetadata {
            title: core.title,
            subtitle: core.subtitle,
            description: core.description,
            language: core.language,
            publisher: core.publisher,
            pub_date: core.pub_date,
            isbn_mismatch: match (core.isbn_10.as_deref(), core.isbn_13.as_deref()) {
                (Some(ten), Some(thirteen)) => match (
                    ten.parse::<isbn::Isbn10>(),
                    thirteen.parse::<isbn::Isbn13>(),
                ) {
                    (Ok(ten), Ok(thirteen)) => {
                        let compact = thirteen.to_string();
                        (compact.starts_with("978") || compact.starts_with("979"))
                            && isbn::Isbn13::from(ten) != thirteen
                    }
                    _ => false,
                },
                _ => false,
            },
            isbn_10: core.isbn_10,
            isbn_13: core.isbn_13,
            pages: core.pages,
            content_rating: core.content_rating,
            genres: genres.remove(&manifestation_id).unwrap_or_default(),
            moods: moods.remove(&manifestation_id).unwrap_or_default(),
            tags: tags.remove(&manifestation_id).unwrap_or_default(),
            contributors,
            work_identifiers,
            manifestation_identifiers,
        },
    }))
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
    current_user.require_not_child()?;

    let mut tx = db::acquire_with_rls_snapshot(&state.pool, current_user.user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let metadata = load_book_metadata(&mut tx, manifestation_id)
        .await?
        .ok_or(AppError::NotFound)?;
    let etag = metadata.revision.etag()?;

    tx.commit()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let mut headers = HeaderMap::new();
    headers.insert(ETAG, etag);
    Ok((headers, axum::Json(metadata.values)))
}

/// Author/editor/translator names for a work, each ordered by stored
/// `work_authors.position` within the role. Only these three PATCH-writable
/// roles are included; narrators are stored but excluded from this
/// representation.
async fn load_contributors_for_work(
    tx: &mut Transaction<'_, Postgres>,
    work_id: Uuid,
) -> Result<MetadataContributors, AppError> {
    let rows = sqlx::query!(
        r#"SELECT wa.role::text AS "role!", a.name AS "name!"
             FROM work_authors wa
             JOIN authors a ON a.id = wa.author_id
            WHERE wa.work_id = $1
              AND wa.role = ANY(ARRAY['author', 'editor', 'translator']::author_role[])
            ORDER BY wa.role, wa.position ASC"#,
        work_id,
    )
    .fetch_all(&mut **tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    let mut contributors = MetadataContributors {
        author: Vec::new(),
        editor: Vec::new(),
        translator: Vec::new(),
    };
    for row in rows {
        match row.role.as_str() {
            "author" => contributors.author.push(row.name),
            "editor" => contributors.editor.push(row.name),
            "translator" => contributors.translator.push(row.name),
            other => {
                return Err(AppError::Internal(anyhow::anyhow!(
                    "unexpected author_role '{other}' in work_authors"
                )));
            }
        }
    }
    Ok(contributors)
}

/// Work-level external identifiers keyed by scheme.
async fn load_work_identifiers(
    tx: &mut Transaction<'_, Postgres>,
    work_id: Uuid,
) -> Result<BTreeMap<String, String>, AppError> {
    Ok(sqlx::query!(
        "SELECT scheme, external_id FROM work_external_identifiers WHERE work_id = $1",
        work_id,
    )
    .fetch_all(&mut **tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?
    .into_iter()
    .map(|r| (r.scheme, r.external_id))
    .collect())
}

/// Manifestation-level external identifiers keyed by scheme.
async fn load_manifestation_identifiers(
    tx: &mut Transaction<'_, Postgres>,
    manifestation_id: Uuid,
) -> Result<BTreeMap<String, String>, AppError> {
    Ok(sqlx::query!(
        "SELECT scheme, external_id FROM manifestation_external_identifiers \
         WHERE manifestation_id = $1",
        manifestation_id,
    )
    .fetch_all(&mut **tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?
    .into_iter()
    .map(|r| (r.scheme, r.external_id))
    .collect())
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
        (status = 200, description = "Manual edit committed with complete canonical values and matching ETag, and per-field Undo operations", body = UpdateMetadataResponse,
         headers(("ETag" = String, description = "Strong entity-tag covering work and edition identities and their metadata revisions after this write"))),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 403, description = "Caller is a child account", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 404, description = "Manifestation missing or RLS-hidden (existence-not-leaked)", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 412, description = "If-Match does not match the manifestation's current metadata ETag", body = crate::openapi::ProblemDetails, content_type = "application/problem+json",
         headers(("ETag" = String, description = "Current entity-tag, so the caller can resync without a follow-up GET"))),
        (status = 400, description = "If-Match is malformed, or carries a form this API refuses by policy: the * wildcard, an entity-tag list, a weak tag, or a repeated header instance, the request body is not valid JSON, or a path parameter is malformed", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 422, description = "No populated fields, ISBN/date parse failure, attempt to clear title, recorded save request identifier reuse, or browser Undo size/work limit. Evaluated only after If-Match has matched, so a stale tag returns 412 instead", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 428, description = "If-Match header absent", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
    )
)]
#[expect(
    clippy::too_many_lines,
    reason = "sequential dispatch across the six patchable field families (scalars, vocabularies, contributors, identifiers, ISBN rematch, response assembly); each family already delegates to its own helper"
)]
#[warn(clippy::cognitive_complexity)]
async fn update_book_metadata(
    current_user: CurrentUser,
    State(state): State<AppState>,
    session: tower_sessions::Session,
    ApiPath(manifestation_id): ApiPath<Uuid>,
    headers_in: HeaderMap,
    ApiJson(mut req_fields): ApiJson<UpdateMetadataFields>,
) -> Result<Response, AppError> {
    current_user.require_scope(Scope::Write)?;
    current_user.require_not_child()?;
    let if_match = parse_if_match(&headers_in)?.ok_or(AppError::IfMatchRequired)?;
    // Extract contributors BEFORE populated() consumes the struct — it is
    // handled separately from the other (scalar) fields.
    let mut browser_save =
        undo::browser_save(&current_user, &session, req_fields.request_id.take())?;
    let contributors = req_fields.contributors.take();
    let genres = req_fields.genres.take();
    let moods = req_fields.moods.take();
    let tags = req_fields.tags.take();
    let work_identifiers = req_fields.work_identifiers.take();
    let manifestation_identifiers = req_fields.manifestation_identifiers.take();
    let fields = req_fields.populated();

    let mut tx = db::acquire_with_rls_read_committed(&state.pool, current_user.user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    // Lock the manifestation + parent work for the duration of the tx.
    // Mirrors `revert_manifestation` — serialises concurrent edits and
    // gates RLS-hidden rows to 404 (the join through `manifestations`
    // applies the RLS predicate).
    undo::require_save_session(&mut tx, browser_save.as_mut()).await?;
    let work_id = undo::lock_parents(&mut tx, manifestation_id).await?;

    // Compared under the row lock just taken above, so a concurrent editor
    // cannot slip a change in between this check and the writes below.
    let current = load_book_metadata(&mut tx, manifestation_id)
        .await?
        .ok_or_else(|| {
            AppError::Internal(anyhow::anyhow!(
                "load_book_metadata returned None for a manifestation just locked"
            ))
        })?;
    let current_etag = current.revision.etag()?;
    if !if_match.matches(&current_etag) {
        return Ok(if_match_mismatch(&current_etag));
    }

    // Semantic body validation runs only once the precondition has held.
    // RFC 9110 §13.2.1 places precondition evaluation after the normal
    // request checks (auth, existence) and before the request content is
    // processed, so a caller holding a stale representation learns that
    // first and refetches, rather than being sent to fix a body that a
    // concurrent write may already have made irrelevant. The remaining
    // per-field rejections below already sat on this side of the check.
    if fields.is_empty()
        && contributors.is_none()
        && genres.is_none()
        && moods.is_none()
        && tags.is_none()
        && work_identifiers.as_ref().is_none_or(BTreeMap::is_empty)
        && manifestation_identifiers
            .as_ref()
            .is_none_or(BTreeMap::is_empty)
    {
        return Err(AppError::Validation("no fields".into()));
    }

    let mut touched_fields: Vec<String> = fields
        .iter()
        .map(|(field, _)| (*field).to_owned())
        .collect();
    touched_fields.extend(
        [
            ("genres", genres.is_some()),
            ("moods", moods.is_some()),
            ("tags", tags.is_some()),
        ]
        .into_iter()
        .filter(|(_, touched)| *touched)
        .map(|(field, _)| field.to_owned()),
    );
    if let Some(patch) = &contributors {
        touched_fields.extend(
            [
                ("author", patch.as_ref().is_none_or(|p| p.author.is_some())),
                ("editor", patch.as_ref().is_none_or(|p| p.editor.is_some())),
                (
                    "translator",
                    patch.as_ref().is_none_or(|p| p.translator.is_some()),
                ),
            ]
            .into_iter()
            .filter(|(_, touched)| *touched)
            .map(|(role, _)| format!("contributors.{role}")),
        );
    }
    for (level, identifiers) in [
        (IdentifierLevel::Work, &work_identifiers),
        (IdentifierLevel::Manifestation, &manifestation_identifiers),
    ] {
        if let Some(identifiers) = identifiers {
            if identifiers.len() > MAX_IDENTIFIERS_PER_LEVEL {
                return Err(AppError::Validation(format!(
                    "{}-level identifiers exceed {MAX_IDENTIFIERS_PER_LEVEL} entries",
                    level.as_str()
                )));
            }
            touched_fields.extend(
                identifiers
                    .keys()
                    .map(|scheme| level.canonical_field(scheme)),
            );
        }
    }
    require_unlocked_fields(
        &mut tx,
        manifestation_id,
        current_user.user_id,
        &touched_fields,
    )
    .await?;

    let before = undo::begin_save(
        &mut tx,
        browser_save,
        manifestation_id,
        work_id,
        &touched_fields,
    )
    .await?;

    let touched_isbn = fields
        .iter()
        .any(|(field, _)| matches!(*field, "isbn_10" | "isbn_13"));
    for (field, maybe_value) in fields {
        apply_scalar_patch_field(
            &mut tx,
            manifestation_id,
            work_id,
            current_user.user_id,
            field,
            maybe_value,
        )
        .await?;
    }

    for (vocab, patch) in [
        (VocabularyField::Genres, genres),
        (VocabularyField::Moods, moods),
        (VocabularyField::Tags, tags),
    ] {
        if let Some(maybe_names) = patch {
            apply_vocabulary_patch(
                &mut tx,
                manifestation_id,
                current_user.user_id,
                vocab,
                maybe_names,
            )
            .await?;
        }
    }

    if let Some(patch) = contributors {
        apply_contributors_patch(
            &mut tx,
            manifestation_id,
            work_id,
            current_user.user_id,
            patch,
        )
        .await?;
    }

    apply_identifier_patches(
        &mut tx,
        manifestation_id,
        work_id,
        current_user.user_id,
        work_identifiers,
        manifestation_identifiers,
    )
    .await?;

    if touched_isbn {
        work::rematch_on_isbn_change(&mut tx, manifestation_id)
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
    }

    // Computed before commit, inside the same transaction as the writes
    // above, so the emitted ETag reflects exactly the representation this
    // PATCH just committed.
    let new_metadata = load_book_metadata(&mut tx, manifestation_id)
        .await?
        .ok_or_else(|| {
            AppError::Internal(anyhow::anyhow!(
                "load_book_metadata returned None for a manifestation just written"
            ))
        })?;
    let new_etag = new_metadata.revision.etag()?;

    let (request_id, operations) = undo::finish_save(&mut tx, before).await?;

    tx.commit()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let mut headers = HeaderMap::new();
    headers.insert(ETAG, new_etag);
    headers.insert(
        axum::http::header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("no-store"),
    );
    Ok((
        StatusCode::OK,
        headers,
        axum::Json(UpdateMetadataResponse {
            request_id,
            operations,
            values: new_metadata.values,
        }),
    )
        .into_response())
}

/// Journal + apply one scalar field from the manual PATCH surface.
/// A set (`Some`) journals the value and wires the canonical pointer via
/// `apply_version`; a clear (`None`) journals an accountability row and
/// nulls the canonical via `clear_field`.
async fn apply_scalar_patch_field(
    tx: &mut Transaction<'_, Postgres>,
    manifestation_id: Uuid,
    work_id: Uuid,
    user_id: Uuid,
    field: &str,
    maybe_value: Option<serde_json::Value>,
) -> Result<(), AppError> {
    if let Some(value) = maybe_value {
        let json = match field {
            "isbn_10" | "isbn_13" => Value::String(normalise_isbn_field(field, &value)?),
            _ => value,
        };
        let version_id = insert_manual_version(tx, manifestation_id, user_id, field, &json).await?;
        apply_version(tx, field, &json, version_id, manifestation_id, work_id).await?;
    } else {
        // Audit-trail row: source='manual', new_value=null,
        // resolved_by = caller. `clear_field` does NOT wire a
        // canonical pointer to this row — by design — so it lives
        // in the journal as an orphan record of the clear action,
        // but `resolved_by` makes the action accountable.
        insert_manual_version(
            tx,
            manifestation_id,
            user_id,
            field,
            &serde_json::Value::Null,
        )
        .await?;
        clear_field(tx, field, manifestation_id, work_id).await?;
    }
    Ok(())
}

/// `identifier_schemes` seeds 10 rows (`backend/migrations/20260810000000_initial_schema.up.sql`);
/// bounds the entries accepted per level in one patch with headroom for
/// growth in the scheme vocabulary.
const MAX_IDENTIFIERS_PER_LEVEL: usize = 64;

/// Apply both identifier maps from the manual PATCH surface, then re-queue
/// enrichment once if anything changed. Resetting the attempt counter and
/// timestamp alongside the status clears any backoff window from a prior
/// failed run; status alone would leave the row uneligible for up to 24
/// hours (see `queue::claim_next`).
///
/// An `in_progress` row is never flipped to `pending` here: the active
/// worker still owns the claim, and making the row eligible again would let
/// a second worker run the same manifestation concurrently. The edit sets
/// `enrichment_rerun_requested` instead, and the worker's completion
/// bookkeeping converts the flag into a fresh eligible row (the active run
/// snapshotted its lookup keys before this edit, so its result may be
/// stale). All CASE arms read the pre-update row state, so the status test
/// is consistent across every column.
async fn apply_identifier_patches(
    tx: &mut Transaction<'_, Postgres>,
    manifestation_id: Uuid,
    work_id: Uuid,
    user_id: Uuid,
    work_identifiers: Option<BTreeMap<String, Option<String>>>,
    manifestation_identifiers: Option<BTreeMap<String, Option<String>>>,
) -> Result<(), AppError> {
    let mut touched = false;
    for (level, map) in [
        (IdentifierLevel::Work, work_identifiers),
        (IdentifierLevel::Manifestation, manifestation_identifiers),
    ] {
        for (scheme, maybe_value) in map.into_iter().flatten() {
            apply_identifier_patch(
                tx,
                manifestation_id,
                work_id,
                user_id,
                level,
                &scheme,
                maybe_value,
            )
            .await?;
            touched = true;
        }
    }
    if touched {
        requeue_identifier_enrichment(tx, manifestation_id).await?;
    }
    Ok(())
}

async fn requeue_identifier_enrichment(
    tx: &mut Transaction<'_, Postgres>,
    manifestation_id: Uuid,
) -> Result<(), AppError> {
    sqlx::query!(
            "UPDATE manifestations \
             SET enrichment_rerun_requested = (enrichment_status = 'in_progress'), \
                 enrichment_status = CASE WHEN enrichment_status = 'in_progress' \
                                          THEN enrichment_status \
                                          ELSE 'pending'::enrichment_status END, \
                 enrichment_attempt_count = CASE WHEN enrichment_status = 'in_progress' \
                                                 THEN enrichment_attempt_count ELSE 0 END, \
                 enrichment_attempted_at = CASE WHEN enrichment_status = 'in_progress' \
                                                THEN enrichment_attempted_at ELSE NULL END, \
                 enrichment_retry_not_before = CASE WHEN enrichment_status = 'in_progress' \
                                                    THEN enrichment_retry_not_before ELSE NULL END, \
                 enrichment_error = CASE WHEN enrichment_status = 'in_progress' \
                                         THEN enrichment_error ELSE NULL END, \
                 enrichment_failures = CASE WHEN enrichment_status = 'in_progress' \
                                            THEN enrichment_failures ELSE '[]'::jsonb END \
             WHERE id = $1",
            manifestation_id,
        )
        .execute(&mut **tx)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    Ok(())
}

/// Journal + apply one external-identifier slot from the manual PATCH
/// surface. A set (`Some`) parses the raw value into the scheme's canonical
/// form, journals it under `identifiers.<level>.<scheme>`, and upserts the
/// registry slot via `apply_version`; a clear (`None`) journals an
/// accountability row and deletes the slot via `clear_field`. Neither path
/// enqueues a writeback (identifiers are never written back to the file).
async fn apply_identifier_patch(
    tx: &mut Transaction<'_, Postgres>,
    manifestation_id: Uuid,
    work_id: Uuid,
    user_id: Uuid,
    level: IdentifierLevel,
    scheme: &str,
    maybe_value: Option<String>,
) -> Result<(), AppError> {
    // Validate the address before journaling so an unknown scheme or a
    // wrong-level address never produces a journal row, on the clear path
    // included.
    external_id::validate_scheme_level(level, scheme).map_err(AppError::Validation)?;
    let field = level.canonical_field(scheme);
    if let Some(raw) = maybe_value {
        let canonical =
            external_id::parse_external_id(level, scheme, &raw).map_err(AppError::Validation)?;
        let json = Value::String(canonical);
        let version_id =
            insert_manual_version(tx, manifestation_id, user_id, &field, &json).await?;
        apply_version(tx, &field, &json, version_id, manifestation_id, work_id).await?;
    } else {
        insert_manual_version(tx, manifestation_id, user_id, &field, &Value::Null).await?;
        clear_field(tx, &field, manifestation_id, work_id).await?;
    }
    Ok(())
}

/// Record an exact manual observation, retaining its submission author on repeat saves.
async fn insert_manual_version(
    tx: &mut Transaction<'_, Postgres>,
    manifestation_id: Uuid,
    user_id: Uuid,
    field: &str,
    value: &Value,
) -> Result<Uuid, AppError> {
    let hash = value_hash::value_hash(field, value);
    let id: Uuid = sqlx::query_scalar!(
        "INSERT INTO metadata_versions \
            (manifestation_id, source, field_name, new_value, value_hash, \
             match_type, confidence_score, status, submitted_by, resolved_by, resolved_at) \
         VALUES ($1, 'manual', $2, $3, $4, 'manual', 1.0, \
                 'pending'::metadata_review_status, $5, $5, now()) \
         ON CONFLICT (manifestation_id, source, field_name, observation_key) \
         DO UPDATE SET last_seen_at = now(), \
                       observation_count = metadata_versions.observation_count + 1, \
                       status = 'pending'::metadata_review_status, \
                       resolved_by = $5, \
                       resolved_at = now() \
         RETURNING id",
        manifestation_id,
        field,
        value,
        hash,
        user_id,
    )
    .fetch_one(&mut **tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    Ok(id)
}

#[cfg(test)]
mod tests {
    use crate::error::problems;
    use crate::test_support;
    use axum::http::StatusCode;
    use uuid::Uuid;

    async fn undo_browser(
        app: &sqlx::PgPool,
        ing: &sqlx::PgPool,
        name: &str,
    ) -> (Uuid, axum_test::TestServer, String) {
        let email = format!("{name}@example.test");
        let user = test_support::db::create_adult_with_password(
            app,
            name,
            &email,
            test_support::mail::PASSWORD,
        )
        .await;
        let mut server = test_support::db::server_with_real_pools(app, ing);
        server.save_cookies();
        let csrf = test_support::mail::login(&server, &email, test_support::mail::PASSWORD).await;
        (user, server, csrf)
    }

    async fn browser_patch(
        server: &axum_test::TestServer,
        edition: Uuid,
        csrf: &str,
        body: serde_json::Value,
    ) -> axum_test::TestResponse {
        let path = format!("/api/v1/books/{edition}/metadata");
        let current = server.get(&path).await;
        assert_eq!(current.status_code(), StatusCode::OK, "{}", current.text());
        server
            .patch(&path)
            .add_header(
                test_support::mail::csrf(csrf).0,
                test_support::mail::csrf(csrf).1,
            )
            .add_header(
                axum::http::header::IF_MATCH,
                current.header(axum::http::header::ETAG),
            )
            .json(&body)
            .await
    }

    async fn browser_undo(
        server: &axum_test::TestServer,
        edition: Uuid,
        csrf: &str,
        operation: &serde_json::Value,
        tag: axum::http::HeaderValue,
    ) -> axum_test::TestResponse {
        server
            .post(&format!("/api/v1/books/{edition}/metadata/undo"))
            .add_header(
                test_support::mail::csrf(csrf).0,
                test_support::mail::csrf(csrf).1,
            )
            .add_header(axum::http::header::IF_MATCH, tag)
            .json(&serde_json::json!({"operation_id": operation}))
            .await
    }

    async fn captured_fields(
        pool: &sqlx::PgPool,
        user: Uuid,
        edition: Uuid,
        work: Uuid,
        fields: &[String],
    ) -> serde_json::Value {
        let mut tx = crate::db::acquire_with_rls_read_committed(pool, user)
            .await
            .unwrap();
        let captured = super::undo::capture(
            &mut tx,
            edition,
            work,
            fields,
            &mut super::undo::ResourceBudget::capture(),
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        serde_json::to_value(captured.fields).unwrap()
    }

    // Data corruption/authentication boundaries: exact state, atomic resource refusal, claim ownership and capture mode.
    // REV-REQ-0052 reversion/identifier jobs; REV-REQ-0072 AC2-4 and REV-REQ-0073 AC2-4.
    #[sqlx::test(migrations = "./migrations")]
    async fn metadata_undo_restores_exact_field_state(pool: sqlx::PgPool) {
        let app = test_support::db::app_pool_for(&pool).await;
        let ing = test_support::db::ingestion_pool_for(&pool).await;
        let (user, server, csrf) = undo_browser(&app, &ing, "undo-exact").await;
        let (_, other, other_csrf) = undo_browser(&app, &ing, "undo-other").await;
        let (work, edition) =
            test_support::db::insert_work_and_manifestation(&ing, "undo-exact").await;
        let initial = browser_patch(&server, edition, &csrf, serde_json::json!({
            "title":"Original", "subtitle":"Original subtitle", "description":null, "language":"en",
            "publisher":"Original Press", "pub_date":"2024-01-15T00:00:00.123456789+05:30",
            "isbn_10":"0306406152", "isbn_13":"9780140328721", "pages":321, "content_rating":"teen",
            "genres":["Fantasy","Mystery"], "moods":["Calm"], "tags":["Read"],
            "contributors":{"author":["Alice","Bob"],"editor":["Editor"],"translator":["Translator"]},
            "work_identifiers":{"openlibrary":"OL123W"}, "manifestation_identifiers":{"googlebooks":"Volume_123"}
        })).await;
        assert_eq!(initial.status_code(), StatusCode::OK, "{}", initial.text());
        sqlx::query!(
            "UPDATE work_authors SET source_version_id = NULL WHERE work_id = $1 AND position = 0",
            work
        )
        .execute(&ing)
        .await
        .unwrap();
        sqlx::query!(
            "UPDATE manifestations SET isbn_10 = '0-306-40615-2' WHERE id = $1",
            edition
        )
        .execute(&ing)
        .await
        .unwrap();
        let fields: Vec<String> = [
            "title",
            "subtitle",
            "description",
            "language",
            "publisher",
            "pub_date",
            "isbn_10",
            "isbn_13",
            "pages",
            "content_rating",
            "genres",
            "moods",
            "tags",
            "contributors.author",
            "contributors.editor",
            "contributors.translator",
            "identifiers.work.openlibrary",
            "identifiers.manifestation.googlebooks",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect();
        let before = captured_fields(&app, user, edition, work, &fields).await;
        let response = browser_patch(&other, edition, &other_csrf, serde_json::json!({
            "title":"Original", "subtitle":null, "description":"New", "language":null,
            "publisher":null, "pub_date":"2025", "isbn_10":null, "isbn_13":"9780306406157", "pages":null,
            "content_rating":"everyone", "genres":["Science"], "moods":null, "tags":["Unread"],
            "contributors":{"author":["Bob","Alice"],"editor":null,"translator":["Another translator"]},
            "work_identifiers":{"openlibrary":null}, "manifestation_identifiers":{"googlebooks":"Volume_456"}
        })).await;
        assert_eq!(
            response.status_code(),
            StatusCode::OK,
            "{}",
            response.text()
        );
        let body: serde_json::Value = response.json();
        let operations = body["operations"].as_array().unwrap();
        assert_eq!(operations.len(), fields.len());
        for op in operations {
            let field = op["field_name"].as_str().unwrap();
            let identifier = field.starts_with("identifiers.");
            let active_enrichment = field == "identifiers.manifestation.googlebooks";
            if identifier {
                sqlx::query!(
                    "UPDATE manifestations SET enrichment_status = CASE WHEN $2 THEN 'in_progress'::enrichment_status \
                     ELSE 'complete'::enrichment_status END, enrichment_rerun_requested = FALSE, \
                     enrichment_claim_seq = 7, enrichment_attempt_count = 2, enrichment_attempted_at = clock_timestamp() \
                     WHERE id = $1", edition, active_enrichment,
                ).execute(&pool).await.unwrap();
            }
            let jobs_before = sqlx::query_scalar!(
                "SELECT count(*) AS \"count!\" FROM writeback_jobs WHERE manifestation_id = $1",
                edition
            )
            .fetch_one(&pool)
            .await
            .unwrap();
            let current = other
                .get(&format!("/api/v1/books/{edition}/metadata"))
                .await;
            let restored = browser_undo(
                &other,
                edition,
                &other_csrf,
                &op["operation_id"],
                current.header(axum::http::header::ETAG),
            )
            .await;
            assert_eq!(
                restored.status_code(),
                StatusCode::OK,
                "{field}: {}",
                restored.text()
            );
            let jobs_after = sqlx::query_scalar!(
                "SELECT count(*) AS \"count!\" FROM writeback_jobs WHERE manifestation_id = $1",
                edition
            )
            .fetch_one(&pool)
            .await
            .unwrap();
            assert_eq!(jobs_after - jobs_before, i64::from(!identifier));
            if identifier {
                let enrichment = sqlx::query!(
                    "SELECT enrichment_status::text AS \"status!\", enrichment_rerun_requested, \
                     enrichment_claim_seq, enrichment_attempt_count FROM manifestations WHERE id = $1",
                    edition,
                ).fetch_one(&pool).await.unwrap();
                assert_eq!(
                    enrichment.status,
                    if active_enrichment {
                        "in_progress"
                    } else {
                        "pending"
                    }
                );
                assert_eq!(enrichment.enrichment_rerun_requested, active_enrichment);
                assert_eq!(enrichment.enrichment_claim_seq, 7);
                assert_eq!(
                    enrichment.enrichment_attempt_count,
                    if active_enrichment { 2 } else { 0 }
                );
            }
        }
        assert_eq!(
            captured_fields(&app, user, edition, work, &fields).await,
            before
        );
        let authors = vec!["contributors.author".to_owned()];
        let original = captured_fields(&app, user, edition, work, &authors).await;
        let reordered = browser_patch(
            &server,
            edition,
            &csrf,
            serde_json::json!({"contributors":{"author":["Bob","Alice"]}}),
        )
        .await;
        assert_eq!(
            reordered.status_code(),
            StatusCode::OK,
            "{}",
            reordered.text()
        );
        let reorder: serde_json::Value = reordered.json();
        let op = &reorder["operations"][0]["operation_id"];
        let result = browser_undo(
            &server,
            edition,
            &csrf,
            op,
            reordered.header(axum::http::header::ETAG),
        )
        .await;
        assert_eq!(result.status_code(), StatusCode::OK, "{}", result.text());
        assert_eq!(
            captured_fields(&app, user, edition, work, &authors).await,
            original
        );

        let (stub_work, stub) =
            test_support::db::insert_work_and_manifestation(&ing, "undo-stub").await;
        let populated = browser_patch(
            &server,
            stub,
            &csrf,
            serde_json::json!({"contributors":{"author":["New author"]}}),
        )
        .await;
        assert_eq!(
            populated.status_code(),
            StatusCode::OK,
            "{}",
            populated.text()
        );
        let op = populated.json::<serde_json::Value>()["operations"][0]["operation_id"].clone();
        let result = browser_undo(
            &server,
            stub,
            &csrf,
            &op,
            populated.header(axum::http::header::ETAG),
        )
        .await;
        assert_eq!(result.status_code(), StatusCode::OK, "{}", result.text());
        assert_eq!(
            sqlx::query_scalar!(
                "SELECT count(*) AS \"count!\" FROM work_authors WHERE work_id = $1",
                stub_work
            )
            .fetch_one(&pool)
            .await
            .unwrap(),
            0
        );

        let (large_work, large_edition) =
            test_support::db::insert_work_and_manifestation(&ing, "undo-large").await;
        let (overflow_work, overflow_edition) =
            test_support::db::insert_work_and_manifestation(&ing, "undo-overflow").await;
        let seed_authors = |work: Uuid, count: i32, marker: String| {
            let ing = &ing;
            async move {
                sqlx::query!(
                    "WITH inserted AS (INSERT INTO authors(name, sort_name) \
                     SELECT 'Undo ' || $3 || g || repeat('x',480), 'undo-' || $3 || g \
                     FROM generate_series(1,$2::int) g RETURNING id) \
                     INSERT INTO work_authors(work_id,author_id,role,position) \
                     SELECT $1,id,'author',(row_number() OVER (ORDER BY id)-1)::int FROM inserted",
                    work,
                    count,
                    marker,
                )
                .execute(ing)
                .await
                .unwrap();
            }
        };
        seed_authors(large_work, 129, "large".into()).await;
        seed_authors(
            overflow_work,
            i32::try_from(super::undo::MAX_CAPTURE_ENTRIES + 1).unwrap(),
            "overflow".into(),
        )
        .await;
        sqlx::query!(
            "WITH inserted AS (INSERT INTO genres(name) SELECT 'Undo large term ' || g \
             FROM generate_series(1,65) g RETURNING id) \
             INSERT INTO manifestation_genres(manifestation_id,genre_id) SELECT $1,id FROM inserted",
            large_edition,
        ).execute(&pool).await.unwrap();
        let legacy_fields = vec!["contributors.author".into(), "genres".into()];
        let original = captured_fields(&app, user, large_edition, large_work, &legacy_fields).await;
        let saved = browser_patch(
            &server,
            large_edition,
            &csrf,
            serde_json::json!({"contributors":{"author":["Replacement"]},"genres":null}),
        )
        .await;
        assert_eq!(saved.status_code(), StatusCode::OK, "{}", saved.text());
        for operation in saved.json::<serde_json::Value>()["operations"]
            .as_array()
            .unwrap()
        {
            let current = server
                .get(&format!("/api/v1/books/{large_edition}/metadata"))
                .await;
            let restored = browser_undo(
                &server,
                large_edition,
                &csrf,
                &operation["operation_id"],
                current.header(axum::http::header::ETAG),
            )
            .await;
            assert_eq!(
                restored.status_code(),
                StatusCode::OK,
                "{}",
                restored.text()
            );
        }
        assert_eq!(
            captured_fields(&app, user, large_edition, large_work, &legacy_fields).await,
            original
        );
        let publisher = browser_patch(
            &server,
            overflow_edition,
            &csrf,
            serde_json::json!({"publisher":"Small change"}),
        )
        .await;
        assert_eq!(
            publisher.status_code(),
            StatusCode::OK,
            "{}",
            publisher.text()
        );
        let operation_state =
            |edition: Uuid| {
                let pool = &pool;
                async move {
                    sqlx::query_scalar!(
                    "SELECT coalesce(jsonb_agg(to_jsonb(o) ORDER BY id),'[]'::jsonb) AS \"state!\" \
                     FROM metadata_undo_operations o WHERE manifestation_id=$1",edition,
                ).fetch_one(pool).await.unwrap()
                }
            };
        let before = review_mutation_state(&pool, overflow_edition).await;
        let operations = operation_state(overflow_edition).await;
        let refused = browser_patch(
            &server,
            overflow_edition,
            &csrf,
            serde_json::json!({"title":"Must roll back","contributors":{"author":["Replacement"]}}),
        )
        .await;
        assert_eq!(
            refused.status_code(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "{}",
            refused.text()
        );
        assert_eq!(review_mutation_state(&pool, overflow_edition).await, before);
        assert_eq!(operation_state(overflow_edition).await, operations);
        let old_description =
            serde_json::Value::String("x".repeat(super::undo::MAX_CAPTURE_BYTES / 2 - 1024 * 1024));
        let old_hash =
            crate::services::enrichment::value_hash::value_hash("description", &old_description);
        let version=sqlx::query_scalar!(
            "INSERT INTO metadata_versions(manifestation_id,source,field_name,new_value,value_hash,match_type,confidence_score) \
             VALUES ($1,'opf','description',$2,$3,'opf',0.5) RETURNING id",
            large_edition,&old_description,&old_hash,
        ).fetch_one(&ing).await.unwrap();
        sqlx::query!(
            "UPDATE works SET description=$2,description_version_id=$3 WHERE id=$1",
            large_work,
            old_description.as_str(),
            version
        )
        .execute(&ing)
        .await
        .unwrap();
        drop(old_description);
        let before = review_mutation_state(&pool, large_edition).await;
        let operations = operation_state(large_edition).await;
        let patch = serde_json::json!({"title":"Must roll back","description":"y".repeat(2 * 1024 * 1024 - 1024)});
        let refused = browser_patch(&server, large_edition, &csrf, patch.clone()).await;
        assert_eq!(
            refused.status_code(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "{}",
            refused.text()
        );
        assert_eq!(review_mutation_state(&pool, large_edition).await, before);
        assert_eq!(operation_state(large_edition).await, operations);
        drop(before);
        let (_, basic) =
            test_support::db::create_adult_and_basic_auth(&app, "undo-budget-basic").await;
        let basic_server = test_support::db::server_with_real_pools(&app, &ing);
        let path = format!("/api/v1/books/{large_edition}/metadata");
        let current = basic_server
            .get(&path)
            .add_header(AUTHORIZATION, basic.clone())
            .await;
        let saved = basic_server
            .patch(&path)
            .add_header(AUTHORIZATION, basic)
            .add_header(
                axum::http::header::IF_MATCH,
                current.header(axum::http::header::ETAG),
            )
            .json(&patch)
            .await;
        assert_eq!(saved.status_code(), StatusCode::OK, "{}", saved.text());
        assert_eq!(
            saved.json::<serde_json::Value>()["operations"],
            serde_json::json!([])
        );
        assert_eq!(operation_state(large_edition).await, operations);

        for invalid in ["123", "0306406153", "1234567890128", "97X0306406157"] {
            sqlx::query!(
                "UPDATE manifestations SET isbn_13 = $2 WHERE id = $1",
                stub,
                invalid
            )
            .execute(&ing)
            .await
            .unwrap();
            let valid = browser_patch(
                &server,
                stub,
                &csrf,
                serde_json::json!({"isbn_13":"9780306406157"}),
            )
            .await;
            assert_eq!(valid.status_code(), StatusCode::OK, "{}", valid.text());
            assert_eq!(
                valid.json::<serde_json::Value>()["operations"],
                serde_json::json!([])
            );
        }
    }

    // Data loss boundary: selective history, freshness, locks, timed rollback and single consumption.
    #[sqlx::test(migrations = "./migrations")]
    async fn metadata_undo_checks_field_history_and_locks(pool: sqlx::PgPool) {
        let app = test_support::db::app_pool_for(&pool).await;
        let ing = test_support::db::ingestion_pool_for(&pool).await;
        let (_, server, csrf) = undo_browser(&app, &ing, "undo-history").await;
        let (_, edition) =
            test_support::db::insert_work_and_manifestation(&ing, "undo-history").await;
        let path = format!("/api/v1/books/{edition}/metadata");
        let saved = browser_patch(
            &server,
            edition,
            &csrf,
            serde_json::json!({"title":"Saved","subtitle":"Saved subtitle"}),
        )
        .await;
        assert_eq!(saved.status_code(), StatusCode::OK, "{}", saved.text());
        let ops = saved.json::<serde_json::Value>();
        let title = ops["operations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|op| op["field_name"] == "title")
            .unwrap()["operation_id"]
            .clone();
        let subtitle = ops["operations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|op| op["field_name"] == "subtitle")
            .unwrap()["operation_id"]
            .clone();
        let unrelated = browser_patch(
            &server,
            edition,
            &csrf,
            serde_json::json!({"publisher":"Keep this"}),
        )
        .await;
        assert_eq!(unrelated.status_code(), StatusCode::OK);
        let baseline = review_mutation_state(&pool, edition).await;
        let stale = browser_undo(
            &server,
            edition,
            &csrf,
            &title,
            saved.header(axum::http::header::ETAG),
        )
        .await;
        assert_eq!(stale.status_code(), StatusCode::PRECONDITION_FAILED);
        assert_eq!(review_mutation_state(&pool, edition).await, baseline);
        let locked = server
            .post(&format!("/api/v1/manifestations/{edition}/metadata/lock"))
            .add_header(
                test_support::mail::csrf(&csrf).0,
                test_support::mail::csrf(&csrf).1,
            )
            .json(&serde_json::json!({"field_name":"title", "entity_type":"work"}))
            .await;
        assert_eq!(
            locked.status_code(),
            StatusCode::CREATED,
            "{}",
            locked.text()
        );
        let current = server.get(&path).await;
        let locked = browser_undo(
            &server,
            edition,
            &csrf,
            &title,
            current.header(axum::http::header::ETAG),
        )
        .await;
        assert_eq!(
            locked.status_code(),
            StatusCode::CONFLICT,
            "{}",
            locked.text()
        );
        assert_eq!(
            locked.json::<serde_json::Value>()["locked_fields"],
            serde_json::json!(["title"])
        );
        let unlocked = server
            .post(&format!("/api/v1/manifestations/{edition}/metadata/unlock"))
            .add_header(
                test_support::mail::csrf(&csrf).0,
                test_support::mail::csrf(&csrf).1,
            )
            .json(&serde_json::json!({"field_name":"title", "entity_type":"work"}))
            .await;
        assert_eq!(
            unlocked.status_code(),
            StatusCode::OK,
            "{}",
            unlocked.text()
        );
        let restored = browser_undo(
            &server,
            edition,
            &csrf,
            &title,
            current.header(axum::http::header::ETAG),
        )
        .await;
        assert_eq!(
            restored.status_code(),
            StatusCode::OK,
            "{}",
            restored.text()
        );
        assert_eq!(
            restored.json::<serde_json::Value>()["values"]["publisher"],
            "Keep this"
        );
        let twice = browser_undo(
            &server,
            edition,
            &csrf,
            &title,
            restored.header(axum::http::header::ETAG),
        )
        .await;
        assert_eq!(twice.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
        for value in ["Away", "Saved subtitle"] {
            let response = browser_patch(
                &server,
                edition,
                &csrf,
                serde_json::json!({"subtitle":value}),
            )
            .await;
            assert_eq!(response.status_code(), StatusCode::OK);
        }
        let current = server.get(&path).await;
        let invalidated = browser_undo(
            &server,
            edition,
            &csrf,
            &subtitle,
            current.header(axum::http::header::ETAG),
        )
        .await;
        assert_eq!(invalidated.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(
            invalidated.header(axum::http::header::CONTENT_TYPE),
            "application/problem+json"
        );
        assert!(
            invalidated.json::<serde_json::Value>()["type"]
                .as_str()
                .unwrap()
                .ends_with("undo-unavailable")
        );
        let saved = browser_patch(
            &server,
            edition,
            &csrf,
            serde_json::json!({"publisher":"A new publisher"}),
        )
        .await;
        assert_eq!(saved.status_code(), StatusCode::OK);
        let op: Uuid = serde_json::from_value(
            saved.json::<serde_json::Value>()["operations"][0]["operation_id"].clone(),
        )
        .unwrap();
        sqlx::query!("UPDATE metadata_versions SET confidence_score = 0.8 WHERE id = (SELECT publisher_version_id FROM manifestations WHERE id = $1)", edition).execute(&ing).await.unwrap();
        let current = server.get(&path).await;
        assert_eq!(
            browser_undo(
                &server,
                edition,
                &csrf,
                &serde_json::json!(op),
                current.header(axum::http::header::ETAG)
            )
            .await
            .status_code(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        let saved = browser_patch(
            &server,
            edition,
            &csrf,
            serde_json::json!({"description":"Description"}),
        )
        .await;
        assert_eq!(saved.status_code(), StatusCode::OK);
        let op: Uuid = serde_json::from_value(
            saved.json::<serde_json::Value>()["operations"][0]["operation_id"].clone(),
        )
        .unwrap();
        sqlx::query!("UPDATE metadata_undo_operations SET after_state = jsonb_set(after_state, '{state,value}', '\"divergent\"'::jsonb) WHERE id = $1", op).execute(&pool).await.unwrap();
        let before = review_mutation_state(&pool, edition).await;
        let conflict = browser_undo(
            &server,
            edition,
            &csrf,
            &serde_json::json!(op),
            saved.header(axum::http::header::ETAG),
        )
        .await;
        assert_eq!(
            conflict.status_code(),
            StatusCode::CONFLICT,
            "{}",
            conflict.text()
        );
        assert!(
            conflict.json::<serde_json::Value>()["type"]
                .as_str()
                .unwrap()
                .ends_with("undo-conflict")
        );
        assert_eq!(review_mutation_state(&pool, edition).await, before);
        assert!(
            sqlx::query_scalar!(
                "SELECT consumed_at FROM metadata_undo_operations WHERE id = $1",
                op
            )
            .fetch_one(&pool)
            .await
            .unwrap()
            .is_none()
        );

        let initial = browser_patch(
            &server,
            edition,
            &csrf,
            serde_json::json!({"contributors":{"editor":["Timeout original"]}}),
        )
        .await;
        assert_eq!(initial.status_code(), StatusCode::OK, "{}", initial.text());
        let saved = browser_patch(
            &server,
            edition,
            &csrf,
            serde_json::json!({"contributors":{"editor":["Timeout replacement"]}}),
        )
        .await;
        assert_eq!(saved.status_code(), StatusCode::OK, "{}", saved.text());
        let operation = saved.json::<serde_json::Value>()["operations"][0]["operation_id"].clone();
        let op_id: Uuid = serde_json::from_value(operation.clone()).unwrap();
        let before = review_mutation_state(&pool, edition).await;
        let mut blocker = pool.begin().await.unwrap();
        sqlx::query!("SELECT id FROM authors WHERE name='Timeout original' FOR UPDATE")
            .fetch_one(&mut *blocker)
            .await
            .unwrap();
        let timed_out = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            browser_undo(
                &server,
                edition,
                &csrf,
                &operation,
                saved.header(axum::http::header::ETAG),
            ),
        )
        .await
        .unwrap();
        assert!(
            timed_out.status_code().is_server_error(),
            "{}",
            timed_out.text()
        );
        blocker.rollback().await.unwrap();
        assert_eq!(review_mutation_state(&pool, edition).await, before);
        assert!(
            sqlx::query_scalar!(
                "SELECT consumed_at FROM metadata_undo_operations WHERE id=$1",
                op_id
            )
            .fetch_one(&pool)
            .await
            .unwrap()
            .is_none()
        );
        let retried = browser_undo(
            &server,
            edition,
            &csrf,
            &operation,
            saved.header(axum::http::header::ETAG),
        )
        .await;
        assert_eq!(retried.status_code(), StatusCode::OK, "{}", retried.text());
    }

    // Authorisation/private-data boundaries; REV-REQ-0056 child 403/no-write and Undo 428/no-write.
    #[sqlx::test(migrations = "./migrations")]
    async fn metadata_undo_binds_receipts_to_session(pool: sqlx::PgPool) {
        let app = test_support::db::app_pool_for(&pool).await;
        let ing = test_support::db::ingestion_pool_for(&pool).await;
        let (user, server, csrf) = undo_browser(&app, &ing, "undo-session").await;
        let (_, edition) =
            test_support::db::insert_work_and_manifestation(&ing, "undo-session").await;
        let (_, other_edition) =
            test_support::db::insert_work_and_manifestation(&ing, "undo-target").await;
        let path = format!("/api/v1/books/{edition}/metadata");
        let request = Uuid::new_v4();
        let saved = browser_patch(
            &server,
            edition,
            &csrf,
            serde_json::json!({"title":"Session title","request_id":request}),
        )
        .await;
        assert_eq!(saved.status_code(), StatusCode::OK, "{}", saved.text());
        assert_eq!(saved.header(axum::http::header::CACHE_CONTROL), "no-store");
        let op: Uuid = serde_json::from_value(
            saved.json::<serde_json::Value>()["operations"][0]["operation_id"].clone(),
        )
        .unwrap();
        let resolve = format!("{path}/operations/resolve");
        let receipt = server
            .post(&resolve)
            .add_header(
                test_support::mail::csrf(&csrf).0,
                test_support::mail::csrf(&csrf).1,
            )
            .json(&serde_json::json!({"request_id":request}))
            .await;
        assert_eq!(receipt.status_code(), StatusCode::OK, "{}", receipt.text());
        assert_eq!(receipt.json::<serde_json::Value>()["outcome"], "saved");
        let missing = server
            .post(&resolve)
            .add_header(
                test_support::mail::csrf(&csrf).0,
                test_support::mail::csrf(&csrf).1,
            )
            .json(&serde_json::json!({"request_id":Uuid::new_v4()}))
            .await;
        assert_eq!(missing.json::<serde_json::Value>()["outcome"], "unknown");
        let before = review_mutation_state(&pool, edition).await;
        let no_csrf = server
            .post(&format!("{path}/undo"))
            .json(&serde_json::json!({"operation_id":op}))
            .await;
        assert_eq!(no_csrf.status_code(), StatusCode::PRECONDITION_REQUIRED);
        let no_match = server
            .post(&format!("{path}/undo"))
            .add_header(
                test_support::mail::csrf(&csrf).0,
                test_support::mail::csrf(&csrf).1,
            )
            .json(&serde_json::json!({"operation_id":op}))
            .await;
        assert_eq!(no_match.status_code(), StatusCode::PRECONDITION_REQUIRED);
        let mut second = test_support::db::server_with_real_pools(&app, &ing);
        second.save_cookies();
        let second_csrf = test_support::mail::login(
            &second,
            "undo-session@example.test",
            test_support::mail::PASSWORD,
        )
        .await;
        let wrong_session = browser_undo(
            &second,
            edition,
            &second_csrf,
            &serde_json::json!(op),
            saved.header(axum::http::header::ETAG),
        )
        .await;
        assert_eq!(
            wrong_session.status_code(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        let current = server
            .get(&format!("/api/v1/books/{other_edition}/metadata"))
            .await;
        assert_eq!(
            browser_undo(
                &server,
                other_edition,
                &csrf,
                &serde_json::json!(op),
                current.header(axum::http::header::ETAG)
            )
            .await
            .status_code(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(review_mutation_state(&pool, edition).await, before);
        let child = test_support::db::create_adult_with_password(
            &app,
            "undo-child",
            "undo-child@example.test",
            test_support::mail::PASSWORD,
        )
        .await;
        sqlx::query!(
            "UPDATE users SET role = 'child', is_child = TRUE WHERE id = $1",
            child
        )
        .execute(&app)
        .await
        .unwrap();
        let shelf = test_support::db::create_shelf(&app, child, "undo-child").await;
        test_support::db::add_to_shelf(&app, shelf, edition).await;
        let mut child_server = test_support::db::server_with_real_pools(&app, &ing);
        child_server.save_cookies();
        let child_csrf = test_support::mail::login(
            &child_server,
            "undo-child@example.test",
            test_support::mail::PASSWORD,
        )
        .await;
        let child_result = browser_undo(
            &child_server,
            edition,
            &child_csrf,
            &serde_json::json!(op),
            saved.header(axum::http::header::ETAG),
        )
        .await;
        assert_eq!(child_result.status_code(), StatusCode::FORBIDDEN);
        assert_eq!(review_mutation_state(&pool, edition).await, before);
        let success = browser_undo(
            &server,
            edition,
            &csrf,
            &serde_json::json!(op),
            saved.header(axum::http::header::ETAG),
        )
        .await;
        assert_eq!(success.status_code(), StatusCode::OK, "{}", success.text());
        let receipt = server
            .post(&resolve)
            .add_header(
                test_support::mail::csrf(&csrf).0,
                test_support::mail::csrf(&csrf).1,
            )
            .json(&serde_json::json!({"operation_id":op}))
            .await;
        assert_eq!(receipt.json::<serde_json::Value>()["outcome"], "restored");
        let saved = browser_patch(
            &server,
            edition,
            &csrf,
            serde_json::json!({"publisher":"Expiring"}),
        )
        .await;
        assert_eq!(saved.status_code(), StatusCode::OK);
        let expiring: Uuid = serde_json::from_value(
            saved.json::<serde_json::Value>()["operations"][0]["operation_id"].clone(),
        )
        .unwrap();
        sqlx::query!("UPDATE metadata_undo_operations SET expires_at = clock_timestamp() - interval '1 second' WHERE id = $1",expiring).execute(&pool).await.unwrap();
        let before = review_mutation_state(&pool, edition).await;
        assert_eq!(
            browser_undo(
                &server,
                edition,
                &csrf,
                &serde_json::json!(expiring),
                saved.header(axum::http::header::ETAG)
            )
            .await
            .status_code(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(review_mutation_state(&pool, edition).await, before);
        let expired_receipt = server
            .post(&resolve)
            .add_header(
                test_support::mail::csrf(&csrf).0,
                test_support::mail::csrf(&csrf).1,
            )
            .json(&serde_json::json!({"operation_id":expiring}))
            .await;
        assert_eq!(expired_receipt.status_code(), StatusCode::OK);
        assert_eq!(
            expired_receipt.json::<serde_json::Value>()["outcome"],
            "unknown"
        );
        assert_eq!(
            sqlx::query_scalar!("SELECT reclaim_metadata_undo() AS \"removed!\"")
                .fetch_one(&app)
                .await
                .unwrap(),
            1
        );
        server
            .post("/auth/logout")
            .add_header(
                test_support::mail::csrf(&csrf).0,
                test_support::mail::csrf(&csrf).1,
            )
            .await
            .assert_status_no_content();
        assert_eq!(
            sqlx::query_scalar!(
                "SELECT count(*) AS \"count!\" FROM metadata_undo_operations WHERE user_id = $1",
                user
            )
            .fetch_one(&pool)
            .await
            .unwrap(),
            0
        );
    }

    async fn metadata_identity(pool: &sqlx::PgPool, edition: Uuid) -> serde_json::Value {
        sqlx::query_scalar!(
            "SELECT jsonb_build_object(\
                 'work', jsonb_build_object('revision', w.metadata_revision, 'fields', w.metadata_field_revisions), \
                 'manifestation', jsonb_build_object('revision', m.metadata_revision, 'fields', m.metadata_field_revisions)\
             ) AS \"identity!\" FROM manifestations m JOIN works w ON w.id = m.work_id WHERE m.id = $1",
            edition,
        )
        .fetch_one(pool)
        .await
        .unwrap()
    }

    fn assert_metadata_fields_advanced(
        before: &serde_json::Value,
        after: &serde_json::Value,
        work_fields: &[&str],
        manifestation_fields: &[&str],
    ) {
        for (owner, fields) in [
            ("work", work_fields),
            ("manifestation", manifestation_fields),
        ] {
            if fields.is_empty() {
                continue;
            }
            assert!(
                after[owner]["revision"].as_i64().unwrap()
                    > before[owner]["revision"].as_i64().unwrap()
            );
            for field in fields {
                assert!(
                    after[owner]["fields"][field].as_i64().unwrap()
                        > before[owner]["fields"][field].as_i64().unwrap_or(0),
                    "{owner}.{field}: {before} -> {after}"
                );
            }
        }
    }

    // Data loss boundary: canonical and referenced-attribution writers cannot retain stale field identities.
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
        assert_eq!(before["work"]["revision"], 0);
        assert_eq!(before["manifestation"]["revision"], 0);
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
        assert_metadata_fields_advanced(
            &before,
            &metadata_identity(&pool, edition).await,
            &[],
            &["work"],
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
        let work_fields = [
            "title",
            "subtitle",
            "description",
            "language",
            "contributors.author",
            "contributors.editor",
            "contributors.translator",
            "identifiers.work.openlibrary",
        ];
        let manifestation_fields = [
            "publisher",
            "pub_date",
            "isbn_10",
            "isbn_13",
            "pages",
            "content_rating",
            "genres",
            "moods",
            "tags",
            "identifiers.manifestation.asin",
        ];
        let after = metadata_identity(&pool, edition).await;
        assert_metadata_fields_advanced(&before, &after, &work_fields, &manifestation_fields);

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
        assert_metadata_fields_advanced(
            &after,
            &metadata_identity(&pool, edition).await,
            &["subtitle"],
            &[],
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
        let attributed_work = [
            "title",
            "contributors.author",
            "contributors.editor",
            "contributors.translator",
            "identifiers.work.openlibrary",
        ];
        let attributed_edition = [
            "publisher",
            "genres",
            "moods",
            "tags",
            "identifiers.manifestation.asin",
        ];
        let after = metadata_identity(&pool, edition).await;
        assert_metadata_fields_advanced(&before, &after, &attributed_work, &attributed_edition);
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
        assert_metadata_fields_advanced(&before, &after, &attributed_work, &attributed_edition);
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
        assert_metadata_fields_advanced(
            &before,
            &metadata_identity(&pool, edition).await,
            &attributed_work,
            &attributed_edition,
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
        assert_metadata_fields_advanced(
            &before,
            &metadata_identity(&pool, edition).await,
            &[
                "contributors.author",
                "contributors.editor",
                "contributors.translator",
                "identifiers.work.openlibrary",
            ],
            &["genres", "moods", "tags", "identifiers.manifestation.asin"],
        );

        assert_metadata_fields_advanced(
            &other_before,
            &metadata_identity(&pool, other_edition).await,
            &[
                "contributors.author",
                "contributors.editor",
                "contributors.translator",
                "identifiers.work.openlibrary",
            ],
            &["genres", "moods", "tags", "identifiers.manifestation.asin"],
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
        assert_metadata_fields_advanced(
            &before,
            &metadata_identity(&pool, edition).await,
            &["contributors.editor", "identifiers.work.openlibrary"],
            &["genres", "moods", "tags", "identifiers.manifestation.asin"],
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

        let r = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic)
            .json(&serde_json::json!({"title": format!("No If-Match {marker}")}))
            .await;
        test_support::assert_problem(
            &r,
            problems::IF_MATCH_REQUIRED,
            StatusCode::PRECONDITION_REQUIRED,
        );
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
