//! `/api/settings` admin-only settings management routes.
//!
//! THREAT: Privilege escalation — all endpoints are admin-gated via
//! `require_admin()`. Non-admin callers receive 403.
//!
//! Settings are persisted to the `settings` table (single-row). Changes
//! propagate to the running process via LISTEN/NOTIFY + RwLock.

use axum::Router;
use axum::extract::State;
use axum::extract::rejection::JsonRejection;
use axum::response::IntoResponse;
use axum::routing::get;

use crate::auth::middleware::CurrentUser;
use crate::error::AppError;
use crate::models::settings::{
    Settings, UpdateSettings, has_restart_required_field, restart_required_fields, validate_update,
};
use crate::state::AppState;

#[cfg(test)]
mod tests;

/// Build the `/api/settings` router.
pub fn router() -> Router<AppState> {
    Router::new().route("/api/settings", get(get_settings).put(put_settings))
}

/// Response shape for `GET /api/settings`.
#[derive(serde::Serialize)]
struct SettingsResponse {
    #[serde(flatten)]
    settings: Settings,
    restart_required_fields: &'static [&'static str],
}

/// `GET /api/settings` — return current effective settings (admin only).
///
/// Reads directly from the database so the response always reflects
/// the latest persisted state (admin endpoint, single-row read,
/// called infrequently). Workers use the `RwLock` cache for zero-DB
/// per-request reads.
///
/// # Errors
/// - [`AppError::Forbidden`] when the caller is not an admin.
/// - [`AppError::Internal`] on database errors.
async fn get_settings(
    current_user: CurrentUser,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    current_user.require_admin()?;

    let settings = crate::services::settings::load(&state.pool)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    Ok(axum::Json(SettingsResponse {
        settings,
        restart_required_fields: restart_required_fields(),
    }))
}

/// Response shape for `PUT /api/settings`.
#[derive(serde::Serialize)]
struct PutSettingsResponse {
    #[serde(flatten)]
    settings: Settings,
    restart_required: bool,
}

/// `PUT /api/settings` — partial update of settings (admin only).
///
/// Accepts RFC 7396 JSON Merge Patch: absent fields are unchanged.
/// Validates field values before persisting. The DB trigger fires
/// `NOTIFY settings_changed` which refreshes the in-memory cache.
///
/// # Errors
/// - [`AppError::Forbidden`] when the caller is not an admin.
/// - [`AppError::Validation`] when the body is empty or contains
///   invalid field values.
/// - [`AppError::Internal`] on database errors.
async fn put_settings(
    current_user: CurrentUser,
    State(state): State<AppState>,
    body: Result<axum::Json<UpdateSettings>, JsonRejection>,
) -> Result<impl IntoResponse, AppError> {
    current_user.require_admin()?;
    let axum::Json(req) = body.map_err(|e| AppError::Validation(e.body_text()))?;

    if req.is_empty() {
        return Err(AppError::Validation(
            "request body must not be empty".into(),
        ));
    }

    validate_update(&req).map_err(AppError::Validation)?;

    let restart_required = has_restart_required_field(&req);

    let updated = crate::services::settings::save(&state.pool, &req)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    // Update local cache immediately so same-process reads reflect
    // the new values without waiting for the NOTIFY round-trip.
    {
        let mut guard = state.settings.write().await;
        *guard = updated.clone();
    }

    Ok(axum::Json(PutSettingsResponse {
        settings: updated,
        restart_required,
    }))
}
