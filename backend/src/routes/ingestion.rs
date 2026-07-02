//! Library-scan trigger (`POST /api/v1/ingestion/scan`); admin-only.

use axum::Json;
use axum::extract::State;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::auth::middleware::CurrentUser;
use crate::auth::scope::Scope;
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
pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(scan))
}

/// Per-outcome file counts for one synchronous scan pass.
#[derive(serde::Serialize, utoipa::ToSchema)]
struct ScanResponse {
    /// Files ingested successfully.
    processed: usize,
    /// Files that errored during ingestion.
    failed: usize,
    /// Files skipped (already ingested or unsupported).
    skipped: usize,
}

/// `POST /api/v1/ingestion/scan` — synchronously scan the ingestion
/// directory (admin only).
///
/// # Errors
/// - [`AppError::Forbidden`] when the caller is not an admin.
/// - [`AppError::Internal`] when the scan fails at the service layer.
#[utoipa::path(
    post,
    path = "/api/v1/ingestion/scan",
    tag = "ingestion",
    security(("session_cookie" = ["admin"]), ("device_token_bearer" = ["admin"]), ("opds_basic" = ["admin"])),
    responses(
        (status = 200, description = "Scan complete; per-outcome file counts. Admin only.", body = ScanResponse),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails),
        (status = 403, description = "Caller is not an admin", body = crate::openapi::ProblemDetails)
    )
)]
async fn scan(
    current_user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<ScanResponse>, AppError> {
    current_user.require_scope(Scope::Admin)?;
    current_user.require_admin()?;

    let result = services::ingestion::scan_once(&state.config, &state.ingestion_pool)
        .await
        .map_err(AppError::Internal)?;

    Ok(Json(ScanResponse {
        processed: result.processed,
        failed: result.failed,
        skipped: result.skipped,
    }))
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
