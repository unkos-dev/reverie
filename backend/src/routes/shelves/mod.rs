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

use axum::extract::{OriginalUri, State};
use axum::http::header::LINK;
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::IntoResponse;
use axum_extra::extract::{Query, QueryRejection};
use serde::{Deserialize, Serialize};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::auth::middleware::CurrentUser;
use crate::error::AppError;
use crate::extract::{ApiJson, ApiPath};
use crate::models::shelf::Shelf;
use crate::routes::library::build_next_url;
use crate::services::reader::ShelfDetailResponse;
use crate::state::AppState;

#[cfg(test)]
mod tests;

/// Build the `/api/v1/shelves*` router as an [`OpenApiRouter`] so each
/// handler's `#[utoipa::path]` contributes to the generated spec (a missing
/// annotation fails to compile).
///
/// Merged into `crate::openapi::pilot_router`
/// and split into its runtime and spec halves there.
pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_shelves, create_shelf))
        .routes(routes!(rename_shelf, delete_shelf, get_shelf_with_items))
        .routes(routes!(add_shelf_item))
        .routes(routes!(remove_shelf_item))
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
    summary = "List shelves",
    description = "Returns one page of the caller's own shelves, ordered with system shelves first then by name.",
    tag = "shelves",
    params(ShelfListParams),
    security(("session_cookie" = ["read"]), ("device_token_bearer" = ["read"]), ("opds_basic" = ["read"])),
    responses(
        (status = 200, description = "One page of the caller's shelves, system shelves first then by name", body = ShelfListResponse,
            headers(("Link" = String, description = "RFC 8288 next-page link; emitted with rel=\"next\" when more rows remain"))),
        (status = 400, description = "Malformed query parameter", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 422, description = "Malformed cursor", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
    )
)]
async fn list_shelves(
    current_user: CurrentUser,
    State(state): State<AppState>,
    params: Result<Query<ShelfListParams>, QueryRejection>,
    OriginalUri(uri): OriginalUri,
) -> Result<impl IntoResponse, AppError> {
    let Query(params) = params?;
    let (items, next_cursor, _) = crate::services::reader::shelves(
        &current_user,
        &state,
        params.cursor.as_deref(),
        i64::from(state.config.opds.page_size),
    )
    .await?;

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
    summary = "Create a shelf",
    description = "Creates a new shelf for the caller. Available to adult accounts only.",
    tag = "shelves",
    request_body = CreateShelfRequest,
    security(("session_cookie" = ["write"]), ("device_token_bearer" = ["write"]), ("opds_basic" = ["write"])),
    responses(
        (status = 201, description = "Shelf created", body = Shelf),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 403, description = "Caller is a child account", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 422, description = "Empty name", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
    )
)]
async fn create_shelf(
    current_user: CurrentUser,
    State(state): State<AppState>,
    ApiJson(req): ApiJson<CreateShelfRequest>,
) -> Result<impl IntoResponse, AppError> {
    let shelf = crate::services::shelves::create_shelf(&current_user, &state, &req.name).await?;
    Ok((StatusCode::CREATED, axum::Json(shelf)))
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
    summary = "Rename a shelf",
    description = "Renames a shelf owned by the caller. Available to adult accounts only. System shelves cannot be renamed and return 409.",
    tag = "shelves",
    params(("id" = Uuid, Path, max_length = 45, description = "Shelf id")),
    request_body = RenameShelfRequest,
    security(("session_cookie" = ["write"]), ("device_token_bearer" = ["write"]), ("opds_basic" = ["write"])),
    responses(
        (status = 200, description = "Renamed shelf", body = Shelf),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 403, description = "Caller is a child account", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 404, description = "Shelf missing or owned by another user (existence-not-leaked)", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 409, description = "System shelves cannot be renamed", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 422, description = "Empty name", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
    )
)]
async fn rename_shelf(
    current_user: CurrentUser,
    State(state): State<AppState>,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(req): ApiJson<RenameShelfRequest>,
) -> Result<impl IntoResponse, AppError> {
    let shelf =
        crate::services::shelves::rename_shelf(&current_user, &state, id, &req.name).await?;
    Ok(axum::Json(shelf))
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
    summary = "Delete a shelf",
    description = "Deletes a shelf owned by the caller. Available to adult accounts only. System shelves cannot be deleted and return 409.",
    tag = "shelves",
    params(("id" = Uuid, Path, max_length = 45, description = "Shelf id")),
    security(("session_cookie" = ["write"]), ("device_token_bearer" = ["write"]), ("opds_basic" = ["write"])),
    responses(
        (status = 204, description = "Shelf deleted"),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 403, description = "Caller is a child account", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 404, description = "Shelf missing or owned by another user (existence-not-leaked)", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 409, description = "System shelves cannot be deleted", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
    )
)]
async fn delete_shelf(
    current_user: CurrentUser,
    State(state): State<AppState>,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    crate::services::shelves::delete_shelf(&current_user, &state, id).await?;
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

/// Shelf identity plus one keyset-paginated page of its items.
///
/// Items page over `(added_at, manifestation_id)`, so every list is bounded by
/// construction.
///
/// # Errors
/// - [`AppError::NotFound`] when missing or not owned by caller.
/// - [`AppError::Validation`] when the cursor is malformed.
/// - [`AppError::Internal`] on database errors.
#[utoipa::path(
    get,
    path = "/api/v1/shelves/{id}",
    summary = "Get a shelf with its items",
    description = "Returns a shelf's identity plus one page of its items ordered by when they were added. Restricted to shelves the caller owns.",
    tag = "shelves",
    params(("id" = Uuid, Path, max_length = 45, description = "Shelf id"), ShelfItemsParams),
    security(("session_cookie" = ["read"]), ("device_token_bearer" = ["read"]), ("opds_basic" = ["read"])),
    responses(
        (status = 200, description = "Shelf identity plus one page of ordered items", body = ShelfDetailResponse,
         headers(
            ("Link" = String, description = "RFC 8288 next-page link; emitted with rel=\"next\" when more items remain")
         )),
        (status = 400, description = "Malformed query or path parameter", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 404, description = "Shelf missing or owned by another user (existence-not-leaked)", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 422, description = "Malformed cursor", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
    )
)]
async fn get_shelf_with_items(
    current_user: CurrentUser,
    State(state): State<AppState>,
    ApiPath(id): ApiPath<Uuid>,
    params: Result<Query<ShelfItemsParams>, QueryRejection>,
    OriginalUri(uri): OriginalUri,
) -> Result<impl IntoResponse, AppError> {
    let Query(params) = params?;
    let body = crate::services::reader::shelf_items(
        &current_user,
        &state,
        id,
        params.cursor.as_deref(),
        i64::from(state.config.opds.page_size),
    )
    .await?;
    let mut headers = HeaderMap::new();
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
    /// Manifestation to add; must be visible to the caller.
    #[schema(max_length = 45)]
    manifestation_id: Uuid,
}

/// `POST /api/v1/shelves/{id}/items` — add a manifestation to the shelf.
/// Bumps the shelf's `updated_at`.
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
    summary = "Add an item to a shelf",
    description = "Adds a manifestation to the caller's shelf; adding one already on the shelf is a no-op. Returns 404 if the manifestation is not visible to the caller.",
    tag = "shelves",
    params(("id" = Uuid, Path, max_length = 45, description = "Shelf id")),
    request_body = AddItemRequest,
    security(("session_cookie" = ["write"]), ("device_token_bearer" = ["write"]), ("opds_basic" = ["write"])),
    responses(
        (status = 204, description = "Item added (no-op if already on the shelf)"),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 404, description = "Shelf missing / not owned, or manifestation not visible to the caller", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
    )
)]
async fn add_shelf_item(
    current_user: CurrentUser,
    State(state): State<AppState>,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(req): ApiJson<AddItemRequest>,
) -> Result<impl IntoResponse, AppError> {
    crate::services::shelves::add_shelf_item(&current_user, &state, id, req.manifestation_id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `DELETE /api/v1/shelves/{id}/items/{manifestation_id}` — remove a
/// shelf item. Bumps the shelf's `updated_at`.
///
/// # Errors
/// - [`AppError::NotFound`] when the shelf is missing / not owned by
///   the caller, OR when the item is not on the shelf (the second
///   case prevents `updated_at` drifting on a no-op delete: bumping it
///   on zero `rows_affected` would move it without a membership
///   change).
/// - [`AppError::Internal`] on database errors.
#[utoipa::path(
    delete,
    path = "/api/v1/shelves/{id}/items/{manifestation_id}",
    summary = "Remove an item from a shelf",
    description = "Removes a manifestation from the caller's shelf. Returns 404 if the manifestation is not currently on the shelf.",
    tag = "shelves",
    params(
        ("id" = Uuid, Path, max_length = 45, description = "Shelf id"),
        ("manifestation_id" = Uuid, Path, max_length = 45, description = "Manifestation to remove")
    ),
    security(("session_cookie" = ["write"]), ("device_token_bearer" = ["write"]), ("opds_basic" = ["write"])),
    responses(
        (status = 204, description = "Item removed"),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 404, description = "Shelf missing / not owned, or item not on the shelf", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
    )
)]
async fn remove_shelf_item(
    current_user: CurrentUser,
    State(state): State<AppState>,
    ApiPath((shelf_id, manifestation_id)): ApiPath<(Uuid, Uuid)>,
) -> Result<impl IntoResponse, AppError> {
    crate::services::shelves::remove_shelf_item(&current_user, &state, shelf_id, manifestation_id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
