//! `GET /api/v1/search?q=` — hybrid full-text + trigram search (11b).
//!
//! Mirrors the read pattern of [`crate::routes::opds::library::emit_search`]
//! (same RLS seam via [`crate::db::acquire_with_rls`]) but two
//! upgrades over the OPDS variant:
//! 1. **`websearch_to_tsquery`** in place of `plainto_tsquery` — gains
//!    quoted-phrase (`"war and peace"`) and exclude (`-tolstoy`)
//!    operator handling on the tsquery leg, with the same
//!    input-validation surface (no user-controlled tsquery syntax
//!    injection). **Caveat**: the trigram leg compares raw similarity
//!    against the whole query string and has no notion of token
//!    negation, so a `-token` excluded row can resurface via trigram
//!    similarity. The composite contract is therefore "tsquery
//!    semantics on the tsquery leg; trigram-fallback semantics on
//!    near-misses" — see the `search_websearch_exclude_operator`
//!    integration test for the documented limitation.
//! 2. **Hybrid CTE** — full-text and trigram hits each use their
//!    native index (GIN on `works.search_vector`, GIST trigram on
//!    `works.title`) then `UNION ALL`-merge with `MAX(rank)`. A
//!    single-OR predicate would force the planner into a bitmap-or
//!    scan; this shape keeps each leg index-friendly even at the
//!    50K-row library size the blueprint targets.
//! 3. **Accent folding** — both legs are accent-insensitive. The
//!    tsvector leg runs under the `unaccent_english` text search
//!    configuration (chained `unaccent` + `english_stem`), so
//!    `search_vector` and the `websearch_to_tsquery`/`ts_headline`
//!    calls against it must share that configuration name; the
//!    trigram leg wraps both `w.title` and the bound query in
//!    `immutable_unaccent`, matching the folded expression index. An
//!    unaccented query therefore finds an accented row on either leg,
//!    while stored titles keep their accents for display.
//!
//! # Invariants
//! - RLS on `manifestations` gates row visibility; the handler never
//!   adds an ad-hoc `WHERE user_id = …` predicate.
//! - `ts_headline` markers are ASCII control codepoints
//!   (`STX` = `\x02` STX, `ETX` = `\x03`
//!   ETX). They are reserved by Unicode and never appear in valid
//!   text, so the React renderer can split on those bytes without
//!   `dangerouslySetInnerHTML` and without colliding with
//!   user-authored typography (e.g. French `‹›` guillemets). The
//!   frontend constants must stay in lockstep with these bytes.
//! - `q` is bound — not interpolated — so the SQL-injection probe
//!   (`'); DROP TABLE works;--`) hits the parameterised path and the
//!   schema survives.

use axum::Json;
use axum::extract::State;
use axum_extra::extract::{Query, QueryRejection};
use serde::Deserialize;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::auth::middleware::CurrentUser;
use crate::error::AppError;
use crate::models::library::SearchResponse;
use crate::state::AppState;

pub(super) fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(search))
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub(super) struct SearchParams {
    /// Free-form query text. Empty / whitespace-only → 422; over
    /// 200 chars → 422.
    #[serde(default)]
    q: Option<String>,
}

#[utoipa::path(
    get,
    path = "/api/v1/search",
    summary = "Search the library",
    description = "Returns the top hybrid-ranked manifestations matching a free-text query, with highlighted snippets.",
    tag = "library",
    params(SearchParams),
    security(("session_cookie" = ["read"]), ("device_token_bearer" = ["read"]), ("opds_basic" = ["read"])),
    responses(
        (status = 200, description = "Top hybrid-ranked search results", body = SearchResponse),
        (status = 400, description = "Malformed query parameter", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 422, description = "Empty, missing, or over-length query", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
    )
)]
pub(super) async fn search(
    current_user: CurrentUser,
    State(state): State<AppState>,
    params: Result<Query<SearchParams>, QueryRejection>,
) -> Result<Json<SearchResponse>, AppError> {
    let Query(params) = params?;
    crate::services::reader::search(&current_user, &state, params.q.as_deref())
        .await
        .map(Json)
}
