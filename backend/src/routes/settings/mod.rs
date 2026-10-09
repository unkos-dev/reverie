//! `/api/v1/settings` admin-only settings management routes.
//!
//! THREAT: Privilege escalation — all endpoints are admin-gated via
//! `require_admin()`. Non-admin callers receive 403.
//!
//! Settings are persisted to the `settings` table (single-row). Changes
//! propagate to the running process via LISTEN/NOTIFY + RwLock.

use axum::extract::State;
use axum::http::header::ETAG;
use axum::http::{HeaderMap, HeaderValue};
use axum::response::{IntoResponse, Response};
use chrono::{DateTime, Utc};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::auth::middleware::CurrentUser;
use crate::auth::scope::Scope;
use crate::error::AppError;
use crate::extract::ApiJson;
use crate::models::settings::{
    Settings, UpdateSettings, has_restart_required_field, restart_required_fields, validate_update,
};
use crate::routes::etag::{hash_etag, if_match_mismatch, parse_if_match};
use crate::state::AppState;

#[cfg(test)]
mod tests;

/// Build the `/api/v1/settings` router as an [`OpenApiRouter`] so each
/// handler's `#[utoipa::path]` contributes to the generated spec (a missing
/// annotation fails to compile).
///
/// Merged into `crate::openapi::pilot_router`
/// and split into its runtime and spec halves there.
pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(get_settings, put_settings))
        .routes(routes!(reload_status))
}

/// Hash input for the settings `ETag`. `revision` increases on every write to
/// the settings row, so the tag changes exactly when the settings do.
#[derive(serde::Serialize)]
struct SettingsEtagFields {
    revision: i64,
}

fn settings_etag(revision: i64) -> Result<HeaderValue, AppError> {
    hash_etag(&SettingsEtagFields { revision })
}

/// Response shape for `GET /api/v1/settings`.
#[derive(serde::Serialize, utoipa::ToSchema)]
struct SettingsResponse {
    #[serde(flatten)]
    settings: Settings,
    /// Settings fields whose changes only take effect after a process
    /// restart.
    #[schema(value_type = Vec<String>)]
    restart_required_fields: &'static [&'static str],
}

/// `GET /api/v1/settings` — return current persisted settings (admin only).
///
/// Reads directly from the database so the response always reflects
/// the latest persisted state (admin endpoint, single-row read,
/// called infrequently). Workers use the `RwLock` cache for zero-DB
/// per-request reads.
///
/// The body is a function of the settings row alone, so the `ETag`
/// header (a hash of the row's revision) identifies it exactly and is the
/// tag a subsequent `PUT` echoes as `If-Match`. Process state such as the
/// live-reload health is served by [`reload_status`] instead.
///
/// # Errors
/// - [`AppError::Forbidden`] when the caller is not an admin.
/// - [`AppError::Internal`] on database errors, or if the `ETag` cannot be
///   built.
#[utoipa::path(
    get,
    path = "/api/v1/settings",
    summary = "Get settings",
    description = "Returns the currently persisted application settings, including the fields that only take effect after a process restart. Admin only.",
    tag = "settings",
    security(("session_cookie" = ["admin"]), ("device_token_bearer" = ["admin"]), ("oidc_jwt_bearer" = ["admin"]), ("opds_basic" = ["admin"])),
    responses(
        (status = 200, description = "Current persisted settings. Admin only.", body = SettingsResponse,
         headers(("ETag" = String, description = "Strong entity-tag of the current settings, to echo as `If-Match` on `PUT`"))),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 403, description = "Caller is not an admin", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
    )
)]
async fn get_settings(
    current_user: CurrentUser,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    current_user.require_scope(Scope::Admin)?;
    current_user.require_admin()?;

    let settings = crate::services::settings::load(&state.pool)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    let etag = settings_etag(settings.revision)?;
    Ok((
        [(ETAG, etag)],
        axum::Json(SettingsResponse {
            settings,
            restart_required_fields: restart_required_fields(),
        }),
    ))
}

/// Response shape for `GET /api/v1/settings/reload-status`.
#[derive(serde::Serialize, utoipa::ToSchema)]
struct ReloadStatusResponse {
    /// Timestamp of the last successful LISTEN/NOTIFY settings reload in
    /// this process; `null` until the first reload.
    last_successful_reload_at: Option<DateTime<Utc>>,
}

/// `GET /api/v1/settings/reload-status` — health of the live-reload
/// mechanism in this process (admin only).
///
/// Reports when the background LISTEN/NOTIFY task last reloaded settings
/// successfully so operators can verify live reload is working. It is
/// process state, not part of the settings representation, and carries no
/// `ETag`.
///
/// # Errors
/// - [`AppError::Forbidden`] when the caller is not an admin.
#[utoipa::path(
    get,
    path = "/api/v1/settings/reload-status",
    summary = "Get settings reload status",
    description = "Returns the timestamp of the last successful live reload of settings in this process, or null before the first reload. Admin only.",
    tag = "settings",
    security(("session_cookie" = ["admin"]), ("device_token_bearer" = ["admin"]), ("oidc_jwt_bearer" = ["admin"]), ("opds_basic" = ["admin"])),
    responses(
        (status = 200, description = "Last successful settings reload in this process. Admin only.", body = ReloadStatusResponse),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 403, description = "Caller is not an admin", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
    )
)]
async fn reload_status(
    current_user: CurrentUser,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    current_user.require_scope(Scope::Admin)?;
    current_user.require_admin()?;

    let last_successful_reload_at = *state.last_settings_reload.read().await;
    Ok(axum::Json(ReloadStatusResponse {
        last_successful_reload_at,
    }))
}

/// Response shape for `PUT /api/v1/settings`.
#[derive(serde::Serialize, utoipa::ToSchema)]
struct PutSettingsResponse {
    #[serde(flatten)]
    settings: Settings,
    /// `true` when the patch touched at least one field that only takes
    /// effect after a process restart.
    restart_required: bool,
}

/// `PUT /api/v1/settings` — partial update of settings (admin only).
///
/// Accepts RFC 7396 JSON Merge Patch: absent fields are unchanged.
/// Validates field values before persisting. Updates the local
/// `RwLock` cache immediately (no NOTIFY round-trip for same-process
/// reads). The DB trigger also fires `NOTIFY settings_changed` to
/// propagate changes to other connected processes.
///
/// Requires `If-Match` carrying the `ETag` of a prior `GET` or `PUT`. The
/// tag is compared under the settings row lock that the update then
/// commits under, so of two writers holding the same tag exactly one wins
/// and the other receives 412. Body validation runs only once the
/// precondition has held.
///
/// # Errors
/// - [`AppError::Forbidden`] when the caller is not an admin.
/// - [`AppError::MalformedHeader`] when `If-Match` is not exactly one
///   well-formed strong entity-tag.
/// - [`AppError::IfMatchRequired`] (428) when `If-Match` is absent.
/// - [`AppError::IfMatchMismatch`] (412) when `If-Match` does not match the
///   current settings `ETag`; the response carries the current `ETag`.
/// - [`AppError::Validation`] when the body is empty or contains
///   invalid field values.
/// - [`AppError::Internal`] on database errors.
#[utoipa::path(
    put,
    path = "/api/v1/settings",
    summary = "Update settings",
    description = "Applies a JSON Merge Patch to the application settings: fields absent from the body are left unchanged, and at least one field is required. Requires an `If-Match` header carrying the `ETag` of a prior `GET` or `PUT`; fails with 428 when it is absent, 412 when it does not match, and 400 when it is malformed or refused by policy. Returns the updated settings and whether the change needs a process restart to take effect. Admin only.",
    tag = "settings",
    params(
        ("If-Match" = String, Header, description = "Exactly one quoted strong entity-tag, as returned in a prior GET or PUT response's ETag header. The * wildcard, entity-tag lists, weak tags, and repeated instances of this field are refused with 400. Required: absent means 428; unequal means 412")
    ),
    request_body(content = UpdateSettings, description = "RFC 7396 JSON Merge Patch: absent fields are unchanged; at least one field is required"),
    security(("session_cookie" = ["admin"]), ("device_token_bearer" = ["admin"]), ("oidc_jwt_bearer" = ["admin"]), ("opds_basic" = ["admin"])),
    responses(
        (status = 200, description = "Updated settings. `restart_required` is true when a changed field only takes effect after restart. Admin only.", body = PutSettingsResponse,
         headers(("ETag" = String, description = "Strong entity-tag of the settings after this write"))),
        (status = 400, description = "If-Match is malformed, or carries a form this API refuses by policy: the * wildcard, an entity-tag list, a weak tag, or a repeated header instance", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 403, description = "Caller is not an admin", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 412, description = "If-Match does not match the current settings ETag", body = crate::openapi::ProblemDetails, content_type = "application/problem+json",
         headers(("ETag" = String, description = "Current entity-tag, so the caller can resync without a follow-up GET"))),
        (status = 422, description = "Empty patch or invalid field values. Evaluated only after If-Match has matched, so a stale tag returns 412 instead", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 428, description = "If-Match header absent", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
    )
)]
async fn put_settings(
    current_user: CurrentUser,
    State(state): State<AppState>,
    headers_in: HeaderMap,
    ApiJson(req): ApiJson<UpdateSettings>,
) -> Result<Response, AppError> {
    current_user.require_scope(Scope::Admin)?;
    current_user.require_admin()?;
    let if_match = parse_if_match(&headers_in)?.ok_or(AppError::IfMatchRequired)?;

    let mut tx = state
        .pool
        .begin()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    let current_revision = crate::services::settings::lock_revision(&mut *tx)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    let current_etag = settings_etag(current_revision)?;
    if !if_match.matches(&current_etag) {
        return Ok(if_match_mismatch(&current_etag));
    }

    if req.is_empty() {
        return Err(AppError::Validation(
            "at least one field must be specified".into(),
        ));
    }

    validate_update(&req).map_err(AppError::Validation)?;
    crate::services::settings::validate_provider_keys(&mut *tx, &req)
        .await
        .map_err(|e| match e {
            crate::services::settings::ProviderKeyError::UnknownKey(_) => {
                AppError::Validation(e.to_string())
            }
            crate::services::settings::ProviderKeyError::Db(db) => AppError::Internal(db.into()),
        })?;

    let restart_required = has_restart_required_field(&req);

    let updated = crate::services::settings::save(&mut *tx, &req)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    tx.commit()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    // Update local cache immediately so same-process reads reflect the new
    // values without waiting for the NOTIFY round-trip. Guarded by the
    // revision counter: if a concurrent writer committed (and swapped) a
    // newer row between our save and this lock acquisition, keep theirs.
    {
        let mut guard = state.settings.write().await;
        crate::services::settings::apply_if_newer(&mut guard, updated.clone());
    }

    let new_etag = settings_etag(updated.revision)?;
    Ok((
        [(ETAG, new_etag)],
        axum::Json(PutSettingsResponse {
            settings: updated,
            restart_required,
        }),
    )
        .into_response())
}
