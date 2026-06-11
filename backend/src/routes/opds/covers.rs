//! Cover image handlers, dual-mounted. `/opds/books/:id/cover{,/thumb}` sits
//! under `BasicOnly` so OPDS clients' Basic credentials stay within the
//! RFC 7617 paired protection space. `/api/v1/books/:id/cover{,/thumb}` sits
//! under `CurrentUser` (cookie-or-Basic) for the web UI. Handler body is
//! shared; the two mounts differ only in extractor wrapping.

use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{StatusCode, header};
use axum::response::Response;
use tokio::fs::File;
use tokio_util::io::ReaderStream;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::auth::basic_only::BasicOnly;
use crate::auth::middleware::CurrentUser;
use crate::error::AppError;
use crate::services::covers::{CoverError, CoverSize, get_or_create};
use crate::state::AppState;

/// Build the OPDS-mount cover router (`/opds/books/:id/cover{,/thumb}`)
/// gated by [`BasicOnly`] so OPDS clients' Basic credentials remain
/// inside the RFC 7617 paired protection space.
pub fn opds_router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(opds_cover))
        .routes(routes!(opds_cover_thumb))
}

/// Build the API-mount cover router (`/api/v1/books/:id/cover{,/thumb}`)
/// gated by [`CurrentUser`] so the web UI can load covers with a
/// session cookie. Always mounted independent of `config.opds.enabled`.
pub fn api_router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(api_cover))
        .routes(routes!(api_cover_thumb))
}

/// `GET /opds/books/{id}/cover` — full-size cover, Basic auth.
///
/// # Errors
/// - [`AppError::NotFound`] when the manifestation is missing, RLS-hidden,
///   or has no cover.
/// - [`AppError::Internal`] on cover-generation or file IO errors.
#[utoipa::path(
    get,
    path = "/opds/books/{id}/cover",
    tag = "opds",
    security(("opds_basic" = [])),
    params(("id" = Uuid, Path, description = "Manifestation id")),
    responses(
        (status = 200, description = "Cover image stream (`image/jpeg` / `image/png` / `image/webp`); Cache-Control: no-store"),
        (status = 401, description = "Basic authentication required (WWW-Authenticate: Basic)"),
        (status = 404, description = "Manifestation missing, RLS-hidden, or coverless", body = crate::openapi::ProblemDetails)
    )
)]
async fn opds_cover(
    BasicOnly(user): BasicOnly,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Response, AppError> {
    serve_cover(&state, user.user_id, id, CoverSize::Full).await
}

/// `GET /opds/books/{id}/cover/thumb` — thumbnail cover, Basic auth.
///
/// # Errors
/// See [`opds_cover`].
#[utoipa::path(
    get,
    path = "/opds/books/{id}/cover/thumb",
    tag = "opds",
    security(("opds_basic" = [])),
    params(("id" = Uuid, Path, description = "Manifestation id")),
    responses(
        (status = 200, description = "Thumbnail image stream; Cache-Control: no-store"),
        (status = 401, description = "Basic authentication required (WWW-Authenticate: Basic)"),
        (status = 404, description = "Manifestation missing, RLS-hidden, or coverless", body = crate::openapi::ProblemDetails)
    )
)]
async fn opds_cover_thumb(
    BasicOnly(user): BasicOnly,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Response, AppError> {
    serve_cover(&state, user.user_id, id, CoverSize::Thumb).await
}

/// `GET /api/v1/books/{id}/cover` — full-size cover for the web UI.
///
/// # Errors
/// See [`opds_cover`].
#[utoipa::path(
    get,
    path = "/api/v1/books/{id}/cover",
    tag = "library",
    params(("id" = Uuid, Path, description = "Manifestation id")),
    responses(
        (status = 200, description = "Cover image stream (`image/jpeg` / `image/png` / `image/webp`); Cache-Control: no-store"),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails),
        (status = 404, description = "Manifestation missing, RLS-hidden, or coverless", body = crate::openapi::ProblemDetails)
    )
)]
async fn api_cover(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Response, AppError> {
    serve_cover(&state, user.user_id, id, CoverSize::Full).await
}

/// `GET /api/v1/books/{id}/cover/thumb` — thumbnail cover for the web UI.
///
/// # Errors
/// See [`opds_cover`].
#[utoipa::path(
    get,
    path = "/api/v1/books/{id}/cover/thumb",
    tag = "library",
    params(("id" = Uuid, Path, description = "Manifestation id")),
    responses(
        (status = 200, description = "Thumbnail image stream; Cache-Control: no-store"),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails),
        (status = 404, description = "Manifestation missing, RLS-hidden, or coverless", body = crate::openapi::ProblemDetails)
    )
)]
async fn api_cover_thumb(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Response, AppError> {
    serve_cover(&state, user.user_id, id, CoverSize::Thumb).await
}

async fn serve_cover(
    state: &AppState,
    user_id: Uuid,
    manifestation_id: Uuid,
    size: CoverSize,
) -> Result<Response, AppError> {
    let path = match get_or_create(state, manifestation_id, user_id, size).await {
        Ok(p) => p,
        Err(CoverError::NoCover) => return Err(AppError::NotFound),
        Err(e) => return Err(AppError::Internal(anyhow::anyhow!(e))),
    };

    let content_type = match path.extension().and_then(|e| e.to_str()) {
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("png") => "image/png",
        Some("webp") => "image/webp",
        _ => "application/octet-stream",
    };

    let file = File::open(&path)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    let stream = ReaderStream::new(file);
    let body = Body::from_stream(stream);

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CACHE_CONTROL, "no-store")
        .body(body)
        .map_err(|e| AppError::Internal(e.into()))
}
