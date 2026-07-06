//! Metadata review endpoints.
//!
//! All routes require an authenticated non-child user.  Write paths open a
//! transaction, `SELECT ... FOR UPDATE` on the owning entity, apply the change,
//! and commit.

use std::collections::BTreeMap;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
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
use crate::models::work;
use crate::services::enrichment::field_lock::{self, EntityType};
use crate::services::enrichment::value_hash;
use crate::services::metadata::isbn;
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
        .routes(routes!(update_book_metadata))
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
            // Work-scoped fields (title/subtitle/description/language,
            // contributors.*) are journaled per-manifestation but their
            // canonical pointer lives on the shared `works` row, so a
            // sibling manifestation of the same work can legitimately own
            // the version an editor now wants to restore: a first-ingest
            // draft filed under manifestation A, an enrichment pass, or a
            // manual edit made while reviewing manifestation B all land
            // in `metadata_versions` rows stamped with that manifestation's
            // id, not the one the canonical pointer was last read through.
            // Manifestation-scoped fields (pages, publisher, isbn_10,
            // isbn_13, pub_date, content_rating) keep the strict same-row
            // match: their canonical column lives on `manifestations`
            // itself, so a sibling's journal row describes a different
            // edition and must not be applicable here.
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
fn is_work_scoped_field(field_name: &str) -> bool {
    matches!(
        field_name,
        "title" | "subtitle" | "description" | "language"
    ) || field_name.starts_with("contributors.")
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
            let previous = capture_role_pointer(tx, work_id, role).await?;
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
            previous
        }
        other => {
            return Err(AppError::Validation(format!(
                "unsupported auto-apply field '{other}' (list/complex fields must be accepted via their dedicated routes)"
            )));
        }
    };
    enqueue_writeback(tx, manifestation_id, field).await?;
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
                    let previous = capture_role_pointer(tx, work_id, role).await?;
                    delete_role_rows(tx, work_id, role).await?;
                    previous
                }
                other => {
                    return Err(AppError::Validation(format!(
                        "unsupported contributor role '{other}'"
                    )));
                }
            }
        }
        other => {
            return Err(AppError::Validation(format!("unsupported field '{other}'")));
        }
    };
    enqueue_writeback(tx, manifestation_id, field).await?;
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
///   (canonical title is `NOT NULL` on `works`).
/// - [`AppError::NotFound`] when the manifestation is missing or hidden
///   by RLS for the current user (existence-not-leaked).
/// - [`AppError::Forbidden`] when the caller is a child account.
/// - [`AppError::Internal`] on database errors.
#[utoipa::path(
    patch,
    path = "/api/v1/books/{id}/metadata",
    tag = "metadata",
    security(("session_cookie" = ["write"]), ("device_token_bearer" = ["write"]), ("oidc_jwt_bearer" = ["write"]), ("opds_basic" = ["write"])),
    params(("id" = Uuid, Path, description = "Manifestation id")),
    request_body(content = UpdateMetadataFields, description = "RFC 7396 JSON Merge Patch: absent fields are unchanged, `null` clears (except `title`)"),
    responses(
        (status = 200, description = "Manual edit recorded as a `manual` metadata version and promoted to canonical (or cleared); body carries the applied value and version pointers per field", body = UpdateMetadataResponse),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails),
        (status = 403, description = "Caller is a child account", body = crate::openapi::ProblemDetails),
        (status = 404, description = "Manifestation missing or RLS-hidden (existence-not-leaked)", body = crate::openapi::ProblemDetails),
        (status = 422, description = "No populated fields, ISBN/date parse failure, or attempt to clear title", body = crate::openapi::ProblemDetails)
    )
)]
async fn update_book_metadata(
    current_user: CurrentUser,
    State(state): State<AppState>,
    Path(manifestation_id): Path<Uuid>,
    body: Result<axum::Json<UpdateMetadataFields>, axum::extract::rejection::JsonRejection>,
) -> Result<impl IntoResponse, AppError> {
    current_user.require_scope(Scope::Write)?;
    current_user.require_not_child()?;
    let axum::Json(mut req_fields) = body.map_err(|e| AppError::Validation(e.body_text()))?;
    // Extract contributors BEFORE populated() consumes the struct — it is
    // handled separately from the other (scalar) fields.
    let contributors = req_fields.contributors.take();
    let genres = req_fields.genres.take();
    let moods = req_fields.moods.take();
    let tags = req_fields.tags.take();
    let fields = req_fields.populated();
    if fields.is_empty()
        && contributors.is_none()
        && genres.is_none()
        && moods.is_none()
        && tags.is_none()
    {
        return Err(AppError::Validation("no fields".into()));
    }

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

    if touched_isbn {
        work::rematch_on_isbn_change(&mut tx, manifestation_id)
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
    }

    tx.commit()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    Ok((
        StatusCode::OK,
        axum::Json(UpdateMetadataResponse {
            fields: response_fields,
        }),
    ))
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

fn parse_iso_date(s: &str) -> Result<time::Date, time::error::Parse> {
    use time::format_description::well_known::Iso8601;
    // `s.len()` is in bytes; user-submitted strings can contain multi-byte
    // UTF-8 codepoints. `is_char_boundary` keeps the slice valid.
    if s.len() >= 10 && s.is_char_boundary(10) {
        time::Date::parse(&s[..10], &Iso8601::DATE)
    } else {
        let padded = match s.len() {
            4 => format!("{s}-01-01"),
            7 => format!("{s}-01"),
            _ => s.to_string(),
        };
        time::Date::parse(&padded, &Iso8601::DATE)
    }
}

#[cfg(test)]
mod tests {
    use super::parse_iso_date;
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
        // The body is now a bare RFC 7396 merge patch (Task A0): the
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
        let response = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic)
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
        let response = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic)
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

        let first_title = format!("First Title {marker}");
        let first_response = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
            .json(&serde_json::json!({"title": first_title}))
            .await;
        assert_eq!(first_response.status_code(), StatusCode::OK);
        let first_body: serde_json::Value = first_response.json();
        let first_version_id = first_body["fields"]["title"]["version_id"]
            .as_str()
            .expect("first patch must return a version_id")
            .to_string();

        let second_title = format!("Second Title {marker}");
        let second_response = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic)
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

        let first_title = format!("First Title {marker}");
        let first_response = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
            .json(&serde_json::json!({"title": first_title.clone()}))
            .await;
        assert_eq!(first_response.status_code(), StatusCode::OK);

        let second_title = format!("Second Title {marker}");
        let second_response = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
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
        let response = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic)
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
        let response = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic)
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
        let response = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic)
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
        let response = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic)
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

        let response = server
            .patch(&format!("/api/v1/books/{m_b}/metadata"))
            .add_header(AUTHORIZATION, basic)
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

        // 13 digits, correct length, wrong check digit (valid form ends 7).
        let response = server
            .patch(&format!("/api/v1/books/{m}/metadata"))
            .add_header(AUTHORIZATION, basic)
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

        let response = server
            .patch(&format!("/api/v1/books/{m}/metadata"))
            .add_header(AUTHORIZATION, basic)
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

        // 13 chars, but a letter where a digit must be.
        let response = server
            .patch(&format!("/api/v1/books/{m}/metadata"))
            .add_header(AUTHORIZATION, basic)
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
        let response = server
            .patch(&format!("/api/v1/books/{m}/metadata"))
            .add_header(AUTHORIZATION, basic)
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

        let response = server
            .patch(&format!("/api/v1/books/{m}/metadata"))
            .add_header(AUTHORIZATION, basic)
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

        let response = server
            .patch(&format!("/api/v1/books/{m_b}/metadata"))
            .add_header(AUTHORIZATION, basic)
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
        let response = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
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
        // PATCH description -> null. Creates a `new_value = 'null'`
        // audit row + clears the canonical column.
        let r = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
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
        let r1 = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
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
        let r2 = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic)
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
        let response = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic)
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
        // Same logical publisher, divergent leading/trailing whitespace.
        for publisher in ["Penguin Random House", "  Penguin Random House  "] {
            let response = server
                .patch(&format!("/api/v1/books/{m_id}/metadata"))
                .add_header(AUTHORIZATION, basic.clone())
                .json(&serde_json::json!({"publisher": publisher}))
                .await;
            assert_eq!(
                response.status_code(),
                StatusCode::OK,
                "body = {}",
                response.text()
            );
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
        // Same calendar date — one bare ISO date, one with a time suffix. The
        // canonical normaliser coerces both to the YYYY-MM-DD prefix, so the
        // second save must dedup rather than open a parallel journal row.
        for pub_date in ["2024-01-15", "2024-01-15T00:00:00Z"] {
            let response = server
                .patch(&format!("/api/v1/books/{m_id}/metadata"))
                .add_header(AUTHORIZATION, basic.clone())
                .json(&serde_json::json!({"pub_date": pub_date}))
                .await;
            assert_eq!(response.status_code(), StatusCode::OK);
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
        let response = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic)
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

    // ── revert learns contributors.<role> (Task A1b) ────────────────────────

    #[sqlx::test(migrations = "./migrations")]
    async fn revert_contributors_author_restores_prior_set(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (work_id, m_id) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ing_pool);

        let name_a = format!("Alpha Author {marker}");
        let name_b = format!("Beta Author {marker}");
        let first_response = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
            .json(
                &serde_json::json!({"contributors": {"author": [name_a.clone(), name_b.clone()]}}),
            )
            .await;
        assert_eq!(first_response.status_code(), StatusCode::OK);
        let first_body: serde_json::Value = first_response.json();
        let first_version_id = first_body["fields"]["contributors.author"]["version_id"]
            .as_str()
            .expect("first patch must journal a contributors.author version")
            .to_string();

        let name_c = format!("Gamma Author {marker}");
        let second_response = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
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

        let name = format!("Solo Author {marker}");
        let patch_response = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
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

        let translator = format!("Translator Name {marker}");
        let patch_response = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
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
        let title_a = format!("Title Via A {marker}");
        let patch_a = server
            .patch(&format!("/api/v1/books/{m_a}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
            .json(&serde_json::json!({"title": title_a}))
            .await;
        assert_eq!(patch_a.status_code(), StatusCode::OK);
        let body_a: serde_json::Value = patch_a.json();
        let version_a = body_a["fields"]["title"]["version_id"]
            .as_str()
            .expect("patch via A must journal a title version")
            .to_string();

        // Patch title via manifestation B: the canonical pointer is
        // work-scoped, so B's response must report A's version as the
        // prior pointer even though the row was journaled under A.
        let title_b = format!("Title Via B {marker}");
        let patch_b = server
            .patch(&format!("/api/v1/books/{m_b}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
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
        let patch_a = server
            .patch(&format!("/api/v1/books/{m_a}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
            .json(&serde_json::json!({"pages": 250}))
            .await;
        assert_eq!(patch_a.status_code(), StatusCode::OK);
        let body_a: serde_json::Value = patch_a.json();
        let version_a = body_a["fields"]["pages"]["version_id"]
            .as_str()
            .expect("patch via A must journal a pages version")
            .to_string();

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

        let name_a = format!("Alpha Author {marker}");
        let patch_a = server
            .patch(&format!("/api/v1/books/{m_a}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
            .json(&serde_json::json!({"contributors": {"author": [name_a.clone()]}}))
            .await;
        assert_eq!(patch_a.status_code(), StatusCode::OK);
        let body_a: serde_json::Value = patch_a.json();
        let version_a = body_a["fields"]["contributors.author"]["version_id"]
            .as_str()
            .expect("patch via A must journal a contributors.author version")
            .to_string();

        let name_b = format!("Beta Author {marker}");
        let patch_b = server
            .patch(&format!("/api/v1/books/{m_b}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
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

        let name_a = format!("Alpha Author {marker}");
        let first = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
            .json(&serde_json::json!({"contributors": {"author": [name_a]}}))
            .await;
        assert_eq!(first.status_code(), StatusCode::OK);
        let first_body: serde_json::Value = first.json();
        assert_eq!(
            first_body["fields"]["contributors.author"]["previous_version_id"],
            serde_json::Value::Null,
            "the work's first author edit has no prior single stamp to report"
        );
        let first_version_id = first_body["fields"]["contributors.author"]["version_id"]
            .as_str()
            .expect("first patch must journal a version")
            .to_string();

        let name_b = format!("Beta Author {marker}");
        let second = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic)
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
        let response = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic)
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
        let response = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic)
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
        let response = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
            .json(&serde_json::json!({"contributors": {"author": [&name_a, &name_b]}}))
            .await;
        assert_eq!(response.status_code(), StatusCode::OK);

        // The reorder hashes identically (order-insensitive normalisation),
        // collides with the first row, and must refresh its new_value.
        let response = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic)
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
        let response = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic)
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
        let response = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic)
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
        let response = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic)
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
        let response = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic)
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
        let response = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic)
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
        let response = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic)
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
        let response = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic)
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
        let response = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic)
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
        let response = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic)
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
        let response = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic)
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
        let response = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic)
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
        let response = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
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

        // Clear round-trip.
        let response = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic)
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
    async fn patch_sets_and_clears_pages(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (_work_id, m_id) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;

        let server = test_support::db::server_with_real_pools(&app_pool, &ing_pool);
        let response = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
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

        let response = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic)
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
        let response = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic)
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
        let response = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic)
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
        let response = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic)
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
    async fn accept_contributors_author_version_returns_422(pool: sqlx::PgPool) {
        // Pins that per-role contributor keys stay accept-unsupported, same as
        // the legacy "creators" key was before this change.
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (_work_id, m_id) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;
        let version_id = insert_version(
            &ing_pool,
            m_id,
            "contributors.author",
            serde_json::json!([format!("Pending Author {marker}")]),
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
            StatusCode::UNPROCESSABLE_ENTITY,
            "body = {}",
            response.text()
        );
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
        let response = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic)
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

        let genre_names: Vec<String> = sqlx::query_scalar!(
            "SELECT g.name FROM genres g \
             JOIN manifestation_genres mg ON mg.genre_id = g.id \
             WHERE mg.manifestation_id = $1 ORDER BY g.name",
            m_id,
        )
        .fetch_all(&app_pool)
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
        .fetch_all(&app_pool)
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
        let first = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
            .json(&serde_json::json!({"genres": ["Astrophysics", "Carpentry"]}))
            .await;
        assert_eq!(first.status_code(), StatusCode::OK);

        let second = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic)
            .json(&serde_json::json!({"genres": ["Numismatics"]}))
            .await;
        assert_eq!(second.status_code(), StatusCode::OK);

        let genre_names: Vec<String> = sqlx::query_scalar!(
            "SELECT g.name FROM genres g \
             JOIN manifestation_genres mg ON mg.genre_id = g.id \
             WHERE mg.manifestation_id = $1",
            m_id,
        )
        .fetch_all(&app_pool)
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
        let response = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic)
            .json(&serde_json::json!({"moods": ["Gloomy"], "tags": ["Signed"]}))
            .await;
        assert_eq!(
            response.status_code(),
            StatusCode::OK,
            "body = {}",
            response.text()
        );

        let mood_name: String = sqlx::query_scalar!(
            "SELECT mo.name FROM moods mo \
             JOIN manifestation_moods mm ON mm.mood_id = mo.id \
             WHERE mm.manifestation_id = $1",
            m_id,
        )
        .fetch_one(&app_pool)
        .await
        .expect("fetch mood name");
        assert_eq!(mood_name, "Gloomy");

        let tag_name: String = sqlx::query_scalar!(
            "SELECT t.name FROM tags t \
             JOIN manifestation_tags mt ON mt.tag_id = t.id \
             WHERE mt.manifestation_id = $1",
            m_id,
        )
        .fetch_one(&app_pool)
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
        let set_a = server
            .patch(&format!("/api/v1/books/{m_a}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
            .json(&serde_json::json!({"genres": ["Origami"]}))
            .await;
        assert_eq!(set_a.status_code(), StatusCode::OK);
        let clear_a = server
            .patch(&format!("/api/v1/books/{m_a}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
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
        let set_b = server
            .patch(&format!("/api/v1/books/{m_b}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
            .json(&serde_json::json!({"genres": ["Beekeeping"]}))
            .await;
        assert_eq!(set_b.status_code(), StatusCode::OK);
        let clear_b = server
            .patch(&format!("/api/v1/books/{m_b}/metadata"))
            .add_header(AUTHORIZATION, basic)
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
        let response = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic)
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
        let response = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic)
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

        // Same order twice — must dedupe onto one journal row via observation_count.
        for _ in 0..2 {
            let response = server
                .patch(&format!("/api/v1/books/{m_id}/metadata"))
                .add_header(AUTHORIZATION, basic.clone())
                .json(&serde_json::json!({"genres": ["Falconry", "Glassblowing"]}))
                .await;
            assert_eq!(response.status_code(), StatusCode::OK);
        }

        // A distinct set, submitted, then reordered — the order-insensitive
        // hash must collapse the reorder onto that same second journal row.
        let first_order = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
            .json(&serde_json::json!({"genres": ["Alchemy", "Basketry"]}))
            .await;
        assert_eq!(first_order.status_code(), StatusCode::OK);
        let reordered = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic)
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
        let set_response = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic.clone())
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

        let clear_response = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic)
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

        let patch_response = server
            .patch(&format!("/api/v1/books/{m_id}/metadata"))
            .add_header(AUTHORIZATION, basic)
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

        let genre_names: Vec<String> = sqlx::query_scalar!(
            "SELECT g.name FROM genres g \
             JOIN manifestation_genres mg ON mg.genre_id = g.id \
             WHERE mg.manifestation_id = $1 ORDER BY g.name",
            m_id,
        )
        .fetch_all(&app_pool)
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
        .fetch_all(&app_pool)
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
}
