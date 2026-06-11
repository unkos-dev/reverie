//! `GET /opds` — navigation feed linking to `/opds/library` and every shelf
//! owned by the authenticated user.

use axum::extract::State;
use axum::http::{StatusCode, header};
use axum::response::Response;
use time::OffsetDateTime;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::auth::basic_only::BasicOnly;
use crate::db;
use crate::error::AppError;
use crate::state::AppState;

use super::feed::{FeedBuilder, FeedKind, feed_urn, shelf_urn};

/// Build the `/opds` root navigation router.
pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(opds_root))
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
    #[allow(
        clippy::expect_used,
        reason = "Response::builder() with a valid StatusCode and caller-controlled content_type string; callers always pass a well-known MIME constant so this cannot fail at runtime"
    )]
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .body(axum::body::Body::from(body))
        .expect("build atom response")
}

/// `GET /opds` — catalog root: navigation feed linking `/opds/library`
/// and every shelf owned by the authenticated user.
///
/// # Errors
/// - [`AppError::Internal`] on database errors or unconfigured base URL.
#[utoipa::path(
    get,
    path = "/opds",
    tag = "opds",
    security(("opds_basic" = [])),
    responses(
        (status = 200, description = "OPDS navigation feed linking the library catalog and the user's shelves", content_type = "application/atom+xml;profile=opds-catalog;kind=navigation", body = String),
        (status = 401, description = "Basic authentication required (WWW-Authenticate: Basic)")
    )
)]
async fn opds_root(
    BasicOnly(user): BasicOnly,
    State(state): State<AppState>,
) -> Result<Response, AppError> {
    let base = base_url(&state)?.clone();
    let mut tx = db::acquire_with_rls(&state.pool, user.user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let shelves = sqlx::query!(
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
    for row in shelves {
        fb.add_navigation_entry(
            &shelf_urn(row.id),
            &row.name,
            &format!("/opds/shelves/{}", row.id),
            true,
        );
    }
    Ok(atom_response(
        fb.finish(),
        FeedKind::Navigation.content_type(),
    ))
}
