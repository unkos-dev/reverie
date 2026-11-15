use crate::auth::middleware::CurrentUser;
use crate::{db, error::AppError, state::AppState};
use serde::Serialize;
use sqlx::Postgres;
use sqlx::Transaction;
use uuid::Uuid;
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

/// One typeahead suggestion.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct Suggestion {
    /// Row id in the backing vocabulary table. `None` for publisher
    /// suggestions, which have no dedicated table of their own.
    pub id: Option<Uuid>,
    /// Display text to show in the typeahead and to resubmit as a filter.
    pub value: String,
}

/// Validated, normalized inputs shared by every entity's query pair, so each
/// handler validates once and the six query functions take one argument.
struct SuggestInput {
    /// Raw trimmed `q`, bound as-is by both legs: the `ILIKE` prefix leg
    /// escapes it in SQL after folding (`immutable_unaccent_like`), and the
    /// `word_similarity` leg operates on the literal text.
    pub raw: String,
    /// `true` when `raw` is under [`FUZZY_MIN_CHARS`]: prefix-only query.
    pub short: bool,
    /// Clamped row cap, bound into every query's `LIMIT`.
    pub limit: i64,
}

/// Row shape shared by the five vocabulary-table entities (genres, moods,
/// tags, authors, series): a stable id plus the display name.
struct VocabRow {
    pub id: Uuid,
    pub name: String,
}

impl From<VocabRow> for Suggestion {
    fn from(row: VocabRow) -> Self {
        Self {
            id: Some(row.id),
            value: row.name,
        }
    }
}

/// Trim, length-validate, and clamp the limit for `params`, shared by every
/// handler below.
///
/// # Errors
/// Returns [`AppError::Validation`] when `q` is empty after trimming or
/// exceeds [`MAX_Q_CHARS`] characters.
fn prepare_suggestions(q: &str, limit: Option<i64>) -> Result<SuggestInput, AppError> {
    let raw = q.trim();
    if raw.is_empty() {
        return Err(AppError::Validation("q must not be empty".into()));
    }
    if raw.chars().count() > MAX_Q_CHARS {
        return Err(AppError::Validation(format!(
            "q must be at most {MAX_Q_CHARS} characters"
        )));
    }
    let limit = suggestion_limit(limit);
    Ok(SuggestInput {
        raw: raw.to_owned(),
        short: raw.chars().count() < FUZZY_MIN_CHARS,
        limit,
    })
}

const FUZZY_MIN_CHARS: usize = 3;

const MAX_Q_CHARS: usize = 100;

pub fn suggestion_limit(limit: Option<i64>) -> i64 {
    limit.unwrap_or(DEFAULT_LIMIT).clamp(MIN_LIMIT, MAX_LIMIT)
}

const DEFAULT_LIMIT: i64 = 10;

const MIN_LIMIT: i64 = 1;

const MAX_LIMIT: i64 = 20;

pub async fn suggest_genres(
    current_user: &CurrentUser,
    state: &AppState,
    q: &str,
    limit: Option<i64>,
) -> Result<Vec<Suggestion>, AppError> {
    let input = prepare_suggestions(q, limit)?;
    let mut tx = db::acquire_with_rls(&state.pool, current_user.user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    let suggestions = query_genres(&mut tx, &input).await?;
    tx.commit()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    Ok(suggestions)
}

pub async fn suggest_moods(
    current_user: &CurrentUser,
    state: &AppState,
    q: &str,
    limit: Option<i64>,
) -> Result<Vec<Suggestion>, AppError> {
    let input = prepare_suggestions(q, limit)?;
    let mut tx = db::acquire_with_rls(&state.pool, current_user.user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    let suggestions = query_moods(&mut tx, &input).await?;
    tx.commit()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    Ok(suggestions)
}

pub async fn suggest_tags(
    current_user: &CurrentUser,
    state: &AppState,
    q: &str,
    limit: Option<i64>,
) -> Result<Vec<Suggestion>, AppError> {
    let input = prepare_suggestions(q, limit)?;
    let mut tx = db::acquire_with_rls(&state.pool, current_user.user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    let suggestions = query_tags(&mut tx, &input).await?;
    tx.commit()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    Ok(suggestions)
}

pub async fn suggest_authors(
    current_user: &CurrentUser,
    state: &AppState,
    q: &str,
    limit: Option<i64>,
) -> Result<Vec<Suggestion>, AppError> {
    let input = prepare_suggestions(q, limit)?;
    let mut tx = db::acquire_with_rls(&state.pool, current_user.user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    let suggestions = query_authors(&mut tx, &input).await?;
    tx.commit()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    Ok(suggestions)
}

pub async fn suggest_series(
    current_user: &CurrentUser,
    state: &AppState,
    q: &str,
    limit: Option<i64>,
) -> Result<Vec<Suggestion>, AppError> {
    let input = prepare_suggestions(q, limit)?;
    let mut tx = db::acquire_with_rls(&state.pool, current_user.user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    let suggestions = query_series(&mut tx, &input).await?;
    tx.commit()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    Ok(suggestions)
}

pub async fn suggest_publishers(
    current_user: &CurrentUser,
    state: &AppState,
    q: &str,
    limit: Option<i64>,
) -> Result<Vec<Suggestion>, AppError> {
    let input = prepare_suggestions(q, limit)?;
    let mut tx = db::acquire_with_rls(&state.pool, current_user.user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    let suggestions = query_publishers(&mut tx, &input).await?;
    tx.commit()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    Ok(suggestions)
}

pub async fn author_labels(
    current_user: &CurrentUser,
    state: &AppState,
    ids: &[Uuid],
) -> Result<Vec<Suggestion>, AppError> {
    validate_label_count(ids.len())?;
    let mut ids = ids.to_vec();
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
    Ok(items)
}

pub async fn series_labels(
    current_user: &CurrentUser,
    state: &AppState,
    ids: &[Uuid],
) -> Result<Vec<SeriesLabel>, AppError> {
    validate_label_count(ids.len())?;
    let mut ids = ids.to_vec();
    ids.sort_unstable();
    ids.dedup();
    let mut tx = db::acquire_with_rls(&state.pool, current_user.user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    let rows = sqlx::query_as!(
        VocabRow,
        r#"SELECT s.id, s.name
             FROM series s
            WHERE s.id = ANY($1)
              AND EXISTS (
                      SELECT 1 FROM series_works sw
                      JOIN manifestations m ON m.work_id = sw.work_id
                     WHERE sw.series_id = s.id
                  )
            ORDER BY s.name ASC"#,
        &ids,
    )
    .fetch_all(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    tx.commit()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    let items = rows
        .into_iter()
        .map(|row| SeriesLabel {
            id: row.id,
            value: row.name,
        })
        .collect();
    Ok(items)
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct SeriesLabel {
    pub id: Uuid,
    pub value: String,
}

pub fn validate_label_count(size: usize) -> Result<(), AppError> {
    if size == 0 {
        return Err(AppError::Validation("id filter is required".into()));
    }
    if size > MAX_RESOLVE_IDS {
        return Err(AppError::Validation(format!(
            "too many id filters: maximum {MAX_RESOLVE_IDS}"
        )));
    }
    Ok(())
}

const MAX_RESOLVE_IDS: usize = 60;
