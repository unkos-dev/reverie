//! `/api/v1/{genres,moods,tags,authors,series,publishers}/suggest` typeahead endpoints.
//!
//! Each is a prefix-then-similarity lookup over the metadata vocabularies,
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
//! - `q` is matched case-insensitively via `ILIKE`, and a caller cannot
//!   smuggle wildcard metacharacters into the prefix match: the raw input is
//!   bound as-is and `immutable_unaccent_like` backslash-escapes `%`/`_`/`\`
//!   in SQL, after accent folding. The order is load-bearing: the unaccent
//!   dictionary folds some Unicode punctuation (fullwidth `％`/`＿`/`＼`)
//!   into those very metacharacters, so an escape applied before folding
//!   would be undone by it. Postgres's default `LIKE`/`ILIKE` escape
//!   character is already backslash, so no explicit `ESCAPE` clause is
//!   needed. A needle the dictionary erases entirely (combining marks fold
//!   to the empty string) nulls the pattern via `nullif` and matches
//!   nothing rather than everything.
//! - Every match is also accent-insensitive: the vocabulary column is
//!   wrapped in `immutable_unaccent` and the bound query text folds through
//!   the same dictionary (the `ILIKE` prefix leg via
//!   `immutable_unaccent_like`, the `word_similarity`/`<%` fuzzy leg via
//!   `immutable_unaccent`), so `garcia` finds `García` without the caller
//!   stripping diacritics itself. Stored values are untouched; folding
//!   happens only at match time.

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
        .routes(routes!(authors_by_id))
}

/// Upper bound on `?id=` repetitions for the authors batch-get. Chip hydration
/// resolves the union of a book filter's `author`, `author_any`, and
/// `author_none` sets; each is capped at 20, so 60 is the largest set the
/// filter grammar yields. Resolving the whole union in one request (rather than
/// truncating) keeps every chip labelled while the `= ANY($)` array stays
/// bounded by construction.
const MAX_RESOLVE_IDS: usize = 60;

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

/// `?id=` repeated query parameter for the authors batch-get. Required by
/// construction: an absent or empty `id` is a 422, never an unbounded list.
#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
struct AuthorsByIdParams {
    /// Author ids to resolve, repeated (`?id=<uuid>&id=<uuid>`). Required
    /// (empty is 422) and capped at [`MAX_RESOLVE_IDS`]; unknown or
    /// out-of-scope ids are silently omitted rather than 404'd.
    #[serde(default)]
    id: Vec<Uuid>,
}

/// `GET /api/v1/authors` response envelope: the resolved author labels for
/// the requested ids, as a plain collection.
#[derive(Debug, Serialize, utoipa::ToSchema)]
struct AuthorsResponse {
    items: Vec<Suggestion>,
}

/// Validated, normalized inputs shared by every entity's query pair, so each
/// handler validates once and the six query functions take one argument.
struct SuggestInput {
    /// Raw trimmed `q`, bound as-is by both legs: the `ILIKE` prefix leg
    /// escapes it in SQL after folding (`immutable_unaccent_like`), and the
    /// `word_similarity` leg operates on the literal text.
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
        raw: raw.to_owned(),
        short: raw.chars().count() < FUZZY_MIN_CHARS,
        limit,
    })
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
                  AND immutable_unaccent(g.name) ILIKE nullif(immutable_unaccent_like($1), '') || '%'
                ORDER BY g.name ASC
                LIMIT $2"#,
            q.raw,
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
                  AND (immutable_unaccent(g.name) ILIKE nullif(immutable_unaccent_like($1), '') || '%'
                       OR immutable_unaccent($1) <% immutable_unaccent(g.name))
                ORDER BY (immutable_unaccent(g.name) ILIKE nullif(immutable_unaccent_like($1), '') || '%') DESC,
                         word_similarity(immutable_unaccent($1), immutable_unaccent(g.name)) DESC,
                         g.name ASC
                LIMIT $2"#,
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
                  AND immutable_unaccent(mo.name) ILIKE nullif(immutable_unaccent_like($1), '') || '%'
                ORDER BY mo.name ASC
                LIMIT $2"#,
            q.raw,
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
                  AND (immutable_unaccent(mo.name) ILIKE nullif(immutable_unaccent_like($1), '') || '%'
                       OR immutable_unaccent($1) <% immutable_unaccent(mo.name))
                ORDER BY (immutable_unaccent(mo.name) ILIKE nullif(immutable_unaccent_like($1), '') || '%') DESC,
                         word_similarity(immutable_unaccent($1), immutable_unaccent(mo.name)) DESC,
                         mo.name ASC
                LIMIT $2"#,
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
                  AND immutable_unaccent(t.name) ILIKE nullif(immutable_unaccent_like($1), '') || '%'
                ORDER BY t.name ASC
                LIMIT $2"#,
            q.raw,
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
                  AND (immutable_unaccent(t.name) ILIKE nullif(immutable_unaccent_like($1), '') || '%'
                       OR immutable_unaccent($1) <% immutable_unaccent(t.name))
                ORDER BY (immutable_unaccent(t.name) ILIKE nullif(immutable_unaccent_like($1), '') || '%') DESC,
                         word_similarity(immutable_unaccent($1), immutable_unaccent(t.name)) DESC,
                         t.name ASC
                LIMIT $2"#,
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
/// `manifestations.work_id`. Only the `author` role counts, matching the
/// `?author=` list filter and the `authors[]` the list/detail payloads
/// display, so the suggest, filter, and chip-label surfaces agree on one
/// author set (editors/translators/narrators are excluded).
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
                         WHERE wa.author_id = a.id AND wa.role = 'author'
                      )
                  AND immutable_unaccent(a.name) ILIKE nullif(immutable_unaccent_like($1), '') || '%'
                ORDER BY a.name ASC
                LIMIT $2"#,
            q.raw,
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
                         WHERE wa.author_id = a.id AND wa.role = 'author'
                      )
                  AND (immutable_unaccent(a.name) ILIKE nullif(immutable_unaccent_like($1), '') || '%'
                       OR immutable_unaccent($1) <% immutable_unaccent(a.name))
                ORDER BY (immutable_unaccent(a.name) ILIKE nullif(immutable_unaccent_like($1), '') || '%') DESC,
                         word_similarity(immutable_unaccent($1), immutable_unaccent(a.name)) DESC,
                         a.name ASC
                LIMIT $2"#,
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
                  AND immutable_unaccent(s.name) ILIKE nullif(immutable_unaccent_like($1), '') || '%'
                ORDER BY s.name ASC
                LIMIT $2"#,
            q.raw,
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
                  AND (immutable_unaccent(s.name) ILIKE nullif(immutable_unaccent_like($1), '') || '%'
                       OR immutable_unaccent($1) <% immutable_unaccent(s.name))
                ORDER BY (immutable_unaccent(s.name) ILIKE nullif(immutable_unaccent_like($1), '') || '%') DESC,
                         word_similarity(immutable_unaccent($1), immutable_unaccent(s.name)) DESC,
                         s.name ASC
                LIMIT $2"#,
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
                WHERE immutable_unaccent(name) ILIKE nullif(immutable_unaccent_like($1), '') || '%'
                ORDER BY name ASC
                LIMIT $2"#,
            q.raw,
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
                WHERE immutable_unaccent(name) ILIKE nullif(immutable_unaccent_like($1), '') || '%'
                   OR immutable_unaccent($1) <% immutable_unaccent(name)
                ORDER BY (immutable_unaccent(name) ILIKE nullif(immutable_unaccent_like($1), '') || '%') DESC,
                         word_similarity(immutable_unaccent($1), immutable_unaccent(name)) DESC,
                         name ASC
                LIMIT $2"#,
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

/// Author names in use on a visible manifestation, ranked prefix-first then by similarity.
///
/// Scoped to the `author` role (see [`query_authors`]), matching the
/// `?author=` filter it feeds.
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

/// Resolve author ids to display names so a restored filter chip can label itself.
///
/// The list payload carries author names, not ids, and a none-of filter hides
/// the author's rows entirely, so a chip restored from a bookmarked URL has
/// nothing else to label itself with.
///
/// The `id` filter is required and capped, keeping this route bounded by
/// construction: it is the seed of a future paginated authors collection,
/// but until that lands a bare `GET /api/v1/authors` is a 422, not a
/// list-all. Ids are RLS-scoped through the same in-use join as
/// `authors/suggest` and restricted to the `author` role, so the chip label
/// resolves the same author set the `?author=` filter matches; an id the
/// caller cannot see (or one that is not an author) is silently omitted
/// rather than 404'd, and unknown ids drop out the same way.
///
/// # Errors
/// - [`AppError::MalformedQuery`] when an `id` value is not a UUID.
/// - [`AppError::Validation`] when `id` is absent/empty or over
///   [`MAX_RESOLVE_IDS`] values.
/// - [`AppError::Internal`] on database errors.
#[utoipa::path(
    get,
    path = "/api/v1/authors",
    tag = "suggest",
    params(AuthorsByIdParams),
    security(("session_cookie" = ["read"]), ("device_token_bearer" = ["read"]), ("oidc_jwt_bearer" = ["read"]), ("opds_basic" = ["read"])),
    responses(
        (status = 200, description = "Resolved author labels for the requested ids", body = AuthorsResponse),
        (status = 400, description = "Malformed id parameter", body = crate::openapi::ProblemDetails),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails),
        (status = 422, description = "Missing id filter or too many ids", body = crate::openapi::ProblemDetails)
    )
)]
async fn authors_by_id(
    current_user: CurrentUser,
    State(state): State<AppState>,
    params: Result<Query<AuthorsByIdParams>, QueryRejection>,
) -> Result<Json<AuthorsResponse>, AppError> {
    let Query(params) = params?;
    if params.id.is_empty() {
        return Err(AppError::Validation("id filter is required".into()));
    }
    if params.id.len() > MAX_RESOLVE_IDS {
        return Err(AppError::Validation(format!(
            "too many id filters: maximum {MAX_RESOLVE_IDS}"
        )));
    }
    let mut ids = params.id.clone();
    ids.sort_unstable();
    ids.dedup();

    let mut tx = db::acquire_with_rls(&state.pool, current_user.user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    let rows = sqlx::query_as!(
        VocabRow,
        r#"SELECT a.id, a.name
             FROM authors a
            WHERE a.id = ANY($1)
              AND EXISTS (
                      SELECT 1 FROM work_authors wa
                      JOIN manifestations m ON m.work_id = wa.work_id
                     WHERE wa.author_id = a.id AND wa.role = 'author'
                  )
            ORDER BY a.name ASC"#,
        &ids,
    )
    .fetch_all(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    tx.commit()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let items = rows.into_iter().map(Suggestion::from).collect();
    Ok(Json(AuthorsResponse { items }))
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

/// Distinct publisher values in use on a visible manifestation, ranked prefix-first then by similarity.
///
/// Every returned [`Suggestion`] carries `id: None` (publishers have no
/// dedicated vocabulary table; see [`query_publishers`]).
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

    use super::MAX_RESOLVE_IDS;
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
    async fn genres_diacritic_folding_matches_unaccented_query(pool: PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_work, m_id) =
            test_support::db::insert_work_and_manifestation(&ingestion_pool, "genre-accent").await;
        insert_linked_genre(&ingestion_pool, "\u{c9}poque", m_id).await;
        let (_user_id, basic) =
            test_support::db::create_adult_and_basic_auth(&app_pool, "genre-accent").await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

        // Unaccented query finds the accented genre name.
        let r = server
            .get("/api/v1/genres/suggest?q=epoque")
            .add_header(auth(&basic).0.clone(), auth(&basic).1.clone())
            .await;
        assert_eq!(r.status_code(), StatusCode::OK, "body: {}", r.text());
        let values = suggestion_values(&r.json());
        assert!(
            values.contains(&"\u{c9}poque".to_owned()),
            "unaccented query should surface \u{c9}poque via folding: {values:?}"
        );

        // Exact accented query still matches (no regression from folding).
        let r = server
            .get("/api/v1/genres/suggest?q=%C3%89poque")
            .add_header(auth(&basic).0, auth(&basic).1)
            .await;
        assert_eq!(r.status_code(), StatusCode::OK, "body: {}", r.text());
        let values = suggestion_values(&r.json());
        assert!(
            values.contains(&"\u{c9}poque".to_owned()),
            "accented query should still match \u{c9}poque: {values:?}"
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

        // unaccent folds fullwidth ％/＿/＼ (U+FF05/U+FF3F/U+FF3C) into LIKE
        // metacharacters, so folding must not re-materialize wildcards from an
        // already-escaped needle. Each probe stays under three characters so
        // only the prefix leg runs.
        insert_linked_genre(&ingestion_pool, "%wool blend", m_id).await;
        insert_linked_genre(&ingestion_pool, r"\backdated", m_id).await;
        insert_linked_genre(&ingestion_pool, "Wildcard Bait", m_id).await;

        // ％ folds to %: a literal prefix match, not match-everything.
        let r = server
            .get("/api/v1/genres/suggest?q=%EF%BC%85")
            .add_header(auth(&basic).0, auth(&basic).1)
            .await;
        assert_eq!(r.status_code(), StatusCode::OK, "body: {}", r.text());
        let values = suggestion_values(&r.json());
        assert_eq!(
            values,
            vec!["%wool blend".to_owned()],
            "folded % must prefix-match only the literal-% genre"
        );

        // ＿ folds to _: no genre starts with a literal underscore, and a
        // smuggled single-character wildcard would match every genre.
        let r = server
            .get("/api/v1/genres/suggest?q=%EF%BC%BF")
            .add_header(auth(&basic).0, auth(&basic).1)
            .await;
        assert_eq!(r.status_code(), StatusCode::OK, "body: {}", r.text());
        let values = suggestion_values(&r.json());
        assert_eq!(
            values,
            Vec::<String>::new(),
            "folded _ must not wildcard-match"
        );

        // ＼ folds to \ and must itself be escaped so it matches the literal
        // backslash prefix.
        let r = server
            .get("/api/v1/genres/suggest?q=%EF%BC%BC")
            .add_header(auth(&basic).0, auth(&basic).1)
            .await;
        assert_eq!(r.status_code(), StatusCode::OK, "body: {}", r.text());
        let values = suggestion_values(&r.json());
        assert_eq!(
            values,
            vec![r"\backdated".to_owned()],
            "folded backslash must match the literal-backslash genre"
        );

        // A needle that folds to nothing (combining marks map to the empty
        // string in unaccent.rules) must suggest nothing, not the unfiltered
        // vocabulary head.
        let r = server
            .get("/api/v1/genres/suggest?q=%CC%81")
            .add_header(auth(&basic).0, auth(&basic).1)
            .await;
        assert_eq!(r.status_code(), StatusCode::OK, "body: {}", r.text());
        let values = suggestion_values(&r.json());
        assert_eq!(
            values,
            Vec::<String>::new(),
            "an empty fold must not match everything"
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
    async fn authors_suggest_diacritic_folding_matches_unaccented_prefix(pool: PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (work_id, _m_id) =
            test_support::db::insert_work_and_manifestation(&ingestion_pool, "author-accent").await;
        test_support::db::insert_contributor(
            &ingestion_pool,
            work_id,
            "Gabriel Garc\u{ed}a M\u{e1}rquez",
            "author",
            0,
        )
        .await;
        let (_user_id, basic) =
            test_support::db::create_adult_and_basic_auth(&app_pool, "author-accent").await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

        // Unaccented prefix rides the folded ILIKE-prefix leg.
        let r = server
            .get("/api/v1/authors/suggest?q=Gabriel%20Garcia")
            .add_header(auth(&basic).0.clone(), auth(&basic).1.clone())
            .await;
        assert_eq!(r.status_code(), StatusCode::OK, "body: {}", r.text());
        let values = suggestion_values(&r.json());
        assert!(
            values.contains(&"Gabriel Garc\u{ed}a M\u{e1}rquez".to_owned()),
            "unaccented prefix must find the accented author: {values:?}"
        );

        // Exact accented prefix still matches (no regression from folding).
        let r = server
            .get("/api/v1/authors/suggest?q=Gabriel%20Garc%C3%ADa")
            .add_header(auth(&basic).0, auth(&basic).1)
            .await;
        assert_eq!(r.status_code(), StatusCode::OK, "body: {}", r.text());
        let values = suggestion_values(&r.json());
        assert!(
            values.contains(&"Gabriel Garc\u{ed}a M\u{e1}rquez".to_owned()),
            "accented prefix must still match: {values:?}"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn author_endpoints_exclude_non_author_roles(pool: PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (work_id, _m_id) =
            test_support::db::insert_work_and_manifestation(&ingestion_pool, "role-scope").await;
        let author_id = test_support::db::insert_contributor(
            &ingestion_pool,
            work_id,
            "Rowan Wells",
            "author",
            0,
        )
        .await;
        let editor_id = test_support::db::insert_contributor(
            &ingestion_pool,
            work_id,
            "Rowan Vane",
            "editor",
            1,
        )
        .await;
        let (_user_id, basic) =
            test_support::db::create_adult_and_basic_auth(&app_pool, "role-scope").await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

        // Typeahead: the shared "Rowan" prefix matches both names, but the
        // editor must not be offered as a pickable author.
        let suggest = server
            .get("/api/v1/authors/suggest?q=Rowan")
            .add_header(auth(&basic).0, auth(&basic).1)
            .await;
        assert_eq!(
            suggest.status_code(),
            StatusCode::OK,
            "body: {}",
            suggest.text()
        );
        let suggested = suggestion_values(&suggest.json());
        assert!(
            suggested.contains(&"Rowan Wells".to_owned()),
            "{suggested:?}"
        );
        assert!(
            !suggested.contains(&"Rowan Vane".to_owned()),
            "editor must not be suggested as an author: {suggested:?}"
        );

        // Resolve: both ids requested; only the author id resolves, so a chip
        // never labels an id the filter cannot match.
        let resolve = server
            .get(&format!("/api/v1/authors?id={author_id}&id={editor_id}"))
            .add_header(auth(&basic).0, auth(&basic).1)
            .await;
        assert_eq!(
            resolve.status_code(),
            StatusCode::OK,
            "body: {}",
            resolve.text()
        );
        let resolved = author_values(&resolve.json());
        assert_eq!(
            resolved,
            vec!["Rowan Wells".to_owned()],
            "editor id must be omitted from resolution: {resolved:?}"
        );
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

    /// The `items[]` labels from an authors batch-get response body.
    fn author_values(body: &serde_json::Value) -> Vec<String> {
        body["items"]
            .as_array()
            .expect("items array")
            .iter()
            .map(|s| s["value"].as_str().expect("value string").to_owned())
            .collect()
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn authors_by_id_requires_auth(pool: PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

        let r = server
            .get(&format!("/api/v1/authors?id={}", Uuid::new_v4()))
            .await;
        assert_eq!(r.status_code(), StatusCode::UNAUTHORIZED);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn authors_by_id_requires_id_filter(pool: PgPool) {
        // A bare `GET /api/v1/authors` must never list all authors: the id
        // filter is required, so the endpoint stays bounded by construction.
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_user_id, basic) =
            test_support::db::create_adult_and_basic_auth(&app_pool, "authors-req").await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

        let r = server
            .get("/api/v1/authors")
            .add_header(auth(&basic).0, auth(&basic).1)
            .await;
        assert_eq!(
            r.status_code(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "body: {}",
            r.text()
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn authors_by_id_over_cap_is_rejected(pool: PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_user_id, basic) =
            test_support::db::create_adult_and_basic_auth(&app_pool, "authors-cap").await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

        let ids = (0..=MAX_RESOLVE_IDS)
            .map(|_| format!("id={}", Uuid::new_v4()))
            .collect::<Vec<_>>()
            .join("&");
        let r = server
            .get(&format!("/api/v1/authors?{ids}"))
            .add_header(auth(&basic).0, auth(&basic).1)
            .await;
        assert_eq!(
            r.status_code(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "body: {}",
            r.text()
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn authors_by_id_accepts_the_full_cap(pool: PgPool) {
        // Exactly MAX_RESOLVE_IDS ids is accepted (the boundary the over-cap
        // test rejects at +1), so the union of a 20-per-set author filter across
        // all/any/none resolves in one request. Unknown ids drop to an empty set
        // rather than 422.
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_user_id, basic) =
            test_support::db::create_adult_and_basic_auth(&app_pool, "authors-cap-ok").await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

        let ids = (0..MAX_RESOLVE_IDS)
            .map(|_| format!("id={}", Uuid::new_v4()))
            .collect::<Vec<_>>()
            .join("&");
        let r = server
            .get(&format!("/api/v1/authors?{ids}"))
            .add_header(auth(&basic).0, auth(&basic).1)
            .await;
        assert_eq!(r.status_code(), StatusCode::OK, "body: {}", r.text());
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn authors_by_id_resolves_known_and_omits_unknown(pool: PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (work_a, _m_a) =
            test_support::db::insert_work_and_manifestation(&ingestion_pool, "authres-a").await;
        let (work_b, _m_b) =
            test_support::db::insert_work_and_manifestation(&ingestion_pool, "authres-b").await;
        let author_a = test_support::db::insert_contributor(
            &ingestion_pool,
            work_a,
            "Ursula Le Guin",
            "author",
            0,
        )
        .await;
        let author_b = test_support::db::insert_contributor(
            &ingestion_pool,
            work_b,
            "Gene Wolfe",
            "author",
            0,
        )
        .await;
        let unknown = Uuid::new_v4();
        let (_user_id, basic) =
            test_support::db::create_adult_and_basic_auth(&app_pool, "authres").await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

        let r = server
            .get(&format!(
                "/api/v1/authors?id={author_a}&id={author_b}&id={unknown}"
            ))
            .add_header(auth(&basic).0, auth(&basic).1)
            .await;
        assert_eq!(r.status_code(), StatusCode::OK, "body: {}", r.text());
        let values = author_values(&r.json());
        assert!(values.contains(&"Ursula Le Guin".to_owned()), "{values:?}");
        assert!(values.contains(&"Gene Wolfe".to_owned()), "{values:?}");
        assert_eq!(
            values.len(),
            2,
            "unknown id must be omitted, not 404: {values:?}"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn authors_by_id_omits_out_of_rls_scope(pool: PgPool) {
        // An author linked only through a manifestation the caller cannot see
        // is omitted, mirroring the suggest endpoint's RLS scoping.
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (work_a, m_a) =
            test_support::db::insert_work_and_manifestation(&ingestion_pool, "authrls-a").await;
        let (work_b, _m_b) =
            test_support::db::insert_work_and_manifestation(&ingestion_pool, "authrls-b").await;
        let author_shelved = test_support::db::insert_contributor(
            &ingestion_pool,
            work_a,
            "Shelved Scribe",
            "author",
            0,
        )
        .await;
        let author_hidden = test_support::db::insert_contributor(
            &ingestion_pool,
            work_b,
            "Hidden Hand",
            "author",
            0,
        )
        .await;
        let (child_id, child_basic) =
            test_support::db::create_child_user_and_basic_auth(&app_pool, "authrls-child").await;
        let shelf_id = test_support::db::create_shelf(&app_pool, child_id, "Child shelf").await;
        test_support::db::add_to_shelf(&app_pool, shelf_id, m_a).await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

        let r = server
            .get(&format!(
                "/api/v1/authors?id={author_shelved}&id={author_hidden}"
            ))
            .add_header(auth(&child_basic).0, auth(&child_basic).1)
            .await;
        assert_eq!(r.status_code(), StatusCode::OK, "body: {}", r.text());
        let values = author_values(&r.json());
        assert!(values.contains(&"Shelved Scribe".to_owned()), "{values:?}");
        assert!(
            !values.contains(&"Hidden Hand".to_owned()),
            "child must not resolve an author from an unshelved manifestation: {values:?}"
        );
    }
}
