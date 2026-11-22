use crate::services::reading::ReadingStateRow;
mod suggest;

pub use suggest::{
    SeriesLabel, Suggestion, author_labels, series_labels, suggest_authors, suggest_genres,
    suggest_moods, suggest_publishers, suggest_series, suggest_tags, suggestion_limit,
    validate_label_count,
};

use crate::auth::middleware::CurrentUser;
use crate::models::library::{
    BookDetail, BookListRow, ContributorRef, ExternalIdRef, ExternalRatingRef, MetadataVersionRow,
    MetadataVersionSummary, SeriesRef, WorkDetail, WorkManifestation,
};
use crate::models::series::{SeriesDetail, SeriesWork};
use crate::models::{
    content_rating::ContentRating, contributor_role::ContributorRole,
    enrichment_status::EnrichmentStatus, external_identifier::IdentifierLevel,
    ingestion_status::IngestionStatus, reading_state::ReadingStateSummary,
    reading_status::ReadingStatus, validation_status::ValidationStatus,
};
use crate::routes::{
    cursor::{CursorApi, CursorValue, SortCursor},
    etag::MetadataRevision,
    library::{ListParams, filters},
    sort_spec::{SortColumn, SortDirection, SortSpec},
};
use crate::{db, error::AppError, state::AppState};
use jiff::Timestamp;
use serde::Serialize;
use sqlx::{Postgres, QueryBuilder, Row};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;
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
pub const MAX_PENDING_VERSIONS: u16 = 200;

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

fn series_ref_from_row(r: &sqlx::postgres::PgRow) -> Result<Option<SeriesRef>, AppError> {
    // Fallible decode at the same loud-but-handled boundary as
    // validation_status above: a column type drift here must surface as a
    // 500, not silently drop series from every list row. Swallowing the Err
    // arm is what hid the NUMERIC decode mismatch on this exact path.
    let id: Option<Uuid> = r
        .try_get("series_id")
        .map_err(|e| AppError::Internal(anyhow::anyhow!("series_id decode failed: {e}")))?;
    let name: Option<String> = r
        .try_get("series_name")
        .map_err(|e| AppError::Internal(anyhow::anyhow!("series_name decode failed: {e}")))?;
    let position: Option<f64> = r
        .try_get("series_position")
        .map_err(|e| AppError::Internal(anyhow::anyhow!("series_position decode failed: {e}")))?;
    Ok(match (id, name) {
        (Some(id), Some(name)) => Some(SeriesRef { id, name, position }),
        _ => None,
    })
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
    qb.push(" AND ");
    push_cursor_expression(qb, spec, cursor);
}

fn push_cursor_expression(qb: &mut QueryBuilder<Postgres>, spec: &SortSpec, cursor: &SortCursor) {
    let levels = spec.levels();
    let tiebreak_dir = levels
        .last()
        .map_or(SortDirection::Desc, |level| level.direction);

    qb.push("(");
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
            qb.push_bind(jiff_sqlx::Timestamp::from(*ts));
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

/// Build the `next_cursor` boundary for `row` under `spec`: one typed
/// value per level (read from `select_alias()`) plus the `m.id`
/// tiebreaker, stamped with `filter_fp` so the following page's request
/// (which carries the same filters) validates the replayed cursor. Uses
/// `try_get`, not the panicking `Row::get`; this dynamic `QueryBuilder`
/// path has no compile-time schema check, so a decode failure becomes a
/// clean 500 instead of a panic (mirrors the `validation_status` decode
/// above).
fn next_cursor_for_row(
    row: &sqlx::postgres::PgRow,
    spec: &SortSpec,
    filter_fp: &str,
) -> Result<SortCursor, AppError> {
    let mut keys = Vec::with_capacity(spec.levels().len());
    for level in spec.levels() {
        let alias = level.column.select_alias();
        let value = match level.column {
            SortColumn::Title => CursorValue::Text(Some(decode_cursor_column(row, alias)?)),
            SortColumn::Author => CursorValue::Text(decode_cursor_column(row, alias)?),
            SortColumn::CreatedAt => {
                CursorValue::Ts(decode_cursor_column::<jiff_sqlx::Timestamp>(row, alias)?.to_jiff())
            }
            SortColumn::Pages => CursorValue::Int(decode_cursor_column(row, alias)?),
        };
        keys.push(value);
    }
    let id = decode_cursor_column(row, "id")?;
    SortCursor::for_spec(spec, keys, id, filter_fp.to_owned())
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

async fn load_authors_for_works(
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

/// Batch-load non-author contributors for a page of works, ordered by
/// role (`author_role` declaration order) then position. The author
/// role is excluded here because it already feeds `authors[]` via
/// [`load_authors_for_works`]; duplicating it would put the same names
/// on the wire twice per row.
async fn load_contributors_for_works(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    work_ids: &[Uuid],
) -> Result<std::collections::HashMap<Uuid, Vec<ContributorRef>>, AppError> {
    let mut out: std::collections::HashMap<Uuid, Vec<ContributorRef>> =
        std::collections::HashMap::new();
    if work_ids.is_empty() {
        return Ok(out);
    }
    let rows = sqlx::query!(
        r#"SELECT wa.work_id, a.name, wa.role AS "role: ContributorRole"
           FROM work_authors wa
           JOIN authors a ON a.id = wa.author_id
           WHERE wa.work_id = ANY($1::uuid[]) AND wa.role <> 'author'
           ORDER BY wa.role ASC, wa.position ASC"#,
        work_ids,
    )
    .fetch_all(&mut **tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    for r in rows {
        out.entry(r.work_id).or_default().push(ContributorRef {
            name: r.name,
            role: r.role,
        });
    }
    Ok(out)
}

async fn load_reading_state_for_manifestations(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    manifestation_ids: &[Uuid],
) -> Result<std::collections::HashMap<Uuid, ReadingStateRow>, AppError> {
    let mut out: std::collections::HashMap<Uuid, ReadingStateRow> =
        std::collections::HashMap::new();
    if manifestation_ids.is_empty() {
        return Ok(out);
    }
    // Same RLS-scoped transaction as the page query: no `WHERE user_id = ...`
    // predicate here (module invariant, see the file doc comment).
    // `reading_state`'s RLS policy already confines this SELECT to the
    // caller's own rows.
    let rows = sqlx::query!(
        r#"SELECT manifestation_id, status AS "status?: ReadingStatus", rating,
                  notes, progress_pct, started_at, finished_at, last_read_at
             FROM reading_state WHERE manifestation_id = ANY($1::uuid[])"#,
        manifestation_ids,
    )
    .fetch_all(&mut **tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    for r in rows {
        out.insert(
            r.manifestation_id,
            ReadingStateRow {
                status: r.status,
                rating: r.rating,
                notes: r.notes,
                progress_pct: r.progress_pct,
                started_at: r.started_at,
                finished_at: r.finished_at,
                last_read_at: r.last_read_at,
            },
        );
    }
    Ok(out)
}

/// Providers the operator has hidden: `provider_visibility` keys whose
/// value is exactly `false`. Absent keys mean visible, so an empty map
/// hides nothing.
fn hidden_providers(visibility: &serde_json::Value) -> std::collections::HashSet<String> {
    visibility.as_object().map_or_default(|m| {
        m.iter()
            .filter(|(_, v)| v.as_bool() == Some(false))
            .map(|(k, _)| k.clone())
            .collect()
    })
}

/// Batch-load external identifiers for a page of manifestations and their
/// works, filtered to visible providers. Returns per-manifestation and
/// per-work maps; the caller concatenates work-level ids (shared across
/// sibling editions) ahead of the edition's own. The one-to-many tables are
/// deliberately NOT joined into the paginated base query: a cartesian
/// product there would break `LIMIT` and the cursor math.
async fn load_external_ids(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    manifestation_ids: &[Uuid],
    work_ids: &[Uuid],
    hidden: &std::collections::HashSet<String>,
) -> Result<
    (
        std::collections::HashMap<Uuid, Vec<ExternalIdRef>>,
        std::collections::HashMap<Uuid, Vec<ExternalIdRef>>,
    ),
    AppError,
> {
    let mut by_manifestation: std::collections::HashMap<Uuid, Vec<ExternalIdRef>> =
        std::collections::HashMap::new();
    let mut by_work: std::collections::HashMap<Uuid, Vec<ExternalIdRef>> =
        std::collections::HashMap::new();
    if manifestation_ids.is_empty() && work_ids.is_empty() {
        return Ok((by_manifestation, by_work));
    }
    let m_rows = sqlx::query!(
        "SELECT manifestation_id, scheme, external_id \
         FROM manifestation_external_identifiers \
         WHERE manifestation_id = ANY($1::uuid[]) \
         ORDER BY scheme ASC",
        manifestation_ids,
    )
    .fetch_all(&mut **tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    for r in m_rows {
        if hidden.contains(&r.scheme) {
            continue;
        }
        by_manifestation
            .entry(r.manifestation_id)
            .or_default()
            .push(ExternalIdRef {
                level: IdentifierLevel::Manifestation,
                scheme: r.scheme,
                external_id: r.external_id,
            });
    }
    let w_rows = sqlx::query!(
        "SELECT work_id, scheme, external_id \
         FROM work_external_identifiers \
         WHERE work_id = ANY($1::uuid[]) \
         ORDER BY scheme ASC",
        work_ids,
    )
    .fetch_all(&mut **tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    for r in w_rows {
        if hidden.contains(&r.scheme) {
            continue;
        }
        by_work.entry(r.work_id).or_default().push(ExternalIdRef {
            level: IdentifierLevel::Work,
            scheme: r.scheme,
            external_id: r.external_id,
        });
    }
    Ok((by_manifestation, by_work))
}

/// Batch-load per-provider ratings for a page of manifestations, filtered
/// to visible providers and sorted by source.
async fn load_external_ratings(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    manifestation_ids: &[Uuid],
    hidden: &std::collections::HashSet<String>,
) -> Result<std::collections::HashMap<Uuid, Vec<ExternalRatingRef>>, AppError> {
    let mut out: std::collections::HashMap<Uuid, Vec<ExternalRatingRef>> =
        std::collections::HashMap::new();
    if manifestation_ids.is_empty() {
        return Ok(out);
    }
    let rows = sqlx::query!(
        "SELECT manifestation_id, source, rating, rating_scale, review_count, fetched_at \
         FROM manifestation_external_ratings \
         WHERE manifestation_id = ANY($1::uuid[]) \
         ORDER BY source ASC",
        manifestation_ids,
    )
    .fetch_all(&mut **tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    for r in rows {
        if hidden.contains(&r.source) {
            continue;
        }
        out.entry(r.manifestation_id)
            .or_default()
            .push(ExternalRatingRef {
                source: r.source,
                rating: r.rating,
                rating_scale: r.rating_scale,
                review_count: r.review_count,
                fetched_at: r.fetched_at.to_jiff(),
            });
    }
    Ok(out)
}

/// Merge the shared work-level ids with the edition's own for one row.
fn merge_external_ids(
    by_work: &std::collections::HashMap<Uuid, Vec<ExternalIdRef>>,
    by_manifestation: &std::collections::HashMap<Uuid, Vec<ExternalIdRef>>,
    work_id: Uuid,
    manifestation_id: Uuid,
) -> Vec<ExternalIdRef> {
    let mut out = by_work.get(&work_id).cloned().unwrap_or_default();
    out.extend(
        by_manifestation
            .get(&manifestation_id)
            .cloned()
            .unwrap_or_default(),
    );
    out
}

/// Single-row fetch for [`book_detail`]. Joins the work for prose +
/// canonical pointers; RLS on `manifestations` makes the query return
/// `None` when the user cannot see the row, which the caller then
/// flips to [`AppError::NotFound`] (existence-not-leaked).
async fn fetch_detail_rows(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    ids: &[Uuid],
) -> Result<Vec<DetailRow>, AppError> {
    // Single `LEFT JOIN LATERAL` pulls all three series columns from
    // the same row, so a future tiebreaker tweak can never let
    // `series_name` and `series_position` drift onto different
    // `series_works` rows. Mirrors the list handler's series pattern.
    sqlx::query_as!(
        DetailRow,
        r#"
        SELECT m.id                   AS "id!",
               m.work_id              AS "work_id!",
               w.metadata_revision AS "work_revision!",
               m.metadata_revision AS "manifestation_revision!",
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
                     sw.position::float8 AS series_position
              FROM series_works sw
              JOIN series s ON s.id = sw.series_id
              WHERE sw.work_id = w.id
              ORDER BY sw.position ASC NULLS LAST, s.id ASC
              LIMIT 1
          ) series_one ON TRUE
         WHERE m.id = ANY($1::uuid[])
        "#,
        ids,
    )
    .fetch_all(&mut **tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))
}

async fn fetch_detail_row(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    id: Uuid,
) -> Result<Option<DetailRow>, AppError> {
    Ok(fetch_detail_rows(tx, &[id]).await?.into_iter().next())
}

/// Batch-load tag names for a page of manifestations, sorted by name
/// within each book. Same shape as [`load_authors_for_works`]: the
/// one-to-many join stays out of the paginated base query so it cannot
/// multiply rows under `LIMIT`.
///
/// This loader and its genre/mood siblings are deliberately three
/// near-identical copies. The constraint is on the SQL text alone:
/// `sqlx::query!` takes only a string literal, so the table and column
/// names cannot be parameterised without dropping to runtime SQL,
/// which this crate bans on the data path. The surrounding Rust
/// scaffolding could be shared; it stays inline so each loader reads
/// as one unit next to its query.
pub async fn load_tags_for_manifestations(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    manifestation_ids: &[Uuid],
) -> Result<std::collections::HashMap<Uuid, Vec<String>>, AppError> {
    let mut out: std::collections::HashMap<Uuid, Vec<String>> = std::collections::HashMap::new();
    if manifestation_ids.is_empty() {
        return Ok(out);
    }
    let rows = sqlx::query!(
        "SELECT mt.manifestation_id, t.name FROM manifestation_tags mt \
         JOIN tags t ON t.id = mt.tag_id \
         WHERE mt.manifestation_id = ANY($1::uuid[]) \
         ORDER BY t.name ASC",
        manifestation_ids,
    )
    .fetch_all(&mut **tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    for r in rows {
        out.entry(r.manifestation_id).or_default().push(r.name);
    }
    Ok(out)
}

/// Batch-load genre names for a page of manifestations, sorted by name
/// within each book. See [`load_tags_for_manifestations`].
pub async fn load_genres_for_manifestations(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    manifestation_ids: &[Uuid],
) -> Result<std::collections::HashMap<Uuid, Vec<String>>, AppError> {
    let mut out: std::collections::HashMap<Uuid, Vec<String>> = std::collections::HashMap::new();
    if manifestation_ids.is_empty() {
        return Ok(out);
    }
    let rows = sqlx::query!(
        "SELECT mg.manifestation_id, g.name FROM manifestation_genres mg \
         JOIN genres g ON g.id = mg.genre_id \
         WHERE mg.manifestation_id = ANY($1::uuid[]) \
         ORDER BY g.name ASC",
        manifestation_ids,
    )
    .fetch_all(&mut **tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    for r in rows {
        out.entry(r.manifestation_id).or_default().push(r.name);
    }
    Ok(out)
}

/// Batch-load mood names for a page of manifestations, sorted by name
/// within each book. See [`load_tags_for_manifestations`].
pub async fn load_moods_for_manifestations(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    manifestation_ids: &[Uuid],
) -> Result<std::collections::HashMap<Uuid, Vec<String>>, AppError> {
    let mut out: std::collections::HashMap<Uuid, Vec<String>> = std::collections::HashMap::new();
    if manifestation_ids.is_empty() {
        return Ok(out);
    }
    let rows = sqlx::query!(
        "SELECT mm.manifestation_id, md.name FROM manifestation_moods mm \
         JOIN moods md ON md.id = mm.mood_id \
         WHERE mm.manifestation_id = ANY($1::uuid[]) \
         ORDER BY md.name ASC",
        manifestation_ids,
    )
    .fetch_all(&mut **tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    for r in rows {
        out.entry(r.manifestation_id).or_default().push(r.name);
    }
    Ok(out)
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
    Ok(pending_version_rows(
        tx,
        &[VersionInput {
            parent: manifestation_id,
            canonical: canonical_ids.to_vec(),
        }],
    )
    .await?
    .remove(&manifestation_id)
    .unwrap_or_default())
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
    Ok(
        junction_pointer_ids_batch(tx, &[(manifestation_id, work_id)])
            .await?
            .remove(&manifestation_id)
            .unwrap_or_default(),
    )
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

/// Row shape decoded from the [`fetch_detail_row`] query — every
/// column the detail handler needs in one round-trip, including the
/// canonical pointer slots so [`accepted_pointer_count`] is a memory
/// op rather than a follow-up query.
#[derive(Clone)]
pub struct DetailRow {
    pub work_revision: i64,
    pub manifestation_revision: i64,
    pub id: Uuid,
    pub work_id: Uuid,
    pub title: String,
    pub subtitle: Option<String>,
    pub description: Option<String>,
    pub language: Option<String>,
    pub isbn_13: Option<String>,
    pub isbn_10: Option<String>,
    pub publisher: Option<String>,
    pub pub_date: Option<String>,
    pub pages: Option<i32>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    pub ingestion_status: String,
    pub validation_status: ValidationStatus,
    pub enrichment_status: String,
    pub content_rating: Option<ContentRating>,
    pub w_title_v: Option<Uuid>,
    pub w_subtitle_v: Option<Uuid>,
    pub w_desc_v: Option<Uuid>,
    pub w_lang_v: Option<Uuid>,
    pub m_publisher_v: Option<Uuid>,
    pub m_pubdate_v: Option<Uuid>,
    pub m_isbn10_v: Option<Uuid>,
    pub m_isbn13_v: Option<Uuid>,
    pub m_cover_v: Option<Uuid>,
    pub m_pages_v: Option<Uuid>,
    pub m_content_rating_v: Option<Uuid>,
    pub series_id: Option<Uuid>,
    pub series_name: Option<String>,
    pub series_position: Option<f64>,
}

impl DetailRow {
    pub fn isbn_mismatch(&self) -> bool {
        if let (Some(ten), Some(thirteen)) = (&self.isbn_10, &self.isbn_13)
            && let (Ok(ten), Ok(thirteen)) = (
                ten.parse::<isbn::Isbn10>(),
                thirteen.parse::<isbn::Isbn13>(),
            )
        {
            matches!(thirteen.to_string().get(..3), Some("978" | "979"))
                && isbn::Isbn13::from(ten) != thirteen
        } else {
            false
        }
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "single dynamic-query assembly preserves the snapshot and hydration flow"
)]
pub async fn list_books(
    current_user: &CurrentUser,
    state: &AppState,
    params: &ListParams,
    page_size: i64,
) -> Result<(Vec<BookListRow>, Option<String>), AppError> {
    filters::validate(params)?;

    let spec = params
        .sort
        .as_deref()
        .map(SortSpec::parse)
        .transpose()
        .map_err(|e| AppError::Validation(format!("invalid sort: {e}")))?
        .unwrap_or_default();

    // One fingerprint of the active filter set, computed once and used for
    // both cursor legs: it rejects an incoming cursor minted under a different
    // filter set, and it stamps the minted next cursor so the same check holds
    // on the following page.
    let filter_fp = filters::filter_fingerprint(params);

    let cursor = match params.cursor.as_deref() {
        Some(raw) => Some(
            SortCursor::parse_for(raw, CursorApi::Rest, &spec, &filter_fp)
                .map_err(|e| AppError::Validation(format!("invalid cursor: {e}")))?,
        ),
        None => None,
    };

    let mut tx = db::acquire_with_rls_snapshot(&state.pool, current_user.user_id)
        .await
        .inspect_err(|e| tracing::error!(error = %e, "list: acquire_with_rls failed"))
        .map_err(|e| AppError::Internal(e.into()))?;

    let rows = fetch_list_rows(&mut tx, params, &spec, cursor.as_ref(), page_size).await?;

    let (page_rows, has_more) = split_page(&rows, page_size);

    let work_ids: Vec<Uuid> = page_rows
        .iter()
        .map(|r| r.get::<Uuid, _>("work_id"))
        .collect();
    let authors_by_work = load_authors_for_works(&mut tx, &work_ids).await?;
    let contributors_by_work = load_contributors_for_works(&mut tx, &work_ids).await?;
    let manifestation_ids: Vec<Uuid> = page_rows.iter().map(|r| r.get::<Uuid, _>("id")).collect();
    let tags_by_manifestation = load_tags_for_manifestations(&mut tx, &manifestation_ids).await?;
    let genres_by_manifestation =
        load_genres_for_manifestations(&mut tx, &manifestation_ids).await?;
    let moods_by_manifestation = load_moods_for_manifestations(&mut tx, &manifestation_ids).await?;
    let reading_by_manifestation =
        load_reading_state_for_manifestations(&mut tx, &manifestation_ids).await?;
    // Snapshot of the hot-reloaded visibility setting; reading it per
    // request is what makes a toggle apply without a restart.
    let hidden = hidden_providers(&state.settings.read().await.provider_visibility);
    let (ids_by_manifestation, ids_by_work) =
        load_external_ids(&mut tx, &manifestation_ids, &work_ids, &hidden).await?;
    let ratings_by_manifestation =
        load_external_ratings(&mut tx, &manifestation_ids, &hidden).await?;

    let mut items: Vec<BookListRow> = Vec::with_capacity(page_rows.len());
    for r in page_rows {
        let m_id: Uuid = r.get("id");
        let work_id: Uuid = r.get("work_id");
        let series = series_ref_from_row(r)?;
        let ingestion_raw: String = r.get("ingestion_status");
        let enrichment_raw: String = r.get("enrichment_status");
        let reading = reading_by_manifestation.get(&m_id);
        let reading_etag = match reading {
            Some(reading) => reading.etag()?,
            None => ReadingStateRow::default().etag()?,
        };
        let metadata_etag = MetadataRevision {
            work_id,
            manifestation_id: m_id,
            work_revision: r.get("work_metadata_revision"),
            manifestation_revision: r.get("manifestation_metadata_revision"),
        }
        .etag()?;
        items.push(BookListRow {
            id: m_id,
            work_id,
            metadata_etag: metadata_etag
                .to_str()
                .map_err(|e| AppError::Internal(e.into()))?
                .to_owned(),
            reading_etag: reading_etag
                .to_str()
                .map_err(|e| AppError::Internal(e.into()))?
                .to_owned(),
            title: r.get("title"),
            subtitle: r.get("subtitle"),
            authors: authors_by_work.get(&work_id).cloned().unwrap_or_default(),
            contributors: contributors_by_work
                .get(&work_id)
                .cloned()
                .unwrap_or_default(),
            series,
            isbn_13: r.get("isbn_13"),
            pages: r.get("pages"),
            tags: tags_by_manifestation
                .get(&m_id)
                .cloned()
                .unwrap_or_default(),
            genres: genres_by_manifestation
                .get(&m_id)
                .cloned()
                .unwrap_or_default(),
            moods: moods_by_manifestation
                .get(&m_id)
                .cloned()
                .unwrap_or_default(),
            // Fallible decode for the same reason as validation_status
            // below: an unknown content_rating variant must be a clean
            // 500, not a panic.
            content_rating: r.try_get("content_rating").map_err(|e| {
                AppError::Internal(anyhow::anyhow!("unknown content_rating value from DB: {e}"))
            })?,
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
            reading_state: reading.map(ReadingStateSummary::from),
            external_ids: merge_external_ids(&ids_by_work, &ids_by_manifestation, work_id, m_id),
            external_ratings: ratings_by_manifestation
                .get(&m_id)
                .cloned()
                .unwrap_or_default(),
            created_at: r
                .try_get::<jiff_sqlx::Timestamp, _>("created_at")
                .map_err(|e| AppError::Internal(e.into()))?
                .to_jiff(),
        });
    }

    tx.commit()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let next_cursor = match page_rows.last() {
        Some(last) if has_more => {
            let row_id = last.get::<Uuid, _>("id");
            let encoded = next_cursor_for_row(last, &spec, &filter_fp)?
                .encode(CursorApi::Rest)
                .map_err(|e| {
                    tracing::warn!(error = %e, %row_id, "failed to encode pagination cursor");
                    AppError::Internal(e.into())
                })?;
            Some(encoded)
        }
        _ => None,
    };

    Ok((items, next_cursor))
}

pub async fn book_detail(
    current_user: &CurrentUser,
    state: &AppState,
    id: Uuid,
) -> Result<BookDetail, AppError> {
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
    let contributors = load_contributors_for_works(&mut tx, &[work_id])
        .await?
        .remove(&work_id)
        .unwrap_or_default();
    let tags = load_tags_for_manifestations(&mut tx, &[id])
        .await?
        .remove(&id)
        .unwrap_or_default();
    let genres = load_genres_for_manifestations(&mut tx, &[id])
        .await?
        .remove(&id)
        .unwrap_or_default();
    let moods = load_moods_for_manifestations(&mut tx, &[id])
        .await?
        .remove(&id)
        .unwrap_or_default();
    let hidden = hidden_providers(&state.settings.read().await.provider_visibility);
    let (ids_by_manifestation, ids_by_work) =
        load_external_ids(&mut tx, &[id], &[work_id], &hidden).await?;
    let external_ids = merge_external_ids(&ids_by_work, &ids_by_manifestation, work_id, id);
    let external_ratings = load_external_ratings(&mut tx, &[id], &hidden)
        .await?
        .remove(&id)
        .unwrap_or_default();
    let mut canonical_ids = canonical_pointer_ids(&row);
    let junction_ids = junction_pointer_ids(&mut tx, id, work_id).await?;
    // Junction-held pointers are applied versions too: without them the
    // summary would report an accepted count that drops every vocabulary
    // and contributor edit.
    let accepted_count = accepted_pointer_count(&row)
        .saturating_add(u32::try_from(junction_ids.len()).unwrap_or(u32::MAX));
    canonical_ids.extend(junction_ids);
    // THREAT: a child cannot act on a proposal (every metadata route refuses a
    // child), so the detail carries none for that caller rather than exposing
    // provider-supplied values no one has reviewed.
    let pending_versions = if current_user.is_child() {
        Vec::new()
    } else {
        load_pending_versions(&mut tx, id, &canonical_ids).await?
    };
    let pending = u32::try_from(pending_versions.len()).unwrap_or(u32::MAX);

    tx.commit()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let isbn_mismatch = row.isbn_mismatch();
    let series = match (row.series_id, row.series_name) {
        (Some(sid), Some(name)) => Some(SeriesRef {
            id: sid,
            name,
            position: row.series_position,
        }),
        _ => None,
    };

    let pub_date_str = row.pub_date;
    Ok(BookDetail {
        id: row.id,
        work_id,
        title: row.title,
        subtitle: row.subtitle,
        authors,
        contributors,
        series,
        description: row.description,
        language: row.language,
        isbn_mismatch,
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
        external_ids,
        external_ratings,
        metadata_version_summary: MetadataVersionSummary {
            pending,
            accepted: accepted_count,
        },
        metadata_versions: pending_versions,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

pub async fn work_detail(
    current_user: &CurrentUser,
    state: &AppState,
    id: Uuid,
) -> Result<WorkDetail, AppError> {
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
                sw.position::float8 AS \"position: f64\" \
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
            created_at: r.created_at.to_jiff(),
        });
    }

    let series = series_row.map(|s| SeriesRef {
        id: s.id,
        name: s.name,
        position: s.position,
    });

    Ok(WorkDetail {
        id: work.id,
        title: work.title,
        subtitle: work.subtitle,
        authors,
        description: work.description,
        language: work.language,
        series,
        manifestations,
    })
}

pub async fn series_detail(
    current_user: &CurrentUser,
    state: &AppState,
    id: Uuid,
) -> Result<SeriesDetail, AppError> {
    let mut tx = db::acquire_with_rls(&state.pool, current_user.user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let mut work_rows = sqlx::query!(
        r#"
        SELECT w.id                  AS "id!",
               w.title               AS "title!",
               sw.position::float8   AS "position?"
          FROM series_works sw
          JOIN works w ON w.id = sw.work_id
         WHERE sw.series_id = $1
         ORDER BY sw.position ASC NULLS LAST, w.id ASC
        "#,
        id,
    )
    .fetch_all(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    if work_rows.is_empty() {
        return Err(AppError::NotFound);
    }

    let work_ids: Vec<Uuid> = work_rows.iter().map(|r| r.id).collect();

    let manifestation_rows = sqlx::query!(
        r#"
        SELECT id          AS "id!",
               work_id     AS "work_id!",
               isbn_10,
               isbn_13,
               pages,
               created_at  AS "created_at!",
               ingestion_status::text  AS "ingestion_status!",
               validation_status       AS "validation_status!: ValidationStatus",
               enrichment_status::text AS "enrichment_status!"
          FROM manifestations
         WHERE work_id = ANY($1::uuid[])
         ORDER BY created_at ASC, id ASC
        "#,
        &work_ids,
    )
    .fetch_all(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    if manifestation_rows.is_empty() {
        return Err(AppError::NotFound);
    }

    if current_user.is_child() {
        // THREAT: Unprotected catalogue rows can disclose works outside the child's visible manifestations.
        let visible_ids: HashSet<Uuid> = manifestation_rows.iter().map(|r| r.work_id).collect();
        work_rows.retain(|work| visible_ids.contains(&work.id));
    }
    let work_ids: Vec<Uuid> = work_rows.iter().map(|r| r.id).collect();

    let series_row = sqlx::query!(
        r#"SELECT id   AS "id!",
                  name AS "name!",
                  sort_name AS "sort_name!"
             FROM series WHERE id = $1"#,
        id,
    )
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?
    .ok_or(AppError::NotFound)?;

    let authors_by_work = load_authors_for_works(&mut tx, &work_ids).await?;

    tx.commit()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let mut manifestations_by_work: HashMap<Uuid, Vec<WorkManifestation>> = HashMap::new();
    for r in manifestation_rows {
        manifestations_by_work
            .entry(r.work_id)
            .or_default()
            .push(WorkManifestation {
                id: r.id,
                isbn_13: r.isbn_13,
                isbn_10: r.isbn_10,
                pages: r.pages,
                cover_url: format!("/api/v1/books/{}/cover/thumb", r.id),
                ingestion_status: parse_ingestion(&r.ingestion_status)?,
                validation_status: r.validation_status,
                enrichment_status: parse_enrichment(&r.enrichment_status)?,
                created_at: r.created_at.to_jiff(),
            });
    }

    let works: Vec<SeriesWork> = work_rows
        .into_iter()
        .map(|w| SeriesWork {
            id: w.id,
            title: w.title,
            authors: authors_by_work.get(&w.id).cloned().unwrap_or_default(),
            position: w.position,
            manifestations: manifestations_by_work.remove(&w.id).unwrap_or_default(),
        })
        .collect();

    Ok(SeriesDetail {
        id: series_row.id,
        name: series_row.name,
        sort_name: series_row.sort_name,
        works,
    })
}

pub async fn reading_state(
    current_user: &CurrentUser,
    state: &AppState,
    manifestation_id: Uuid,
) -> Result<ReadingStateRow, AppError> {
    let mut tx = db::acquire_with_rls(&state.pool, current_user.user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    crate::services::reading::ensure_manifestation_visible(&mut tx, manifestation_id).await?;

    let row = sqlx::query_as!(
        ReadingStateRow,
        r#"SELECT status AS "status?: ReadingStatus", rating, notes, progress_pct,
                  started_at, finished_at, last_read_at
             FROM reading_state WHERE manifestation_id = $1"#,
        manifestation_id,
    )
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    tx.commit()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let row = row.unwrap_or_default();
    Ok(row)
}

#[expect(
    clippy::too_many_lines,
    reason = "single hybrid CTE assembly; splitting hurts readability of the SQL flow"
)]
pub async fn search(
    current_user: &CurrentUser,
    state: &AppState,
    q_raw: Option<&str>,
) -> Result<crate::models::library::SearchResponse, AppError> {
    use crate::models::library::{SearchHit, SearchHitKind, SearchResponse};
    let q_raw = q_raw.ok_or_else(|| AppError::Validation("query required".into()))?;
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
            q AS (SELECT websearch_to_tsquery('unaccent_english', $1) AS tsq),
            ts_hits AS (
                SELECT w.id, ts_rank_cd(w.search_vector, q.tsq, 32) AS rank
                FROM works w, q
                WHERE w.search_vector @@ q.tsq
            ),
            trgm_hits AS (
                SELECT w.id,
                       similarity(immutable_unaccent(w.title), immutable_unaccent($1)) * 0.5 AS rank
                FROM works w
                WHERE immutable_unaccent(w.title) % immutable_unaccent($1)
                  AND similarity(immutable_unaccent(w.title), immutable_unaccent($1)) > $4
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
                           'unaccent_english',
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
        i64::from(SEARCH_LIMIT),
        headline_opts,
        TRGM_SIMILARITY_FLOOR,
    )
    .fetch_all(&mut *tx)
    .await
    .inspect_err(|e| tracing::error!(error = %e, "search: hybrid CTE query failed"))
    .map_err(|e| AppError::Internal(e.into()))?;

    let work_ids: Vec<Uuid> = rows.iter().map(|r| r.work_id).collect();
    let authors = search_authors(&mut tx, &work_ids).await?;

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

    Ok(SearchResponse { items })
}

const MAX_Q_LEN: usize = 200;

pub const SEARCH_LIMIT: u16 = 20;

const TRGM_SIMILARITY_FLOOR: f32 = 0.3;

const SNIPPET_HL_START: &str = "\u{0002}";

const SNIPPET_HL_END: &str = "\u{0003}";

async fn search_authors(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
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
    .inspect_err(|e| tracing::error!(error = %e, "search: load_authors batch query failed"))
    .map_err(|e| AppError::Internal(e.into()))?;
    for r in rows {
        out.entry(r.work_id).or_default().push(r.name);
    }
    Ok(out)
}

pub async fn shelves(
    current_user: &CurrentUser,
    state: &AppState,
    after: Option<&str>,
    page_size: i64,
    api: CursorApi,
) -> Result<(Vec<crate::models::shelf::Shelf>, Option<String>, bool), AppError> {
    use crate::models::shelf::Shelf;
    use crate::routes::cursor::ShelfCursor;
    let cursor = after
        .map(|raw| ShelfCursor::parse(raw, api))
        .transpose()
        .map_err(|e| AppError::Validation(format!("invalid cursor: {e}")))?;

    let mut qb: QueryBuilder<Postgres> = QueryBuilder::new(
        "SELECT s.id, s.name, s.is_system, s.created_at, s.updated_at, \
                (SELECT COUNT(*) FROM shelf_items si WHERE si.shelf_id = s.id) AS item_count \
           FROM shelves s \
          WHERE s.user_id = ",
    );
    qb.push_bind(current_user.user_id);
    if let Some(c) = &cursor {
        // Mixed-direction sort (is_system DESC, then name/id ASC) cannot
        // use a single tuple comparison — expand into the two-arm OR.
        qb.push(" AND (s.is_system < ");
        qb.push_bind(c.is_system);
        qb.push(" OR (s.is_system = ");
        qb.push_bind(c.is_system);
        qb.push(" AND (s.name, s.id) > (");
        qb.push_bind(c.name.clone());
        qb.push(", ");
        qb.push_bind(c.id);
        qb.push(")))");
    }
    qb.push(" ORDER BY s.is_system DESC, s.name ASC, s.id ASC LIMIT ");
    qb.push_bind(page_size + 1);

    let rows = qb
        .build()
        .fetch_all(&state.pool)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let (page_rows, has_more) = split_page(&rows, page_size);

    let items: Vec<Shelf> = page_rows
        .iter()
        .map(|r| {
            Ok(Shelf {
                id: r.get("id"),
                name: r.get("name"),
                is_system: r.get("is_system"),
                created_at: r
                    .try_get::<jiff_sqlx::Timestamp, _>("created_at")
                    .map_err(|e| AppError::Internal(e.into()))?
                    .to_jiff(),
                updated_at: r
                    .try_get::<jiff_sqlx::Timestamp, _>("updated_at")
                    .map_err(|e| AppError::Internal(e.into()))?
                    .to_jiff(),
                item_count: r.get("item_count"),
            })
        })
        .collect::<Result<_, AppError>>()?;

    let next_cursor = match page_rows.last() {
        Some(last) if has_more => Some(
            ShelfCursor {
                is_system: last.get("is_system"),
                name: last.get("name"),
                id: last.get("id"),
            }
            .encode(api),
        ),
        _ => None,
    };

    Ok((items, next_cursor, has_more))
}

pub async fn shelf_items(
    current_user: &CurrentUser,
    state: &AppState,
    id: Uuid,
    after: Option<&str>,
    page_size: i64,
) -> Result<ShelfDetailResponse, AppError> {
    use crate::models::shelf::ShelfItem;
    use crate::routes::cursor::ShelfItemCursor;
    let cursor = after
        .map(ShelfItemCursor::parse)
        .transpose()
        .map_err(|e| AppError::Validation(format!("invalid cursor: {e}")))?;

    let shelf = sqlx::query!(
        r#"
        SELECT id          AS "id!",
               name        AS "name!",
               is_system   AS "is_system!",
               created_at  AS "created_at!",
               updated_at  AS "updated_at!"
          FROM shelves WHERE id = $1 AND user_id = $2
        "#,
        id,
        current_user.user_id,
    )
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| AppError::Internal(e.into()))?
    .ok_or(AppError::NotFound)?;

    let mut qb: QueryBuilder<Postgres> = QueryBuilder::new(
        "SELECT manifestation_id, added_at \
           FROM shelf_items WHERE shelf_id = ",
    );
    qb.push_bind(id);
    if let Some(c) = &cursor {
        // All-ASC sort — a single row-tuple comparison is total. The
        // manifestation_id tiebreaker is load-bearing: added_at is not
        // unique per shelf.
        qb.push(" AND (added_at, manifestation_id) > (");
        qb.push_bind(jiff_sqlx::Timestamp::from(c.added_at));
        qb.push(", ");
        qb.push_bind(c.manifestation_id);
        qb.push(")");
    }
    qb.push(" ORDER BY added_at ASC, manifestation_id ASC LIMIT ");
    qb.push_bind(page_size + 1);

    let rows = qb
        .build()
        .fetch_all(&state.pool)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let (page_rows, has_more) = split_page(&rows, page_size);

    let items: Vec<ShelfItem> = page_rows
        .iter()
        .map(|r| {
            Ok(ShelfItem {
                manifestation_id: r.get("manifestation_id"),
                added_at: r
                    .try_get::<jiff_sqlx::Timestamp, _>("added_at")
                    .map_err(|e| AppError::Internal(e.into()))?
                    .to_jiff(),
            })
        })
        .collect::<Result<_, AppError>>()?;

    let next_cursor = match page_rows.last() {
        Some(last) if has_more => {
            let manifestation_id: Uuid = last.get("manifestation_id");
            Some(
                ShelfItemCursor {
                    added_at: last
                        .try_get::<jiff_sqlx::Timestamp, _>("added_at")
                        .map_err(|e| AppError::Internal(e.into()))?
                        .to_jiff(),
                    manifestation_id,
                }
                .encode(),
            )
        }
        _ => None,
    };

    Ok(ShelfDetailResponse {
        id: shelf.id,
        name: shelf.name,
        is_system: shelf.is_system,
        created_at: shelf.created_at.to_jiff(),
        updated_at: shelf.updated_at.to_jiff(),
        items,
        next_cursor,
    })
}

/// Shelf identity, one ordered page of items and the next-page cursor.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ShelfDetailResponse {
    pub id: Uuid,
    pub name: String,
    pub is_system: bool,
    // This envelope re-declares the model's timestamps rather than
    // embedding `Shelf`.
    pub created_at: Timestamp,
    /// `shelves.updated_at`.
    pub updated_at: Timestamp,
    /// One page of items ordered by `added_at ASC, manifestation_id ASC`.
    pub items: Vec<crate::models::shelf::ShelfItem>,
    /// Opaque cursor for the next items page; `null` on the last page.
    pub next_cursor: Option<String>,
}

async fn fetch_list_rows(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    params: &ListParams,
    spec: &SortSpec,
    cursor: Option<&SortCursor>,
    page_size: i64,
) -> Result<Vec<sqlx::postgres::PgRow>, AppError> {
    let mut qb: QueryBuilder<Postgres> = QueryBuilder::new(
        "SELECT m.id, m.work_id, m.created_at, m.isbn_13, m.pages, \
                w.metadata_revision AS work_metadata_revision, \
                m.metadata_revision AS manifestation_metadata_revision, \
                m.content_rating, \
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
             SELECT s.id AS series_id, s.name AS series_name, \
                    sw.position::float8 AS series_position \
             FROM series_works sw \
             JOIN series s ON s.id = sw.series_id \
             WHERE sw.work_id = w.id \
             ORDER BY sw.position ASC NULLS LAST, s.id ASC \
             LIMIT 1 \
         ) series_one ON TRUE \
         WHERE TRUE",
    );
    filters::push_filter_predicates(&mut qb, params);
    push_cursor_predicate(&mut qb, spec, cursor);
    push_order_by(&mut qb, spec);
    qb.push(" LIMIT ");
    qb.push_bind(page_size + 1);

    qb.build()
        .fetch_all(&mut **tx)
        .await
        .map_err(|e| AppError::Internal(e.into()))
}

#[derive(Clone)]
pub struct BookRecord {
    pub detail: std::sync::Arc<DetailRow>,
    pub reading: std::sync::Arc<ReadingStateRow>,
    pub metadata_etag: String,
    pub reading_etag: String,
}

async fn read_book_records(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    ids: &[Uuid],
) -> Result<HashMap<Uuid, BookRecord>, AppError> {
    let rows = fetch_detail_rows(tx, ids).await?;
    let visible: Vec<_> = rows.iter().map(|row| row.id).collect();
    let mut reading = load_reading_state_for_manifestations(tx, &visible).await?;
    let mut records = HashMap::new();
    for row in rows {
        parse_ingestion(&row.ingestion_status)?;
        parse_enrichment(&row.enrichment_status)?;
        let reading = reading.remove(&row.id).unwrap_or_default();
        let metadata_etag = MetadataRevision {
            work_id: row.work_id,
            manifestation_id: row.id,
            work_revision: row.work_revision,
            manifestation_revision: row.manifestation_revision,
        }
        .etag()?;
        let reading_etag = reading.etag()?;
        records.insert(
            row.id,
            BookRecord {
                detail: std::sync::Arc::new(row),
                reading: std::sync::Arc::new(reading),
                metadata_etag: metadata_etag
                    .to_str()
                    .map_err(|e| AppError::Internal(e.into()))?
                    .to_owned(),
                reading_etag: reading_etag
                    .to_str()
                    .map_err(|e| AppError::Internal(e.into()))?
                    .to_owned(),
            },
        );
    }
    Ok(records)
}

pub async fn book_records(
    user: &CurrentUser,
    state: &AppState,
    ids: &[Uuid],
) -> Result<HashMap<Uuid, BookRecord>, AppError> {
    let mut tx = db::acquire_with_rls_snapshot(&state.pool, user.user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    let rows = read_book_records(&mut tx, ids).await?;
    tx.commit()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    Ok(rows)
}

pub struct BookPage {
    pub edges: Vec<(String, BookRecord)>,
    pub has_next: bool,
    pub has_previous: bool,
}

pub async fn books_connection(
    user: &CurrentUser,
    state: &AppState,
    params: &ListParams,
    first: i64,
) -> Result<BookPage, AppError> {
    filters::validate(params)?;
    let spec = params
        .sort
        .as_deref()
        .map(SortSpec::parse)
        .transpose()
        .map_err(|e| AppError::Validation(format!("invalid sort: {e}")))?
        .unwrap_or_default();
    let fingerprint = filters::filter_fingerprint(params);
    let cursor = params
        .cursor
        .as_deref()
        .map(|raw| SortCursor::parse_for(raw, CursorApi::Graphql, &spec, &fingerprint))
        .transpose()
        .map_err(|_| AppError::Validation("Invalid cursor.".into()))?;
    let mut tx = db::acquire_with_rls_snapshot(&state.pool, user.user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    let rows = fetch_list_rows(&mut tx, params, &spec, cursor.as_ref(), first).await?;
    let (page, has_next) = split_page(&rows, first);
    let ids: Vec<_> = page.iter().map(|row| row.get("id")).collect();
    let mut records = read_book_records(&mut tx, &ids).await?;
    let mut edges = Vec::with_capacity(page.len());
    for row in page {
        let id: Uuid = row.get("id");
        let cursor = next_cursor_for_row(row, &spec, &fingerprint)?
            .encode(CursorApi::Graphql)
            .map_err(|e| AppError::Internal(e.into()))?;
        if let Some(record) = records.remove(&id) {
            edges.push((cursor, record));
        }
    }
    let has_previous = if let Some(cursor) = &cursor {
        let mut qb = QueryBuilder::<Postgres>::new(
            "SELECT EXISTS(SELECT 1 FROM manifestations m JOIN works w ON w.id = m.work_id WHERE TRUE",
        );
        filters::push_filter_predicates(&mut qb, params);
        qb.push(" AND NOT (");
        push_cursor_expression(&mut qb, &spec, cursor);
        qb.push(") LIMIT 1)");
        qb.build_query_scalar::<bool>()
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| AppError::Internal(e.into()))?
    } else {
        false
    };
    tx.commit()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    Ok(BookPage {
        edges,
        has_next,
        has_previous,
    })
}

#[derive(Clone)]
pub struct WorkRecord {
    pub id: Uuid,
    pub title: String,
    pub subtitle: Option<String>,
    pub description: Option<String>,
    pub language: Option<String>,
    pub series_id: Option<Uuid>,
    pub series_position: Option<f64>,
}

pub async fn work_records(
    user: &CurrentUser,
    state: &AppState,
    ids: &[Uuid],
) -> Result<HashMap<Uuid, WorkRecord>, AppError> {
    let mut tx = db::acquire_with_rls_snapshot(&state.pool, user.user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    // THREAT: catalogue rows require a manifestation visible to the caller.
    let rows = sqlx::query_as!(
        WorkRecord,
        r#"
        SELECT w.id, w.title, w.subtitle, w.description, w.language,
          selected.series_id AS "series_id?", selected.position AS "series_position?: f64"
        FROM works w LEFT JOIN LATERAL (
          SELECT sw.series_id, sw.position::float8 AS position FROM series_works sw
          WHERE sw.work_id = w.id ORDER BY sw.position ASC NULLS LAST, sw.series_id ASC LIMIT 1
        ) selected ON TRUE WHERE w.id = ANY($1::uuid[]) AND (
            EXISTS (SELECT 1 FROM manifestations m WHERE m.work_id = w.id)

        )
    "#,
        ids
    )
    .fetch_all(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    tx.commit()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    Ok(rows.into_iter().map(|row| (row.id, row)).collect())
}

#[derive(Clone)]
pub struct ShelfRecord {
    pub id: Uuid,
    pub name: String,
    pub is_system: bool,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    pub item_count: i64,
}

pub async fn shelf_records(
    user: &CurrentUser,
    state: &AppState,
    ids: &[Uuid],
) -> Result<HashMap<Uuid, ShelfRecord>, AppError> {
    let mut tx = db::acquire_with_rls_snapshot(&state.pool, user.user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    let rows = sqlx::query_as!(
        ShelfRecord,
        r#"
        SELECT s.id, s.name, s.is_system, s.created_at, s.updated_at,
          (SELECT COUNT(*) FROM shelf_items si WHERE si.shelf_id = s.id) AS "item_count!"
        FROM shelves s WHERE s.id = ANY($1::uuid[]) AND s.user_id = $2
    "#,
        ids,
        user.user_id
    )
    .fetch_all(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    tx.commit()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    Ok(rows.into_iter().map(|row| (row.id, row)).collect())
}

#[derive(Clone)]
pub struct SeriesRecord {
    pub id: Uuid,
    pub name: String,
    pub sort_name: String,
}

pub async fn series_records(
    user: &CurrentUser,
    state: &AppState,
    ids: &[Uuid],
) -> Result<HashMap<Uuid, SeriesRecord>, AppError> {
    let mut tx = db::acquire_with_rls_snapshot(&state.pool, user.user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    let rows = sqlx::query_as!(SeriesRecord, r#"
        SELECT s.id, s.name, s.sort_name FROM series s WHERE s.id = ANY($1::uuid[])
        AND EXISTS (SELECT 1 FROM series_works sw JOIN manifestations m ON m.work_id = sw.work_id WHERE sw.series_id = s.id)
    "#, ids).fetch_all(&mut *tx).await.map_err(|e| AppError::Internal(e.into()))?;
    tx.commit()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    Ok(rows.into_iter().map(|row| (row.id, row)).collect())
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum RelationKind {
    Manifestations,
    SeriesWorks,
    ShelfItems,
    Authors,
    Contributors,
    Genres,
    Moods,
    Tags,
    Identifiers,
    WorkIdentifiers,
    Ratings,
}

pub const PAGE_MAX: u32 = 100;
pub const PENDING_MAX: u32 = 50;

impl RelationKind {
    pub const fn max_first(self) -> u32 {
        match self {
            Self::SeriesWorks | Self::ShelfItems => PAGE_MAX,
            Self::Tags => 50,
            _ => 20,
        }
    }
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct RelationKey {
    pub kind: RelationKind,
    pub parent: Uuid,
    pub first: i32,
    pub after: Option<String>,
}

#[derive(Clone, Default, serde::Serialize, serde::Deserialize)]
struct Boundary {
    id: Option<Uuid>,
    name: Option<String>,
    position: Option<f64>,
    ordinal: Option<i32>,
    created_at: Option<Timestamp>,
    level: Option<i32>,
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct RelationCursor {
    kind: RelationKind,
    parent: Uuid,
    boundary: Boundary,
    inner: Option<String>,
}

#[derive(serde::Serialize)]
struct RelationInput {
    request: i32,
    parent: Uuid,
    first: i32,
    has_after: bool,
    after_id: Option<Uuid>,
    after_name: Option<String>,
    after_position: Option<f64>,
    after_ordinal: Option<i32>,
    after_created: Option<Timestamp>,
    after_level: Option<i32>,
}

#[derive(Clone)]
pub struct RelationRow {
    pub request: i32,
    pub present: bool,
    pub id: Option<Uuid>,
    pub name: Option<String>,
    pub value: Option<String>,
    pub role: Option<ContributorRole>,
    pub position: Option<f64>,
    pub ordinal: Option<i32>,
    pub position_index: Option<i32>,
    pub created_at: Option<jiff_sqlx::Timestamp>,
    pub level: Option<i32>,
    pub rating: Option<f64>,
    pub scale: Option<f64>,
    pub reviews: Option<i32>,
    pub previous: bool,
}

#[derive(Clone)]
pub struct RelationPage {
    pub edges: Vec<(String, RelationRow)>,
    pub has_next: bool,
    pub has_previous: bool,
}

fn invalid_cursor() -> AppError {
    AppError::Validation("Invalid cursor.".into())
}

fn relation_input(key: &RelationKey, request: i32) -> Result<RelationInput, AppError> {
    use base64ct::{Base64UrlUnpadded, Encoding};
    let boundary = if let Some(after) = &key.after {
        let bytes = Base64UrlUnpadded::decode_vec(after).map_err(|_| invalid_cursor())?;
        let cursor: RelationCursor =
            serde_json::from_slice(&bytes).map_err(|_| invalid_cursor())?;
        if cursor.kind != key.kind || cursor.parent != key.parent {
            return Err(invalid_cursor());
        }
        match key.kind {
            RelationKind::Manifestations => {
                let spec =
                    SortSpec::parse("created_at").map_err(|e| AppError::Internal(e.into()))?;
                let inner = SortCursor::parse_for(
                    cursor.inner.as_deref().ok_or_else(invalid_cursor)?,
                    CursorApi::Graphql,
                    &spec,
                    &format!("work:{}", key.parent),
                )
                .map_err(|_| invalid_cursor())?;
                let [CursorValue::Ts(created)] = inner.keys.as_slice() else {
                    return Err(invalid_cursor());
                };
                Boundary {
                    id: Some(inner.id),
                    created_at: Some(*created),
                    ..Boundary::default()
                }
            }
            RelationKind::ShelfItems => {
                let inner = crate::routes::cursor::ShelfItemCursor::parse(
                    cursor.inner.as_deref().ok_or_else(invalid_cursor)?,
                )
                .map_err(|_| invalid_cursor())?;
                Boundary {
                    id: Some(inner.manifestation_id),
                    created_at: Some(inner.added_at),
                    ..Boundary::default()
                }
            }
            _ => cursor.boundary,
        }
    } else {
        Boundary::default()
    };
    if key.after.is_some() {
        let valid = match key.kind {
            RelationKind::Manifestations | RelationKind::ShelfItems => {
                boundary.id.is_some() && boundary.created_at.is_some()
            }
            RelationKind::SeriesWorks => boundary.id.is_some(),
            RelationKind::Authors | RelationKind::Contributors => {
                boundary.id.is_some()
                    && boundary.ordinal.is_some()
                    && boundary.position.is_some_and(|p| {
                        p.fract() == 0.0 && p >= f64::from(i32::MIN) && p <= f64::from(i32::MAX)
                    })
            }
            RelationKind::Genres | RelationKind::Moods | RelationKind::Tags => {
                boundary.id.is_some() && boundary.name.is_some()
            }
            RelationKind::Identifiers | RelationKind::WorkIdentifiers => {
                boundary.name.is_some() && boundary.level.is_some()
            }
            RelationKind::Ratings => boundary.name.is_some(),
        };
        if !valid {
            return Err(invalid_cursor());
        }
    }
    Ok(RelationInput {
        request,
        parent: key.parent,
        first: key.first,
        has_after: key.after.is_some(),
        after_id: boundary.id,
        after_name: boundary.name,
        after_position: boundary.position,
        after_ordinal: boundary.ordinal,
        after_created: boundary.created_at,
        after_level: boundary.level,
    })
}

/// The `items` connection cursor for one shelf item, as its own pages encode it.
pub fn shelf_item_cursor(
    shelf_id: Uuid,
    manifestation_id: Uuid,
    added_at: jiff::Timestamp,
) -> Result<String, AppError> {
    relation_cursor(
        &RelationKey {
            kind: RelationKind::ShelfItems,
            parent: shelf_id,
            first: 0,
            after: None,
        },
        &RelationRow {
            request: 0,
            present: true,
            id: Some(manifestation_id),
            name: None,
            value: None,
            role: None,
            position: None,
            ordinal: None,
            position_index: None,
            created_at: Some(added_at.into()),
            level: None,
            rating: None,
            scale: None,
            reviews: None,
            previous: false,
        },
    )
}

fn relation_cursor(key: &RelationKey, row: &RelationRow) -> Result<String, AppError> {
    use base64ct::{Base64UrlUnpadded, Encoding};
    let inner = match key.kind {
        RelationKind::Manifestations => {
            let spec = SortSpec::parse("created_at").map_err(|e| AppError::Internal(e.into()))?;
            Some(
                SortCursor::for_spec(
                    &spec,
                    vec![CursorValue::Ts(
                        row.created_at
                            .ok_or_else(|| {
                                AppError::Internal(anyhow::anyhow!(
                                    "Missing manifestation boundary"
                                ))
                            })?
                            .to_jiff(),
                    )],
                    row.id.ok_or_else(|| {
                        AppError::Internal(anyhow::anyhow!("Missing manifestation identity"))
                    })?,
                    format!("work:{}", key.parent),
                )
                .map_err(|e| AppError::Internal(e.into()))?
                .encode(CursorApi::Graphql)
                .map_err(|e| AppError::Internal(e.into()))?,
            )
        }
        RelationKind::ShelfItems => Some(
            crate::routes::cursor::ShelfItemCursor {
                added_at: row
                    .created_at
                    .ok_or_else(|| AppError::Internal(anyhow::anyhow!("Missing shelf boundary")))?
                    .to_jiff(),
                manifestation_id: row.id.ok_or_else(|| {
                    AppError::Internal(anyhow::anyhow!("Missing shelf item identity"))
                })?,
            }
            .encode(),
        ),
        _ => None,
    };
    let boundary = if inner.is_some() {
        Boundary::default()
    } else {
        Boundary {
            id: row.id,
            name: row.name.clone(),
            position: row.position,
            ordinal: row.ordinal,
            created_at: row.created_at.map(jiff_sqlx::Timestamp::to_jiff),
            level: row.level,
        }
    };
    let cursor = RelationCursor {
        kind: key.kind,
        parent: key.parent,
        boundary,
        inner,
    };
    Ok(Base64UrlUnpadded::encode_string(
        &serde_json::to_vec(&cursor).map_err(|e| AppError::Internal(e.into()))?,
    ))
}

fn relation_inputs(keys: &[RelationKey]) -> Result<Vec<RelationInput>, AppError> {
    let mut inputs = Vec::with_capacity(keys.len());
    for (index, key) in keys.iter().enumerate() {
        if !(0..=i64::from(key.kind.max_first())).contains(&i64::from(key.first)) {
            return Err(AppError::Validation("Invalid page size.".into()));
        }
        inputs.push(relation_input(
            key,
            i32::try_from(index).map_err(|e| AppError::Internal(e.into()))?,
        )?);
    }
    Ok(inputs)
}

pub async fn relation_pages(
    user: &CurrentUser,
    state: &AppState,
    keys: &[RelationKey],
) -> Result<HashMap<RelationKey, RelationPage>, AppError> {
    let mut tx = db::acquire_with_rls_snapshot(&state.pool, user.user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    let pages = relation_pages_in_tx(user, state, &mut tx, keys).await?;
    tx.commit()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    Ok(pages)
}

pub async fn relation_pages_in_tx(
    user: &CurrentUser,
    state: &AppState,
    tx: &mut sqlx::Transaction<'_, Postgres>,
    keys: &[RelationKey],
) -> Result<HashMap<RelationKey, RelationPage>, AppError> {
    let inputs = relation_inputs(keys)?;
    let hidden: Vec<_> = hidden_providers(&state.settings.read().await.provider_visibility)
        .into_iter()
        .collect();
    let mut grouped: HashMap<RelationKey, Vec<RelationRow>> = HashMap::new();
    for kind in [
        RelationKind::Manifestations,
        RelationKind::SeriesWorks,
        RelationKind::ShelfItems,
        RelationKind::Authors,
        RelationKind::Contributors,
        RelationKind::Genres,
        RelationKind::Moods,
        RelationKind::Tags,
        RelationKind::Identifiers,
        RelationKind::WorkIdentifiers,
        RelationKind::Ratings,
    ] {
        let batch: Vec<_> = inputs
            .iter()
            .enumerate()
            .filter(|(index, _)| keys[*index].kind == kind)
            .map(|(_, input)| input)
            .collect();
        if batch.is_empty() {
            continue;
        }
        let payload = serde_json::to_value(batch).map_err(|e| AppError::Internal(e.into()))?;
        let rows = match kind {
            RelationKind::Manifestations => {
                relation_manifestations(tx, &payload, user, &hidden).await?
            }
            RelationKind::SeriesWorks => relation_seriesworks(tx, &payload, user, &hidden).await?,
            RelationKind::ShelfItems => relation_shelfitems(tx, &payload, user, &hidden).await?,
            RelationKind::Authors => relation_authors(tx, &payload, user, &hidden).await?,
            RelationKind::Contributors => {
                relation_contributors(tx, &payload, user, &hidden).await?
            }
            RelationKind::Genres => relation_genres(tx, &payload, user, &hidden).await?,
            RelationKind::Moods => relation_moods(tx, &payload, user, &hidden).await?,
            RelationKind::Tags => relation_tags(tx, &payload, user, &hidden).await?,
            RelationKind::WorkIdentifiers => {
                relation_workidentifiers(tx, &payload, user, &hidden).await?
            }
            RelationKind::Identifiers => relation_identifiers(tx, &payload, user, &hidden).await?,
            RelationKind::Ratings => relation_ratings(tx, &payload, user, &hidden).await?,
        };
        for row in rows {
            let index = usize::try_from(row.request).map_err(|e| AppError::Internal(e.into()))?;
            let key = keys.get(index).ok_or_else(|| {
                AppError::Internal(anyhow::anyhow!("Unknown relation batch index"))
            })?;
            grouped.entry(key.clone()).or_default().push(row);
        }
    }
    let mut pages = HashMap::new();
    for key in keys {
        let rows = grouped.remove(key).unwrap_or_default();
        let has_previous = rows.iter().any(|row| row.previous);
        let rows: Vec<_> = rows.into_iter().filter(|row| row.present).collect();
        let size = usize::try_from(key.first).map_err(|e| AppError::Internal(e.into()))?;
        let has_next = rows.len() > size;
        let edges = rows
            .into_iter()
            .take(size)
            .map(|row| relation_cursor(key, &row).map(|cursor| (cursor, row)))
            .collect::<Result<Vec<_>, _>>()?;
        pages.insert(
            key.clone(),
            RelationPage {
                edges,
                has_next,
                has_previous,
            },
        );
    }
    Ok(pages)
}
async fn relation_manifestations(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    payload: &serde_json::Value,
    user: &CurrentUser,
    hidden: &[String],
) -> Result<Vec<RelationRow>, AppError> {
    sqlx::query_as!(RelationRow, r#"WITH inputs AS (SELECT * FROM jsonb_to_recordset($1::jsonb) AS input(
 request integer, parent uuid, first integer, has_after boolean, after_id uuid, after_name text,
 after_position float8, after_ordinal integer, after_created timestamptz, after_level integer)),
 caller AS (SELECT $2::uuid AS id, $3::boolean AS child, $4::text[] AS hidden)
 SELECT q.request AS "request!", COALESCE(item.present, FALSE) AS "present!",
 item.id AS "id?: Uuid",
item.name AS "name?: String",
item.value AS "value?: String",
item.role AS "role?: ContributorRole",
item.position AS "position?: f64",
item.ordinal AS "ordinal?: i32",
 item.position_index AS "position_index?: i32",
item.created_at AS "created_at?",
item.level AS "level?: i32",
item.rating AS "rating?: f64",
item.scale AS "scale?: f64",
item.reviews AS "reviews?: i32",
 (q.has_after AND EXISTS (SELECT 1 FROM manifestations m WHERE m.work_id = q.parent AND NOT ((m.created_at, m.id) > (q.after_created, q.after_id)) LIMIT 1)) AS "previous!"
 FROM inputs q CROSS JOIN caller
 LEFT JOIN LATERAL (SELECT TRUE AS present, m.id AS id,
NULL::text AS name,
NULL::text AS value,
NULL::author_role AS role,
NULL::float8 AS position,
NULL::integer AS ordinal,
 NULL::integer AS position_index,
m.created_at AS created_at,
NULL::integer AS level,
NULL::float8 AS rating,
NULL::float8 AS scale,
NULL::integer AS reviews
 FROM manifestations m WHERE m.work_id = q.parent AND (NOT q.has_after OR (m.created_at, m.id) > (q.after_created, q.after_id))
 ORDER BY m.created_at ASC, m.id ASC LIMIT (q.first + 1)) item ON TRUE
 WHERE TRUE
 ORDER BY q.request, item.created_at ASC, item.id ASC"#, payload, user.user_id, user.is_child(), hidden)
 .fetch_all(&mut **tx).await.map_err(|e| AppError::Internal(e.into()))
}

async fn relation_seriesworks(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    payload: &serde_json::Value,
    user: &CurrentUser,
    hidden: &[String],
) -> Result<Vec<RelationRow>, AppError> {
    sqlx::query_as!(RelationRow, r#"WITH inputs AS (SELECT * FROM jsonb_to_recordset($1::jsonb) AS input(
 request integer, parent uuid, first integer, has_after boolean, after_id uuid, after_name text,
 after_position float8, after_ordinal integer, after_created timestamptz, after_level integer)),
 caller AS (SELECT $2::uuid AS id, $3::boolean AS child, $4::text[] AS hidden)
 SELECT q.request AS "request!", COALESCE(item.present, FALSE) AS "present!",
 item.id AS "id?: Uuid",
item.name AS "name?: String",
item.value AS "value?: String",
item.role AS "role?: ContributorRole",
item.position AS "position?: f64",
item.ordinal AS "ordinal?: i32",
 item.position_index AS "position_index?: i32",
item.created_at AS "created_at?",
item.level AS "level?: i32",
item.rating AS "rating?: f64",
item.scale AS "scale?: f64",
item.reviews AS "reviews?: i32",
 (q.has_after AND EXISTS (SELECT 1 FROM series_works sw JOIN works w ON w.id = sw.work_id WHERE sw.series_id = q.parent AND EXISTS (SELECT 1 FROM manifestations visible WHERE visible.work_id = w.id) AND NOT (((q.after_position IS NULL AND sw.position IS NULL AND w.id > q.after_id)
 OR (q.after_position IS NOT NULL AND (sw.position > q.after_position OR sw.position IS NULL OR (sw.position = q.after_position AND w.id > q.after_id))))) LIMIT 1)) AS "previous!"
 FROM inputs q CROSS JOIN caller
 LEFT JOIN LATERAL (SELECT TRUE AS present, w.id AS id,
w.title AS name,
NULL::text AS value,
NULL::author_role AS role,
sw.position::float8 AS position,
NULL::integer AS ordinal,
 NULL::integer AS position_index,
NULL::timestamptz AS created_at,
NULL::integer AS level,
NULL::float8 AS rating,
NULL::float8 AS scale,
NULL::integer AS reviews
 FROM series_works sw JOIN works w ON w.id = sw.work_id WHERE sw.series_id = q.parent AND EXISTS (SELECT 1 FROM manifestations visible WHERE visible.work_id = w.id) AND (NOT q.has_after OR ((q.after_position IS NULL AND sw.position IS NULL AND w.id > q.after_id)
 OR (q.after_position IS NOT NULL AND (sw.position > q.after_position OR sw.position IS NULL OR (sw.position = q.after_position AND w.id > q.after_id)))))
 ORDER BY sw.position ASC NULLS LAST, w.id ASC LIMIT (q.first + 1)) item ON TRUE
 WHERE EXISTS (SELECT 1 FROM series_works permitted JOIN manifestations visible ON visible.work_id = permitted.work_id WHERE permitted.series_id = q.parent)
 ORDER BY q.request, item.position ASC NULLS LAST, item.id ASC"#, payload, user.user_id, user.is_child(), hidden)
 .fetch_all(&mut **tx).await.map_err(|e| AppError::Internal(e.into()))
}

async fn relation_shelfitems(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    payload: &serde_json::Value,
    user: &CurrentUser,
    hidden: &[String],
) -> Result<Vec<RelationRow>, AppError> {
    sqlx::query_as!(RelationRow, r#"WITH inputs AS (SELECT * FROM jsonb_to_recordset($1::jsonb) AS input(
 request integer, parent uuid, first integer, has_after boolean, after_id uuid, after_name text,
 after_position float8, after_ordinal integer, after_created timestamptz, after_level integer)),
 caller AS (SELECT $2::uuid AS id, $3::boolean AS child, $4::text[] AS hidden)
 SELECT q.request AS "request!", COALESCE(item.present, FALSE) AS "present!",
 item.id AS "id?: Uuid",
item.name AS "name?: String",
item.value AS "value?: String",
item.role AS "role?: ContributorRole",
item.position AS "position?: f64",
item.ordinal AS "ordinal?: i32",
 item.position_index AS "position_index?: i32",
item.created_at AS "created_at?",
item.level AS "level?: i32",
item.rating AS "rating?: f64",
item.scale AS "scale?: f64",
item.reviews AS "reviews?: i32",
 (q.has_after AND EXISTS (SELECT 1 FROM shelf_items si JOIN manifestations m ON m.id = si.manifestation_id WHERE si.shelf_id = q.parent AND NOT ((si.added_at, m.id) > (q.after_created, q.after_id)) LIMIT 1)) AS "previous!"
 FROM inputs q CROSS JOIN caller
 LEFT JOIN LATERAL (SELECT TRUE AS present, m.id AS id,
NULL::text AS name,
NULL::text AS value,
NULL::author_role AS role,
NULL::float8 AS position,
NULL::integer AS ordinal,
 NULL::integer AS position_index,
si.added_at AS created_at,
NULL::integer AS level,
NULL::float8 AS rating,
NULL::float8 AS scale,
NULL::integer AS reviews
 FROM shelf_items si JOIN manifestations m ON m.id = si.manifestation_id WHERE si.shelf_id = q.parent AND (NOT q.has_after OR (si.added_at, m.id) > (q.after_created, q.after_id))
 ORDER BY si.added_at ASC, m.id ASC LIMIT (q.first + 1)) item ON TRUE
 WHERE EXISTS (SELECT 1 FROM shelves permitted WHERE permitted.id = q.parent AND permitted.user_id = caller.id)
 ORDER BY q.request, item.created_at ASC, item.id ASC"#, payload, user.user_id, user.is_child(), hidden)
 .fetch_all(&mut **tx).await.map_err(|e| AppError::Internal(e.into()))
}

async fn relation_authors(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    payload: &serde_json::Value,
    user: &CurrentUser,
    hidden: &[String],
) -> Result<Vec<RelationRow>, AppError> {
    sqlx::query_as!(RelationRow, r#"WITH inputs AS (SELECT * FROM jsonb_to_recordset($1::jsonb) AS input(
 request integer, parent uuid, first integer, has_after boolean, after_id uuid, after_name text,
 after_position float8, after_ordinal integer, after_created timestamptz, after_level integer)),
 caller AS (SELECT $2::uuid AS id, $3::boolean AS child, $4::text[] AS hidden)
 SELECT q.request AS "request!", COALESCE(item.present, FALSE) AS "present!",
 item.id AS "id?: Uuid",
item.name AS "name?: String",
item.value AS "value?: String",
item.role AS "role?: ContributorRole",
item.position AS "position?: f64",
item.ordinal AS "ordinal?: i32",
 item.position_index AS "position_index?: i32",
item.created_at AS "created_at?",
item.level AS "level?: i32",
item.rating AS "rating?: f64",
item.scale AS "scale?: f64",
item.reviews AS "reviews?: i32",
 (q.has_after AND EXISTS (SELECT 1 FROM work_authors wa JOIN authors a ON a.id = wa.author_id WHERE wa.work_id = q.parent AND wa.role = 'author' AND NOT ((CASE wa.role WHEN 'author' THEN 0 WHEN 'editor' THEN 1 WHEN 'translator' THEN 2 WHEN 'narrator' THEN 3 END, wa.position::float8, a.id) > (q.after_ordinal, q.after_position, q.after_id)) LIMIT 1)) AS "previous!"
 FROM inputs q CROSS JOIN caller
 LEFT JOIN LATERAL (SELECT TRUE AS present, a.id AS id,
a.name AS name,
NULL::text AS value,
wa.role AS role,
wa.position::float8 AS position,
CASE wa.role WHEN 'author' THEN 0 WHEN 'editor' THEN 1 WHEN 'translator' THEN 2 WHEN 'narrator' THEN 3 END AS ordinal,
 wa.position AS position_index,
NULL::timestamptz AS created_at,
NULL::integer AS level,
NULL::float8 AS rating,
NULL::float8 AS scale,
NULL::integer AS reviews
 FROM work_authors wa JOIN authors a ON a.id = wa.author_id WHERE wa.work_id = q.parent AND wa.role = 'author' AND (NOT q.has_after OR (CASE wa.role WHEN 'author' THEN 0 WHEN 'editor' THEN 1 WHEN 'translator' THEN 2 WHEN 'narrator' THEN 3 END, wa.position::float8, a.id) > (q.after_ordinal, q.after_position, q.after_id))
 ORDER BY wa.role ASC, wa.position ASC, a.id ASC LIMIT (q.first + 1)) item ON TRUE
 WHERE EXISTS (SELECT 1 FROM manifestations visible WHERE visible.work_id = q.parent)
 ORDER BY q.request, item.ordinal ASC, item.position ASC, item.id ASC"#, payload, user.user_id, user.is_child(), hidden)
 .fetch_all(&mut **tx).await.map_err(|e| AppError::Internal(e.into()))
}

async fn relation_contributors(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    payload: &serde_json::Value,
    user: &CurrentUser,
    hidden: &[String],
) -> Result<Vec<RelationRow>, AppError> {
    sqlx::query_as!(RelationRow, r#"WITH inputs AS (SELECT * FROM jsonb_to_recordset($1::jsonb) AS input(
 request integer, parent uuid, first integer, has_after boolean, after_id uuid, after_name text,
 after_position float8, after_ordinal integer, after_created timestamptz, after_level integer)),
 caller AS (SELECT $2::uuid AS id, $3::boolean AS child, $4::text[] AS hidden)
 SELECT q.request AS "request!", COALESCE(item.present, FALSE) AS "present!",
 item.id AS "id?: Uuid",
item.name AS "name?: String",
item.value AS "value?: String",
item.role AS "role?: ContributorRole",
item.position AS "position?: f64",
item.ordinal AS "ordinal?: i32",
 item.position_index AS "position_index?: i32",
item.created_at AS "created_at?",
item.level AS "level?: i32",
item.rating AS "rating?: f64",
item.scale AS "scale?: f64",
item.reviews AS "reviews?: i32",
 (q.has_after AND EXISTS (SELECT 1 FROM work_authors wa JOIN authors a ON a.id = wa.author_id WHERE wa.work_id = q.parent AND wa.role <> 'author' AND NOT ((CASE wa.role WHEN 'author' THEN 0 WHEN 'editor' THEN 1 WHEN 'translator' THEN 2 WHEN 'narrator' THEN 3 END, wa.position::float8, a.id) > (q.after_ordinal, q.after_position, q.after_id)) LIMIT 1)) AS "previous!"
 FROM inputs q CROSS JOIN caller
 LEFT JOIN LATERAL (SELECT TRUE AS present, a.id AS id,
a.name AS name,
NULL::text AS value,
wa.role AS role,
wa.position::float8 AS position,
CASE wa.role WHEN 'author' THEN 0 WHEN 'editor' THEN 1 WHEN 'translator' THEN 2 WHEN 'narrator' THEN 3 END AS ordinal,
 wa.position AS position_index,
NULL::timestamptz AS created_at,
NULL::integer AS level,
NULL::float8 AS rating,
NULL::float8 AS scale,
NULL::integer AS reviews
 FROM work_authors wa JOIN authors a ON a.id = wa.author_id WHERE wa.work_id = q.parent AND wa.role <> 'author' AND (NOT q.has_after OR (CASE wa.role WHEN 'author' THEN 0 WHEN 'editor' THEN 1 WHEN 'translator' THEN 2 WHEN 'narrator' THEN 3 END, wa.position::float8, a.id) > (q.after_ordinal, q.after_position, q.after_id))
 ORDER BY wa.role ASC, wa.position ASC, a.id ASC LIMIT (q.first + 1)) item ON TRUE
 WHERE EXISTS (SELECT 1 FROM manifestations visible WHERE visible.work_id = q.parent)
 ORDER BY q.request, item.ordinal ASC, item.position ASC, item.id ASC"#, payload, user.user_id, user.is_child(), hidden)
 .fetch_all(&mut **tx).await.map_err(|e| AppError::Internal(e.into()))
}

async fn relation_genres(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    payload: &serde_json::Value,
    user: &CurrentUser,
    hidden: &[String],
) -> Result<Vec<RelationRow>, AppError> {
    sqlx::query_as!(RelationRow, r#"WITH inputs AS (SELECT * FROM jsonb_to_recordset($1::jsonb) AS input(
 request integer, parent uuid, first integer, has_after boolean, after_id uuid, after_name text,
 after_position float8, after_ordinal integer, after_created timestamptz, after_level integer)),
 caller AS (SELECT $2::uuid AS id, $3::boolean AS child, $4::text[] AS hidden)
 SELECT q.request AS "request!", COALESCE(item.present, FALSE) AS "present!",
 item.id AS "id?: Uuid",
item.name AS "name?: String",
item.value AS "value?: String",
item.role AS "role?: ContributorRole",
item.position AS "position?: f64",
item.ordinal AS "ordinal?: i32",
 item.position_index AS "position_index?: i32",
item.created_at AS "created_at?",
item.level AS "level?: i32",
item.rating AS "rating?: f64",
item.scale AS "scale?: f64",
item.reviews AS "reviews?: i32",
 (q.has_after AND EXISTS (SELECT 1 FROM manifestation_genres junction JOIN genres g ON g.id = junction.genre_id WHERE junction.manifestation_id = q.parent AND NOT ((g.name, g.id) > (q.after_name, q.after_id)) LIMIT 1)) AS "previous!"
 FROM inputs q CROSS JOIN caller
 LEFT JOIN LATERAL (SELECT TRUE AS present, g.id AS id,
g.name AS name,
NULL::text AS value,
NULL::author_role AS role,
NULL::float8 AS position,
NULL::integer AS ordinal,
 NULL::integer AS position_index,
NULL::timestamptz AS created_at,
NULL::integer AS level,
NULL::float8 AS rating,
NULL::float8 AS scale,
NULL::integer AS reviews
 FROM manifestation_genres junction JOIN genres g ON g.id = junction.genre_id WHERE junction.manifestation_id = q.parent AND (NOT q.has_after OR (g.name, g.id) > (q.after_name, q.after_id))
 ORDER BY g.name ASC, g.id ASC LIMIT (q.first + 1)) item ON TRUE
 WHERE EXISTS (SELECT 1 FROM manifestations visible WHERE visible.id = q.parent)
 ORDER BY q.request, item.name ASC, item.id ASC"#, payload, user.user_id, user.is_child(), hidden)
 .fetch_all(&mut **tx).await.map_err(|e| AppError::Internal(e.into()))
}

async fn relation_moods(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    payload: &serde_json::Value,
    user: &CurrentUser,
    hidden: &[String],
) -> Result<Vec<RelationRow>, AppError> {
    sqlx::query_as!(RelationRow, r#"WITH inputs AS (SELECT * FROM jsonb_to_recordset($1::jsonb) AS input(
 request integer, parent uuid, first integer, has_after boolean, after_id uuid, after_name text,
 after_position float8, after_ordinal integer, after_created timestamptz, after_level integer)),
 caller AS (SELECT $2::uuid AS id, $3::boolean AS child, $4::text[] AS hidden)
 SELECT q.request AS "request!", COALESCE(item.present, FALSE) AS "present!",
 item.id AS "id?: Uuid",
item.name AS "name?: String",
item.value AS "value?: String",
item.role AS "role?: ContributorRole",
item.position AS "position?: f64",
item.ordinal AS "ordinal?: i32",
 item.position_index AS "position_index?: i32",
item.created_at AS "created_at?",
item.level AS "level?: i32",
item.rating AS "rating?: f64",
item.scale AS "scale?: f64",
item.reviews AS "reviews?: i32",
 (q.has_after AND EXISTS (SELECT 1 FROM manifestation_moods junction JOIN moods g ON g.id = junction.mood_id WHERE junction.manifestation_id = q.parent AND NOT ((g.name, g.id) > (q.after_name, q.after_id)) LIMIT 1)) AS "previous!"
 FROM inputs q CROSS JOIN caller
 LEFT JOIN LATERAL (SELECT TRUE AS present, g.id AS id,
g.name AS name,
NULL::text AS value,
NULL::author_role AS role,
NULL::float8 AS position,
NULL::integer AS ordinal,
 NULL::integer AS position_index,
NULL::timestamptz AS created_at,
NULL::integer AS level,
NULL::float8 AS rating,
NULL::float8 AS scale,
NULL::integer AS reviews
 FROM manifestation_moods junction JOIN moods g ON g.id = junction.mood_id WHERE junction.manifestation_id = q.parent AND (NOT q.has_after OR (g.name, g.id) > (q.after_name, q.after_id))
 ORDER BY g.name ASC, g.id ASC LIMIT (q.first + 1)) item ON TRUE
 WHERE EXISTS (SELECT 1 FROM manifestations visible WHERE visible.id = q.parent)
 ORDER BY q.request, item.name ASC, item.id ASC"#, payload, user.user_id, user.is_child(), hidden)
 .fetch_all(&mut **tx).await.map_err(|e| AppError::Internal(e.into()))
}

async fn relation_tags(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    payload: &serde_json::Value,
    user: &CurrentUser,
    hidden: &[String],
) -> Result<Vec<RelationRow>, AppError> {
    sqlx::query_as!(RelationRow, r#"WITH inputs AS (SELECT * FROM jsonb_to_recordset($1::jsonb) AS input(
 request integer, parent uuid, first integer, has_after boolean, after_id uuid, after_name text,
 after_position float8, after_ordinal integer, after_created timestamptz, after_level integer)),
 caller AS (SELECT $2::uuid AS id, $3::boolean AS child, $4::text[] AS hidden)
 SELECT q.request AS "request!", COALESCE(item.present, FALSE) AS "present!",
 item.id AS "id?: Uuid",
item.name AS "name?: String",
item.value AS "value?: String",
item.role AS "role?: ContributorRole",
item.position AS "position?: f64",
item.ordinal AS "ordinal?: i32",
 item.position_index AS "position_index?: i32",
item.created_at AS "created_at?",
item.level AS "level?: i32",
item.rating AS "rating?: f64",
item.scale AS "scale?: f64",
item.reviews AS "reviews?: i32",
 (q.has_after AND EXISTS (SELECT 1 FROM manifestation_tags junction JOIN tags g ON g.id = junction.tag_id WHERE junction.manifestation_id = q.parent AND NOT ((g.name, g.id) > (q.after_name, q.after_id)) LIMIT 1)) AS "previous!"
 FROM inputs q CROSS JOIN caller
 LEFT JOIN LATERAL (SELECT TRUE AS present, g.id AS id,
g.name AS name,
NULL::text AS value,
NULL::author_role AS role,
NULL::float8 AS position,
NULL::integer AS ordinal,
 NULL::integer AS position_index,
NULL::timestamptz AS created_at,
NULL::integer AS level,
NULL::float8 AS rating,
NULL::float8 AS scale,
NULL::integer AS reviews
 FROM manifestation_tags junction JOIN tags g ON g.id = junction.tag_id WHERE junction.manifestation_id = q.parent AND (NOT q.has_after OR (g.name, g.id) > (q.after_name, q.after_id))
 ORDER BY g.name ASC, g.id ASC LIMIT (q.first + 1)) item ON TRUE
 WHERE EXISTS (SELECT 1 FROM manifestations visible WHERE visible.id = q.parent)
 ORDER BY q.request, item.name ASC, item.id ASC"#, payload, user.user_id, user.is_child(), hidden)
 .fetch_all(&mut **tx).await.map_err(|e| AppError::Internal(e.into()))
}

async fn relation_identifiers(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    payload: &serde_json::Value,
    user: &CurrentUser,
    hidden: &[String],
) -> Result<Vec<RelationRow>, AppError> {
    sqlx::query_as!(RelationRow, r#"WITH inputs AS (SELECT * FROM jsonb_to_recordset($1::jsonb) AS input(
 request integer, parent uuid, first integer, has_after boolean, after_id uuid, after_name text,
 after_position float8, after_ordinal integer, after_created timestamptz, after_level integer)),
 caller AS (SELECT $2::uuid AS id, $3::boolean AS child, $4::text[] AS hidden)
 SELECT q.request AS "request!", COALESCE(item.present, FALSE) AS "present!",
 item.id AS "id?: Uuid",
item.name AS "name?: String",
item.value AS "value?: String",
item.role AS "role?: ContributorRole",
item.position AS "position?: f64",
item.ordinal AS "ordinal?: i32",
 item.position_index AS "position_index?: i32",
item.created_at AS "created_at?",
item.level AS "level?: i32",
item.rating AS "rating?: f64",
item.scale AS "scale?: f64",
item.reviews AS "reviews?: i32",
 (q.has_after AND EXISTS (SELECT 1 FROM (SELECT 1 AS level, scheme, external_id FROM manifestation_external_identifiers WHERE manifestation_id = q.parent) x WHERE NOT (x.scheme = ANY(caller.hidden)) AND NOT ((x.level, x.scheme) > (q.after_level, q.after_name)) LIMIT 1)) AS "previous!"
 FROM inputs q CROSS JOIN caller
 LEFT JOIN LATERAL (SELECT TRUE AS present, NULL::uuid AS id,
x.scheme AS name,
x.external_id AS value,
NULL::author_role AS role,
NULL::float8 AS position,
NULL::integer AS ordinal,
 NULL::integer AS position_index,
NULL::timestamptz AS created_at,
x.level AS level,
NULL::float8 AS rating,
NULL::float8 AS scale,
NULL::integer AS reviews
 FROM (SELECT 1 AS level, scheme, external_id FROM manifestation_external_identifiers WHERE manifestation_id = q.parent) x WHERE NOT (x.scheme = ANY(caller.hidden)) AND (NOT q.has_after OR (x.level, x.scheme) > (q.after_level, q.after_name))
 ORDER BY x.level ASC, x.scheme ASC LIMIT (q.first + 1)) item ON TRUE
 WHERE EXISTS (SELECT 1 FROM manifestations visible WHERE visible.id = q.parent)
 ORDER BY q.request, item.level ASC, item.name ASC"#, payload, user.user_id, user.is_child(), hidden)
 .fetch_all(&mut **tx).await.map_err(|e| AppError::Internal(e.into()))
}
async fn relation_workidentifiers(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    payload: &serde_json::Value,
    user: &CurrentUser,
    hidden: &[String],
) -> Result<Vec<RelationRow>, AppError> {
    sqlx::query_as!(RelationRow, r#"WITH inputs AS (SELECT * FROM jsonb_to_recordset($1::jsonb) AS input(
 request integer, parent uuid, first integer, has_after boolean, after_id uuid, after_name text,
 after_position float8, after_ordinal integer, after_created timestamptz, after_level integer)),
 caller AS (SELECT $2::uuid AS id, $3::boolean AS child, $4::text[] AS hidden)
 SELECT q.request AS "request!", COALESCE(item.present, FALSE) AS "present!",
 item.id AS "id?: Uuid",
item.name AS "name?: String",
item.value AS "value?: String",
item.role AS "role?: ContributorRole",
item.position AS "position?: f64",
item.ordinal AS "ordinal?: i32",
 item.position_index AS "position_index?: i32",
item.created_at AS "created_at?",
item.level AS "level?: i32",
item.rating AS "rating?: f64",
item.scale AS "scale?: f64",
item.reviews AS "reviews?: i32",
 (q.has_after AND EXISTS (SELECT 1 FROM (SELECT 0 AS level, scheme, external_id FROM work_external_identifiers WHERE work_id = q.parent) x WHERE NOT (x.scheme = ANY(caller.hidden)) AND NOT ((x.level, x.scheme) > (q.after_level, q.after_name)) LIMIT 1)) AS "previous!"
 FROM inputs q CROSS JOIN caller
 LEFT JOIN LATERAL (SELECT TRUE AS present, NULL::uuid AS id,
x.scheme AS name,
x.external_id AS value,
NULL::author_role AS role,
NULL::float8 AS position,
NULL::integer AS ordinal,
 NULL::integer AS position_index,
NULL::timestamptz AS created_at,
x.level AS level,
NULL::float8 AS rating,
NULL::float8 AS scale,
NULL::integer AS reviews
 FROM (SELECT 0 AS level, scheme, external_id FROM work_external_identifiers WHERE work_id = q.parent) x WHERE NOT (x.scheme = ANY(caller.hidden)) AND (NOT q.has_after OR (x.level, x.scheme) > (q.after_level, q.after_name))
 ORDER BY x.level ASC, x.scheme ASC LIMIT (q.first + 1)) item ON TRUE
 WHERE EXISTS (SELECT 1 FROM manifestations visible WHERE visible.work_id = q.parent)
 ORDER BY q.request, item.level ASC, item.name ASC"#, payload, user.user_id, user.is_child(), hidden)
 .fetch_all(&mut **tx).await.map_err(|e| AppError::Internal(e.into()))
}

async fn relation_ratings(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    payload: &serde_json::Value,
    user: &CurrentUser,
    hidden: &[String],
) -> Result<Vec<RelationRow>, AppError> {
    sqlx::query_as!(RelationRow, r#"WITH inputs AS (SELECT * FROM jsonb_to_recordset($1::jsonb) AS input(
 request integer, parent uuid, first integer, has_after boolean, after_id uuid, after_name text,
 after_position float8, after_ordinal integer, after_created timestamptz, after_level integer)),
 caller AS (SELECT $2::uuid AS id, $3::boolean AS child, $4::text[] AS hidden)
 SELECT q.request AS "request!", COALESCE(item.present, FALSE) AS "present!",
 item.id AS "id?: Uuid",
item.name AS "name?: String",
item.value AS "value?: String",
item.role AS "role?: ContributorRole",
item.position AS "position?: f64",
item.ordinal AS "ordinal?: i32",
 item.position_index AS "position_index?: i32",
item.created_at AS "created_at?",
item.level AS "level?: i32",
item.rating AS "rating?: f64",
item.scale AS "scale?: f64",
item.reviews AS "reviews?: i32",
 (q.has_after AND EXISTS (SELECT 1 FROM manifestation_external_ratings x WHERE x.manifestation_id = q.parent AND NOT (x.source = ANY(caller.hidden)) AND NOT (x.source > q.after_name) LIMIT 1)) AS "previous!"
 FROM inputs q CROSS JOIN caller
 LEFT JOIN LATERAL (SELECT TRUE AS present, NULL::uuid AS id,
x.source AS name,
NULL::text AS value,
NULL::author_role AS role,
NULL::float8 AS position,
NULL::integer AS ordinal,
 NULL::integer AS position_index,
x.fetched_at AS created_at,
NULL::integer AS level,
x.rating::float8 AS rating,
x.rating_scale::float8 AS scale,
x.review_count AS reviews
 FROM manifestation_external_ratings x WHERE x.manifestation_id = q.parent AND NOT (x.source = ANY(caller.hidden)) AND (NOT q.has_after OR x.source > q.after_name)
 ORDER BY x.source ASC LIMIT (q.first + 1)) item ON TRUE
 WHERE EXISTS (SELECT 1 FROM manifestations visible WHERE visible.id = q.parent)
 ORDER BY q.request, item.name ASC"#, payload, user.user_id, user.is_child(), hidden)
 .fetch_all(&mut **tx).await.map_err(|e| AppError::Internal(e.into()))
}

pub async fn shelf_has_previous(
    user: &CurrentUser,
    state: &AppState,
    after: Option<&str>,
) -> Result<bool, AppError> {
    let Some(after) = after else {
        return Ok(false);
    };
    let cursor = crate::routes::cursor::ShelfCursor::parse(after, CursorApi::Graphql)
        .map_err(|_| invalid_cursor())?;
    let mut tx = db::acquire_with_rls(&state.pool, user.user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    let previous = sqlx::query_scalar!(r#"SELECT EXISTS(SELECT 1 FROM shelves s WHERE s.user_id = $1 AND (s.is_system > $2 OR (s.is_system = $2 AND (s.name, s.id) <= ($3, $4))) LIMIT 1) AS "previous!""#, user.user_id, cursor.is_system, cursor.name, cursor.id)
      .fetch_one(&mut *tx).await.map_err(|e| AppError::Internal(e.into()))?;
    tx.commit()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    Ok(previous)
}

#[derive(serde::Serialize)]
struct VersionInput {
    parent: Uuid,
    canonical: Vec<Uuid>,
}

#[derive(Clone)]
pub struct VersionSet {
    pub pending_count: i32,
    pub accepted: Option<i32>,
}

async fn junction_pointer_ids_batch(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    parents: &[(Uuid, Uuid)],
) -> Result<HashMap<Uuid, Vec<Uuid>>, AppError> {
    let manifestations: Vec<_> = parents.iter().map(|pair| pair.0).collect();
    let works: Vec<_> = parents.iter().map(|pair| pair.1).collect();
    let rows = sqlx::query!(r#"
        SELECT DISTINCT inputs.manifestation_id AS "manifestation_id!", pointers.id AS "id!"
        FROM UNNEST($1::uuid[], $2::uuid[]) inputs(manifestation_id, work_id)
        CROSS JOIN LATERAL (
          SELECT source_version_id AS id FROM manifestation_genres WHERE manifestation_id = inputs.manifestation_id
          UNION SELECT source_version_id FROM manifestation_moods WHERE manifestation_id = inputs.manifestation_id
          UNION SELECT source_version_id FROM manifestation_tags WHERE manifestation_id = inputs.manifestation_id
          UNION SELECT source_version_id FROM work_authors WHERE work_id = inputs.work_id
        ) pointers WHERE pointers.id IS NOT NULL
    "#, &manifestations, &works).fetch_all(&mut **tx).await.map_err(|e| AppError::Internal(e.into()))?;
    let mut result: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
    for row in rows {
        result.entry(row.manifestation_id).or_default().push(row.id);
    }
    Ok(result)
}

async fn version_inputs(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    ids: &[Uuid],
) -> Result<Vec<(VersionInput, i32)>, AppError> {
    let rows = fetch_detail_rows(tx, ids).await?;
    let parents: Vec<_> = rows.iter().map(|row| (row.id, row.work_id)).collect();
    let mut junctions = junction_pointer_ids_batch(tx, &parents).await?;
    let mut inputs = Vec::with_capacity(rows.len());
    for row in rows {
        let junctions = junctions.remove(&row.id).unwrap_or_default();
        let accepted = i32::try_from(
            u64::from(accepted_pointer_count(&row))
                + u64::try_from(junctions.len()).map_err(|e| AppError::Internal(e.into()))?,
        )
        .map_err(|e| AppError::Internal(e.into()))?;
        let mut canonical = canonical_pointer_ids(&row);
        canonical.extend(junctions);
        inputs.push((
            VersionInput {
                parent: row.id,
                canonical,
            },
            accepted,
        ));
    }
    Ok(inputs)
}

async fn pending_version_rows(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    inputs: &[VersionInput],
) -> Result<HashMap<Uuid, Vec<MetadataVersionRow>>, AppError> {
    if inputs.is_empty() {
        return Ok(HashMap::new());
    }
    let payload = serde_json::to_value(inputs).map_err(|e| AppError::Internal(e.into()))?;
    let rows = sqlx::query!(r#"
        WITH inputs AS (SELECT * FROM jsonb_to_recordset($1::jsonb) AS input(parent uuid, canonical uuid[]))
        SELECT inputs.parent AS "manifestation_id!", v.id AS "id!", v.field_name AS "field_name!", v.source AS "source!",
               v.new_value AS "new_value!", v.status::text AS "status!", v.confidence_score AS "confidence_score!",
               v.match_type AS "match_type!", v.observation_count AS "observation_count!"
        FROM inputs JOIN manifestations visible ON visible.id = inputs.parent
        CROSS JOIN LATERAL (
          SELECT id, field_name, source, new_value, status, confidence_score, match_type, observation_count, last_seen_at
          FROM metadata_versions WHERE manifestation_id = inputs.parent
            AND status = 'pending'::metadata_review_status AND new_value != 'null'::jsonb
            AND NOT (id = ANY(inputs.canonical))
          ORDER BY last_seen_at DESC LIMIT $2
        ) v ORDER BY inputs.parent, v.last_seen_at DESC
    "#, payload, i64::from(MAX_PENDING_VERSIONS)).fetch_all(&mut **tx).await.map_err(|e| AppError::Internal(e.into()))?;
    let mut result: HashMap<Uuid, Vec<MetadataVersionRow>> = HashMap::new();
    for row in rows {
        result
            .entry(row.manifestation_id)
            .or_default()
            .push(MetadataVersionRow {
                id: row.id,
                field_name: row.field_name,
                source: row.source,
                new_value: row.new_value,
                status: row.status,
                confidence_score: row.confidence_score,
                match_type: row.match_type,
                observation_count: row.observation_count,
            });
    }
    Ok(result)
}

pub async fn versions(
    user: &CurrentUser,
    state: &AppState,
    ids: &[Uuid],
) -> Result<HashMap<Uuid, VersionSet>, AppError> {
    let keys: Vec<_> = ids
        .iter()
        .map(|id| PendingKey {
            parent: *id,
            first: 0,
            after: None,
        })
        .collect();
    Ok(pending_pages(user, state, &keys)
        .await?
        .into_iter()
        .map(|(key, page)| {
            (
                key.parent,
                VersionSet {
                    pending_count: page.total,
                    accepted: page.accepted,
                },
            )
        })
        .collect())
}

pub async fn canonical_counts(
    user: &CurrentUser,
    state: &AppState,
    ids: &[Uuid],
) -> Result<HashMap<Uuid, i32>, AppError> {
    let mut tx = db::acquire_with_rls_snapshot(&state.pool, user.user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    let inputs = version_inputs(&mut tx, ids).await?;
    tx.commit()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    Ok(inputs
        .into_iter()
        .map(|(input, accepted)| (input.parent, accepted))
        .collect())
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct PendingKey {
    pub parent: Uuid,
    pub first: i32,
    pub after: Option<String>,
}
#[derive(Clone)]
pub struct PendingPage {
    pub edges: Vec<(String, MetadataVersionRow)>,
    pub total: i32,
    pub accepted: Option<i32>,
    pub has_previous: bool,
    pub has_next: bool,
}
impl PendingPage {
    const fn empty() -> Self {
        Self {
            edges: Vec::new(),
            total: 0,
            accepted: None,
            has_previous: false,
            has_next: false,
        }
    }
}
#[derive(serde::Serialize, serde::Deserialize)]
struct PendingCursor {
    parent: Uuid,
    seen: Timestamp,
    id: Uuid,
}
#[derive(serde::Serialize)]
struct PendingInput {
    request: i32,
    parent: Uuid,
    canonical: Vec<Uuid>,
    first: i32,
    seen: Option<Timestamp>,
    id: Option<Uuid>,
}

fn pending_inputs(keys: &[PendingKey]) -> Result<Vec<PendingInput>, AppError> {
    use base64ct::{Base64UrlUnpadded, Encoding};
    let mut inputs = Vec::new();
    for (index, key) in keys.iter().enumerate() {
        if !(0..=i64::from(PENDING_MAX)).contains(&i64::from(key.first)) {
            return Err(AppError::Validation("Invalid page size.".into()));
        }
        let cursor = match &key.after {
            None => None,
            Some(token) => {
                let bytes = Base64UrlUnpadded::decode_vec(token).map_err(|_| invalid_cursor())?;
                let cursor: PendingCursor =
                    serde_json::from_slice(&bytes).map_err(|_| invalid_cursor())?;
                if cursor.parent != key.parent {
                    return Err(invalid_cursor());
                }
                Some(cursor)
            }
        };
        inputs.push(PendingInput {
            request: i32::try_from(index).map_err(|e| AppError::Internal(e.into()))?,
            parent: key.parent,
            canonical: Vec::new(),
            first: key.first,
            seen: cursor.as_ref().map(|c| c.seen),
            id: cursor.map(|c| c.id),
        });
    }
    Ok(inputs)
}

pub async fn pending_pages(
    user: &CurrentUser,
    state: &AppState,
    keys: &[PendingKey],
) -> Result<HashMap<PendingKey, PendingPage>, AppError> {
    use base64ct::{Base64UrlUnpadded, Encoding};
    let mut inputs = pending_inputs(keys)?;
    // THREAT: children return before any proposal count or value query.
    if user.is_child() {
        return Ok(keys
            .iter()
            .cloned()
            .map(|key| (key, PendingPage::empty()))
            .collect());
    }
    let mut tx = db::acquire_with_rls_snapshot(&state.pool, user.user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    let ids: Vec<_> = keys.iter().map(|key| key.parent).collect();
    let canonical = version_inputs(&mut tx, &ids).await?;
    let accepted: HashMap<_, _> = canonical
        .iter()
        .map(|(input, count)| (input.parent, *count))
        .collect();
    let canonical: HashMap<_, _> = canonical
        .into_iter()
        .map(|(input, _)| (input.parent, input.canonical))
        .collect();
    for input in &mut inputs {
        input.canonical = canonical.get(&input.parent).cloned().unwrap_or_default();
    }
    let payload = serde_json::to_value(inputs).map_err(|e| AppError::Internal(e.into()))?;
    let rows = sqlx::query!(r#"
      WITH inputs AS (SELECT * FROM jsonb_to_recordset($1::jsonb) AS q(request integer,parent uuid,canonical uuid[],first integer,seen timestamptz,id uuid)),
      eligible AS NOT MATERIALIZED (
        SELECT q.request,q.parent,v.id,v.last_seen_at,v.field_name,v.source,v.new_value,v.status,v.confidence_score,v.match_type,v.observation_count
        FROM inputs q JOIN manifestations visible ON visible.id=q.parent
        JOIN metadata_versions v ON v.manifestation_id=q.parent
        WHERE v.status='pending'::metadata_review_status AND v.new_value != 'null'::jsonb AND NOT(v.id=ANY(q.canonical))
      )
      SELECT q.request AS "request!", totals.total AS "total!", previous.value AS "previous!",
        item.id AS "id?", item.last_seen_at AS "seen?", item.field_name AS "field_name?", item.source AS "source?", item.new_value AS "new_value?",
        item.status::text AS "status?",item.confidence_score AS "confidence_score?",item.match_type AS "match_type?",item.observation_count AS "observation_count?"
      FROM inputs q
      CROSS JOIN LATERAL (SELECT count(*) AS total FROM eligible e WHERE e.request=q.request) totals
      CROSS JOIN LATERAL (SELECT (q.seen IS NOT NULL AND EXISTS(SELECT 1 FROM eligible e WHERE e.request=q.request AND (e.last_seen_at,e.id)>=(q.seen,q.id) LIMIT 1)) AS value) previous
      LEFT JOIN LATERAL (SELECT e.* FROM eligible e WHERE e.request=q.request AND (q.seen IS NULL OR (e.last_seen_at,e.id)<(q.seen,q.id))
        ORDER BY e.last_seen_at DESC,e.id DESC LIMIT (q.first+1)) item ON TRUE
      ORDER BY q.request,item.last_seen_at DESC,item.id DESC
    "#,payload).fetch_all(&mut *tx).await.map_err(|e|AppError::Internal(e.into()))?;
    let mut pages = HashMap::new();
    for row in rows {
        let key = keys
            .get(usize::try_from(row.request).map_err(|e| AppError::Internal(e.into()))?)
            .ok_or_else(|| AppError::Internal(anyhow::anyhow!("Unknown pending batch index")))?;
        let page = pages.entry(key.clone()).or_insert(PendingPage {
            edges: Vec::new(),
            total: i32::try_from(row.total).map_err(|e| AppError::Internal(e.into()))?,
            accepted: accepted.get(&key.parent).copied(),
            has_previous: row.previous,
            has_next: false,
        });
        if let Some(id) = row.id {
            let seen = pending_required(row.seen)?.to_jiff();
            let cursor = Base64UrlUnpadded::encode_string(
                &serde_json::to_vec(&PendingCursor {
                    parent: key.parent,
                    seen,
                    id,
                })
                .map_err(|e| AppError::Internal(e.into()))?,
            );
            page.edges.push((
                cursor,
                MetadataVersionRow {
                    id,
                    field_name: pending_required(row.field_name)?,
                    source: pending_required(row.source)?,
                    new_value: pending_required(row.new_value)?,
                    status: pending_required(row.status)?,
                    confidence_score: pending_required(row.confidence_score)?,
                    match_type: pending_required(row.match_type)?,
                    observation_count: pending_required(row.observation_count)?,
                },
            ));
        }
    }
    for (key, page) in &mut pages {
        let size = usize::try_from(key.first).map_err(|e| AppError::Internal(e.into()))?;
        page.has_next = page.edges.len() > size;
        page.edges.truncate(size);
    }
    tx.commit()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    Ok(pages)
}
fn pending_required<T>(value: Option<T>) -> Result<T, AppError> {
    value.ok_or_else(|| AppError::Internal(anyhow::anyhow!("Missing pending projection field")))
}
