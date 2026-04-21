//! `/opds/shelves/:shelf_id/*` handlers. Delegates to the shared
//! [`super::library`] emit_* helpers with [`Scope::Shelf`]. Every shelf route
//! verifies ownership under `acquire_with_rls` and returns 404 on foreign
//! shelves (BLUEPRINT: cross-user access returns 404, not 403).

use axum::Router;
use axum::extract::{Path, Query, State};
use axum::response::Response;
use axum::routing::get;
use uuid::Uuid;

use crate::auth::basic_only::BasicOnly;
use crate::db;
use crate::error::AppError;
use crate::state::AppState;

use super::feed::FeedKind;
use super::library::{
    PageParams, SearchParams, emit_author_books, emit_authors, emit_new, emit_search, emit_series,
    emit_series_books,
};
use super::root::{atom_response, base_url};
use super::scope::Scope;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/opds/shelves/{shelf_id}", get(shelf_root))
        .route("/opds/shelves/{shelf_id}/new", get(shelf_new))
        .route("/opds/shelves/{shelf_id}/authors", get(shelf_authors))
        .route(
            "/opds/shelves/{shelf_id}/authors/{author_id}",
            get(shelf_author_books),
        )
        .route("/opds/shelves/{shelf_id}/series", get(shelf_series))
        .route(
            "/opds/shelves/{shelf_id}/series/{series_id}",
            get(shelf_series_books),
        )
        .route("/opds/shelves/{shelf_id}/search", get(shelf_search))
}

async fn assert_shelf_owned(
    state: &AppState,
    user_id: Uuid,
    shelf_id: Uuid,
) -> Result<String, AppError> {
    let mut tx = db::acquire_with_rls(&state.pool, user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT name FROM shelves \
         WHERE id = $1 \
           AND user_id = current_setting('app.current_user_id', true)::uuid \
         LIMIT 1",
    )
    .bind(shelf_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    row.map(|(name,)| name).ok_or(AppError::NotFound)
}

async fn shelf_root(
    BasicOnly(user): BasicOnly,
    State(state): State<AppState>,
    Path(shelf_id): Path<Uuid>,
) -> Result<Response, AppError> {
    let base = base_url(&state)?.clone();
    let name = assert_shelf_owned(&state, user.user_id, shelf_id).await?;
    let self_path = format!("/opds/shelves/{shelf_id}");
    let bytes = super::library::build_subcatalog_root(&base, &self_path, &name);
    Ok(atom_response(bytes, FeedKind::Navigation.content_type()))
}

async fn shelf_new(
    BasicOnly(user): BasicOnly,
    State(state): State<AppState>,
    Path(shelf_id): Path<Uuid>,
    Query(params): Query<PageParams>,
) -> Result<Response, AppError> {
    let _ = assert_shelf_owned(&state, user.user_id, shelf_id).await?;
    let base = base_url(&state)?.clone();
    let self_parent = format!("/opds/shelves/{shelf_id}");
    let bytes = emit_new(
        &state,
        user.user_id,
        &Scope::Shelf(shelf_id),
        &self_parent,
        &base,
        params.cursor,
    )
    .await?;
    Ok(atom_response(bytes, FeedKind::Acquisition.content_type()))
}

async fn shelf_authors(
    BasicOnly(user): BasicOnly,
    State(state): State<AppState>,
    Path(shelf_id): Path<Uuid>,
    Query(params): Query<PageParams>,
) -> Result<Response, AppError> {
    let _ = assert_shelf_owned(&state, user.user_id, shelf_id).await?;
    let base = base_url(&state)?.clone();
    let self_parent = format!("/opds/shelves/{shelf_id}");
    let bytes = emit_authors(
        &state,
        user.user_id,
        &Scope::Shelf(shelf_id),
        &self_parent,
        &base,
        params.cursor,
    )
    .await?;
    Ok(atom_response(bytes, FeedKind::Navigation.content_type()))
}

async fn shelf_author_books(
    BasicOnly(user): BasicOnly,
    State(state): State<AppState>,
    Path((shelf_id, author_id)): Path<(Uuid, Uuid)>,
    Query(params): Query<PageParams>,
) -> Result<Response, AppError> {
    let _ = assert_shelf_owned(&state, user.user_id, shelf_id).await?;
    let base = base_url(&state)?.clone();
    let self_parent = format!("/opds/shelves/{shelf_id}");
    let bytes = emit_author_books(
        &state,
        user.user_id,
        &Scope::Shelf(shelf_id),
        &self_parent,
        &base,
        author_id,
        params.cursor,
    )
    .await?;
    Ok(atom_response(bytes, FeedKind::Acquisition.content_type()))
}

async fn shelf_series(
    BasicOnly(user): BasicOnly,
    State(state): State<AppState>,
    Path(shelf_id): Path<Uuid>,
    Query(params): Query<PageParams>,
) -> Result<Response, AppError> {
    let _ = assert_shelf_owned(&state, user.user_id, shelf_id).await?;
    let base = base_url(&state)?.clone();
    let self_parent = format!("/opds/shelves/{shelf_id}");
    let bytes = emit_series(
        &state,
        user.user_id,
        &Scope::Shelf(shelf_id),
        &self_parent,
        &base,
        params.cursor,
    )
    .await?;
    Ok(atom_response(bytes, FeedKind::Navigation.content_type()))
}

async fn shelf_series_books(
    BasicOnly(user): BasicOnly,
    State(state): State<AppState>,
    Path((shelf_id, series_id)): Path<(Uuid, Uuid)>,
    Query(params): Query<PageParams>,
) -> Result<Response, AppError> {
    let _ = assert_shelf_owned(&state, user.user_id, shelf_id).await?;
    let base = base_url(&state)?.clone();
    let self_parent = format!("/opds/shelves/{shelf_id}");
    let bytes = emit_series_books(
        &state,
        user.user_id,
        &Scope::Shelf(shelf_id),
        &self_parent,
        &base,
        series_id,
        params.cursor,
    )
    .await?;
    Ok(atom_response(bytes, FeedKind::Acquisition.content_type()))
}

async fn shelf_search(
    BasicOnly(user): BasicOnly,
    State(state): State<AppState>,
    Path(shelf_id): Path<Uuid>,
    Query(params): Query<SearchParams>,
) -> Result<Response, AppError> {
    let _ = assert_shelf_owned(&state, user.user_id, shelf_id).await?;
    let base = base_url(&state)?.clone();
    let self_parent = format!("/opds/shelves/{shelf_id}");
    let bytes = emit_search(
        &state,
        user.user_id,
        &Scope::Shelf(shelf_id),
        &self_parent,
        &base,
        &params.q,
    )
    .await?;
    Ok(atom_response(bytes, FeedKind::Acquisition.content_type()))
}
