//! Response DTOs for `/api/v1/shelves*`.
//!
//! Wire-format conventions follow the JSON-API conventions ADR
//! (`adr/2026-05-22-json-api-conventions.md`): snake_case field names,
//! `Option<T>` for nullable, RFC 3339 timestamps via `time`.
//!
//! # `ETag` + If-Match contract
//!
//! Every shelf read endpoint emits ``ETag`: "<updated_at RFC3339>"`.
//! The `PUT /api/v1/shelves/{id}/items` reorder endpoint requires the
//! caller to echo that value as `If-Match`; mutation handlers issue
//! an `UPDATE shelves SET updated_at = now() WHERE id = $1` in the
//! same transaction so item-mutation events also move the `ETag`.
//! Without that bump, a `POST .../items` would not change the `ETag`
//! and the next reorder PUT would 412 spuriously.

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
    // The adapter is load-bearing, not decorative: `time`'s default
    // serde impl emits a 9-element tuple of date and offset components,
    // so dropping it publishes a shape no `date-time` consumer accepts.
    // Applies to every timestamp in this module.
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
/// Carries the position so the frontend can render in stored order
/// and round-trip back as `PUT /api/v1/shelves/{id}/items` with the
/// re-arranged list.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
#[non_exhaustive]
pub struct ShelfItem {
    /// `shelf_items.manifestation_id`.
    pub manifestation_id: Uuid,
    /// `shelf_items.position`. Caller echoes the ordering when sending
    /// the reorder PUT.
    pub position: i32,
    /// `shelf_items.added_at`.
    pub added_at: DateTime<Utc>,
}

// `GET /api/v1/shelves/{id}`'s envelope (`ShelfDetailResponse`) lives in
// `routes::shelves` — pagination is a wire concern, so the paged response
// shape stays route-local like the books `BookListResponse`.
