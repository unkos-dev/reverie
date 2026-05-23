//! `/api/books*` JSON routes (Step 11a — list/detail/work).
//!
//! Mirrors the read pattern of [`crate::routes::opds::library`] —
//! same RLS seam (`db::acquire_with_rls`), same dynamic
//! [`sqlx::QueryBuilder`] for cursor pagination — but serialises as
//! JSON via the response DTOs in [`crate::models::library`] instead
//! of Atom XML. The list handler signals pagination via an RFC 8288
//! `Link: <next>; rel="next"` header *and* an in-body `next_cursor`
//! field, so JS clients can read either source without parsing the
//! header.
//!
//! # Invariants
//! - Authentication is the cookie-or-Basic [`CurrentUser`] extractor
//!   (web UI uses the session cookie; e-reader clients lean on
//!   `/opds/*` instead).
//! - Per-row visibility is delegated to PostgreSQL RLS through
//!   [`db::acquire_with_rls`]; the handler never adds an
//!   ad-hoc `WHERE user_id = …` predicate because doing so would
//!   bypass the unified policy set on `manifestations`.
//! - Cursor tags are verified against the requested `?sort=` axis so
//!   a cursor minted for one sort cannot be replayed against
//!   another (see [`crate::routes::cursor::CursorKey::parse_for`]).

use axum::Router;
use axum::extract::{OriginalUri, State};
use axum::http::{HeaderMap, HeaderValue, header::LINK};
use axum::response::IntoResponse;
use axum::routing::get;
use axum_extra::extract::Query;
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, QueryBuilder, Row};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::auth::middleware::CurrentUser;
use crate::db;
use crate::error::AppError;
use crate::models::enrichment_status::EnrichmentStatus;
use crate::models::ingestion_status::IngestionStatus;
use crate::models::library::{BookListRow, SeriesRef};
use crate::routes::cursor::{CursorKey, SortMode};
use crate::state::AppState;

#[cfg(test)]
mod tests;

/// Build the `/api/books*` router.
pub fn router() -> Router<AppState> {
    Router::new().route("/api/books", get(list))
}

/// `?cursor=` / `?sort=` query parameters for `GET /api/books`.
///
/// Decoded via [`axum_extra::extract::Query`] (not built-in
/// `axum::Query`) so future multi-value filter params (`?tag=a&tag=b`)
/// can extend this struct without a router-side rewrite.
#[derive(Debug, Default, Deserialize)]
pub struct ListParams {
    /// Opaque base64url cursor from the previous response's
    /// `next_cursor` field (or the `Link: rel="next"` URL).
    #[serde(default)]
    pub cursor: Option<String>,
    /// Sort axis; defaults to [`SortMode::Recent`].
    #[serde(default)]
    pub sort: SortMode,
}

/// `GET /api/books` response envelope. Carries the page rows plus
/// the opaque cursor for the following page; pagination is also
/// signalled via the RFC 8288 `Link` header on the response.
#[derive(Debug, Serialize)]
pub struct BookListResponse {
    /// Page of [`BookListRow`]s ordered per the requested sort.
    pub items: Vec<BookListRow>,
    /// Opaque cursor for the next page; `None` when this is the
    /// final page.
    pub next_cursor: Option<String>,
}

/// List the manifestations visible to the current user, paginated
/// via an opaque cursor and ordered per the requested sort axis.
///
/// # Errors
/// - [`AppError::Validation`] when the cursor is malformed or its
///   tag does not match the requested `sort`.
/// - [`AppError::Internal`] on database errors.
async fn list(
    current_user: CurrentUser,
    State(state): State<AppState>,
    Query(params): Query<ListParams>,
    OriginalUri(uri): OriginalUri,
) -> Result<impl IntoResponse, AppError> {
    let page_size = i64::from(state.config.opds.page_size);

    let cursor = match params.cursor.as_deref() {
        Some(raw) => Some(
            CursorKey::parse_for(raw, params.sort)
                .map_err(|e| AppError::Validation(format!("invalid cursor: {e}")))?,
        ),
        None => None,
    };

    let mut tx = db::acquire_with_rls(&state.pool, current_user.user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let mut qb: QueryBuilder<'_, Postgres> = QueryBuilder::new(
        "SELECT m.id, m.work_id, m.created_at, m.isbn_13, \
                m.ingestion_status::text AS ingestion_status, \
                m.validation_status::text AS validation_status, \
                m.enrichment_status::text AS enrichment_status, \
                w.title, w.sort_title, \
                series_one.series_id AS series_id, \
                series_one.series_name AS series_name, \
                series_one.series_position AS series_position",
    );
    if params.sort == SortMode::Author {
        qb.push(
            ", (SELECT a.sort_name FROM work_authors wa \
                   JOIN authors a ON a.id = wa.author_id \
                   WHERE wa.work_id = w.id \
                   ORDER BY wa.position ASC LIMIT 1) AS first_author_sort_name",
        );
    }
    // LEFT JOIN LATERAL pick-one so a work in multiple series does
    // not multiply manifestation rows — duplicates would break LIMIT
    // + cursor math. Deterministic pick: lowest sw.position (NULLS
    // LAST), then series.id, so the same work surfaces the same
    // series across pages.
    qb.push(
        " FROM manifestations m \
         JOIN works w ON w.id = m.work_id \
         LEFT JOIN LATERAL ( \
             SELECT s.id AS series_id, s.name AS series_name, sw.position AS series_position \
             FROM series_works sw \
             JOIN series s ON s.id = sw.series_id \
             WHERE sw.work_id = w.id \
             ORDER BY sw.position ASC NULLS LAST, s.id ASC \
             LIMIT 1 \
         ) series_one ON TRUE \
         WHERE TRUE",
    );
    push_cursor_predicate(&mut qb, params.sort, cursor.as_ref());
    push_order_by(&mut qb, params.sort);
    qb.push(" LIMIT ");
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
    let authors_by_work = load_authors_for_works(&mut tx, &work_ids).await?;

    let mut items: Vec<BookListRow> = Vec::with_capacity(page_rows.len());
    for r in page_rows {
        let m_id: Uuid = r.get("id");
        let work_id: Uuid = r.get("work_id");
        let series = series_ref_from_row(r);
        let validation_raw: String = r.get("validation_status");
        let ingestion_raw: String = r.get("ingestion_status");
        let enrichment_raw: String = r.get("enrichment_status");
        items.push(BookListRow {
            id: m_id,
            work_id,
            title: r.get("title"),
            authors: authors_by_work.get(&work_id).cloned().unwrap_or_default(),
            series,
            isbn_13: r.get("isbn_13"),
            cover_url: format!("/api/books/{m_id}/cover/thumb"),
            ingestion_status: parse_ingestion(&ingestion_raw)?,
            validation_status: validation_raw,
            enrichment_status: parse_enrichment(&enrichment_raw)?,
            created_at: r.get("created_at"),
        });
    }

    tx.commit()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let next_cursor = if has_more {
        page_rows
            .last()
            .map(|last| next_cursor_for_row(last, params.sort).encode())
    } else {
        None
    };

    let mut headers = HeaderMap::new();
    if let Some(ref nc) = next_cursor {
        let next_url = build_next_url(&uri, nc);
        if let Ok(value) = HeaderValue::from_str(&format!("<{next_url}>; rel=\"next\"")) {
            headers.insert(LINK, value);
        }
    }

    Ok((headers, axum::Json(BookListResponse { items, next_cursor })))
}

fn parse_ingestion(raw: &str) -> Result<IngestionStatus, AppError> {
    match raw {
        "pending" => Ok(IngestionStatus::Pending),
        "processing" => Ok(IngestionStatus::Processing),
        "complete" => Ok(IngestionStatus::Complete),
        "failed" => Ok(IngestionStatus::Failed),
        "skipped" => Ok(IngestionStatus::Skipped),
        other => Err(AppError::Internal(anyhow::anyhow!(
            "unknown ingestion_status value from DB: {other}"
        ))),
    }
}

fn parse_enrichment(raw: &str) -> Result<EnrichmentStatus, AppError> {
    match raw {
        "pending" => Ok(EnrichmentStatus::Pending),
        "in_progress" => Ok(EnrichmentStatus::InProgress),
        "complete" => Ok(EnrichmentStatus::Complete),
        "failed" => Ok(EnrichmentStatus::Failed),
        "skipped" => Ok(EnrichmentStatus::Skipped),
        other => Err(AppError::Internal(anyhow::anyhow!(
            "unknown enrichment_status value from DB: {other}"
        ))),
    }
}

fn series_ref_from_row(r: &sqlx::postgres::PgRow) -> Option<SeriesRef> {
    let id: Option<Uuid> = r.try_get("series_id").ok().flatten();
    let name: Option<String> = r.try_get("series_name").ok().flatten();
    let position: Option<f64> = r.try_get("series_position").ok().flatten();
    match (id, name) {
        (Some(id), Some(name)) => Some(SeriesRef { id, name, position }),
        _ => None,
    }
}

fn push_cursor_predicate(
    qb: &mut QueryBuilder<'_, Postgres>,
    sort: SortMode,
    key: Option<&CursorKey>,
) {
    let Some(key) = key else {
        return;
    };
    match (sort, key) {
        (SortMode::Recent, CursorKey::Recent { created_at, id }) => {
            qb.push(" AND (m.created_at, m.id) < (");
            qb.push_bind(*created_at);
            qb.push(", ");
            qb.push_bind(*id);
            qb.push(")");
        }
        (SortMode::Title, CursorKey::Title { sort_title, id }) => {
            qb.push(" AND (w.sort_title, w.id) > (");
            qb.push_bind(sort_title.clone());
            qb.push(", ");
            qb.push_bind(*id);
            qb.push(")");
        }
        (SortMode::Author, CursorKey::Author { sort_name, id }) => {
            qb.push(
                " AND ((SELECT a.sort_name FROM work_authors wa \
                       JOIN authors a ON a.id = wa.author_id \
                       WHERE wa.work_id = w.id \
                       ORDER BY wa.position ASC LIMIT 1), w.id) > (",
            );
            qb.push_bind(sort_name.clone());
            qb.push(", ");
            qb.push_bind(*id);
            qb.push(")");
        }
        // `parse_for` already rejected cross-sort cursors; this arm is
        // unreachable but kept exhaustive to satisfy the compiler.
        _ => {}
    }
}

fn push_order_by(qb: &mut QueryBuilder<'_, Postgres>, sort: SortMode) {
    match sort {
        SortMode::Recent => {
            qb.push(" ORDER BY m.created_at DESC, m.id DESC");
        }
        SortMode::Title => {
            qb.push(" ORDER BY w.sort_title ASC, w.id ASC");
        }
        SortMode::Author => {
            qb.push(" ORDER BY first_author_sort_name ASC NULLS LAST, w.id ASC");
        }
    }
}

fn split_page(rows: &[sqlx::postgres::PgRow], page_size: i64) -> (&[sqlx::postgres::PgRow], bool) {
    let page_size_usize = usize::try_from(page_size).unwrap_or(usize::MAX);
    let has_more = rows.len() > page_size_usize;
    let page_rows = if has_more {
        &rows[..page_size_usize]
    } else {
        rows
    };
    (page_rows, has_more)
}

fn next_cursor_for_row(row: &sqlx::postgres::PgRow, sort: SortMode) -> CursorKey {
    match sort {
        SortMode::Recent => CursorKey::Recent {
            created_at: row.get::<OffsetDateTime, _>("created_at"),
            id: row.get::<Uuid, _>("id"),
        },
        SortMode::Title => CursorKey::Title {
            sort_title: row.get::<String, _>("sort_title"),
            id: row.get::<Uuid, _>("work_id"),
        },
        SortMode::Author => CursorKey::Author {
            sort_name: row
                .try_get::<Option<String>, _>("first_author_sort_name")
                .ok()
                .flatten()
                .unwrap_or_default(),
            id: row.get::<Uuid, _>("work_id"),
        },
    }
}

fn build_next_url(uri: &axum::http::Uri, next_cursor: &str) -> String {
    let path = uri.path();
    let mut pairs: Vec<(String, String)> = Vec::new();
    let mut saw_cursor = false;
    if let Some(query) = uri.query() {
        for (k, v) in url::form_urlencoded::parse(query.as_bytes()) {
            if k == "cursor" {
                pairs.push((k.into_owned(), next_cursor.to_owned()));
                saw_cursor = true;
            } else {
                pairs.push((k.into_owned(), v.into_owned()));
            }
        }
    }
    if !saw_cursor {
        pairs.push(("cursor".into(), next_cursor.to_owned()));
    }
    let qs: String = url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs(pairs.iter().map(|(k, v)| (k.as_str(), v.as_str())))
        .finish();
    if qs.is_empty() {
        path.to_owned()
    } else {
        format!("{path}?{qs}")
    }
}

async fn load_authors_for_works(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    work_ids: &[Uuid],
) -> Result<std::collections::HashMap<Uuid, Vec<String>>, AppError> {
    let mut out: std::collections::HashMap<Uuid, Vec<String>> = std::collections::HashMap::new();
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
