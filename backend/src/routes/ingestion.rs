//! Ingestion control and visibility: the library-scan trigger
//! (`POST /api/v1/ingestion/scan`) and the admin lists of inputs needing
//! attention (`GET /api/v1/ingestion/inputs`, `.../inputs/counts`).
//!
//! THREAT: Information disclosure / privilege escalation: every handler
//! enforces the admin scope and role before any database access, so a
//! non-admin caller receives 403 and never reaches the queries. The inputs
//! list is the one admin surface that names files: each item carries the
//! ingestion-relative path only, never an absolute or library path, with
//! non-UTF-8 bytes escaped. Stored failure text (`reason`, which can carry
//! archive-controlled names and internal error text) never leaves the
//! server; the wire carries closed reason classes, and legacy rows without a
//! structured class surface as `unspecified`. Every list is bounded by a
//! clamped limit and a keyset cursor bound to its filter.

use axum::Json;
use axum::extract::{OriginalUri, State};
use axum::http::header::LINK;
use axum::http::{HeaderMap, HeaderValue};
use axum::response::IntoResponse;
use axum_extra::extract::{Query, QueryRejection};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::auth::middleware::CurrentUser;
use crate::auth::scope::Scope;
use crate::error::AppError;
use crate::models::ingestion_input::{AttemptOutcome, InputStatus, display_path};
use crate::routes::cursor::FilteredIdCursor;
use crate::routes::library::build_next_url;
use crate::services;
use crate::state::AppState;

const DEFAULT_INPUTS_LIMIT: i64 = 25;
const MAX_INPUTS_LIMIT: i64 = 100;
const INPUTS_CURSOR_TAG: &str = "ii";

/// Build the ingestion router: the scan trigger and the inputs lists.
///
/// # Invariants
/// - Admin-only: every handler enforces `CurrentUser::require_admin` before
///   doing any work.
///
/// The admin gate controls discovery commands submitted to the ingestion owner
/// and the visibility of retained inputs.
pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(scan))
        .routes(routes!(list_inputs))
        .routes(routes!(input_counts))
}

type ScanResponse = services::ingestion::DiscoveryResult;

/// `POST /api/v1/ingestion/scan` — request discovery of the ingestion
/// directory (admin only).
///
/// # Errors
/// - [`AppError::Forbidden`] when the caller is not an admin.
/// - [`AppError::Internal`] when the scan fails at the service layer.
#[utoipa::path(
    post,
    path = "/api/v1/ingestion/scan",
    summary = "Scan the ingestion directory",
    description = "Discovers current ingestion inputs and reports queued, deferred and suppressed counts with the activity monitor. Admin only.",
    tag = "ingestion",
    security(("session_cookie" = ["admin"]), ("device_token_bearer" = ["admin"]), ("oidc_jwt_bearer" = ["admin"]), ("opds_basic" = ["admin"])),
    responses(
        (status = 202, description = "Discovery complete; processing follows readiness. Admin only.", body = ScanResponse),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 403, description = "Caller is not an admin", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
    )
)]
async fn scan(
    current_user: CurrentUser,
    State(state): State<AppState>,
) -> Result<(axum::http::StatusCode, Json<ScanResponse>), AppError> {
    current_user.require_scope(Scope::Admin)?;
    current_user.require_admin()?;

    let result = state.ingestion.scan().await.map_err(AppError::Internal)?;
    Ok((axum::http::StatusCode::ACCEPTED, Json(result)))
}

/// Closed class an input is listed under. Rejected inputs carry one of the
/// first five, declared most severe first; the rest classify the other
/// attention states. Raw stored failure text never maps to anything but
/// `unspecified`.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
enum InputReason {
    UnsafeContents,
    Damaged,
    InvalidStructure,
    OverLimits,
    Unspecified,
    NeedsChange,
    RetriesExhausted,
    FormatNotAccepted,
}

impl InputReason {
    const ALL: [Self; 8] = [
        Self::UnsafeContents,
        Self::Damaged,
        Self::InvalidStructure,
        Self::OverLimits,
        Self::Unspecified,
        Self::NeedsChange,
        Self::RetriesExhausted,
        Self::FormatNotAccepted,
    ];

    const fn as_str(self) -> &'static str {
        match self {
            Self::UnsafeContents => "unsafe_contents",
            Self::Damaged => "damaged",
            Self::InvalidStructure => "invalid_structure",
            Self::OverLimits => "over_limits",
            Self::Unspecified => "unspecified",
            Self::NeedsChange => "needs_change",
            Self::RetriesExhausted => "retries_exhausted",
            Self::FormatNotAccepted => "format_not_accepted",
        }
    }

    fn parse(value: &str) -> Result<Self, AppError> {
        Self::ALL
            .into_iter()
            .find(|reason| reason.as_str() == value)
            .ok_or_else(|| AppError::Internal(anyhow::anyhow!("unknown ingestion reason class")))
    }
}

/// `?reason=`, `?limit=` and `?cursor=` for `GET /api/v1/ingestion/inputs`.
#[derive(Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
struct InputsParams {
    /// Lists only inputs whose primary reason is this class. Absent lists
    /// every input needing attention, which excludes `format_not_accepted`.
    reason: Option<InputReason>,
    /// Page size; clamped to 1..=100, default 25.
    limit: Option<i64>,
    /// Opaque cursor from a previous response's `next_cursor`.
    cursor: Option<String>,
}

/// One ingestion input that needs attention.
#[derive(Serialize, utoipa::ToSchema)]
struct InputItem {
    id: Uuid,
    /// Ingestion-relative path; bytes that are not UTF-8 are escaped.
    path: String,
    status: InputStatus,
    /// Outcome of the latest finished attempt on the current generation.
    outcome: Option<AttemptOutcome>,
    /// The class the input is listed and counted under; equals the first
    /// entry of `reasons`.
    primary_reason: InputReason,
    /// Every class that applies, primary first. Secondary classes are never
    /// counted.
    reasons: Vec<InputReason>,
    observed_at: DateTime<Utc>,
    completed_at: Option<DateTime<Utc>>,
}

/// Response for `GET /api/v1/ingestion/inputs`.
#[derive(Serialize, utoipa::ToSchema)]
struct InputsResponse {
    items: Vec<InputItem>,
    next_cursor: Option<String>,
}

/// One class and the number of inputs listed under it.
#[derive(Serialize, utoipa::ToSchema)]
struct ReasonCount {
    reason: InputReason,
    count: i64,
}

/// Response for `GET /api/v1/ingestion/inputs/counts`.
#[derive(Serialize, utoipa::ToSchema)]
struct InputCountsResponse {
    /// Every class with at least one input, most severe first. Includes
    /// `format_not_accepted`.
    by_reason: Vec<ReasonCount>,
    /// Inputs needing attention: the sum of `by_reason` without
    /// `format_not_accepted`.
    attention_total: i64,
}

/// `GET /api/v1/ingestion/inputs`: inputs needing attention, one page,
/// ordered by id (admin only).
///
/// An input is listed once, under its primary reason. Rejected inputs use
/// the most severe class the validator recorded; operational failures are
/// listed only when they need a change or have used their retries, so an
/// input waiting for its next automatic attempt is absent.
///
/// # Errors
/// - [`AppError::Forbidden`] when the caller is not an admin.
/// - [`AppError::Validation`] when the cursor is malformed or was minted
///   under a different `reason`.
/// - [`AppError::Internal`] on database errors.
#[utoipa::path(
    get,
    path = "/api/v1/ingestion/inputs",
    summary = "List ingestion inputs needing attention",
    description = "Returns one page of ingestion inputs that were rejected, need a change or have used their retries, each under one primary reason class. With reason=format_not_accepted it lists ignored files instead. Admin only.",
    tag = "ingestion",
    params(InputsParams),
    security(("session_cookie" = ["admin"]), ("device_token_bearer" = ["admin"]), ("oidc_jwt_bearer" = ["admin"]), ("opds_basic" = ["admin"])),
    responses(
        (status = 200, description = "One page of inputs needing attention. Admin only.", body = InputsResponse,
            headers(("Link" = String, description = "RFC 8288 next-page link; emitted with rel=\"next\" when more rows remain"))),
        (status = 400, description = "Malformed query parameter", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 403, description = "Caller is not an admin", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 422, description = "Malformed cursor", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
    )
)]
async fn list_inputs(
    current_user: CurrentUser,
    State(state): State<AppState>,
    params: Result<Query<InputsParams>, QueryRejection>,
    OriginalUri(uri): OriginalUri,
) -> Result<impl IntoResponse, AppError> {
    current_user.require_scope(Scope::Admin)?;
    current_user.require_admin()?;
    let Query(params) = params?;

    let limit = params
        .limit
        .unwrap_or(DEFAULT_INPUTS_LIMIT)
        .clamp(1, MAX_INPUTS_LIMIT);
    let filter = params.reason.map_or("", InputReason::as_str);
    let after = params
        .cursor
        .as_deref()
        .map(|raw| FilteredIdCursor::parse(raw, INPUTS_CURSOR_TAG, filter))
        .transpose()
        .map_err(|e| AppError::Validation(format!("invalid cursor: {e}")))?
        .map(|cursor| cursor.id);

    let rows = sqlx::query!(
        r#"SELECT i.id AS "id!", i.source_path AS "source_path!",
                  i.status AS "status!: InputStatus",
                  latest.outcome AS "outcome: AttemptOutcome",
                  c.class AS "class!",
                  i.rejection_reasons AS "rejection_reasons!",
                  i.observed_at AS "observed_at!", i.completed_at
           FROM ingestion_inputs i
           LEFT JOIN LATERAL (
               SELECT j.outcome FROM ingestion_jobs j
               WHERE j.input_id = i.id AND j.input_generation = i.generation AND j.outcome IS NOT NULL
               ORDER BY j.created_at DESC, j.id DESC LIMIT 1
           ) latest ON TRUE
           CROSS JOIN LATERAL (
               SELECT CASE i.status
                   WHEN 'rejected' THEN COALESCE(i.rejection_reasons[1], 'unspecified')
                   WHEN 'not_accepted' THEN 'format_not_accepted'
                   WHEN 'operational_failure' THEN CASE latest.outcome
                       WHEN 'needs_change' THEN 'needs_change'
                       WHEN 'transient_input' THEN CASE WHEN (
                           SELECT COUNT(*) FROM ingestion_jobs t
                           WHERE t.input_id = i.id AND t.input_generation = i.generation
                             AND t.created_at >= i.retry_reset_at AND t.outcome = 'transient_input'
                       ) >= 6 THEN 'retries_exhausted' END
                   END
               END AS class
           ) c
           WHERE i.status IN ('rejected', 'not_accepted', 'operational_failure')
             AND c.class IS NOT NULL
             AND ($1::uuid IS NULL OR i.id > $1)
             AND CASE WHEN $2::text IS NULL THEN c.class <> 'format_not_accepted' ELSE c.class = $2 END
           ORDER BY i.id
           LIMIT $3"#,
        after,
        params.reason.map(InputReason::as_str),
        limit + 1,
    )
    .fetch_all(&state.pool)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    let page_len = usize::try_from(limit).unwrap_or(usize::MAX);
    let has_more = rows.len() > page_len;
    let items = rows
        .into_iter()
        .take(page_len)
        .map(|row| {
            let primary = InputReason::parse(&row.class)?;
            let reasons = match row.status {
                InputStatus::Rejected if !row.rejection_reasons.is_empty() => row
                    .rejection_reasons
                    .iter()
                    .map(|reason| InputReason::parse(reason))
                    .collect::<Result<Vec<_>, _>>()?,
                _ => vec![primary],
            };
            Ok(InputItem {
                id: row.id,
                path: display_path(&row.source_path),
                status: row.status,
                outcome: row.outcome,
                primary_reason: primary,
                reasons,
                observed_at: row.observed_at,
                completed_at: row.completed_at,
            })
        })
        .collect::<Result<Vec<_>, AppError>>()?;

    let next_cursor = match items.last() {
        Some(last) if has_more => Some(
            FilteredIdCursor {
                filter: filter.to_owned(),
                id: last.id,
            }
            .encode(INPUTS_CURSOR_TAG),
        ),
        _ => None,
    };

    let mut headers = HeaderMap::new();
    if let Some(cursor) = &next_cursor {
        let next_url = build_next_url(&uri, cursor);
        match HeaderValue::from_str(&format!("<{next_url}>; rel=\"next\"")) {
            Ok(value) => {
                headers.insert(LINK, value);
            }
            Err(e) => {
                tracing::warn!(error = %e, "could not encode Link rel=next header");
            }
        }
    }

    Ok((headers, Json(InputsResponse { items, next_cursor })))
}

/// `GET /api/v1/ingestion/inputs/counts`: how many inputs are listed under
/// each class (admin only).
///
/// Independent of the list's cursor, limit and reason filter. A class's count
/// equals the number of items the list returns for it across all pages.
///
/// # Errors
/// - [`AppError::Forbidden`] when the caller is not an admin.
/// - [`AppError::Internal`] on database errors.
#[utoipa::path(
    get,
    path = "/api/v1/ingestion/inputs/counts",
    summary = "Count ingestion inputs by reason",
    description = "Returns the number of inputs under each primary reason class, and the total needing attention (every class except format_not_accepted). Admin only.",
    tag = "ingestion",
    security(("session_cookie" = ["admin"]), ("device_token_bearer" = ["admin"]), ("oidc_jwt_bearer" = ["admin"]), ("opds_basic" = ["admin"])),
    responses(
        (status = 200, description = "Per-class counts. Admin only.", body = InputCountsResponse),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 403, description = "Caller is not an admin", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
    )
)]
async fn input_counts(
    current_user: CurrentUser,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    current_user.require_scope(Scope::Admin)?;
    current_user.require_admin()?;

    let rows = sqlx::query!(
        r#"SELECT c.class AS "class!", COUNT(*) AS "count!"
           FROM ingestion_inputs i
           LEFT JOIN LATERAL (
               SELECT j.outcome FROM ingestion_jobs j
               WHERE j.input_id = i.id AND j.input_generation = i.generation AND j.outcome IS NOT NULL
               ORDER BY j.created_at DESC, j.id DESC LIMIT 1
           ) latest ON TRUE
           CROSS JOIN LATERAL (
               SELECT CASE i.status
                   WHEN 'rejected' THEN COALESCE(i.rejection_reasons[1], 'unspecified')
                   WHEN 'not_accepted' THEN 'format_not_accepted'
                   WHEN 'operational_failure' THEN CASE latest.outcome
                       WHEN 'needs_change' THEN 'needs_change'
                       WHEN 'transient_input' THEN CASE WHEN (
                           SELECT COUNT(*) FROM ingestion_jobs t
                           WHERE t.input_id = i.id AND t.input_generation = i.generation
                             AND t.created_at >= i.retry_reset_at AND t.outcome = 'transient_input'
                       ) >= 6 THEN 'retries_exhausted' END
                   END
               END AS class
           ) c
           WHERE i.status IN ('rejected', 'not_accepted', 'operational_failure')
             AND c.class IS NOT NULL
           GROUP BY c.class"#,
    )
    .fetch_all(&state.pool)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    let counts = rows
        .into_iter()
        .map(|row| Ok((InputReason::parse(&row.class)?, row.count)))
        .collect::<Result<Vec<_>, AppError>>()?;
    let by_reason: Vec<ReasonCount> = InputReason::ALL
        .into_iter()
        .filter_map(|reason| {
            counts
                .iter()
                .find(|(found, _)| *found == reason)
                .map(|(_, count)| ReasonCount {
                    reason,
                    count: *count,
                })
        })
        .collect();
    let attention_total = by_reason
        .iter()
        .filter(|entry| entry.reason != InputReason::FormatNotAccepted)
        .map(|entry| entry.count)
        .sum();

    Ok(Json(InputCountsResponse {
        by_reason,
        attention_total,
    }))
}

#[cfg(test)]
mod inputs_tests;

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;

    use crate::test_support;

    #[tokio::test]
    async fn scan_returns_401_without_auth() {
        let server = test_support::test_server();
        let response = server.post("/api/v1/ingestion/scan").await;
        assert_eq!(response.status_code(), StatusCode::UNAUTHORIZED);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn capability_ingestion_coordinator_admin_scan_returns_discovery_counts(
        pool: sqlx::PgPool,
    ) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let (_id, auth) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let mut state = test_support::test_state();
        state.pool = app_pool;
        let (handle, mut commands) = crate::services::ingestion::coordinator_channel();
        state.ingestion = handle;
        let owner = tokio::spawn(async move {
            let reply = commands.recv().await.unwrap();
            reply
                .send(Ok(crate::services::ingestion::DiscoveryResult {
                    queued: 1,
                    deferred: 2,
                    suppressed: 3,
                    monitor: "/api/v1/dashboard/activity",
                }))
                .unwrap();
        });
        let server = axum_test::TestServer::new(crate::build_router_with_session_store(
            state,
            tower_sessions::MemoryStore::default(),
        ));
        let response = server
            .post("/api/v1/ingestion/scan")
            .add_header(axum::http::header::AUTHORIZATION, auth)
            .await;
        assert_eq!(response.status_code(), StatusCode::ACCEPTED);
        let body: serde_json::Value = response.json();
        assert_eq!(body["queued"], 1);
        assert_eq!(body["deferred"], 2);
        assert_eq!(body["suppressed"], 3);
        assert_eq!(body["monitor"], "/api/v1/dashboard/activity");
        owner.await.unwrap();
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn capability_ingestion_coordinator_non_admin_cannot_submit_scan(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let (_id, auth) =
            test_support::db::create_adult_and_basic_auth(&app_pool, "scan-adult").await;
        let mut state = test_support::test_state();
        state.pool = app_pool;
        let (handle, mut commands) = crate::services::ingestion::coordinator_channel();
        state.ingestion = handle;
        let server = axum_test::TestServer::new(crate::build_router_with_session_store(
            state,
            tower_sessions::MemoryStore::default(),
        ));
        let response = server
            .post("/api/v1/ingestion/scan")
            .add_header(axum::http::header::AUTHORIZATION, auth)
            .await;
        assert_eq!(response.status_code(), StatusCode::FORBIDDEN);
        assert!(commands.try_recv().is_err());
    }
}
