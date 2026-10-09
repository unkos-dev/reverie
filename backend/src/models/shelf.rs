//! Response DTOs for `/api/v1/shelves*`.
//!
//! Wire-format conventions follow the JSON-API conventions ADR
//! (`docs/adr/0011-json-api-conventions-for-the-browser-facing-rest-surface.md`): snake_case field names,
//! `Option<T>` for nullable, RFC 3339 timestamps.
//!
//! # `ETag`
//!
//! Every shelf read endpoint emits `ETag: "<updated_at RFC3339>"`.
//! Item mutations issue an `UPDATE shelves SET updated_at = now()` in
//! the same transaction so the tag moves with membership changes.

use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

/// One shelf in the shelves-list response, plus the read shape for
/// create/rename round-trips.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
#[non_exhaustive]
pub struct Shelf {
    /// `shelves.id`.
    pub id: Uuid,
    /// `shelves.name` (display).
    pub name: String,
    /// `shelves.is_system` — `true` for backend-managed shelves whose
    /// row cannot be renamed or deleted (409 `system-shelf-immutable`
    /// on the mutating handlers).
    pub is_system: bool,
    /// `shelves.created_at`.
    pub created_at: DateTime<Utc>,
    /// `shelves.updated_at`. Doubles as the `ETag` value the client
    /// echoes on `If-Match` for the reorder endpoint.
    pub updated_at: DateTime<Utc>,
    /// Count of `shelf_items` rows on this shelf. Computed inline via
    /// a correlated scalar subquery on the list endpoint and surfaces
    /// in the sidebar without a follow-up request per shelf.
    pub item_count: i64,
}

/// One row of the shelf-items list (`GET /api/v1/shelves/{id}`).
///
/// Items arrive ordered by `added_at`, then `manifestation_id`.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
#[non_exhaustive]
pub struct ShelfItem {
    /// `shelf_items.manifestation_id`.
    pub manifestation_id: Uuid,
    /// `shelf_items.added_at`.
    pub added_at: DateTime<Utc>,
}

// `GET /api/v1/shelves/{id}`'s envelope (`ShelfDetailResponse`) lives in
// `routes::shelves` — pagination is a wire concern, so the paged response
// shape stays route-local like the books `BookListResponse`.
