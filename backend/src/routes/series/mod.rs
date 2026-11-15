//! `/api/v1/series/{id}` JSON route.
//!
//! Returns recorded series metadata with RLS-visible manifestations.
//! Child callers receive only works with a visible manifestation;
//! a series with no visible manifestations returns 404.

use axum::extract::State;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::auth::middleware::CurrentUser;
use crate::error::AppError;
use crate::extract::ApiPath;
use crate::models::series::SeriesDetail;
use crate::state::AppState;

#[cfg(test)]
mod tests;

/// Build the `/api/v1/series/{id}` router as an [`OpenApiRouter`] so the
/// handler's `#[utoipa::path]` contributes to the generated spec (a missing
/// annotation fails to compile).
///
/// Merged into `crate::openapi::pilot_router` and
/// split into its runtime and spec halves there.
pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(detail))
}

/// `GET /api/v1/series/{id}` — series + ordered works with visible
/// manifestations per work.
///
/// # Errors
/// - [`AppError::NotFound`] when the series row is missing, the series
///   has no linked works, or every work in the series has zero visible
///   manifestations under the caller's RLS context (existence-not-leaked).
/// - [`AppError::Internal`] on database errors.
#[utoipa::path(
    get,
    path = "/api/v1/series/{id}",
    operation_id = "series_detail",
    summary = "Get a series",
    description = "Returns the series identity and recorded work metadata in series order, with RLS-visible manifestations. Child callers receive only works with a visible manifestation.",
    tag = "series",
    params(("id" = Uuid, Path, max_length = 45, description = "Series id")),
    security(("session_cookie" = ["read"]), ("device_token_bearer" = ["read"]), ("opds_basic" = ["read"])),
    responses(
        (status = 200, description = "Series identity + ordered works with visible manifestations", body = SeriesDetail),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 404, description = "Series not found, has no linked works, or no work has a visible manifestation under the caller's RLS context (existence-not-leaked)", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
    )
)]
async fn detail(
    current_user: CurrentUser,
    State(state): State<AppState>,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<axum::Json<SeriesDetail>, AppError> {
    crate::services::reader::series_detail(&current_user, &state, id)
        .await
        .map(axum::Json)
}
