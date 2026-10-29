//! Response DTOs for `/api/v1/series/{id}`.
//!
//! Wire-format conventions follow the JSON-API conventions ADR
//! (`docs/adr/0011-json-api-conventions-for-the-browser-facing-rest-surface.md`): snake_case field names,
//! `Option<T>` for nullable, RFC 3339 timestamps.
//!
//! # RLS posture
//!
//! Catalogue tables have no RLS policies. The handler derives child work
//! membership from visible manifestations and returns 404 when none are visible.

use serde::Serialize;
use uuid::Uuid;

use crate::models::library::WorkManifestation;

/// `/api/v1/series/{id}` response.
///
/// Carries series identity and recorded work metadata in series order.
/// Manifestations are scoped by RLS; child entries require a visible manifestation.
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

/// Recorded work metadata with the manifestations visible to the caller.
/// Adult callers can receive an empty manifestation list.
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
    /// Manifestations of this work visible to the caller under RLS.
    pub manifestations: Vec<WorkManifestation>,
}
