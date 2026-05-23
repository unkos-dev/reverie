//! `GET /api/search?q=` — hybrid full-text + trigram search (11b).
//!
//! Mirrors the read pattern of [`crate::routes::opds::library::emit_search`]
//! (same RLS seam via [`crate::db::acquire_with_rls`]) but two
//! upgrades over the OPDS variant:
//! 1. **`websearch_to_tsquery`** in place of `plainto_tsquery` — gains
//!    quoted-phrase (`"war and peace"`) and exclude (`-tolstoy`)
//!    operator handling for free, with the same input-validation
//!    surface (no user-controlled tsquery syntax injection).
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
//! - `ts_headline` markers are non-HTML (`‹` / `›`) so the React
//!   renderer can split on those characters without
//!   `dangerouslySetInnerHTML`.
//! - `q` is bound — not interpolated — so the SQL-injection probe
//!   (`'); DROP TABLE works;--`) hits the parameterised path and the
//!   schema survives.

use std::collections::HashMap;

use axum::Json;
use axum::extract::State;
use axum_extra::extract::Query;
use serde::Deserialize;
use uuid::Uuid;

use crate::auth::middleware::CurrentUser;
use crate::db;
use crate::error::AppError;
use crate::models::library::{SearchHit, SearchHitKind, SearchResponse};
use crate::state::AppState;

/// Maximum input length for `q`. Exceeding this returns a 422
/// `validation` problem before any DB work — bounds the worst-case
/// `ts_headline` cost and matches the front-end input cap.
const MAX_Q_LEN: usize = 200;

/// Top-N rows returned by `GET /api/search`. Command-palette UX needs
/// a tight set; deeper exploration uses `/api/books` with filters.
const SEARCH_LIMIT: i64 = 20;

/// Trigram similarity floor — rows below this are dropped from the
/// `trgm_hits` CTE so the result set isn't padded with near-miss
/// noise. Matches the value used in the GIST `%` operator default and
/// the plan's design constraint.
const TRGM_SIMILARITY_FLOOR: f32 = 0.3;

/// `?q=` query parameter for `GET /api/search`.
#[derive(Debug, Deserialize)]
pub(super) struct SearchParams {
    /// Free-form query text. Empty / whitespace-only → 422; over
    /// [`MAX_Q_LEN`] chars → 422.
    #[serde(default)]
    q: Option<String>,
}

/// `GET /api/search` — return the top hybrid-ranked manifestations
/// matching `q`, snippet-highlighted via `ts_headline`.
///
/// # Errors
/// - [`AppError::Validation`] when `q` is missing, empty, or longer
///   than [`MAX_Q_LEN`] characters.
/// - [`AppError::Internal`] on database errors.
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
        .map_err(|e| AppError::Internal(e.into()))?;

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
          FROM merged
          JOIN works w           ON w.id = merged.id
          JOIN manifestations m  ON m.work_id = w.id
         ORDER BY merged.rank DESC, m.id ASC
         LIMIT $2
        "#,
        q,
        SEARCH_LIMIT,
        "StartSel=‹, StopSel=›, MaxFragments=2, MaxWords=20, MinWords=5",
        TRGM_SIMILARITY_FLOOR,
    )
    .fetch_all(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    let work_ids: Vec<Uuid> = rows.iter().map(|r| r.work_id).collect();
    let authors = load_authors(&mut tx, &work_ids).await?;

    tx.commit()
        .await
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
            cover_url: Some(format!("/api/books/{}/cover/thumb", r.m_id)),
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
    .map_err(|e| AppError::Internal(e.into()))?;
    for r in rows {
        out.entry(r.work_id).or_default().push(r.name);
    }
    Ok(out)
}
