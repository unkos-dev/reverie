//! `/api/v1/{genres,moods,tags,authors,series,publishers}/suggest` typeahead
//! endpoints: prefix-then-similarity lookup over the metadata vocabularies,
//! scoped to values already in use on a manifestation the caller can see.
//!
//! # Invariants
//! - Same RLS seam as [`crate::routes::library`]:
//!   [`crate::db::acquire_with_rls`] sets `app.current_user_id` for the
//!   transaction, and every scope subquery below reaches the vocabulary
//!   through a join onto `manifestations`, so a child account's shelf-gated
//!   visibility narrows suggestions exactly like it narrows `/api/v1/books`.
//! - "In use" means linked to at least one manifestation row the caller can
//!   see; a vocabulary row with no linking rows (an orphaned `genres`/`moods`/
//!   `tags` entry, or a `series`/`authors` row with no visible manifestation)
//!   never surfaces, so the palette only offers values worth reusing.
//! - Two static query shapes per entity, chosen by input length: under three
//!   characters, trigram matching is unreliable (too few three-character
//!   windows), so only the `ILIKE` prefix leg runs; three characters or more
//!   adds a `word_similarity` fuzzy leg for typo tolerance.
//! - `q` is matched case-insensitively via `ILIKE`; `%` and `_` in the raw
//!   input are backslash-escaped before binding into the pattern (Postgres's
//!   default `LIKE`/`ILIKE` escape character is already backslash, so no
//!   explicit `ESCAPE` clause is needed) so a caller cannot smuggle wildcard
//!   metacharacters into the prefix match.

use axum::Json;
use axum::extract::State;
use axum_extra::extract::{Query, QueryRejection};
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, Transaction};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::auth::middleware::CurrentUser;
use crate::db;
use crate::error::AppError;
use crate::state::AppState;

/// Build the `/api/v1/*/suggest` router as an [`OpenApiRouter`] so each
/// handler's `#[utoipa::path]` contributes to the generated spec. Merged
/// into `crate::openapi::pilot_router`.
pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(suggest_genres))
        .routes(routes!(suggest_moods))
        .routes(routes!(suggest_tags))
        .routes(routes!(suggest_authors))
        .routes(routes!(suggest_series))
        .routes(routes!(suggest_publishers))
}

/// Below this character count, `word_similarity` runs on too few trigrams to
/// be a meaningful signal, so only the `ILIKE` prefix leg is used.
const FUZZY_MIN_CHARS: usize = 3;
/// Upper bound on `q`; matches the front-end typeahead input cap.
const MAX_Q_CHARS: usize = 100;
/// `?limit=` default when omitted.
const DEFAULT_LIMIT: i64 = 10;
/// `?limit=` floor; requests below this are clamped up rather than rejected.
const MIN_LIMIT: i64 = 1;
/// `?limit=` ceiling; requests above this are clamped down rather than
/// rejected, so a caller cannot force an unbounded scan via a huge value.
const MAX_LIMIT: i64 = 20;

/// `?q=` / `?limit=` query parameters shared by every suggest endpoint.
#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
struct SuggestParams {
    /// Free-form typeahead text. Trimmed before matching; empty after
    /// trimming, or longer than 100 characters, is rejected with 422.
    q: String,
    /// Maximum rows returned. Clamped to 1..=20 (default 10); never rejects.
    #[serde(default)]
    limit: Option<i64>,
}

/// One typeahead suggestion.
#[derive(Debug, Serialize, utoipa::ToSchema)]
struct Suggestion {
    /// Row id in the backing vocabulary table. `None` for publisher
    /// suggestions, which have no dedicated table of their own.
    id: Option<Uuid>,
    /// Display text to show in the typeahead and to resubmit as a filter.
    value: String,
}

/// `GET .../suggest` response envelope.
#[derive(Debug, Serialize, utoipa::ToSchema)]
struct SuggestResponse {
    suggestions: Vec<Suggestion>,
}

/// Validated, normalized inputs shared by every entity's query pair, so each
/// handler validates once and the six query functions take one argument.
struct SuggestInput {
    /// Raw trimmed `q`, `%`/`_`/`\` escaped, for the `ILIKE` prefix leg.
    escaped: String,
    /// Raw trimmed `q`, unescaped, for the `word_similarity` leg (fuzzy
    /// matching operates on the literal text, not an `ILIKE` pattern).
    raw: String,
    /// `true` when `raw` is under [`FUZZY_MIN_CHARS`]: prefix-only query.
    short: bool,
    /// Clamped row cap, bound into every query's `LIMIT`.
    limit: i64,
}

/// Trim, length-validate, and clamp the limit for `params`, shared by every
/// handler below.
///
/// # Errors
/// Returns [`AppError::Validation`] when `q` is empty after trimming or
/// exceeds [`MAX_Q_CHARS`] characters.
fn prepare(params: &SuggestParams) -> Result<SuggestInput, AppError> {
    let raw = params.q.trim();
    if raw.is_empty() {
        return Err(AppError::Validation("q must not be empty".into()));
    }
    if raw.chars().count() > MAX_Q_CHARS {
        return Err(AppError::Validation(format!(
            "q must be at most {MAX_Q_CHARS} characters"
        )));
    }
    let limit = params
        .limit
        .unwrap_or(DEFAULT_LIMIT)
        .clamp(MIN_LIMIT, MAX_LIMIT);
    Ok(SuggestInput {
        escaped: escape_like(raw),
        raw: raw.to_owned(),
        short: raw.chars().count() < FUZZY_MIN_CHARS,
        limit,
    })
}

/// Backslash-escape `\`, `%`, and `_` so `raw` is matched literally by
/// `ILIKE` rather than as a wildcard pattern. Postgres's default `LIKE`
/// escape character is already `\`, so callers need no explicit `ESCAPE`
/// clause on the query side.
fn escape_like(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for c in raw.chars() {
        if matches!(c, '\\' | '%' | '_') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Row shape shared by the five vocabulary-table entities (genres, moods,
/// tags, authors, series): a stable id plus the display name.
struct VocabRow {
    id: Uuid,
    name: String,
}

impl From<VocabRow> for Suggestion {
    fn from(row: VocabRow) -> Self {
        Self {
            id: Some(row.id),
            value: row.name,
        }
    }
}

/// Lower `pg_trgm.word_similarity_threshold` for the current transaction.
///
/// The `<%` operator is gated by that GUC, which defaults to 0.6: too strict
/// for typeahead, where a common one-letter typo against a short vocabulary
/// name scores 0.5 to 0.59 and would never match. 0.3 matches the trigram
/// floor the search endpoint uses. `SET LOCAL` keeps `<%` index-eligible
/// where a bare `word_similarity(..) >= x` predicate would not be, and
/// expires with the transaction.
async fn lower_word_similarity_threshold(
    tx: &mut Transaction<'_, Postgres>,
) -> Result<(), AppError> {
    // Runtime query: GUC mutation (allowlist class 3); macros cannot
    // validate SET LOCAL.
    sqlx::query("SET LOCAL pg_trgm.word_similarity_threshold = 0.3")
        .execute(&mut **tx)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    Ok(())
}

async fn query_genres(
    tx: &mut Transaction<'_, Postgres>,
    q: &SuggestInput,
) -> Result<Vec<Suggestion>, AppError> {
    let rows = if q.short {
        sqlx::query_as!(
            VocabRow,
            r#"SELECT g.id, g.name
                 FROM genres g
                WHERE EXISTS (
                          SELECT 1 FROM manifestation_genres mg
                          JOIN manifestations m ON m.id = mg.manifestation_id
                         WHERE mg.genre_id = g.id
                      )
                  AND g.name ILIKE $1 || '%'
                ORDER BY g.name ASC
                LIMIT $2"#,
            q.escaped,
            q.limit,
        )
        .fetch_all(&mut **tx)
        .await
    } else {
        lower_word_similarity_threshold(tx).await?;
        sqlx::query_as!(
            VocabRow,
            r#"SELECT g.id, g.name
                 FROM genres g
                WHERE EXISTS (
                          SELECT 1 FROM manifestation_genres mg
                          JOIN manifestations m ON m.id = mg.manifestation_id
                         WHERE mg.genre_id = g.id
                      )
                  AND (g.name ILIKE $1 || '%' OR $2 <% g.name)
                ORDER BY (g.name ILIKE $1 || '%') DESC,
                         word_similarity($2, g.name) DESC,
                         g.name ASC
                LIMIT $3"#,
            q.escaped,
            q.raw,
            q.limit,
        )
        .fetch_all(&mut **tx)
        .await
    };
    Ok(rows
        .map_err(|e| AppError::Internal(e.into()))?
        .into_iter()
        .map(Suggestion::from)
        .collect())
}

async fn query_moods(
    tx: &mut Transaction<'_, Postgres>,
    q: &SuggestInput,
) -> Result<Vec<Suggestion>, AppError> {
    let rows = if q.short {
        sqlx::query_as!(
            VocabRow,
            r#"SELECT mo.id, mo.name
                 FROM moods mo
                WHERE EXISTS (
                          SELECT 1 FROM manifestation_moods mm
                          JOIN manifestations m ON m.id = mm.manifestation_id
                         WHERE mm.mood_id = mo.id
                      )
                  AND mo.name ILIKE $1 || '%'
                ORDER BY mo.name ASC
                LIMIT $2"#,
            q.escaped,
            q.limit,
        )
        .fetch_all(&mut **tx)
        .await
    } else {
        lower_word_similarity_threshold(tx).await?;
        sqlx::query_as!(
            VocabRow,
            r#"SELECT mo.id, mo.name
                 FROM moods mo
                WHERE EXISTS (
                          SELECT 1 FROM manifestation_moods mm
                          JOIN manifestations m ON m.id = mm.manifestation_id
                         WHERE mm.mood_id = mo.id
                      )
                  AND (mo.name ILIKE $1 || '%' OR $2 <% mo.name)
                ORDER BY (mo.name ILIKE $1 || '%') DESC,
                         word_similarity($2, mo.name) DESC,
                         mo.name ASC
                LIMIT $3"#,
            q.escaped,
            q.raw,
            q.limit,
        )
        .fetch_all(&mut **tx)
        .await
    };
    Ok(rows
        .map_err(|e| AppError::Internal(e.into()))?
        .into_iter()
        .map(Suggestion::from)
        .collect())
}

async fn query_tags(
    tx: &mut Transaction<'_, Postgres>,
    q: &SuggestInput,
) -> Result<Vec<Suggestion>, AppError> {
    let rows = if q.short {
        sqlx::query_as!(
            VocabRow,
            r#"SELECT t.id, t.name
                 FROM tags t
                WHERE EXISTS (
                          SELECT 1 FROM manifestation_tags mt
                          JOIN manifestations m ON m.id = mt.manifestation_id
                         WHERE mt.tag_id = t.id
                      )
                  AND t.name ILIKE $1 || '%'
                ORDER BY t.name ASC
                LIMIT $2"#,
            q.escaped,
            q.limit,
        )
        .fetch_all(&mut **tx)
        .await
    } else {
        lower_word_similarity_threshold(tx).await?;
        sqlx::query_as!(
            VocabRow,
            r#"SELECT t.id, t.name
                 FROM tags t
                WHERE EXISTS (
                          SELECT 1 FROM manifestation_tags mt
                          JOIN manifestations m ON m.id = mt.manifestation_id
                         WHERE mt.tag_id = t.id
                      )
                  AND (t.name ILIKE $1 || '%' OR $2 <% t.name)
                ORDER BY (t.name ILIKE $1 || '%') DESC,
                         word_similarity($2, t.name) DESC,
                         t.name ASC
                LIMIT $3"#,
            q.escaped,
            q.raw,
            q.limit,
        )
        .fetch_all(&mut **tx)
        .await
    };
    Ok(rows
        .map_err(|e| AppError::Internal(e.into()))?
        .into_iter()
        .map(Suggestion::from)
        .collect())
}

/// Scope join deviation: authors have no direct `manifestation_authors`
/// junction; linkage runs `authors -> work_authors -> manifestations` via
/// `manifestations.work_id`, any [`crate::models::author_role::AuthorRole`]
/// (author/editor/translator/narrator) counting as "in use", unlike the
/// `?author=` list filter which narrows to the `author` role for display
/// purposes.
async fn query_authors(
    tx: &mut Transaction<'_, Postgres>,
    q: &SuggestInput,
) -> Result<Vec<Suggestion>, AppError> {
    let rows = if q.short {
        sqlx::query_as!(
            VocabRow,
            r#"SELECT a.id, a.name
                 FROM authors a
                WHERE EXISTS (
                          SELECT 1 FROM work_authors wa
                          JOIN manifestations m ON m.work_id = wa.work_id
                         WHERE wa.author_id = a.id
                      )
                  AND a.name ILIKE $1 || '%'
                ORDER BY a.name ASC
                LIMIT $2"#,
            q.escaped,
            q.limit,
        )
        .fetch_all(&mut **tx)
        .await
    } else {
        lower_word_similarity_threshold(tx).await?;
        sqlx::query_as!(
            VocabRow,
            r#"SELECT a.id, a.name
                 FROM authors a
                WHERE EXISTS (
                          SELECT 1 FROM work_authors wa
                          JOIN manifestations m ON m.work_id = wa.work_id
                         WHERE wa.author_id = a.id
                      )
                  AND (a.name ILIKE $1 || '%' OR $2 <% a.name)
                ORDER BY (a.name ILIKE $1 || '%') DESC,
                         word_similarity($2, a.name) DESC,
                         a.name ASC
                LIMIT $3"#,
            q.escaped,
            q.raw,
            q.limit,
        )
        .fetch_all(&mut **tx)
        .await
    };
    Ok(rows
        .map_err(|e| AppError::Internal(e.into()))?
        .into_iter()
        .map(Suggestion::from)
        .collect())
}

/// Scope join deviation: `series` carries no `work_id` (the schema links a
/// series to its works through the `series_works` junction, mirroring how
/// genres/moods/tags link to manifestations), so the scope predicate joins
/// `series_works -> manifestations` on `manifestations.work_id` rather than
/// the single-table `works.series_id` shape sketched in the API contract.
async fn query_series(
    tx: &mut Transaction<'_, Postgres>,
    q: &SuggestInput,
) -> Result<Vec<Suggestion>, AppError> {
    let rows = if q.short {
        sqlx::query_as!(
            VocabRow,
            r#"SELECT s.id, s.name
                 FROM series s
                WHERE EXISTS (
                          SELECT 1 FROM series_works sw
                          JOIN manifestations m ON m.work_id = sw.work_id
                         WHERE sw.series_id = s.id
                      )
                  AND s.name ILIKE $1 || '%'
                ORDER BY s.name ASC
                LIMIT $2"#,
            q.escaped,
            q.limit,
        )
        .fetch_all(&mut **tx)
        .await
    } else {
        lower_word_similarity_threshold(tx).await?;
        sqlx::query_as!(
            VocabRow,
            r#"SELECT s.id, s.name
                 FROM series s
                WHERE EXISTS (
                          SELECT 1 FROM series_works sw
                          JOIN manifestations m ON m.work_id = sw.work_id
                         WHERE sw.series_id = s.id
                      )
                  AND (s.name ILIKE $1 || '%' OR $2 <% s.name)
                ORDER BY (s.name ILIKE $1 || '%') DESC,
                         word_similarity($2, s.name) DESC,
                         s.name ASC
                LIMIT $3"#,
            q.escaped,
            q.raw,
            q.limit,
        )
        .fetch_all(&mut **tx)
        .await
    };
    Ok(rows
        .map_err(|e| AppError::Internal(e.into()))?
        .into_iter()
        .map(Suggestion::from)
        .collect())
}

/// Publishers have no dedicated vocabulary table: `manifestations.publisher`
/// is free text, so the inner `pubs` CTE is the de-duplicated "vocabulary"
/// (RLS-scoped directly on `manifestations`, no separate `EXISTS` needed)
/// and every returned [`Suggestion`] carries `id: None`.
async fn query_publishers(
    tx: &mut Transaction<'_, Postgres>,
    q: &SuggestInput,
) -> Result<Vec<Suggestion>, AppError> {
    let names = if q.short {
        sqlx::query_scalar!(
            r#"WITH pubs AS (
                   SELECT DISTINCT publisher AS name
                     FROM manifestations
                    WHERE publisher IS NOT NULL
               )
               SELECT name AS "name!"
                 FROM pubs
                WHERE name ILIKE $1 || '%'
                ORDER BY name ASC
                LIMIT $2"#,
            q.escaped,
            q.limit,
        )
        .fetch_all(&mut **tx)
        .await
    } else {
        lower_word_similarity_threshold(tx).await?;
        sqlx::query_scalar!(
            r#"WITH pubs AS (
                   SELECT DISTINCT publisher AS name
                     FROM manifestations
                    WHERE publisher IS NOT NULL
               )
               SELECT name AS "name!"
                 FROM pubs
                WHERE name ILIKE $1 || '%' OR $2 <% name
                ORDER BY (name ILIKE $1 || '%') DESC,
                         word_similarity($2, name) DESC,
                         name ASC
                LIMIT $3"#,
            q.escaped,
            q.raw,
            q.limit,
        )
        .fetch_all(&mut **tx)
        .await
    };
    Ok(names
        .map_err(|e| AppError::Internal(e.into()))?
        .into_iter()
        .map(|value| Suggestion { id: None, value })
        .collect())
}

/// `GET /api/v1/genres/suggest`: genre names in use on a visible
/// manifestation, ranked prefix-first then by similarity.
///
/// Read-scope gated exactly like `GET /api/v1/books` (the least-privilege
/// floor); every other suggest handler below shares the same shape.
///
/// # Errors
/// - [`AppError::MalformedQuery`] when `?limit=` fails to parse as an
///   integer; HTTP 400 via the `From<QueryRejection>` impl.
/// - [`AppError::Validation`] when `q` is empty after trimming or exceeds
///   [`MAX_Q_CHARS`] characters.
/// - [`AppError::Internal`] on database errors.
#[utoipa::path(
    get,
    path = "/api/v1/genres/suggest",
    tag = "suggest",
    params(SuggestParams),
    security(("session_cookie" = ["read"]), ("device_token_bearer" = ["read"]), ("oidc_jwt_bearer" = ["read"]), ("opds_basic" = ["read"])),
    responses(
        (status = 200, description = "Genre names in use on a visible manifestation, ranked prefix-first then by similarity", body = SuggestResponse),
        (status = 400, description = "Malformed query parameter", body = crate::openapi::ProblemDetails),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails),
        (status = 422, description = "Empty or over-length q", body = crate::openapi::ProblemDetails)
    )
)]
async fn suggest_genres(
    current_user: CurrentUser,
    State(state): State<AppState>,
    params: Result<Query<SuggestParams>, QueryRejection>,
) -> Result<Json<SuggestResponse>, AppError> {
    let Query(params) = params?;
    let input = prepare(&params)?;

    let mut tx = db::acquire_with_rls(&state.pool, current_user.user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    let suggestions = query_genres(&mut tx, &input).await?;
    tx.commit()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    Ok(Json(SuggestResponse { suggestions }))
}

/// `GET /api/v1/moods/suggest`: mood names in use on a visible
/// manifestation, ranked prefix-first then by similarity.
///
/// # Errors
/// Same as [`suggest_genres`].
#[utoipa::path(
    get,
    path = "/api/v1/moods/suggest",
    tag = "suggest",
    params(SuggestParams),
    security(("session_cookie" = ["read"]), ("device_token_bearer" = ["read"]), ("oidc_jwt_bearer" = ["read"]), ("opds_basic" = ["read"])),
    responses(
        (status = 200, description = "Mood names in use on a visible manifestation, ranked prefix-first then by similarity", body = SuggestResponse),
        (status = 400, description = "Malformed query parameter", body = crate::openapi::ProblemDetails),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails),
        (status = 422, description = "Empty or over-length q", body = crate::openapi::ProblemDetails)
    )
)]
async fn suggest_moods(
    current_user: CurrentUser,
    State(state): State<AppState>,
    params: Result<Query<SuggestParams>, QueryRejection>,
) -> Result<Json<SuggestResponse>, AppError> {
    let Query(params) = params?;
    let input = prepare(&params)?;

    let mut tx = db::acquire_with_rls(&state.pool, current_user.user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    let suggestions = query_moods(&mut tx, &input).await?;
    tx.commit()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    Ok(Json(SuggestResponse { suggestions }))
}

/// `GET /api/v1/tags/suggest`: tag names in use on a visible
/// manifestation, ranked prefix-first then by similarity.
///
/// # Errors
/// Same as [`suggest_genres`].
#[utoipa::path(
    get,
    path = "/api/v1/tags/suggest",
    tag = "suggest",
    params(SuggestParams),
    security(("session_cookie" = ["read"]), ("device_token_bearer" = ["read"]), ("oidc_jwt_bearer" = ["read"]), ("opds_basic" = ["read"])),
    responses(
        (status = 200, description = "Tag names in use on a visible manifestation, ranked prefix-first then by similarity", body = SuggestResponse),
        (status = 400, description = "Malformed query parameter", body = crate::openapi::ProblemDetails),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails),
        (status = 422, description = "Empty or over-length q", body = crate::openapi::ProblemDetails)
    )
)]
async fn suggest_tags(
    current_user: CurrentUser,
    State(state): State<AppState>,
    params: Result<Query<SuggestParams>, QueryRejection>,
) -> Result<Json<SuggestResponse>, AppError> {
    let Query(params) = params?;
    let input = prepare(&params)?;

    let mut tx = db::acquire_with_rls(&state.pool, current_user.user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    let suggestions = query_tags(&mut tx, &input).await?;
    tx.commit()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    Ok(Json(SuggestResponse { suggestions }))
}

/// `GET /api/v1/authors/suggest`: author names in use on a visible
/// manifestation, ranked prefix-first then by similarity. Any contributor
/// role counts (see [`query_authors`]), not just the `author` role.
///
/// # Errors
/// Same as [`suggest_genres`].
#[utoipa::path(
    get,
    path = "/api/v1/authors/suggest",
    tag = "suggest",
    params(SuggestParams),
    security(("session_cookie" = ["read"]), ("device_token_bearer" = ["read"]), ("oidc_jwt_bearer" = ["read"]), ("opds_basic" = ["read"])),
    responses(
        (status = 200, description = "Author names in use on a visible manifestation, ranked prefix-first then by similarity", body = SuggestResponse),
        (status = 400, description = "Malformed query parameter", body = crate::openapi::ProblemDetails),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails),
        (status = 422, description = "Empty or over-length q", body = crate::openapi::ProblemDetails)
    )
)]
async fn suggest_authors(
    current_user: CurrentUser,
    State(state): State<AppState>,
    params: Result<Query<SuggestParams>, QueryRejection>,
) -> Result<Json<SuggestResponse>, AppError> {
    let Query(params) = params?;
    let input = prepare(&params)?;

    let mut tx = db::acquire_with_rls(&state.pool, current_user.user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    let suggestions = query_authors(&mut tx, &input).await?;
    tx.commit()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    Ok(Json(SuggestResponse { suggestions }))
}

/// `GET /api/v1/series/suggest`: series names in use on a visible
/// manifestation, ranked prefix-first then by similarity.
///
/// # Errors
/// Same as [`suggest_genres`].
#[utoipa::path(
    get,
    path = "/api/v1/series/suggest",
    tag = "suggest",
    params(SuggestParams),
    security(("session_cookie" = ["read"]), ("device_token_bearer" = ["read"]), ("oidc_jwt_bearer" = ["read"]), ("opds_basic" = ["read"])),
    responses(
        (status = 200, description = "Series names in use on a visible manifestation, ranked prefix-first then by similarity", body = SuggestResponse),
        (status = 400, description = "Malformed query parameter", body = crate::openapi::ProblemDetails),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails),
        (status = 422, description = "Empty or over-length q", body = crate::openapi::ProblemDetails)
    )
)]
async fn suggest_series(
    current_user: CurrentUser,
    State(state): State<AppState>,
    params: Result<Query<SuggestParams>, QueryRejection>,
) -> Result<Json<SuggestResponse>, AppError> {
    let Query(params) = params?;
    let input = prepare(&params)?;

    let mut tx = db::acquire_with_rls(&state.pool, current_user.user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    let suggestions = query_series(&mut tx, &input).await?;
    tx.commit()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    Ok(Json(SuggestResponse { suggestions }))
}

/// `GET /api/v1/publishers/suggest`: distinct publisher values in use on a
/// visible manifestation, ranked prefix-first then by similarity. Every
/// returned [`Suggestion`] carries `id: None` (publishers have no dedicated
/// vocabulary table; see [`query_publishers`]).
///
/// # Errors
/// Same as [`suggest_genres`].
#[utoipa::path(
    get,
    path = "/api/v1/publishers/suggest",
    tag = "suggest",
    params(SuggestParams),
    security(("session_cookie" = ["read"]), ("device_token_bearer" = ["read"]), ("oidc_jwt_bearer" = ["read"]), ("opds_basic" = ["read"])),
    responses(
        (status = 200, description = "Distinct publisher values in use on a visible manifestation, ranked prefix-first then by similarity", body = SuggestResponse),
        (status = 400, description = "Malformed query parameter", body = crate::openapi::ProblemDetails),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails),
        (status = 422, description = "Empty or over-length q", body = crate::openapi::ProblemDetails)
    )
)]
async fn suggest_publishers(
    current_user: CurrentUser,
    State(state): State<AppState>,
    params: Result<Query<SuggestParams>, QueryRejection>,
) -> Result<Json<SuggestResponse>, AppError> {
    let Query(params) = params?;
    let input = prepare(&params)?;

    let mut tx = db::acquire_with_rls(&state.pool, current_user.user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    let suggestions = query_publishers(&mut tx, &input).await?;
    tx.commit()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    Ok(Json(SuggestResponse { suggestions }))
}

#[cfg(test)]
mod tests {
    use axum::http::{HeaderName, HeaderValue, StatusCode};
    use sqlx::PgPool;
    use uuid::Uuid;

    use crate::test_support;

    fn auth(header: &str) -> (HeaderName, HeaderValue) {
        (
            axum::http::header::AUTHORIZATION,
            HeaderValue::from_str(header).expect("ascii auth header"),
        )
    }

    async fn insert_linked_genre(pool: &PgPool, name: &str, manifestation_id: Uuid) -> Uuid {
        let genre_id: Uuid =
            sqlx::query_scalar!("INSERT INTO genres (name) VALUES ($1) RETURNING id", name,)
                .fetch_one(pool)
                .await
                .expect("insert genre");
        sqlx::query!(
            "INSERT INTO manifestation_genres (manifestation_id, genre_id) VALUES ($1, $2)",
            manifestation_id,
            genre_id,
        )
        .execute(pool)
        .await
        .expect("link genre");
        genre_id
    }

    async fn insert_unlinked_genre(pool: &PgPool, name: &str) -> Uuid {
        sqlx::query_scalar!("INSERT INTO genres (name) VALUES ($1) RETURNING id", name)
            .fetch_one(pool)
            .await
            .expect("insert orphan genre")
    }

    async fn insert_linked_mood(pool: &PgPool, name: &str, manifestation_id: Uuid) -> Uuid {
        let mood_id: Uuid =
            sqlx::query_scalar!("INSERT INTO moods (name) VALUES ($1) RETURNING id", name,)
                .fetch_one(pool)
                .await
                .expect("insert mood");
        sqlx::query!(
            "INSERT INTO manifestation_moods (manifestation_id, mood_id) VALUES ($1, $2)",
            manifestation_id,
            mood_id,
        )
        .execute(pool)
        .await
        .expect("link mood");
        mood_id
    }

    async fn insert_linked_tag(pool: &PgPool, name: &str, manifestation_id: Uuid) -> Uuid {
        let tag_id: Uuid =
            sqlx::query_scalar!("INSERT INTO tags (name) VALUES ($1) RETURNING id", name,)
                .fetch_one(pool)
                .await
                .expect("insert tag");
        sqlx::query!(
            "INSERT INTO manifestation_tags (manifestation_id, tag_id) VALUES ($1, $2)",
            manifestation_id,
            tag_id,
        )
        .execute(pool)
        .await
        .expect("link tag");
        tag_id
    }

    async fn insert_linked_series(pool: &PgPool, name: &str, work_id: Uuid) -> Uuid {
        let series_id: Uuid = sqlx::query_scalar!(
            "INSERT INTO series (name, sort_name) VALUES ($1, $1) RETURNING id",
            name,
        )
        .fetch_one(pool)
        .await
        .expect("insert series");
        sqlx::query!(
            "INSERT INTO series_works (series_id, work_id) VALUES ($1, $2)",
            series_id,
            work_id,
        )
        .execute(pool)
        .await
        .expect("link series");
        series_id
    }

    async fn set_publisher(pool: &PgPool, manifestation_id: Uuid, publisher: &str) {
        sqlx::query!(
            "UPDATE manifestations SET publisher = $1 WHERE id = $2",
            publisher,
            manifestation_id,
        )
        .execute(pool)
        .await
        .expect("set publisher");
    }

    fn suggestion_values(body: &serde_json::Value) -> Vec<String> {
        body["suggestions"]
            .as_array()
            .expect("suggestions array")
            .iter()
            .map(|s| s["value"].as_str().expect("value string").to_owned())
            .collect()
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn suggest_requires_auth(pool: PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

        let r = server.get("/api/v1/genres/suggest?q=fan").await;
        assert_eq!(r.status_code(), StatusCode::UNAUTHORIZED);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn genres_prefix_match_ranks_first(pool: PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_work, m_id) =
            test_support::db::insert_work_and_manifestation(&ingestion_pool, "genre-prefix").await;
        insert_linked_genre(&ingestion_pool, "Fantasy", m_id).await;
        let (_user_id, basic) =
            test_support::db::create_adult_and_basic_auth(&app_pool, "genre-prefix").await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

        let r = server
            .get("/api/v1/genres/suggest?q=Fan")
            .add_header(auth(&basic).0, auth(&basic).1)
            .await;
        assert_eq!(r.status_code(), StatusCode::OK, "body: {}", r.text());
        let body: serde_json::Value = r.json();
        let values = suggestion_values(&body);
        assert_eq!(values.first(), Some(&"Fantasy".to_owned()));
        assert!(body["suggestions"][0]["id"].is_string());
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn genres_similarity_match_finds_typo(pool: PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_work, m_id) =
            test_support::db::insert_work_and_manifestation(&ingestion_pool, "genre-typo").await;
        // Fixture vocabulary is a single distinct word; a second, unrelated
        // word would risk the shared-token false-positive hazard trigram
        // similarity has for short strings.
        insert_linked_genre(&ingestion_pool, "Cyberpunk", m_id).await;
        let (_user_id, basic) =
            test_support::db::create_adult_and_basic_auth(&app_pool, "genre-typo").await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

        let r = server
            .get("/api/v1/genres/suggest?q=cybrpunk")
            .add_header(auth(&basic).0, auth(&basic).1)
            .await;
        assert_eq!(r.status_code(), StatusCode::OK, "body: {}", r.text());
        let values = suggestion_values(&r.json());
        assert!(
            values.contains(&"Cyberpunk".to_owned()),
            "typo query should surface Cyberpunk via similarity: {values:?}"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn short_query_is_prefix_only(pool: PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_work, m_id) =
            test_support::db::insert_work_and_manifestation(&ingestion_pool, "genre-short").await;
        insert_linked_genre(&ingestion_pool, "Manga", m_id).await;
        let (_user_id, basic) =
            test_support::db::create_adult_and_basic_auth(&app_pool, "genre-short").await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

        // "an" is not a prefix of "Manga"; under three characters only the
        // prefix leg runs, so a fuzzy/substring hit must not surface it.
        let r = server
            .get("/api/v1/genres/suggest?q=an")
            .add_header(auth(&basic).0, auth(&basic).1)
            .await;
        assert_eq!(r.status_code(), StatusCode::OK, "body: {}", r.text());
        let values = suggestion_values(&r.json());
        assert!(
            values.is_empty(),
            "a 2-char query must not fuzzy-match: {values:?}"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn limit_clamps_to_twenty(pool: PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_work, m_id) =
            test_support::db::insert_work_and_manifestation(&ingestion_pool, "genre-limit").await;
        for i in 0..25 {
            insert_linked_genre(&ingestion_pool, &format!("Genre{i:02}"), m_id).await;
        }
        let (_user_id, basic) =
            test_support::db::create_adult_and_basic_auth(&app_pool, "genre-limit").await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

        let r = server
            .get("/api/v1/genres/suggest?q=Genre&limit=999")
            .add_header(auth(&basic).0, auth(&basic).1)
            .await;
        assert_eq!(r.status_code(), StatusCode::OK, "body: {}", r.text());
        let values = suggestion_values(&r.json());
        assert_eq!(values.len(), 20, "limit must clamp to 20: got {values:?}");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn q_over_cap_and_blank_are_rejected(pool: PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_user_id, basic) =
            test_support::db::create_adult_and_basic_auth(&app_pool, "genre-validate").await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

        let over_cap = "a".repeat(101);
        let r = server
            .get(&format!("/api/v1/genres/suggest?q={over_cap}"))
            .add_header(auth(&basic).0.clone(), auth(&basic).1.clone())
            .await;
        assert_eq!(r.status_code(), StatusCode::UNPROCESSABLE_ENTITY);

        let r = server
            .get("/api/v1/genres/suggest?q=")
            .add_header(auth(&basic).0.clone(), auth(&basic).1.clone())
            .await;
        assert_eq!(r.status_code(), StatusCode::UNPROCESSABLE_ENTITY);

        let r = server
            .get("/api/v1/genres/suggest?q=%20%20%20")
            .add_header(auth(&basic).0, auth(&basic).1)
            .await;
        assert_eq!(r.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn ilike_metacharacters_are_escaped(pool: PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_work, m_id) =
            test_support::db::insert_work_and_manifestation(&ingestion_pool, "genre-escape").await;
        insert_linked_genre(&ingestion_pool, "100% Wool", m_id).await;
        let (_user_id, basic) =
            test_support::db::create_adult_and_basic_auth(&app_pool, "genre-escape").await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

        let r = server
            .get("/api/v1/genres/suggest?q=100%25")
            .add_header(auth(&basic).0.clone(), auth(&basic).1.clone())
            .await;
        assert_eq!(r.status_code(), StatusCode::OK, "body: {}", r.text());
        let values = suggestion_values(&r.json());
        assert!(
            values.contains(&"100% Wool".to_owned()),
            "literal %% must match: {values:?}"
        );

        // Two-char query stays on the prefix-only branch, isolating the
        // escape behavior from the fuzzy leg (which may legitimately
        // trigram-match a longer query like "100_" against "100% Wool").
        // An unescaped underscore would wildcard-match here ("1" + any
        // char prefixes "100% Wool"); the escaped one must not.
        let r = server
            .get("/api/v1/genres/suggest?q=1_")
            .add_header(auth(&basic).0, auth(&basic).1)
            .await;
        assert_eq!(r.status_code(), StatusCode::OK, "body: {}", r.text());
        let values = suggestion_values(&r.json());
        assert!(
            !values.contains(&"100% Wool".to_owned()),
            "escaped _ must not wildcard-match: {values:?}"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn rls_scopes_child_to_shelved_manifestation(pool: PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_work_a, m_a) =
            test_support::db::insert_work_and_manifestation(&ingestion_pool, "rls-a").await;
        let (_work_b, m_b) =
            test_support::db::insert_work_and_manifestation(&ingestion_pool, "rls-b").await;
        insert_linked_genre(&ingestion_pool, "Alpha", m_a).await;
        insert_linked_genre(&ingestion_pool, "Bravo", m_b).await;
        let (child_id, child_basic) =
            test_support::db::create_child_user_and_basic_auth(&app_pool, "rls-child").await;
        let shelf_id = test_support::db::create_shelf(&app_pool, child_id, "Child shelf").await;
        test_support::db::add_to_shelf(&app_pool, shelf_id, m_a).await;
        let (_adult_id, adult_basic) =
            test_support::db::create_adult_and_basic_auth(&app_pool, "rls-adult").await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

        let child_sees_shelved = server
            .get("/api/v1/genres/suggest?q=Alp")
            .add_header(auth(&child_basic).0.clone(), auth(&child_basic).1.clone())
            .await;
        assert!(suggestion_values(&child_sees_shelved.json()).contains(&"Alpha".to_owned()));

        let child_hides_unshelved = server
            .get("/api/v1/genres/suggest?q=Bra")
            .add_header(auth(&child_basic).0, auth(&child_basic).1)
            .await;
        assert!(
            suggestion_values(&child_hides_unshelved.json()).is_empty(),
            "child must not see a genre from an unshelved manifestation"
        );

        let adult_sees_a = server
            .get("/api/v1/genres/suggest?q=Alp")
            .add_header(auth(&adult_basic).0.clone(), auth(&adult_basic).1.clone())
            .await;
        assert!(suggestion_values(&adult_sees_a.json()).contains(&"Alpha".to_owned()));

        let adult_sees_b = server
            .get("/api/v1/genres/suggest?q=Bra")
            .add_header(auth(&adult_basic).0, auth(&adult_basic).1)
            .await;
        assert!(suggestion_values(&adult_sees_b.json()).contains(&"Bravo".to_owned()));
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn publishers_are_distinct_with_no_id(pool: PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_work_a, m_a) =
            test_support::db::insert_work_and_manifestation(&ingestion_pool, "pub-a").await;
        let (_work_b, m_b) =
            test_support::db::insert_work_and_manifestation(&ingestion_pool, "pub-b").await;
        set_publisher(&ingestion_pool, m_a, "Acme Press").await;
        set_publisher(&ingestion_pool, m_b, "Acme Press").await;
        let (_user_id, basic) =
            test_support::db::create_adult_and_basic_auth(&app_pool, "pub-dedupe").await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

        let r = server
            .get("/api/v1/publishers/suggest?q=Acme")
            .add_header(auth(&basic).0, auth(&basic).1)
            .await;
        assert_eq!(r.status_code(), StatusCode::OK, "body: {}", r.text());
        let body: serde_json::Value = r.json();
        let suggestions = body["suggestions"].as_array().expect("array");
        assert_eq!(
            suggestions.len(),
            1,
            "two manifestations sharing a publisher must dedupe to one row: {suggestions:?}"
        );
        assert_eq!(suggestions[0]["value"], "Acme Press");
        assert!(suggestions[0]["id"].is_null());
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn unused_genre_is_excluded(pool: PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
        insert_unlinked_genre(&ingestion_pool, "Orphan").await;
        let (_user_id, basic) =
            test_support::db::create_adult_and_basic_auth(&app_pool, "genre-orphan").await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

        let r = server
            .get("/api/v1/genres/suggest?q=Orp")
            .add_header(auth(&basic).0, auth(&basic).1)
            .await;
        assert_eq!(r.status_code(), StatusCode::OK, "body: {}", r.text());
        assert!(
            suggestion_values(&r.json()).is_empty(),
            "a genre with no manifestation link must never be suggested"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn moods_prefix_smoke(pool: PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_work, m_id) =
            test_support::db::insert_work_and_manifestation(&ingestion_pool, "mood-smoke").await;
        insert_linked_mood(&ingestion_pool, "Cozy", m_id).await;
        let (_user_id, basic) =
            test_support::db::create_adult_and_basic_auth(&app_pool, "mood-smoke").await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

        let r = server
            .get("/api/v1/moods/suggest?q=Coz")
            .add_header(auth(&basic).0, auth(&basic).1)
            .await;
        assert_eq!(r.status_code(), StatusCode::OK, "body: {}", r.text());
        assert!(suggestion_values(&r.json()).contains(&"Cozy".to_owned()));
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn tags_prefix_smoke(pool: PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_work, m_id) =
            test_support::db::insert_work_and_manifestation(&ingestion_pool, "tag-smoke").await;
        insert_linked_tag(&ingestion_pool, "Signed", m_id).await;
        let (_user_id, basic) =
            test_support::db::create_adult_and_basic_auth(&app_pool, "tag-smoke").await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

        let r = server
            .get("/api/v1/tags/suggest?q=Sig")
            .add_header(auth(&basic).0, auth(&basic).1)
            .await;
        assert_eq!(r.status_code(), StatusCode::OK, "body: {}", r.text());
        assert!(suggestion_values(&r.json()).contains(&"Signed".to_owned()));
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn authors_prefix_smoke(pool: PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (work_id, _m_id) =
            test_support::db::insert_work_and_manifestation(&ingestion_pool, "author-smoke").await;
        test_support::db::insert_contributor(&ingestion_pool, work_id, "Ann Leckie", "author", 0)
            .await;
        let (_user_id, basic) =
            test_support::db::create_adult_and_basic_auth(&app_pool, "author-smoke").await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

        let r = server
            .get("/api/v1/authors/suggest?q=Ann")
            .add_header(auth(&basic).0, auth(&basic).1)
            .await;
        assert_eq!(r.status_code(), StatusCode::OK, "body: {}", r.text());
        assert!(suggestion_values(&r.json()).contains(&"Ann Leckie".to_owned()));
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn series_prefix_smoke(pool: PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (work_id, _m_id) =
            test_support::db::insert_work_and_manifestation(&ingestion_pool, "series-smoke").await;
        insert_linked_series(&ingestion_pool, "Foundation", work_id).await;
        let (_user_id, basic) =
            test_support::db::create_adult_and_basic_auth(&app_pool, "series-smoke").await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

        let r = server
            .get("/api/v1/series/suggest?q=Fou")
            .add_header(auth(&basic).0, auth(&basic).1)
            .await;
        assert_eq!(r.status_code(), StatusCode::OK, "body: {}", r.text());
        assert!(suggestion_values(&r.json()).contains(&"Foundation".to_owned()));
    }
}
