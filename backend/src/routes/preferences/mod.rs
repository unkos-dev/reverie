//! `/auth/me/preferences`, the caller's per-user library display
//! preferences: hidden columns, row density, view choice, and default sort.
//!
//! THREAT: horizontal privilege abuse. The resource is keyed on the caller's
//! own id and never accepts a user id from the request, so there is no
//! identifier to tamper with. Both handlers additionally run inside
//! [`crate::db::acquire_with_rls`], which binds `app.current_user_id` for the
//! transaction; the `user_preferences_owner` policy is what confines the
//! statements to the caller's row. The neighbouring `PATCH /auth/me/theme` is
//! deliberately not the model here: it queries the pool directly, which is
//! only safe because `users` carries no RLS. A bare pool acquisition against
//! an owner-scoped policy leaves `app.current_user_id` unset and matches zero
//! rows.
//!
//! Child accounts are not excluded. These are self-scoped presentation
//! choices about the caller's own screen, not shared-library curation.

use axum::extract::State;
use axum::response::IntoResponse;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::auth::middleware::CurrentUser;
use crate::error::AppError;
use crate::extract::ApiJson;
use crate::services::preferences::{self, PreferencesResponse, UpdatePreferencesRequest};
use crate::state::AppState;

#[cfg(test)]
mod tests;

/// Build the `/auth/me/preferences` router as an [`OpenApiRouter`] so each
/// handler's `#[utoipa::path]` contributes to the generated spec (a missing
/// annotation fails to compile).
///
/// Merged into `crate::openapi::pilot_router`.
pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(get_preferences, patch_preferences))
}

/// `GET /auth/me/preferences`: the caller's display preferences.
///
/// An account with no row reads as all-null overrides. The read is
/// deliberately non-materialising: rows appear on first write, so "has this
/// user customised anything?" stays answerable.
///
/// # Errors
/// - [`AppError::Internal`] on database errors.
#[utoipa::path(
    get,
    path = "/auth/me/preferences",
    summary = "Get the caller's library preferences",
    description = "Returns the caller's overrides for hidden columns, row density, view, and default sort, alongside the installation defaults; a `null` group means the default applies. An account with no stored row reads as all-null overrides.",
    tag = "auth",
    security(("session_cookie" = ["read"]), ("device_token_bearer" = ["read"]), ("opds_basic" = ["read"])),
    responses(
        (status = 200, description = "Caller's overrides plus the installation defaults; a null group means the default applies", body = PreferencesResponse),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
    )
)]
async fn get_preferences(
    current_user: CurrentUser,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    Ok(axum::Json(
        preferences::get_preferences(&state.pool, &current_user).await?,
    ))
}

/// `PATCH /auth/me/preferences`: merge a partial update into the caller's
/// preferences, creating the row on first write.
///
/// Last write wins, with no precondition header. A preferences row is
/// written by exactly one person, so the only possible conflict is between
/// two of that person's own tabs, where the later click is the intended
/// outcome. Two tabs can therefore drift until one of them reloads; nothing
/// is silently corrupted, and no partial merge is ever persisted, because
/// the whole patch is applied in one statement.
///
/// # Errors
/// - [`AppError::Validation`] when the body names no group, carries an
///   unknown density or view, submits a `sort_stack` outside the whitelisted
///   grammar, or submits column keys that are too many or out of shape.
/// - [`AppError::Internal`] on database errors.
#[utoipa::path(
    patch,
    path = "/auth/me/preferences",
    summary = "Update the caller's library preferences",
    description = "Applies a JSON Merge Patch to the caller's hidden columns, row density, view, and default sort: an absent group is left unchanged and an explicit `null` resets it to the installation default. At least one group is required.",
    tag = "auth",
    security(("session_cookie" = ["write"]), ("device_token_bearer" = ["write"]), ("opds_basic" = ["write"])),
    request_body(content = UpdatePreferencesRequest, description = "RFC 7396 JSON Merge Patch: absent groups are unchanged, `null` resets a group to the installation default"),
    responses(
        (status = 200, description = "Preferences after the merge, in the same shape as the read", body = PreferencesResponse),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 422, description = "Unknown density or view, or an out-of-range sort", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
    )
)]
async fn patch_preferences(
    current_user: CurrentUser,
    State(state): State<AppState>,
    ApiJson(req): ApiJson<UpdatePreferencesRequest>,
) -> Result<impl IntoResponse, AppError> {
    Ok(axum::Json(
        preferences::update_preferences(&state.pool, &current_user, req).await?,
    ))
}
