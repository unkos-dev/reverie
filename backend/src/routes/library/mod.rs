//! `/api/v1/books*` JSON routes (Step 11a — list/detail/work).
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
//! - `?sort=` is a validated, ordered stack of whitelisted columns
//!   ([`crate::routes::sort_spec::SortSpec`]), not a single fixed
//!   axis; `push_order_by` and `push_cursor_predicate` iterate its
//!   levels to build the `ORDER BY` and the general keyset cascade.
//!   Cursor tags are verified against the requested stack so a
//!   cursor minted for one sort cannot be replayed against another
//!   (see [`crate::routes::cursor::SortCursor::parse_for`]).

use axum::extract::{OriginalUri, Path, State};
use axum::http::{HeaderMap, HeaderValue, header::LINK};
use axum::response::IntoResponse;
use axum_extra::extract::{Query, QueryRejection};
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, QueryBuilder, Row};
use time::OffsetDateTime;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::auth::middleware::CurrentUser;
use crate::db;
use crate::error::AppError;
use crate::models::content_rating::ContentRating;
use crate::models::enrichment_status::EnrichmentStatus;
use crate::models::ingestion_status::IngestionStatus;
use crate::models::library::{
    BookDetail, BookListRow, MetadataVersionRow, MetadataVersionSummary, SeriesRef, WorkDetail,
    WorkManifestation,
};
use crate::models::reading_state::ReadingStateSummary;
use crate::models::reading_status::ReadingStatus;
use crate::models::validation_status::ValidationStatus;
use crate::routes::cursor::{CursorValue, SortCursor};
use crate::routes::sort_spec::{SortColumn, SortDirection, SortSpec};
use crate::state::AppState;

mod search;

#[cfg(test)]
mod tests;

/// Build the `/api/v1/books*`, `/api/v1/works/{id}`, and `/api/v1/search`
/// router as an [`OpenApiRouter`] so each handler's `#[utoipa::path]`
/// contributes to the generated spec (a missing annotation fails to compile).
/// Merged into `crate::openapi::pilot_router` and split into its runtime and
/// spec halves there. `search` is contributed by its own submodule router so
/// the `routes!` macro resolves the handler in the module that defines it.
pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list))
        .routes(routes!(detail))
        .routes(routes!(work_detail))
        .merge(search::router())
}

/// Upper bound on repetitions of any single vocabulary filter param
/// (`?tag=`, `?genre=`, `?mood=`, and their `_any`/`_none` variants)
/// accepted by `GET /api/v1/books`. Practical input is ≤ 10 (UI
/// palette caps at a handful); the limit is set higher than expected
/// use to leave headroom but small enough to bound each predicate's
/// parameter array. Exceeding the cap returns a 422 `validation`
/// problem rather than executing a pathologically large query.
const MAX_TAG_FILTERS: usize = 20;

/// `?cursor=` / `?sort=` / filter query parameters for `GET /api/v1/books`.
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
/// - `genre`, `mood`: multi-value AND-match over the genre/mood
///   vocabularies, same shape as `tag`.
/// - `tag_any`, `genre_any`, `mood_any`: any-of match (`EXISTS`).
/// - `tag_none`, `genre_none`, `mood_none`: none-of match
///   (`NOT EXISTS`). Every vocabulary param shares the
///   [`MAX_TAG_FILTERS`] cap, enforced per param.
/// - `shelf` filter is RLS-aware via the join on `shelves.user_id =
///   current_setting('app.current_user_id', true)::uuid` so a caller
///   cannot probe another user's shelf membership.
#[derive(Debug, Default, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
struct ListParams {
    #[serde(default)]
    cursor: Option<String>,
    /// Sort stack, JSON:API grammar: comma-separated whitelisted field
    /// names (`title`, `author`, `created_at`, `pages`); a leading `-`
    /// reverses that level to descending; order of appearance sets
    /// priority (the first field is the primary key). An unwhitelisted
    /// field, a repeated column, or more than three levels is a 400.
    /// Omitted, this is `-created_at` (today's "recent" order).
    #[serde(default)]
    #[param(example = "-created_at,title")]
    sort: Option<String>,
    #[serde(default)]
    author: Option<Uuid>,
    #[serde(default)]
    series: Option<Uuid>,
    #[serde(default)]
    shelf: Option<Uuid>,
    #[serde(default)]
    tag: Vec<String>,
    /// All-of genre filter: every listed genre name must be attached.
    #[serde(default)]
    genre: Vec<String>,
    /// Any-of genre filter: at least one listed genre name is attached.
    #[serde(default)]
    genre_any: Vec<String>,
    /// None-of genre filter: no listed genre name is attached.
    #[serde(default)]
    genre_none: Vec<String>,
    /// All-of mood filter: every listed mood name must be attached.
    #[serde(default)]
    mood: Vec<String>,
    /// Any-of mood filter: at least one listed mood name is attached.
    #[serde(default)]
    mood_any: Vec<String>,
    /// None-of mood filter: no listed mood name is attached.
    #[serde(default)]
    mood_none: Vec<String>,
    /// Any-of tag filter: at least one listed tag name is attached.
    #[serde(default)]
    tag_any: Vec<String>,
    /// None-of tag filter: no listed tag name is attached.
    #[serde(default)]
    tag_none: Vec<String>,
}

/// `GET /api/v1/books` response envelope. Carries the page rows plus
/// the opaque cursor for the following page; pagination is also
/// signalled via the RFC 8288 `Link` header on the response.
///
/// Private to the route module — handler-internal wire shape.
#[derive(Debug, Serialize, utoipa::ToSchema)]
struct BookListResponse {
    items: Vec<BookListRow>,
    next_cursor: Option<String>,
}

/// List the manifestations visible to the current user, paginated
/// via an opaque cursor and ordered per the requested sort stack.
///
/// # Errors
/// - [`AppError::MalformedQuery`] when a filter param fails to
///   deserialize (a malformed UUID in `?author=`/`?series=`/`?shelf=`,
///   via the `From<QueryRejection>` impl), or when `?sort=` names an
///   unwhitelisted field, repeats a column, or exceeds
///   [`crate::routes::sort_spec::MAX_SORT_LEVELS`] levels.
/// - [`AppError::Validation`] when the cursor is malformed, its
///   embedded sort spec mismatches the requested stack, or any
///   vocabulary filter param carries more than [`MAX_TAG_FILTERS`]
///   values.
/// - [`AppError::Internal`] on database errors.
#[expect(
    clippy::too_many_lines,
    reason = "single dynamic-query assembly; splitting hurts readability of the QueryBuilder flow"
)]
#[utoipa::path(
    get,
    path = "/api/v1/books",
    tag = "library",
    params(ListParams),
    security(("session_cookie" = ["read"]), ("device_token_bearer" = ["read"]), ("oidc_jwt_bearer" = ["read"]), ("opds_basic" = ["read"])),
    responses(
        (status = 200, description = "Paginated list of visible books", body = BookListResponse,
            headers(("Link" = String, description = "RFC 8288 next-page link; emitted with rel=\"next\" when more rows remain"))),
        (status = 400, description = "Malformed query parameter", body = crate::openapi::ProblemDetails),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails),
        (status = 422, description = "Invalid cursor or too many filter values", body = crate::openapi::ProblemDetails)
    )
)]
async fn list(
    current_user: CurrentUser,
    State(state): State<AppState>,
    params: Result<Query<ListParams>, QueryRejection>,
    OriginalUri(uri): OriginalUri,
) -> Result<impl IntoResponse, AppError> {
    let Query(params) = params?;
    let page_size = i64::from(state.config.opds.page_size);

    for (param, values) in [
        ("tag", &params.tag),
        ("tag_any", &params.tag_any),
        ("tag_none", &params.tag_none),
        ("genre", &params.genre),
        ("genre_any", &params.genre_any),
        ("genre_none", &params.genre_none),
        ("mood", &params.mood),
        ("mood_any", &params.mood_any),
        ("mood_none", &params.mood_none),
    ] {
        if values.len() > MAX_TAG_FILTERS {
            return Err(AppError::Validation(format!(
                "too many {param} filters: maximum {MAX_TAG_FILTERS}"
            )));
        }
    }

    let spec = params
        .sort
        .as_deref()
        .map(SortSpec::parse)
        .transpose()
        .map_err(|e| AppError::MalformedQuery(format!("invalid sort: {e}")))?
        .unwrap_or_default();

    let cursor = match params.cursor.as_deref() {
        Some(raw) => Some(
            SortCursor::parse_for(raw, &spec)
                .map_err(|e| AppError::Validation(format!("invalid cursor: {e}")))?,
        ),
        None => None,
    };

    let mut tx = db::acquire_with_rls(&state.pool, current_user.user_id)
        .await
        .inspect_err(|e| tracing::error!(error = %e, "list: acquire_with_rls failed"))
        .map_err(|e| AppError::Internal(e.into()))?;

    let mut qb: QueryBuilder<Postgres> = QueryBuilder::new(
        "SELECT m.id, m.work_id, m.created_at, m.isbn_13, m.pages, \
                m.ingestion_status::text AS ingestion_status, \
                m.validation_status, \
                m.enrichment_status::text AS enrichment_status, \
                w.title, w.subtitle, w.sort_title, \
                w.first_author_sort_name, \
                series_one.series_id AS series_id, \
                series_one.series_name AS series_name, \
                series_one.series_position AS series_position",
    );
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
    push_cursor_predicate(&mut qb, &spec, cursor.as_ref());
    push_order_by(&mut qb, &spec);
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
    let manifestation_ids: Vec<Uuid> = page_rows.iter().map(|r| r.get::<Uuid, _>("id")).collect();
    let reading_by_manifestation =
        load_reading_state_for_manifestations(&mut tx, &manifestation_ids).await?;

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
            subtitle: r.get("subtitle"),
            authors: authors_by_work.get(&work_id).cloned().unwrap_or_default(),
            series,
            isbn_13: r.get("isbn_13"),
            pages: r.get("pages"),
            cover_url: format!("/api/v1/books/{m_id}/cover/thumb"),
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
            reading_state: reading_by_manifestation.get(&m_id).cloned(),
            created_at: r.get("created_at"),
        });
    }

    tx.commit()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let next_cursor = match page_rows.last() {
        Some(last) if has_more => {
            let row_id = last.get::<Uuid, _>("id");
            let encoded = next_cursor_for_row(last, &spec)?.encode().map_err(|e| {
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
/// The vocabulary filters (`tag`/`genre`/`mood` plus their `_any` and
/// `_none` variants) are delegated to [`push_vocab_predicates`], one
/// call per junction+vocabulary pair. Empty lists push no predicate.
/// Caller validates every list against [`MAX_TAG_FILTERS`] before
/// calling, so the `i64::from(u32)` cast on the all-of count is
/// provably in range.
fn push_filter_predicates(qb: &mut QueryBuilder<Postgres>, params: &ListParams) {
    if let Some(author_id) = params.author {
        // Role-scoped to match the response surface: `authors[]` carries the
        // author role only, so `?author=` must not match a work where the
        // supplied id is linked as editor/translator.
        qb.push(
            " AND EXISTS (SELECT 1 FROM work_authors wa \
              WHERE wa.work_id = w.id AND wa.role = 'author' AND wa.author_id = ",
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
    push_vocab_predicates(
        qb,
        "COUNT(DISTINCT lower(t.name))",
        "FROM manifestation_tags mt \
          JOIN tags t ON t.id = mt.tag_id \
          WHERE mt.manifestation_id = m.id AND lower(t.name) = ANY(",
        &params.tag,
        &params.tag_any,
        &params.tag_none,
    );
    push_vocab_predicates(
        qb,
        "COUNT(DISTINCT lower(g.name))",
        "FROM manifestation_genres mg \
          JOIN genres g ON g.id = mg.genre_id \
          WHERE mg.manifestation_id = m.id AND lower(g.name) = ANY(",
        &params.genre,
        &params.genre_any,
        &params.genre_none,
    );
    push_vocab_predicates(
        qb,
        "COUNT(DISTINCT lower(md.name))",
        "FROM manifestation_moods mm \
          JOIN moods md ON md.id = mm.mood_id \
          WHERE mm.manifestation_id = m.id AND lower(md.name) = ANY(",
        &params.mood,
        &params.mood_any,
        &params.mood_none,
    );
}

/// Push up to three vocabulary predicates for one junction+vocabulary
/// pair onto the dynamic list query. `count_expr` and `core` are
/// static SQL fragments (never user input); user-supplied names travel
/// exclusively through `push_bind`.
///
/// Names match case-insensitively: vocabulary identity is `lower(name)`
/// (the tables are unique on it and suggest matches via `ILIKE`), so the
/// fragments compare `lower(name)` and the bound values are lowercased
/// here to mirror that.
///
/// - all-of: scalar correlated subquery
///   `(SELECT COUNT(DISTINCT lower(name)) ...) = N`, not a GROUP BY/HAVING.
/// - any-of: `EXISTS (SELECT 1 ...)`.
/// - none-of: `NOT EXISTS (SELECT 1 ...)`.
fn push_vocab_predicates(
    qb: &mut QueryBuilder<Postgres>,
    count_expr: &str,
    core: &str,
    all_of: &[String],
    any_of: &[String],
    none_of: &[String],
) {
    let lowered =
        |names: &[String]| -> Vec<String> { names.iter().map(|n| n.to_lowercase()).collect() };
    if !all_of.is_empty() {
        // Dedupe before binding: COUNT(DISTINCT ..) compares against the
        // requested count, so a repeated name (`?genre=X&genre=X`, or the
        // same name in two casings) would make the predicate
        // unsatisfiable (1 = 2) and silently return zero rows.
        let mut all_of = lowered(all_of);
        all_of.sort_unstable();
        all_of.dedup();
        // Caller has already validated `all_of.len() <= MAX_TAG_FILTERS`
        // (a small constant), so the `usize → u32 → i64` step is exact.
        // `u32::try_from` cannot fail here; the fallback is purely a
        // defensive layer and would still emit a sensible (non-matching)
        // predicate rather than 0 rows.
        let count = i64::from(u32::try_from(all_of.len()).unwrap_or(u32::MAX));
        qb.push(" AND (SELECT ");
        qb.push(count_expr);
        qb.push(" ");
        qb.push(core);
        qb.push_bind(all_of);
        qb.push(")) = ");
        qb.push_bind(count);
    }
    if !any_of.is_empty() {
        qb.push(" AND EXISTS (SELECT 1 ");
        qb.push(core);
        qb.push_bind(lowered(any_of));
        qb.push("))");
    }
    if !none_of.is_empty() {
        qb.push(" AND NOT EXISTS (SELECT 1 ");
        qb.push(core);
        qb.push_bind(lowered(none_of));
        qb.push("))");
    }
}

/// Push the generalized keyset "advance past the cursor" predicate for
/// `spec`. For levels `L1..Lk` plus the implicit `m.id` tiebreaker this
/// is `AND ( T1 OR T2 OR ... OR Tk OR Tid )`, where `Ti` requires exact
/// equality on every level before `i` plus a strict advance on level
/// `i`, and `Tid` requires exact equality on every level plus a strict
/// `m.id` advance. This is the single-level author arm generalized to N
/// levels: a bare tuple comparison is wrong once any level is nullable
/// or levels mix `ASC`/`DESC`, so each level gets its own OR-branch
/// instead of a `(a, b, c) > (x, y, z)` row-value comparison.
fn push_cursor_predicate(
    qb: &mut QueryBuilder<Postgres>,
    spec: &SortSpec,
    cursor: Option<&SortCursor>,
) {
    let Some(cursor) = cursor else {
        return;
    };
    let levels = spec.levels();
    let tiebreak_dir = levels
        .last()
        .map_or(SortDirection::Desc, |level| level.direction);

    qb.push(" AND (");
    let mut wrote_branch = false;
    for (i, level) in levels.iter().enumerate() {
        let key = &cursor.keys[i];
        // A nullable column whose own boundary is NULL has nothing
        // "after" it within this level: NULLS LAST puts every NULL row
        // at the tail regardless of direction, so a deeper level or the
        // id tiebreaker below is what continues the walk.
        if level.column.nullable() && is_null_boundary(key) {
            continue;
        }
        qb.push(if wrote_branch { " OR (" } else { "(" });
        for (prior_level, prior_key) in levels[..i].iter().zip(&cursor.keys[..i]) {
            push_boundary_equality(qb, prior_level.column, prior_key);
            qb.push(" AND ");
        }
        push_boundary_advance(qb, level.column, level.direction, key);
        qb.push(")");
        wrote_branch = true;
    }

    // Every level ties the boundary exactly; `m.id` (unique per row)
    // breaks it. Direction follows the last explicit level so a
    // single-level stack reproduces the pre-stack tuple-comparison
    // arms exactly.
    qb.push(if wrote_branch { " OR (" } else { "(" });
    for (level, key) in levels.iter().zip(&cursor.keys) {
        push_boundary_equality(qb, level.column, key);
        qb.push(" AND ");
    }
    qb.push("m.id ");
    qb.push(tiebreak_dir.comparison_op());
    qb.push(" ");
    qb.push_bind(cursor.id);
    qb.push(")");

    qb.push(")");
}

/// Whether a cascade boundary value sits in a nullable column's NULLS
/// LAST bucket. Only [`CursorValue::Text`] and [`CursorValue::Int`] can
/// carry `None`; [`SortCursor::parse_for`] already rejected any other
/// arity or value-type mismatch against the spec.
const fn is_null_boundary(value: &CursorValue) -> bool {
    matches!(value, CursorValue::Text(None) | CursorValue::Int(None))
}

/// Push the correctly-typed `push_bind` for a non-null `CursorValue`. The
/// single place a `CursorValue` variant is matched down to a concrete
/// bindable type, shared by [`push_boundary_equality`] and
/// [`push_boundary_advance`] so neither re-enumerates the four variants.
/// Both callers guard the null-bucket case themselves (`IS NULL` in
/// [`push_boundary_equality`]; an early return in [`push_boundary_advance`]),
/// so the null arm here is a no-op kept only for exhaustiveness.
fn push_typed_bind(qb: &mut QueryBuilder<Postgres>, value: &CursorValue) {
    match value {
        CursorValue::Text(None) | CursorValue::Int(None) => {}
        CursorValue::Text(Some(v)) => {
            qb.push_bind(v.clone());
        }
        CursorValue::Int(Some(v)) => {
            qb.push_bind(*v);
        }
        CursorValue::Ts(ts) => {
            qb.push_bind(*ts);
        }
    }
}

/// Push an equality term against `column`'s boundary `value`: `IS NULL`
/// when the boundary itself is NULL, `= $bind` otherwise.
fn push_boundary_equality(
    qb: &mut QueryBuilder<Postgres>,
    column: SortColumn,
    value: &CursorValue,
) {
    qb.push(column.sql_expr());
    if is_null_boundary(value) {
        qb.push(" IS NULL");
    } else {
        qb.push(" = ");
        push_typed_bind(qb, value);
    }
}

/// Push the "strictly past this level's boundary" clause. A nullable
/// column with a non-NULL boundary wraps in `(expr OP $bind OR expr IS
/// NULL)` in BOTH directions: NULLS LAST means the NULL bucket sits
/// after every non-NULL value regardless of direction, so a bare `<`
/// (DESC) would silently drop the entire NULL bucket the first time a
/// non-NULL boundary is used, the same bug class a bare `>` (ASC) would
/// have without the `OR IS NULL` arm. A NULL boundary returns early here;
/// the caller already skips this level's branch (see [`is_null_boundary`]),
/// so this is defense in depth.
fn push_boundary_advance(
    qb: &mut QueryBuilder<Postgres>,
    column: SortColumn,
    direction: SortDirection,
    value: &CursorValue,
) {
    if is_null_boundary(value) {
        return;
    }
    let expr = column.sql_expr();
    let nullable = column.nullable();
    if nullable {
        qb.push("(");
    }
    qb.push(expr);
    qb.push(" ");
    qb.push(direction.comparison_op());
    qb.push(" ");
    push_typed_bind(qb, value);
    if nullable {
        qb.push(" OR ");
        qb.push(expr);
        qb.push(" IS NULL)");
    }
}

/// Push the `ORDER BY` clause for `spec`: each level's column and
/// direction, `NULLS LAST` when the column is nullable, and a final
/// `m.id` tiebreaker in the last level's direction so equal-key rows
/// still resolve to a total order (required for keyset pagination).
fn push_order_by(qb: &mut QueryBuilder<Postgres>, spec: &SortSpec) {
    qb.push(" ORDER BY ");
    let mut tiebreak_dir = SortDirection::Desc;
    for (i, level) in spec.levels().iter().enumerate() {
        if i > 0 {
            qb.push(", ");
        }
        qb.push(level.column.sql_expr());
        qb.push(" ");
        qb.push(level.direction.sql());
        if level.column.nullable() {
            qb.push(" NULLS LAST");
        }
        tiebreak_dir = level.direction;
    }
    qb.push(", m.id ");
    qb.push(tiebreak_dir.sql());
}

pub(crate) fn split_page(
    rows: &[sqlx::postgres::PgRow],
    page_size: i64,
) -> (&[sqlx::postgres::PgRow], bool) {
    let page_size_usize = usize::try_from(page_size).unwrap_or(usize::MAX);
    let has_more = rows.len() > page_size_usize;
    let page_rows = if has_more {
        &rows[..page_size_usize]
    } else {
        rows
    };
    (page_rows, has_more)
}

/// Build the `next_cursor` boundary for `row` under `spec`: one typed
/// value per level (read from `select_alias()`) plus the `m.id`
/// tiebreaker. Uses `try_get`, not the panicking `Row::get`; this
/// dynamic `QueryBuilder` path has no compile-time schema check, so a
/// decode failure becomes a clean 500 instead of a panic (mirrors the
/// `validation_status` decode above).
fn next_cursor_for_row(
    row: &sqlx::postgres::PgRow,
    spec: &SortSpec,
) -> Result<SortCursor, AppError> {
    let mut keys = Vec::with_capacity(spec.levels().len());
    for level in spec.levels() {
        let alias = level.column.select_alias();
        let value = match level.column {
            SortColumn::Title => CursorValue::Text(Some(decode_cursor_column(row, alias)?)),
            SortColumn::Author => CursorValue::Text(decode_cursor_column(row, alias)?),
            SortColumn::CreatedAt => CursorValue::Ts(decode_cursor_column(row, alias)?),
            SortColumn::Pages => CursorValue::Int(decode_cursor_column(row, alias)?),
        };
        keys.push(value);
    }
    let id = decode_cursor_column(row, "id")?;
    SortCursor::for_spec(spec, keys, id)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("minted an invalid cursor: {e}")))
}

/// `try_get` wrapper shared by [`next_cursor_for_row`]'s per-column
/// reads: maps a decode failure to [`AppError::Internal`] instead of
/// panicking or silently defaulting.
fn decode_cursor_column<'r, T>(row: &'r sqlx::postgres::PgRow, alias: &str) -> Result<T, AppError>
where
    T: sqlx::Decode<'r, Postgres> + sqlx::Type<Postgres>,
{
    row.try_get(alias).map_err(|e| {
        AppError::Internal(anyhow::anyhow!("failed to decode {alias} for cursor: {e}"))
    })
}

/// Rewrite `uri`'s query string for the `Link: rel="next"` header and
/// the body's `next_cursor` echo, replacing (or appending) `cursor` and
/// passing every other param through unchanged. Shared with the
/// shelves pagination surface, so this stays sort-agnostic; a `sort`
/// param survives the round trip unmodified because
/// [`crate::routes::sort_spec::SortSpec::canonical`] reproduces
/// whatever raw string parsed successfully (its own round-trip
/// invariant), and an absent `sort` is never added here, so the
/// default stack keeps producing clean `?cursor=`-only URLs.
pub(crate) fn build_next_url(uri: &axum::http::Uri, next_cursor: &str) -> String {
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
    // Editors/translators/narrators never substitute for an author in
    // display — this list feeds authors[] on the list/detail responses.
    let rows = sqlx::query!(
        "SELECT wa.work_id, a.name \
         FROM work_authors wa \
         JOIN authors a ON a.id = wa.author_id \
         WHERE wa.work_id = ANY($1::uuid[]) AND wa.role = 'author' \
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

pub(crate) async fn load_reading_state_for_manifestations(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    manifestation_ids: &[Uuid],
) -> Result<std::collections::HashMap<Uuid, ReadingStateSummary>, AppError> {
    let mut out: std::collections::HashMap<Uuid, ReadingStateSummary> =
        std::collections::HashMap::new();
    if manifestation_ids.is_empty() {
        return Ok(out);
    }
    // Same RLS-scoped transaction as the page query: no `WHERE user_id = ...`
    // predicate here (module invariant, see the file doc comment).
    // `reading_state`'s RLS policy already confines this SELECT to the
    // caller's own rows.
    let rows = sqlx::query!(
        r#"SELECT manifestation_id, status AS "status?: ReadingStatus", rating, progress_pct
             FROM reading_state WHERE manifestation_id = ANY($1::uuid[])"#,
        manifestation_ids,
    )
    .fetch_all(&mut **tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    for r in rows {
        out.insert(
            r.manifestation_id,
            ReadingStateSummary {
                status: r.status,
                rating: r.rating,
                progress_pct: r.progress_pct,
            },
        );
    }
    Ok(out)
}

/// `GET /api/v1/books/{id}` — single-manifestation detail with the
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
#[utoipa::path(
    get,
    path = "/api/v1/books/{id}",
    tag = "library",
    params(("id" = Uuid, Path, description = "Manifestation id")),
    security(("session_cookie" = ["read"]), ("device_token_bearer" = ["read"]), ("oidc_jwt_bearer" = ["read"]), ("opds_basic" = ["read"])),
    responses(
        (status = 200, description = "Book detail", body = BookDetail),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails),
        (status = 404, description = "Not found or RLS-hidden", body = crate::openapi::ProblemDetails)
    )
)]
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
    let genres = load_manifestation_genres(&mut tx, id).await?;
    let moods = load_manifestation_moods(&mut tx, id).await?;
    let mut canonical_ids = canonical_pointer_ids(&row);
    let junction_ids = junction_pointer_ids(&mut tx, id, work_id).await?;
    // Junction-held pointers are applied versions too: without them the
    // summary would report an accepted count that drops every vocabulary
    // and contributor edit.
    let accepted_count = accepted_pointer_count(&row)
        .saturating_add(u32::try_from(junction_ids.len()).unwrap_or(u32::MAX));
    canonical_ids.extend(junction_ids);
    let pending_versions = load_pending_versions(&mut tx, id, &canonical_ids).await?;
    let pending = u32::try_from(pending_versions.len()).unwrap_or(u32::MAX);

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
        subtitle: row.subtitle,
        authors,
        series,
        description: row.description,
        language: row.language,
        isbn_13: row.isbn_13,
        isbn_10: row.isbn_10,
        pages: row.pages,
        publisher: row.publisher,
        pub_date: pub_date_str,
        cover_url: format!("/api/v1/books/{}/cover/thumb", row.id),
        tags,
        genres,
        moods,
        content_rating: row.content_rating,
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
    subtitle: Option<String>,
    description: Option<String>,
    language: Option<String>,
    isbn_13: Option<String>,
    isbn_10: Option<String>,
    publisher: Option<String>,
    pub_date: Option<time::Date>,
    pages: Option<i32>,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
    ingestion_status: String,
    validation_status: ValidationStatus,
    enrichment_status: String,
    content_rating: Option<ContentRating>,
    w_title_v: Option<Uuid>,
    w_subtitle_v: Option<Uuid>,
    w_desc_v: Option<Uuid>,
    w_lang_v: Option<Uuid>,
    m_publisher_v: Option<Uuid>,
    m_pubdate_v: Option<Uuid>,
    m_isbn10_v: Option<Uuid>,
    m_isbn13_v: Option<Uuid>,
    m_cover_v: Option<Uuid>,
    m_pages_v: Option<Uuid>,
    m_content_rating_v: Option<Uuid>,
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
               w.subtitle             AS subtitle,
               w.description          AS description,
               w.language             AS language,
               m.isbn_13              AS isbn_13,
               m.isbn_10              AS isbn_10,
               m.publisher            AS publisher,
               m.pub_date             AS pub_date,
               m.pages                AS pages,
               m.created_at           AS "created_at!",
               m.updated_at           AS "updated_at!",
               m.ingestion_status::text  AS "ingestion_status!",
               m.validation_status       AS "validation_status!: ValidationStatus",
               m.enrichment_status::text AS "enrichment_status!",
               m.content_rating          AS "content_rating: ContentRating",
               w.title_version_id        AS w_title_v,
               w.subtitle_version_id     AS w_subtitle_v,
               w.description_version_id  AS w_desc_v,
               w.language_version_id     AS w_lang_v,
               m.publisher_version_id    AS m_publisher_v,
               m.pub_date_version_id     AS m_pubdate_v,
               m.isbn_10_version_id      AS m_isbn10_v,
               m.isbn_13_version_id      AS m_isbn13_v,
               m.cover_version_id        AS m_cover_v,
               m.pages_version_id        AS m_pages_v,
               m.content_rating_version_id AS m_content_rating_v,
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
        subtitle: r.subtitle,
        description: r.description,
        language: r.language,
        isbn_13: r.isbn_13,
        isbn_10: r.isbn_10,
        publisher: r.publisher,
        pub_date: r.pub_date,
        pages: r.pages,
        created_at: r.created_at,
        updated_at: r.updated_at,
        ingestion_status: r.ingestion_status,
        validation_status: r.validation_status,
        enrichment_status: r.enrichment_status,
        content_rating: r.content_rating,
        w_title_v: r.w_title_v,
        w_subtitle_v: r.w_subtitle_v,
        w_desc_v: r.w_desc_v,
        w_lang_v: r.w_lang_v,
        m_publisher_v: r.m_publisher_v,
        m_pubdate_v: r.m_pubdate_v,
        m_isbn10_v: r.m_isbn10_v,
        m_isbn13_v: r.m_isbn13_v,
        m_cover_v: r.m_cover_v,
        m_pages_v: r.m_pages_v,
        m_content_rating_v: r.m_content_rating_v,
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

async fn load_manifestation_genres(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    manifestation_id: Uuid,
) -> Result<Vec<String>, AppError> {
    sqlx::query_scalar!(
        "SELECT g.name FROM manifestation_genres mg \
         JOIN genres g ON g.id = mg.genre_id \
         WHERE mg.manifestation_id = $1 \
         ORDER BY g.name ASC",
        manifestation_id,
    )
    .fetch_all(&mut **tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))
}

async fn load_manifestation_moods(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    manifestation_id: Uuid,
) -> Result<Vec<String>, AppError> {
    sqlx::query_scalar!(
        "SELECT md.name FROM manifestation_moods mm \
         JOIN moods md ON md.id = mm.mood_id \
         WHERE mm.manifestation_id = $1 \
         ORDER BY md.name ASC",
        manifestation_id,
    )
    .fetch_all(&mut **tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))
}

/// Upper bound on pending versions loaded for one manifestation's
/// Versions tab. Normal operation is single digits, but a bulk
/// enrichment run or repeated manual edits can accumulate hundreds of
/// pending rows for one book; the cap bounds the result set (and the
/// derived `pending` count) so a single book accumulating many pending
/// rows can't degrade the request into an unbounded scan. Access is
/// authenticated and RLS-scoped to one manifestation, so this guards
/// resource exhaustion, not an arbitrary-input denial-of-service
/// vector. `last_seen_at DESC` ordering means the freshest drafts
/// survive the cut.
const MAX_PENDING_VERSIONS: i64 = 200;

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
    // by the manual-clear path (`PATCH /api/v1/books/{id}/metadata` with a
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
         ORDER BY last_seen_at DESC \
         LIMIT $3",
        manifestation_id,
        canonical_ids,
        MAX_PENDING_VERSIONS,
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

/// Collect the journal pointers held on junction rows (vocabularies and
/// contributors) for the manifestation. Junction-backed fields carry their
/// canonical pointer per row instead of on a scalar `*_version_id` column,
/// so without this set an applied vocabulary or contributor version would
/// surface in the Versions tab as a phantom pending draft.
async fn junction_pointer_ids(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    manifestation_id: Uuid,
    work_id: Uuid,
) -> Result<Vec<Uuid>, AppError> {
    sqlx::query_scalar!(
        "SELECT t.source_version_id AS \"id!\" FROM ( \
             SELECT source_version_id FROM manifestation_genres WHERE manifestation_id = $1 \
             UNION \
             SELECT source_version_id FROM manifestation_moods WHERE manifestation_id = $1 \
             UNION \
             SELECT source_version_id FROM manifestation_tags WHERE manifestation_id = $1 \
             UNION \
             SELECT source_version_id FROM work_authors WHERE work_id = $2 \
         ) t \
         WHERE t.source_version_id IS NOT NULL",
        manifestation_id,
        work_id,
    )
    .fetch_all(&mut **tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))
}

/// Collect the non-NULL canonical pointer ids on the given detail row
/// — the set the pending-count query excludes and `accepted_pointer_count`
/// derives its count from.
fn canonical_pointer_ids(row: &DetailRow) -> Vec<Uuid> {
    [
        row.w_title_v,
        row.w_subtitle_v,
        row.w_desc_v,
        row.w_lang_v,
        row.m_publisher_v,
        row.m_pubdate_v,
        row.m_isbn10_v,
        row.m_isbn13_v,
        row.m_cover_v,
        row.m_pages_v,
        row.m_content_rating_v,
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
        row.w_subtitle_v.is_some(),
        row.w_desc_v.is_some(),
        row.w_lang_v.is_some(),
        row.m_publisher_v.is_some(),
        row.m_pubdate_v.is_some(),
        row.m_isbn10_v.is_some(),
        row.m_isbn13_v.is_some(),
        row.m_cover_v.is_some(),
        row.m_pages_v.is_some(),
        row.m_content_rating_v.is_some(),
    ]
    .into_iter()
    .filter(|b| *b)
    .count();
    u32::try_from(filled).unwrap_or(u32::MAX)
}

/// `GET /api/v1/works/{id}` — work-level prose with every manifestation
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
#[utoipa::path(
    get,
    path = "/api/v1/works/{id}",
    tag = "library",
    params(("id" = Uuid, Path, description = "Work id")),
    security(("session_cookie" = ["read"]), ("device_token_bearer" = ["read"]), ("oidc_jwt_bearer" = ["read"]), ("opds_basic" = ["read"])),
    responses(
        (status = 200, description = "Work detail with visible manifestations", body = WorkDetail),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails),
        (status = 404, description = "Not found or RLS-hidden", body = crate::openapi::ProblemDetails)
    )
)]
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
               pages,
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
                subtitle, \
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
            pages: r.pages,
            cover_url: format!("/api/v1/books/{}/cover/thumb", r.id),
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
        subtitle: work.subtitle,
        authors,
        description: work.description,
        language: work.language,
        series,
        manifestations,
    }))
}
