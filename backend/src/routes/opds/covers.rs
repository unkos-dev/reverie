//! Cover image handlers, dual-mounted. `/opds/books/:id/cover{,/thumb}` sits
//! under `BasicOnly` so OPDS clients' Basic credentials stay within the
//! RFC 7617 paired protection space. `/api/v1/books/:id/cover{,/thumb}` sits
//! under `CurrentUser` (cookie-or-Basic) for the web UI. Handler body is
//! shared; the two mounts differ only in extractor wrapping.

use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, header};
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

/// `max-age` for cover responses. Covers are content-addressed on disk and
/// carry a strong `ETag`, so a day of client caching is safe: a Step 8
/// writeback changes the `ETag`, and the browser revalidates (304 when
/// unchanged) once `max-age` lapses.
const COVER_MAX_AGE_SECS: u32 = 86_400;

/// `Cache-Control` for cover responses.
///
/// THREAT: covers are RLS-scoped per-user content, not credentials —
/// `codeguard-0-http-headers.md` reserves `no-store` for sensitive data. We
/// use `private` so shared proxies/CDNs never store a cover; the residual is a
/// cover lingering in a *shared browser* cache post-logout (low severity — a
/// public-domain book cover, and re-fetching still requires auth). See
/// `adr/2026-06-14-cover-cache-headers-and-thumbnail-encoding.md`.
fn cover_cache_control() -> String {
    format!("private, max-age={COVER_MAX_AGE_SECS}")
}

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
        (status = 200, description = "Cover image stream (`image/jpeg` / `image/png`); Cache-Control: private, max-age=86400; carries a strong ETag"),
        (status = 304, description = "Not Modified — the request's If-None-Match matched the cover ETag"),
        (status = 401, description = "Basic authentication required (WWW-Authenticate: Basic)"),
        (status = 404, description = "Manifestation missing, RLS-hidden, or coverless", body = crate::openapi::ProblemDetails)
    )
)]
async fn opds_cover(
    BasicOnly(user): BasicOnly,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    serve_cover(
        &state,
        user.user_id,
        id,
        CoverSize::Full,
        if_none_match(&headers),
    )
    .await
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
        (status = 200, description = "Thumbnail image stream (`image/jpeg`); Cache-Control: private, max-age=86400; carries a strong ETag"),
        (status = 304, description = "Not Modified — the request's If-None-Match matched the cover ETag"),
        (status = 401, description = "Basic authentication required (WWW-Authenticate: Basic)"),
        (status = 404, description = "Manifestation missing, RLS-hidden, or coverless", body = crate::openapi::ProblemDetails)
    )
)]
async fn opds_cover_thumb(
    BasicOnly(user): BasicOnly,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    serve_cover(
        &state,
        user.user_id,
        id,
        CoverSize::Thumb,
        if_none_match(&headers),
    )
    .await
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
        (status = 200, description = "Cover image stream (`image/jpeg` / `image/png`); Cache-Control: private, max-age=86400; carries a strong ETag"),
        (status = 304, description = "Not Modified — the request's If-None-Match matched the cover ETag"),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails),
        (status = 404, description = "Manifestation missing, RLS-hidden, or coverless", body = crate::openapi::ProblemDetails)
    )
)]
async fn api_cover(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    serve_cover(
        &state,
        user.user_id,
        id,
        CoverSize::Full,
        if_none_match(&headers),
    )
    .await
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
        (status = 200, description = "Thumbnail image stream (`image/jpeg`); Cache-Control: private, max-age=86400; carries a strong ETag"),
        (status = 304, description = "Not Modified — the request's If-None-Match matched the cover ETag"),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails),
        (status = 404, description = "Manifestation missing, RLS-hidden, or coverless", body = crate::openapi::ProblemDetails)
    )
)]
async fn api_cover_thumb(
    user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    serve_cover(
        &state,
        user.user_id,
        id,
        CoverSize::Thumb,
        if_none_match(&headers),
    )
    .await
}

/// Extract the request's `If-None-Match` validator, if present and UTF-8.
fn if_none_match(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
}

async fn serve_cover(
    state: &AppState,
    user_id: Uuid,
    manifestation_id: Uuid,
    size: CoverSize,
    if_none_match: Option<&str>,
) -> Result<Response, AppError> {
    let artifact = match get_or_create(state, manifestation_id, user_id, size).await {
        Ok(a) => a,
        Err(CoverError::NoCover) => return Err(AppError::NotFound),
        Err(e) => return Err(AppError::Internal(anyhow::anyhow!(e))),
    };

    // Strong validator, quoted per RFC 9110 §8.8.3. Browsers echo the quoted
    // form verbatim in If-None-Match, so compare against the quoted string.
    let etag = format!("\"{}\"", artifact.etag);

    if if_none_match.is_some_and(|inm| inm == etag) {
        return Response::builder()
            .status(StatusCode::NOT_MODIFIED)
            .header(header::ETAG, &etag)
            .header(header::CACHE_CONTROL, cover_cache_control())
            .body(Body::empty())
            .map_err(|e| AppError::Internal(e.into()));
    }

    let content_type = match artifact.path.extension().and_then(|e| e.to_str()) {
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("png") => "image/png",
        Some("webp") => "image/webp",
        _ => "application/octet-stream",
    };

    let file = File::open(&artifact.path)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    let stream = ReaderStream::new(file);
    let body = Body::from_stream(stream);

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CACHE_CONTROL, cover_cache_control())
        .header(header::ETAG, &etag)
        .body(body)
        .map_err(|e| AppError::Internal(e.into()))
}
