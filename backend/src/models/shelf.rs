//! Response DTOs for `/api/shelves*` (sub-phase 11d).
//!
//! Wire-format conventions follow the JSON-API conventions ADR
//! (`adr/2026-05-22-json-api-conventions.md`): snake_case field names,
//! `Option<T>` for nullable, RFC 3339 timestamps via `time`.
//!
//! # `ETag` + If-Match contract
//!
//! Every shelf read endpoint emits ``ETag`: "<updated_at RFC3339>"`.
//! The `PUT /api/shelves/{id}/items` reorder endpoint requires the
//! caller to echo that value as `If-Match`; mutation handlers issue
//! an `UPDATE shelves SET updated_at = now() WHERE id = $1` in the
//! same transaction so item-mutation events also move the `ETag`.
//! Without that bump, a `POST .../items` would not change the `ETag`
//! and the next reorder PUT would 412 spuriously.

use serde::Serialize;
use time::OffsetDateTime;
use uuid::Uuid;

/// One shelf in the shelves-list response, plus the read shape for
/// create/rename round-trips.
#[derive(Debug, Clone, Serialize)]
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
    pub created_at: OffsetDateTime,
    /// `shelves.updated_at`. Doubles as the `ETag` value the client
    /// echoes on `If-Match` for the reorder endpoint.
    pub updated_at: OffsetDateTime,
    /// Count of `shelf_items` rows on this shelf. Cheap to compute via
    /// a `LEFT JOIN LATERAL (SELECT COUNT(*) …)` and surfaces in the
    /// sidebar without a follow-up request per shelf.
    pub item_count: i64,
}

/// One row of the shelf-items list (`GET /api/shelves/{id}`).
///
/// Carries the position so the frontend can render in stored order
/// and round-trip back as `PUT /api/shelves/{id}/items` with the
/// re-arranged list.
#[derive(Debug, Clone, Serialize)]
#[non_exhaustive]
pub struct ShelfItem {
    /// `shelf_items.manifestation_id`.
    pub manifestation_id: Uuid,
    /// `shelf_items.position`. Caller echoes the ordering when sending
    /// the reorder PUT.
    pub position: i32,
    /// `shelf_items.added_at`.
    pub added_at: OffsetDateTime,
}

/// `GET /api/shelves/{id}` response: shelf identity + ordered items.
#[derive(Debug, Clone, Serialize)]
#[non_exhaustive]
pub struct ShelfWithItems {
    /// `shelves.id`.
    pub id: Uuid,
    /// `shelves.name`.
    pub name: String,
    /// `shelves.is_system`.
    pub is_system: bool,
    /// `shelves.created_at`.
    pub created_at: OffsetDateTime,
    /// `shelves.updated_at` — the `ETag` value.
    pub updated_at: OffsetDateTime,
    /// Items ordered by `shelf_items.position` ASC, `added_at` ASC
    /// for tiebreaking determinism.
    pub items: Vec<ShelfItem>,
}
