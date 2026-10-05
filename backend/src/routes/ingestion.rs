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
/// The admin gate controls discovery commands submitted to the ingestion owner.
pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(scan))
}

type ScanResponse = services::ingestion::DiscoveryResult;

/// `POST /api/v1/ingestion/scan` — request discovery of the ingestion
/// directory (admin only).
///
/// # Errors
/// - [`AppError::Forbidden`] when the caller is not an admin.
/// - [`AppError::Internal`] when the scan fails at the service layer.
#[utoipa::path(
    post,
    path = "/api/v1/ingestion/scan",
    summary = "Scan the ingestion directory",
    description = "Discovers current ingestion inputs and reports queued, deferred and suppressed counts with the activity monitor. Admin only.",
    tag = "ingestion",
    security(("session_cookie" = ["admin"]), ("device_token_bearer" = ["admin"]), ("oidc_jwt_bearer" = ["admin"]), ("opds_basic" = ["admin"])),
    responses(
        (status = 202, description = "Discovery complete; processing follows readiness. Admin only.", body = ScanResponse),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 403, description = "Caller is not an admin", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
    )
)]
async fn scan(
    current_user: CurrentUser,
    State(state): State<AppState>,
) -> Result<(axum::http::StatusCode, Json<ScanResponse>), AppError> {
    current_user.require_scope(Scope::Admin)?;
    current_user.require_admin()?;

    let result = state.ingestion.scan().await.map_err(AppError::Internal)?;
    Ok((axum::http::StatusCode::ACCEPTED, Json(result)))
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

    #[sqlx::test(migrations = "./migrations")]
    async fn capability_ingestion_coordinator_admin_scan_returns_discovery_counts(
        pool: sqlx::PgPool,
    ) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let (_id, auth) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let mut state = test_support::test_state();
        state.pool = app_pool;
        let (handle, mut commands) = crate::services::ingestion::coordinator_channel();
        state.ingestion = handle;
        let owner = tokio::spawn(async move {
            let reply = commands.recv().await.unwrap();
            reply
                .send(Ok(crate::services::ingestion::DiscoveryResult {
                    queued: 1,
                    deferred: 2,
                    suppressed: 3,
                    monitor: "/api/v1/dashboard/activity",
                }))
                .unwrap();
        });
        let server = axum_test::TestServer::new(crate::build_router_with_session_store(
            state,
            tower_sessions::MemoryStore::default(),
        ));
        let response = server
            .post("/api/v1/ingestion/scan")
            .add_header(axum::http::header::AUTHORIZATION, auth)
            .await;
        assert_eq!(response.status_code(), StatusCode::ACCEPTED);
        let body: serde_json::Value = response.json();
        assert_eq!(body["queued"], 1);
        assert_eq!(body["deferred"], 2);
        assert_eq!(body["suppressed"], 3);
        assert_eq!(body["monitor"], "/api/v1/dashboard/activity");
        owner.await.unwrap();
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn capability_ingestion_coordinator_non_admin_cannot_submit_scan(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let (_id, auth) =
            test_support::db::create_adult_and_basic_auth(&app_pool, "scan-adult").await;
        let mut state = test_support::test_state();
        state.pool = app_pool;
        let (handle, mut commands) = crate::services::ingestion::coordinator_channel();
        state.ingestion = handle;
        let server = axum_test::TestServer::new(crate::build_router_with_session_store(
            state,
            tower_sessions::MemoryStore::default(),
        ));
        let response = server
            .post("/api/v1/ingestion/scan")
            .add_header(axum::http::header::AUTHORIZATION, auth)
            .await;
        assert_eq!(response.status_code(), StatusCode::FORBIDDEN);
        assert!(commands.try_recv().is_err());
    }
}
