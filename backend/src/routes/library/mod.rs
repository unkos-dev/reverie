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
//! - Authentication is the cookie-or-Basic
//!   [`crate::auth::middleware::CurrentUser`] extractor (web UI uses
//!   the session cookie; e-reader clients lean on `/opds/*`
//!   instead).
//! - Per-row visibility is delegated to PostgreSQL RLS through
//!   [`crate::db::acquire_with_rls`]; the handler never adds an
//!   ad-hoc `WHERE user_id = …` predicate because doing so would
//!   bypass the unified policy set on `manifestations`.
//! - Cursor tags are verified against the requested `?sort=` axis so
//!   a cursor minted for one sort cannot be replayed against
//!   another (see [`crate::routes::cursor::CursorKey::parse_for`]).

use axum::Router;
use axum::extract::{OriginalUri, Path, State};
use axum::http::{HeaderMap, HeaderValue, header::LINK};
use axum::response::IntoResponse;
use axum::routing::get;
use axum_extra::extract::{Query, QueryRejection};
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, QueryBuilder, Row};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::auth::middleware::CurrentUser;
use crate::db;
use crate::error::AppError;
use crate::models::enrichment_status::EnrichmentStatus;
use crate::models::ingestion_status::IngestionStatus;
use crate::models::library::{
    BookDetail, BookListRow, MetadataVersionRow, MetadataVersionSummary, SeriesRef, WorkDetail,
    WorkManifestation,
};
use crate::models::validation_status::ValidationStatus;
use crate::routes::cursor::{CursorKey, SortMode};
use crate::state::AppState;

mod search;

#[cfg(test)]
mod tests;

/// Build the `/api/books*`, `/api/works/{id}`, and `/api/search`
/// router.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/books", get(list))
        .route("/api/books/{id}", get(detail))
        .route("/api/works/{id}", get(work_detail))
        .route("/api/search", get(search::search))
}

/// Upper bound on `?tag=` repetitions accepted by `GET /api/books`.
/// Practical input is ≤ 10 (UI palette caps at a handful); the limit
/// is set higher than expected use to leave headroom but small enough
/// to bound the COUNT subquery's parameter array. Exceeding the cap
/// returns a 422 `validation` problem rather than executing a
/// pathologically large query.
const MAX_TAG_FILTERS: usize = 20;

/// `?cursor=` / `?sort=` / filter query parameters for `GET /api/books`.
///
/// Decoded via [`axum_extra::extract::Query`] (not built-in
/// `axum::Query`) so multi-value `?tag=a&tag=b` filters extend without
/// a router rewrite. Private to the route module — handler-internal
/// wire shape.
///
/// # Filter semantics (11b)
/// - `author`, `series`, `shelf`: single-value `EXISTS` predicates
///   pushed onto the existing dynamic query.
/// - `tag`: multi-value AND-match — a row qualifies only when its
///   `manifestation_tags` set covers every supplied tag name (see
///   [`push_filter_predicates`]). Capped at [`MAX_TAG_FILTERS`] entries.
/// - `shelf` filter is RLS-aware via the join on `shelves.user_id =
///   current_setting('app.current_user_id', true)::uuid` so a caller
///   cannot probe another user's shelf membership.
#[derive(Debug, Default, Deserialize)]
struct ListParams {
    #[serde(default)]
    cursor: Option<String>,
    #[serde(default)]
    sort: SortMode,
    #[serde(default)]
    author: Option<Uuid>,
    #[serde(default)]
    series: Option<Uuid>,
    #[serde(default)]
    shelf: Option<Uuid>,
    #[serde(default)]
    tag: Vec<String>,
}

/// `GET /api/books` response envelope. Carries the page rows plus
/// the opaque cursor for the following page; pagination is also
/// signalled via the RFC 8288 `Link` header on the response.
///
/// Private to the route module — handler-internal wire shape.
#[derive(Debug, Serialize)]
struct BookListResponse {
    items: Vec<BookListRow>,
    next_cursor: Option<String>,
}

/// List the manifestations visible to the current user, paginated
/// via an opaque cursor and ordered per the requested sort axis.
///
/// # Errors
/// - [`AppError::MalformedQuery`] when a filter param fails to
///   deserialize (e.g. a malformed UUID in `?author=`/`?series=`/
///   `?shelf=`) — HTTP 400 via the `From<QueryRejection>` impl.
/// - [`AppError::Validation`] when the cursor is malformed, the sort
///   tag mismatches the cursor, or `tag.len() > MAX_TAG_FILTERS`.
/// - [`AppError::Internal`] on database errors.
#[allow(
    clippy::too_many_lines,
    reason = "single dynamic-query assembly; splitting hurts readability of the QueryBuilder flow"
)]
async fn list(
    current_user: CurrentUser,
    State(state): State<AppState>,
    params: Result<Query<ListParams>, QueryRejection>,
    OriginalUri(uri): OriginalUri,
) -> Result<impl IntoResponse, AppError> {
    let Query(params) = params?;
    let page_size = i64::from(state.config.opds.page_size);

    if params.tag.len() > MAX_TAG_FILTERS {
        return Err(AppError::Validation(format!(
            "too many tag filters: maximum {MAX_TAG_FILTERS}"
        )));
    }

    let cursor = match params.cursor.as_deref() {
        Some(raw) => Some(
            CursorKey::parse_for(raw, params.sort)
                .map_err(|e| AppError::Validation(format!("invalid cursor: {e}")))?,
        ),
        None => None,
    };

    let mut tx = db::acquire_with_rls(&state.pool, current_user.user_id)
        .await
        .inspect_err(|e| tracing::error!(error = %e, "list: acquire_with_rls failed"))
        .map_err(|e| AppError::Internal(e.into()))?;

    let mut qb: QueryBuilder<'_, Postgres> = QueryBuilder::new(
        "SELECT m.id, m.work_id, m.created_at, m.isbn_13, \
                m.ingestion_status::text AS ingestion_status, \
                m.validation_status, \
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
    push_filter_predicates(&mut qb, &params);
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
            // Fallible decode: this dynamic QueryBuilder path can't use a
            // sqlx macro, and infallible `Row::get` panics on an unknown
            // `validation_status` variant. `try_get` surfaces it as a clean
            // 500 instead — matching the loud-but-handled boundary the
            // typed enum promises (see models::validation_status).
            validation_status: r.try_get("validation_status").map_err(|e| {
                AppError::Internal(anyhow::anyhow!(
                    "unknown validation_status value from DB: {e}"
                ))
            })?,
            enrichment_status: parse_enrichment(&enrichment_raw)?,
            created_at: r.get("created_at"),
        });
    }

    tx.commit()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let next_cursor = match page_rows.last() {
        Some(last) if has_more => {
            let row_id = last.get::<Uuid, _>("id");
            let encoded = next_cursor_for_row(last, params.sort)
                .encode()
                .map_err(|e| {
                    tracing::warn!(error = %e, %row_id, "failed to encode pagination cursor");
                    AppError::Internal(e.into())
                })?;
            Some(encoded)
        }
        _ => None,
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

pub(crate) fn parse_ingestion(raw: &str) -> Result<IngestionStatus, AppError> {
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

pub(crate) fn parse_enrichment(raw: &str) -> Result<EnrichmentStatus, AppError> {
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

/// Push `author` / `series` / `shelf` / `tag` filter predicates onto
/// the dynamic list query (11b). `author`, `series`, and `shelf` are
/// `EXISTS (SELECT 1 …)` subqueries so the row set stays at "one
/// manifestation per row" — the LIMIT and cursor math both assume this
/// invariant.
///
/// `shelf` is hardened against cross-user probes: the inner join on
/// `shelves.user_id = current_setting('app.current_user_id', true)::uuid`
/// rejects shelf ids the caller does not own.
///
/// `tag` AND-matches every supplied name via a scalar correlated
/// subquery `(SELECT COUNT(DISTINCT t.name) …) = N` — not a GROUP BY
/// / HAVING. Empty `tag` list → no predicate. Caller validates
/// `params.tag.len() <= MAX_TAG_FILTERS` before calling, so the
/// `i64::from(u32)` cast on `tag.len()` is provably in range.
fn push_filter_predicates(qb: &mut QueryBuilder<'_, Postgres>, params: &ListParams) {
    if let Some(author_id) = params.author {
        qb.push(
            " AND EXISTS (SELECT 1 FROM work_authors wa \
              WHERE wa.work_id = w.id AND wa.author_id = ",
        );
        qb.push_bind(author_id);
        qb.push(")");
    }
    if let Some(series_id) = params.series {
        qb.push(
            " AND EXISTS (SELECT 1 FROM series_works sw2 \
              WHERE sw2.work_id = w.id AND sw2.series_id = ",
        );
        qb.push_bind(series_id);
        qb.push(")");
    }
    if let Some(shelf_id) = params.shelf {
        qb.push(
            " AND EXISTS (SELECT 1 FROM shelf_items si \
              JOIN shelves s ON s.id = si.shelf_id \
              WHERE si.manifestation_id = m.id \
                AND si.shelf_id = ",
        );
        qb.push_bind(shelf_id);
        qb.push(" AND s.user_id = current_setting('app.current_user_id', true)::uuid)");
    }
    if !params.tag.is_empty() {
        qb.push(
            " AND (SELECT COUNT(DISTINCT t.name) FROM manifestation_tags mt \
              JOIN tags t ON t.id = mt.tag_id \
              WHERE mt.manifestation_id = m.id AND t.name = ANY(",
        );
        qb.push_bind(params.tag.clone());
        qb.push(")) = ");
        // Caller has already validated `params.tag.len() <= MAX_TAG_FILTERS`
        // (a small constant), so the `usize → u32 → i64` step is exact.
        // `u32::try_from` cannot fail here; the fallback is purely a
        // defensive layer and would still emit a sensible (non-matching)
        // predicate rather than 0 rows.
        let tag_count_u32 = u32::try_from(params.tag.len()).unwrap_or(u32::MAX);
        qb.push_bind(i64::from(tag_count_u32));
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
        (
            SortMode::Title,
            CursorKey::Title {
                sort_title,
                work_id,
                manifestation_id,
            },
        ) => {
            // Triple-row tuple comparison. The `m.id` tiebreaker is
            // required because a single work can carry multiple
            // manifestations (epub + pdf of the same edition), and
            // they all share `(sort_title, work_id)`; without
            // `m.id` the page boundary between two such siblings
            // silently drops the second from page N+1.
            qb.push(" AND (w.sort_title, w.id, m.id) > (");
            qb.push_bind(sort_title.clone());
            qb.push(", ");
            qb.push_bind(*work_id);
            qb.push(", ");
            qb.push_bind(*manifestation_id);
            qb.push(")");
        }
        (
            SortMode::Author,
            CursorKey::Author {
                sort_name: Some(sort_name),
                work_id,
                manifestation_id,
            },
        ) => {
            // ORDER BY first_author_sort_name ASC NULLS LAST means the
            // NULL-author bucket sits at the tail. Cursor was minted
            // off a non-NULL boundary row, so the next page must
            // emit: rows whose sort_name compares strictly after
            // `(sort_name, work_id, m.id)` in lexicographic ASC
            // order, PLUS the entire trailing NULL bucket. Postgres
            // three-valued logic would silently drop the NULL bucket
            // from a naive tuple `>` comparison.
            let author_sub = "(SELECT a.sort_name FROM work_authors wa \
                  JOIN authors a ON a.id = wa.author_id \
                  WHERE wa.work_id = w.id \
                  ORDER BY wa.position ASC LIMIT 1)";
            qb.push(" AND (");
            qb.push(author_sub);
            qb.push(" > ");
            qb.push_bind(sort_name.clone());
            qb.push(" OR (");
            qb.push(author_sub);
            qb.push(" = ");
            qb.push_bind(sort_name.clone());
            qb.push(" AND (w.id, m.id) > (");
            qb.push_bind(*work_id);
            qb.push(", ");
            qb.push_bind(*manifestation_id);
            qb.push(")) OR ");
            qb.push(author_sub);
            qb.push(" IS NULL)");
        }
        (
            SortMode::Author,
            CursorKey::Author {
                sort_name: None,
                work_id,
                manifestation_id,
            },
        ) => {
            // Cursor pointed at a NULL-author boundary row; remaining
            // page is the rest of the NULL bucket ordered by
            // `(w.id, m.id)`.
            qb.push(
                " AND (SELECT a.sort_name FROM work_authors wa \
                  JOIN authors a ON a.id = wa.author_id \
                  WHERE wa.work_id = w.id \
                  ORDER BY wa.position ASC LIMIT 1) IS NULL \
                  AND (w.id, m.id) > (",
            );
            qb.push_bind(*work_id);
            qb.push(", ");
            qb.push_bind(*manifestation_id);
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
            // `m.id` alone is unique per row, but pair it with
            // created_at for index-friendly DESC scan.
            qb.push(" ORDER BY m.created_at DESC, m.id DESC");
        }
        SortMode::Title => {
            // `m.id` is the final tiebreaker because a work can have
            // multiple manifestations (epub + pdf) that share
            // `(sort_title, w.id)`.
            qb.push(" ORDER BY w.sort_title ASC, w.id ASC, m.id ASC");
        }
        SortMode::Author => {
            qb.push(" ORDER BY first_author_sort_name ASC NULLS LAST, w.id ASC, m.id ASC");
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
            work_id: row.get::<Uuid, _>("work_id"),
            manifestation_id: row.get::<Uuid, _>("id"),
        },
        SortMode::Author => CursorKey::Author {
            sort_name: row
                .try_get::<Option<String>, _>("first_author_sort_name")
                .ok()
                .flatten(),
            work_id: row.get::<Uuid, _>("work_id"),
            manifestation_id: row.get::<Uuid, _>("id"),
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

pub(crate) async fn load_authors_for_works(
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

/// `GET /api/books/{id}` — single-manifestation detail with the
/// work-level prose, tags, and a metadata-version summary used by the
/// book-detail Versions tab.
///
/// RLS hides manifestations the current user cannot see; the handler
/// reports those as 404 rather than 403 so existence is not leaked.
///
/// # Errors
/// - [`AppError::NotFound`] when the manifestation is missing or
///   RLS-hidden.
/// - [`AppError::Internal`] on database errors.
async fn detail(
    current_user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<axum::Json<BookDetail>, AppError> {
    let mut tx = db::acquire_with_rls(&state.pool, current_user.user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let row = fetch_detail_row(&mut tx, id)
        .await?
        .ok_or(AppError::NotFound)?;

    let work_id = row.work_id;
    let authors = load_authors_for_works(&mut tx, &[work_id])
        .await?
        .remove(&work_id)
        .unwrap_or_default();
    let tags = load_manifestation_tags(&mut tx, id).await?;
    let canonical_ids = canonical_pointer_ids(&row);
    let pending_versions = load_pending_versions(&mut tx, id, &canonical_ids).await?;
    let pending = u32::try_from(pending_versions.len()).unwrap_or(u32::MAX);
    let accepted_count = accepted_pointer_count(&row);

    tx.commit()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let series = match (row.series_id, row.series_name) {
        (Some(sid), Some(name)) => Some(SeriesRef {
            id: sid,
            name,
            position: row.series_position,
        }),
        _ => None,
    };

    let pub_date_str = row
        .pub_date
        .map(|d| {
            d.format(&time::format_description::well_known::Iso8601::DATE)
                .map_err(|e| AppError::Internal(e.into()))
        })
        .transpose()?;
    Ok(axum::Json(BookDetail {
        id: row.id,
        work_id,
        title: row.title,
        authors,
        series,
        description: row.description,
        language: row.language,
        isbn_13: row.isbn_13,
        isbn_10: row.isbn_10,
        publisher: row.publisher,
        pub_date: pub_date_str,
        cover_url: format!("/api/books/{}/cover/thumb", row.id),
        tags,
        ingestion_status: parse_ingestion(&row.ingestion_status)?,
        validation_status: row.validation_status,
        enrichment_status: parse_enrichment(&row.enrichment_status)?,
        metadata_version_summary: MetadataVersionSummary {
            pending,
            accepted: accepted_count,
        },
        metadata_versions: pending_versions,
        created_at: row.created_at,
        updated_at: row.updated_at,
    }))
}

/// Row shape decoded from the [`fetch_detail_row`] query — every
/// column the detail handler needs in one round-trip, including the
/// canonical pointer slots so [`accepted_pointer_count`] is a memory
/// op rather than a follow-up query.
struct DetailRow {
    id: Uuid,
    work_id: Uuid,
    title: String,
    description: Option<String>,
    language: Option<String>,
    isbn_13: Option<String>,
    isbn_10: Option<String>,
    publisher: Option<String>,
    pub_date: Option<time::Date>,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
    ingestion_status: String,
    validation_status: ValidationStatus,
    enrichment_status: String,
    w_title_v: Option<Uuid>,
    w_desc_v: Option<Uuid>,
    w_lang_v: Option<Uuid>,
    m_publisher_v: Option<Uuid>,
    m_pubdate_v: Option<Uuid>,
    m_isbn10_v: Option<Uuid>,
    m_isbn13_v: Option<Uuid>,
    m_cover_v: Option<Uuid>,
    series_id: Option<Uuid>,
    series_name: Option<String>,
    series_position: Option<f64>,
}

/// Single-row fetch for [`detail`]. Joins the work for prose +
/// canonical pointers; RLS on `manifestations` makes the query return
/// `None` when the user cannot see the row, which the caller then
/// flips to [`AppError::NotFound`] (existence-not-leaked).
async fn fetch_detail_row(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    id: Uuid,
) -> Result<Option<DetailRow>, AppError> {
    // Single `LEFT JOIN LATERAL` pulls all three series columns from
    // the same row, so a future tiebreaker tweak can never let
    // `series_name` and `series_position` drift onto different
    // `series_works` rows. Mirrors the list handler's series pattern.
    let row = sqlx::query!(
        r#"
        SELECT m.id                   AS "id!",
               m.work_id              AS "work_id!",
               w.title                AS "title!",
               w.description          AS description,
               w.language             AS language,
               m.isbn_13              AS isbn_13,
               m.isbn_10              AS isbn_10,
               m.publisher            AS publisher,
               m.pub_date             AS pub_date,
               m.created_at           AS "created_at!",
               m.updated_at           AS "updated_at!",
               m.ingestion_status::text  AS "ingestion_status!",
               m.validation_status       AS "validation_status!: ValidationStatus",
               m.enrichment_status::text AS "enrichment_status!",
               w.title_version_id        AS w_title_v,
               w.description_version_id  AS w_desc_v,
               w.language_version_id     AS w_lang_v,
               m.publisher_version_id    AS m_publisher_v,
               m.pub_date_version_id     AS m_pubdate_v,
               m.isbn_10_version_id      AS m_isbn10_v,
               m.isbn_13_version_id      AS m_isbn13_v,
               m.cover_version_id        AS m_cover_v,
               series_one.series_id      AS "series_id?",
               series_one.series_name    AS "series_name?",
               series_one.series_position AS "series_position?: f64"
          FROM manifestations m
          JOIN works w ON w.id = m.work_id
          LEFT JOIN LATERAL (
              SELECT s.id    AS series_id,
                     s.name  AS series_name,
                     sw.position AS series_position
              FROM series_works sw
              JOIN series s ON s.id = sw.series_id
              WHERE sw.work_id = w.id
              ORDER BY sw.position ASC NULLS LAST, s.id ASC
              LIMIT 1
          ) series_one ON TRUE
         WHERE m.id = $1
        "#,
        id,
    )
    .fetch_optional(&mut **tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    Ok(row.map(|r| DetailRow {
        id: r.id,
        work_id: r.work_id,
        title: r.title,
        description: r.description,
        language: r.language,
        isbn_13: r.isbn_13,
        isbn_10: r.isbn_10,
        publisher: r.publisher,
        pub_date: r.pub_date,
        created_at: r.created_at,
        updated_at: r.updated_at,
        ingestion_status: r.ingestion_status,
        validation_status: r.validation_status,
        enrichment_status: r.enrichment_status,
        w_title_v: r.w_title_v,
        w_desc_v: r.w_desc_v,
        w_lang_v: r.w_lang_v,
        m_publisher_v: r.m_publisher_v,
        m_pubdate_v: r.m_pubdate_v,
        m_isbn10_v: r.m_isbn10_v,
        m_isbn13_v: r.m_isbn13_v,
        m_cover_v: r.m_cover_v,
        series_id: r.series_id,
        series_name: r.series_name,
        series_position: r.series_position,
    }))
}

async fn load_manifestation_tags(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    manifestation_id: Uuid,
) -> Result<Vec<String>, AppError> {
    sqlx::query_scalar!(
        "SELECT t.name FROM manifestation_tags mt \
         JOIN tags t ON t.id = mt.tag_id \
         WHERE mt.manifestation_id = $1 \
         ORDER BY t.name ASC",
        manifestation_id,
    )
    .fetch_all(&mut **tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))
}

/// Load every `metadata_versions` row with `status = 'pending'` for the
/// given manifestation, excluding any row that is already the canonical
/// pointer for some field. Ordered `last_seen_at DESC` so the freshest
/// draft is first on the Versions tab.
///
/// The enum was simplified to `pending|rejected` in
/// `20260417000001_add_enrichment_pipeline` — promotion lives on the
/// canonical pointer columns, NOT on a `status = 'accepted'` value, so
/// a promoted row keeps `status = 'pending'`. Without the exclusion
/// filter the Versions tab would surface accepted versions as if they
/// were still draft.
async fn load_pending_versions(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    manifestation_id: Uuid,
    canonical_ids: &[Uuid],
) -> Result<Vec<MetadataVersionRow>, AppError> {
    // `new_value != 'null'::jsonb` excludes audit-trail rows recorded
    // by the manual-clear path (`PATCH /api/books/{id}/metadata` with a
    // `null` value). Those rows live in the journal for accountability
    // but never become a draft an operator could accept — surfacing
    // them here would render `(null)` proposals with Accept/Reject
    // buttons in the Versions tab.
    let rows = sqlx::query!(
        "SELECT id, field_name, source, \
                new_value AS \"new_value!\", \
                status::text AS \"status!\", \
                confidence_score AS \"confidence_score!\", \
                match_type, observation_count \
         FROM metadata_versions \
         WHERE manifestation_id = $1 \
           AND status = 'pending'::metadata_review_status \
           AND new_value != 'null'::jsonb \
           AND NOT (id = ANY($2::uuid[])) \
         ORDER BY last_seen_at DESC",
        manifestation_id,
        canonical_ids,
    )
    .fetch_all(&mut **tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    Ok(rows
        .into_iter()
        .map(|r| MetadataVersionRow {
            id: r.id,
            field_name: r.field_name,
            source: r.source,
            new_value: r.new_value,
            status: r.status,
            confidence_score: r.confidence_score,
            match_type: r.match_type,
            observation_count: r.observation_count,
        })
        .collect())
}

/// Collect the non-NULL canonical pointer ids on the given detail row
/// — the set the pending-count query excludes and `accepted_pointer_count`
/// derives its count from.
fn canonical_pointer_ids(row: &DetailRow) -> Vec<Uuid> {
    [
        row.w_title_v,
        row.w_desc_v,
        row.w_lang_v,
        row.m_publisher_v,
        row.m_pubdate_v,
        row.m_isbn10_v,
        row.m_isbn13_v,
        row.m_cover_v,
    ]
    .into_iter()
    .flatten()
    .collect()
}

/// "Accepted" = canonical pointer slots currently filled for this
/// manifestation + its work. Each non-NULL pointer represents one
/// field whose canonical value has been promoted out of the version
/// journal.
fn accepted_pointer_count(row: &DetailRow) -> u32 {
    let filled = [
        row.w_title_v.is_some(),
        row.w_desc_v.is_some(),
        row.w_lang_v.is_some(),
        row.m_publisher_v.is_some(),
        row.m_pubdate_v.is_some(),
        row.m_isbn10_v.is_some(),
        row.m_isbn13_v.is_some(),
        row.m_cover_v.is_some(),
    ]
    .into_iter()
    .filter(|b| *b)
    .count();
    u32::try_from(filled).unwrap_or(u32::MAX)
}

/// `GET /api/works/{id}` — work-level prose with every manifestation
/// of that work the current user can see.
///
/// `works` carries no RLS policy on its own; RLS lives on
/// `manifestations`. The handler fetches the manifestations first
/// under the RLS transaction and treats an empty result as 404 — the
/// same row set drives both visibility gating and the response body,
/// so an interleaved shelf-revoke or manifestation-delete between
/// statements (`PostgreSQL` `READ COMMITTED` per-statement snapshots)
/// cannot leave the existence-not-leaked invariant in an "EXISTS=true,
/// rows=[]" half-state. Without this, the handler could return 200
/// with `manifestations: []`, leaking the existence of the work row.
///
/// # Errors
/// - [`AppError::NotFound`] when the work is missing or every
///   manifestation is RLS-hidden (or the work row was deleted between
///   the manifestations fetch and the work fetch).
/// - [`AppError::Internal`] on database errors.
async fn work_detail(
    current_user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<axum::Json<WorkDetail>, AppError> {
    let mut tx = db::acquire_with_rls(&state.pool, current_user.user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let manifestation_rows = sqlx::query!(
        r#"
        SELECT id          AS "id!",
               isbn_10,
               isbn_13,
               created_at  AS "created_at!",
               ingestion_status::text  AS "ingestion_status!",
               validation_status       AS "validation_status!: ValidationStatus",
               enrichment_status::text AS "enrichment_status!"
          FROM manifestations
         WHERE work_id = $1
         ORDER BY created_at ASC, id ASC
        "#,
        id,
    )
    .fetch_all(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    if manifestation_rows.is_empty() {
        return Err(AppError::NotFound);
    }

    let work = sqlx::query!(
        "SELECT id   AS \"id!\", \
                title AS \"title!\", \
                description, \
                language \
         FROM works WHERE id = $1",
        id,
    )
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?
    .ok_or(AppError::NotFound)?;

    let series_row = sqlx::query!(
        "SELECT s.id   AS \"id!\", \
                s.name AS \"name!\", \
                sw.position AS \"position: f64\" \
         FROM series_works sw \
         JOIN series s ON s.id = sw.series_id \
         WHERE sw.work_id = $1 \
         ORDER BY sw.position ASC NULLS LAST, s.id ASC \
         LIMIT 1",
        id,
    )
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    let authors = load_authors_for_works(&mut tx, &[id])
        .await?
        .remove(&id)
        .unwrap_or_default();

    tx.commit()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let mut manifestations: Vec<WorkManifestation> = Vec::with_capacity(manifestation_rows.len());
    for r in manifestation_rows {
        manifestations.push(WorkManifestation {
            id: r.id,
            isbn_13: r.isbn_13,
            isbn_10: r.isbn_10,
            cover_url: format!("/api/books/{}/cover/thumb", r.id),
            ingestion_status: parse_ingestion(&r.ingestion_status)?,
            validation_status: r.validation_status,
            enrichment_status: parse_enrichment(&r.enrichment_status)?,
            created_at: r.created_at,
        });
    }

    let series = series_row.map(|s| SeriesRef {
        id: s.id,
        name: s.name,
        position: s.position,
    });

    Ok(axum::Json(WorkDetail {
        id: work.id,
        title: work.title,
        authors,
        description: work.description,
        language: work.language,
        series,
        manifestations,
    }))
}
