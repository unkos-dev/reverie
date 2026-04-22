//! `GET /opds` — navigation feed linking to `/opds/library` and every shelf
//! owned by the authenticated user.

use axum::Router;
use axum::extract::State;
use axum::http::{StatusCode, header};
use axum::response::Response;
use axum::routing::get;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::auth::basic_only::BasicOnly;
use crate::db;
use crate::error::AppError;
use crate::state::AppState;

use super::feed::{FeedBuilder, FeedKind, feed_urn, shelf_urn};

pub fn router() -> Router<AppState> {
    Router::new().route("/opds", get(opds_root))
}

pub(super) fn base_url(state: &AppState) -> Result<&url::Url, AppError> {
    state
        .config
        .opds
        .public_url
        .as_ref()
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("REVERIE_PUBLIC_URL not configured")))
}

pub(super) fn atom_response(body: Vec<u8>, content_type: &str) -> Response {
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .body(axum::body::Body::from(body))
        .expect("build atom response")
}

async fn opds_root(
    BasicOnly(user): BasicOnly,
    State(state): State<AppState>,
) -> Result<Response, AppError> {
    let base = base_url(&state)?.clone();
    let mut tx = db::acquire_with_rls(&state.pool, user.user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let shelves: Vec<(Uuid, String)> = sqlx::query_as(
        "SELECT id, name FROM shelves \
         WHERE user_id = current_setting('app.current_user_id', true)::uuid \
         ORDER BY name ASC",
    )
    .fetch_all(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    let mut fb = FeedBuilder::new(
        &base,
        "/opds",
        FeedKind::Navigation,
        "Reverie",
        OffsetDateTime::now_utc(),
    );
    fb.add_search_link("/opds/library/opensearch.xml");
    fb.add_navigation_entry(&feed_urn("/opds/library"), "Library", "/opds/library", true);
    for (shelf_id, name) in shelves {
        fb.add_navigation_entry(
            &shelf_urn(shelf_id),
            &name,
            &format!("/opds/shelves/{shelf_id}"),
            true,
        );
    }
    Ok(atom_response(
        fb.finish(),
        FeedKind::Navigation.content_type(),
    ))
}
