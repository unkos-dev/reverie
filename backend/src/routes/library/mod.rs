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

use axum::extract::{OriginalUri, State};
use axum::http::{HeaderMap, HeaderValue, header::LINK};
use axum::response::IntoResponse;
use axum_extra::extract::{Query, QueryRejection};
use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::auth::middleware::CurrentUser;
use crate::error::AppError;
use crate::extract::ApiPath;
use crate::models::library::{BookDetail, BookListRow, WorkDetail};
use crate::state::AppState;

pub(crate) use crate::services::reader::{
    load_genres_for_manifestations, load_moods_for_manifestations, load_tags_for_manifestations,
};
pub(crate) mod filters;
mod search;

#[cfg(test)]
mod tests;

/// Build the `/api/v1/books*`, `/api/v1/works/{id}`, and `/api/v1/search` router.
///
/// It is an [`OpenApiRouter`] so each handler's `#[utoipa::path]` contributes
/// to the generated spec (a missing annotation fails to compile). Merged into `crate::openapi::pilot_router` and split into its runtime and
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
pub(crate) const MAX_TAG_FILTERS: usize = 20;

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
///
/// # Typed column filters
/// A flat suffix grammar of typed per-column conditions, AND-combined with the
/// vocabulary filters above and each other. Text (`title_contains/_eq/_ne`,
/// `subtitle_contains/_empty`, `isbn_13_contains/_eq/_empty`) is
/// case-insensitive `ILIKE`; the `_contains` legs (and the `q` quick filter
/// below) also fold accents, so an unaccented needle matches an accented
/// stored value, while `_eq`/`_ne` stay accent-sensitive; numeric
/// (`pages_gte/_lte/_empty`) and date
/// (`created_at_gte/_lt`, inclusive lower and exclusive upper instants) are scalar bounds; `status_any/_none`
/// (with the `unread` pseudo-value) and `rating_gte/_lte/_empty` probe the
/// caller's RLS-scoped `reading_state`; `author`/`author_any`/`author_none`
/// mirror the vocabulary triple over the `author` role; `q` is a full-text
/// OR title-trigram filter (no ranking). Building and validation live in
/// [`filters`]; see [`filters::push_filter_predicates`] and
/// [`filters::validate`].
#[derive(Debug, Default, Clone, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub(crate) struct ListParams {
    #[serde(default)]
    pub(crate) cursor: Option<String>,
    /// Sort stack, JSON:API grammar: comma-separated whitelisted field
    /// names (`title`, `subtitle`, `author`, `series`, `isbn_13`, `pages`,
    /// `status`, `rating`, `created_at`); a leading `-` reverses that level
    /// to descending; order of appearance sets priority (the first field is
    /// the primary key). An unwhitelisted field, a repeated column, or more
    /// than three levels is a 422. Omitted, this is `-created_at` (today's
    /// "recent" order).
    #[serde(default)]
    #[param(example = "-created_at,title", max_length = 40)]
    pub(crate) sort: Option<String>,
    /// All-of author filter: every listed author id must be linked to
    /// the work in the `author` role. A single `?author=<uuid>` is the
    /// one-element case.
    #[serde(default)]
    #[param(max_items = 20, max_length = 45)]
    pub(crate) author: Vec<Uuid>,
    /// Any-of author filter: at least one listed author id is linked.
    #[serde(default)]
    #[param(max_items = 20, max_length = 45)]
    pub(crate) author_any: Vec<Uuid>,
    /// None-of author filter: no listed author id is linked.
    #[serde(default)]
    #[param(max_items = 20, max_length = 45)]
    pub(crate) author_none: Vec<Uuid>,
    #[serde(default)]
    #[param(max_length = 45)]
    pub(crate) series: Option<Uuid>,
    #[serde(default)]
    #[param(max_items = 20, max_length = 45)]
    pub(crate) series_any: Vec<Uuid>,
    #[serde(default)]
    #[param(max_length = 45)]
    pub(crate) shelf: Option<Uuid>,
    #[serde(default)]
    #[param(max_items = 20, max_length = 45)]
    pub(crate) shelf_any: Vec<Uuid>,
    #[serde(default)]
    #[param(max_items = 20)]
    pub(crate) tag: Vec<String>,
    /// All-of genre filter: every listed genre name must be attached.
    #[serde(default)]
    #[param(max_items = 20)]
    pub(crate) genre: Vec<String>,
    /// Any-of genre filter: at least one listed genre name is attached.
    #[serde(default)]
    #[param(max_items = 20)]
    pub(crate) genre_any: Vec<String>,
    /// None-of genre filter: no listed genre name is attached.
    #[serde(default)]
    #[param(max_items = 20)]
    pub(crate) genre_none: Vec<String>,
    /// All-of mood filter: every listed mood name must be attached.
    #[serde(default)]
    #[param(max_items = 20)]
    pub(crate) mood: Vec<String>,
    /// Any-of mood filter: at least one listed mood name is attached.
    #[serde(default)]
    #[param(max_items = 20)]
    pub(crate) mood_any: Vec<String>,
    /// None-of mood filter: no listed mood name is attached.
    #[serde(default)]
    #[param(max_items = 20)]
    pub(crate) mood_none: Vec<String>,
    /// Any-of tag filter: at least one listed tag name is attached.
    #[serde(default)]
    #[param(max_items = 20)]
    pub(crate) tag_any: Vec<String>,
    /// None-of tag filter: no listed tag name is attached.
    #[serde(default)]
    #[param(max_items = 20)]
    pub(crate) tag_none: Vec<String>,
    /// Case-insensitive, accent-insensitive substring match on the work
    /// title.
    #[serde(default)]
    pub(crate) title_contains: Option<String>,
    /// Case-insensitive exact match on the work title (no wildcards).
    #[serde(default)]
    pub(crate) title_eq: Option<String>,
    /// Case-insensitive exact non-match on the work title.
    #[serde(default)]
    pub(crate) title_ne: Option<String>,
    /// Case-insensitive, accent-insensitive substring match on the work
    /// subtitle.
    #[serde(default)]
    pub(crate) subtitle_contains: Option<String>,
    /// `true` keeps only rows with no subtitle; `false` keeps only rows
    /// that have one.
    #[serde(default)]
    pub(crate) subtitle_empty: Option<bool>,
    /// Partial substring match on the manifestation ISBN-13 after removing ASCII
    /// spaces and hyphens; a complete ISBN is not required.
    #[serde(default)]
    pub(crate) isbn_13_contains: Option<String>,
    /// Case-insensitive exact match on the manifestation ISBN-13.
    #[serde(default)]
    pub(crate) isbn_13_eq: Option<String>,
    /// `true` keeps only rows with no ISBN-13; `false` only rows with one.
    #[serde(default)]
    pub(crate) isbn_13_empty: Option<bool>,
    /// Lower bound (inclusive) on page count.
    #[serde(default)]
    pub(crate) pages_gte: Option<i32>,
    /// Upper bound (inclusive) on page count.
    #[serde(default)]
    pub(crate) pages_lte: Option<i32>,
    /// `true` keeps only rows with no page count; `false` only rows with one.
    #[serde(default)]
    pub(crate) pages_empty: Option<bool>,
    /// Inclusive lower bound on the added timestamp, as an RFC 3339 instant.
    #[serde(default, deserialize_with = "filters::timestamp_opt::deserialize")]
    pub(crate) created_at_gte: Option<Timestamp>,
    /// Exclusive upper bound on the added timestamp, as an RFC 3339 instant.
    #[serde(default, deserialize_with = "filters::timestamp_opt::deserialize")]
    pub(crate) created_at_lt: Option<Timestamp>,
    /// Any-of reading-status filter. Each value is a `reading_status` wire
    /// name or the `unread` pseudo-value (no status set); a row qualifies
    /// when it matches at least one.
    #[serde(default)]
    #[param(max_items = 20, max_length = 12)]
    pub(crate) status_any: Vec<String>,
    /// None-of reading-status filter: excludes rows matching any value.
    #[serde(default)]
    #[param(max_items = 20, max_length = 12)]
    pub(crate) status_none: Vec<String>,
    /// Lower bound (inclusive) on the caller's rating (1 to 5).
    #[serde(default)]
    pub(crate) rating_gte: Option<i16>,
    /// Upper bound (inclusive) on the caller's rating (1 to 5).
    #[serde(default)]
    pub(crate) rating_lte: Option<i16>,
    /// `true` keeps only rows the caller has not rated; `false` only rated rows.
    #[serde(default)]
    pub(crate) rating_empty: Option<bool>,
    /// Quick-search text: narrows the current result set to rows whose
    /// title or full-text vector matches, within the active sort order.
    /// Case- and accent-insensitive. A filter, not a ranked search (that
    /// lives at `/api/v1/search`).
    #[serde(default)]
    pub(crate) q: Option<String>,
    /// `true` keeps only rows with pending metadata proposals awaiting review;
    /// `false` keeps only rows without any. Child accounts never inspect
    /// proposals (`true` yields no rows; `false` is a no-op).
    #[serde(default)]
    pub(crate) needs_review: Option<bool>,
    /// `true` keeps only rows whose enrichment exhausted retries and gave up
    /// (`enrichment_status = 'skipped'` with a recorded error); `false`
    /// excludes them.
    #[serde(default)]
    pub(crate) enrichment_failed: Option<bool>,
    /// `true` keeps only rows with no stored or embedded cover image; `false`
    /// keeps only rows with a cover image.
    #[serde(default)]
    pub(crate) cover_empty: Option<bool>,
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
///   via the `From<QueryRejection>` impl).
/// - [`AppError::Validation`] when the cursor is malformed, its
///   embedded sort spec mismatches the requested stack, a filter exceeds
///   its semantic limits, a range's lower bound exceeds its upper bound,
///   or `?sort=` names an unwhitelisted field, repeats a column, or
///   exceeds [`crate::routes::sort_spec::MAX_SORT_LEVELS`] levels.
/// - [`AppError::Internal`] on database errors.
#[utoipa::path(
    get,
    path = "/api/v1/books",
    summary = "List books",
    description = "Returns one page of the manifestations visible to the caller, filterable and sortable via query parameters.",
    tag = "library",
    params(ListParams),
    security(("session_cookie" = ["read"]), ("device_token_bearer" = ["read"]), ("opds_basic" = ["read"])),
    responses(
        (status = 200, description = "Paginated list of visible books", body = BookListResponse,
            headers(("Link" = String, description = "RFC 8288 next-page link; emitted with rel=\"next\" when more rows remain"))),
        (status = 400, description = "Malformed query parameter", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 422, description = "Invalid cursor, filter value or sort stack, including inverted range bounds", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
    )
)]
async fn list(
    current_user: CurrentUser,
    State(state): State<AppState>,
    params: Result<Query<ListParams>, QueryRejection>,
    OriginalUri(uri): OriginalUri,
) -> Result<impl IntoResponse, AppError> {
    let Query(params) = params?;
    let (items, next_cursor) = crate::services::reader::list_books(
        &current_user,
        &state,
        &params,
        i64::from(state.config.opds.page_size),
    )
    .await?;

    let mut headers = HeaderMap::new();
    if let Some(ref nc) = next_cursor {
        let next_url = build_next_url(&uri, nc);
        if let Ok(value) = HeaderValue::from_str(&format!("<{next_url}>; rel=\"next\"")) {
            headers.insert(LINK, value);
        }
    }

    Ok((headers, axum::Json(BookListResponse { items, next_cursor })))
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
    operation_id = "book_detail",
    summary = "Get a book's detail",
    description = "Returns detail for one manifestation, including work-level prose and a metadata-version summary. A manifestation hidden from the caller is reported as 404 rather than leaking its existence. A child account receives an empty pending-version list and a pending count of zero.",
    tag = "library",
    params(("id" = Uuid, Path, max_length = 45, description = "Manifestation id")),
    security(("session_cookie" = ["read"]), ("device_token_bearer" = ["read"]), ("opds_basic" = ["read"])),
    responses(
        (status = 200, description = "Book detail", body = BookDetail),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 404, description = "Not found or RLS-hidden", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
    )
)]
async fn detail(
    current_user: CurrentUser,
    State(state): State<AppState>,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<axum::Json<BookDetail>, AppError> {
    crate::services::reader::book_detail(&current_user, &state, id)
        .await
        .map(axum::Json)
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
    summary = "Get a work's detail",
    description = "Returns work-level prose plus every manifestation of that work visible to the caller. Fails with 404 when the work is missing or every manifestation is hidden from the caller.",
    tag = "library",
    params(("id" = Uuid, Path, max_length = 45, description = "Work id")),
    security(("session_cookie" = ["read"]), ("device_token_bearer" = ["read"]), ("opds_basic" = ["read"])),
    responses(
        (status = 200, description = "Work detail with visible manifestations", body = WorkDetail),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 404, description = "Not found or RLS-hidden", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
    )
)]
async fn work_detail(
    current_user: CurrentUser,
    State(state): State<AppState>,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<axum::Json<WorkDetail>, AppError> {
    crate::services::reader::work_detail(&current_user, &state, id)
        .await
        .map(axum::Json)
}
