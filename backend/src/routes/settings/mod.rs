//! `/api/v1/settings` admin-only settings management routes.
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
use time::OffsetDateTime;

use crate::auth::middleware::CurrentUser;
use crate::error::AppError;
use crate::models::settings::{
    Settings, UpdateSettings, has_restart_required_field, restart_required_fields, validate_update,
};
use crate::state::AppState;

#[cfg(test)]
mod tests;

/// Build the `/api/v1/settings` router.
pub fn router() -> Router<AppState> {
    Router::new().route("/api/v1/settings", get(get_settings).put(put_settings))
}

/// Response shape for `GET /api/v1/settings`.
#[derive(serde::Serialize)]
struct SettingsResponse {
    #[serde(flatten)]
    settings: Settings,
    restart_required_fields: &'static [&'static str],
    #[serde(with = "time::serde::rfc3339::option")]
    last_successful_reload_at: Option<OffsetDateTime>,
}

/// `GET /api/v1/settings` — return current persisted settings (admin only).
///
/// Reads directly from the database so the response always reflects
/// the latest persisted state (admin endpoint, single-row read,
/// called infrequently). Workers use the `RwLock` cache for zero-DB
/// per-request reads.
///
/// Also includes `last_successful_reload_at` from the background
/// LISTEN/NOTIFY task so operators can verify the live-reload
/// mechanism is healthy.
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
    let last_successful_reload_at = *state.last_settings_reload.read().await;
    Ok(axum::Json(SettingsResponse {
        settings,
        restart_required_fields: restart_required_fields(),
        last_successful_reload_at,
    }))
}

/// Response shape for `PUT /api/v1/settings`.
#[derive(serde::Serialize)]
struct PutSettingsResponse {
    #[serde(flatten)]
    settings: Settings,
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
            "at least one field must be specified".into(),
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
