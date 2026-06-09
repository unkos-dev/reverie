//! Liveness (`GET /health`) and readiness (`GET /health/ready`) probes.
//!
//! These are operational endpoints, deliberately unversioned and unauthenticated
//! (`adr/2026-06-08-api-versioning-openapi.md` exempts `/health` from `/api/v1`).
//! They are the docs-as-done OpenAPI pilot: the `#[utoipa::path]` annotations
//! below are compile-checked by the `routes!` macro in `router`, so a handler
//! cannot be served without being documented.

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::openapi::ProblemDetails;
use crate::state::AppState;

/// Build the `/health{,/ready}` router as an [`OpenApiRouter`] so each handler's
/// `#[utoipa::path]` contributes to the generated spec and a missing annotation
/// fails compilation. The caller splits this into a runtime router and the spec
/// (see [`crate::openapi`]).
pub fn router() -> OpenApiRouter<AppState> {
    // Distinct paths get distinct `.routes()` calls; `routes!(a, b)` is for
    // multiple methods on one path.
    OpenApiRouter::new()
        .routes(routes!(health))
        .routes(routes!(ready))
}

/// Liveness probe: returns `200 ok` as soon as the process is serving.
#[utoipa::path(
    get,
    path = "/health",
    tag = "health",
    responses((status = 200, description = "Process is live", body = String, content_type = "text/plain"))
)]
pub async fn health() -> &'static str {
    "ok"
}

/// Readiness probe: pings the application pool with `SELECT 1` and returns
/// `503` while the database is unreachable so orchestrators withhold traffic.
#[utoipa::path(
    get,
    path = "/health/ready",
    tag = "health",
    responses(
        (status = 200, description = "Ready — database reachable", body = String, content_type = "text/plain"),
        (status = 503, description = "Database unreachable", body = ProblemDetails, content_type = "application/problem+json")
    )
)]
pub async fn ready(State(state): State<AppState>) -> Result<impl IntoResponse, StatusCode> {
    sqlx::query_scalar!("SELECT 1 AS \"one!: i32\"")
        .fetch_one(&state.pool)
        .await
        .map_err(|e| {
            tracing::warn!(error = ?e, "readiness probe DB check failed");
            StatusCode::SERVICE_UNAVAILABLE
        })?;
    Ok("ok")
}
