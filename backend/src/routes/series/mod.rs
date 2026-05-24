//! `/api/series/{id}` JSON route.
//!
//! Returns the series identity plus an ordered list of its works,
//! each with the manifestations the caller can see. Mirrors the
//! existence-not-leaked invariant of [`crate::routes::library`]:
//! `series` and `series_works` are catalog data (no RLS), but the
//! gate `at-least-one-manifestation-in-series-visible-to-caller`
//! short-circuits to 404 to avoid leaking series existence to child
//! accounts or across adult isolation boundaries.

use axum::Router;
use axum::extract::{Path, State};
use axum::routing::get;
use std::collections::HashMap;
use uuid::Uuid;

use crate::auth::middleware::CurrentUser;
use crate::db;
use crate::error::AppError;
use crate::models::library::WorkManifestation;
use crate::models::series::{SeriesDetail, SeriesWork};
use crate::routes::library::{load_authors_for_works, parse_enrichment, parse_ingestion};
use crate::state::AppState;

#[cfg(test)]
mod tests;

/// Build the `/api/series/{id}` router.
pub fn router() -> Router<AppState> {
    Router::new().route("/api/series/{id}", get(detail))
}

/// `GET /api/series/{id}` — series + ordered works with visible
/// manifestations per work.
///
/// # Errors
/// - [`AppError::NotFound`] when the series row is missing or every
///   work in the series has zero visible manifestations under the
///   caller's RLS context (existence-not-leaked).
/// - [`AppError::Internal`] on database errors.
async fn detail(
    current_user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<axum::Json<SeriesDetail>, AppError> {
    let mut tx = db::acquire_with_rls(&state.pool, current_user.user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let work_rows = sqlx::query!(
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
               created_at  AS "created_at!",
               ingestion_status::text  AS "ingestion_status!",
               validation_status::text AS "validation_status!",
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
                cover_url: format!("/api/books/{}/cover/thumb", r.id),
                ingestion_status: parse_ingestion(&r.ingestion_status)?,
                validation_status: r.validation_status,
                enrichment_status: parse_enrichment(&r.enrichment_status)?,
                created_at: r.created_at,
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

    Ok(axum::Json(SeriesDetail {
        id: series_row.id,
        name: series_row.name,
        sort_name: series_row.sort_name,
        works,
    }))
}
