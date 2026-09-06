//! Liveness (`GET /health`) and readiness (`GET /health/ready`) probes.
//!
//! These are operational endpoints, deliberately unversioned and unauthenticated
//! (`docs/adr/0016-api-versioning-by-url-path-with-openapi-as-the-contract.md` exempts `/health` from `/api/v1`).
//! They are the docs-as-done OpenAPI pilot: the `#[utoipa::path]` annotations
//! below are compile-checked by the `routes!` macro in `router`, so a handler
//! cannot be served without being documented.

use axum::extract::State;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::openapi::ProblemDetails;
use crate::state::AppState;

/// Build the `/health{,/ready}` router as an [`OpenApiRouter`] so each handler's
/// `#[utoipa::path]` contributes to the generated spec and a missing annotation
/// fails compilation.
///
/// The caller splits this into a runtime router and the spec
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
    summary = "Check liveness",
    description = "Returns 200 with a plain-text body as soon as the process is serving requests. Unauthenticated and outside `/api/v1` by design.",
    tag = "health",
    // Explicitly public: opt out of the document-level session_cookie default
    // (operational probe, ADR-exempt and unauthenticated by design).
    security(()),
    responses((status = 200, description = "Process is live", body = String, content_type = "text/plain"))
)]
pub async fn health() -> &'static str {
    "ok"
}

/// Readiness probe: pings the application pool with `SELECT 1` and returns
/// `503` with an RFC 9457 body while the database is unreachable so
/// orchestrators withhold traffic.
///
/// # Errors
///
/// Returns a 503 [`ProblemDetails`] if the `SELECT 1` liveness query against
/// the application pool fails.
#[utoipa::path(
    get,
    path = "/health/ready",
    summary = "Check readiness",
    description = "Pings the application database and returns 200 with a plain-text body when it is reachable, or 503 when it is not, so orchestrators can withhold traffic. Unauthenticated and outside `/api/v1` by design.",
    tag = "health",
    // Explicitly public: opt out of the document-level session_cookie default
    // (operational probe, ADR-exempt and unauthenticated by design).
    security(()),
    responses(
        (status = 200, description = "Ready — database reachable", body = String, content_type = "text/plain"),
        (status = 503, description = "Database unreachable", body = ProblemDetails)
    )
)]
pub async fn ready(State(state): State<AppState>) -> Result<&'static str, ProblemDetails> {
    sqlx::query_scalar!("SELECT 1 AS \"one!: i32\"")
        .fetch_one(&state.pool)
        .await
        .map_err(|e| {
            tracing::warn!(error = ?e, "readiness probe DB check failed");
            // `about:blank` + reason-phrase title per RFC 9457 §4.2.1: the
            // semantics are fully carried by the 503 status, so no
            // Reverie-specific problem type is registered for it.
            ProblemDetails {
                r#type: "about:blank".to_owned(),
                title: "Service Unavailable".to_owned(),
                status: 503,
                detail: "Database unreachable.".to_owned(),
                instance: None,
            }
        })?;
    Ok("ok")
}

#[cfg(test)]
mod tests {
    use crate::test_support::test_server;

    #[tokio::test]
    async fn ready_returns_503_problem_when_database_unreachable() {
        let server = test_server();
        let response = server.get("/health/ready").await;
        response.assert_status(axum::http::StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(
            response
                .headers()
                .get(axum::http::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok()),
            Some("application/problem+json"),
        );
        let json: serde_json::Value = response.json();
        assert_eq!(json["type"], "about:blank");
        assert_eq!(json["title"], "Service Unavailable");
        assert_eq!(json["status"], 503);
        assert_eq!(json["detail"], "Database unreachable.");
        assert_eq!(json["instance"], "/health/ready");
    }
}
