//! `GET /api/v1/dashboard/enrichment-failures` and its counts read (admin only).

use axum::Json;
use axum::extract::{OriginalUri, State};
use axum::response::IntoResponse;
use axum_extra::extract::{Query, QueryRejection};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::auth::middleware::CurrentUser;
use crate::auth::scope::Scope;
use crate::db;
use crate::error::AppError;
use crate::models::enrichment_failure::{FailureClass, FailureEntry};
use crate::models::enrichment_status::EnrichmentStatus;
use crate::routes::cursor::FilteredIdCursor;
use crate::state::AppState;

const DEFAULT_LIMIT: i64 = 25;
const MAX_LIMIT: i64 = 100;
const MAX_SOURCE_KEY_LEN: usize = 64;
const CURSOR_TAG: &str = "ef";

pub(super) fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_failures))
        .routes(routes!(failure_counts))
}

/// A metadata source key as it appears in the registry: lowercase ASCII
/// letters, digits and underscores.
#[derive(Clone, Debug, Deserialize)]
#[serde(try_from = "String")]
struct SourceKey(String);

impl TryFrom<String> for SourceKey {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let valid = !value.is_empty()
            && value.len() <= MAX_SOURCE_KEY_LEN
            && value
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_');
        if valid {
            Ok(Self(value))
        } else {
            Err("source must be 1 to 64 lowercase letters, digits or underscores".into())
        }
    }
}

/// `?source=`, `?class=`, `?limit=` and `?cursor=` for the failures list.
#[derive(Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
struct FailuresParams {
    /// Lists only books whose primary failure came from this source.
    #[param(value_type = Option<String>)]
    source: Option<SourceKey>,
    /// Lists only books whose primary failure has this class.
    class: Option<FailureClass>,
    /// Page size; clamped to 1..=100, default 25.
    limit: Option<i64>,
    /// Opaque cursor from a previous response's `next_cursor`.
    cursor: Option<String>,
}

/// One failure of a book's latest enrichment attempt.
#[derive(Serialize, utoipa::ToSchema)]
struct FailureRef {
    /// The failing source; absent for `internal` and `unspecified`.
    source: Option<String>,
    class: FailureClass,
}

impl From<FailureEntry> for FailureRef {
    fn from(entry: FailureEntry) -> Self {
        Self {
            source: entry.source,
            class: entry.class,
        }
    }
}

/// One book whose enrichment is failing or has stopped retrying.
#[derive(Serialize, utoipa::ToSchema)]
struct FailureItem {
    manifestation_id: Uuid,
    work_id: Uuid,
    title: String,
    /// `failed` while retries remain, `skipped` once they are used up.
    status: EnrichmentStatus,
    attempt_count: i32,
    attempted_at: Option<DateTime<Utc>>,
    /// The failure the book is listed and counted under.
    primary: FailureRef,
    /// Failures from other sources in the same attempt; never counted.
    also: Vec<FailureRef>,
}

/// Response for the failures list.
#[derive(Serialize, utoipa::ToSchema)]
struct FailuresResponse {
    items: Vec<FailureItem>,
    next_cursor: Option<String>,
}

/// One failure group and the number of books listed under it.
#[derive(Serialize, utoipa::ToSchema)]
struct FailureCount {
    source: Option<String>,
    class: FailureClass,
    count: i64,
}

/// Response for the failures counts read.
#[derive(Serialize, utoipa::ToSchema)]
struct FailureCountsResponse {
    /// Every group with at least one book, by source key then class.
    by_failure: Vec<FailureCount>,
    /// Books with a failure: the sum of `by_failure`.
    total: i64,
}

fn parse_entries(stored: serde_json::Value) -> Vec<FailureEntry> {
    serde_json::from_value::<Vec<FailureEntry>>(stored)
        .ok()
        .filter(|entries| !entries.is_empty())
        .unwrap_or_else(|| {
            vec![FailureEntry {
                source: None,
                class: FailureClass::Unspecified,
            }]
        })
}

/// `GET /api/v1/dashboard/enrichment-failures`: books whose enrichment is
/// failing or has stopped retrying, one page, ordered by id (admin only).
///
/// A book appears once, under its primary failure: the failing source with
/// the lowest key, because every source in an attempt fails at the same
/// moment. Other failing sources are in `also`. Failures recorded before
/// classes existed read as `unspecified`.
///
/// # Errors
/// - [`AppError::Forbidden`] when the caller is not an admin.
/// - [`AppError::Validation`] when the cursor is malformed or was minted
///   under different filters.
/// - [`AppError::Internal`] on database errors.
#[utoipa::path(
    get,
    path = "/api/v1/dashboard/enrichment-failures",
    summary = "List books with failing enrichment",
    description = "Returns one page of books whose latest enrichment attempt failed, including those that stopped retrying, each under one primary failure. Admin only.",
    tag = "dashboard",
    params(FailuresParams),
    security(("session_cookie" = ["admin"]), ("device_token_bearer" = ["admin"]), ("oidc_jwt_bearer" = ["admin"]), ("opds_basic" = ["admin"])),
    responses(
        (status = 200, description = "One page of books with failing enrichment. Admin only.", body = FailuresResponse,
            headers(("Link" = String, description = "RFC 8288 next-page link; emitted with rel=\"next\" when more rows remain"))),
        (status = 400, description = "Malformed query parameter", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 403, description = "Caller is not an admin", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 422, description = "Malformed cursor", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
    )
)]
async fn list_failures(
    current_user: CurrentUser,
    State(state): State<AppState>,
    params: Result<Query<FailuresParams>, QueryRejection>,
    OriginalUri(uri): OriginalUri,
) -> Result<impl IntoResponse, AppError> {
    current_user.require_scope(Scope::Admin)?;
    current_user.require_admin()?;
    let Query(params) = params?;

    let limit = params.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let source = params.source.as_ref().map(|key| key.0.as_str());
    let class = params.class.map(FailureClass::as_str);
    let filter = format!("{}/{}", source.unwrap_or(""), class.unwrap_or(""));
    let after = params
        .cursor
        .as_deref()
        .map(|raw| FilteredIdCursor::parse(raw, CURSOR_TAG, &filter))
        .transpose()
        .map_err(|e| AppError::Validation(format!("invalid cursor: {e}")))?
        .map(|cursor| cursor.id);

    let mut tx = db::acquire_with_rls(&state.pool, current_user.user_id)
        .await
        .inspect_err(
            |e| tracing::error!(error = %e, "enrichment failures: acquire_with_rls failed"),
        )
        .map_err(|e| AppError::Internal(e.into()))?;
    let rows = sqlx::query!(
        r#"SELECT m.id AS "id!", m.work_id AS "work_id!", w.title AS "title!",
                  m.enrichment_status AS "status!: EnrichmentStatus",
                  m.enrichment_attempt_count AS "attempt_count!",
                  m.enrichment_attempted_at AS attempted_at,
                  m.enrichment_failures AS "failures!"
           FROM manifestations m
           JOIN works w ON w.id = m.work_id
           WHERE m.enrichment_status IN ('failed', 'skipped')
             AND m.enrichment_error IS NOT NULL
             AND ($1::uuid IS NULL OR m.id > $1)
             AND ($2::text IS NULL OR m.enrichment_failures -> 0 ->> 'source' = $2)
             AND ($3::text IS NULL
                  OR COALESCE(m.enrichment_failures -> 0 ->> 'class', 'unspecified') = $3)
           ORDER BY m.id
           LIMIT $4"#,
        after,
        source,
        class,
        limit + 1,
    )
    .fetch_all(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    tx.commit()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let page_len = usize::try_from(limit).unwrap_or(usize::MAX);
    let has_more = rows.len() > page_len;
    let items: Vec<FailureItem> = rows
        .into_iter()
        .take(page_len)
        .map(|row| {
            let mut entries = parse_entries(row.failures)
                .into_iter()
                .map(FailureRef::from);
            let primary = entries.next().unwrap_or(FailureRef {
                source: None,
                class: FailureClass::Unspecified,
            });
            FailureItem {
                manifestation_id: row.id,
                work_id: row.work_id,
                title: row.title,
                status: row.status,
                attempt_count: row.attempt_count,
                attempted_at: row.attempted_at,
                primary,
                also: entries.collect(),
            }
        })
        .collect();

    let next_cursor = match items.last() {
        Some(last) if has_more => Some(
            FilteredIdCursor {
                filter,
                id: last.manifestation_id,
            }
            .encode(CURSOR_TAG),
        ),
        _ => None,
    };

    Ok((
        crate::routes::ingestion::next_link_headers(&uri, next_cursor.as_deref()),
        Json(FailuresResponse { items, next_cursor }),
    ))
}

/// `GET /api/v1/dashboard/enrichment-failures/counts`: how many books are
/// listed under each failure group (admin only).
///
/// Independent of the list's cursor, limit and filters. A group's count
/// equals the number of items the list returns for it across all pages.
///
/// # Errors
/// - [`AppError::Forbidden`] when the caller is not an admin.
/// - [`AppError::Internal`] on database errors.
#[utoipa::path(
    get,
    path = "/api/v1/dashboard/enrichment-failures/counts",
    summary = "Count books with failing enrichment by failure",
    description = "Returns the number of books under each primary failure group (source and class) and the total. Admin only.",
    tag = "dashboard",
    security(("session_cookie" = ["admin"]), ("device_token_bearer" = ["admin"]), ("oidc_jwt_bearer" = ["admin"]), ("opds_basic" = ["admin"])),
    responses(
        (status = 200, description = "Per-group counts. Admin only.", body = FailureCountsResponse),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 403, description = "Caller is not an admin", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
    )
)]
async fn failure_counts(
    current_user: CurrentUser,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    current_user.require_scope(Scope::Admin)?;
    current_user.require_admin()?;

    let mut tx = db::acquire_with_rls(&state.pool, current_user.user_id)
        .await
        .inspect_err(
            |e| tracing::error!(error = %e, "enrichment failure counts: acquire_with_rls failed"),
        )
        .map_err(|e| AppError::Internal(e.into()))?;
    let rows = sqlx::query!(
        r#"SELECT m.enrichment_failures -> 0 ->> 'source' AS source,
                  COALESCE(m.enrichment_failures -> 0 ->> 'class', 'unspecified') AS "class!",
                  COUNT(*) AS "count!"
           FROM manifestations m
           WHERE m.enrichment_status IN ('failed', 'skipped')
             AND m.enrichment_error IS NOT NULL
           GROUP BY 1, 2"#,
    )
    .fetch_all(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    tx.commit()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let mut groups: std::collections::BTreeMap<(Option<String>, usize), i64> =
        std::collections::BTreeMap::new();
    for row in rows {
        let class_index = FailureClass::ALL
            .iter()
            .position(|class| class.as_str() == row.class)
            .unwrap_or(FailureClass::ALL.len() - 1);
        *groups.entry((row.source, class_index)).or_default() += row.count;
    }
    let mut by_failure: Vec<FailureCount> = groups
        .into_iter()
        .map(|((source, class_index), count)| FailureCount {
            source,
            class: FailureClass::ALL[class_index],
            count,
        })
        .collect();
    by_failure.sort_by(|a, b| {
        a.source
            .is_none()
            .cmp(&b.source.is_none())
            .then_with(|| a.source.cmp(&b.source))
    });
    let total = by_failure.iter().map(|group| group.count).sum();

    Ok(Json(FailureCountsResponse { by_failure, total }))
}
