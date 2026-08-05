//! Response DTOs for `/api/v1/series/{id}`.
//!
//! Wire-format conventions follow the JSON-API conventions ADR
//! (`adr/2026-05-22-json-api-conventions.md`): snake_case field names,
//! `Option<T>` for nullable, RFC 3339 timestamps.
//!
//! # RLS posture
//!
//! `series` and `series_works` carry no RLS policies (catalog data,
//! not user-scoped). RLS lives on `manifestations`, so per-work
//! `manifestations` slots come back empty for works the caller has no
//! visibility into. The route handler treats "every work in the
//! series has zero visible manifestations" as 404 to keep the
//! existence-not-leaked invariant intact for child accounts and
//! cross-user adult isolation.

use serde::Serialize;
use uuid::Uuid;

use crate::models::library::WorkManifestation;

/// `/api/v1/series/{id}` response. Carries the series identity plus an
/// ordered list of works; each work surfaces only the manifestations
/// the caller can see, so the frontend can render a completeness
/// indicator (`own = works with len(manifestations) > 0`, `total =
/// works.len()`).
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
#[non_exhaustive]
pub struct SeriesDetail {
    /// `series.id`.
    pub id: Uuid,
    /// `series.name` (display).
    pub name: String,
    /// `series.sort_name` (collation key — surfaced so the UI can
    /// alpha-sort a list of series without a follow-up request).
    pub sort_name: String,
    /// Works in the series, ordered by `series_works.position` (NULLS
    /// LAST) then `works.id` for tiebreaking determinism.
    pub works: Vec<SeriesWork>,
}

/// One work within a series. Includes every manifestation of the work
/// the caller can see; an empty `manifestations` list means the user
/// does not yet own a copy ("gap" in the series).
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
#[non_exhaustive]
pub struct SeriesWork {
    /// `works.id`.
    pub id: Uuid,
    /// `works.title`.
    pub title: String,
    /// Author display names ordered by `work_authors.position`.
    pub authors: Vec<String>,
    /// `series_works.position`; `None` when the membership row has a
    /// null position.
    pub position: Option<f64>,
    /// Manifestations of this work visible to the caller. Empty when
    /// the user owns no copy.
    pub manifestations: Vec<WorkManifestation>,
}
