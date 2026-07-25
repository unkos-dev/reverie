//! Enrichment control endpoints.
//!
//! * `POST /api/v1/manifestations/:id/enrichment/trigger` — re-queue this
//!   manifestation for a fresh enrichment pass.
//! * `POST /api/v1/manifestations/:id/enrichment/dry-run`  — synchronous preview
//!   of what the pipeline would change.
//! * `GET  /api/v1/enrichment/status` — aggregate queue counters.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use serde::Serialize;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::auth::middleware::CurrentUser;
use crate::auth::scope::Scope;
use crate::db;
use crate::error::AppError;
use crate::models::enrichment_status::EnrichmentStatus;
use crate::services;
use crate::state::AppState;

/// Build the enrichment-control router.
///
/// Mounts `POST /api/v1/manifestations/{id}/enrichment/{trigger,dry-run}` and
/// `GET /api/v1/enrichment/status` on the application `AppState`.
///
/// # Invariants
/// - Handlers require an authenticated non-child user
///   (`CurrentUser::require_not_child`).
/// - Per-manifestation visibility is gated by `db::acquire_with_rls`;
///   manifestations the caller can't see resolve to `AppError::NotFound`
///   so existence is not leaked across users.
///
/// Why: `dry_run` fans out onto `state.ingestion_pool` (the queue's
/// pool, no RLS) for the joined-table read, so the RLS visibility check
/// must run first on the user's pool before crossing the pool boundary.
pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(trigger))
        .routes(routes!(dry_run))
        .routes(routes!(status))
}

/// `POST /api/v1/manifestations/{id}/enrichment/trigger` — re-queue the
/// manifestation for a fresh enrichment pass.
///
/// An `in_progress` row is not flipped to `pending`: the active worker
/// still owns the claim, and releasing it would let a second worker run
/// the same manifestation concurrently. The trigger sets
/// `enrichment_rerun_requested` instead, and the worker's completion
/// bookkeeping converts the flag into a fresh eligible row. All CASE arms
/// read the pre-update row state, so the status test is consistent across
/// every column.
///
/// # Errors
/// - [`AppError::Forbidden`] when the caller is a child account.
/// - [`AppError::NotFound`] when the manifestation is missing or
///   RLS-hidden (existence-not-leaked).
/// - [`AppError::Internal`] on database errors.
#[utoipa::path(
    post,
    path = "/api/v1/manifestations/{id}/enrichment/trigger",
    tag = "enrichment",
    security(("session_cookie" = ["write"]), ("device_token_bearer" = ["write"]), ("oidc_jwt_bearer" = ["write"]), ("opds_basic" = ["write"])),
    params(("id" = Uuid, Path, description = "Manifestation id")),
    responses(
        (status = 202, description = "Re-run scheduled: an idle manifestation is reset to pending for the background worker's next poll; one with an active run is re-queued when that run completes"),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails),
        (status = 403, description = "Caller is a child account", body = crate::openapi::ProblemDetails),
        (status = 404, description = "Manifestation missing or RLS-hidden", body = crate::openapi::ProblemDetails)
    )
)]
async fn trigger(
    current_user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    current_user.require_scope(Scope::Write)?;
    current_user.require_not_child()?;

    let mut tx = db::acquire_with_rls(&state.pool, current_user.user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let rows = sqlx::query!(
        "UPDATE manifestations \
         SET enrichment_rerun_requested = (enrichment_status = 'in_progress'), \
             enrichment_status = CASE WHEN enrichment_status = 'in_progress' \
                                      THEN enrichment_status \
                                      ELSE 'pending'::enrichment_status END, \
             enrichment_attempt_count = CASE WHEN enrichment_status = 'in_progress' \
                                             THEN enrichment_attempt_count ELSE 0 END, \
             enrichment_attempted_at = CASE WHEN enrichment_status = 'in_progress' \
                                            THEN enrichment_attempted_at ELSE NULL END, \
             enrichment_error = CASE WHEN enrichment_status = 'in_progress' \
                                     THEN enrichment_error ELSE NULL END \
         WHERE id = $1",
        id,
    )
    .execute(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    if rows.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }

    tx.commit()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    Ok(StatusCode::ACCEPTED)
}

/// `POST /api/v1/manifestations/{id}/enrichment/dry-run` — synchronous
/// preview of what an enrichment pass would change. No side effects
/// beyond the source cache.
///
/// # Errors
/// - [`AppError::Forbidden`] when the caller is a child account.
/// - [`AppError::NotFound`] when the manifestation is missing or
///   RLS-hidden (existence-not-leaked).
/// - [`AppError::Internal`] on database errors or fan-out infrastructure
///   failure.
#[utoipa::path(
    post,
    path = "/api/v1/manifestations/{id}/enrichment/dry-run",
    tag = "enrichment",
    security(("session_cookie" = ["read"]), ("device_token_bearer" = ["read"]), ("oidc_jwt_bearer" = ["read"]), ("opds_basic" = ["read"])),
    params(("id" = Uuid, Path, description = "Manifestation id")),
    responses(
        (status = 200, description = "Diff of changes an enrichment pass would make; per-source failures are listed, not fatal", body = crate::services::enrichment::dry_run::DryRunDiff),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails),
        (status = 403, description = "Caller is a child account", body = crate::openapi::ProblemDetails),
        (status = 404, description = "Manifestation missing or RLS-hidden", body = crate::openapi::ProblemDetails)
    )
)]
async fn dry_run(
    current_user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    current_user.require_not_child()?;

    // RLS-gated visibility check: hide manifestations the user can't see.
    // The check runs on the user's pool with `app.current_user_id` set; the
    // subsequent fan-out runs on `ingestion_pool` (matching the queue's
    // pattern) because the dry-run service reads through several joined
    // tables without re-checking RLS at each step.
    let mut tx = db::acquire_with_rls(&state.pool, current_user.user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    let visible = sqlx::query_scalar!("SELECT id FROM manifestations WHERE id = $1", id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    drop(tx);
    if visible.is_none() {
        return Err(AppError::NotFound);
    }

    let diff = services::enrichment::dry_run::preview(&state.ingestion_pool, &state.config, id)
        .await
        .map_err(AppError::Internal)?;
    Ok(axum::Json(diff))
}

/// Aggregate per-status manifestation counts for the enrichment queue.
#[derive(Debug, Serialize, utoipa::ToSchema)]
struct StatusSummary {
    /// Manifestations awaiting enrichment.
    pending: i64,
    /// Manifestations currently being enriched.
    in_progress: i64,
    /// Manifestations enriched successfully.
    complete: i64,
    /// Manifestations whose last attempt failed (retryable).
    failed: i64,
    /// Manifestations skipped terminally.
    skipped: i64,
}

/// `GET /api/v1/enrichment/status` — aggregate queue counters over the
/// manifestations visible to the caller.
///
/// # Errors
/// - [`AppError::Forbidden`] when the caller is a child account.
/// - [`AppError::Internal`] on database errors.
#[utoipa::path(
    get,
    path = "/api/v1/enrichment/status",
    tag = "enrichment",
    security(("session_cookie" = ["read"]), ("device_token_bearer" = ["read"]), ("oidc_jwt_bearer" = ["read"]), ("opds_basic" = ["read"])),
    responses(
        (status = 200, description = "Per-status manifestation counts under the caller's RLS context", body = StatusSummary),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails),
        (status = 403, description = "Caller is a child account", body = crate::openapi::ProblemDetails)
    )
)]
async fn status(
    current_user: CurrentUser,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    current_user.require_not_child()?;

    let mut tx = db::acquire_with_rls(&state.pool, current_user.user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let rows = sqlx::query!(
        "SELECT enrichment_status AS \"status!: EnrichmentStatus\", COUNT(*)::bigint AS \"count!\" \
         FROM manifestations \
         GROUP BY enrichment_status",
    )
    .fetch_all(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    let mut summary = StatusSummary {
        pending: 0,
        in_progress: 0,
        complete: 0,
        failed: 0,
        skipped: 0,
    };
    for r in rows {
        match r.status {
            EnrichmentStatus::Pending => summary.pending = r.count,
            EnrichmentStatus::InProgress => summary.in_progress = r.count,
            EnrichmentStatus::Complete => summary.complete = r.count,
            EnrichmentStatus::Failed => summary.failed = r.count,
            EnrichmentStatus::Skipped => summary.skipped = r.count,
        }
    }
    Ok(axum::Json(summary))
}

#[cfg(test)]
mod tests {
    use crate::test_support;
    use axum::http::StatusCode;
    use uuid::Uuid;

    #[tokio::test]
    async fn trigger_requires_auth() {
        let server = test_support::test_server();
        let id = Uuid::new_v4();
        let response = server
            .post(&format!("/api/v1/manifestations/{id}/enrichment/trigger"))
            .await;
        assert_eq!(response.status_code(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn status_requires_auth() {
        let server = test_support::test_server();
        let response = server.get("/api/v1/enrichment/status").await;
        assert_eq!(response.status_code(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn dry_run_requires_auth() {
        let server = test_support::test_server();
        let id = Uuid::new_v4();
        let response = server
            .post(&format!("/api/v1/manifestations/{id}/enrichment/dry-run"))
            .await;
        assert_eq!(response.status_code(), StatusCode::UNAUTHORIZED);
    }

    /// X3: admin POST /trigger must reset `enrichment_status` / `attempt_count`
    /// on a real manifestation row.  Pre-C2 this 404'd in production
    /// because state.pool had no `app.current_user_id` session var.
    #[sqlx::test(migrations = "./migrations")]
    async fn trigger_admin_resets_enrichment_state(pool: sqlx::PgPool) {
        use axum::http::header::AUTHORIZATION;
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (_work_id, m_id) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;

        // Pre-set the manifestation to a "failed" state with retries logged.
        sqlx::query!(
            "UPDATE manifestations \
             SET enrichment_status = 'failed'::enrichment_status, \
                 enrichment_attempt_count = 3, \
                 enrichment_attempted_at = now(), \
                 enrichment_error = 'simulated failure' \
             WHERE id = $1",
            m_id,
        )
        .execute(&ing_pool)
        .await
        .expect("seed failed state");

        let server = test_support::db::server_with_real_pools(&app_pool, &ing_pool);
        let response = server
            .post(&format!("/api/v1/manifestations/{m_id}/enrichment/trigger"))
            .add_header(AUTHORIZATION, basic)
            .await;
        assert_eq!(
            response.status_code(),
            StatusCode::ACCEPTED,
            "body = {}",
            response.text()
        );

        // Verify via ingestion_pool — `manifestations` is RLS-gated under
        // `reverie_app` and the verification SELECT carries no session context.
        let row = sqlx::query!(
            "SELECT enrichment_status AS \"status!: crate::models::enrichment_status::EnrichmentStatus\", \
                    enrichment_attempt_count, enrichment_error \
             FROM manifestations WHERE id = $1",
            m_id,
        )
        .fetch_one(&ing_pool)
        .await
        .expect("fetch manifestation");
        assert_eq!(
            row.status,
            crate::models::enrichment_status::EnrichmentStatus::Pending,
            "enrichment_status not reset"
        );
        assert_eq!(row.enrichment_attempt_count, 0, "attempt_count not reset");
        assert_eq!(row.enrichment_error, None, "enrichment_error not cleared");
    }

    /// A trigger while a run is active must not release the worker's claim
    /// (a second worker could pick the row up concurrently); it records a
    /// rerun request for the completion bookkeeping instead.
    #[sqlx::test(migrations = "./migrations")]
    async fn trigger_during_active_run_defers_requeue(pool: sqlx::PgPool) {
        use axum::http::header::AUTHORIZATION;
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let marker = Uuid::new_v4().simple().to_string();
        let (_work_id, m_id) =
            test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;

        sqlx::query!(
            "UPDATE manifestations \
             SET enrichment_status = 'in_progress'::enrichment_status, \
                 enrichment_attempt_count = 1, \
                 enrichment_attempted_at = now() \
             WHERE id = $1",
            m_id,
        )
        .execute(&ing_pool)
        .await
        .expect("seed in_progress state");

        let server = test_support::db::server_with_real_pools(&app_pool, &ing_pool);
        let response = server
            .post(&format!("/api/v1/manifestations/{m_id}/enrichment/trigger"))
            .add_header(AUTHORIZATION, basic)
            .await;
        assert_eq!(
            response.status_code(),
            StatusCode::ACCEPTED,
            "body = {}",
            response.text()
        );

        let row = sqlx::query!(
            "SELECT enrichment_status AS \"status!: crate::models::enrichment_status::EnrichmentStatus\", \
                    enrichment_attempt_count, enrichment_attempted_at, \
                    enrichment_rerun_requested \
             FROM manifestations WHERE id = $1",
            m_id,
        )
        .fetch_one(&ing_pool)
        .await
        .expect("fetch manifestation");
        assert_eq!(
            row.status,
            crate::models::enrichment_status::EnrichmentStatus::InProgress,
            "the trigger must not release the active claim"
        );
        assert!(
            row.enrichment_rerun_requested,
            "the trigger must leave a rerun request for the completion path"
        );
        assert_eq!(
            row.enrichment_attempt_count, 1,
            "the active claim's attempt counter is untouched"
        );
        assert!(
            row.enrichment_attempted_at.is_some(),
            "the active claim's timestamp is untouched"
        );
    }
}
