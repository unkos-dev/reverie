//! `/api/v1/shelves*` CRUD JSON routes.
//!
//! Shelves are user-scoped curation buckets. The shelves and
//! shelf_items tables ship without RLS — the runtime role
//! (`reverie_app`) is grant-gated, and ownership is enforced at the
//! handler boundary via explicit `WHERE user_id = current_user`
//! predicates. Mismatched ownership resolves to `AppError::NotFound`
//! so existence is not leaked across users.
//!
//! THREAT: Every mutating handler enforces shelf ownership in the SQL
//! `WHERE user_id = $current_user` predicate before any DML. The DB
//! layer carries no RLS on `shelves` / `shelf_items`, so removing
//! that predicate would silently expose every user's shelves to every
//! authenticated caller. The `add_shelf_item` handler additionally
//! runs an RLS-scoped probe on the target manifestation to prevent
//! shelf-existence probing via random UUIDs (relevant for children,
//! whose manifestation visibility is shelf-gated).
//!
//! # ETag / If-Match contract
//!
//! Every shelf read endpoint emits `ETag: "<updated_at RFC3339>"`.
//! The reorder endpoint (`PUT /api/v1/shelves/{id}/items`) requires the
//! caller to echo that value as `If-Match`; the handler runs
//! `SELECT updated_at FROM shelves WHERE id = $1 FOR UPDATE` inside
//! the transaction and refuses the write with 412 if the value
//! differs. All items-mutation handlers explicitly issue
//! `UPDATE shelves SET updated_at = now() WHERE id = $1` in the same
//! transaction so add/remove also bump the ETag — without that, a
//! follow-up reorder PUT would 412 spuriously.

use axum::extract::{OriginalUri, Path, State};
use axum::http::header::{ETAG, IF_MATCH, LINK};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::IntoResponse;
use axum_extra::extract::{Query, QueryRejection};
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, QueryBuilder, Row};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::auth::middleware::CurrentUser;
use crate::auth::scope::Scope;
use crate::error::AppError;
use crate::models::shelf::{Shelf, ShelfItem};
use crate::routes::cursor::{ShelfCursor, ShelfItemCursor};
use crate::routes::library::{build_next_url, split_page};
use crate::state::AppState;

#[cfg(test)]
mod tests;

/// Build the `/api/v1/shelves*` router as an [`OpenApiRouter`] so each
/// handler's `#[utoipa::path]` contributes to the generated spec (a missing
/// annotation fails to compile). Merged into `crate::openapi::pilot_router`
/// and split into its runtime and spec halves there.
pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_shelves, create_shelf))
        .routes(routes!(rename_shelf, delete_shelf, get_shelf_with_items))
        .routes(routes!(add_shelf_item, reorder_shelf_items))
        .routes(routes!(remove_shelf_item))
}

/// Format the `updated_at` timestamp as an RFC 9110 entity-tag (the
/// header value carries the RFC 3339 timestamp wrapped in double
/// quotes per RFC 9110 §8.8.3).
fn etag_header(updated_at: OffsetDateTime) -> Result<HeaderValue, AppError> {
    let formatted = updated_at
        .format(&Rfc3339)
        .map_err(|e| AppError::Internal(e.into()))?;
    HeaderValue::from_str(&format!("\"{formatted}\"")).map_err(|e| AppError::Internal(e.into()))
}

/// Parse the `If-Match` request header into an [`OffsetDateTime`].
///
/// Strips the RFC 9110 entity-tag quoting and decodes the inner
/// RFC 3339 timestamp. Returns:
/// - `Ok(None)` when the header is absent (handler typically responds
///   with [`AppError::IfMatchRequired`]).
/// - `Ok(Some(_))` on a well-formed entity-tag.
/// - `Err(AppError::Validation)` when the value is malformed.
fn parse_if_match(headers: &HeaderMap) -> Result<Option<OffsetDateTime>, AppError> {
    let Some(raw) = headers.get(IF_MATCH) else {
        return Ok(None);
    };
    let value = raw
        .to_str()
        .map_err(|_| AppError::Validation("If-Match header must be ASCII".into()))?;
    let trimmed = value.trim();
    // RFC 9110 §13.1.2: If-Match requires strong comparison. Weak
    // validators (`W/"..."`) are semantically wrong for optimistic
    // concurrency and reject explicitly rather than silently being
    // promoted to strong.
    if trimmed.starts_with("W/") {
        return Err(AppError::Validation(
            "weak entity-tags (W/\"...\") not accepted for If-Match".into(),
        ));
    }
    // Strong tag MUST be wrapped in double quotes per RFC 9110 §8.8.3.
    let Some(stripped) = trimmed.strip_prefix('"').and_then(|s| s.strip_suffix('"')) else {
        return Err(AppError::Validation(
            "If-Match value must be a quoted entity-tag".into(),
        ));
    };
    let ts = OffsetDateTime::parse(stripped, &Rfc3339).map_err(|e| {
        AppError::Validation(format!(
            "If-Match value is not a valid RFC3339 entity-tag: {e}"
        ))
    })?;
    Ok(Some(ts))
}

/// Cursor pagination input for `GET /api/v1/shelves`.
#[derive(Debug, Default, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
struct ShelfListParams {
    /// Opaque cursor from a previous response's `next_cursor`; absent
    /// returns the first page.
    #[serde(default)]
    cursor: Option<String>,
}

/// `GET /api/v1/shelves` response envelope. Carries the page rows plus
/// the opaque cursor for the following page; pagination is also
/// signalled via the RFC 8288 `Link` header on the response.
///
/// Private to the route module — handler-internal wire shape.
#[derive(Debug, Serialize, utoipa::ToSchema)]
struct ShelfListResponse {
    items: Vec<Shelf>,
    next_cursor: Option<String>,
}

/// `GET /api/v1/shelves` — list the caller's shelves, keyset-paginated
/// over `(is_system DESC, name ASC, id ASC)` — every list is bounded by
/// construction.
///
/// # Errors
/// - [`AppError::Validation`] when the cursor is malformed.
/// - [`AppError::Internal`] on database errors.
#[utoipa::path(
    get,
    path = "/api/v1/shelves",
    tag = "shelves",
    params(ShelfListParams),
    security(("session_cookie" = ["read"]), ("device_token_bearer" = ["read"]), ("opds_basic" = ["read"])),
    responses(
        (status = 200, description = "One page of the caller's shelves, system shelves first then by name", body = ShelfListResponse,
            headers(("Link" = String, description = "RFC 8288 next-page link; emitted with rel=\"next\" when more rows remain"))),
        (status = 400, description = "Malformed query parameter", body = crate::openapi::ProblemDetails),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails),
        (status = 422, description = "Malformed cursor", body = crate::openapi::ProblemDetails)
    )
)]
async fn list_shelves(
    current_user: CurrentUser,
    State(state): State<AppState>,
    params: Result<Query<ShelfListParams>, QueryRejection>,
    OriginalUri(uri): OriginalUri,
) -> Result<impl IntoResponse, AppError> {
    let Query(params) = params?;
    let page_size = i64::from(state.config.opds.page_size);
    let cursor = params
        .cursor
        .as_deref()
        .map(ShelfCursor::parse)
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
        .map(|r| Shelf {
            id: r.get("id"),
            name: r.get("name"),
            is_system: r.get("is_system"),
            created_at: r.get("created_at"),
            updated_at: r.get("updated_at"),
            item_count: r.get("item_count"),
        })
        .collect();

    let next_cursor = match page_rows.last() {
        Some(last) if has_more => Some(
            ShelfCursor {
                is_system: last.get("is_system"),
                name: last.get("name"),
                id: last.get("id"),
            }
            .encode(),
        ),
        _ => None,
    };

    let mut headers = HeaderMap::new();
    if let Some(ref nc) = next_cursor {
        let next_url = build_next_url(&uri, nc);
        match HeaderValue::from_str(&format!("<{next_url}>; rel=\"next\"")) {
            Ok(value) => {
                headers.insert(LINK, value);
            }
            Err(e) => {
                tracing::warn!(error = %e, %next_url, "could not encode Link rel=next header");
            }
        }
    }

    Ok((
        headers,
        axum::Json(ShelfListResponse { items, next_cursor }),
    ))
}

/// Body for `POST /api/v1/shelves`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
struct CreateShelfRequest {
    /// Shelf display name (non-empty after trim).
    name: String,
}

/// `POST /api/v1/shelves` — create a new (non-system) shelf for the
/// caller.
///
/// # Errors
/// - [`AppError::Forbidden`] when the caller is a child account.
/// - [`AppError::Validation`] when the body is missing or `name` is
///   empty / whitespace-only.
/// - [`AppError::Internal`] on database errors.
#[utoipa::path(
    post,
    path = "/api/v1/shelves",
    tag = "shelves",
    request_body = CreateShelfRequest,
    security(("session_cookie" = ["write"]), ("device_token_bearer" = ["write"]), ("opds_basic" = ["write"])),
    responses(
        (status = 201, description = "Shelf created", body = Shelf,
         headers(("ETag" = String, description = "Entity-tag carrying the shelf's updated_at (RFC 3339, quoted per RFC 9110)"))),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails),
        (status = 403, description = "Caller is a child account", body = crate::openapi::ProblemDetails),
        (status = 422, description = "Missing body or empty name", body = crate::openapi::ProblemDetails)
    )
)]
async fn create_shelf(
    current_user: CurrentUser,
    State(state): State<AppState>,
    body: Result<axum::Json<CreateShelfRequest>, axum::extract::rejection::JsonRejection>,
) -> Result<impl IntoResponse, AppError> {
    current_user.require_scope(Scope::Write)?;
    current_user.require_not_child()?;
    let axum::Json(req) = body.map_err(|e| AppError::Validation(e.body_text()))?;
    let name = req.name.trim();
    if name.is_empty() {
        return Err(AppError::Validation("shelf name must not be empty".into()));
    }

    let row = sqlx::query!(
        r#"
        INSERT INTO shelves (user_id, name)
        VALUES ($1, $2)
        RETURNING id           AS "id!",
                  name         AS "name!",
                  is_system    AS "is_system!",
                  created_at   AS "created_at!",
                  updated_at   AS "updated_at!"
        "#,
        current_user.user_id,
        name,
    )
    .fetch_one(&state.pool)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    let shelf = Shelf {
        id: row.id,
        name: row.name,
        is_system: row.is_system,
        created_at: row.created_at,
        updated_at: row.updated_at,
        item_count: 0,
    };

    let mut headers = HeaderMap::new();
    headers.insert(ETAG, etag_header(shelf.updated_at)?);
    Ok((StatusCode::CREATED, headers, axum::Json(shelf)))
}

/// Body for `PATCH /api/v1/shelves/{id}`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
struct RenameShelfRequest {
    /// New shelf display name (non-empty after trim).
    name: String,
}

/// `PATCH /api/v1/shelves/{id}` — rename a non-system shelf owned by
/// the caller.
///
/// # Errors
/// - [`AppError::Forbidden`] when the caller is a child account.
/// - [`AppError::NotFound`] when the shelf does not exist or belongs
///   to another user (existence-not-leaked).
/// - [`AppError::SystemShelfImmutable`] (409) when the target shelf
///   has `is_system = TRUE`.
/// - [`AppError::Validation`] when `name` is missing or
///   whitespace-only.
/// - [`AppError::Internal`] on database errors.
#[utoipa::path(
    patch,
    path = "/api/v1/shelves/{id}",
    tag = "shelves",
    params(("id" = Uuid, Path, description = "Shelf id")),
    request_body = RenameShelfRequest,
    security(("session_cookie" = ["write"]), ("device_token_bearer" = ["write"]), ("opds_basic" = ["write"])),
    responses(
        (status = 200, description = "Renamed shelf", body = Shelf,
         headers(("ETag" = String, description = "Entity-tag carrying the shelf's updated_at (RFC 3339, quoted per RFC 9110)"))),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails),
        (status = 403, description = "Caller is a child account", body = crate::openapi::ProblemDetails),
        (status = 404, description = "Shelf missing or owned by another user (existence-not-leaked)", body = crate::openapi::ProblemDetails),
        (status = 409, description = "System shelves cannot be renamed", body = crate::openapi::ProblemDetails),
        (status = 422, description = "Missing body or empty name", body = crate::openapi::ProblemDetails)
    )
)]
async fn rename_shelf(
    current_user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    body: Result<axum::Json<RenameShelfRequest>, axum::extract::rejection::JsonRejection>,
) -> Result<impl IntoResponse, AppError> {
    current_user.require_scope(Scope::Write)?;
    current_user.require_not_child()?;
    let axum::Json(req) = body.map_err(|e| AppError::Validation(e.body_text()))?;
    let name = req.name.trim();
    if name.is_empty() {
        return Err(AppError::Validation("shelf name must not be empty".into()));
    }

    let mut tx = state
        .pool
        .begin()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    let existing = sqlx::query!(
        r#"SELECT is_system AS "is_system!" FROM shelves
           WHERE id = $1 AND user_id = $2 FOR UPDATE"#,
        id,
        current_user.user_id,
    )
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?
    .ok_or(AppError::NotFound)?;
    if existing.is_system {
        return Err(AppError::SystemShelfImmutable);
    }
    let row = sqlx::query!(
        r#"
        UPDATE shelves SET name = $1
        WHERE id = $2 AND user_id = $3
        RETURNING id           AS "id!",
                  name         AS "name!",
                  is_system    AS "is_system!",
                  created_at   AS "created_at!",
                  updated_at   AS "updated_at!"
        "#,
        name,
        id,
        current_user.user_id,
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    let item_count: i64 = sqlx::query_scalar!(
        r#"SELECT COUNT(*) AS "n!" FROM shelf_items WHERE shelf_id = $1"#,
        id,
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    tx.commit()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let shelf = Shelf {
        id: row.id,
        name: row.name,
        is_system: row.is_system,
        created_at: row.created_at,
        updated_at: row.updated_at,
        item_count,
    };
    let mut headers = HeaderMap::new();
    headers.insert(ETAG, etag_header(shelf.updated_at)?);
    Ok((headers, axum::Json(shelf)))
}

/// `DELETE /api/v1/shelves/{id}` — delete a non-system shelf.
///
/// # Errors
/// - [`AppError::Forbidden`] when the caller is a child account.
/// - [`AppError::NotFound`] when the shelf is missing / not owned by
///   the caller.
/// - [`AppError::SystemShelfImmutable`] when `is_system = TRUE`.
/// - [`AppError::Internal`] on database errors.
#[utoipa::path(
    delete,
    path = "/api/v1/shelves/{id}",
    tag = "shelves",
    params(("id" = Uuid, Path, description = "Shelf id")),
    security(("session_cookie" = ["write"]), ("device_token_bearer" = ["write"]), ("opds_basic" = ["write"])),
    responses(
        (status = 204, description = "Shelf deleted"),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails),
        (status = 403, description = "Caller is a child account", body = crate::openapi::ProblemDetails),
        (status = 404, description = "Shelf missing or owned by another user (existence-not-leaked)", body = crate::openapi::ProblemDetails),
        (status = 409, description = "System shelves cannot be deleted", body = crate::openapi::ProblemDetails)
    )
)]
async fn delete_shelf(
    current_user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    current_user.require_scope(Scope::Write)?;
    current_user.require_not_child()?;
    let mut tx = state
        .pool
        .begin()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    let row = sqlx::query!(
        r#"SELECT is_system AS "is_system!" FROM shelves
           WHERE id = $1 AND user_id = $2 FOR UPDATE"#,
        id,
        current_user.user_id,
    )
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?
    .ok_or(AppError::NotFound)?;
    if row.is_system {
        return Err(AppError::SystemShelfImmutable);
    }
    sqlx::query!("DELETE FROM shelves WHERE id = $1", id)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    tx.commit()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    Ok(StatusCode::NO_CONTENT)
}

/// Cursor pagination input for the items page of
/// `GET /api/v1/shelves/{id}`.
#[derive(Debug, Default, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
struct ShelfItemsParams {
    /// Opaque items cursor from a previous response's `next_cursor`;
    /// absent returns the shelf with its first page of items.
    #[serde(default)]
    cursor: Option<String>,
}

/// `GET /api/v1/shelves/{id}` response envelope: shelf identity plus
/// one page of ordered items and the cursor for the next page.
///
/// Private to the route module — handler-internal wire shape
/// (pagination is a transport concern; the `models::shelf` DTOs stay
/// transport-free).
#[derive(Debug, Serialize, utoipa::ToSchema)]
struct ShelfDetailResponse {
    id: Uuid,
    name: String,
    is_system: bool,
    created_at: OffsetDateTime,
    /// `shelves.updated_at` — the `ETag` value the client echoes as
    /// `If-Match` on the reorder PUT.
    updated_at: OffsetDateTime,
    /// One page of items ordered by `position ASC, added_at ASC,
    /// manifestation_id ASC`.
    items: Vec<ShelfItem>,
    /// Opaque cursor for the next items page; `null` on the last page.
    next_cursor: Option<String>,
}

/// `GET /api/v1/shelves/{id}` — shelf identity plus ordered items,
/// keyset-paginated over `(position, added_at, manifestation_id)` — every
/// list is bounded by construction. The ETag/If-Match contract is
/// unaffected by item paging:
/// the entity-tag carries the shelf's `updated_at` regardless of which
/// items page is requested.
///
/// # Errors
/// - [`AppError::NotFound`] when missing or not owned by caller.
/// - [`AppError::Validation`] when the cursor is malformed.
/// - [`AppError::Internal`] on database errors.
#[utoipa::path(
    get,
    path = "/api/v1/shelves/{id}",
    tag = "shelves",
    params(("id" = Uuid, Path, description = "Shelf id"), ShelfItemsParams),
    security(("session_cookie" = ["read"]), ("device_token_bearer" = ["read"]), ("opds_basic" = ["read"])),
    responses(
        (status = 200, description = "Shelf identity plus one page of ordered items", body = ShelfDetailResponse,
         headers(
            ("ETag" = String, description = "Entity-tag carrying the shelf's updated_at (RFC 3339, quoted per RFC 9110); echo as If-Match on reorder"),
            ("Link" = String, description = "RFC 8288 next-page link; emitted with rel=\"next\" when more items remain")
         )),
        (status = 400, description = "Malformed query parameter", body = crate::openapi::ProblemDetails),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails),
        (status = 404, description = "Shelf missing or owned by another user (existence-not-leaked)", body = crate::openapi::ProblemDetails),
        (status = 422, description = "Malformed cursor", body = crate::openapi::ProblemDetails)
    )
)]
async fn get_shelf_with_items(
    current_user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    params: Result<Query<ShelfItemsParams>, QueryRejection>,
    OriginalUri(uri): OriginalUri,
) -> Result<impl IntoResponse, AppError> {
    let Query(params) = params?;
    let page_size = i64::from(state.config.opds.page_size);
    let cursor = params
        .cursor
        .as_deref()
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
        "SELECT manifestation_id, position, added_at \
           FROM shelf_items WHERE shelf_id = ",
    );
    qb.push_bind(id);
    if let Some(c) = &cursor {
        // All-ASC sort — a single row-tuple comparison is total. The
        // manifestation_id tiebreaker is load-bearing: neither position
        // nor added_at is unique per shelf.
        qb.push(" AND (position, added_at, manifestation_id) > (");
        qb.push_bind(c.position);
        qb.push(", ");
        qb.push_bind(c.added_at);
        qb.push(", ");
        qb.push_bind(c.manifestation_id);
        qb.push(")");
    }
    qb.push(" ORDER BY position ASC, added_at ASC, manifestation_id ASC LIMIT ");
    qb.push_bind(page_size + 1);

    let rows = qb
        .build()
        .fetch_all(&state.pool)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let (page_rows, has_more) = split_page(&rows, page_size);

    let items: Vec<ShelfItem> = page_rows
        .iter()
        .map(|r| ShelfItem {
            manifestation_id: r.get("manifestation_id"),
            position: r.get("position"),
            added_at: r.get("added_at"),
        })
        .collect();

    let next_cursor = match page_rows.last() {
        Some(last) if has_more => {
            let manifestation_id: Uuid = last.get("manifestation_id");
            Some(
                ShelfItemCursor {
                    position: last.get("position"),
                    added_at: last.get("added_at"),
                    manifestation_id,
                }
                .encode()
                .map_err(|e| {
                    tracing::warn!(error = %e, %manifestation_id, "failed to encode shelf-items cursor");
                    AppError::Internal(e.into())
                })?,
            )
        }
        _ => None,
    };

    let body = ShelfDetailResponse {
        id: shelf.id,
        name: shelf.name,
        is_system: shelf.is_system,
        created_at: shelf.created_at,
        updated_at: shelf.updated_at,
        items,
        next_cursor,
    };
    let mut headers = HeaderMap::new();
    headers.insert(ETAG, etag_header(body.updated_at)?);
    if let Some(ref nc) = body.next_cursor {
        let next_url = build_next_url(&uri, nc);
        match HeaderValue::from_str(&format!("<{next_url}>; rel=\"next\"")) {
            Ok(value) => {
                headers.insert(LINK, value);
            }
            Err(e) => {
                tracing::warn!(error = %e, %next_url, "could not encode Link rel=next header");
            }
        }
    }
    Ok((headers, axum::Json(body)))
}

/// Body for `POST /api/v1/shelves/{id}/items`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
struct AddItemRequest {
    /// Manifestation to append; must be visible to the caller.
    manifestation_id: Uuid,
}

/// `POST /api/v1/shelves/{id}/items` — append a manifestation at
/// `max(position) + 1`. Bumps the shelf's `updated_at` (`ETag`).
///
/// # Errors
/// - [`AppError::Validation`] when the request body is missing,
///   malformed, or lacks the `manifestation_id` field.
/// - [`AppError::NotFound`] when the shelf is missing / not owned, OR
///   when the manifestation is not visible to the caller (verified
///   via an RLS-scoped existence probe to prevent shelf-existence
///   probing of arbitrary manifestation ids).
/// - [`AppError::Internal`] on database errors.
#[utoipa::path(
    post,
    path = "/api/v1/shelves/{id}/items",
    tag = "shelves",
    params(("id" = Uuid, Path, description = "Shelf id")),
    request_body = AddItemRequest,
    security(("session_cookie" = ["write"]), ("device_token_bearer" = ["write"]), ("opds_basic" = ["write"])),
    responses(
        (status = 204, description = "Item appended at the end (no-op if already on the shelf); shelf ETag bumped",
         headers(("ETag" = String, description = "Entity-tag carrying the shelf's new updated_at (RFC 3339, quoted per RFC 9110)"))),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails),
        (status = 404, description = "Shelf missing / not owned, or manifestation not visible to the caller", body = crate::openapi::ProblemDetails),
        (status = 422, description = "Missing or malformed body", body = crate::openapi::ProblemDetails)
    )
)]
async fn add_shelf_item(
    current_user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    body: Result<axum::Json<AddItemRequest>, axum::extract::rejection::JsonRejection>,
) -> Result<impl IntoResponse, AppError> {
    current_user.require_scope(Scope::Write)?;
    let axum::Json(req) = body.map_err(|e| AppError::Validation(e.body_text()))?;

    // RLS-scoped probe: confirm the caller can see the target
    // manifestation. Without this an attacker could brute-force the
    // manifestation-id space by feeding random UUIDs and watching for
    // a 200 vs a 404.
    {
        let mut rls_tx = crate::db::acquire_with_rls(&state.pool, current_user.user_id)
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
        let visible: Option<Uuid> = sqlx::query_scalar!(
            "SELECT id FROM manifestations WHERE id = $1",
            req.manifestation_id,
        )
        .fetch_optional(&mut *rls_tx)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
        if visible.is_none() {
            return Err(AppError::NotFound);
        }
    }

    let mut tx = state
        .pool
        .begin()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    let owned = sqlx::query_scalar!(
        "SELECT id FROM shelves WHERE id = $1 AND user_id = $2 FOR UPDATE",
        id,
        current_user.user_id,
    )
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    if owned.is_none() {
        return Err(AppError::NotFound);
    }
    let next_position: i32 = sqlx::query_scalar!(
        r#"SELECT COALESCE(MAX(position), -1) + 1 AS "p!"
             FROM shelf_items WHERE shelf_id = $1"#,
        id,
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    sqlx::query!(
        "INSERT INTO shelf_items (shelf_id, manifestation_id, position) \
         VALUES ($1, $2, $3) \
         ON CONFLICT (shelf_id, manifestation_id) DO NOTHING",
        id,
        req.manifestation_id,
        next_position,
    )
    .execute(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    let updated_at: OffsetDateTime = sqlx::query_scalar!(
        r#"UPDATE shelves SET updated_at = now() WHERE id = $1
           RETURNING updated_at AS "ts!""#,
        id,
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    tx.commit()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let mut headers = HeaderMap::new();
    headers.insert(ETAG, etag_header(updated_at)?);
    Ok((StatusCode::NO_CONTENT, headers))
}

/// `DELETE /api/v1/shelves/{id}/items/{manifestation_id}` — remove a
/// shelf item. Bumps the shelf's `updated_at` (`ETag`).
///
/// # Errors
/// - [`AppError::NotFound`] when the shelf is missing / not owned by
///   the caller, OR when the item is not on the shelf (the second
///   case prevents silent `ETag` drift from a no-op delete: bumping
///   `updated_at` on zero `rows_affected` would invalidate a
///   correctly-held client `If-Match` for the next reorder).
/// - [`AppError::Internal`] on database errors.
#[utoipa::path(
    delete,
    path = "/api/v1/shelves/{id}/items/{manifestation_id}",
    tag = "shelves",
    params(
        ("id" = Uuid, Path, description = "Shelf id"),
        ("manifestation_id" = Uuid, Path, description = "Manifestation to remove")
    ),
    security(("session_cookie" = ["write"]), ("device_token_bearer" = ["write"]), ("opds_basic" = ["write"])),
    responses(
        (status = 204, description = "Item removed; shelf ETag bumped",
         headers(("ETag" = String, description = "Entity-tag carrying the shelf's new updated_at (RFC 3339, quoted per RFC 9110)"))),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails),
        (status = 404, description = "Shelf missing / not owned, or item not on the shelf", body = crate::openapi::ProblemDetails)
    )
)]
async fn remove_shelf_item(
    current_user: CurrentUser,
    State(state): State<AppState>,
    Path((shelf_id, manifestation_id)): Path<(Uuid, Uuid)>,
) -> Result<impl IntoResponse, AppError> {
    current_user.require_scope(Scope::Write)?;
    let mut tx = state
        .pool
        .begin()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    let owned = sqlx::query_scalar!(
        "SELECT id FROM shelves WHERE id = $1 AND user_id = $2 FOR UPDATE",
        shelf_id,
        current_user.user_id,
    )
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    if owned.is_none() {
        return Err(AppError::NotFound);
    }
    let deleted = sqlx::query!(
        "DELETE FROM shelf_items WHERE shelf_id = $1 AND manifestation_id = $2",
        shelf_id,
        manifestation_id,
    )
    .execute(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    if deleted.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    let updated_at: OffsetDateTime = sqlx::query_scalar!(
        r#"UPDATE shelves SET updated_at = now() WHERE id = $1
           RETURNING updated_at AS "ts!""#,
        shelf_id,
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    tx.commit()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    let mut headers = HeaderMap::new();
    headers.insert(ETAG, etag_header(updated_at)?);
    Ok((StatusCode::NO_CONTENT, headers))
}

/// Body for `PUT /api/v1/shelves/{id}/items` — full ordered manifestation list.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
struct ReorderItemsRequest {
    /// Complete new ordering: every manifestation currently on the shelf,
    /// exactly once. Partial reorders are rejected.
    items: Vec<Uuid>,
}

/// `PUT /api/v1/shelves/{id}/items` — transactionally rewrite
/// `shelf_items.position` based on the supplied order.
///
/// Optimistic-concurrency: requires `If-Match: "<updated_at>"`
/// header matching the shelf's current `updated_at`; otherwise 412.
///
/// # Errors
/// - [`AppError::IfMatchRequired`] (428) when the header is absent.
/// - [`AppError::Validation`] when the header is malformed or the
///   posted items list contains UUIDs that are not currently on the
///   shelf.
/// - [`AppError::NotFound`] when the shelf is missing / not owned.
/// - [`AppError::IfMatchMismatch`] (412) when the `ETag` does not match.
/// - [`AppError::Internal`] on database errors.
#[utoipa::path(
    put,
    path = "/api/v1/shelves/{id}/items",
    tag = "shelves",
    params(
        ("id" = Uuid, Path, description = "Shelf id"),
        ("If-Match" = String, Header, description = "Strong entity-tag from a prior shelf response; weak validators (W/\"...\") are rejected")
    ),
    request_body = ReorderItemsRequest,
    security(("session_cookie" = ["write"]), ("device_token_bearer" = ["write"]), ("opds_basic" = ["write"])),
    responses(
        (status = 204, description = "Positions rewritten; shelf ETag bumped",
         headers(("ETag" = String, description = "Entity-tag carrying the shelf's new updated_at (RFC 3339, quoted per RFC 9110)"))),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails),
        (status = 404, description = "Shelf missing or owned by another user (existence-not-leaked)", body = crate::openapi::ProblemDetails),
        (status = 412, description = "If-Match does not match the shelf's current updated_at", body = crate::openapi::ProblemDetails),
        (status = 422, description = "Malformed If-Match, or items list does not exactly cover the shelf", body = crate::openapi::ProblemDetails),
        (status = 428, description = "If-Match header absent", body = crate::openapi::ProblemDetails)
    )
)]
async fn reorder_shelf_items(
    current_user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    headers_in: HeaderMap,
    body: Result<axum::Json<ReorderItemsRequest>, axum::extract::rejection::JsonRejection>,
) -> Result<impl IntoResponse, AppError> {
    current_user.require_scope(Scope::Write)?;
    let if_match = parse_if_match(&headers_in)?.ok_or(AppError::IfMatchRequired)?;
    let axum::Json(req) = body.map_err(|e| AppError::Validation(e.body_text()))?;

    let mut tx = state
        .pool
        .begin()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    let row = sqlx::query!(
        r#"SELECT updated_at AS "ts!" FROM shelves
           WHERE id = $1 AND user_id = $2 FOR UPDATE"#,
        id,
        current_user.user_id,
    )
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?
    .ok_or(AppError::NotFound)?;
    // The FOR UPDATE lock serialises concurrent reorder attempts;
    // the second transaction reads the bumped `updated_at` and 412s.
    if row.ts != if_match {
        return Err(AppError::IfMatchMismatch);
    }

    // Refuse partial reorders so the operator either sees the full
    // new order or none of it.
    let current: Vec<Uuid> = sqlx::query_scalar!(
        "SELECT manifestation_id FROM shelf_items WHERE shelf_id = $1",
        id,
    )
    .fetch_all(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    if current.len() != req.items.len() {
        return Err(AppError::Validation(format!(
            "items list size mismatch: shelf has {}, request supplied {}",
            current.len(),
            req.items.len(),
        )));
    }
    let current_set: std::collections::HashSet<Uuid> = current.iter().copied().collect();
    for item in &req.items {
        if !current_set.contains(item) {
            return Err(AppError::Validation(format!(
                "manifestation {item} is not on this shelf"
            )));
        }
    }

    // Bulk-update positions via UNNEST in a single round-trip — N
    // sequential UPDATE statements would make the lock window scale
    // with shelf size.
    let positions: Vec<i32> = (0..req.items.len())
        .map(|i| {
            i32::try_from(i).map_err(|e| AppError::Validation(format!("position overflow: {e}")))
        })
        .collect::<Result<_, _>>()?;
    sqlx::query!(
        "UPDATE shelf_items AS si \
            SET position = updates.pos \
           FROM unnest($1::int4[], $2::uuid[]) AS updates(pos, mid) \
          WHERE si.shelf_id = $3 AND si.manifestation_id = updates.mid",
        &positions,
        &req.items,
        id,
    )
    .execute(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    let updated_at: OffsetDateTime = sqlx::query_scalar!(
        r#"UPDATE shelves SET updated_at = now() WHERE id = $1
           RETURNING updated_at AS "ts!""#,
        id,
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    tx.commit()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let mut headers = HeaderMap::new();
    headers.insert(ETAG, etag_header(updated_at)?);
    Ok((StatusCode::NO_CONTENT, headers))
}
