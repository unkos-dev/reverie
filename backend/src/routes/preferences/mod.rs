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
use axum::extract::rejection::JsonRejection;
use axum::response::IntoResponse;
use serde::Deserialize;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::auth::middleware::CurrentUser;
use crate::auth::scope::Scope;
use crate::db;
use crate::error::AppError;
use crate::models::user_preferences::{
    LibraryDensity, LibraryView, PreferenceDefaults, PreferenceOverrides, validate_hidden_columns,
    validate_sort_stack,
};
use crate::state::AppState;

#[cfg(test)]
mod tests;

/// Build the `/auth/me/preferences` router as an [`OpenApiRouter`] so each
/// handler's `#[utoipa::path]` contributes to the generated spec (a missing
/// annotation fails to compile). Merged into `crate::openapi::pilot_router`.
pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(get_preferences, patch_preferences))
}

/// Both endpoints answer with the caller's overrides and the installation
/// defaults together, so the client never has to hold a second copy of the
/// defaults and cannot drift from the server's.
#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
struct PreferencesResponse {
    /// The caller's overrides; a `null` group inherits from `defaults`.
    #[serde(flatten)]
    overrides: PreferenceOverrides,
    defaults: PreferenceDefaults,
}

impl PreferencesResponse {
    const fn new(overrides: PreferenceOverrides) -> Self {
        Self {
            overrides,
            defaults: PreferenceDefaults::installation(),
        }
    }
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
    tag = "auth",
    security(("session_cookie" = ["read"]), ("device_token_bearer" = ["read"]), ("oidc_jwt_bearer" = ["read"]), ("opds_basic" = ["read"])),
    responses(
        (status = 200, description = "Caller's overrides plus the installation defaults; a null group means the default applies", body = PreferencesResponse),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails)
    )
)]
async fn get_preferences(
    current_user: CurrentUser,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    current_user.require_scope(Scope::Read)?;

    let mut tx = db::acquire_with_rls(&state.pool, current_user.user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let row = sqlx::query_as!(
        PreferenceOverrides,
        r#"SELECT hidden_columns,
                  density AS "density?: LibraryDensity",
                  view AS "view?: LibraryView",
                  sort_stack
             FROM user_preferences
            WHERE user_id = $1"#,
        current_user.user_id,
    )
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    tx.commit()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    Ok(axum::Json(PreferencesResponse::new(
        row.unwrap_or_default(),
    )))
}

/// Body for `PATCH /auth/me/preferences`. RFC 7396 JSON Merge Patch: an
/// absent key leaves that group unchanged, an explicit `null` resets it to
/// the installation default.
#[expect(
    clippy::option_option,
    reason = "RFC 7396 sparse-update encoding: outer Option distinguishes absent (None) from present-and-null (Some(None)), which is the whole reset mechanism"
)]
#[derive(Debug, Default, Deserialize, utoipa::ToSchema)]
struct UpdatePreferencesRequest {
    /// Column keys to hide. Absent = unchanged; `null` resets to the default.
    #[serde(default, with = "::serde_with::rust::double_option")]
    #[schema(value_type = Option<Vec<String>>)]
    hidden_columns: Option<Option<Vec<String>>>,
    /// Row density. Absent = unchanged; `null` resets to the default.
    #[serde(default, with = "::serde_with::rust::double_option")]
    #[schema(value_type = Option<LibraryDensity>)]
    density: Option<Option<LibraryDensity>>,
    /// Library view. Absent = unchanged; `null` resets to the default.
    #[serde(default, with = "::serde_with::rust::double_option")]
    #[schema(value_type = Option<LibraryView>)]
    view: Option<Option<LibraryView>>,
    /// Default sort in the `?sort=` wire grammar. Absent = unchanged; `null`
    /// resets to the default.
    #[serde(default, with = "::serde_with::rust::double_option")]
    #[schema(value_type = Option<String>)]
    sort_stack: Option<Option<String>>,
}

impl UpdatePreferencesRequest {
    /// Reject a body that names no group. A patch that changes nothing is a
    /// client bug, and answering 200 to it would hide the bug behind a
    /// successful-looking round trip.
    const fn names_a_group(&self) -> bool {
        self.hidden_columns.is_some()
            || self.density.is_some()
            || self.view.is_some()
            || self.sort_stack.is_some()
    }

    /// Check every submitted value before any of them is written, so a
    /// rejected patch persists nothing at all.
    fn validate(&self) -> Result<(), AppError> {
        if let Some(Some(keys)) = &self.hidden_columns {
            validate_hidden_columns(keys).map_err(|e| AppError::Validation(e.to_string()))?;
        }
        if let Some(Some(raw)) = &self.sort_stack {
            validate_sort_stack(raw).map_err(|e| AppError::Validation(e.to_string()))?;
        }
        Ok(())
    }
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
    tag = "auth",
    security(("session_cookie" = ["write"]), ("device_token_bearer" = ["write"]), ("oidc_jwt_bearer" = ["write"]), ("opds_basic" = ["write"])),
    request_body(content = UpdatePreferencesRequest, description = "RFC 7396 JSON Merge Patch: absent groups are unchanged, `null` resets a group to the installation default"),
    responses(
        (status = 200, description = "Preferences after the merge, in the same shape as the read", body = PreferencesResponse),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails),
        (status = 422, description = "Empty body, unknown density or view, or an out-of-range sort stack or column key", body = crate::openapi::ProblemDetails)
    )
)]
async fn patch_preferences(
    current_user: CurrentUser,
    State(state): State<AppState>,
    body: Result<axum::Json<UpdatePreferencesRequest>, JsonRejection>,
) -> Result<impl IntoResponse, AppError> {
    current_user.require_scope(Scope::Write)?;
    let axum::Json(req) = body.map_err(|e| AppError::Validation(e.body_text()))?;

    if !req.names_a_group() {
        return Err(AppError::Validation("no fields".into()));
    }
    req.validate()?;

    // Bound before the query so the borrows outlive the macro's arguments.
    let hidden_columns = req.hidden_columns.clone().flatten();
    let sort_stack = req.sort_stack.clone().flatten();

    let mut tx = db::acquire_with_rls(&state.pool, current_user.user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    // One statement rather than read-modify-write. Each group travels as a
    // value plus a "was it named?" flag, so the UPDATE arm can tell an
    // absent group (keep what is stored) from an explicit null (reset),
    // which a merged struct written back wholesale could not. It also means
    // two concurrent patches serialise on the row instead of racing between
    // a read and a write, which is what keeps last-write-wins from ever
    // persisting half of one patch and half of the other.
    let updated = sqlx::query_as!(
        PreferenceOverrides,
        r#"INSERT INTO user_preferences (user_id, hidden_columns, density, view, sort_stack)
                VALUES ($1, $2, $3, $4, $5)
           ON CONFLICT (user_id) DO UPDATE SET
                hidden_columns = CASE WHEN $6 THEN EXCLUDED.hidden_columns
                                      ELSE user_preferences.hidden_columns END,
                density        = CASE WHEN $7 THEN EXCLUDED.density
                                      ELSE user_preferences.density END,
                view           = CASE WHEN $8 THEN EXCLUDED.view
                                      ELSE user_preferences.view END,
                sort_stack     = CASE WHEN $9 THEN EXCLUDED.sort_stack
                                      ELSE user_preferences.sort_stack END,
                updated_at     = now()
             RETURNING hidden_columns,
                       density AS "density?: LibraryDensity",
                       view AS "view?: LibraryView",
                       sort_stack"#,
        current_user.user_id,
        hidden_columns.as_deref(),
        req.density.flatten() as Option<LibraryDensity>,
        req.view.flatten() as Option<LibraryView>,
        sort_stack.as_deref(),
        req.hidden_columns.is_some(),
        req.density.is_some(),
        req.view.is_some(),
        req.sort_stack.is_some(),
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    tx.commit()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    Ok(axum::Json(PreferencesResponse::new(updated)))
}
