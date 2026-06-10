//! `/api/v1/dashboard/*` admin-only library-health aggregation routes
//! (Step 12 / UNK-81).
//!
//! THREAT: Privilege escalation / information disclosure — both endpoints
//! are admin-gated via [`CurrentUser::require_admin`](crate::auth::middleware::CurrentUser::require_admin)
//! **before** any DB
//! access, so a non-admin caller receives 403 (401 when unauthenticated)
//! and never reaches the aggregation queries. Responses carry only
//! library-wide counts and byte totals — no per-user rows, file paths, or
//! PII. The sole request parameter (`?limit` on `/activity`) is clamped to
//! `1..=100` before binding, so it cannot drive an unbounded scan or a
//! negative-LIMIT error.
//!
//! The handlers acquire their transaction through
//! [`crate::db::acquire_with_rls`] so the `manifestations_select_adult` RLS
//! policy resolves the admin caller to full-library visibility — without the
//! `app.current_user_id` GUC the manifestation aggregates would silently
//! return zero. The other tables read here (`works`, `ingestion_jobs`) carry
//! no RLS and are `SELECT`-granted to `reverie_app` directly.

use axum::Json;
use axum::extract::{Query, State};
use axum::response::IntoResponse;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::auth::middleware::CurrentUser;
use crate::db;
use crate::error::AppError;
use crate::models::enrichment_status::EnrichmentStatus;
use crate::models::validation_status::ValidationStatus;
use crate::state::AppState;

#[cfg(test)]
mod tests;

/// Default number of recent ingestion batches returned by `/activity`.
const DEFAULT_ACTIVITY_LIMIT: i64 = 20;
/// Upper bound on the `?limit` parameter — caps the activity scan regardless
/// of caller input.
const MAX_ACTIVITY_LIMIT: i64 = 100;

/// Build the `/api/v1/dashboard/*` router (admin-only) as an [`OpenApiRouter`]
/// so each handler's `#[utoipa::path]` contributes to the generated spec (a
/// missing annotation fails to compile). Merged into `crate::openapi::pilot_router`
/// and split into its runtime and spec halves there.
pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(stats))
        .routes(routes!(activity))
}

/// One `{ status, count }` bucket in a breakdown array. `status` is the
/// canonical wire label of a [`ValidationStatus`] / [`EnrichmentStatus`]
/// variant, so the closed enum set is reflected on the wire even for zero
/// counts.
#[derive(serde::Serialize, utoipa::ToSchema)]
struct StatusCount {
    status: &'static str,
    count: i64,
}

/// One per-format storage bucket (`GROUP BY format`).
#[derive(serde::Serialize, utoipa::ToSchema)]
struct FormatBucket {
    format: String,
    count: i64,
    bytes: i64,
}

/// Metadata-coverage numerators (denominator is `total`). The frontend
/// derives percentages so the wire stays integer-only.
#[derive(serde::Serialize, utoipa::ToSchema)]
struct MetadataCoverage {
    total: i64,
    has_description: i64,
    has_language: i64,
    has_isbn_13: i64,
    has_cover: i64,
}

/// Response shape for `GET /api/v1/dashboard/stats`.
#[derive(serde::Serialize, utoipa::ToSchema)]
struct StatsResponse {
    total_manifestations: i64,
    total_works: i64,
    storage_total_bytes: i64,
    storage_cover_bytes: i64,
    storage_by_format: Vec<FormatBucket>,
    validation_breakdown: Vec<StatusCount>,
    /// Number of `clean` rows that are non-EPUB formats. UNK-313: non-EPUB
    /// files carry `validation_status='clean'` despite never being
    /// structurally validated — surfaced so the UI can footnote the bucket.
    clean_non_epub_count: i64,
    enrichment_breakdown: Vec<StatusCount>,
    metadata_coverage: MetadataCoverage,
}

/// `GET /api/v1/dashboard/stats` — library-wide aggregate health metrics
/// (admin only).
///
/// Computes the manifestation-side metrics in a single conditional-aggregate
/// scan joined to `works` (query A), plus one per-format `GROUP BY` (query
/// B). Validation/enrichment breakdowns are reassembled in Rust from the
/// fixed `FILTER` columns so the closed enum sets render deterministically.
///
/// # Errors
/// - [`AppError::Forbidden`] when the caller is not an admin.
/// - [`AppError::Internal`] on database errors.
#[utoipa::path(
    get,
    path = "/api/v1/dashboard/stats",
    tag = "dashboard",
    responses(
        (status = 200, description = "Library-wide aggregate health metrics. Admin only.", body = StatsResponse),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails),
        (status = 403, description = "Caller is not an admin", body = crate::openapi::ProblemDetails)
    )
)]
#[allow(
    clippy::too_many_lines,
    reason = "single aggregate-query assembly + DTO mapping; splitting hurts readability"
)]
async fn stats(
    current_user: CurrentUser,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    current_user.require_admin()?;

    let mut tx = db::acquire_with_rls(&state.pool, current_user.user_id)
        .await
        .inspect_err(|e| tracing::error!(error = %e, "dashboard stats: acquire_with_rls failed"))
        .map_err(|e| AppError::Internal(e.into()))?;

    // Query A — one scan of manifestations joined to works. SUM is cast to
    // bigint because Postgres SUM(bigint) yields numeric; COALESCE(...,0)
    // keeps an empty library at 0 rather than NULL.
    let agg = sqlx::query!(
        r#"SELECT
              COUNT(*)                            AS "total_manifestations!",
              COUNT(DISTINCT m.work_id)           AS "total_works!",
              COALESCE(SUM(m.file_size_bytes), 0)::bigint  AS "storage_total_bytes!",
              COALESCE(SUM(m.cover_size_bytes), 0)::bigint AS "storage_cover_bytes!",
              COUNT(*) FILTER (WHERE m.validation_status = 'pending')  AS "val_pending!",
              COUNT(*) FILTER (WHERE m.validation_status = 'clean')    AS "val_clean!",
              COUNT(*) FILTER (WHERE m.validation_status = 'repaired') AS "val_repaired!",
              COUNT(*) FILTER (WHERE m.validation_status = 'degraded') AS "val_degraded!",
              COUNT(*) FILTER (WHERE m.validation_status = 'clean' AND m.format <> 'epub') AS "clean_non_epub!",
              COUNT(*) FILTER (WHERE m.enrichment_status = 'pending')     AS "enr_pending!",
              COUNT(*) FILTER (WHERE m.enrichment_status = 'in_progress') AS "enr_in_progress!",
              COUNT(*) FILTER (WHERE m.enrichment_status = 'complete')    AS "enr_complete!",
              COUNT(*) FILTER (WHERE m.enrichment_status = 'failed')      AS "enr_failed!",
              COUNT(*) FILTER (WHERE m.enrichment_status = 'skipped')     AS "enr_skipped!",
              COUNT(*) FILTER (WHERE w.description IS NOT NULL AND w.description <> '') AS "has_description!",
              COUNT(*) FILTER (WHERE w.language    IS NOT NULL AND w.language    <> '') AS "has_language!",
              COUNT(*) FILTER (WHERE m.isbn_13     IS NOT NULL) AS "has_isbn_13!",
              COUNT(*) FILTER (WHERE m.cover_path  IS NOT NULL) AS "has_cover!"
           FROM manifestations m
           JOIN works w ON w.id = m.work_id"#,
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    // Query B — per-format storage + count. format is an enum → ::text.
    let formats = sqlx::query!(
        r#"SELECT format::text AS "format!", COUNT(*) AS "count!",
                  COALESCE(SUM(file_size_bytes), 0)::bigint AS "bytes!"
           FROM manifestations GROUP BY format ORDER BY format"#,
    )
    .fetch_all(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    tx.commit()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let validation_breakdown = vec![
        StatusCount {
            status: ValidationStatus::Pending.as_str(),
            count: agg.val_pending,
        },
        StatusCount {
            status: ValidationStatus::Clean.as_str(),
            count: agg.val_clean,
        },
        StatusCount {
            status: ValidationStatus::Repaired.as_str(),
            count: agg.val_repaired,
        },
        StatusCount {
            status: ValidationStatus::Degraded.as_str(),
            count: agg.val_degraded,
        },
    ];
    let enrichment_breakdown = vec![
        StatusCount {
            status: EnrichmentStatus::Pending.as_str(),
            count: agg.enr_pending,
        },
        StatusCount {
            status: EnrichmentStatus::InProgress.as_str(),
            count: agg.enr_in_progress,
        },
        StatusCount {
            status: EnrichmentStatus::Complete.as_str(),
            count: agg.enr_complete,
        },
        StatusCount {
            status: EnrichmentStatus::Failed.as_str(),
            count: agg.enr_failed,
        },
        StatusCount {
            status: EnrichmentStatus::Skipped.as_str(),
            count: agg.enr_skipped,
        },
    ];
    let storage_by_format = formats
        .into_iter()
        .map(|r| FormatBucket {
            format: r.format,
            count: r.count,
            bytes: r.bytes,
        })
        .collect();

    Ok(Json(StatsResponse {
        total_manifestations: agg.total_manifestations,
        total_works: agg.total_works,
        storage_total_bytes: agg.storage_total_bytes,
        storage_cover_bytes: agg.storage_cover_bytes,
        storage_by_format,
        validation_breakdown,
        clean_non_epub_count: agg.clean_non_epub,
        enrichment_breakdown,
        metadata_coverage: MetadataCoverage {
            total: agg.total_manifestations,
            has_description: agg.has_description,
            has_language: agg.has_language,
            has_isbn_13: agg.has_isbn_13,
            has_cover: agg.has_cover,
        },
    }))
}

/// `?limit=` query parameter for `GET /api/v1/dashboard/activity`. Absent →
/// [`DEFAULT_ACTIVITY_LIMIT`]; the value is clamped to `1..=MAX_ACTIVITY_LIMIT`
/// in the handler before binding.
#[derive(serde::Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
struct ActivityParams {
    /// Max recent batches to return; clamped to 1..=100, default 20.
    limit: Option<i64>,
}

/// One recent ingestion batch. Invariant:
/// `completed + failed + skipped + in_progress == total`. `ended_at` is null
/// while the batch is still in flight (any `queued`/`running` job).
#[derive(serde::Serialize, utoipa::ToSchema)]
struct BatchRow {
    batch_id: uuid::Uuid,
    #[serde(with = "time::serde::rfc3339")]
    started_at: time::OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    ended_at: Option<time::OffsetDateTime>,
    total: i64,
    completed: i64,
    failed: i64,
    skipped: i64,
    in_progress: i64,
}

/// Response shape for `GET /api/v1/dashboard/activity`.
#[derive(serde::Serialize, utoipa::ToSchema)]
struct ActivityResponse {
    batches: Vec<BatchRow>,
}

/// `GET /api/v1/dashboard/activity?limit=N` — most-recent ingestion batches,
/// newest first (admin only).
///
/// Groups `ingestion_jobs` by `batch_id` into per-batch outcome counts.
/// `acquire_with_rls` is used for pattern consistency but is not load-bearing
/// here — `ingestion_jobs` has no RLS.
///
/// # Errors
/// - [`AppError::Forbidden`] when the caller is not an admin.
/// - [`AppError::Internal`] on database errors.
#[utoipa::path(
    get,
    path = "/api/v1/dashboard/activity",
    tag = "dashboard",
    params(ActivityParams),
    responses(
        (status = 200, description = "Most-recent ingestion batches, newest first. Admin only.", body = ActivityResponse),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails),
        (status = 403, description = "Caller is not an admin", body = crate::openapi::ProblemDetails)
    )
)]
async fn activity(
    current_user: CurrentUser,
    State(state): State<AppState>,
    Query(params): Query<ActivityParams>,
) -> Result<impl IntoResponse, AppError> {
    current_user.require_admin()?;

    let limit = params
        .limit
        .unwrap_or(DEFAULT_ACTIVITY_LIMIT)
        .clamp(1, MAX_ACTIVITY_LIMIT);

    let mut tx = db::acquire_with_rls(&state.pool, current_user.user_id)
        .await
        .inspect_err(|e| tracing::error!(error = %e, "dashboard activity: acquire_with_rls failed"))
        .map_err(|e| AppError::Internal(e.into()))?;

    // ended_at: bare MAX(completed_at) would ignore NULLs and report a non-null
    // time for a batch that has some completed jobs but is still in flight, so
    // it is nulled whenever any job is queued/running.
    let rows = sqlx::query!(
        r#"SELECT batch_id AS "batch_id!",
                  MIN(created_at) AS "started_at!",
                  CASE WHEN COUNT(*) FILTER (WHERE status IN ('queued','running')) > 0
                       THEN NULL ELSE MAX(completed_at) END AS ended_at,
                  COUNT(*)                                               AS "total!",
                  COUNT(*) FILTER (WHERE status = 'complete')            AS "completed!",
                  COUNT(*) FILTER (WHERE status = 'failed')              AS "failed!",
                  COUNT(*) FILTER (WHERE status = 'skipped')             AS "skipped!",
                  COUNT(*) FILTER (WHERE status IN ('queued','running')) AS "in_progress!"
           FROM ingestion_jobs
           GROUP BY batch_id
           ORDER BY MIN(created_at) DESC
           LIMIT $1"#,
        limit,
    )
    .fetch_all(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    tx.commit()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let batches = rows
        .into_iter()
        .map(|r| BatchRow {
            batch_id: r.batch_id,
            started_at: r.started_at,
            ended_at: r.ended_at,
            total: r.total,
            completed: r.completed,
            failed: r.failed,
            skipped: r.skipped,
            in_progress: r.in_progress,
        })
        .collect();

    Ok(Json(ActivityResponse { batches }))
}
