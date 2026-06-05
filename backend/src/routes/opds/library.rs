//! `/opds/library/*` and shared scope-parameterised handlers for the
//! `new`, `authors`, `authors/:id`, `series`, `series/:id`, `search`
//! subcatalogs. `/opds/shelves/:id/*` delegates to these same handlers via
//! [`super::shelves::router`] with [`Scope::Shelf`].

use std::collections::HashMap;

use axum::Router;
use axum::extract::{Path, Query, State};
use axum::response::Response;
use axum::routing::get;
use serde::Deserialize;
use sqlx::{Postgres, QueryBuilder, Row};
use time::OffsetDateTime;
use url::Url;
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
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/opds/library", get(library_root))
        .route("/opds/library/new", get(library_new))
        .route("/opds/library/authors", get(library_authors))
        .route("/opds/library/authors/{id}", get(library_author_books))
        .route("/opds/library/series", get(library_series))
        .route("/opds/library/series/{id}", get(library_series_books))
        .route("/opds/library/search", get(library_search))
}

// ── Library navigation root ──────────────────────────────────────────────

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
    let mut fb = FeedBuilder::new(
        base,
        self_path,
        FeedKind::Navigation,
        title,
        OffsetDateTime::now_utc(),
    );
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
#[derive(Debug, Deserialize, Default)]
pub struct PageParams {
    /// Opaque cursor returned in a previous response's `rel="next"` link;
    /// `None` returns the first page.
    pub cursor: Option<String>,
}

async fn library_new(
    BasicOnly(user): BasicOnly,
    State(state): State<AppState>,
    Query(params): Query<PageParams>,
) -> Result<Response, AppError> {
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

async fn library_authors(
    BasicOnly(user): BasicOnly,
    State(state): State<AppState>,
    Query(params): Query<PageParams>,
) -> Result<Response, AppError> {
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

async fn library_author_books(
    BasicOnly(user): BasicOnly,
    State(state): State<AppState>,
    Path(author_id): Path<Uuid>,
    Query(params): Query<PageParams>,
) -> Result<Response, AppError> {
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

async fn library_series(
    BasicOnly(user): BasicOnly,
    State(state): State<AppState>,
    Query(params): Query<PageParams>,
) -> Result<Response, AppError> {
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

async fn library_series_books(
    BasicOnly(user): BasicOnly,
    State(state): State<AppState>,
    Path(series_id): Path<Uuid>,
    Query(params): Query<PageParams>,
) -> Result<Response, AppError> {
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
#[derive(Debug, Deserialize, Default)]
pub struct SearchParams {
    /// Search term; an empty (or whitespace-only) query short-circuits
    /// to an empty feed without hitting the DB.
    #[serde(default)]
    pub q: String,
}

async fn library_search(
    BasicOnly(user): BasicOnly,
    State(state): State<AppState>,
    Query(params): Query<SearchParams>,
) -> Result<Response, AppError> {
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

    let mut fb = FeedBuilder::new(
        base,
        &self_path,
        FeedKind::Acquisition,
        "New",
        OffsetDateTime::now_utc(),
    );
    for r in page_rows {
        let m_id: Uuid = r.get("id");
        let work_id: Uuid = r.get("work_id");
        let updated_at: OffsetDateTime = r.get("updated_at");
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
        #[allow(
            clippy::expect_used,
            reason = "has_more is only true when page_rows is non-empty (split_page guarantees this invariant)"
        )]
        let last = page_rows.last().expect("page non-empty when has_more");
        let last_created: OffsetDateTime = last.get("created_at");
        let last_id: Uuid = last.get("id");
        let next_cursor = super::cursor::Cursor {
            created_at: last_created,
            id: last_id,
        }
        .encode()
        .map_err(|e| {
            tracing::warn!(error = %e, row_id = %last_id, "failed to encode OPDS pagination cursor");
            AppError::Internal(e.into())
        })?;
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
    _cursor: Option<String>, // navigation feeds load fully per plan decision
) -> Result<Vec<u8>, AppError> {
    let self_path = format!("{self_parent}/authors");

    let mut tx = db::acquire_with_rls(&state.pool, user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    // authors with at least one visible manifestation under scope.
    let mut qb: QueryBuilder<Postgres> = QueryBuilder::new(
        "SELECT a.id, a.name FROM authors a \
         WHERE EXISTS (SELECT 1 FROM work_authors wa \
             JOIN manifestations m ON m.work_id = wa.work_id \
             WHERE wa.author_id = a.id",
    );
    if let Scope::Shelf(_) = scope {
        qb.push(" AND ");
        push_scope(&mut qb, scope, "m");
    }
    qb.push(") ORDER BY a.sort_name ASC");
    let rows = qb
        .build()
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let mut fb = FeedBuilder::new(
        base,
        &self_path,
        FeedKind::Navigation,
        "Authors",
        OffsetDateTime::now_utc(),
    );
    for r in &rows {
        let id: Uuid = r.get("id");
        let name: String = r.get("name");
        fb.add_navigation_entry(
            &author_urn(id),
            &name,
            &format!("{self_parent}/authors/{id}"),
            true,
        );
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
        OffsetDateTime::now_utc(),
    );
    for r in page_rows {
        let m_id: Uuid = r.get("id");
        let work_id: Uuid = r.get("work_id");
        let updated_at: OffsetDateTime = r.get("updated_at");
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
        #[allow(
            clippy::expect_used,
            reason = "has_more is only true when page_rows is non-empty (split_page guarantees this invariant)"
        )]
        let last = page_rows.last().expect("has_more implies non-empty");
        let row_id: Uuid = last.get("id");
        let next = super::cursor::Cursor {
            created_at: last.get("created_at"),
            id: row_id,
        }
        .encode()
        .map_err(|e| {
            tracing::warn!(error = %e, %row_id, "failed to encode OPDS pagination cursor");
            AppError::Internal(e.into())
        })?;
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
    _cursor: Option<String>,
) -> Result<Vec<u8>, AppError> {
    let self_path = format!("{self_parent}/series");

    let mut tx = db::acquire_with_rls(&state.pool, user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let mut qb: QueryBuilder<Postgres> = QueryBuilder::new(
        "SELECT s.id, s.name FROM series s \
         WHERE EXISTS (SELECT 1 FROM series_works sw \
             JOIN manifestations m ON m.work_id = sw.work_id \
             WHERE sw.series_id = s.id",
    );
    if let Scope::Shelf(_) = scope {
        qb.push(" AND ");
        push_scope(&mut qb, scope, "m");
    }
    qb.push(") ORDER BY s.sort_name ASC");
    let rows = qb
        .build()
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let mut fb = FeedBuilder::new(
        base,
        &self_path,
        FeedKind::Navigation,
        "Series",
        OffsetDateTime::now_utc(),
    );
    for r in &rows {
        let id: Uuid = r.get("id");
        let name: String = r.get("name");
        fb.add_navigation_entry(
            &series_urn(id),
            &name,
            &format!("{self_parent}/series/{id}"),
            true,
        );
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
                w.id AS work_id, w.title, w.description, w.language, sw.position \
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
        OffsetDateTime::now_utc(),
    );
    for r in &rows {
        let m_id: Uuid = r.get("id");
        let work_id: Uuid = r.get("work_id");
        let updated_at: OffsetDateTime = r.get("updated_at");
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
        OffsetDateTime::now_utc(),
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
         WHERE w.search_vector @@ plainto_tsquery('english', ",
    );
    qb.push_bind(q_trim);
    qb.push(")");
    if let Scope::Shelf(_) = scope {
        qb.push(" AND ");
        push_scope(&mut qb, scope, "m");
    }
    qb.push(" ORDER BY ts_rank_cd(w.search_vector, plainto_tsquery('english', ");
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
        let updated_at: OffsetDateTime = r.get("updated_at");
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
