//! Metadata review endpoints.
//!
//! All routes require an authenticated non-child user.  Write paths open a
//! transaction, `SELECT ... FOR UPDATE` on the owning entity, apply the change,
//! and commit.

use std::collections::BTreeMap;

use axum::extract::{Path, State};
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
use crate::models::content_rating::ContentRating;
use crate::models::external_identifier::{
    IdentifierLevel, delete_manifestation_identifier, delete_work_identifier,
    upsert_manifestation_identifier, upsert_work_identifier,
};
use crate::models::work;
use crate::routes::etag::{hash_etag, if_match_mismatch, parse_if_match};
use crate::routes::library::{
    load_genres_for_manifestations, load_moods_for_manifestations, load_tags_for_manifestations,
};
use crate::services::enrichment::field_lock::{self, EntityType};
use crate::services::enrichment::value_hash;
use crate::services::metadata::{external_id, isbn};
use crate::state::AppState;

/// Build the metadata-review router.
///
/// Mounts the `GET` views over manifestation/work metadata and the
/// `POST` accept/reject/revert/lock/unlock mutators on `AppState`.
///
/// # Invariants
/// - Every handler requires an authenticated non-child user
///   (`CurrentUser::require_not_child`).
/// - Reads and writes acquire a connection via `db::acquire_with_rls`
///   so RLS policies see `app.current_user_id` for the caller.
/// - Write paths open a transaction, take `SELECT ... FOR UPDATE` on
///   the owning manifestation/work row, apply the change, and commit.
///
/// Why: the row-level lock serialises concurrent reviewers against the
/// same entity so accept/reject/revert can't race with each other or
/// with re-enrichment writes that mutate the same metadata row.
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
    tag = "metadata",
    security(("session_cookie" = ["read"]), ("device_token_bearer" = ["read"]), ("oidc_jwt_bearer" = ["read"]), ("opds_basic" = ["read"])),
    params(("id" = Uuid, Path, description = "Manifestation id")),
    responses(
        (status = 200, description = "Metadata version rows for the manifestation, newest first (empty when the manifestation is missing or RLS-hidden)", body = [MetadataRow]),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails),
        (status = 403, description = "Caller is a child account", body = crate::openapi::ProblemDetails)
    )
)]
async fn get_manifestation_metadata(
    current_user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
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
    tag = "metadata",
    security(("session_cookie" = ["read"]), ("device_token_bearer" = ["read"]), ("oidc_jwt_bearer" = ["read"]), ("opds_basic" = ["read"])),
    params(("id" = Uuid, Path, description = "Work id")),
    responses(
        (status = 200, description = "Metadata version rows across the work's manifestations, newest first (empty when the work is missing or RLS-hidden)", body = [MetadataRow]),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails),
        (status = 403, description = "Caller is a child account", body = crate::openapi::ProblemDetails)
    )
)]
async fn get_work_metadata(
    current_user: CurrentUser,
    State(state): State<AppState>,
    Path(work_id): Path<Uuid>,
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
    version_id: Uuid,
}

/// Body for revert.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
struct RevertPayload {
    /// Canonical field to revert (e.g. `title`).
    field_name: String,
    /// Version to restore; `null` clears the canonical pointer AND the
    /// canonical column.
    version_id: Option<Uuid>,
}

/// Body for lock / unlock.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
struct LockPayload {
    /// Field to lock or unlock (e.g. `title`).
    field_name: String,
    /// Entity the lock applies to: `work` or `manifestation`.
    #[schema(example = "manifestation")]
    entity_type: String,
}

/// `POST /api/v1/manifestations/{id}/metadata/accept` — promote a pending
/// metadata version to canonical.
///
/// # Errors
/// - [`AppError::Forbidden`] when the caller is a child account.
/// - [`AppError::NotFound`] when the version row does not belong to the
///   manifestation or is RLS-hidden.
/// - [`AppError::Validation`] when the stored value fails field parsing.
/// - [`AppError::Internal`] on database errors.
#[utoipa::path(
    post,
    path = "/api/v1/manifestations/{id}/metadata/accept",
    tag = "metadata",
    security(("session_cookie" = ["write"]), ("device_token_bearer" = ["write"]), ("oidc_jwt_bearer" = ["write"]), ("opds_basic" = ["write"])),
    params(("id" = Uuid, Path, description = "Manifestation id")),
    request_body = VersionPayload,
    responses(
        (status = 200, description = "Version promoted to canonical; accepted ISBN changes may re-match the work"),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails),
        (status = 403, description = "Caller is a child account", body = crate::openapi::ProblemDetails),
        (status = 404, description = "Version not found for this manifestation, or RLS-hidden", body = crate::openapi::ProblemDetails),
        (status = 422, description = "Stored value fails field parsing", body = crate::openapi::ProblemDetails)
    )
)]
async fn accept_manifestation(
    current_user: CurrentUser,
    State(state): State<AppState>,
    Path(manifestation_id): Path<Uuid>,
    axum::Json(payload): axum::Json<VersionPayload>,
) -> Result<impl IntoResponse, AppError> {
    current_user.require_scope(Scope::Write)?;
    current_user.require_not_child()?;

    let mut tx = db::acquire_with_rls(&state.pool, current_user.user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let row = sqlx::query!(
        "SELECT mv.id, mv.field_name, \
                mv.new_value AS \"new_value!\", \
                m.work_id \
         FROM metadata_versions mv \
         JOIN manifestations m ON m.id = mv.manifestation_id \
         JOIN works w ON w.id = m.work_id \
         WHERE mv.id = $1 AND mv.manifestation_id = $2 \
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

    // Accept promotes the version to canonical; the caller has no use for
    // the prior pointer (that's the revert/undo surface's concern).
    let _ = apply_version(
        &mut tx,
        &field_name,
        &new_value,
        version_id,
        manifestation_id,
        work_id,
    )
    .await?;

    // Accepted ISBN changes can trigger auto-merge; match orchestrator behaviour.
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
    tag = "metadata",
    security(("session_cookie" = ["write"]), ("device_token_bearer" = ["write"]), ("oidc_jwt_bearer" = ["write"]), ("opds_basic" = ["write"])),
    params(("id" = Uuid, Path, description = "Manifestation id")),
    request_body = VersionPayload,
    responses(
        (status = 200, description = "Version marked rejected"),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails),
        (status = 403, description = "Caller is a child account", body = crate::openapi::ProblemDetails),
        (status = 404, description = "Version not found for this manifestation, or RLS-hidden", body = crate::openapi::ProblemDetails)
    )
)]
async fn reject_manifestation(
    current_user: CurrentUser,
    State(state): State<AppState>,
    Path(manifestation_id): Path<Uuid>,
    axum::Json(payload): axum::Json<VersionPayload>,
) -> Result<impl IntoResponse, AppError> {
    current_user.require_scope(Scope::Write)?;
    current_user.require_not_child()?;

    let mut tx = db::acquire_with_rls(&state.pool, current_user.user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

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
    tag = "metadata",
    security(("session_cookie" = ["write"]), ("device_token_bearer" = ["write"]), ("oidc_jwt_bearer" = ["write"]), ("opds_basic" = ["write"])),
    params(("id" = Uuid, Path, description = "Manifestation id")),
    request_body = RevertPayload,
    responses(
        (status = 200, description = "Field reverted to the given version, or cleared when version_id is null"),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails),
        (status = 403, description = "Caller is a child account", body = crate::openapi::ProblemDetails),
        (status = 404, description = "Manifestation or version missing, or RLS-hidden", body = crate::openapi::ProblemDetails),
        (status = 422, description = "Field cannot be cleared or stored value fails parsing", body = crate::openapi::ProblemDetails)
    )
)]
async fn revert_manifestation(
    current_user: CurrentUser,
    State(state): State<AppState>,
    Path(manifestation_id): Path<Uuid>,
    axum::Json(payload): axum::Json<RevertPayload>,
) -> Result<impl IntoResponse, AppError> {
    current_user.require_scope(Scope::Write)?;
    current_user.require_not_child()?;

    let mut tx = db::acquire_with_rls(&state.pool, current_user.user_id)
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
            let _ = apply_version(
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
            let _ = clear_field(&mut tx, &payload.field_name, manifestation_id, work_id).await?;
        }
    }

    tx.commit()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    Ok(StatusCode::OK)
}

/// `POST /api/v1/manifestations/{id}/metadata/lock` — lock a field
/// against future enrichment writes.
///
/// # Errors
/// - [`AppError::Forbidden`] when the caller is a child account.
/// - [`AppError::Validation`] when `entity_type` is not `work` /
///   `manifestation`.
/// - [`AppError::Internal`] on database errors.
#[utoipa::path(
    post,
    path = "/api/v1/manifestations/{id}/metadata/lock",
    tag = "metadata",
    security(("session_cookie" = ["write"]), ("device_token_bearer" = ["write"]), ("oidc_jwt_bearer" = ["write"]), ("opds_basic" = ["write"])),
    params(("id" = Uuid, Path, description = "Manifestation id")),
    request_body = LockPayload,
    responses(
        (status = 201, description = "Lock recorded (idempotent)"),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails),
        (status = 403, description = "Caller is a child account", body = crate::openapi::ProblemDetails),
        (status = 422, description = "Unknown entity_type", body = crate::openapi::ProblemDetails)
    )
)]
async fn lock_field(
    current_user: CurrentUser,
    State(state): State<AppState>,
    Path(manifestation_id): Path<Uuid>,
    axum::Json(payload): axum::Json<LockPayload>,
) -> Result<impl IntoResponse, AppError> {
    current_user.require_scope(Scope::Write)?;
    current_user.require_not_child()?;
    let entity = parse_entity(&payload.entity_type)?;
    field_lock::lock(
        &state.pool,
        manifestation_id,
        entity,
        &payload.field_name,
        current_user.user_id,
    )
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    Ok(StatusCode::CREATED)
}

/// `POST /api/v1/manifestations/{id}/metadata/unlock` — remove a field
/// lock.
///
/// # Errors
/// - [`AppError::Forbidden`] when the caller is a child account.
/// - [`AppError::NotFound`] when no matching lock exists.
/// - [`AppError::Validation`] when `entity_type` is not `work` /
///   `manifestation`.
/// - [`AppError::Internal`] on database errors.
#[utoipa::path(
    post,
    path = "/api/v1/manifestations/{id}/metadata/unlock",
    tag = "metadata",
    security(("session_cookie" = ["write"]), ("device_token_bearer" = ["write"]), ("oidc_jwt_bearer" = ["write"]), ("opds_basic" = ["write"])),
    params(("id" = Uuid, Path, description = "Manifestation id")),
    request_body = LockPayload,
    responses(
        (status = 200, description = "Lock removed"),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails),
        (status = 403, description = "Caller is a child account", body = crate::openapi::ProblemDetails),
        (status = 404, description = "No matching lock", body = crate::openapi::ProblemDetails),
        (status = 422, description = "Unknown entity_type", body = crate::openapi::ProblemDetails)
    )
)]
async fn unlock_field(
    current_user: CurrentUser,
    State(state): State<AppState>,
    Path(manifestation_id): Path<Uuid>,
    axum::Json(payload): axum::Json<LockPayload>,
) -> Result<impl IntoResponse, AppError> {
    current_user.require_scope(Scope::Write)?;
    current_user.require_not_child()?;
    let entity = parse_entity(&payload.entity_type)?;
    let removed = field_lock::unlock(&state.pool, manifestation_id, entity, &payload.field_name)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    if !removed {
        return Err(AppError::NotFound);
    }
    Ok(StatusCode::OK)
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

/// Apply a specific version to its canonical column + pointer.
/// Reused by `/accept` and `/revert`.
///
/// Returns the pointer that was canonical for this field before the write
/// (`None` when the field was previously unset, or when the field kind has
/// no single-pointer concept, e.g. the vocabulary junctions).
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
) -> Result<Option<Uuid>, AppError> {
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
    let str_val = value
        .as_str()
        .map_or_else(|| value.to_string(), str::to_owned);
    let previous_version_id: Option<Uuid> = match field {
        "title" => sqlx::query_scalar!(
            "WITH old AS (SELECT title_version_id FROM works WHERE id = $3) \
             UPDATE works \
             SET title = $1, sort_title = lower($1), title_version_id = $2 \
             WHERE id = $3 \
             RETURNING (SELECT title_version_id FROM old) AS previous_version_id",
            str_val,
            version_id,
            work_id,
        )
        .fetch_one(&mut **tx)
        .await
        .map_err(|e| AppError::Internal(e.into()))?,
        "description" => sqlx::query_scalar!(
            "WITH old AS (SELECT description_version_id FROM works WHERE id = $3) \
             UPDATE works SET description = $1, description_version_id = $2 \
             WHERE id = $3 \
             RETURNING (SELECT description_version_id FROM old) AS previous_version_id",
            str_val,
            version_id,
            work_id,
        )
        .fetch_one(&mut **tx)
        .await
        .map_err(|e| AppError::Internal(e.into()))?,
        "language" => sqlx::query_scalar!(
            "WITH old AS (SELECT language_version_id FROM works WHERE id = $3) \
             UPDATE works SET language = $1, language_version_id = $2 \
             WHERE id = $3 \
             RETURNING (SELECT language_version_id FROM old) AS previous_version_id",
            str_val,
            version_id,
            work_id,
        )
        .fetch_one(&mut **tx)
        .await
        .map_err(|e| AppError::Internal(e.into()))?,
        "subtitle" => sqlx::query_scalar!(
            "WITH old AS (SELECT subtitle_version_id FROM works WHERE id = $3) \
             UPDATE works SET subtitle = $1, subtitle_version_id = $2 \
             WHERE id = $3 \
             RETURNING (SELECT subtitle_version_id FROM old) AS previous_version_id",
            str_val,
            version_id,
            work_id,
        )
        .fetch_one(&mut **tx)
        .await
        .map_err(|e| AppError::Internal(e.into()))?,
        "pages" => {
            let pages = value
                .as_i64()
                .and_then(|n| i32::try_from(n).ok())
                .filter(|n| *n > 0)
                .ok_or_else(|| AppError::Validation("pages must be a positive integer".into()))?;
            sqlx::query_scalar!(
                "WITH old AS (SELECT pages_version_id FROM manifestations WHERE id = $3) \
                 UPDATE manifestations SET pages = $1, pages_version_id = $2 \
                 WHERE id = $3 \
                 RETURNING (SELECT pages_version_id FROM old) AS previous_version_id",
                pages,
                version_id,
                manifestation_id,
            )
            .fetch_one(&mut **tx)
            .await
            .map_err(|e| AppError::Internal(e.into()))?
        }
        "publisher" => sqlx::query_scalar!(
            "WITH old AS (SELECT publisher_version_id FROM manifestations WHERE id = $3) \
             UPDATE manifestations \
             SET publisher = $1, publisher_version_id = $2 \
             WHERE id = $3 \
             RETURNING (SELECT publisher_version_id FROM old) AS previous_version_id",
            str_val,
            version_id,
            manifestation_id,
        )
        .fetch_one(&mut **tx)
        .await
        .map_err(|e| AppError::Internal(e.into()))?,
        "isbn_10" => sqlx::query_scalar!(
            "WITH old AS (SELECT isbn_10_version_id FROM manifestations WHERE id = $3) \
             UPDATE manifestations \
             SET isbn_10 = $1, isbn_10_version_id = $2 \
             WHERE id = $3 \
             RETURNING (SELECT isbn_10_version_id FROM old) AS previous_version_id",
            str_val,
            version_id,
            manifestation_id,
        )
        .fetch_one(&mut **tx)
        .await
        .map_err(|e| AppError::Internal(e.into()))?,
        "isbn_13" => sqlx::query_scalar!(
            "WITH old AS (SELECT isbn_13_version_id FROM manifestations WHERE id = $3) \
             UPDATE manifestations \
             SET isbn_13 = $1, isbn_13_version_id = $2 \
             WHERE id = $3 \
             RETURNING (SELECT isbn_13_version_id FROM old) AS previous_version_id",
            str_val,
            version_id,
            manifestation_id,
        )
        .fetch_one(&mut **tx)
        .await
        .map_err(|e| AppError::Internal(e.into()))?,
        "pub_date" => {
            let date = parse_iso_date(&str_val)
                .map_err(|e| AppError::Validation(format!("invalid pub_date: {e}")))?;
            sqlx::query_scalar!(
                "WITH old AS (SELECT pub_date_version_id FROM manifestations WHERE id = $3) \
                 UPDATE manifestations \
                 SET pub_date = $1, pub_date_version_id = $2 \
                 WHERE id = $3 \
                 RETURNING (SELECT pub_date_version_id FROM old) AS previous_version_id",
                date,
                version_id,
                manifestation_id,
            )
            .fetch_one(&mut **tx)
            .await
            .map_err(|e| AppError::Internal(e.into()))?
        }
        "content_rating" => {
            let rating: ContentRating = serde_json::from_value(value.clone()).map_err(|_| {
                AppError::Validation(
                    "content_rating must be one of everyone, teen, mature, adult, explicit".into(),
                )
            })?;
            sqlx::query_scalar!(
                "WITH old AS (SELECT content_rating_version_id FROM manifestations WHERE id = $3) \
                 UPDATE manifestations \
                 SET content_rating = $1, content_rating_version_id = $2 \
                 WHERE id = $3 \
                 RETURNING (SELECT content_rating_version_id FROM old) AS previous_version_id",
                rating as ContentRating,
                version_id,
                manifestation_id,
            )
            .fetch_one(&mut **tx)
            .await
            .map_err(|e| AppError::Internal(e.into()))?
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
            let previous_version_id = capture_role_pointer(tx, work_id, role).await?;
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
            previous_version_id
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
                    let previous: Option<Uuid> = sqlx::query_scalar!(
                        "SELECT source_version_id FROM work_external_identifiers \
                         WHERE work_id = $1 AND scheme = $2",
                        work_id,
                        scheme,
                    )
                    .fetch_optional(&mut **tx)
                    .await
                    .map_err(|e| AppError::Internal(e.into()))?
                    .flatten();
                    upsert_work_identifier(
                        &mut **tx,
                        work_id,
                        scheme,
                        &canonical,
                        Some(version_id),
                    )
                    .await
                    .map_err(|e| AppError::Internal(e.into()))?;
                    previous
                }
                IdentifierLevel::Manifestation => {
                    let previous: Option<Uuid> = sqlx::query_scalar!(
                        "SELECT source_version_id FROM manifestation_external_identifiers \
                         WHERE manifestation_id = $1 AND scheme = $2",
                        manifestation_id,
                        scheme,
                    )
                    .fetch_optional(&mut **tx)
                    .await
                    .map_err(|e| AppError::Internal(e.into()))?
                    .flatten();
                    upsert_manifestation_identifier(
                        &mut **tx,
                        manifestation_id,
                        scheme,
                        &canonical,
                        Some(version_id),
                    )
                    .await
                    .map_err(|e| AppError::Internal(e.into()))?;
                    previous
                }
            }
        }
        other => {
            return Err(AppError::Validation(format!(
                "unsupported auto-apply field '{other}' (list/complex fields must be accepted via their dedicated routes)"
            )));
        }
    };
    // External identifiers are never written back to the source file, so an
    // identifier apply (manual set, accept, or revert) must not enqueue an
    // OPF writeback job.
    if !field.starts_with("identifiers.") {
        enqueue_writeback(tx, manifestation_id, field).await?;
    }
    Ok(previous_version_id)
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
        "INSERT INTO writeback_jobs (manifestation_id, reason) VALUES ($1, $2)",
        manifestation_id,
        reason,
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
) -> Result<Option<Uuid>, AppError> {
    let previous_version_id: Option<Uuid> = match field {
        "title" => {
            return Err(AppError::Validation(
                "cannot clear title — revert to a specific version instead".into(),
            ));
        }
        "description" => sqlx::query_scalar!(
            "WITH old AS (SELECT description_version_id FROM works WHERE id = $1) \
             UPDATE works SET description = NULL, description_version_id = NULL \
             WHERE id = $1 \
             RETURNING (SELECT description_version_id FROM old) AS previous_version_id",
            work_id,
        )
        .fetch_one(&mut **tx)
        .await
        .map_err(|e| AppError::Internal(e.into()))?,
        "language" => sqlx::query_scalar!(
            "WITH old AS (SELECT language_version_id FROM works WHERE id = $1) \
             UPDATE works SET language = NULL, language_version_id = NULL \
             WHERE id = $1 \
             RETURNING (SELECT language_version_id FROM old) AS previous_version_id",
            work_id,
        )
        .fetch_one(&mut **tx)
        .await
        .map_err(|e| AppError::Internal(e.into()))?,
        "subtitle" => sqlx::query_scalar!(
            "WITH old AS (SELECT subtitle_version_id FROM works WHERE id = $1) \
             UPDATE works SET subtitle = NULL, subtitle_version_id = NULL \
             WHERE id = $1 \
             RETURNING (SELECT subtitle_version_id FROM old) AS previous_version_id",
            work_id,
        )
        .fetch_one(&mut **tx)
        .await
        .map_err(|e| AppError::Internal(e.into()))?,
        "pages" => sqlx::query_scalar!(
            "WITH old AS (SELECT pages_version_id FROM manifestations WHERE id = $1) \
             UPDATE manifestations SET pages = NULL, pages_version_id = NULL \
             WHERE id = $1 \
             RETURNING (SELECT pages_version_id FROM old) AS previous_version_id",
            manifestation_id,
        )
        .fetch_one(&mut **tx)
        .await
        .map_err(|e| AppError::Internal(e.into()))?,
        "publisher" => sqlx::query_scalar!(
            "WITH old AS (SELECT publisher_version_id FROM manifestations WHERE id = $1) \
             UPDATE manifestations \
             SET publisher = NULL, publisher_version_id = NULL \
             WHERE id = $1 \
             RETURNING (SELECT publisher_version_id FROM old) AS previous_version_id",
            manifestation_id,
        )
        .fetch_one(&mut **tx)
        .await
        .map_err(|e| AppError::Internal(e.into()))?,
        "pub_date" => sqlx::query_scalar!(
            "WITH old AS (SELECT pub_date_version_id FROM manifestations WHERE id = $1) \
             UPDATE manifestations \
             SET pub_date = NULL, pub_date_version_id = NULL \
             WHERE id = $1 \
             RETURNING (SELECT pub_date_version_id FROM old) AS previous_version_id",
            manifestation_id,
        )
        .fetch_one(&mut **tx)
        .await
        .map_err(|e| AppError::Internal(e.into()))?,
        "isbn_10" => sqlx::query_scalar!(
            "WITH old AS (SELECT isbn_10_version_id FROM manifestations WHERE id = $1) \
             UPDATE manifestations \
             SET isbn_10 = NULL, isbn_10_version_id = NULL \
             WHERE id = $1 \
             RETURNING (SELECT isbn_10_version_id FROM old) AS previous_version_id",
            manifestation_id,
        )
        .fetch_one(&mut **tx)
        .await
        .map_err(|e| AppError::Internal(e.into()))?,
        "isbn_13" => sqlx::query_scalar!(
            "WITH old AS (SELECT isbn_13_version_id FROM manifestations WHERE id = $1) \
             UPDATE manifestations \
             SET isbn_13 = NULL, isbn_13_version_id = NULL \
             WHERE id = $1 \
             RETURNING (SELECT isbn_13_version_id FROM old) AS previous_version_id",
            manifestation_id,
        )
        .fetch_one(&mut **tx)
        .await
        .map_err(|e| AppError::Internal(e.into()))?,
        "content_rating" => sqlx::query_scalar!(
            "WITH old AS (SELECT content_rating_version_id FROM manifestations WHERE id = $1) \
             UPDATE manifestations \
             SET content_rating = NULL, content_rating_version_id = NULL \
             WHERE id = $1 \
             RETURNING (SELECT content_rating_version_id FROM old) AS previous_version_id",
            manifestation_id,
        )
        .fetch_one(&mut **tx)
        .await
        .map_err(|e| AppError::Internal(e.into()))?,
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
                    let previous_version_id = capture_role_pointer(tx, work_id, role).await?;
                    delete_role_rows(tx, work_id, role).await?;
                    previous_version_id
                }
                other => {
                    return Err(AppError::Validation(format!(
                        "unsupported contributor role '{other}'"
                    )));
                }
            }
        }
        _ if field.starts_with("identifiers.") => {
            let (level, scheme) = parse_identifier_field(field)?;
            match level {
                IdentifierLevel::Work => {
                    let previous: Option<Uuid> = sqlx::query_scalar!(
                        "SELECT source_version_id FROM work_external_identifiers \
                         WHERE work_id = $1 AND scheme = $2",
                        work_id,
                        scheme,
                    )
                    .fetch_optional(&mut **tx)
                    .await
                    .map_err(|e| AppError::Internal(e.into()))?
                    .flatten();
                    delete_work_identifier(&mut **tx, work_id, scheme)
                        .await
                        .map_err(|e| AppError::Internal(e.into()))?;
                    previous
                }
                IdentifierLevel::Manifestation => {
                    let previous: Option<Uuid> = sqlx::query_scalar!(
                        "SELECT source_version_id FROM manifestation_external_identifiers \
                         WHERE manifestation_id = $1 AND scheme = $2",
                        manifestation_id,
                        scheme,
                    )
                    .fetch_optional(&mut **tx)
                    .await
                    .map_err(|e| AppError::Internal(e.into()))?
                    .flatten();
                    delete_manifestation_identifier(&mut **tx, manifestation_id, scheme)
                        .await
                        .map_err(|e| AppError::Internal(e.into()))?;
                    previous
                }
            }
        }
        other => {
            return Err(AppError::Validation(format!("unsupported field '{other}'")));
        }
    };
    // Identifier clears mirror identifier applies: no OPF writeback.
    if !field.starts_with("identifiers.") {
        enqueue_writeback(tx, manifestation_id, field).await?;
    }
    Ok(previous_version_id)
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

/// Capture the version pointer a contributor role's `work_authors` rows
/// currently carry, before a rebuild replaces them.
///
/// Returns `Some(id)` only when every row for the role stamps the same
/// `source_version_id`; an empty role or a role whose rows disagree (mixed
/// stamps, e.g. from edits made before this column was wired) has no single
/// "previous version" to revert to, so both cases return `None`.
async fn capture_role_pointer(
    tx: &mut Transaction<'_, Postgres>,
    work_id: Uuid,
    role: &str,
) -> Result<Option<Uuid>, AppError> {
    let distinct: Vec<Option<Uuid>> = sqlx::query_scalar!(
        "SELECT DISTINCT source_version_id FROM work_authors \
         WHERE work_id = $1 AND role = ($2::text)::author_role",
        work_id,
        role,
    )
    .fetch_all(&mut **tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    match distinct.as_slice() {
        [single] => Ok(*single),
        _ => Ok(None),
    }
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
) -> Result<BTreeMap<String, FieldVersionChange>, AppError> {
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
    let mut changes: BTreeMap<String, FieldVersionChange> = BTreeMap::new();

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
        // Captured before `delete_role_rows` clears the role's `work_authors`
        // rows, mirroring the `apply_version` contributors arm so a manual
        // PATCH's undo pointer is as reliable as the accept/revert path's.
        let previous_version_id = capture_role_pointer(tx, work_id, role).await?;
        let (value, version_id) = match names {
            None => {
                let version_id =
                    insert_manual_version(tx, manifestation_id, user_id, &key, &Value::Null)
                        .await?;
                delete_role_rows(tx, work_id, role).await?;
                (None, version_id)
            }
            Some(names) => {
                let trimmed = validated_role_names(role, &names)?;
                let json =
                    serde_json::to_value(&trimmed).map_err(|e| AppError::Internal(e.into()))?;
                let version_id =
                    insert_manual_version(tx, manifestation_id, user_id, &key, &json).await?;
                delete_role_rows(tx, work_id, role).await?;
                insert_role_rows(tx, work_id, role, &trimmed, version_id).await?;
                (Some(json), version_id)
            }
        };
        changes.insert(
            key,
            FieldVersionChange {
                value,
                version_id: Some(version_id),
                previous_version_id,
            },
        );
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
    Ok(changes)
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
/// terms too, with last-writer-wins display casing.
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
                     ON CONFLICT ((lower(name))) DO UPDATE SET name = EXCLUDED.name \
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
                     ON CONFLICT ((lower(name))) DO UPDATE SET name = EXCLUDED.name \
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
                     ON CONFLICT ((lower(name))) DO UPDATE SET name = EXCLUDED.name \
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
) -> Result<FieldVersionChange, AppError> {
    let key = field.field_name();
    // Empty array ≡ null: both clear the field.
    let names = maybe_names.filter(|v| !v.is_empty());
    let (value, version_id) = match names {
        None => {
            let version_id =
                insert_manual_version(tx, manifestation_id, user_id, key, &Value::Null).await?;
            delete_vocabulary_rows(tx, manifestation_id, field).await?;
            (None, version_id)
        }
        Some(names) => {
            let trimmed = validated_vocabulary_terms(key, &names)?;
            let json = serde_json::to_value(&trimmed).map_err(|e| AppError::Internal(e.into()))?;
            let version_id =
                insert_manual_version(tx, manifestation_id, user_id, key, &json).await?;
            delete_vocabulary_rows(tx, manifestation_id, field).await?;
            insert_vocabulary_rows(tx, manifestation_id, field, &trimmed, version_id).await?;
            (Some(json), version_id)
        }
    };
    enqueue_writeback(tx, manifestation_id, key).await?;
    Ok(FieldVersionChange {
        value,
        version_id: Some(version_id),
        previous_version_id: None,
    })
}

// ── manual metadata edit (RFC 7396 JSON Merge Patch) ──────────────────────

/// Per-field sparse update body for `PATCH /api/v1/books/{id}/metadata`.
///
/// Encodes RFC 7396 JSON Merge Patch semantics via
/// `serde_with::rust::double_option` (per
/// `adr/2026-05-22-backend-aux-crates.md`):
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
    /// New publication date (`YYYY-MM-DD`). Absent = unchanged; `null`
    /// clears.
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
    #[schema(value_type = Option<BTreeMap<String, Option<String>>>)]
    work_identifiers: Option<BTreeMap<String, Option<String>>>,
    /// Manifestation-level external identifiers keyed by scheme (e.g.
    /// `{"googlebooks": "zyTZAAAAYAAJ"}`). Same per-entry set/clear semantics
    /// as `work_identifiers`.
    #[serde(default)]
    #[schema(value_type = Option<BTreeMap<String, Option<String>>>)]
    manifestation_identifiers: Option<BTreeMap<String, Option<String>>>,
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

/// One field's outcome from `PATCH /api/v1/books/{id}/metadata`.
#[derive(Debug, Serialize, utoipa::ToSchema)]
struct FieldVersionChange {
    /// Canonical value as applied, after server normalization (null when the
    /// field was cleared).
    value: Option<serde_json::Value>,
    /// Journal row now wired as canonical (null when the field was cleared
    /// to no version).
    version_id: Option<Uuid>,
    /// Version pointer that was canonical before this patch (null when the
    /// field was previously unset, or when no pointer exists for the field
    /// kind).
    previous_version_id: Option<Uuid>,
}

/// Response body for `PATCH /api/v1/books/{id}/metadata`.
#[derive(Debug, Serialize, utoipa::ToSchema)]
struct UpdateMetadataResponse {
    /// Keyed by patched field name, including `contributors.<role>` keys.
    fields: BTreeMap<String, FieldVersionChange>,
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
    /// Publication date (`YYYY-MM-DD`); `null` when unset.
    pub_date: Option<String>,
    /// ISBN-10; `null` when unset.
    isbn_10: Option<String>,
    /// ISBN-13; `null` when unset.
    isbn_13: Option<String>,
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
    work_identifiers: BTreeMap<String, String>,
    /// Manifestation-level external identifiers keyed by scheme.
    manifestation_identifiers: BTreeMap<String, String>,
}

/// Load the editable metadata span for one manifestation, in exactly the
/// shape [`get_book_metadata`] serves and [`update_book_metadata`] hashes.
/// Sharing this assembly means the precondition check, the post-write
/// `ETag`, and the matched `GET` all hash the identical representation
/// through one code path rather than three independently maintained ones.
///
/// `Ok(None)` when the manifestation or its parent work does not exist.
async fn load_book_metadata(
    tx: &mut Transaction<'_, Postgres>,
    manifestation_id: Uuid,
) -> Result<Option<BookMetadata>, AppError> {
    let Some(core) = sqlx::query!(
        r#"
        SELECT w.id             AS "work_id!",
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

    Ok(Some(BookMetadata {
        title: core.title,
        subtitle: core.subtitle,
        description: core.description,
        language: core.language,
        publisher: core.publisher,
        pub_date: core.pub_date.map(|d| d.format("%Y-%m-%d").to_string()),
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
    }))
}

/// `GET /api/v1/books/{id}/metadata` — the editable metadata span for one
/// manifestation: the matched-pair read for [`update_book_metadata`], same
/// URI and id key, so a read-modify-write flow targets one resource.
///
/// # Errors
/// - [`AppError::Forbidden`] when the caller is a child account.
/// - [`AppError::NotFound`] when the manifestation is missing or hidden by
///   RLS for the current user (existence-not-leaked).
/// - [`AppError::Internal`] on database errors.
#[utoipa::path(
    get,
    path = "/api/v1/books/{id}/metadata",
    tag = "metadata",
    security(("session_cookie" = ["read"]), ("device_token_bearer" = ["read"]), ("oidc_jwt_bearer" = ["read"]), ("opds_basic" = ["read"])),
    params(("id" = Uuid, Path, description = "Manifestation id")),
    responses(
        (status = 200, description = "The editable metadata span, matching what PATCH accepts", body = BookMetadata,
         headers(("ETag" = String, description = "Strong entity-tag hashing this response body. Echo as If-Match on PATCH /api/v1/books/{id}/metadata"))),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails),
        (status = 403, description = "Caller is a child account", body = crate::openapi::ProblemDetails),
        (status = 404, description = "Manifestation missing or RLS-hidden (existence-not-leaked)", body = crate::openapi::ProblemDetails)
    )
)]
async fn get_book_metadata(
    current_user: CurrentUser,
    State(state): State<AppState>,
    Path(manifestation_id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    current_user.require_not_child()?;

    let mut tx = db::acquire_with_rls(&state.pool, current_user.user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let metadata = load_book_metadata(&mut tx, manifestation_id)
        .await?
        .ok_or(AppError::NotFound)?;
    let etag = hash_etag(&metadata)?;

    tx.commit()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let mut headers = HeaderMap::new();
    headers.insert(ETAG, etag);
    Ok((headers, axum::Json(metadata)))
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
/// applied (normalized) value plus the new and previous version pointers
/// per field, so a caller can offer undo without a follow-up read.
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
    tag = "metadata",
    security(("session_cookie" = ["write"]), ("device_token_bearer" = ["write"]), ("oidc_jwt_bearer" = ["write"]), ("opds_basic" = ["write"])),
    params(
        ("id" = Uuid, Path, description = "Manifestation id"),
        ("If-Match" = String, Header, description = "Exactly one quoted strong entity-tag, as returned in a prior GET or PATCH response's ETag header. Opaque content follows the full RFC 9110 8.8.3 grammar, obs-text included. The * wildcard, entity-tag lists, weak tags, and repeated instances of this field are refused with 400. Required: absent means 428; unequal means 412")
    ),
    request_body(content = UpdateMetadataFields, description = "RFC 7396 JSON Merge Patch: absent fields are unchanged, `null` clears (except `title`)"),
    responses(
        (status = 200, description = "Manual edit recorded as a `manual` metadata version and promoted to canonical (or cleared); body carries the applied value and version pointers per field", body = UpdateMetadataResponse,
         headers(("ETag" = String, description = "Strong entity-tag hashing every PATCH-modifiable field, reflecting the state after this write"))),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails),
        (status = 403, description = "Caller is a child account", body = crate::openapi::ProblemDetails),
        (status = 404, description = "Manifestation missing or RLS-hidden (existence-not-leaked)", body = crate::openapi::ProblemDetails),
        (status = 412, description = "If-Match does not match the manifestation's current metadata ETag", body = crate::openapi::ProblemDetails,
         headers(("ETag" = String, description = "Current entity-tag, so the caller can resync without a follow-up GET"))),
        (status = 400, description = "If-Match is malformed, or carries a form this API refuses by policy: the * wildcard, an entity-tag list, a weak tag, or a repeated header instance", body = crate::openapi::ProblemDetails),
        (status = 422, description = "No populated fields, ISBN/date parse failure, or attempt to clear title. Evaluated only after If-Match has matched, so a stale tag returns 412 instead", body = crate::openapi::ProblemDetails),
        (status = 428, description = "If-Match header absent", body = crate::openapi::ProblemDetails)
    )
)]
#[expect(
    clippy::too_many_lines,
    reason = "sequential dispatch across the six patchable field families (scalars, vocabularies, contributors, identifiers, ISBN rematch, response assembly); each family already delegates to its own helper"
)]
async fn update_book_metadata(
    current_user: CurrentUser,
    State(state): State<AppState>,
    Path(manifestation_id): Path<Uuid>,
    headers_in: HeaderMap,
    body: Result<axum::Json<UpdateMetadataFields>, axum::extract::rejection::JsonRejection>,
) -> Result<Response, AppError> {
    current_user.require_scope(Scope::Write)?;
    current_user.require_not_child()?;
    let if_match = parse_if_match(&headers_in)?.ok_or(AppError::IfMatchRequired)?;
    let axum::Json(mut req_fields) = body.map_err(|e| AppError::Validation(e.body_text()))?;
    // Extract contributors BEFORE populated() consumes the struct — it is
    // handled separately from the other (scalar) fields.
    let contributors = req_fields.contributors.take();
    let genres = req_fields.genres.take();
    let moods = req_fields.moods.take();
    let tags = req_fields.tags.take();
    let work_identifiers = req_fields.work_identifiers.take();
    let manifestation_identifiers = req_fields.manifestation_identifiers.take();
    let fields = req_fields.populated();

    let mut tx = db::acquire_with_rls(&state.pool, current_user.user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    // Lock the manifestation + parent work for the duration of the tx.
    // Mirrors `revert_manifestation` — serialises concurrent edits and
    // gates RLS-hidden rows to 404 (the join through `manifestations`
    // applies the RLS predicate).
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

    // Compared under the row lock just taken above, so a concurrent editor
    // cannot slip a change in between this check and the writes below.
    let current = load_book_metadata(&mut tx, manifestation_id)
        .await?
        .ok_or_else(|| {
            AppError::Internal(anyhow::anyhow!(
                "load_book_metadata returned None for a manifestation just locked"
            ))
        })?;
    let current_etag = hash_etag(&current)?;
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

    let mut response_fields: BTreeMap<String, FieldVersionChange> = BTreeMap::new();
    let mut touched_isbn = false;
    for (field, maybe_value) in fields {
        // ISBN edits — set OR clear — change the matching surface.
        // Rematch is read-only on rejection so it's safe to gate on
        // the column name alone without inspecting the new value.
        if field == "isbn_10" || field == "isbn_13" {
            touched_isbn = true;
        }
        let change = apply_scalar_patch_field(
            &mut tx,
            manifestation_id,
            work_id,
            current_user.user_id,
            field,
            maybe_value,
        )
        .await?;
        response_fields.insert(field.to_string(), change);
    }

    for (vocab, patch) in [
        (VocabularyField::Genres, genres),
        (VocabularyField::Moods, moods),
        (VocabularyField::Tags, tags),
    ] {
        if let Some(maybe_names) = patch {
            let change = apply_vocabulary_patch(
                &mut tx,
                manifestation_id,
                current_user.user_id,
                vocab,
                maybe_names,
            )
            .await?;
            response_fields.insert(vocab.field_name().to_string(), change);
        }
    }

    if let Some(patch) = contributors {
        let changes = apply_contributors_patch(
            &mut tx,
            manifestation_id,
            work_id,
            current_user.user_id,
            patch,
        )
        .await?;
        response_fields.extend(changes);
    }

    apply_identifier_patches(
        &mut tx,
        manifestation_id,
        work_id,
        current_user.user_id,
        work_identifiers,
        manifestation_identifiers,
        &mut response_fields,
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
    let new_etag = hash_etag(&new_metadata)?;

    tx.commit()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let mut headers = HeaderMap::new();
    headers.insert(ETAG, new_etag);
    Ok((
        StatusCode::OK,
        headers,
        axum::Json(UpdateMetadataResponse {
            fields: response_fields,
        }),
    )
        .into_response())
}

/// Journal + apply one scalar field from the manual PATCH surface.
/// A set (`Some`) journals the value and wires the canonical pointer via
/// `apply_version`; a clear (`None`) journals an accountability row and
/// nulls the canonical via `clear_field`. Returns the field's applied value
/// alongside its new and previous version pointers for the PATCH response.
async fn apply_scalar_patch_field(
    tx: &mut Transaction<'_, Postgres>,
    manifestation_id: Uuid,
    work_id: Uuid,
    user_id: Uuid,
    field: &str,
    maybe_value: Option<serde_json::Value>,
) -> Result<FieldVersionChange, AppError> {
    if let Some(value) = maybe_value {
        // Reject malformed ISBNs (wrong length, bad check digit,
        // non-numeric) and normalise valid ones to digits-only so the
        // stored value matches the ingestion surface and rematch's
        // exact-equality join can find ingested twins. Guard lives here,
        // not in `apply_version`, so accept/revert paths keep accepting
        // historical pre-validation/pre-normalisation values.
        let json = match field {
            "isbn_10" | "isbn_13" => {
                let raw = value
                    .as_str()
                    .ok_or_else(|| AppError::Validation(format!("{field} must be a string")))?;
                let normalised = if field == "isbn_10" {
                    isbn::checked_isbn10(raw)
                } else {
                    isbn::checked_isbn13(raw)
                }
                .ok_or_else(|| AppError::Validation(format!("invalid {field}")))?;
                serde_json::Value::String(normalised)
            }
            _ => value,
        };
        let version_id = insert_manual_version(tx, manifestation_id, user_id, field, &json).await?;
        let previous_version_id =
            apply_version(tx, field, &json, version_id, manifestation_id, work_id).await?;
        Ok(FieldVersionChange {
            value: Some(json),
            version_id: Some(version_id),
            previous_version_id,
        })
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
        let previous_version_id = clear_field(tx, field, manifestation_id, work_id).await?;
        Ok(FieldVersionChange {
            value: None,
            version_id: None,
            previous_version_id,
        })
    }
}

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
    response_fields: &mut BTreeMap<String, FieldVersionChange>,
) -> Result<(), AppError> {
    let mut touched = false;
    for (level, map) in [
        (IdentifierLevel::Work, work_identifiers),
        (IdentifierLevel::Manifestation, manifestation_identifiers),
    ] {
        for (scheme, maybe_value) in map.into_iter().flatten() {
            let change = apply_identifier_patch(
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
            response_fields.insert(level.canonical_field(&scheme), change);
        }
    }
    if touched {
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
                 enrichment_error = CASE WHEN enrichment_status = 'in_progress' \
                                         THEN enrichment_error ELSE NULL END \
             WHERE id = $1",
            manifestation_id,
        )
        .execute(&mut **tx)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    }
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
) -> Result<FieldVersionChange, AppError> {
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
        let previous_version_id =
            apply_version(tx, &field, &json, version_id, manifestation_id, work_id).await?;
        Ok(FieldVersionChange {
            value: Some(json),
            version_id: Some(version_id),
            previous_version_id,
        })
    } else {
        insert_manual_version(tx, manifestation_id, user_id, &field, &Value::Null).await?;
        let previous_version_id = clear_field(tx, &field, manifestation_id, work_id).await?;
        Ok(FieldVersionChange {
            value: None,
            version_id: None,
            previous_version_id,
        })
    }
}

/// Insert one `metadata_versions` row with `source='manual'`, returning
/// its id. `value_hash` is the SHA-256 of the field-normalised
/// canonical-JSON value — same shape as the enrichment pipeline emits, so duplicate
/// manual saves of the same value collide on the existing
/// `(manifestation, source, field, value_hash)` unique. We surface
/// that collision as a no-op by bumping `last_seen_at` /
/// `observation_count` on the existing row and re-recording who
/// touched it last.
///
/// `resolved_by` + `resolved_at` here denote the *author* of the
/// manual entry, not a resolution action — the row stays
/// `status = 'pending'` after insert so the accept/reject machinery
/// can still operate on it. The accountability use of these columns
/// is project-local and documented in the manual-edit ADR.
async fn insert_manual_version(
    tx: &mut Transaction<'_, Postgres>,
    manifestation_id: Uuid,
    user_id: Uuid,
    field: &str,
    value: &Value,
) -> Result<Uuid, AppError> {
    // Hash via the shared canonical normaliser (same call the enrichment
    // pipeline uses) so whitespace-equivalent values — e.g. a publisher
    // with stray leading/trailing space — produce an identical `value_hash`
    // regardless of entry path, restoring journal dedup.
    let hash = value_hash::value_hash(field, value);

    // `status` is reset to `pending` on conflict so a previously-
    // rejected manual entry of the same value is re-offered for
    // promotion rather than left stranded as `rejected`. Without this
    // reset, `apply_version` could write a canonical pointer to a row
    // still flagged rejected, leaving the journal internally
    // inconsistent.
    //
    // `new_value` is refreshed on conflict: the hash identifies the value
    // only up to normalisation (contributor arrays hash order-insensitively),
    // so a reorder collides with the original row and the journal must
    // record the latest representation, not the first one seen.
    let id: Uuid = sqlx::query_scalar!(
        "INSERT INTO metadata_versions \
            (manifestation_id, source, field_name, new_value, value_hash, \
             match_type, confidence_score, status, resolved_by, resolved_at) \
         VALUES ($1, 'manual', $2, $3, $4, 'manual', 1.0, \
                 'pending'::metadata_review_status, $5, now()) \
         ON CONFLICT (manifestation_id, source, field_name, value_hash) \
         DO UPDATE SET new_value = EXCLUDED.new_value, \
                       last_seen_at = now(), \
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

/// Why a submitted `pub_date` was rejected.
///
/// The two cases mean different things to the caller: a malformed value is a
/// spelling mistake, while an out-of-range year is a well-formed date the
/// column will not hold. Both surface as a 400, and the distinction is what
/// makes the message actionable.
#[derive(Debug, thiserror::Error)]
enum PubDateError {
    /// Not a `YYYY-MM-DD` calendar date, or not a date at all.
    #[error("expected an ISO 8601 calendar date (YYYY-MM-DD)")]
    Malformed,
    /// A real date, outside the years Reverie accepts.
    #[error("year must be between {MIN_PUB_YEAR} and {MAX_PUB_YEAR}")]
    YearOutOfRange,
}

/// Accepted `pub_date` year bounds.
///
/// These mirror the common-era convention the `timestamptz` decode-range
/// checks set, for the same reason: no Reverie date legitimately predates the
/// common era, and Postgres accepts years that `DateTime<Utc>` cannot
/// represent. The bound lives here rather than in the schema because
/// `manifestations.pub_date` carries no CHECK constraint, so it holds only as
/// far as every writer routes through this function.
const MIN_PUB_YEAR: i32 = 1;
const MAX_PUB_YEAR: i32 = 9999;

/// The one accepted spelling, used to parse and to re-render for the
/// canonical-form check below.
const ISO_DATE_FMT: &str = "%Y-%m-%d";

/// Normalise `s` to the `YYYY-MM-DD` candidate this parser will accept,
/// widening a bare `YYYY` or `YYYY-MM` and truncating a full timestamp.
fn iso_candidate(s: &str) -> String {
    // `s.len()` is in bytes; user-submitted strings can contain multi-byte
    // UTF-8 codepoints. `is_char_boundary` keeps the slice valid.
    if s.len() >= 10 && s.is_char_boundary(10) {
        s[..10].to_owned()
    } else {
        match s.len() {
            4 => format!("{s}-01-01"),
            7 => format!("{s}-01"),
            _ => s.to_owned(),
        }
    }
}

fn parse_iso_date(s: &str) -> Result<chrono::NaiveDate, PubDateError> {
    let candidate = iso_candidate(s);
    let date = chrono::NaiveDate::parse_from_str(&candidate, ISO_DATE_FMT)
        .map_err(|_| PubDateError::Malformed)?;
    // Two independent widenings have to be closed, and a range check alone
    // closes only one. chrono's numeric fields accept fewer digits than the
    // format spells, so `26-05-01` parses as year 26, which is inside any
    // sane range. Requiring the parse to round-trip to the same string
    // rejects every non-canonical spelling; the range check then rejects
    // canonical years the column cannot hold, such as `0000-01-01`.
    if date.format(ISO_DATE_FMT).to_string() != candidate {
        return Err(PubDateError::Malformed);
    }
    if !(MIN_PUB_YEAR..=MAX_PUB_YEAR).contains(&chrono::Datelike::year(&date)) {
        return Err(PubDateError::YearOutOfRange);
    }
    Ok(date)
}

#[cfg(test)]
mod tests {
    use super::parse_iso_date;
    use crate::error::problems;
    use crate::models::content_rating::ContentRating;
    use crate::test_support;
    use axum::http::StatusCode;
    use uuid::Uuid;

    #[test]
    fn parse_iso_date_rejects_multibyte_garbage_without_panicking() {
        // 3-byte codepoint pushes byte-10 mid-character; pre-fix this panicked.
        let s = "2024-01-€€€garbage";
        assert!(parse_iso_date(s).is_err());
    }

    #[test]
    fn parse_iso_date_rejects_a_short_year() {
        // chrono's `%Y` consumes one to four digits, so this parses cleanly
        // as year 26 and lands inside any sane year range. Only the
        // round-trip check catches it, and storing it would silently record a
        // date two millennia off the one submitted.
        let err = parse_iso_date("26-05-01").expect_err("short year must not be accepted");
        assert!(
            matches!(err, super::PubDateError::Malformed),
            "expected a spelling rejection, got {err:?}"
        );
    }

    #[test]
    fn parse_iso_date_rejects_unpadded_fields() {
        for input in ["2024-1-15", "2024-01-5", "-9999-1-1"] {
            assert!(
                parse_iso_date(input).is_err(),
                "{input} is not canonical and must be rejected"
            );
        }
    }

    #[test]
    fn parse_iso_date_rejects_year_zero_as_out_of_range() {
        // Ten characters and canonically spelled, so the round-trip passes it
        // and the range check is the only thing between it and a bind
        // Postgres rejects: there is no year 0 in a date column.
        let err = parse_iso_date("0000-01-01").expect_err("year 0 must not be accepted");
        assert!(
            matches!(err, super::PubDateError::YearOutOfRange),
            "expected a range rejection, got {err:?}"
        );
    }

    #[test]
    fn parse_iso_date_rejects_a_signed_year() {
        // A leading sign makes the date eleven characters, so it never
        // survives the ten-byte narrowing, whichever variant reports it. The
        // property under test is rejection, not which check fired.
        assert!(parse_iso_date("-9999-01-01").is_err());
    }

    #[test]
    fn parse_iso_date_accepts_the_range_boundaries() {
        assert!(parse_iso_date("0001-01-01").is_ok());
        assert!(parse_iso_date("9999-12-31").is_ok());
    }

    #[test]
    fn parse_iso_date_distinguishes_malformed_from_out_of_range() {
        // The two map to different operator-facing messages, so the variant
        // has to survive the parse rather than collapsing to one error.
        assert!(matches!(
            parse_iso_date("nonsense").expect_err("not a date"),
            super::PubDateError::Malformed
        ));
    }

    #[test]
    fn parse_iso_date_accepts_well_formed_iso() {
        assert!(parse_iso_date("2024-01-15").is_ok());
        assert!(parse_iso_date("2024-01-15T00:00:00Z").is_ok());
    }

    #[tokio::test]
    async fn get_manifestation_metadata_requires_auth() {
        let server = test_support::test_server();
        let id = Uuid::new_v4();
        let response = server
            .get(&format!("/api/v1/manifestations/{id}/metadata"))
            .await;
        assert_eq!(response.status_code(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn get_book_metadata_requires_auth() {
        let server = test_support::test_server();
        let id = Uuid::new_v4();
        let response = server.get(&format!("/api/v1/books/{id}/metadata")).await;
        assert_eq!(response.status_code(), StatusCode::UNAUTHORIZED);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn get_book_metadata_unknown_manifestation_returns_404(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;

        let server = test_support::db::server_with_real_pools(&app_pool, &ing_pool);
        let id = Uuid::new_v4();
        let response = server
            .get(&format!("/api/v1/books/{id}/metadata"))
            .add_header(AUTHORIZATION, basic)
            .await;
        assert_eq!(response.status_code(), StatusCode::NOT_FOUND);
        let body: serde_json::Value = response.json();
        assert_eq!(body["type"], "https://reverie.example/probs/not-found");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn get_book_metadata_returns_empty_collections_not_errors(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (_work_id, m_id) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;

        let server = test_support::db::server_with_real_pools(&app_pool, &ing_pool);
        let response = server
            .get(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic)
            .await;
        assert_eq!(
            response.status_code(),
            StatusCode::OK,
            "body = {}",
            response.text()
        );
        let body: serde_json::Value = response.json();
        assert_eq!(body["title"], "");
        assert_eq!(body["subtitle"], serde_json::Value::Null);
        assert_eq!(body["genres"], serde_json::json!([]));
        assert_eq!(body["moods"], serde_json::json!([]));
        assert_eq!(body["tags"], serde_json::json!([]));
        assert_eq!(body["contributors"]["author"], serde_json::json!([]));
        assert_eq!(body["contributors"]["editor"], serde_json::json!([]));
        assert_eq!(body["contributors"]["translator"], serde_json::json!([]));
        assert_eq!(body["work_identifiers"], serde_json::json!({}));
        assert_eq!(body["manifestation_identifiers"], serde_json::json!({}));
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn get_book_metadata_contributors_in_stored_position_order(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (work_id, m_id) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;
        let name_last = format!("Zed Last {marker}");
        let name_first = format!("Alpha First {marker}");
        // Deliberately insert in reverse alphabetical order so a name-sorted
        // response would fail this assertion; only stored `position` must
        // determine the returned order.
        test_support::db::insert_contributor(&ing_pool, work_id, &name_last, "author", 0).await;
        test_support::db::insert_contributor(&ing_pool, work_id, &name_first, "author", 1).await;

        let server = test_support::db::server_with_real_pools(&app_pool, &ing_pool);
        let response = server
            .get(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic)
            .await;
        assert_eq!(response.status_code(), StatusCode::OK);
        let body: serde_json::Value = response.json();
        assert_eq!(
            body["contributors"]["author"],
            serde_json::json!([name_last, name_first]),
            "authors must come back in stored position order, not name order"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn get_book_metadata_returns_writable_roles_and_excludes_narrator(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (work_id, m_id) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;
        let author_name = format!("Author {marker}");
        let editor_name = format!("Editor {marker}");
        let translator_name = format!("Translator {marker}");
        let narrator_name = format!("Narrator {marker}");
        test_support::db::insert_contributor(&ing_pool, work_id, &author_name, "author", 0).await;
        test_support::db::insert_contributor(&ing_pool, work_id, &editor_name, "editor", 0).await;
        test_support::db::insert_contributor(&ing_pool, work_id, &translator_name, "translator", 0)
            .await;
        test_support::db::insert_contributor(&ing_pool, work_id, &narrator_name, "narrator", 0)
            .await;

        let server = test_support::db::server_with_real_pools(&app_pool, &ing_pool);
        let response = server
            .get(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic)
            .await;
        assert_eq!(
            response.status_code(),
            StatusCode::OK,
            "body = {}",
            response.text()
        );
        let body: serde_json::Value = response.json();
        assert_eq!(
            body["contributors"]["author"],
            serde_json::json!([author_name])
        );
        assert_eq!(
            body["contributors"]["editor"],
            serde_json::json!([editor_name])
        );
        assert_eq!(
            body["contributors"]["translator"],
            serde_json::json!([translator_name])
        );
        let contributors = body["contributors"]
            .as_object()
            .expect("contributors object");
        let mut keys: Vec<&str> = contributors.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(keys, ["author", "editor", "translator"]);
        assert!(
            !body.to_string().contains(&narrator_name),
            "narrator name must not appear anywhere in the response"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn get_book_metadata_round_trips_with_patch(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (_work_id, m_id) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;
        let subtitle = format!("Round Trip Subtitle {marker}");
        let genre = format!("Genre {marker}");
        let author = format!("Round Trip Author {marker}");

        let server = test_support::db::server_with_real_pools(&app_pool, &ing_pool);
        let initial = server
            .get(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
            .await;
        let etag = etag_value(initial.headers());
        let patch_response = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
            .add_header(
                axum::http::header::IF_MATCH,
                axum::http::HeaderValue::from_str(&etag).unwrap(),
            )
            .json(&serde_json::json!({
                "subtitle": subtitle,
                "publisher": "Round Trip Press",
                "pub_date": "2020-05-01",
                "pages": 321,
                "content_rating": "teen",
                "genres": [genre],
                "contributors": {"author": [author]},
                "work_identifiers": {"openlibrary": "OL333W"},
                "manifestation_identifiers": {"googlebooks": "zyTZAAAAYAAJ"},
            }))
            .await;
        assert_eq!(
            patch_response.status_code(),
            StatusCode::OK,
            "body = {}",
            patch_response.text()
        );

        let response = server
            .get(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic)
            .await;
        assert_eq!(
            response.status_code(),
            StatusCode::OK,
            "body = {}",
            response.text()
        );
        let body: serde_json::Value = response.json();
        assert_eq!(body["subtitle"], subtitle);
        assert_eq!(body["publisher"], "Round Trip Press");
        assert_eq!(body["pub_date"], "2020-05-01");
        assert_eq!(body["pages"], 321);
        assert_eq!(body["content_rating"], "teen");
        assert_eq!(body["genres"], serde_json::json!([genre]));
        assert_eq!(body["contributors"]["author"], serde_json::json!([author]));
        assert_eq!(
            body["work_identifiers"],
            serde_json::json!({"openlibrary": "OL333W"})
        );
        assert_eq!(
            body["manifestation_identifiers"],
            serde_json::json!({"googlebooks": "zyTZAAAAYAAJ"})
        );
    }

    #[tokio::test]
    async fn accept_requires_auth() {
        let server = test_support::test_server();
        let id = Uuid::new_v4();
        let vid = Uuid::new_v4();
        let response = server
            .post(&format!("/api/v1/manifestations/{id}/metadata/accept"))
            .json(&serde_json::json!({"version_id": vid}))
            .await;
        assert_eq!(response.status_code(), StatusCode::UNAUTHORIZED);
    }

    // ── Admin-authenticated success tests (X3) ────────────────────────────
    //
    // These exercise the C2 fix: route handlers now open their tx via
    // `acquire_with_rls`, so the manifestations RLS policies see a real
    // `app.current_user_id` and an admin user satisfies the
    // `role IN ('admin','adult')` clause.  Without C2 these tests would
    // 404 on the initial SELECT.

    use axum::http::header::AUTHORIZATION;

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

    /// Insert a second manifestation on an existing work, mirroring
    /// `insert_work_and_manifestation`'s row shape. Used by the sibling-
    /// manifestation revert tests below, which need two editions of the
    /// same work.
    async fn insert_sibling_manifestation(ingestion_pool: &sqlx::PgPool, work_id: Uuid) -> Uuid {
        let marker = Uuid::new_v4().simple().to_string();
        let file_path = format!("/tmp/admin-test-sibling-{marker}.epub");
        let file_hash = format!("admin-test-sibling-hash-{marker}");
        sqlx::query_scalar!(
            "INSERT INTO manifestations \
                (work_id, format, file_path, ingestion_file_hash, current_file_hash, \
                 file_size_bytes, ingestion_status, validation_status) \
             VALUES ($1, 'epub'::manifestation_format, $2, $3, $3, 1000, \
                     'complete'::ingestion_status, 'clean'::validation_status) \
             RETURNING id",
            work_id,
            file_path,
            file_hash,
        )
        .fetch_one(ingestion_pool)
        .await
        .expect("insert sibling manifestation")
    }

    /// Pull `body["fields"][field]["version_id"]` as an owned `String`,
    /// panicking with the field name and full body if it's missing.
    fn version_id_of(body: &serde_json::Value, field: &str) -> String {
        body["fields"][field]["version_id"]
            .as_str()
            .unwrap_or_else(|| panic!("expected a version_id for field '{field}' in {body}"))
            .to_string()
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
    async fn reject_admin_marks_version_rejected(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (_work_id, m_id) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;
        let version_id = insert_version(
            &ing_pool,
            m_id,
            "title",
            serde_json::json!(format!("Reject Me {marker}")),
        )
        .await;

        let server = test_support::db::server_with_real_pools(&app_pool, &ing_pool);
        let response = server
            .post(&format!("/api/v1/manifestations/{m_id}/metadata/reject"))
            .add_header(AUTHORIZATION, basic)
            .json(&serde_json::json!({"version_id": version_id}))
            .await;
        assert_eq!(response.status_code(), StatusCode::OK);

        let row = sqlx::query!(
            "SELECT status::text AS status, resolved_by FROM metadata_versions WHERE id = $1",
            version_id,
        )
        .fetch_one(&app_pool)
        .await
        .expect("fetch version");
        assert_eq!(row.status.as_deref(), Some("rejected"));
        assert_eq!(
            row.resolved_by,
            Some(admin_id),
            "resolved_by should record admin id"
        );

        // Reject does NOT change the canonical pointer, so it must NOT
        // enqueue a writeback job.
        let job_count: i64 = sqlx::query_scalar!(
            "SELECT count(*) AS \"count!\" FROM writeback_jobs WHERE manifestation_id = $1",
            m_id,
        )
        .fetch_one(&app_pool)
        .await
        .expect("fetch job count");
        assert_eq!(
            job_count, 0,
            "reject must NOT enqueue writeback; got {job_count}"
        );
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

    #[tokio::test]
    async fn patch_book_metadata_requires_auth() {
        let server = test_support::test_server();
        let id = Uuid::new_v4();
        let response = server
            .patch(&format!("/api/v1/books/{id}/metadata"))
            .json(&serde_json::json!({"title": "X"}))
            .await;
        assert_eq!(response.status_code(), StatusCode::UNAUTHORIZED);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn patch_old_fields_envelope_returns_422(pool: sqlx::PgPool) {
        // The body is now a bare RFC 7396 merge patch: the
        // previous `{"fields": {...}}` envelope carries no recognized
        // top-level key, so every field is treated as absent and the
        // handler's "no populated fields" guard rejects it. Pins the break
        // as a deliberate, visible change for API consumers.
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (_work_id, m_id) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;

        let server = test_support::db::server_with_real_pools(&app_pool, &ing_pool);
        // A matching tag, so the request reaches body validation: the
        // precondition is evaluated before the content.
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
            .json(&serde_json::json!({"fields": {"title": "Enveloped"}}))
            .await;
        assert_eq!(
            response.status_code(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "the enveloped shape must no longer be accepted; body = {}",
            response.text()
        );
    }

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
            body["fields"]["title"]["version_id"],
            serde_json::json!(version_id),
            "response version_id must equal the new canonical pointer"
        );
        assert_eq!(
            body["fields"]["title"]["previous_version_id"],
            serde_json::Value::Null,
            "a freshly created work has no prior title pointer"
        );
        assert_eq!(
            body["fields"]["title"]["value"],
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
    async fn patch_title_response_reports_previous_pointer(pool: sqlx::PgPool) {
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
        let first_title = format!("First Title {marker}");
        let first_response = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
            .add_header(
                axum::http::header::IF_MATCH,
                axum::http::HeaderValue::from_str(&etag).unwrap(),
            )
            .json(&serde_json::json!({"title": first_title}))
            .await;
        assert_eq!(first_response.status_code(), StatusCode::OK);
        let first_body: serde_json::Value = first_response.json();
        let first_version_id = version_id_of(&first_body, "title");
        let etag = etag_value(first_response.headers());

        let second_title = format!("Second Title {marker}");
        let second_response = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic)
            .add_header(
                axum::http::header::IF_MATCH,
                axum::http::HeaderValue::from_str(&etag).unwrap(),
            )
            .json(&serde_json::json!({"title": second_title}))
            .await;
        assert_eq!(second_response.status_code(), StatusCode::OK);
        let second_body: serde_json::Value = second_response.json();
        assert_eq!(
            second_body["fields"]["title"]["previous_version_id"],
            serde_json::json!(first_version_id),
            "second patch must report the first patch's version as the previous pointer"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn patch_title_then_revert_restores_previous_value(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (work_id, m_id) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ing_pool);

        let initial = server
            .get(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
            .await;
        let etag = etag_value(initial.headers());
        let first_title = format!("First Title {marker}");
        let first_response = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
            .add_header(
                axum::http::header::IF_MATCH,
                axum::http::HeaderValue::from_str(&etag).unwrap(),
            )
            .json(&serde_json::json!({"title": first_title.clone()}))
            .await;
        assert_eq!(first_response.status_code(), StatusCode::OK);
        let etag = etag_value(first_response.headers());

        let second_title = format!("Second Title {marker}");
        let second_response = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
            .add_header(
                axum::http::header::IF_MATCH,
                axum::http::HeaderValue::from_str(&etag).unwrap(),
            )
            .json(&serde_json::json!({"title": second_title}))
            .await;
        assert_eq!(second_response.status_code(), StatusCode::OK);
        let second_body: serde_json::Value = second_response.json();
        let previous_version_id = second_body["fields"]["title"]["previous_version_id"]
            .as_str()
            .expect("second patch must report a previous_version_id")
            .to_string();

        let revert_response = server
            .post(&format!("/api/v1/manifestations/{m_id}/metadata/revert"))
            .add_header(AUTHORIZATION, basic)
            .json(&serde_json::json!({
                "field_name": "title",
                "version_id": previous_version_id,
            }))
            .await;
        assert_eq!(
            revert_response.status_code(),
            StatusCode::OK,
            "body = {}",
            revert_response.text()
        );

        let title: String = sqlx::query_scalar!("SELECT title FROM works WHERE id = $1", work_id)
            .fetch_one(&app_pool)
            .await
            .expect("fetch title after revert");
        assert_eq!(
            title, first_title,
            "revert to the previous_version_id must restore the original title"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn patch_clears_description_with_null(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (work_id, m_id) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;
        let initial = format!("Existing prose {marker}");
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
            .add_header(AUTHORIZATION, basic)
            .add_header(
                axum::http::header::IF_MATCH,
                axum::http::HeaderValue::from_str(&etag).unwrap(),
            )
            .json(&serde_json::json!({"description": serde_json::Value::Null}))
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
        assert!(row.description.is_none(), "description should be cleared");
        assert!(
            row.description_version_id.is_none(),
            "version pointer should be cleared"
        );

        // Audit-trail row recorded with new_value = JSON null.
        let null_count: i64 = sqlx::query_scalar!(
            "SELECT count(*) AS \"count!\" \
             FROM metadata_versions \
             WHERE manifestation_id = $1 AND source = 'manual' \
               AND field_name = 'description' AND new_value = 'null'::jsonb",
            m_id,
        )
        .fetch_one(&app_pool)
        .await
        .expect("fetch null count");
        assert_eq!(null_count, 1, "audit-trail row missing for clear action");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn patch_with_empty_body_returns_422(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (_work_id, m_id) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;

        let server = test_support::db::server_with_real_pools(&app_pool, &ing_pool);
        // A matching tag, so the request reaches body validation: the
        // precondition is evaluated before the content.
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
            .json(&serde_json::json!({}))
            .await;
        assert_eq!(response.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
        let body = response.text();
        assert!(
            body.contains("no fields"),
            "expected 'no fields' detail, got: {body}"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn patch_clear_title_with_null_returns_422(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (work_id, m_id) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;

        let title_before: String =
            sqlx::query_scalar!("SELECT title FROM works WHERE id = $1", work_id)
                .fetch_one(&app_pool)
                .await
                .expect("fetch title before");

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
            .json(&serde_json::json!({"title": serde_json::Value::Null}))
            .await;
        assert_eq!(
            response.status_code(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "title is NOT NULL on works — clearing it must be rejected"
        );
        let body = response.text();
        assert!(
            body.contains("cannot clear title"),
            "expected 'cannot clear title' detail, got: {body}"
        );

        // Rejection happens before commit — the canonical title is intact
        // and the rolled-back tx leaves no manual audit row behind.
        let title_after: String =
            sqlx::query_scalar!("SELECT title FROM works WHERE id = $1", work_id)
                .fetch_one(&app_pool)
                .await
                .expect("fetch title after");
        assert_eq!(
            title_after, title_before,
            "title must be unchanged after a rejected clear"
        );
        let manual_title_rows: i64 = sqlx::query_scalar!(
            "SELECT count(*) AS \"count!\" \
             FROM metadata_versions \
             WHERE manifestation_id = $1 AND source = 'manual' AND field_name = 'title'",
            m_id,
        )
        .fetch_one(&app_pool)
        .await
        .expect("fetch manual title row count");
        assert_eq!(
            manual_title_rows, 0,
            "rejected clear must not persist an audit-trail row"
        );
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
    async fn patch_leaves_sibling_pending_drafts_alone(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (_work_id, m_id) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;

        // AI draft sitting pending on `title` BEFORE the manual edit.
        let ai_draft = insert_version(
            &ing_pool,
            m_id,
            "title",
            serde_json::json!("AI Draft Title"),
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
            .json(&serde_json::json!({"title": "Operator-chosen"}))
            .await;
        assert_eq!(response.status_code(), StatusCode::OK);

        // AI draft stays pending — operator can later revert to it.
        let status: Option<String> = sqlx::query_scalar!(
            "SELECT status::text FROM metadata_versions WHERE id = $1",
            ai_draft,
        )
        .fetch_one(&app_pool)
        .await
        .expect("fetch ai draft status");
        assert_eq!(
            status.as_deref(),
            Some("pending"),
            "sibling AI draft was mutated by the manual edit"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn patch_returns_404_for_missing_manifestation(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ing_pool);

        let fake = Uuid::new_v4();
        let response = server
            .patch(&format!("/api/v1/books/{fake}/metadata"))
            .add_header(AUTHORIZATION, basic)
            .add_header(
                axum::http::header::IF_MATCH,
                axum::http::HeaderValue::from_static("\"dummy\""),
            )
            .json(&serde_json::json!({"title": "ghost"}))
            .await;
        assert_eq!(response.status_code(), StatusCode::NOT_FOUND);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn patch_isbn_set_triggers_rematch(pool: sqlx::PgPool) {
        // Two manifestations sharing an ISBN — patching the second
        // with that ISBN must wire `suspected_duplicate_work_id` on it.
        // Auto-merge is skipped because the PATCH inserts a manual
        // metadata draft on the current work (the orchestrator guards
        // against merging works that already carry manual drafts).
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ing_pool);

        let marker_a = Uuid::new_v4().simple().to_string();
        let marker_b = Uuid::new_v4().simple().to_string();
        let (work_a, m_a) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker_a).await;
        let (_work_b, m_b) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker_b).await;

        let isbn = "9780306406157";
        sqlx::query!(
            "UPDATE manifestations SET isbn_13 = $1 WHERE id = $2",
            isbn,
            m_a,
        )
        .execute(&ing_pool)
        .await
        .expect("seed isbn_a");

        let initial = server
            .get(&format!("/api/v1/books/{m_b}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
            .await;
        let etag = etag_value(initial.headers());
        let response = server
            .patch(&format!("/api/v1/books/{m_b}/metadata"))
            .add_header(AUTHORIZATION, basic)
            .add_header(
                axum::http::header::IF_MATCH,
                axum::http::HeaderValue::from_str(&etag).unwrap(),
            )
            .json(&serde_json::json!({"isbn_13": isbn}))
            .await;
        assert_eq!(
            response.status_code(),
            StatusCode::OK,
            "body = {}",
            response.text()
        );

        // Rematch wired the suspected_duplicate_work_id pointer on m_b
        // back to work_a. The flag firing is the proof the route
        // correctly invoked rematch_on_isbn_change — without the route-
        // level wiring, this column stays NULL.
        let suspected: Option<Uuid> = sqlx::query_scalar!(
            "SELECT suspected_duplicate_work_id FROM manifestations WHERE id = $1",
            m_b,
        )
        .fetch_one(&ing_pool)
        .await
        .expect("fetch suspected");
        assert_eq!(
            suspected,
            Some(work_a),
            "ISBN PATCH did not trigger rematch — suspected_duplicate_work_id is {suspected:?}, expected Some({work_a})"
        );
    }

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

        // 13 digits, correct length, wrong check digit (valid form ends 7).
        let response = server
            .patch(&format!("/api/v1/books/{m}/metadata"))
            .add_header(AUTHORIZATION, basic)
            .add_header(
                axum::http::header::IF_MATCH,
                axum::http::HeaderValue::from_str(&etag).unwrap(),
            )
            .json(&serde_json::json!({"isbn_13": "9780306406150"}))
            .await;
        assert_eq!(
            response.status_code(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "body = {}",
            response.text()
        );
    }

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

    #[sqlx::test(migrations = "./migrations")]
    async fn patch_valid_isbn10_accepted(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ing_pool);
        let marker = Uuid::new_v4().simple().to_string();
        let (_work, m) = test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;

        // Valid ISBN-10 (check digit 2) must pass the new guard.
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
            .json(&serde_json::json!({"isbn_10": "0306406152"}))
            .await;
        assert_eq!(
            response.status_code(),
            StatusCode::OK,
            "body = {}",
            response.text()
        );
    }

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
            body["fields"]["isbn_13"]["value"],
            serde_json::json!("9780306406157"),
            "response must echo the normalized value, not the raw hyphenated input"
        );

        // Stored canonical value is digits-only, matching the ingestion
        // surface so rematch exact-equality can find ingested twins.
        let stored: Option<String> =
            sqlx::query_scalar!("SELECT isbn_13 FROM manifestations WHERE id = $1", m)
                .fetch_one(&ing_pool)
                .await
                .expect("fetch isbn_13");
        assert_eq!(stored.as_deref(), Some("9780306406157"));
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn patch_dashed_isbn_rematches_undashed_twin(pool: sqlx::PgPool) {
        // m_a carries an ingested (digits-only) ISBN; PATCHing m_b with the
        // same ISBN in dashed form must normalise on write so rematch's
        // exact-equality join wires the duplicate pointer.
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ing_pool);

        let marker_a = Uuid::new_v4().simple().to_string();
        let marker_b = Uuid::new_v4().simple().to_string();
        let (work_a, m_a) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker_a).await;
        let (_work_b, m_b) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker_b).await;

        sqlx::query!(
            "UPDATE manifestations SET isbn_13 = $1 WHERE id = $2",
            "9780306406157",
            m_a,
        )
        .execute(&ing_pool)
        .await
        .expect("seed isbn_a");

        let initial = server
            .get(&format!("/api/v1/books/{m_b}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
            .await;
        let etag = etag_value(initial.headers());
        let response = server
            .patch(&format!("/api/v1/books/{m_b}/metadata"))
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

        let suspected: Option<Uuid> = sqlx::query_scalar!(
            "SELECT suspected_duplicate_work_id FROM manifestations WHERE id = $1",
            m_b,
        )
        .fetch_one(&ing_pool)
        .await
        .expect("fetch suspected");
        assert_eq!(
            suspected,
            Some(work_a),
            "dashed PATCH did not rematch undashed twin — suspected_duplicate_work_id is {suspected:?}"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn backfill_normalises_dashed_isbns(pool: sqlx::PgPool) {
        // Exercises the backfill statements from
        // 20260603032915_normalise_existing_isbns.up.sql against rows that
        // predate ISBN normalisation: dashed isbn_13 + spaced/lowercase
        // isbn_10, a `urn:isbn:` OPF prefix, and a leading-whitespace +
        // prefix value (the case where a missing trim would leave the prefix
        // literal and silently break rematch). The migration itself only
        // proves it applies to an empty table; this proves the transform
        // collapses real divergent data, including the prefix and whitespace
        // forms `normalise()` strips. The backfill SQL below is kept verbatim
        // in sync with the migration file.
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (_work, m) = test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;
        let prefix_marker = Uuid::new_v4().simple().to_string();
        let (_work2, m_prefix) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &prefix_marker).await;
        let ws_marker = Uuid::new_v4().simple().to_string();
        let (_work3, m_ws) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &ws_marker).await;

        sqlx::query!(
            "UPDATE manifestations SET isbn_13 = $1, isbn_10 = $2 WHERE id = $3",
            "978-0-306-40615-7",
            "0-8044-2957-x",
            m,
        )
        .execute(&ing_pool)
        .await
        .expect("seed dashed isbns");
        sqlx::query!(
            "UPDATE manifestations SET isbn_13 = $1 WHERE id = $2",
            "urn:isbn:9780306406157",
            m_prefix,
        )
        .execute(&ing_pool)
        .await
        .expect("seed prefixed isbn");
        sqlx::query!(
            "UPDATE manifestations SET isbn_13 = $1 WHERE id = $2",
            " urn:isbn:9780306406157",
            m_ws,
        )
        .execute(&ing_pool)
        .await
        .expect("seed whitespace-prefixed isbn");

        sqlx::query!(
            "UPDATE manifestations \
             SET isbn_10 = upper(regexp_replace(regexp_replace(regexp_replace(isbn_10, '^[[:space:]]+|[[:space:]]+$', '', 'g'), '^(urn:isbn:|URN:ISBN:|isbn:|ISBN:|ISBN )', ''), '[- ]', '', 'g')) \
             WHERE isbn_10 IS NOT NULL \
               AND isbn_10 <> upper(regexp_replace(regexp_replace(regexp_replace(isbn_10, '^[[:space:]]+|[[:space:]]+$', '', 'g'), '^(urn:isbn:|URN:ISBN:|isbn:|ISBN:|ISBN )', ''), '[- ]', '', 'g'))"
        )
        .execute(&ing_pool)
        .await
        .expect("backfill isbn_10");
        sqlx::query!(
            "UPDATE manifestations \
             SET isbn_13 = regexp_replace(regexp_replace(regexp_replace(isbn_13, '^[[:space:]]+|[[:space:]]+$', '', 'g'), '^(urn:isbn:|URN:ISBN:|isbn:|ISBN:|ISBN )', ''), '[- ]', '', 'g') \
             WHERE isbn_13 IS NOT NULL \
               AND isbn_13 <> regexp_replace(regexp_replace(regexp_replace(isbn_13, '^[[:space:]]+|[[:space:]]+$', '', 'g'), '^(urn:isbn:|URN:ISBN:|isbn:|ISBN:|ISBN )', ''), '[- ]', '', 'g')"
        )
        .execute(&ing_pool)
        .await
        .expect("backfill isbn_13");

        let row = sqlx::query!(
            "SELECT isbn_10, isbn_13 FROM manifestations WHERE id = $1",
            m
        )
        .fetch_one(&ing_pool)
        .await
        .expect("fetch normalised");
        assert_eq!(row.isbn_13.as_deref(), Some("9780306406157"));
        assert_eq!(row.isbn_10.as_deref(), Some("080442957X"));

        let prefix_row = sqlx::query!("SELECT isbn_13 FROM manifestations WHERE id = $1", m_prefix)
            .fetch_one(&ing_pool)
            .await
            .expect("fetch prefix normalised");
        assert_eq!(prefix_row.isbn_13.as_deref(), Some("9780306406157"));

        let ws_row = sqlx::query!("SELECT isbn_13 FROM manifestations WHERE id = $1", m_ws)
            .fetch_one(&ing_pool)
            .await
            .expect("fetch whitespace-prefix normalised");
        assert_eq!(ws_row.isbn_13.as_deref(), Some("9780306406157"));
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
    async fn patch_reuses_conflicting_row_and_resets_status_to_pending(pool: sqlx::PgPool) {
        // Operator rejects their own manual entry, then re-saves the
        // same value. The conflict path must reset status back to
        // 'pending' so apply_version's canonical-pointer write doesn't
        // leave the row in 'rejected' state.
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (work_id, m_id) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;

        let value = format!("Manual title {marker}");
        let server = test_support::db::server_with_real_pools(&app_pool, &ing_pool);

        // First save.
        let initial = server
            .get(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
            .await;
        let etag = etag_value(initial.headers());
        let r1 = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
            .add_header(
                axum::http::header::IF_MATCH,
                axum::http::HeaderValue::from_str(&etag).unwrap(),
            )
            .json(&serde_json::json!({"title": value.clone()}))
            .await;
        assert_eq!(r1.status_code(), StatusCode::OK);

        // Mark that manual row rejected directly (simulates an operator
        // rejecting their own manual entry via the per-row Reject button).
        let v1: Uuid = sqlx::query_scalar!(
            "SELECT title_version_id AS \"id?\" FROM works WHERE id = $1",
            work_id
        )
        .fetch_one(&app_pool)
        .await
        .expect("fetch v1")
        .expect("title_version_id wired by prior PATCH");
        sqlx::query!(
            "UPDATE metadata_versions SET status = 'rejected', resolved_at = now() WHERE id = $1",
            v1,
        )
        .execute(&app_pool)
        .await
        .expect("mark rejected");

        // Second save of the same value — should reset to pending.
        let etag = etag_value(r1.headers());
        let r2 = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic)
            .add_header(
                axum::http::header::IF_MATCH,
                axum::http::HeaderValue::from_str(&etag).unwrap(),
            )
            .json(&serde_json::json!({"title": value}))
            .await;
        assert_eq!(r2.status_code(), StatusCode::OK);

        let status: String = sqlx::query_scalar!(
            "SELECT status::text AS \"status!\" FROM metadata_versions WHERE id = $1",
            v1,
        )
        .fetch_one(&app_pool)
        .await
        .expect("fetch status");
        assert_eq!(status, "pending", "conflict path must reset status");
    }

    // ── Publisher hash convergence (debt: publisher-hash-divergence) ──────
    //
    // The manual edit path must hash via the same canonical normaliser as
    // the enrichment pipeline so whitespace-equivalent publishers dedup
    // identically regardless of entry path.

    #[sqlx::test(migrations = "./migrations")]
    async fn patch_publisher_hash_matches_enrichment_canonical(pool: sqlx::PgPool) {
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
            .json(&serde_json::json!({"publisher": "  Acme Press  "}))
            .await;
        assert_eq!(response.status_code(), StatusCode::OK);

        // The stored manual hash must equal the enrichment pipeline's
        // canonical hash for the trimmed-equivalent value — byte for byte.
        let stored: Vec<u8> = sqlx::query_scalar!(
            "SELECT value_hash AS \"value_hash!\" FROM metadata_versions \
             WHERE manifestation_id = $1 AND source = 'manual' AND field_name = 'publisher'",
            m_id,
        )
        .fetch_one(&app_pool)
        .await
        .expect("fetch stored publisher hash");

        let canonical = crate::services::enrichment::value_hash::value_hash(
            "publisher",
            &serde_json::json!("Acme Press"),
        );
        assert_eq!(
            stored, canonical,
            "manual publisher hash must equal enrichment canonical hash for whitespace-equivalent values"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn patch_publisher_whitespace_variants_dedup(pool: sqlx::PgPool) {
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
        let mut etag = etag_value(initial.headers());
        // Same logical publisher, divergent leading/trailing whitespace.
        for publisher in ["Penguin Random House", "  Penguin Random House  "] {
            let response = server
                .patch(&format!("/api/v1/books/{m_id}/metadata"))
                .add_header(AUTHORIZATION, basic.clone())
                .add_header(
                    axum::http::header::IF_MATCH,
                    axum::http::HeaderValue::from_str(&etag).unwrap(),
                )
                .json(&serde_json::json!({"publisher": publisher}))
                .await;
            assert_eq!(
                response.status_code(),
                StatusCode::OK,
                "body = {}",
                response.text()
            );
            etag = etag_value(response.headers());
        }

        let rows = sqlx::query!(
            "SELECT observation_count FROM metadata_versions \
             WHERE manifestation_id = $1 AND source = 'manual' AND field_name = 'publisher'",
            m_id,
        )
        .fetch_all(&app_pool)
        .await
        .expect("fetch manual publisher rows");

        assert_eq!(
            rows.len(),
            1,
            "whitespace-variant publishers must collapse to one journal row; got {}",
            rows.len()
        );
        assert_eq!(
            rows[0].observation_count, 2,
            "second save must bump observation_count via ON CONFLICT, not insert a new row"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn patch_pub_date_iso_variants_dedup(pool: sqlx::PgPool) {
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
        let mut etag = etag_value(initial.headers());
        // Same calendar date — one bare ISO date, one with a time suffix. The
        // canonical normaliser coerces both to the YYYY-MM-DD prefix, so the
        // second save must dedup rather than open a parallel journal row.
        for pub_date in ["2024-01-15", "2024-01-15T00:00:00Z"] {
            let response = server
                .patch(&format!("/api/v1/books/{m_id}/metadata"))
                .add_header(AUTHORIZATION, basic.clone())
                .add_header(
                    axum::http::header::IF_MATCH,
                    axum::http::HeaderValue::from_str(&etag).unwrap(),
                )
                .json(&serde_json::json!({"pub_date": pub_date}))
                .await;
            assert_eq!(response.status_code(), StatusCode::OK);
            etag = etag_value(response.headers());
        }

        let rows = sqlx::query!(
            "SELECT observation_count FROM metadata_versions \
             WHERE manifestation_id = $1 AND source = 'manual' AND field_name = 'pub_date'",
            m_id,
        )
        .fetch_all(&app_pool)
        .await
        .expect("fetch manual pub_date rows");

        assert_eq!(
            rows.len(),
            1,
            "ISO-variant pub_dates must collapse to one journal row; got {}",
            rows.len()
        );
        assert_eq!(
            rows[0].observation_count, 2,
            "second save must bump observation_count via ON CONFLICT, not insert a new row"
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
            .json(&serde_json::json!({"contributors": {"author": [name_a, name_b]}}))
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

    // ── revert learns contributors.<role> ───────────────────────────────────

    #[sqlx::test(migrations = "./migrations")]
    async fn revert_contributors_author_restores_prior_set(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (work_id, m_id) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ing_pool);

        let initial = server
            .get(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
            .await;
        let etag = etag_value(initial.headers());
        let name_a = format!("Alpha Author {marker}");
        let name_b = format!("Beta Author {marker}");
        let first_response = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
            .add_header(
                axum::http::header::IF_MATCH,
                axum::http::HeaderValue::from_str(&etag).unwrap(),
            )
            .json(
                &serde_json::json!({"contributors": {"author": [name_a.clone(), name_b.clone()]}}),
            )
            .await;
        assert_eq!(first_response.status_code(), StatusCode::OK);
        let first_body: serde_json::Value = first_response.json();
        let first_version_id = version_id_of(&first_body, "contributors.author");
        let etag = etag_value(first_response.headers());

        let name_c = format!("Gamma Author {marker}");
        let second_response = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
            .add_header(
                axum::http::header::IF_MATCH,
                axum::http::HeaderValue::from_str(&etag).unwrap(),
            )
            .json(&serde_json::json!({"contributors": {"author": [name_c]}}))
            .await;
        assert_eq!(second_response.status_code(), StatusCode::OK);

        // Regression: this endpoint used to 422 on any `contributors.*`
        // field name because `apply_version`'s catch-all rejected it.
        let revert_response = server
            .post(&format!("/api/v1/manifestations/{m_id}/metadata/revert"))
            .add_header(AUTHORIZATION, basic)
            .json(&serde_json::json!({
                "field_name": "contributors.author",
                "version_id": first_version_id,
            }))
            .await;
        assert_eq!(
            revert_response.status_code(),
            StatusCode::OK,
            "body = {}",
            revert_response.text()
        );

        let rows = sqlx::query!(
            "SELECT a.name, wa.position FROM work_authors wa \
             JOIN authors a ON a.id = wa.author_id \
             WHERE wa.work_id = $1 AND wa.role = 'author' ORDER BY wa.position",
            work_id,
        )
        .fetch_all(&app_pool)
        .await
        .expect("fetch work_authors after revert");
        assert_eq!(
            rows.len(),
            2,
            "revert must restore the exact prior author set"
        );
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
        .expect("fetch sort name after revert");
        assert_eq!(
            sort_name.as_deref(),
            Some(format!("{marker}, Alpha Author").as_str()),
            "revert must refresh the denormalized sort column"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn revert_contributors_author_to_null_returns_422(pool: sqlx::PgPool) {
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
        let name = format!("Solo Author {marker}");
        let patch_response = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
            .add_header(
                axum::http::header::IF_MATCH,
                axum::http::HeaderValue::from_str(&etag).unwrap(),
            )
            .json(&serde_json::json!({"contributors": {"author": [name]}}))
            .await;
        assert_eq!(patch_response.status_code(), StatusCode::OK);

        let revert_response = server
            .post(&format!("/api/v1/manifestations/{m_id}/metadata/revert"))
            .add_header(AUTHORIZATION, basic)
            .json(&serde_json::json!({
                "field_name": "contributors.author",
                "version_id": serde_json::Value::Null,
            }))
            .await;
        assert_eq!(
            revert_response.status_code(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "clearing the sole author role must be rejected like title's clear guard"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn revert_contributors_translator_to_null_clears_role(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (work_id, m_id) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ing_pool);

        let initial = server
            .get(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
            .await;
        let etag = etag_value(initial.headers());
        let translator = format!("Translator Name {marker}");
        let patch_response = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
            .add_header(
                axum::http::header::IF_MATCH,
                axum::http::HeaderValue::from_str(&etag).unwrap(),
            )
            .json(&serde_json::json!({"contributors": {"translator": [translator]}}))
            .await;
        assert_eq!(patch_response.status_code(), StatusCode::OK);

        let revert_response = server
            .post(&format!("/api/v1/manifestations/{m_id}/metadata/revert"))
            .add_header(AUTHORIZATION, basic)
            .json(&serde_json::json!({
                "field_name": "contributors.translator",
                "version_id": serde_json::Value::Null,
            }))
            .await;
        assert_eq!(
            revert_response.status_code(),
            StatusCode::OK,
            "body = {}",
            revert_response.text()
        );

        let remaining: i64 = sqlx::query_scalar!(
            "SELECT count(*) AS \"count!\" FROM work_authors \
             WHERE work_id = $1 AND role = 'translator'",
            work_id,
        )
        .fetch_one(&app_pool)
        .await
        .expect("count translator rows");
        assert_eq!(
            remaining, 0,
            "revert-to-null must clear the translator role"
        );
    }

    // ── revert accepts a sibling manifestation's version for work-scoped
    //    fields, but keeps the strict same-manifestation match for
    //    manifestation-scoped fields ─────────────────────────────────────

    #[sqlx::test(migrations = "./migrations")]
    async fn revert_title_accepts_version_journaled_under_sibling_manifestation(
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

        // Patch title via manifestation A: journals a version under A and
        // wires it as the work's canonical title_version_id.
        let initial_a = server
            .get(&format!("/api/v1/books/{m_a}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
            .await;
        let etag_a = etag_value(initial_a.headers());
        let title_a = format!("Title Via A {marker}");
        let patch_a = server
            .patch(&format!("/api/v1/books/{m_a}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
            .add_header(
                axum::http::header::IF_MATCH,
                axum::http::HeaderValue::from_str(&etag_a).unwrap(),
            )
            .json(&serde_json::json!({"title": title_a}))
            .await;
        assert_eq!(patch_a.status_code(), StatusCode::OK);
        let body_a: serde_json::Value = patch_a.json();
        let version_a = version_id_of(&body_a, "title");

        // Patch title via manifestation B: the canonical pointer is
        // work-scoped, so B's response must report A's version as the
        // prior pointer even though the row was journaled under A.
        let initial_b = server
            .get(&format!("/api/v1/books/{m_b}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
            .await;
        let etag_b = etag_value(initial_b.headers());
        let title_b = format!("Title Via B {marker}");
        let patch_b = server
            .patch(&format!("/api/v1/books/{m_b}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
            .add_header(
                axum::http::header::IF_MATCH,
                axum::http::HeaderValue::from_str(&etag_b).unwrap(),
            )
            .json(&serde_json::json!({"title": title_b}))
            .await;
        assert_eq!(patch_b.status_code(), StatusCode::OK);
        let body_b: serde_json::Value = patch_b.json();
        assert_eq!(
            body_b["fields"]["title"]["previous_version_id"].as_str(),
            Some(version_a.as_str()),
            "title's canonical pointer lives on works, so B's previous_version_id \
             must be A's version"
        );

        // Undo on B for that A-owned version must succeed: title is
        // work-scoped, so the sibling's journal row is a valid revert target.
        let revert = server
            .post(&format!("/api/v1/manifestations/{m_b}/metadata/revert"))
            .add_header(AUTHORIZATION, basic)
            .json(&serde_json::json!({
                "field_name": "title",
                "version_id": version_a,
            }))
            .await;
        assert_eq!(
            revert.status_code(),
            StatusCode::OK,
            "body = {}",
            revert.text()
        );

        let row = sqlx::query!("SELECT title FROM works WHERE id = $1", work_id,)
            .fetch_one(&app_pool)
            .await
            .expect("fetch work title");
        assert_eq!(
            row.title, title_a,
            "revert on B must restore A's title as canonical"
        );
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
        let body_a: serde_json::Value = patch_a.json();
        let version_a = version_id_of(&body_a, "pages");

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

    #[sqlx::test(migrations = "./migrations")]
    async fn revert_contributors_author_accepts_version_journaled_under_sibling_manifestation(
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

        let initial_a = server
            .get(&format!("/api/v1/books/{m_a}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
            .await;
        let etag_a = etag_value(initial_a.headers());
        let name_a = format!("Alpha Author {marker}");
        let patch_a = server
            .patch(&format!("/api/v1/books/{m_a}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
            .add_header(
                axum::http::header::IF_MATCH,
                axum::http::HeaderValue::from_str(&etag_a).unwrap(),
            )
            .json(&serde_json::json!({"contributors": {"author": [name_a.clone()]}}))
            .await;
        assert_eq!(patch_a.status_code(), StatusCode::OK);
        let body_a: serde_json::Value = patch_a.json();
        let version_a = version_id_of(&body_a, "contributors.author");

        let initial_b = server
            .get(&format!("/api/v1/books/{m_b}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
            .await;
        let etag_b = etag_value(initial_b.headers());
        let name_b = format!("Beta Author {marker}");
        let patch_b = server
            .patch(&format!("/api/v1/books/{m_b}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
            .add_header(
                axum::http::header::IF_MATCH,
                axum::http::HeaderValue::from_str(&etag_b).unwrap(),
            )
            .json(&serde_json::json!({"contributors": {"author": [name_b]}}))
            .await;
        assert_eq!(patch_b.status_code(), StatusCode::OK);

        // contributors.* is work-scoped (work_authors keys off work_id, not
        // manifestation_id), so B can undo to the version A journaled.
        let revert = server
            .post(&format!("/api/v1/manifestations/{m_b}/metadata/revert"))
            .add_header(AUTHORIZATION, basic)
            .json(&serde_json::json!({
                "field_name": "contributors.author",
                "version_id": version_a,
            }))
            .await;
        assert_eq!(
            revert.status_code(),
            StatusCode::OK,
            "body = {}",
            revert.text()
        );

        let rows = sqlx::query!(
            "SELECT a.name FROM work_authors wa \
             JOIN authors a ON a.id = wa.author_id \
             WHERE wa.work_id = $1 AND wa.role = 'author'",
            work_id,
        )
        .fetch_all(&app_pool)
        .await
        .expect("fetch work_authors after revert");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].name, name_a);
    }

    // ── manual PATCH threads the prior contributors.<role> stamp instead of
    //    always reporting previous_version_id: None ──────────────────────

    #[sqlx::test(migrations = "./migrations")]
    async fn patch_contributors_author_second_edit_reports_prior_version_id(pool: sqlx::PgPool) {
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
        let name_a = format!("Alpha Author {marker}");
        let first = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
            .add_header(
                axum::http::header::IF_MATCH,
                axum::http::HeaderValue::from_str(&etag).unwrap(),
            )
            .json(&serde_json::json!({"contributors": {"author": [name_a]}}))
            .await;
        assert_eq!(first.status_code(), StatusCode::OK);
        let first_body: serde_json::Value = first.json();
        assert_eq!(
            first_body["fields"]["contributors.author"]["previous_version_id"],
            serde_json::Value::Null,
            "the work's first author edit has no prior single stamp to report"
        );
        let first_version_id = version_id_of(&first_body, "contributors.author");
        let etag = etag_value(first.headers());

        let name_b = format!("Beta Author {marker}");
        let second = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic)
            .add_header(
                axum::http::header::IF_MATCH,
                axum::http::HeaderValue::from_str(&etag).unwrap(),
            )
            .json(&serde_json::json!({"contributors": {"author": [name_b]}}))
            .await;
        assert_eq!(second.status_code(), StatusCode::OK);
        let second_body: serde_json::Value = second.json();
        assert_eq!(
            second_body["fields"]["contributors.author"]["previous_version_id"].as_str(),
            Some(first_version_id.as_str()),
            "the second edit must thread the first edit's version as its \
             previous_version_id instead of reporting null"
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
    async fn patch_contributors_over_cap_returns_422(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (_work_id, m_id) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;

        let names: Vec<String> = (0..=super::MAX_CONTRIBUTORS_PER_ROLE)
            .map(|i| format!("Cap Name {i} {marker}"))
            .collect();
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
            .json(&serde_json::json!({"contributors": {"author": names}}))
            .await;
        assert_eq!(
            response.status_code(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "body = {}",
            response.text()
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn patch_contributors_reorder_refreshes_journal_new_value(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (_work_id, m_id) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;
        let name_a = format!("Journal Alpha {marker}");
        let name_b = format!("Journal Beta {marker}");

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
            .json(&serde_json::json!({"contributors": {"author": [&name_a, &name_b]}}))
            .await;
        assert_eq!(response.status_code(), StatusCode::OK);
        let etag = etag_value(response.headers());

        // The reorder hashes identically (order-insensitive normalisation),
        // collides with the first row, and must refresh its new_value.
        let response = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic)
            .add_header(
                axum::http::header::IF_MATCH,
                axum::http::HeaderValue::from_str(&etag).unwrap(),
            )
            .json(&serde_json::json!({"contributors": {"author": [&name_b, &name_a]}}))
            .await;
        assert_eq!(response.status_code(), StatusCode::OK);

        let rows = sqlx::query!(
            "SELECT new_value, observation_count FROM metadata_versions \
             WHERE manifestation_id = $1 AND field_name = 'contributors.author'",
            m_id,
        )
        .fetch_all(&app_pool)
        .await
        .expect("fetch journal rows");
        assert_eq!(rows.len(), 1, "reorder must collide, not add a second row");
        assert_eq!(
            rows[0].new_value,
            serde_json::json!([name_b, name_a]),
            "surviving journal row must record the latest contributor order"
        );
        assert_eq!(rows[0].observation_count, 2);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn patch_contributors_reorder_changes_position(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (work_id, m_id) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;
        let name_a = format!("Reorder Alpha {marker}");
        let name_b = format!("Reorder Beta {marker}");
        test_support::db::insert_contributor(&ing_pool, work_id, &name_a, "author", 0).await;
        test_support::db::insert_contributor(&ing_pool, work_id, &name_b, "author", 1).await;

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
            .json(&serde_json::json!({"contributors": {"author": [name_b, name_a]}}))
            .await;
        assert_eq!(response.status_code(), StatusCode::OK);

        let rows = sqlx::query!(
            "SELECT a.name, wa.position FROM work_authors wa \
             JOIN authors a ON a.id = wa.author_id \
             WHERE wa.work_id = $1 AND wa.role = 'author' ORDER BY wa.position",
            work_id,
        )
        .fetch_all(&app_pool)
        .await
        .expect("fetch work_authors");
        assert_eq!(
            rows[0].name, name_b,
            "reordered patch must move Beta to position 0"
        );
        assert_eq!(rows[1].name, name_a);
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
    async fn patch_contributors_null_clears_role(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (work_id, m_id) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;
        test_support::db::insert_contributor(
            &ing_pool,
            work_id,
            &format!("Editor To Clear {marker}"),
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
            .json(&serde_json::json!({"contributors": {"editor": null}}))
            .await;
        assert_eq!(response.status_code(), StatusCode::OK);

        let editor_count = sqlx::query_scalar!(
            "SELECT COUNT(*) AS \"count!\" FROM work_authors \
             WHERE work_id = $1 AND role = 'editor'",
            work_id,
        )
        .fetch_one(&app_pool)
        .await
        .expect("count editors");
        assert_eq!(editor_count, 0);

        let null_count = sqlx::query_scalar!(
            "SELECT COUNT(*) AS \"count!\" FROM metadata_versions \
             WHERE manifestation_id = $1 AND field_name = 'contributors.editor' \
               AND new_value = 'null'::jsonb",
            m_id,
        )
        .fetch_one(&app_pool)
        .await
        .expect("count null journal rows");
        assert_eq!(null_count, 1, "clearing must leave an audit-trail row");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn patch_contributors_empty_array_equals_null(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (work_id, m_id) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;
        test_support::db::insert_contributor(
            &ing_pool,
            work_id,
            &format!("Translator To Clear {marker}"),
            "translator",
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
            .json(&serde_json::json!({"contributors": {"translator": []}}))
            .await;
        assert_eq!(response.status_code(), StatusCode::OK);

        let translator_count = sqlx::query_scalar!(
            "SELECT COUNT(*) AS \"count!\" FROM work_authors \
             WHERE work_id = $1 AND role = 'translator'",
            work_id,
        )
        .fetch_one(&app_pool)
        .await
        .expect("count translators");
        assert_eq!(translator_count, 0, "[] must clear the role like null");
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
    async fn patch_contributors_duplicate_name_returns_422(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (_work_id, m_id) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;
        let name = format!("Dup Name {marker}");

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
            .json(&serde_json::json!({"contributors": {"author": [name.clone(), name]}}))
            .await;
        assert_eq!(response.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn patch_contributors_empty_name_returns_422(pool: sqlx::PgPool) {
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
            .json(&serde_json::json!({"contributors": {"author": ["   "]}}))
            .await;
        assert_eq!(response.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn patch_contributors_name_over_500_chars_returns_422(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (_work_id, m_id) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;
        let long_name = "A".repeat(501);

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
            .json(&serde_json::json!({"contributors": {"author": [long_name]}}))
            .await;
        assert_eq!(response.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn patch_contributors_unknown_role_key_returns_422(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (_work_id, m_id) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;

        let server = test_support::db::server_with_real_pools(&app_pool, &ing_pool);
        let response = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic)
            .add_header(
                axum::http::header::IF_MATCH,
                axum::http::HeaderValue::from_static("\"dummy\""),
            )
            .json(&serde_json::json!({"contributors": {"narrator": ["Someone"]}}))
            .await;
        assert_eq!(
            response.status_code(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "narrator is reserved, not writable, and must 422 like any unknown key"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn patch_accepts_merge_patch_json_content_type(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (_work_id, m_id) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;
        let subtitle = format!("Merge Patch Subtitle {marker}");
        let body = serde_json::to_vec(&serde_json::json!({"subtitle": subtitle})).unwrap();

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
            .content_type("application/merge-patch+json")
            .bytes(body.into())
            .await;
        assert_eq!(
            response.status_code(),
            StatusCode::OK,
            "stock axum Json must accept the +json suffix; body = {}",
            response.text()
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn patch_sets_subtitle_and_writes_canonical(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (work_id, m_id) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;
        let subtitle = format!("A New Subtitle {marker}");

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
            .json(&serde_json::json!({"subtitle": subtitle}))
            .await;
        assert_eq!(response.status_code(), StatusCode::OK);

        let row = sqlx::query!(
            "SELECT subtitle, subtitle_version_id FROM works WHERE id = $1",
            work_id,
        )
        .fetch_one(&app_pool)
        .await
        .expect("fetch work");
        assert_eq!(row.subtitle.as_deref(), Some(subtitle.as_str()));
        assert!(row.subtitle_version_id.is_some());
        let set_version_id = row.subtitle_version_id.expect("subtitle_version_id wired");
        let etag = etag_value(response.headers());

        // Clear round-trip.
        let response = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic)
            .add_header(
                axum::http::header::IF_MATCH,
                axum::http::HeaderValue::from_str(&etag).unwrap(),
            )
            .json(&serde_json::json!({"subtitle": serde_json::Value::Null}))
            .await;
        assert_eq!(response.status_code(), StatusCode::OK);
        let body: serde_json::Value = response.json();
        assert_eq!(
            body["fields"]["subtitle"]["value"],
            serde_json::Value::Null,
            "cleared field must echo a null applied value"
        );
        assert_eq!(
            body["fields"]["subtitle"]["version_id"],
            serde_json::Value::Null,
            "a clear wires no canonical pointer"
        );
        assert_eq!(
            body["fields"]["subtitle"]["previous_version_id"],
            serde_json::json!(set_version_id),
            "clear must report the pointer that was canonical beforehand"
        );

        let row = sqlx::query!(
            "SELECT subtitle, subtitle_version_id FROM works WHERE id = $1",
            work_id,
        )
        .fetch_one(&app_pool)
        .await
        .expect("fetch work after clear");
        assert!(row.subtitle.is_none());
        assert!(row.subtitle_version_id.is_none());
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

    #[sqlx::test(migrations = "./migrations")]
    async fn patch_sets_and_clears_pages(pool: sqlx::PgPool) {
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
            .add_header(AUTHORIZATION, basic.clone())
            .add_header(
                axum::http::header::IF_MATCH,
                axum::http::HeaderValue::from_str(&etag).unwrap(),
            )
            .json(&serde_json::json!({"pages": 353}))
            .await;
        assert_eq!(response.status_code(), StatusCode::OK);

        let row = sqlx::query!(
            "SELECT pages, pages_version_id FROM manifestations WHERE id = $1",
            m_id,
        )
        .fetch_one(&ing_pool)
        .await
        .expect("fetch manifestation");
        assert_eq!(row.pages, Some(353));
        assert!(row.pages_version_id.is_some());

        let etag = etag_value(response.headers());
        let response = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic)
            .add_header(
                axum::http::header::IF_MATCH,
                axum::http::HeaderValue::from_str(&etag).unwrap(),
            )
            .json(&serde_json::json!({"pages": serde_json::Value::Null}))
            .await;
        assert_eq!(response.status_code(), StatusCode::OK);

        let row = sqlx::query!(
            "SELECT pages, pages_version_id FROM manifestations WHERE id = $1",
            m_id,
        )
        .fetch_one(&ing_pool)
        .await
        .expect("fetch manifestation after clear");
        assert!(row.pages.is_none());
        assert!(row.pages_version_id.is_none());
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn patch_pages_zero_returns_422(pool: sqlx::PgPool) {
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
            .json(&serde_json::json!({"pages": 0}))
            .await;
        assert_eq!(response.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn patch_pages_non_integer_returns_422(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (_work_id, m_id) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;

        let server = test_support::db::server_with_real_pools(&app_pool, &ing_pool);
        let response = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic)
            .add_header(
                axum::http::header::IF_MATCH,
                axum::http::HeaderValue::from_static("\"dummy\""),
            )
            .json(&serde_json::json!({"pages": "not-a-number"}))
            .await;
        assert_eq!(response.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn patch_ignores_field_locks_today(pool: sqlx::PgPool) {
        // Pre-existing gap (surfaced in the design, PATCH-vs-locks is out of
        // scope here): PATCH does not consult field_locks. This pins the
        // CURRENT behavior so a future fix is a deliberate, visible change.
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (work_id, m_id) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;
        sqlx::query!(
            "INSERT INTO field_locks (manifestation_id, entity_type, field_name) \
             VALUES ($1, 'work', 'contributors.author')",
            m_id,
        )
        .execute(&app_pool)
        .await
        .expect("insert lock");

        let name = format!("Locked Field Author {marker}");
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
            .json(&serde_json::json!({"contributors": {"author": [name.clone()]}}))
            .await;
        assert_eq!(
            response.status_code(),
            StatusCode::OK,
            "PATCH does not check field_locks today; body = {}",
            response.text()
        );

        let author_name = sqlx::query_scalar!(
            "SELECT a.name FROM work_authors wa \
             JOIN authors a ON a.id = wa.author_id \
             WHERE wa.work_id = $1 AND wa.role = 'author'",
            work_id,
        )
        .fetch_one(&app_pool)
        .await
        .expect("fetch author");
        assert_eq!(author_name, name);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn patch_contributors_bumps_work_updated_at(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (work_id, m_id) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;
        let before = sqlx::query_scalar!("SELECT updated_at FROM works WHERE id = $1", work_id)
            .fetch_one(&app_pool)
            .await
            .expect("fetch updated_at before");

        let name = format!("Timestamp Author {marker}");
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
            .json(&serde_json::json!({"contributors": {"author": [name]}}))
            .await;
        assert_eq!(response.status_code(), StatusCode::OK);

        let after = sqlx::query_scalar!("SELECT updated_at FROM works WHERE id = $1", work_id)
            .fetch_one(&app_pool)
            .await
            .expect("fetch updated_at after");
        assert!(
            after > before,
            "refresh_first_author_sort's UPDATE must bump updated_at via the trigger"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn accept_contributors_author_version_rebuilds_role_rows(pool: sqlx::PgPool) {
        // Contributor versions used to be accept-unsupported (422); the
        // shared apply path now rebuilds the role rows, so accepting a
        // pending contributors draft promotes it like any scalar field.
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (work_id, m_id) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;
        let author_name = format!("Pending Author {marker}");
        let version_id = insert_version(
            &ing_pool,
            m_id,
            "contributors.author",
            serde_json::json!([author_name.clone()]),
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
            "body = {}",
            response.text()
        );

        let author_names: Vec<String> = sqlx::query_scalar!(
            "SELECT a.name FROM authors a \
             JOIN work_authors wa ON wa.author_id = a.id \
             WHERE wa.work_id = $1 AND wa.role = 'author' \
             ORDER BY wa.position",
            work_id,
        )
        .fetch_all(&app_pool)
        .await
        .expect("fetch author names");
        assert_eq!(author_names, vec![author_name]);

        let stamps: Vec<Option<Uuid>> = sqlx::query_scalar!(
            "SELECT DISTINCT source_version_id FROM work_authors \
             WHERE work_id = $1 AND role = 'author'",
            work_id,
        )
        .fetch_all(&app_pool)
        .await
        .expect("fetch work_authors stamps");
        assert_eq!(stamps, vec![Some(version_id)]);
    }

    // ── PATCH genres / moods / tags / content_rating ───────────────────────

    #[sqlx::test(migrations = "./migrations")]
    async fn patch_genres_sets_journals_and_links(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
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
            .json(&serde_json::json!({"genres": ["Astrophysics", "Carpentry"]}))
            .await;
        assert_eq!(
            response.status_code(),
            StatusCode::OK,
            "body = {}",
            response.text()
        );

        let version = sqlx::query!(
            "SELECT id, status::text AS \"status!\", resolved_by \
             FROM metadata_versions \
             WHERE manifestation_id = $1 AND source = 'manual' AND field_name = 'genres'",
            m_id,
        )
        .fetch_one(&app_pool)
        .await
        .expect("fetch genres version");
        assert_eq!(version.status, "pending");
        assert_eq!(
            version.resolved_by,
            Some(admin_id),
            "resolved_by should record the operator"
        );

        // Owner pool: `manifestation_genres` is RLS-scoped, and this
        // assertion pool never sets the GUC the SELECT policy needs.
        let genre_names: Vec<String> = sqlx::query_scalar!(
            "SELECT g.name FROM genres g \
             JOIN manifestation_genres mg ON mg.genre_id = g.id \
             WHERE mg.manifestation_id = $1 ORDER BY g.name",
            m_id,
        )
        .fetch_all(&pool)
        .await
        .expect("fetch genre names");
        assert_eq!(
            genre_names,
            vec!["Astrophysics".to_string(), "Carpentry".to_string()]
        );

        let source_version_ids: Vec<Option<Uuid>> = sqlx::query_scalar!(
            "SELECT DISTINCT source_version_id FROM manifestation_genres \
             WHERE manifestation_id = $1",
            m_id,
        )
        .fetch_all(&pool)
        .await
        .expect("fetch junction source_version_id");
        assert_eq!(
            source_version_ids,
            vec![Some(version.id)],
            "both junction rows must carry the journal row's id"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn patch_genres_replaces_wholesale(pool: sqlx::PgPool) {
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
        let first = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
            .add_header(
                axum::http::header::IF_MATCH,
                axum::http::HeaderValue::from_str(&etag).unwrap(),
            )
            .json(&serde_json::json!({"genres": ["Astrophysics", "Carpentry"]}))
            .await;
        assert_eq!(first.status_code(), StatusCode::OK);
        let etag = etag_value(first.headers());

        let second = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic)
            .add_header(
                axum::http::header::IF_MATCH,
                axum::http::HeaderValue::from_str(&etag).unwrap(),
            )
            .json(&serde_json::json!({"genres": ["Numismatics"]}))
            .await;
        assert_eq!(second.status_code(), StatusCode::OK);

        // Owner pool: see the comment on the equivalent read above.
        let genre_names: Vec<String> = sqlx::query_scalar!(
            "SELECT g.name FROM genres g \
             JOIN manifestation_genres mg ON mg.genre_id = g.id \
             WHERE mg.manifestation_id = $1",
            m_id,
        )
        .fetch_all(&pool)
        .await
        .expect("fetch genre names after replace");
        assert_eq!(
            genre_names,
            vec!["Numismatics".to_string()],
            "second PATCH must replace the junction wholesale, not merge"
        );

        let orphaned_vocab_count: i64 = sqlx::query_scalar!(
            "SELECT count(*) AS \"count!\" FROM genres \
             WHERE lower(name) IN ('astrophysics', 'carpentry')",
        )
        .fetch_one(&app_pool)
        .await
        .expect("fetch orphaned vocab count");
        assert_eq!(
            orphaned_vocab_count, 2,
            "vocabulary rows are never garbage-collected"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn patch_moods_and_tags_smoke(pool: sqlx::PgPool) {
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
            .json(&serde_json::json!({"moods": ["Gloomy"], "tags": ["Signed"]}))
            .await;
        assert_eq!(
            response.status_code(),
            StatusCode::OK,
            "body = {}",
            response.text()
        );

        // Owner pool: see the comment on the equivalent genres read above.
        let mood_name: String = sqlx::query_scalar!(
            "SELECT mo.name FROM moods mo \
             JOIN manifestation_moods mm ON mm.mood_id = mo.id \
             WHERE mm.manifestation_id = $1",
            m_id,
        )
        .fetch_one(&pool)
        .await
        .expect("fetch mood name");
        assert_eq!(mood_name, "Gloomy");

        let tag_name: String = sqlx::query_scalar!(
            "SELECT t.name FROM tags t \
             JOIN manifestation_tags mt ON mt.tag_id = t.id \
             WHERE mt.manifestation_id = $1",
            m_id,
        )
        .fetch_one(&pool)
        .await
        .expect("fetch tag name");
        assert_eq!(tag_name, "Signed");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn patch_genres_null_clears_and_journals(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ing_pool);

        // `null` clears.
        let marker_a = Uuid::new_v4().simple().to_string();
        let (_work_a, m_a) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker_a).await;
        let initial_a = server
            .get(&format!("/api/v1/books/{m_a}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
            .await;
        let etag_a = etag_value(initial_a.headers());
        let set_a = server
            .patch(&format!("/api/v1/books/{m_a}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
            .add_header(
                axum::http::header::IF_MATCH,
                axum::http::HeaderValue::from_str(&etag_a).unwrap(),
            )
            .json(&serde_json::json!({"genres": ["Origami"]}))
            .await;
        assert_eq!(set_a.status_code(), StatusCode::OK);
        let etag_a = etag_value(set_a.headers());
        let clear_a = server
            .patch(&format!("/api/v1/books/{m_a}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
            .add_header(
                axum::http::header::IF_MATCH,
                axum::http::HeaderValue::from_str(&etag_a).unwrap(),
            )
            .json(&serde_json::json!({"genres": serde_json::Value::Null}))
            .await;
        assert_eq!(clear_a.status_code(), StatusCode::OK);

        let junction_count_a: i64 = sqlx::query_scalar!(
            "SELECT count(*) AS \"count!\" FROM manifestation_genres WHERE manifestation_id = $1",
            m_a,
        )
        .fetch_one(&app_pool)
        .await
        .expect("fetch junction count a");
        assert_eq!(junction_count_a, 0);

        let null_journal_a: i64 = sqlx::query_scalar!(
            "SELECT count(*) AS \"count!\" FROM metadata_versions \
             WHERE manifestation_id = $1 AND source = 'manual' \
               AND field_name = 'genres' AND new_value = 'null'::jsonb",
            m_a,
        )
        .fetch_one(&app_pool)
        .await
        .expect("fetch null journal count a");
        assert_eq!(null_journal_a, 1, "null-valued manual journal row missing");

        // Empty array must clear identically to `null`.
        let marker_b = Uuid::new_v4().simple().to_string();
        let (_work_b, m_b) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker_b).await;
        let initial_b = server
            .get(&format!("/api/v1/books/{m_b}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
            .await;
        let etag_b = etag_value(initial_b.headers());
        let set_b = server
            .patch(&format!("/api/v1/books/{m_b}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
            .add_header(
                axum::http::header::IF_MATCH,
                axum::http::HeaderValue::from_str(&etag_b).unwrap(),
            )
            .json(&serde_json::json!({"genres": ["Beekeeping"]}))
            .await;
        assert_eq!(set_b.status_code(), StatusCode::OK);
        let etag_b = etag_value(set_b.headers());
        let clear_b = server
            .patch(&format!("/api/v1/books/{m_b}/metadata"))
            .add_header(AUTHORIZATION, basic)
            .add_header(
                axum::http::header::IF_MATCH,
                axum::http::HeaderValue::from_str(&etag_b).unwrap(),
            )
            .json(&serde_json::json!({"genres": []}))
            .await;
        assert_eq!(clear_b.status_code(), StatusCode::OK);

        let junction_count_b: i64 = sqlx::query_scalar!(
            "SELECT count(*) AS \"count!\" FROM manifestation_genres WHERE manifestation_id = $1",
            m_b,
        )
        .fetch_one(&app_pool)
        .await
        .expect("fetch junction count b");
        assert_eq!(junction_count_b, 0, "[] must clear identically to null");

        let null_journal_b: i64 = sqlx::query_scalar!(
            "SELECT count(*) AS \"count!\" FROM metadata_versions \
             WHERE manifestation_id = $1 AND source = 'manual' \
               AND field_name = 'genres' AND new_value = 'null'::jsonb",
            m_b,
        )
        .fetch_one(&app_pool)
        .await
        .expect("fetch null journal count b");
        assert_eq!(null_journal_b, 1, "null-valued manual journal row missing");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn patch_genres_duplicate_case_insensitive_422(pool: sqlx::PgPool) {
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
            .json(&serde_json::json!({"genres": ["SciFi", "scifi"]}))
            .await;
        assert_eq!(response.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn patch_genres_over_cap_422(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (_work_id, m_id) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;

        let terms: Vec<String> = (0..51).map(|i| format!("VocabTerm{i}")).collect();
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
            .json(&serde_json::json!({"genres": terms}))
            .await;
        assert_eq!(response.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn patch_genres_repeat_save_dedupes_journal(pool: sqlx::PgPool) {
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
        let mut etag = etag_value(initial.headers());

        // Same order twice — must dedupe onto one journal row via observation_count.
        for _ in 0..2 {
            let response = server
                .patch(&format!("/api/v1/books/{m_id}/metadata"))
                .add_header(AUTHORIZATION, basic.clone())
                .add_header(
                    axum::http::header::IF_MATCH,
                    axum::http::HeaderValue::from_str(&etag).unwrap(),
                )
                .json(&serde_json::json!({"genres": ["Falconry", "Glassblowing"]}))
                .await;
            assert_eq!(response.status_code(), StatusCode::OK);
            etag = etag_value(response.headers());
        }

        // A distinct set, submitted, then reordered — the order-insensitive
        // hash must collapse the reorder onto that same second journal row.
        let first_order = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
            .add_header(
                axum::http::header::IF_MATCH,
                axum::http::HeaderValue::from_str(&etag).unwrap(),
            )
            .json(&serde_json::json!({"genres": ["Alchemy", "Basketry"]}))
            .await;
        assert_eq!(first_order.status_code(), StatusCode::OK);
        let etag = etag_value(first_order.headers());
        let reordered = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic)
            .add_header(
                axum::http::header::IF_MATCH,
                axum::http::HeaderValue::from_str(&etag).unwrap(),
            )
            .json(&serde_json::json!({"genres": ["Basketry", "Alchemy"]}))
            .await;
        assert_eq!(reordered.status_code(), StatusCode::OK);

        let rows = sqlx::query!(
            "SELECT observation_count, new_value AS \"new_value!\" FROM metadata_versions \
             WHERE manifestation_id = $1 AND source = 'manual' AND field_name = 'genres'",
            m_id,
        )
        .fetch_all(&app_pool)
        .await
        .expect("fetch genres journal rows");
        assert_eq!(
            rows.len(),
            2,
            "two distinct value sets must produce exactly two journal rows; got {}",
            rows.len()
        );

        let falconry_row = rows
            .iter()
            .find(|r| r.new_value.to_string().contains("Falconry"))
            .expect("falconry/glassblowing row present");
        assert_eq!(
            falconry_row.observation_count, 2,
            "repeat same-order save must bump observation_count, not insert a new row"
        );

        let alchemy_row = rows
            .iter()
            .find(|r| r.new_value.to_string().contains("Alchemy"))
            .expect("alchemy/basketry row present");
        assert_eq!(
            alchemy_row.observation_count, 2,
            "reordered save must collide on the order-insensitive hash, not insert a new row"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn patch_content_rating_set_and_clear(pool: sqlx::PgPool) {
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
        let set_response = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
            .add_header(
                axum::http::header::IF_MATCH,
                axum::http::HeaderValue::from_str(&etag).unwrap(),
            )
            .json(&serde_json::json!({"content_rating": "teen"}))
            .await;
        assert_eq!(
            set_response.status_code(),
            StatusCode::OK,
            "body = {}",
            set_response.text()
        );

        let row = sqlx::query!(
            "SELECT content_rating AS \"content_rating: ContentRating\", \
                    content_rating_version_id \
             FROM manifestations WHERE id = $1",
            m_id,
        )
        .fetch_one(&ing_pool)
        .await
        .expect("fetch manifestation after set");
        assert_eq!(row.content_rating, Some(ContentRating::Teen));
        let version_id = row
            .content_rating_version_id
            .expect("content_rating_version_id wired");

        let version = sqlx::query!(
            "SELECT source, field_name FROM metadata_versions WHERE id = $1",
            version_id,
        )
        .fetch_one(&app_pool)
        .await
        .expect("fetch content_rating version");
        assert_eq!(version.source, "manual");
        assert_eq!(version.field_name, "content_rating");

        let etag = etag_value(set_response.headers());
        let clear_response = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic)
            .add_header(
                axum::http::header::IF_MATCH,
                axum::http::HeaderValue::from_str(&etag).unwrap(),
            )
            .json(&serde_json::json!({"content_rating": serde_json::Value::Null}))
            .await;
        assert_eq!(clear_response.status_code(), StatusCode::OK);

        let row_after = sqlx::query!(
            "SELECT content_rating AS \"content_rating: ContentRating\", \
                    content_rating_version_id \
             FROM manifestations WHERE id = $1",
            m_id,
        )
        .fetch_one(&ing_pool)
        .await
        .expect("fetch manifestation after clear");
        assert!(row_after.content_rating.is_none());
        assert!(row_after.content_rating_version_id.is_none());
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn patch_content_rating_invalid_422(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (_work_id, m_id) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;

        let server = test_support::db::server_with_real_pools(&app_pool, &ing_pool);
        let response = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic)
            .add_header(
                axum::http::header::IF_MATCH,
                axum::http::HeaderValue::from_static("\"dummy\""),
            )
            .json(&serde_json::json!({"content_rating": "ultra_violent"}))
            .await;
        assert_eq!(response.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn lock_does_not_block_manual_content_rating(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (_work_id, m_id) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;

        let server = test_support::db::server_with_real_pools(&app_pool, &ing_pool);
        let lock_response = server
            .post(&format!("/api/v1/manifestations/{m_id}/metadata/lock"))
            .add_header(AUTHORIZATION, basic.clone())
            .json(&serde_json::json!({
                "field_name": "content_rating",
                "entity_type": "manifestation",
            }))
            .await;
        assert_eq!(
            lock_response.status_code(),
            StatusCode::CREATED,
            "body = {}",
            lock_response.text()
        );

        let initial = server
            .get(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
            .await;
        let etag = etag_value(initial.headers());
        let patch_response = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic)
            .add_header(
                axum::http::header::IF_MATCH,
                axum::http::HeaderValue::from_str(&etag).unwrap(),
            )
            .json(&serde_json::json!({"content_rating": "mature"}))
            .await;
        assert_eq!(
            patch_response.status_code(),
            StatusCode::OK,
            "locks gate automated enrichment only, not the manual PATCH surface; body = {}",
            patch_response.text()
        );

        let rating = sqlx::query_scalar!(
            "SELECT content_rating AS \"content_rating: ContentRating\" \
             FROM manifestations WHERE id = $1",
            m_id,
        )
        .fetch_one(&ing_pool)
        .await
        .expect("fetch content_rating");
        assert_eq!(rating, Some(ContentRating::Mature));
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn accept_genres_version_applies_junctions(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (_work_id, m_id) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;
        let version_id = insert_version(
            &ing_pool,
            m_id,
            "genres",
            serde_json::json!(["Falconry", "Glassblowing"]),
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
            "body = {}",
            response.text()
        );

        // Owner pool: see the comment on the equivalent read above.
        let genre_names: Vec<String> = sqlx::query_scalar!(
            "SELECT g.name FROM genres g \
             JOIN manifestation_genres mg ON mg.genre_id = g.id \
             WHERE mg.manifestation_id = $1 ORDER BY g.name",
            m_id,
        )
        .fetch_all(&pool)
        .await
        .expect("fetch genre names");
        assert_eq!(
            genre_names,
            vec!["Falconry".to_string(), "Glassblowing".to_string()]
        );

        let source_version_ids: Vec<Option<Uuid>> = sqlx::query_scalar!(
            "SELECT DISTINCT source_version_id FROM manifestation_genres \
             WHERE manifestation_id = $1",
            m_id,
        )
        .fetch_all(&pool)
        .await
        .expect("fetch junction source_version_id");
        assert_eq!(source_version_ids, vec![Some(version_id)]);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn revert_genres_to_null_clears(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (_work_id, m_id) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;
        let version_id = insert_version(
            &ing_pool,
            m_id,
            "genres",
            serde_json::json!(["Falconry", "Glassblowing"]),
        )
        .await;

        let server = test_support::db::server_with_real_pools(&app_pool, &ing_pool);
        let accept_response = server
            .post(&format!("/api/v1/manifestations/{m_id}/metadata/accept"))
            .add_header(AUTHORIZATION, basic.clone())
            .json(&serde_json::json!({"version_id": version_id}))
            .await;
        assert_eq!(accept_response.status_code(), StatusCode::OK);

        let revert_response = server
            .post(&format!("/api/v1/manifestations/{m_id}/metadata/revert"))
            .add_header(AUTHORIZATION, basic)
            .json(&serde_json::json!({
                "field_name": "genres",
                "version_id": serde_json::Value::Null,
            }))
            .await;
        assert_eq!(
            revert_response.status_code(),
            StatusCode::OK,
            "body = {}",
            revert_response.text()
        );

        let junction_count: i64 = sqlx::query_scalar!(
            "SELECT count(*) AS \"count!\" FROM manifestation_genres WHERE manifestation_id = $1",
            m_id,
        )
        .fetch_one(&app_pool)
        .await
        .expect("fetch junction count after revert");
        assert_eq!(
            junction_count, 0,
            "revert-to-null must clear the junction rows"
        );
    }

    // ── external identifiers: manual write paths ──────────────────────────

    /// Force a failed, backed-off enrichment state so a test can assert the
    /// identifier re-queue clears the whole backoff window, not just status.
    async fn set_enrichment_backoff(ing_pool: &sqlx::PgPool, m_id: Uuid) {
        sqlx::query!(
            "UPDATE manifestations \
             SET enrichment_status = 'failed', \
                 enrichment_attempt_count = 3, \
                 enrichment_attempted_at = now(), \
                 enrichment_error = 'boom' \
             WHERE id = $1",
            m_id,
        )
        .execute(ing_pool)
        .await
        .expect("preset backoff state");
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
    async fn patch_sets_work_identifier_journals_and_requeues(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (work_id, m_id) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;
        set_enrichment_backoff(&ing_pool, m_id).await;

        let server = test_support::db::server_with_real_pools(&app_pool, &ing_pool);
        let initial = server
            .get(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
            .await;
        let etag = etag_value(initial.headers());
        // Lower-case input: the parser canonicalises Open Library ids to
        // upper case before journaling and storage.
        let response = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic)
            .add_header(
                axum::http::header::IF_MATCH,
                axum::http::HeaderValue::from_str(&etag).unwrap(),
            )
            .json(&serde_json::json!({"work_identifiers": {"openlibrary": "ol45804w"}}))
            .await;
        assert_eq!(
            response.status_code(),
            StatusCode::OK,
            "body = {}",
            response.text()
        );
        let body: serde_json::Value = response.json();
        let field = "identifiers.work.openlibrary";
        assert_eq!(
            body["fields"][field]["value"],
            serde_json::json!("OL45804W")
        );
        let version_id = version_id_of(&body, field);

        let row = sqlx::query!(
            "SELECT external_id, source_version_id FROM work_external_identifiers \
             WHERE work_id = $1 AND scheme = 'openlibrary'",
            work_id,
        )
        .fetch_one(&pool)
        .await
        .expect("registry row");
        assert_eq!(row.external_id, "OL45804W");
        assert_eq!(
            row.source_version_id.map(|u| u.to_string()),
            Some(version_id.clone()),
            "registry row must point at the journal row"
        );

        let v = sqlx::query!(
            "SELECT source, field_name, status::text AS \"status!\" \
             FROM metadata_versions WHERE id = $1::uuid",
            Uuid::parse_str(&version_id).expect("uuid"),
        )
        .fetch_one(&pool)
        .await
        .expect("journal row");
        assert_eq!(v.source, "manual");
        assert_eq!(v.field_name, field);
        assert_eq!(v.status, "pending");

        let m = sqlx::query!(
            "SELECT enrichment_status::text AS \"enrichment_status!\", \
                    enrichment_attempt_count, enrichment_attempted_at, enrichment_error \
             FROM manifestations WHERE id = $1",
            m_id,
        )
        .fetch_one(&pool)
        .await
        .expect("manifestation row");
        assert_eq!(m.enrichment_status, "pending");
        assert_eq!(
            m.enrichment_attempt_count, 0,
            "re-queue must clear the backoff counter"
        );
        assert!(
            m.enrichment_attempted_at.is_none(),
            "re-queue must null the attempt timestamp"
        );
        assert!(m.enrichment_error.is_none());

        assert_eq!(
            writeback_job_count(&app_pool, m_id).await,
            0,
            "identifier set must not enqueue a writeback"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn patch_sets_manifestation_identifier_and_requeues(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (_work_id, m_id) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;
        set_enrichment_backoff(&ing_pool, m_id).await;

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
            .json(
                &serde_json::json!({"manifestation_identifiers": {"googlebooks": "zyTZAAAAYAAJ"}}),
            )
            .await;
        assert_eq!(
            response.status_code(),
            StatusCode::OK,
            "body = {}",
            response.text()
        );

        let got: String = sqlx::query_scalar!(
            "SELECT external_id FROM manifestation_external_identifiers \
             WHERE manifestation_id = $1 AND scheme = 'googlebooks'",
            m_id,
        )
        .fetch_one(&pool)
        .await
        .expect("registry row");
        assert_eq!(got, "zyTZAAAAYAAJ");

        let m = sqlx::query!(
            "SELECT enrichment_status::text AS \"enrichment_status!\", \
                    enrichment_attempt_count, enrichment_attempted_at, \
                    enrichment_rerun_requested \
             FROM manifestations WHERE id = $1",
            m_id,
        )
        .fetch_one(&pool)
        .await
        .expect("manifestation row");
        assert_eq!(m.enrichment_status, "pending");
        assert_eq!(m.enrichment_attempt_count, 0);
        assert!(m.enrichment_attempted_at.is_none());
        assert!(
            !m.enrichment_rerun_requested,
            "an idle-row edit re-queues directly; no rerun request is left behind"
        );
    }

    /// An identifier edit while enrichment is actively running must not
    /// release the worker's claim (a second worker could pick the row up
    /// concurrently). The edit records a rerun request instead; the queue's
    /// completion bookkeeping converts it into a fresh pending row.
    #[sqlx::test(migrations = "./migrations")]
    async fn patch_identifier_during_active_run_defers_requeue(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (_work_id, m_id) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;
        sqlx::query!(
            "UPDATE manifestations \
             SET enrichment_status = 'in_progress', \
                 enrichment_attempt_count = 1, \
                 enrichment_attempted_at = now() \
             WHERE id = $1",
            m_id,
        )
        .execute(&ing_pool)
        .await
        .expect("preset in_progress state");

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
            .json(
                &serde_json::json!({"manifestation_identifiers": {"googlebooks": "zyTZAAAAYAAJ"}}),
            )
            .await;
        assert_eq!(
            response.status_code(),
            StatusCode::OK,
            "body = {}",
            response.text()
        );

        let m = sqlx::query!(
            "SELECT enrichment_status::text AS \"enrichment_status!\", \
                    enrichment_attempt_count, enrichment_attempted_at, \
                    enrichment_rerun_requested \
             FROM manifestations WHERE id = $1",
            m_id,
        )
        .fetch_one(&pool)
        .await
        .expect("manifestation row");
        assert_eq!(
            m.enrichment_status, "in_progress",
            "the edit must not release the active claim"
        );
        assert!(
            m.enrichment_rerun_requested,
            "the edit must leave a rerun request for the completion path"
        );
        assert_eq!(
            m.enrichment_attempt_count, 1,
            "the active claim's attempt counter is untouched"
        );
        assert!(
            m.enrichment_attempted_at.is_some(),
            "the active claim's timestamp is untouched"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn patch_replaces_identifier_slot_in_place(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (work_id, m_id) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ing_pool);

        let initial = server
            .get(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
            .await;
        let etag = etag_value(initial.headers());
        let first = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
            .add_header(
                axum::http::header::IF_MATCH,
                axum::http::HeaderValue::from_str(&etag).unwrap(),
            )
            .json(&serde_json::json!({"work_identifiers": {"openlibrary": "OL111W"}}))
            .await;
        assert_eq!(first.status_code(), StatusCode::OK);
        let first_body: serde_json::Value = first.json();
        let first_version = version_id_of(&first_body, "identifiers.work.openlibrary");
        let etag = etag_value(first.headers());

        let second = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic)
            .add_header(
                axum::http::header::IF_MATCH,
                axum::http::HeaderValue::from_str(&etag).unwrap(),
            )
            .json(&serde_json::json!({"work_identifiers": {"openlibrary": "OL222W"}}))
            .await;
        assert_eq!(second.status_code(), StatusCode::OK);
        let second_body: serde_json::Value = second.json();
        assert_eq!(
            second_body["fields"]["identifiers.work.openlibrary"]["previous_version_id"],
            serde_json::json!(first_version),
            "replacement must report the replaced journal pointer"
        );

        let rows: Vec<String> = sqlx::query_scalar!(
            "SELECT external_id FROM work_external_identifiers \
             WHERE work_id = $1 AND scheme = 'openlibrary'",
            work_id,
        )
        .fetch_all(&pool)
        .await
        .expect("select");
        assert_eq!(
            rows,
            vec!["OL222W".to_string()],
            "single slot: old and new must not both persist"
        );
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
            body["fields"]["identifiers.manifestation.asin"]["value"],
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
    async fn patch_identifier_unknown_scheme_returns_422(pool: sqlx::PgPool) {
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

        // `manual` is a provenance source, not an identifier scheme; clearing
        // an unknown scheme is rejected identically to setting one.
        for payload in [
            serde_json::json!({"work_identifiers": {"manual": "x1"}}),
            serde_json::json!({"manifestation_identifiers": {"manual": null}}),
        ] {
            let response = server
                .patch(&format!("/api/v1/books/{m_id}/metadata"))
                .add_header(AUTHORIZATION, basic.clone())
                .add_header(
                    axum::http::header::IF_MATCH,
                    axum::http::HeaderValue::from_str(&etag).unwrap(),
                )
                .json(&payload)
                .await;
            assert_eq!(
                response.status_code(),
                StatusCode::UNPROCESSABLE_ENTITY,
                "payload {payload} must be rejected; body = {}",
                response.text()
            );
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn patch_identifier_level_mismatch_returns_422(pool: sqlx::PgPool) {
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

        for payload in [
            // A Google Books volume id has no work-level form.
            serde_json::json!({"work_identifiers": {"googlebooks": "zyTZAAAAYAAJ"}}),
            // An Open Library edition id in the work map and vice versa.
            serde_json::json!({"work_identifiers": {"openlibrary": "OL7353617M"}}),
            serde_json::json!({"manifestation_identifiers": {"openlibrary": "OL45804W"}}),
        ] {
            let response = server
                .patch(&format!("/api/v1/books/{m_id}/metadata"))
                .add_header(AUTHORIZATION, basic.clone())
                .add_header(
                    axum::http::header::IF_MATCH,
                    axum::http::HeaderValue::from_str(&etag).unwrap(),
                )
                .json(&payload)
                .await;
            assert_eq!(
                response.status_code(),
                StatusCode::UNPROCESSABLE_ENTITY,
                "payload {payload} must be rejected; body = {}",
                response.text()
            );
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn patch_identifier_malformed_value_returns_422(pool: sqlx::PgPool) {
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

        for bad in ["OL45804W/../x", "OL45804W?x=1", "OL45804W\u{0}", "OL४५W"] {
            let response = server
                .patch(&format!("/api/v1/books/{m_id}/metadata"))
                .add_header(AUTHORIZATION, basic.clone())
                .add_header(
                    axum::http::header::IF_MATCH,
                    axum::http::HeaderValue::from_str(&etag).unwrap(),
                )
                .json(&serde_json::json!({"work_identifiers": {"openlibrary": bad}}))
                .await;
            assert_eq!(
                response.status_code(),
                StatusCode::UNPROCESSABLE_ENTITY,
                "{bad:?} must be rejected; body = {}",
                response.text()
            );
        }
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

    #[sqlx::test(migrations = "./migrations")]
    async fn revert_identifier_restores_prior_value(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (work_id, m_id) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ing_pool);

        let initial = server
            .get(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
            .await;
        let etag = etag_value(initial.headers());
        let first = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
            .add_header(
                axum::http::header::IF_MATCH,
                axum::http::HeaderValue::from_str(&etag).unwrap(),
            )
            .json(&serde_json::json!({"work_identifiers": {"openlibrary": "OL111W"}}))
            .await;
        assert_eq!(first.status_code(), StatusCode::OK);
        let first_body: serde_json::Value = first.json();
        let first_version = version_id_of(&first_body, "identifiers.work.openlibrary");
        let etag = etag_value(first.headers());

        let second = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
            .add_header(
                axum::http::header::IF_MATCH,
                axum::http::HeaderValue::from_str(&etag).unwrap(),
            )
            .json(&serde_json::json!({"work_identifiers": {"openlibrary": "OL222W"}}))
            .await;
        assert_eq!(second.status_code(), StatusCode::OK);

        let revert = server
            .post(&format!("/api/v1/manifestations/{m_id}/metadata/revert"))
            .add_header(AUTHORIZATION, basic)
            .json(&serde_json::json!({
                "field_name": "identifiers.work.openlibrary",
                "version_id": first_version,
            }))
            .await;
        assert_eq!(
            revert.status_code(),
            StatusCode::OK,
            "revert of an identifier version must round-trip; body = {}",
            revert.text()
        );

        let got: String = sqlx::query_scalar!(
            "SELECT external_id FROM work_external_identifiers \
             WHERE work_id = $1 AND scheme = 'openlibrary'",
            work_id,
        )
        .fetch_one(&pool)
        .await
        .expect("registry row");
        assert_eq!(got, "OL111W", "revert must restore the prior value");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn revert_identifier_to_null_clears_slot(pool: sqlx::PgPool) {
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
            .json(&serde_json::json!({"manifestation_identifiers": {"goodreads": "5907"}}))
            .await;
        assert_eq!(set.status_code(), StatusCode::OK);

        let revert = server
            .post(&format!("/api/v1/manifestations/{m_id}/metadata/revert"))
            .add_header(AUTHORIZATION, basic)
            .json(&serde_json::json!({
                "field_name": "identifiers.manifestation.goodreads",
                "version_id": null,
            }))
            .await;
        assert_eq!(
            revert.status_code(),
            StatusCode::OK,
            "revert-to-null of an identifier must round-trip; body = {}",
            revert.text()
        );

        let remaining: i64 = sqlx::query_scalar!(
            "SELECT count(*) AS \"count!\" FROM manifestation_external_identifiers \
             WHERE manifestation_id = $1 AND scheme = 'goodreads'",
            m_id,
        )
        .fetch_one(&pool)
        .await
        .expect("count");
        assert_eq!(remaining, 0);

        assert_eq!(
            writeback_job_count(&app_pool, m_id).await,
            0,
            "identifier clear via revert must not enqueue a writeback"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn revert_work_identifier_accepts_version_from_sibling_manifestation(pool: sqlx::PgPool) {
        // Work-level identifiers live on the shared work row, so a version
        // journaled under one edition is a valid revert target from another.
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (work_id, m_a) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;
        let m_b = insert_sibling_manifestation(&ing_pool, work_id).await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ing_pool);

        let initial = server
            .get(&format!("/api/v1/books/{m_a}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
            .await;
        let etag = etag_value(initial.headers());
        let set = server
            .patch(&format!("/api/v1/books/{m_a}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
            .add_header(
                axum::http::header::IF_MATCH,
                axum::http::HeaderValue::from_str(&etag).unwrap(),
            )
            .json(&serde_json::json!({"work_identifiers": {"openlibrary": "OL333W"}}))
            .await;
        assert_eq!(set.status_code(), StatusCode::OK);
        let set_body: serde_json::Value = set.json();
        let version = version_id_of(&set_body, "identifiers.work.openlibrary");

        let revert = server
            .post(&format!("/api/v1/manifestations/{m_b}/metadata/revert"))
            .add_header(AUTHORIZATION, basic)
            .json(&serde_json::json!({
                "field_name": "identifiers.work.openlibrary",
                "version_id": version,
            }))
            .await;
        assert_eq!(
            revert.status_code(),
            StatusCode::OK,
            "work-scoped identifier version journaled under a sibling must be accepted; body = {}",
            revert.text()
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
    async fn get_book_metadata_emits_etag(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (_work_id, m_id) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ing_pool);

        let r = server
            .get(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic)
            .await;
        assert_eq!(r.status_code(), StatusCode::OK);
        let etag = etag_value(r.headers());
        assert!(etag.starts_with('"') && etag.ends_with('"'));
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn get_manifestation_metadata_emits_no_etag(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (_work_id, m_id) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ing_pool);

        let r = server
            .get(&format!("/api/v1/manifestations/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic)
            .await;
        assert_eq!(r.status_code(), StatusCode::OK);
        assert!(
            r.headers().get(axum::http::header::ETAG).is_none(),
            "the review-queue GET is unprotected and must not carry an ETag"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn get_book_metadata_etag_stable_across_identical_reads(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (_work_id, m_id) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ing_pool);

        let first = server
            .get(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
            .await;
        let second = server
            .get(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic)
            .await;
        assert_eq!(etag_value(first.headers()), etag_value(second.headers()));
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn patch_metadata_bumps_etag_after_write(pool: sqlx::PgPool) {
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
        let before_etag = etag_value(before.headers());

        let r = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic)
            .add_header(
                axum::http::header::IF_MATCH,
                axum::http::HeaderValue::from_str(&before_etag).unwrap(),
            )
            .json(&serde_json::json!({"title": format!("Bumped {marker}")}))
            .await;
        assert_eq!(r.status_code(), StatusCode::OK, "body: {}", r.text());
        let after_etag = etag_value(r.headers());
        assert_ne!(
            before_etag, after_etag,
            "a real field change must change the ETag"
        );
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

    // Both grammar violations and the well-formed forms this API refuses by
    // policy are defects in the request's own shape, so they are 400 rather
    // than the 412 a false precondition earns. No refreshed tag would make
    // any of these parse.
    #[sqlx::test(migrations = "./migrations")]
    async fn patch_metadata_with_refused_if_match_returns_400(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (_work_id, m_id) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ing_pool);

        for (label, if_match) in [
            ("SP is not etagc", "\"a b\""),
            ("wildcard", "*"),
            ("entity-tag list", "\"abc\", \"def\""),
            ("weak validator", "W/\"abc\""),
            ("unquoted", "abc"),
        ] {
            let r = server
                .patch(&format!("/api/v1/books/{m_id}/metadata"))
                .add_header(AUTHORIZATION, basic.clone())
                .add_header(
                    axum::http::header::IF_MATCH,
                    axum::http::HeaderValue::from_str(if_match).unwrap(),
                )
                .json(&serde_json::json!({"title": format!("Refused {marker}")}))
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

    // `obs-text` is valid entity-tag content, so such a tag is parsed and
    // compared rather than refused. Reverie's own validators are ASCII, so
    // it compares unequal and earns the ordinary 412.
    #[sqlx::test(migrations = "./migrations")]
    async fn patch_metadata_with_obs_text_if_match_reaches_comparison(pool: sqlx::PgPool) {
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
            .add_header(
                axum::http::header::IF_MATCH,
                axum::http::HeaderValue::from_bytes(b"\"\x80obs-text\"").unwrap(),
            )
            .json(&serde_json::json!({"title": format!("Obs text {marker}")}))
            .await;
        test_support::assert_problem(
            &r,
            problems::IF_MATCH_MISMATCH,
            StatusCode::PRECONDITION_FAILED,
        );
    }

    // A request whose header and body are both defective is answered for the
    // header: 422 is scoped to content that parsed, and this request's own
    // shape failed first.
    #[sqlx::test(migrations = "./migrations")]
    async fn patch_metadata_refused_if_match_precedes_body_validation(pool: sqlx::PgPool) {
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
            .add_header(
                axum::http::header::IF_MATCH,
                axum::http::HeaderValue::from_static("\"a b\""),
            )
            .json(&serde_json::json!({}))
            .await;
        test_support::assert_problem(&r, problems::MALFORMED_HEADER, StatusCode::BAD_REQUEST);
    }

    // An accepted tag is compared before the content is processed, so the
    // same invalid body resolves differently by precondition outcome: the
    // matching tag reaches body validation and earns 422, while the stale
    // tag stops at the precondition and earns 412. Evaluating the body
    // first would collapse both rows to 422 and hide a stale representation
    // behind a body error.
    #[sqlx::test(migrations = "./migrations")]
    async fn patch_metadata_precondition_is_evaluated_before_the_body(pool: sqlx::PgPool) {
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
        let matching = etag_value(initial.headers());

        let r = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
            .add_header(
                axum::http::header::IF_MATCH,
                axum::http::HeaderValue::from_str(&matching).unwrap(),
            )
            .json(&serde_json::json!({}))
            .await;
        assert_eq!(
            r.status_code(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "matching tag must reach body validation, body: {}",
            r.text()
        );

        let r = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic)
            .add_header(
                axum::http::header::IF_MATCH,
                axum::http::HeaderValue::from_static("\"stale\""),
            )
            .json(&serde_json::json!({}))
            .await;
        test_support::assert_problem(
            &r,
            problems::IF_MATCH_MISMATCH,
            StatusCode::PRECONDITION_FAILED,
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
        async fn genres_adult_reads_all(pool: PgPool) {
            let ingestion = ingestion_pool_for(&pool).await;
            let app = app_pool_for(&pool).await;
            let (_w, m) = insert_work_and_manifestation(&ingestion, "genre-adult").await;
            let genre = seed_genre(&ingestion, "genre-adult-scope").await;
            link_genre(&ingestion, m, genre).await;

            let (adult_id, _) = create_adult_and_basic_auth(&app, "genre-adult").await;
            let mut tx = acquire_with_rls(&app, adult_id).await.expect("rls tx");
            let count: i64 = sqlx::query_scalar!(
                "SELECT count(*) AS \"c!\" FROM manifestation_genres WHERE manifestation_id = $1",
                m,
            )
            .fetch_one(&mut *tx)
            .await
            .expect("select");
            assert_eq!(count, 1, "adult sees all genre links");
        }

        #[sqlx::test(migrations = "./migrations")]
        async fn genres_readonly_reads_scoped_but_cannot_write(pool: PgPool) {
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

        #[sqlx::test(migrations = "./migrations")]
        async fn genres_ingestion_inserts_and_deletes_unimpeded(pool: PgPool) {
            let ingestion = ingestion_pool_for(&pool).await;
            let (_w, m) = insert_work_and_manifestation(&ingestion, "genre-ing").await;
            let genre = seed_genre(&ingestion, "genre-ing-scope").await;
            link_genre(&ingestion, m, genre).await;

            let done = sqlx::query!(
                "DELETE FROM manifestation_genres WHERE manifestation_id = $1 AND genre_id = $2",
                m,
                genre,
            )
            .execute(&ingestion)
            .await
            .expect("ingestion delete succeeds");
            assert_eq!(done.rows_affected(), 1);
        }

        #[sqlx::test(migrations = "./migrations")]
        async fn genres_rls_enabled_fresh_child_sees_no_rows(pool: PgPool) {
            let ingestion = ingestion_pool_for(&pool).await;
            let app = app_pool_for(&pool).await;
            let (_w, m) = insert_work_and_manifestation(&ingestion, "genre-fresh").await;
            let genre = seed_genre(&ingestion, "genre-fresh-scope").await;
            link_genre(&ingestion, m, genre).await;

            let (child_id, _) = create_child_user_and_basic_auth(&app, "genre-fresh-child").await;
            let mut tx = acquire_with_rls(&app, child_id).await.expect("rls tx");
            let count: i64 =
                sqlx::query_scalar!("SELECT count(*) AS \"c!\" FROM manifestation_genres")
                    .fetch_one(&mut *tx)
                    .await
                    .expect("select");
            assert_eq!(
                count, 0,
                "RLS ENABLE hides all rows from a shelf-less child"
            );
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
        async fn moods_adult_reads_all(pool: PgPool) {
            let ingestion = ingestion_pool_for(&pool).await;
            let app = app_pool_for(&pool).await;
            let (_w, m) = insert_work_and_manifestation(&ingestion, "mood-adult").await;
            let mood = seed_mood(&ingestion, "mood-adult-scope").await;
            link_mood(&ingestion, m, mood).await;

            let (adult_id, _) = create_adult_and_basic_auth(&app, "mood-adult").await;
            let mut tx = acquire_with_rls(&app, adult_id).await.expect("rls tx");
            let count: i64 = sqlx::query_scalar!(
                "SELECT count(*) AS \"c!\" FROM manifestation_moods WHERE manifestation_id = $1",
                m,
            )
            .fetch_one(&mut *tx)
            .await
            .expect("select");
            assert_eq!(count, 1, "adult sees all mood links");
        }

        #[sqlx::test(migrations = "./migrations")]
        async fn moods_readonly_reads_scoped_but_cannot_write(pool: PgPool) {
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

        #[sqlx::test(migrations = "./migrations")]
        async fn moods_ingestion_inserts_and_deletes_unimpeded(pool: PgPool) {
            let ingestion = ingestion_pool_for(&pool).await;
            let (_w, m) = insert_work_and_manifestation(&ingestion, "mood-ing").await;
            let mood = seed_mood(&ingestion, "mood-ing-scope").await;
            link_mood(&ingestion, m, mood).await;

            let done = sqlx::query!(
                "DELETE FROM manifestation_moods WHERE manifestation_id = $1 AND mood_id = $2",
                m,
                mood,
            )
            .execute(&ingestion)
            .await
            .expect("ingestion delete succeeds");
            assert_eq!(done.rows_affected(), 1);
        }

        #[sqlx::test(migrations = "./migrations")]
        async fn moods_rls_enabled_fresh_child_sees_no_rows(pool: PgPool) {
            let ingestion = ingestion_pool_for(&pool).await;
            let app = app_pool_for(&pool).await;
            let (_w, m) = insert_work_and_manifestation(&ingestion, "mood-fresh").await;
            let mood = seed_mood(&ingestion, "mood-fresh-scope").await;
            link_mood(&ingestion, m, mood).await;

            let (child_id, _) = create_child_user_and_basic_auth(&app, "mood-fresh-child").await;
            let mut tx = acquire_with_rls(&app, child_id).await.expect("rls tx");
            let count: i64 =
                sqlx::query_scalar!("SELECT count(*) AS \"c!\" FROM manifestation_moods")
                    .fetch_one(&mut *tx)
                    .await
                    .expect("select");
            assert_eq!(
                count, 0,
                "RLS ENABLE hides all rows from a shelf-less child"
            );
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
        async fn tags_adult_reads_all(pool: PgPool) {
            let ingestion = ingestion_pool_for(&pool).await;
            let app = app_pool_for(&pool).await;
            let (_w, m) = insert_work_and_manifestation(&ingestion, "tag-adult").await;
            let tag = seed_tag(&ingestion, "tag-adult-scope").await;
            link_tag(&ingestion, m, tag).await;

            let (adult_id, _) = create_adult_and_basic_auth(&app, "tag-adult").await;
            let mut tx = acquire_with_rls(&app, adult_id).await.expect("rls tx");
            let count: i64 = sqlx::query_scalar!(
                "SELECT count(*) AS \"c!\" FROM manifestation_tags WHERE manifestation_id = $1",
                m,
            )
            .fetch_one(&mut *tx)
            .await
            .expect("select");
            assert_eq!(count, 1, "adult sees all tag links");
        }

        #[sqlx::test(migrations = "./migrations")]
        async fn tags_readonly_reads_scoped_but_cannot_write(pool: PgPool) {
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

        #[sqlx::test(migrations = "./migrations")]
        async fn tags_ingestion_inserts_and_deletes_unimpeded(pool: PgPool) {
            let ingestion = ingestion_pool_for(&pool).await;
            let (_w, m) = insert_work_and_manifestation(&ingestion, "tag-ing").await;
            let tag = seed_tag(&ingestion, "tag-ing-scope").await;
            link_tag(&ingestion, m, tag).await;

            let done = sqlx::query!(
                "DELETE FROM manifestation_tags WHERE manifestation_id = $1 AND tag_id = $2",
                m,
                tag,
            )
            .execute(&ingestion)
            .await
            .expect("ingestion delete succeeds");
            assert_eq!(done.rows_affected(), 1);
        }

        #[sqlx::test(migrations = "./migrations")]
        async fn tags_rls_enabled_fresh_child_sees_no_rows(pool: PgPool) {
            let ingestion = ingestion_pool_for(&pool).await;
            let app = app_pool_for(&pool).await;
            let (_w, m) = insert_work_and_manifestation(&ingestion, "tag-fresh").await;
            let tag = seed_tag(&ingestion, "tag-fresh-scope").await;
            link_tag(&ingestion, m, tag).await;

            let (child_id, _) = create_child_user_and_basic_auth(&app, "tag-fresh-child").await;
            let mut tx = acquire_with_rls(&app, child_id).await.expect("rls tx");
            let count: i64 =
                sqlx::query_scalar!("SELECT count(*) AS \"c!\" FROM manifestation_tags")
                    .fetch_one(&mut *tx)
                    .await
                    .expect("select");
            assert_eq!(
                count, 0,
                "RLS ENABLE hides all rows from a shelf-less child"
            );
        }
    }
}
