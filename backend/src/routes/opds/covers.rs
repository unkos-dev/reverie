//! Cover image handlers, dual-mounted. `/opds/books/:id/cover{,/thumb}` sits
//! under `BasicOnly` so OPDS clients' Basic credentials stay within the
//! RFC 7617 paired protection space. `/api/books/:id/cover{,/thumb}` sits
//! under `CurrentUser` (cookie-or-Basic) for the web UI. Handler body is
//! shared; the two mounts differ only in extractor wrapping.

use axum::Router;
use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{StatusCode, header};
use axum::response::Response;
use axum::routing::get;
use tokio::fs::File;
use tokio_util::io::ReaderStream;
use uuid::Uuid;

use crate::auth::basic_only::BasicOnly;
use crate::auth::middleware::CurrentUser;
use crate::error::AppError;
use crate::services::covers::{CoverError, CoverSize, get_or_create};
use crate::state::AppState;

pub fn opds_router() -> Router<AppState> {
    Router::new()
        .route(
            "/opds/books/{id}/cover",
            get(
                |BasicOnly(user): BasicOnly,
                 State(state): State<AppState>,
                 Path(id): Path<Uuid>| async move {
                    serve_cover(&state, user.user_id, id, CoverSize::Full).await
                },
            ),
        )
        .route(
            "/opds/books/{id}/cover/thumb",
            get(
                |BasicOnly(user): BasicOnly,
                 State(state): State<AppState>,
                 Path(id): Path<Uuid>| async move {
                    serve_cover(&state, user.user_id, id, CoverSize::Thumb).await
                },
            ),
        )
}

pub fn api_router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/books/{id}/cover",
            get(
                |user: CurrentUser,
                 State(state): State<AppState>,
                 Path(id): Path<Uuid>| async move {
                    serve_cover(&state, user.user_id, id, CoverSize::Full).await
                },
            ),
        )
        .route(
            "/api/books/{id}/cover/thumb",
            get(
                |user: CurrentUser,
                 State(state): State<AppState>,
                 Path(id): Path<Uuid>| async move {
                    serve_cover(&state, user.user_id, id, CoverSize::Thumb).await
                },
            ),
        )
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
        Some("jpg") | Some("jpeg") => "image/jpeg",
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
