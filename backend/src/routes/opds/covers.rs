//! Cover image handlers, dual-mounted. `/opds/books/:id/cover{,/thumb}` sits
//! under `BasicOnly` so OPDS clients' Basic credentials stay within the
//! RFC 7617 paired protection space. `/api/v1/books/:id/cover{,/thumb}` sits
//! under `CurrentUser` (cookie-or-Basic) for the web UI. Handler body is
//! shared; the two mounts differ only in extractor wrapping.

use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
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

/// `Vary` for cover responses.
///
/// THREAT: covers are RLS-scoped, so two users hitting the same cover URL can
/// get different results (one a `200`, one a `404`). `private` keeps shared
/// proxies out but does NOT partition a browser's *own* cache by user — without
/// `Vary`, on a shared browser a cached cover for user A would be replayed to
/// user B after an account switch, leaking an RLS-hidden cover (e.g. a child
/// account seeing an adult-only cover an adult viewed). `Vary` partitions the
/// cache by the credential: `Authorization` for OPDS Basic, `Cookie` for the
/// web session. Session/Basic credentials are stable within a session, so this
/// preserves the per-session caching win while closing the cross-user replay.
const COVER_VARY: &str = "Authorization, Cookie";

/// `Cache-Control` for cover responses. `private` (not `no-store`) — covers are
/// RLS-scoped images, not credentials, and `codeguard-0-http-headers.md`
/// reserves `no-store` for sensitive data. Cross-user cache replay is handled
/// by [`COVER_VARY`], not by suppressing caching. See
/// `docs/adr/0025-cover-cache-headers-ingest-pre-warm-and-jpeg-thumbnails.md`.
fn cover_cache_control() -> String {
    format!("private, max-age={COVER_MAX_AGE_SECS}")
}

/// `max-age` for the negative (missing-file) cover response. Deliberately far
/// shorter than [`COVER_MAX_AGE_SECS`]: a missing cover file is an
/// operator-remediable state (a re-scan or a remounted library restores the
/// same URL, and a `404` carries no `ETag` to revalidate against), so the
/// negative response is cached only long enough to collapse the repeated
/// misses of one library-grid load rather than pinning a broken image for a
/// day.
const COVER_MISS_MAX_AGE_SECS: u32 = 60;

/// `Cache-Control` for the missing-file `404`. `private` matches the success
/// path so no shared proxy stores an RLS-scoped negative response.
fn cover_miss_cache_control() -> String {
    format!("private, max-age={COVER_MISS_MAX_AGE_SECS}")
}

/// Build the cacheable `404` for a cover whose backing file is absent on disk.
/// Reuses the canonical [`AppError::NotFound`] problem+json body so the error
/// shape and RFC 9457 `instance` capture match every other `404`, then adds the
/// same `private` scoping and [`COVER_VARY`] partitioning as the success path
/// (at a short negative TTL) so a grid of missing covers is not re-fetched from
/// disk on every navigation.
fn cover_miss_not_found() -> Response {
    let mut response = AppError::NotFound.into_response();
    let headers = response.headers_mut();
    // Best-effort cache hints: a header-format failure must never escalate the
    // 404 into a 500, so parse fallibly and skip the header on error.
    if let Ok(value) = HeaderValue::from_str(&cover_miss_cache_control()) {
        headers.insert(header::CACHE_CONTROL, value);
    }
    headers.insert(header::VARY, HeaderValue::from_static(COVER_VARY));
    response
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
        (status = 401, description = "Basic authentication required (WWW-Authenticate: Basic)", body = crate::openapi::ProblemDetails),
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
        (status = 401, description = "Basic authentication required (WWW-Authenticate: Basic)", body = crate::openapi::ProblemDetails),
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
    security(("session_cookie" = ["read"]), ("device_token_bearer" = ["read"]), ("oidc_jwt_bearer" = ["read"]), ("opds_basic" = ["read"])),
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
    security(("session_cookie" = ["read"]), ("device_token_bearer" = ["read"]), ("oidc_jwt_bearer" = ["read"]), ("opds_basic" = ["read"])),
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
        // A cover file that has vanished from disk (the source EPUB moved, an
        // evicted cache entry, an unmounted library) is a 404, not a 500: the
        // manifestation was already RLS-gated inside `get_or_create`, so this
        // discloses nothing the caller's own list row did not, and RFC 9110
        // §15.5.5 is the correct status. It also replaces the per-request
        // `Internal` ERROR, which floods the log once per thumbnail when a whole
        // shelf of files is missing on a grid load, with a single WARN.
        Err(CoverError::Io(e)) if e.kind() == std::io::ErrorKind::NotFound => {
            tracing::warn!(%manifestation_id, ?size, error = %e, "cover file missing on disk; serving 404");
            return Ok(cover_miss_not_found());
        }
        Err(e) => return Err(AppError::Internal(anyhow::anyhow!(e))),
    };

    // Strong validator, quoted per RFC 9110 §8.8.3. We emit exactly one strong
    // ETag and browsers echo it back verbatim, so an exact compare is
    // sufficient: list / `*` / weak `W/` forms only arise from hand-crafted
    // requests, and any unmatched form falls through to a correct `200` (never
    // a wrong `304`).
    let etag = format!("\"{}\"", artifact.etag);

    if if_none_match.is_some_and(|inm| inm == etag) {
        return Response::builder()
            .status(StatusCode::NOT_MODIFIED)
            .header(header::ETAG, &etag)
            .header(header::CACHE_CONTROL, cover_cache_control())
            .header(header::VARY, COVER_VARY)
            .body(Body::empty())
            .map_err(|e| AppError::Internal(e.into()));
    }

    let content_type = match artifact.path.extension().and_then(|e| e.to_str()) {
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("png") => "image/png",
        Some("webp") => "image/webp",
        _ => "application/octet-stream",
    };

    let file = match File::open(&artifact.path).await {
        Ok(f) => f,
        // The cached artifact was present when `get_or_create` resolved it but
        // was removed before this open (a cache-eviction race). Same reasoning
        // as the miss-path branch above: a missing file is a 404, not a 500.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            tracing::warn!(%manifestation_id, ?size, "cover file missing on disk; serving 404");
            return Ok(cover_miss_not_found());
        }
        Err(e) => return Err(AppError::Internal(e.into())),
    };
    let stream = ReaderStream::new(file);
    let body = Body::from_stream(stream);

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CACHE_CONTROL, cover_cache_control())
        .header(header::ETAG, &etag)
        .header(header::VARY, COVER_VARY)
        .body(body)
        .map_err(|e| AppError::Internal(e.into()))
}
