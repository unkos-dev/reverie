//! `/opds/library/*` and shared scope-parameterised handlers for the
//! `new`, `authors`, `authors/:id`, `series`, `series/:id`, `search`
//! subcatalogs.
//!
//! `/opds/shelves/:id/*` delegates to these same handlers via
//! [`super::shelves::router`] with [`Scope::Shelf`].

use std::collections::HashMap;

use axum::extract::{Path, State};
use axum::response::Response;
use axum_extra::extract::{Query, QueryRejection};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use sqlx::{Postgres, QueryBuilder, Row};
use url::Url;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::auth::basic_only::BasicOnly;
use crate::db;
use crate::error::AppError;
use crate::state::AppState;

use super::feed::{AcquisitionEntry, FeedBuilder, FeedKind, author_urn, feed_urn, series_urn};
use super::root::{atom_response, base_url};
use super::scope::{Scope, push_scope};

/// Build the `/opds/library/*` router.
///
/// # Invariants
/// - Every handler authenticates via the `BasicOnly` extractor — OPDS
///   clients (e.g. `KOReader`) are Basic-only; session cookies are not
///   accepted on these routes.
/// - Per-row visibility is enforced inside `db::acquire_with_rls` plus
///   a `Scope` predicate (`Scope::Library` here, `Scope::Shelf` when
///   reused via [`super::shelves::router`]) appended through
///   [`push_scope`].
///
/// Why: the feed URL determines the requested scope, but RLS in
/// `acquire_with_rls` is the authoritative guard — `Scope` shapes
/// which subset of visible rows the feed surfaces, and an over-broad
/// or forged scope cannot bypass row-level security.
pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(library_root))
        .routes(routes!(library_new))
        .routes(routes!(library_authors))
        .routes(routes!(library_author_books))
        .routes(routes!(library_series))
        .routes(routes!(library_series_books))
        .routes(routes!(library_search))
}

// ── Library navigation root ──────────────────────────────────────────────

/// `GET /opds/library` — library subcatalog navigation root.
///
/// # Errors
/// - [`AppError::Internal`] when the OPDS base URL is unconfigured.
#[utoipa::path(
    get,
    path = "/opds/library",
    summary = "Get the library navigation feed",
    description = "Returns an OPDS navigation feed linking the New, Authors and Series subcatalogues for the caller's whole library. Requires HTTP Basic authentication (401).",
    tag = "opds",
    security(("opds_basic" = [])),
    responses(
        (status = 200, description = "OPDS navigation feed linking the New / Authors / Series subcatalogs", content_type = "application/atom+xml;profile=opds-catalog;kind=navigation", body = String),
        (status = 401, description = "Basic authentication required (WWW-Authenticate: Basic)", body = crate::openapi::ProblemDetails)
    )
)]
async fn library_root(
    BasicOnly(user): BasicOnly,
    State(state): State<AppState>,
) -> Result<Response, AppError> {
    let base = base_url(&state)?.clone();
    let _ = user; // RLS context set below is all we need; library root doesn't hit DB.
    Ok(atom_response(
        build_subcatalog_root(&base, "/opds/library", "Library"),
        FeedKind::Navigation.content_type(),
    ))
}

pub(super) fn build_subcatalog_root(base: &Url, self_path: &str, title: &str) -> Vec<u8> {
    let mut fb = FeedBuilder::new(base, self_path, FeedKind::Navigation, title, Utc::now());
    fb.add_search_link(&format!("{self_path}/opensearch.xml"));
    fb.add_navigation_entry(
        &feed_urn(&format!("{self_path}/new")),
        "New",
        &format!("{self_path}/new"),
        true,
    );
    fb.add_navigation_entry(
        &feed_urn(&format!("{self_path}/authors")),
        "Authors",
        &format!("{self_path}/authors"),
        true,
    );
    fb.add_navigation_entry(
        &feed_urn(&format!("{self_path}/series")),
        "Series",
        &format!("{self_path}/series"),
        true,
    );
    fb.finish()
}

// ── Subcatalog handlers ──────────────────────────────────────────────────

/// Cursor pagination input shared by every paginated feed handler.
#[derive(Debug, Deserialize, Default, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct PageParams {
    /// Opaque cursor returned in a previous response's `rel="next"` link;
    /// `None` returns the first page.
    pub cursor: Option<String>,
}

/// `GET /opds/library/new` — newest books, cursor-paginated.
///
/// # Errors
/// - [`AppError::Validation`] on a malformed cursor.
/// - [`AppError::Internal`] on database errors or unconfigured base URL.
#[utoipa::path(
    get,
    path = "/opds/library/new",
    summary = "List the newest books",
    description = "Returns an OPDS acquisition feed of the newest visible books, newest first, cursor-paginated via `cursor`. Requires HTTP Basic authentication (401) and returns 400 for a malformed query parameter or 422 for an invalid cursor.",
    tag = "opds",
    security(("opds_basic" = [])),
    params(PageParams),
    responses(
        (status = 200, description = "OPDS acquisition feed of the newest visible books; rel=\"next\" link carries the cursor", content_type = "application/atom+xml;profile=opds-catalog;kind=acquisition", body = String),
        (status = 400, description = "Malformed query parameter", body = crate::openapi::ProblemDetails),
        (status = 401, description = "Basic authentication required (WWW-Authenticate: Basic)", body = crate::openapi::ProblemDetails),
        (status = 422, description = "Malformed cursor", body = crate::openapi::ProblemDetails)
    )
)]
async fn library_new(
    BasicOnly(user): BasicOnly,
    State(state): State<AppState>,
    params: Result<Query<PageParams>, QueryRejection>,
) -> Result<Response, AppError> {
    let Query(params) = params?;
    let base = base_url(&state)?.clone();
    let bytes = emit_new(
        &state,
        user.user_id,
        &Scope::Library,
        "/opds/library",
        &base,
        params.cursor,
    )
    .await?;
    Ok(atom_response(bytes, FeedKind::Acquisition.content_type()))
}

/// `GET /opds/library/authors` — author index, cursor-paginated.
///
/// # Errors
/// - [`AppError::Validation`] on a malformed cursor.
/// - [`AppError::Internal`] on database errors or unconfigured base URL.
#[utoipa::path(
    get,
    path = "/opds/library/authors",
    summary = "List authors",
    description = "Returns an OPDS navigation feed of authors with at least one visible book, cursor-paginated via `cursor`. Requires HTTP Basic authentication (401) and returns 400 for a malformed query parameter or 422 for an invalid cursor.",
    tag = "opds",
    security(("opds_basic" = [])),
    params(PageParams),
    responses(
        (status = 200, description = "OPDS navigation feed of authors with visible books", content_type = "application/atom+xml;profile=opds-catalog;kind=navigation", body = String),
        (status = 400, description = "Malformed query parameter", body = crate::openapi::ProblemDetails),
        (status = 401, description = "Basic authentication required (WWW-Authenticate: Basic)", body = crate::openapi::ProblemDetails),
        (status = 422, description = "Malformed cursor", body = crate::openapi::ProblemDetails)
    )
)]
async fn library_authors(
    BasicOnly(user): BasicOnly,
    State(state): State<AppState>,
    params: Result<Query<PageParams>, QueryRejection>,
) -> Result<Response, AppError> {
    let Query(params) = params?;
    let base = base_url(&state)?.clone();
    let bytes = emit_authors(
        &state,
        user.user_id,
        &Scope::Library,
        "/opds/library",
        &base,
        params.cursor,
    )
    .await?;
    Ok(atom_response(bytes, FeedKind::Navigation.content_type()))
}

/// `GET /opds/library/authors/{id}` — one author's books,
/// cursor-paginated.
///
/// # Errors
/// - [`AppError::Validation`] on a malformed cursor.
/// - [`AppError::Internal`] on database errors or unconfigured base URL.
#[utoipa::path(
    get,
    path = "/opds/library/authors/{id}",
    summary = "List an author's books",
    description = "Returns an OPDS acquisition feed of one author's visible books, cursor-paginated via `cursor` (empty if the author is unknown). Requires HTTP Basic authentication (401) and returns 400 for a malformed query parameter or 422 for an invalid cursor.",
    tag = "opds",
    security(("opds_basic" = [])),
    params(("id" = Uuid, Path, description = "Author id"), PageParams),
    responses(
        (status = 200, description = "OPDS acquisition feed of the author's visible books (empty for unknown authors)", content_type = "application/atom+xml;profile=opds-catalog;kind=acquisition", body = String),
        (status = 400, description = "Malformed query parameter", body = crate::openapi::ProblemDetails),
        (status = 401, description = "Basic authentication required (WWW-Authenticate: Basic)", body = crate::openapi::ProblemDetails),
        (status = 422, description = "Malformed cursor", body = crate::openapi::ProblemDetails)
    )
)]
async fn library_author_books(
    BasicOnly(user): BasicOnly,
    State(state): State<AppState>,
    Path(author_id): Path<Uuid>,
    params: Result<Query<PageParams>, QueryRejection>,
) -> Result<Response, AppError> {
    let Query(params) = params?;
    let base = base_url(&state)?.clone();
    let bytes = emit_author_books(
        &state,
        user.user_id,
        &Scope::Library,
        "/opds/library",
        &base,
        author_id,
        params.cursor,
    )
    .await?;
    Ok(atom_response(bytes, FeedKind::Acquisition.content_type()))
}

/// `GET /opds/library/series` — series index, cursor-paginated.
///
/// # Errors
/// - [`AppError::Validation`] on a malformed cursor.
/// - [`AppError::Internal`] on database errors or unconfigured base URL.
#[utoipa::path(
    get,
    path = "/opds/library/series",
    summary = "List series",
    description = "Returns an OPDS navigation feed of series with at least one visible book, cursor-paginated via `cursor`. Requires HTTP Basic authentication (401) and returns 400 for a malformed query parameter or 422 for an invalid cursor.",
    tag = "opds",
    security(("opds_basic" = [])),
    params(PageParams),
    responses(
        (status = 200, description = "OPDS navigation feed of series with visible books", content_type = "application/atom+xml;profile=opds-catalog;kind=navigation", body = String),
        (status = 400, description = "Malformed query parameter", body = crate::openapi::ProblemDetails),
        (status = 401, description = "Basic authentication required (WWW-Authenticate: Basic)", body = crate::openapi::ProblemDetails),
        (status = 422, description = "Malformed cursor", body = crate::openapi::ProblemDetails)
    )
)]
async fn library_series(
    BasicOnly(user): BasicOnly,
    State(state): State<AppState>,
    params: Result<Query<PageParams>, QueryRejection>,
) -> Result<Response, AppError> {
    let Query(params) = params?;
    let base = base_url(&state)?.clone();
    let bytes = emit_series(
        &state,
        user.user_id,
        &Scope::Library,
        "/opds/library",
        &base,
        params.cursor,
    )
    .await?;
    Ok(atom_response(bytes, FeedKind::Navigation.content_type()))
}

/// `GET /opds/library/series/{id}` — one series' books, in series order,
/// cursor-paginated.
///
/// # Errors
/// - [`AppError::Validation`] on a malformed cursor.
/// - [`AppError::Internal`] on database errors or unconfigured base URL.
#[utoipa::path(
    get,
    path = "/opds/library/series/{id}",
    summary = "List a series' books",
    description = "Returns an OPDS acquisition feed of one series' visible books, in series order (empty if the series is unknown). Requires HTTP Basic authentication (401) and returns 400 for a malformed query parameter or 422 for an invalid cursor.",
    tag = "opds",
    security(("opds_basic" = [])),
    params(("id" = Uuid, Path, description = "Series id"), PageParams),
    responses(
        (status = 200, description = "OPDS acquisition feed of the series' visible books (empty for unknown series)", content_type = "application/atom+xml;profile=opds-catalog;kind=acquisition", body = String),
        (status = 400, description = "Malformed query parameter", body = crate::openapi::ProblemDetails),
        (status = 401, description = "Basic authentication required (WWW-Authenticate: Basic)", body = crate::openapi::ProblemDetails),
        (status = 422, description = "Malformed cursor", body = crate::openapi::ProblemDetails)
    )
)]
async fn library_series_books(
    BasicOnly(user): BasicOnly,
    State(state): State<AppState>,
    Path(series_id): Path<Uuid>,
    params: Result<Query<PageParams>, QueryRejection>,
) -> Result<Response, AppError> {
    let Query(params) = params?;
    let base = base_url(&state)?.clone();
    let bytes = emit_series_books(
        &state,
        user.user_id,
        &Scope::Library,
        "/opds/library",
        &base,
        series_id,
        params.cursor,
    )
    .await?;
    Ok(atom_response(bytes, FeedKind::Acquisition.content_type()))
}

/// `OpenSearch` `?q=` query parameter for the search endpoints.
#[derive(Debug, Deserialize, Default, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct SearchParams {
    /// Search term; an empty (or whitespace-only) query short-circuits
    /// to an empty feed without hitting the DB.
    #[serde(default)]
    pub q: String,
}

/// `GET /opds/library/search` — full-text search over visible books.
///
/// # Errors
/// - [`AppError::Internal`] on database errors or unconfigured base URL.
#[utoipa::path(
    get,
    path = "/opds/library/search",
    summary = "Search the library",
    description = "Returns an OPDS acquisition feed of full-text search results over visible books, using the `q` query parameter; an empty or whitespace-only `q` yields an empty feed. Requires HTTP Basic authentication (401) and returns 400 for a malformed query parameter.",
    tag = "opds",
    security(("opds_basic" = [])),
    params(SearchParams),
    responses(
        (status = 200, description = "OPDS acquisition feed of search results; empty/whitespace query yields an empty feed", content_type = "application/atom+xml;profile=opds-catalog;kind=acquisition", body = String),
        (status = 400, description = "Malformed query parameter", body = crate::openapi::ProblemDetails),
        (status = 401, description = "Basic authentication required (WWW-Authenticate: Basic)", body = crate::openapi::ProblemDetails)
    )
)]
async fn library_search(
    BasicOnly(user): BasicOnly,
    State(state): State<AppState>,
    params: Result<Query<SearchParams>, QueryRejection>,
) -> Result<Response, AppError> {
    let Query(params) = params?;
    let base = base_url(&state)?.clone();
    let bytes = emit_search(
        &state,
        user.user_id,
        &Scope::Library,
        "/opds/library",
        &base,
        &params.q,
    )
    .await?;
    Ok(atom_response(bytes, FeedKind::Acquisition.content_type()))
}

// ── Pagination helpers shared by emit_new / emit_author_books ────────────

fn parse_cursor(raw: Option<&str>) -> Result<Option<super::cursor::Cursor>, AppError> {
    raw.map(super::cursor::Cursor::parse)
        .transpose()
        .map_err(|_| AppError::Validation("invalid cursor".into()))
}

fn parse_name_cursor(raw: Option<&str>) -> Result<Option<super::cursor::NameCursor>, AppError> {
    raw.map(super::cursor::NameCursor::parse)
        .transpose()
        .map_err(|e| AppError::Validation(format!("invalid cursor: {e}")))
}

fn push_cursor_predicate(qb: &mut QueryBuilder<Postgres>, cursor: Option<&super::cursor::Cursor>) {
    if let Some(c) = cursor {
        qb.push(" AND (m.created_at, m.id) < (");
        qb.push_bind(c.created_at);
        qb.push(", ");
        qb.push_bind(c.id);
        qb.push(")");
    }
}

fn split_page(rows: &[sqlx::postgres::PgRow], page_size: i64) -> (&[sqlx::postgres::PgRow], bool) {
    // page_size is always a small positive config value (1–100), so these conversions are safe.
    let page_size_usize = usize::try_from(page_size).unwrap_or(usize::MAX);
    let has_more = rows.len() > page_size_usize;
    let page_rows = if has_more {
        &rows[..page_size_usize]
    } else {
        rows
    };
    (page_rows, has_more)
}

// ── Shared feed-emitter helpers (pub(super) so shelves.rs delegates) ──

pub(super) async fn emit_new(
    state: &AppState,
    user_id: Uuid,
    scope: &Scope,
    self_parent: &str,
    base: &Url,
    cursor: Option<String>,
) -> Result<Vec<u8>, AppError> {
    let self_path = format!("{self_parent}/new");
    let page_size = i64::from(state.config.opds.page_size);

    let mut tx = db::acquire_with_rls(&state.pool, user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let cursor = parse_cursor(cursor.as_deref())?;

    let mut qb: QueryBuilder<Postgres> = QueryBuilder::new(
        "SELECT m.id, m.created_at, m.updated_at, m.isbn_13, m.isbn_10, \
                w.id AS work_id, w.title, w.description, w.language \
         FROM manifestations m \
         JOIN works w ON w.id = m.work_id \
         WHERE TRUE",
    );
    if let Scope::Shelf(_) = scope {
        qb.push(" AND ");
        push_scope(&mut qb, scope, "m");
    }
    push_cursor_predicate(&mut qb, cursor.as_ref());
    qb.push(" ORDER BY m.created_at DESC, m.id DESC LIMIT ");
    qb.push_bind(page_size + 1);

    let rows = qb
        .build()
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let (page_rows, has_more) = split_page(&rows, page_size);

    let work_ids: Vec<Uuid> = page_rows
        .iter()
        .map(|r| r.get::<Uuid, _>("work_id"))
        .collect();
    let creators = load_creators(&mut tx, &work_ids).await?;
    let m_ids: Vec<Uuid> = page_rows.iter().map(|r| r.get::<Uuid, _>("id")).collect();
    let tags = load_tags(&mut tx, &m_ids).await?;

    let mut fb = FeedBuilder::new(base, &self_path, FeedKind::Acquisition, "New", Utc::now());
    for r in page_rows {
        let m_id: Uuid = r.get("id");
        let work_id: Uuid = r.get("work_id");
        let updated_at: DateTime<Utc> = r.get("updated_at");
        let title: String = r.get("title");
        let description: Option<String> = r.get("description");
        let language: Option<String> = r.get("language");
        let isbn_13: Option<String> = r.get("isbn_13");
        let isbn_10: Option<String> = r.get("isbn_10");
        fb.add_acquisition_entry(&AcquisitionEntry {
            manifestation_id: m_id,
            work_title: title,
            creators: creators.get(&work_id).cloned().unwrap_or_default(),
            description,
            language,
            tags: tags.get(&m_id).cloned().unwrap_or_default(),
            isbn: isbn_13.or(isbn_10),
            updated_at,
        });
    }

    if has_more {
        #[expect(
            clippy::expect_used,
            reason = "has_more is only true when page_rows is non-empty (split_page guarantees this invariant)"
        )]
        let last = page_rows.last().expect("page non-empty when has_more");
        let last_created: DateTime<Utc> = last.get("created_at");
        let last_id: Uuid = last.get("id");
        let next_cursor = super::cursor::Cursor {
            created_at: last_created,
            id: last_id,
        }
        .encode();
        fb.add_next_link(&format!("{self_path}?cursor={next_cursor}"));
    }

    Ok(fb.finish())
}

pub(super) async fn emit_authors(
    state: &AppState,
    user_id: Uuid,
    scope: &Scope,
    self_parent: &str,
    base: &Url,
    cursor: Option<String>,
) -> Result<Vec<u8>, AppError> {
    let self_path = format!("{self_parent}/authors");
    let page_size = i64::from(state.config.opds.page_size);

    let mut tx = db::acquire_with_rls(&state.pool, user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let cursor = parse_name_cursor(cursor.as_deref())?;

    // authors with at least one visible manifestation under scope.
    // sort_name rides in the SELECT because the page boundary's value
    // feeds the next-page cursor.
    let mut qb: QueryBuilder<Postgres> = QueryBuilder::new(
        "SELECT a.id, a.name, a.sort_name FROM authors a \
         WHERE EXISTS (SELECT 1 FROM work_authors wa \
             JOIN manifestations m ON m.work_id = wa.work_id \
             WHERE wa.author_id = a.id",
    );
    if let Scope::Shelf(_) = scope {
        qb.push(" AND ");
        push_scope(&mut qb, scope, "m");
    }
    qb.push(")");
    if let Some(c) = &cursor {
        qb.push(" AND (a.sort_name, a.id) > (");
        qb.push_bind(c.sort_name.clone());
        qb.push(", ");
        qb.push_bind(c.id);
        qb.push(")");
    }
    qb.push(" ORDER BY a.sort_name ASC, a.id ASC LIMIT ");
    qb.push_bind(page_size + 1);
    let rows = qb
        .build()
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let (page_rows, has_more) = split_page(&rows, page_size);

    let mut fb = FeedBuilder::new(
        base,
        &self_path,
        FeedKind::Navigation,
        "Authors",
        Utc::now(),
    );
    for r in page_rows {
        let id: Uuid = r.get("id");
        let name: String = r.get("name");
        fb.add_navigation_entry(
            &author_urn(id),
            &name,
            &format!("{self_parent}/authors/{id}"),
            true,
        );
    }
    if has_more {
        #[expect(
            clippy::expect_used,
            reason = "has_more is only true when page_rows is non-empty (split_page guarantees this invariant)"
        )]
        let last = page_rows.last().expect("page non-empty when has_more");
        let next = super::cursor::NameCursor {
            sort_name: last.get("sort_name"),
            id: last.get("id"),
        }
        .encode();
        fb.add_next_link(&format!("{self_path}?cursor={next}"));
    }
    Ok(fb.finish())
}

pub(super) async fn emit_author_books(
    state: &AppState,
    user_id: Uuid,
    scope: &Scope,
    self_parent: &str,
    base: &Url,
    author_id: Uuid,
    cursor: Option<String>,
) -> Result<Vec<u8>, AppError> {
    let self_path = format!("{self_parent}/authors/{author_id}");
    let page_size = i64::from(state.config.opds.page_size);

    let mut tx = db::acquire_with_rls(&state.pool, user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let cursor = parse_cursor(cursor.as_deref())?;

    let mut qb: QueryBuilder<Postgres> = QueryBuilder::new(
        "SELECT m.id, m.created_at, m.updated_at, m.isbn_13, m.isbn_10, \
                w.id AS work_id, w.title, w.description, w.language \
         FROM manifestations m \
         JOIN works w ON w.id = m.work_id \
         WHERE w.id IN (SELECT wa.work_id FROM work_authors wa WHERE wa.author_id = ",
    );
    qb.push_bind(author_id);
    qb.push(")");
    if let Scope::Shelf(_) = scope {
        qb.push(" AND ");
        push_scope(&mut qb, scope, "m");
    }
    push_cursor_predicate(&mut qb, cursor.as_ref());
    qb.push(" ORDER BY m.created_at DESC, m.id DESC LIMIT ");
    qb.push_bind(page_size + 1);

    let rows = qb
        .build()
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let (page_rows, has_more) = split_page(&rows, page_size);

    let work_ids: Vec<Uuid> = page_rows
        .iter()
        .map(|r| r.get::<Uuid, _>("work_id"))
        .collect();
    let creators = load_creators(&mut tx, &work_ids).await?;
    let m_ids: Vec<Uuid> = page_rows.iter().map(|r| r.get::<Uuid, _>("id")).collect();
    let tags = load_tags(&mut tx, &m_ids).await?;

    let mut fb = FeedBuilder::new(
        base,
        &self_path,
        FeedKind::Acquisition,
        "Books by author",
        Utc::now(),
    );
    for r in page_rows {
        let m_id: Uuid = r.get("id");
        let work_id: Uuid = r.get("work_id");
        let updated_at: DateTime<Utc> = r.get("updated_at");
        fb.add_acquisition_entry(&AcquisitionEntry {
            manifestation_id: m_id,
            work_title: r.get("title"),
            creators: creators.get(&work_id).cloned().unwrap_or_default(),
            description: r.get("description"),
            language: r.get("language"),
            tags: tags.get(&m_id).cloned().unwrap_or_default(),
            isbn: r
                .get::<Option<String>, _>("isbn_13")
                .or_else(|| r.get("isbn_10")),
            updated_at,
        });
    }
    if has_more {
        #[expect(
            clippy::expect_used,
            reason = "has_more is only true when page_rows is non-empty (split_page guarantees this invariant)"
        )]
        let last = page_rows.last().expect("has_more implies non-empty");
        let row_id: Uuid = last.get("id");
        let next = super::cursor::Cursor {
            created_at: last.get("created_at"),
            id: row_id,
        }
        .encode();
        fb.add_next_link(&format!("{self_path}?cursor={next}"));
    }
    Ok(fb.finish())
}

pub(super) async fn emit_series(
    state: &AppState,
    user_id: Uuid,
    scope: &Scope,
    self_parent: &str,
    base: &Url,
    cursor: Option<String>,
) -> Result<Vec<u8>, AppError> {
    let self_path = format!("{self_parent}/series");
    let page_size = i64::from(state.config.opds.page_size);

    let mut tx = db::acquire_with_rls(&state.pool, user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let cursor = parse_name_cursor(cursor.as_deref())?;

    // sort_name rides in the SELECT because the page boundary's value
    // feeds the next-page cursor.
    let mut qb: QueryBuilder<Postgres> = QueryBuilder::new(
        "SELECT s.id, s.name, s.sort_name FROM series s \
         WHERE EXISTS (SELECT 1 FROM series_works sw \
             JOIN manifestations m ON m.work_id = sw.work_id \
             WHERE sw.series_id = s.id",
    );
    if let Scope::Shelf(_) = scope {
        qb.push(" AND ");
        push_scope(&mut qb, scope, "m");
    }
    qb.push(")");
    if let Some(c) = &cursor {
        qb.push(" AND (s.sort_name, s.id) > (");
        qb.push_bind(c.sort_name.clone());
        qb.push(", ");
        qb.push_bind(c.id);
        qb.push(")");
    }
    qb.push(" ORDER BY s.sort_name ASC, s.id ASC LIMIT ");
    qb.push_bind(page_size + 1);
    let rows = qb
        .build()
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let (page_rows, has_more) = split_page(&rows, page_size);

    let mut fb = FeedBuilder::new(base, &self_path, FeedKind::Navigation, "Series", Utc::now());
    for r in page_rows {
        let id: Uuid = r.get("id");
        let name: String = r.get("name");
        fb.add_navigation_entry(
            &series_urn(id),
            &name,
            &format!("{self_parent}/series/{id}"),
            true,
        );
    }
    if has_more {
        #[expect(
            clippy::expect_used,
            reason = "has_more is only true when page_rows is non-empty (split_page guarantees this invariant)"
        )]
        let last = page_rows.last().expect("page non-empty when has_more");
        let next = super::cursor::NameCursor {
            sort_name: last.get("sort_name"),
            id: last.get("id"),
        }
        .encode();
        fb.add_next_link(&format!("{self_path}?cursor={next}"));
    }
    Ok(fb.finish())
}

pub(super) async fn emit_series_books(
    state: &AppState,
    user_id: Uuid,
    scope: &Scope,
    self_parent: &str,
    base: &Url,
    series_id: Uuid,
    _cursor: Option<String>,
) -> Result<Vec<u8>, AppError> {
    let self_path = format!("{self_parent}/series/{series_id}");
    // Cap at 10× the configured page size — a series of this size is
    // pathological, and a single oversized feed beats silently dropping
    // high-position entries under cursor pagination whose (created_at, id)
    // key doesn't match the series ORDER BY.
    let series_limit = i64::from(state.config.opds.page_size) * 10;

    let mut tx = db::acquire_with_rls(&state.pool, user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let mut qb: QueryBuilder<Postgres> = QueryBuilder::new(
        "SELECT m.id, m.created_at, m.updated_at, m.isbn_13, m.isbn_10, \
                w.id AS work_id, w.title, w.description, w.language \
         FROM manifestations m \
         JOIN works w ON w.id = m.work_id \
         JOIN series_works sw ON sw.work_id = w.id \
         WHERE sw.series_id = ",
    );
    qb.push_bind(series_id);
    if let Scope::Shelf(_) = scope {
        qb.push(" AND ");
        push_scope(&mut qb, scope, "m");
    }
    qb.push(" ORDER BY sw.position ASC NULLS LAST, m.created_at DESC, m.id DESC LIMIT ");
    qb.push_bind(series_limit);

    let rows = qb
        .build()
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let work_ids: Vec<Uuid> = rows.iter().map(|r| r.get::<Uuid, _>("work_id")).collect();
    let creators = load_creators(&mut tx, &work_ids).await?;
    let m_ids: Vec<Uuid> = rows.iter().map(|r| r.get::<Uuid, _>("id")).collect();
    let tags = load_tags(&mut tx, &m_ids).await?;

    let mut fb = FeedBuilder::new(
        base,
        &self_path,
        FeedKind::Acquisition,
        "Series",
        Utc::now(),
    );
    for r in &rows {
        let m_id: Uuid = r.get("id");
        let work_id: Uuid = r.get("work_id");
        let updated_at: DateTime<Utc> = r.get("updated_at");
        fb.add_acquisition_entry(&AcquisitionEntry {
            manifestation_id: m_id,
            work_title: r.get("title"),
            creators: creators.get(&work_id).cloned().unwrap_or_default(),
            description: r.get("description"),
            language: r.get("language"),
            tags: tags.get(&m_id).cloned().unwrap_or_default(),
            isbn: r
                .get::<Option<String>, _>("isbn_13")
                .or_else(|| r.get("isbn_10")),
            updated_at,
        });
    }
    Ok(fb.finish())
}

pub(super) async fn emit_search(
    state: &AppState,
    user_id: Uuid,
    scope: &Scope,
    self_parent: &str,
    base: &Url,
    q: &str,
) -> Result<Vec<u8>, AppError> {
    let self_path = format!("{self_parent}/search");
    let page_size = i64::from(state.config.opds.page_size);

    let mut fb = FeedBuilder::new(
        base,
        &self_path,
        FeedKind::Acquisition,
        "Search results",
        Utc::now(),
    );

    // Empty query → empty feed (Moon+ quirk). Skip DB hit entirely.
    let q_trim = q.trim();
    if q_trim.is_empty() {
        return Ok(fb.finish());
    }

    let mut tx = db::acquire_with_rls(&state.pool, user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let mut qb: QueryBuilder<Postgres> = QueryBuilder::new(
        "SELECT m.id, m.created_at, m.updated_at, m.isbn_13, m.isbn_10, \
                w.id AS work_id, w.title, w.description, w.language \
         FROM manifestations m \
         JOIN works w ON w.id = m.work_id \
         WHERE w.search_vector @@ plainto_tsquery('unaccent_english', ",
    );
    qb.push_bind(q_trim);
    qb.push(")");
    if let Scope::Shelf(_) = scope {
        qb.push(" AND ");
        push_scope(&mut qb, scope, "m");
    }
    qb.push(" ORDER BY ts_rank_cd(w.search_vector, plainto_tsquery('unaccent_english', ");
    qb.push_bind(q_trim);
    qb.push(")) DESC, m.created_at DESC, m.id DESC LIMIT ");
    qb.push_bind(page_size);

    let rows = qb
        .build()
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let work_ids: Vec<Uuid> = rows.iter().map(|r| r.get::<Uuid, _>("work_id")).collect();
    let creators = load_creators(&mut tx, &work_ids).await?;
    let m_ids: Vec<Uuid> = rows.iter().map(|r| r.get::<Uuid, _>("id")).collect();
    let tags = load_tags(&mut tx, &m_ids).await?;

    for r in &rows {
        let m_id: Uuid = r.get("id");
        let work_id: Uuid = r.get("work_id");
        let updated_at: DateTime<Utc> = r.get("updated_at");
        fb.add_acquisition_entry(&AcquisitionEntry {
            manifestation_id: m_id,
            work_title: r.get("title"),
            creators: creators.get(&work_id).cloned().unwrap_or_default(),
            description: r.get("description"),
            language: r.get("language"),
            tags: tags.get(&m_id).cloned().unwrap_or_default(),
            isbn: r
                .get::<Option<String>, _>("isbn_13")
                .or_else(|| r.get("isbn_10")),
            updated_at,
        });
    }

    Ok(fb.finish())
}

// ── Batch loaders ────────────────────────────────────────────────────────

async fn load_creators(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    work_ids: &[Uuid],
) -> Result<HashMap<Uuid, Vec<String>>, AppError> {
    let mut out: HashMap<Uuid, Vec<String>> = HashMap::new();
    if work_ids.is_empty() {
        return Ok(out);
    }
    let rows = sqlx::query!(
        "SELECT wa.work_id, a.name \
         FROM work_authors wa \
         JOIN authors a ON a.id = wa.author_id \
         WHERE wa.work_id = ANY($1::uuid[]) \
         ORDER BY wa.position ASC",
        work_ids,
    )
    .fetch_all(&mut **tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    for r in rows {
        out.entry(r.work_id).or_default().push(r.name);
    }
    Ok(out)
}

async fn load_tags(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    manifestation_ids: &[Uuid],
) -> Result<HashMap<Uuid, Vec<String>>, AppError> {
    let mut out: HashMap<Uuid, Vec<String>> = HashMap::new();
    if manifestation_ids.is_empty() {
        return Ok(out);
    }
    let rows = sqlx::query!(
        "SELECT mt.manifestation_id, t.name \
         FROM manifestation_tags mt \
         JOIN tags t ON t.id = mt.tag_id \
         WHERE mt.manifestation_id = ANY($1::uuid[])",
        manifestation_ids,
    )
    .fetch_all(&mut **tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    for r in rows {
        out.entry(r.manifestation_id).or_default().push(r.name);
    }
    Ok(out)
}
