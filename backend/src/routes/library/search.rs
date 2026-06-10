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
//!
//! # Invariants
//! - RLS on `manifestations` gates row visibility; the handler never
//!   adds an ad-hoc `WHERE user_id = …` predicate.
//! - `ts_headline` markers are ASCII control codepoints
//!   ([`SNIPPET_HL_START`] = `\x02` STX, [`SNIPPET_HL_END`] = `\x03`
//!   ETX). They are reserved by Unicode and never appear in valid
//!   text, so the React renderer can split on those bytes without
//!   `dangerouslySetInnerHTML` and without colliding with
//!   user-authored typography (e.g. French `‹›` guillemets). The
//!   frontend constants must stay in lockstep with these bytes.
//! - `q` is bound — not interpolated — so the SQL-injection probe
//!   (`'); DROP TABLE works;--`) hits the parameterised path and the
//!   schema survives.

use std::collections::HashMap;

use axum::Json;
use axum::extract::State;
use axum_extra::extract::Query;
use serde::Deserialize;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::auth::middleware::CurrentUser;
use crate::db;
use crate::error::AppError;
use crate::models::library::{SearchHit, SearchHitKind, SearchResponse};
use crate::state::AppState;

/// Build the `/api/v1/search` router as an [`OpenApiRouter`] so the handler's
/// `#[utoipa::path]` contributes to the generated spec. Merged by the parent
/// `library::router` (the macro resolves `search` in the module that defines it).
pub(super) fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(search))
}

/// Maximum input length for `q`. Exceeding this returns a 422
/// `validation` problem before any DB work — bounds the worst-case
/// `ts_headline` cost and matches the front-end input cap.
const MAX_Q_LEN: usize = 200;

/// Top-N rows returned by `GET /api/v1/search`. Command-palette UX needs
/// a tight set; deeper exploration uses `/api/v1/books` with filters.
const SEARCH_LIMIT: i64 = 20;

/// Trigram similarity floor — rows below this are dropped from the
/// `trgm_hits` CTE so the result set isn't padded with near-miss
/// noise. Matches `pg_trgm.similarity_threshold` (Postgres GUC,
/// default `0.3`) so the floor aligns with what the `%` operator uses
/// implicitly. Independent of the underlying index kind (GIST/GIN).
const TRGM_SIMILARITY_FLOOR: f32 = 0.3;

/// `ts_headline` start marker — ASCII STX (`\x02`). Paired with
/// [`SNIPPET_HL_END`]. Reserved by Unicode and never appears in valid
/// text, so frontend can split on this byte without colliding with
/// user-authored typography (French `‹›` guillemets, math notation,
/// etc.). The frontend's `SNIPPET_HL_START` / `SNIPPET_HL_END`
/// constants in `frontend/src/components/CommandPalette.tsx` must
/// stay in lockstep with these bytes.
const SNIPPET_HL_START: &str = "\u{0002}";
/// `ts_headline` end marker — ASCII ETX (`\x03`). See
/// [`SNIPPET_HL_START`].
const SNIPPET_HL_END: &str = "\u{0003}";

/// `?q=` query parameter for `GET /api/v1/search`.
#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub(super) struct SearchParams {
    /// Free-form query text. Empty / whitespace-only → 422; over
    /// [`MAX_Q_LEN`] chars → 422.
    #[serde(default)]
    q: Option<String>,
}

/// `GET /api/v1/search` — return the top hybrid-ranked manifestations
/// matching `q`, snippet-highlighted via `ts_headline`.
///
/// # Errors
/// - [`AppError::Validation`] when `q` is missing, empty, or longer
///   than [`MAX_Q_LEN`] characters.
/// - [`AppError::Internal`] on database errors.
#[allow(
    clippy::too_many_lines,
    reason = "single hybrid CTE assembly; splitting hurts readability of the SQL flow"
)]
#[utoipa::path(
    get,
    path = "/api/v1/search",
    tag = "library",
    params(SearchParams),
    responses(
        (status = 200, description = "Top hybrid-ranked search results", body = SearchResponse),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails),
        (status = 422, description = "Empty, missing, or over-length query", body = crate::openapi::ProblemDetails)
    )
)]
pub(super) async fn search(
    current_user: CurrentUser,
    State(state): State<AppState>,
    Query(params): Query<SearchParams>,
) -> Result<Json<SearchResponse>, AppError> {
    let q_raw = params
        .q
        .as_deref()
        .ok_or_else(|| AppError::Validation("query required".into()))?;
    let q = q_raw.trim();
    if q.is_empty() {
        return Err(AppError::Validation("query required".into()));
    }
    if q.chars().count() > MAX_Q_LEN {
        return Err(AppError::Validation(format!(
            "query too long: {MAX_Q_LEN} character limit",
        )));
    }

    let mut tx = db::acquire_with_rls(&state.pool, current_user.user_id)
        .await
        .inspect_err(|e| tracing::error!(error = %e, "search: acquire_with_rls failed"))
        .map_err(|e| AppError::Internal(e.into()))?;

    // ts_headline option string assembled from typed constants so the
    // marker bytes stay in sync with [`SNIPPET_HL_START`] /
    // [`SNIPPET_HL_END`] (the frontend reads them at the matching
    // offsets in `CommandPalette.tsx`).
    let headline_opts = format!(
        "StartSel={SNIPPET_HL_START}, StopSel={SNIPPET_HL_END}, \
         MaxFragments=2, MaxWords=20, MinWords=5"
    );

    let rows = sqlx::query!(
        r#"
        WITH
            q AS (SELECT websearch_to_tsquery('english', $1) AS tsq),
            ts_hits AS (
                SELECT w.id, ts_rank_cd(w.search_vector, q.tsq, 32) AS rank
                FROM works w, q
                WHERE w.search_vector @@ q.tsq
            ),
            trgm_hits AS (
                SELECT w.id, similarity(w.title, $1) * 0.5 AS rank
                FROM works w
                WHERE w.title % $1
                  AND similarity(w.title, $1) > $4
            ),
            merged AS (
                SELECT id, MAX(rank) AS rank, BOOL_OR(src = 'ts') AS has_ts
                FROM (
                    SELECT id, rank, 'ts'::text AS src FROM ts_hits
                    UNION ALL
                    SELECT id, rank, 'trgm'::text AS src FROM trgm_hits
                ) u
                GROUP BY id
            )
        SELECT m.id                AS "m_id!",
               m.work_id           AS "work_id!",
               w.title             AS "title!",
               merged.rank::float8 AS "rank!",
               merged.has_ts       AS "has_ts!",
               CASE
                   WHEN merged.has_ts THEN
                       ts_headline(
                           'english',
                           COALESCE(w.title, '') || ' ' || COALESCE(w.description, ''),
                           (SELECT tsq FROM q),
                           $3
                       )
                   ELSE NULL
               END AS snippet
          -- The JOIN to manifestations fans out one row per
          -- (work, manifestation) pair, so a work with multiple
          -- formats (epub + pdf + cbz) emits a separate result per
          -- format. This is intentional for the command-palette UX:
          -- each manifestation is its own navigable card, mirroring
          -- the list-page row shape. The LIMIT is therefore "top-N
          -- manifestations across all matching works", not
          -- "top-N works".
          FROM merged
          JOIN works w           ON w.id = merged.id
          JOIN manifestations m  ON m.work_id = w.id
         ORDER BY merged.rank DESC, m.id ASC
         LIMIT $2
        "#,
        q,
        SEARCH_LIMIT,
        headline_opts,
        TRGM_SIMILARITY_FLOOR,
    )
    .fetch_all(&mut *tx)
    .await
    .inspect_err(|e| tracing::error!(error = %e, "search: hybrid CTE query failed"))
    .map_err(|e| AppError::Internal(e.into()))?;

    let work_ids: Vec<Uuid> = rows.iter().map(|r| r.work_id).collect();
    let authors = load_authors(&mut tx, &work_ids).await?;

    tx.commit()
        .await
        .inspect_err(|e| tracing::error!(error = %e, "search: tx.commit failed"))
        .map_err(|e| AppError::Internal(e.into()))?;

    let items: Vec<SearchHit> = rows
        .into_iter()
        .map(|r| SearchHit {
            kind: SearchHitKind::Book,
            id: r.m_id,
            work_id: Some(r.work_id),
            title: r.title,
            authors: authors.get(&r.work_id).cloned().unwrap_or_default(),
            snippet: r.snippet,
            cover_url: Some(format!("/api/v1/books/{}/cover/thumb", r.m_id)),
        })
        .collect();

    Ok(Json(SearchResponse { items }))
}

/// Batch-load author display names for the work ids in the search
/// page, preserving `work_authors.position` order. Mirrors the list
/// handler's `load_authors_for_works` shape so call sites stay
/// consistent.
async fn load_authors(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
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
    .inspect_err(|e| tracing::error!(error = %e, "search: load_authors batch query failed"))
    .map_err(|e| AppError::Internal(e.into()))?;
    for r in rows {
        out.entry(r.work_id).or_default().push(r.name);
    }
    Ok(out)
}
