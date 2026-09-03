//! `/opds/shelves/:shelf_id/*` handlers. Delegates to the shared
//! [`super::library`] emit_* helpers with [`Scope::Shelf`]. Every shelf route
//! verifies ownership under `acquire_with_rls` and returns 404 for foreign
//! shelves to avoid leaking shelf existence.

use axum::extract::{Path, State};
use axum::response::Response;
use axum_extra::extract::{Query, QueryRejection};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
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

/// Build the `/opds/shelves/:shelf_id/*` router. Every handler verifies
/// shelf ownership under [`crate::db::acquire_with_rls`] and 404s on
/// foreign shelves to avoid leaking shelf existence.
pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(shelf_root))
        .routes(routes!(shelf_new))
        .routes(routes!(shelf_authors))
        .routes(routes!(shelf_author_books))
        .routes(routes!(shelf_series))
        .routes(routes!(shelf_series_books))
        .routes(routes!(shelf_search))
}

async fn assert_shelf_owned(
    state: &AppState,
    user_id: Uuid,
    shelf_id: Uuid,
) -> Result<String, AppError> {
    let mut tx = db::acquire_with_rls(&state.pool, user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    let row = sqlx::query_scalar!(
        "SELECT name FROM shelves \
         WHERE id = $1 \
           AND user_id = current_setting('app.current_user_id', true)::uuid \
         LIMIT 1",
        shelf_id,
    )
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    row.ok_or(AppError::NotFound)
}

/// `GET /opds/shelves/{shelf_id}` — shelf subcatalog navigation root.
///
/// # Errors
/// - [`AppError::NotFound`] when the shelf is missing or not owned by the
///   caller (existence not leaked).
/// - [`AppError::Internal`] on database errors or unconfigured base URL.
#[utoipa::path(
    get,
    path = "/opds/shelves/{shelf_id}",
    tag = "opds",
    security(("opds_basic" = [])),
    params(("shelf_id" = Uuid, Path, description = "Shelf id")),
    responses(
        (status = 200, description = "OPDS navigation feed linking the shelf's New / Authors / Series subcatalogs", content_type = "application/atom+xml;profile=opds-catalog;kind=navigation", body = String),
        (status = 401, description = "Basic authentication required (WWW-Authenticate: Basic)", body = crate::openapi::ProblemDetails),
        (status = 404, description = "Shelf missing or not owned by the caller", body = crate::openapi::ProblemDetails)
    )
)]
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

/// `GET /opds/shelves/{shelf_id}/new` — the shelf's newest books,
/// cursor-paginated.
///
/// # Errors
/// - [`AppError::NotFound`] when the shelf is missing or not owned by the
///   caller (existence not leaked).
/// - [`AppError::Validation`] on a malformed cursor.
/// - [`AppError::Internal`] on database errors or unconfigured base URL.
#[utoipa::path(
    get,
    path = "/opds/shelves/{shelf_id}/new",
    tag = "opds",
    security(("opds_basic" = [])),
    params(("shelf_id" = Uuid, Path, description = "Shelf id"), PageParams),
    responses(
        (status = 200, description = "OPDS acquisition feed of the shelf's newest visible books; rel=\"next\" link carries the cursor", content_type = "application/atom+xml;profile=opds-catalog;kind=acquisition", body = String),
        (status = 400, description = "Malformed query parameter", body = crate::openapi::ProblemDetails),
        (status = 401, description = "Basic authentication required (WWW-Authenticate: Basic)", body = crate::openapi::ProblemDetails),
        (status = 404, description = "Shelf missing or not owned by the caller", body = crate::openapi::ProblemDetails),
        (status = 422, description = "Malformed cursor", body = crate::openapi::ProblemDetails)
    )
)]
async fn shelf_new(
    BasicOnly(user): BasicOnly,
    State(state): State<AppState>,
    Path(shelf_id): Path<Uuid>,
    params: Result<Query<PageParams>, QueryRejection>,
) -> Result<Response, AppError> {
    let Query(params) = params?;
    assert_shelf_owned(&state, user.user_id, shelf_id).await?;
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

/// `GET /opds/shelves/{shelf_id}/authors` — the shelf's author index,
/// cursor-paginated.
///
/// # Errors
/// - [`AppError::NotFound`] when the shelf is missing or not owned by the
///   caller (existence not leaked).
/// - [`AppError::Validation`] on a malformed cursor.
/// - [`AppError::Internal`] on database errors or unconfigured base URL.
#[utoipa::path(
    get,
    path = "/opds/shelves/{shelf_id}/authors",
    tag = "opds",
    security(("opds_basic" = [])),
    params(("shelf_id" = Uuid, Path, description = "Shelf id"), PageParams),
    responses(
        (status = 200, description = "OPDS navigation feed of authors with books on the shelf", content_type = "application/atom+xml;profile=opds-catalog;kind=navigation", body = String),
        (status = 400, description = "Malformed query parameter", body = crate::openapi::ProblemDetails),
        (status = 401, description = "Basic authentication required (WWW-Authenticate: Basic)", body = crate::openapi::ProblemDetails),
        (status = 404, description = "Shelf missing or not owned by the caller", body = crate::openapi::ProblemDetails),
        (status = 422, description = "Malformed cursor", body = crate::openapi::ProblemDetails)
    )
)]
async fn shelf_authors(
    BasicOnly(user): BasicOnly,
    State(state): State<AppState>,
    Path(shelf_id): Path<Uuid>,
    params: Result<Query<PageParams>, QueryRejection>,
) -> Result<Response, AppError> {
    let Query(params) = params?;
    assert_shelf_owned(&state, user.user_id, shelf_id).await?;
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

/// `GET /opds/shelves/{shelf_id}/authors/{author_id}` — one author's books
/// on the shelf, cursor-paginated.
///
/// # Errors
/// - [`AppError::NotFound`] when the shelf is missing or not owned by the
///   caller (existence not leaked).
/// - [`AppError::Validation`] on a malformed cursor.
/// - [`AppError::Internal`] on database errors or unconfigured base URL.
#[utoipa::path(
    get,
    path = "/opds/shelves/{shelf_id}/authors/{author_id}",
    tag = "opds",
    security(("opds_basic" = [])),
    params(
        ("shelf_id" = Uuid, Path, description = "Shelf id"),
        ("author_id" = Uuid, Path, description = "Author id"),
        PageParams
    ),
    responses(
        (status = 200, description = "OPDS acquisition feed of the author's books on the shelf (empty for unknown authors)", content_type = "application/atom+xml;profile=opds-catalog;kind=acquisition", body = String),
        (status = 400, description = "Malformed query parameter", body = crate::openapi::ProblemDetails),
        (status = 401, description = "Basic authentication required (WWW-Authenticate: Basic)", body = crate::openapi::ProblemDetails),
        (status = 404, description = "Shelf missing or not owned by the caller", body = crate::openapi::ProblemDetails),
        (status = 422, description = "Malformed cursor", body = crate::openapi::ProblemDetails)
    )
)]
async fn shelf_author_books(
    BasicOnly(user): BasicOnly,
    State(state): State<AppState>,
    Path((shelf_id, author_id)): Path<(Uuid, Uuid)>,
    params: Result<Query<PageParams>, QueryRejection>,
) -> Result<Response, AppError> {
    let Query(params) = params?;
    assert_shelf_owned(&state, user.user_id, shelf_id).await?;
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

/// `GET /opds/shelves/{shelf_id}/series` — the shelf's series index,
/// cursor-paginated.
///
/// # Errors
/// - [`AppError::NotFound`] when the shelf is missing or not owned by the
///   caller (existence not leaked).
/// - [`AppError::Validation`] on a malformed cursor.
/// - [`AppError::Internal`] on database errors or unconfigured base URL.
#[utoipa::path(
    get,
    path = "/opds/shelves/{shelf_id}/series",
    tag = "opds",
    security(("opds_basic" = [])),
    params(("shelf_id" = Uuid, Path, description = "Shelf id"), PageParams),
    responses(
        (status = 200, description = "OPDS navigation feed of series with books on the shelf", content_type = "application/atom+xml;profile=opds-catalog;kind=navigation", body = String),
        (status = 400, description = "Malformed query parameter", body = crate::openapi::ProblemDetails),
        (status = 401, description = "Basic authentication required (WWW-Authenticate: Basic)", body = crate::openapi::ProblemDetails),
        (status = 404, description = "Shelf missing or not owned by the caller", body = crate::openapi::ProblemDetails),
        (status = 422, description = "Malformed cursor", body = crate::openapi::ProblemDetails)
    )
)]
async fn shelf_series(
    BasicOnly(user): BasicOnly,
    State(state): State<AppState>,
    Path(shelf_id): Path<Uuid>,
    params: Result<Query<PageParams>, QueryRejection>,
) -> Result<Response, AppError> {
    let Query(params) = params?;
    assert_shelf_owned(&state, user.user_id, shelf_id).await?;
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

/// `GET /opds/shelves/{shelf_id}/series/{series_id}` — one series' books
/// on the shelf, in series order, cursor-paginated.
///
/// # Errors
/// - [`AppError::NotFound`] when the shelf is missing or not owned by the
///   caller (existence not leaked).
/// - [`AppError::Validation`] on a malformed cursor.
/// - [`AppError::Internal`] on database errors or unconfigured base URL.
#[utoipa::path(
    get,
    path = "/opds/shelves/{shelf_id}/series/{series_id}",
    tag = "opds",
    security(("opds_basic" = [])),
    params(
        ("shelf_id" = Uuid, Path, description = "Shelf id"),
        ("series_id" = Uuid, Path, description = "Series id"),
        PageParams
    ),
    responses(
        (status = 200, description = "OPDS acquisition feed of the series' books on the shelf (empty for unknown series)", content_type = "application/atom+xml;profile=opds-catalog;kind=acquisition", body = String),
        (status = 400, description = "Malformed query parameter", body = crate::openapi::ProblemDetails),
        (status = 401, description = "Basic authentication required (WWW-Authenticate: Basic)", body = crate::openapi::ProblemDetails),
        (status = 404, description = "Shelf missing or not owned by the caller", body = crate::openapi::ProblemDetails),
        (status = 422, description = "Malformed cursor", body = crate::openapi::ProblemDetails)
    )
)]
async fn shelf_series_books(
    BasicOnly(user): BasicOnly,
    State(state): State<AppState>,
    Path((shelf_id, series_id)): Path<(Uuid, Uuid)>,
    params: Result<Query<PageParams>, QueryRejection>,
) -> Result<Response, AppError> {
    let Query(params) = params?;
    assert_shelf_owned(&state, user.user_id, shelf_id).await?;
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

/// `GET /opds/shelves/{shelf_id}/search` — full-text search over the
/// shelf's books.
///
/// # Errors
/// - [`AppError::NotFound`] when the shelf is missing or not owned by the
///   caller (existence not leaked).
/// - [`AppError::Internal`] on database errors or unconfigured base URL.
#[utoipa::path(
    get,
    path = "/opds/shelves/{shelf_id}/search",
    tag = "opds",
    security(("opds_basic" = [])),
    params(("shelf_id" = Uuid, Path, description = "Shelf id"), SearchParams),
    responses(
        (status = 200, description = "OPDS acquisition feed of search results scoped to the shelf; empty/whitespace query yields an empty feed", content_type = "application/atom+xml;profile=opds-catalog;kind=acquisition", body = String),
        (status = 400, description = "Malformed query parameter", body = crate::openapi::ProblemDetails),
        (status = 401, description = "Basic authentication required (WWW-Authenticate: Basic)", body = crate::openapi::ProblemDetails),
        (status = 404, description = "Shelf missing or not owned by the caller", body = crate::openapi::ProblemDetails)
    )
)]
async fn shelf_search(
    BasicOnly(user): BasicOnly,
    State(state): State<AppState>,
    Path(shelf_id): Path<Uuid>,
    params: Result<Query<SearchParams>, QueryRejection>,
) -> Result<Response, AppError> {
    let Query(params) = params?;
    assert_shelf_owned(&state, user.user_id, shelf_id).await?;
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
