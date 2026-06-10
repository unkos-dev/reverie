//! Library-scan trigger (`POST /api/v1/ingestion/scan`); admin-only.

use axum::extract::State;
use axum::response::IntoResponse;
use axum::routing::post;
use axum::{Json, Router};

use crate::auth::middleware::CurrentUser;
use crate::error::AppError;
use crate::services;
use crate::state::AppState;

/// Build the ingestion-control router for `POST /api/v1/ingestion/scan`.
///
/// # Invariants
/// - Admin-only: the `scan` handler enforces `CurrentUser::require_admin`
///   before doing any work.
///
/// Why: `services::ingestion::scan_once` mutates library state and is
/// expensive enough to warrant being kept off regular user flows; the
/// admin gate is the single trust boundary for triggering it via HTTP.
pub fn router() -> Router<AppState> {
    Router::new().route("/api/v1/ingestion/scan", post(scan))
}

async fn scan(
    current_user: CurrentUser,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    current_user.require_admin()?;

    let result = services::ingestion::scan_once(&state.config, &state.ingestion_pool)
        .await
        .map_err(AppError::Internal)?;

    Ok(Json(serde_json::json!({
        "processed": result.processed,
        "failed": result.failed,
        "skipped": result.skipped,
    })))
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;

    use crate::test_support;

    #[tokio::test]
    async fn scan_returns_401_without_auth() {
        let server = test_support::test_server();
        let response = server.post("/api/v1/ingestion/scan").await;
        assert_eq!(response.status_code(), StatusCode::UNAUTHORIZED);
    }
}
