use axum::extract::State;
use axum::response::IntoResponse;
use axum::routing::post;
use axum::{Json, Router};

use crate::auth::middleware::CurrentUser;
use crate::error::AppError;
use crate::services;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/api/ingestion/scan", post(scan))
}

async fn scan(
    current_user: CurrentUser,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    if current_user.role != "admin" {
        return Err(AppError::Forbidden);
    }

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
        let response = server.post("/api/ingestion/scan").await;
        assert_eq!(response.status_code(), StatusCode::UNAUTHORIZED);
    }
}
