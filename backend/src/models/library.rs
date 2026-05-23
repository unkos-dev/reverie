//! Response DTOs for `/api/books`, `/api/books/{id}`, `/api/works/{id}`.
//!
//! Wire-format conventions follow the JSON-API conventions ADR
//! (`adr/2026-05-22-json-api-conventions.md`): snake_case field names
//! (no `serde(rename_all)`), `Option<T>` for nullable fields (no
//! `skip_serializing_if`), RFC 3339 timestamps via the `time` crate
//! default. Mirrors [`crate::models::user::User`] shape.
//!
//! [`crate::models::library::BookListRow`] doubles as the `sqlx::FromRow` decode target and
//! the API response item. The `created_at` field is decoded for use
//! as a cursor key but is `#[serde(skip)]`-elided from the wire to
//! keep the response payload aligned with the frontend
//! `BookListItem` interface in `frontend/src/api/books.ts`.
//!
//! `authors` is loaded via a separate batch query (`ANY($1::uuid[])`)
//! after the page rows arrive — the join cannot be expressed in a
//! single `sqlx::query!` macro without producing one row per
//! `(manifestation, author)` pair.

use serde::Serialize;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::models::enrichment_status::EnrichmentStatus;
use crate::models::ingestion_status::IngestionStatus;

/// Series membership for a manifestation. Embedded into both
/// [`BookListRow`] and [`BookDetail`]; `None` when the work isn't on
/// any series.
#[derive(Debug, Clone, Serialize)]
#[non_exhaustive]
pub struct SeriesRef {
    /// Series primary key.
    pub id: Uuid,
    /// Series display name.
    pub name: String,
    /// Position within the series (`series_works.position`), `None`
    /// when the membership row has a null position.
    pub position: Option<f64>,
}

/// One row of a paginated book list. Decoded via [`sqlx::FromRow`]
/// against the query in `routes/library::list`, then enriched with
/// the batch-loaded `authors` slot and serialised straight to JSON.
#[derive(Debug, Clone, Serialize)]
#[non_exhaustive]
pub struct BookListRow {
    /// `manifestations.id` — the canonical book id on the wire.
    pub id: Uuid,
    /// `works.id` of the parent work. Lets the client navigate to
    /// `/api/works/{work_id}` from a list row without an extra fetch.
    pub work_id: Uuid,
    /// `works.title` of the parent work.
    pub title: String,
    /// Author display names ordered by `work_authors.position`. Empty
    /// when the work has no authors yet (pre-enrichment stub).
    pub authors: Vec<String>,
    /// Series membership; `None` when the work isn't on a series.
    pub series: Option<SeriesRef>,
    /// `manifestations.isbn_13`, when known.
    pub isbn_13: Option<String>,
    /// Pre-signed thumbnail URL — backend constructs it so the
    /// frontend has a single source of truth for the cover surface.
    pub cover_url: String,
    /// Ingestion lifecycle state.
    pub ingestion_status: IngestionStatus,
    /// Validation lifecycle state. Typed enum deferred — DB has
    /// `pending|valid|repaired|degraded`, frontend uses
    /// `clean|repaired|degraded|quarantined`. Reconciliation lives
    /// in a follow-up; surface the raw DB string for now.
    pub validation_status: String,
    /// Enrichment lifecycle state.
    pub enrichment_status: EnrichmentStatus,
    /// `manifestations.created_at`; used as the recent-sort cursor
    /// key and elided from the JSON wire shape.
    #[serde(skip)]
    pub created_at: OffsetDateTime,
}

/// `/api/books/{id}` response. Carries the [`BookListRow`] fields
/// plus the work-level prose and metadata-version summary surfaced
/// in the book-detail UI.
#[derive(Debug, Clone, Serialize)]
#[non_exhaustive]
#[allow(
    dead_code,
    reason = "consumed by the GET /api/books/{id} handler in the 11a-A.4 slice"
)]
pub struct BookDetail {
    /// `manifestations.id`.
    pub id: Uuid,
    /// `works.id`.
    pub work_id: Uuid,
    /// `works.title`.
    pub title: String,
    /// Author display names ordered by `work_authors.position`.
    pub authors: Vec<String>,
    /// Series membership; `None` when the work isn't on a series.
    pub series: Option<SeriesRef>,
    /// Long-form description (`works.description`).
    pub description: Option<String>,
    /// BCP-47 language tag (`works.language`).
    pub language: Option<String>,
    /// `manifestations.isbn_13`.
    pub isbn_13: Option<String>,
    /// `manifestations.isbn_10`.
    pub isbn_10: Option<String>,
    /// Pre-signed cover URL.
    pub cover_url: String,
    /// Tag names attached to the manifestation.
    pub tags: Vec<String>,
    /// Ingestion lifecycle state.
    pub ingestion_status: IngestionStatus,
    /// Validation lifecycle state (raw DB string — see
    /// [`BookListRow::validation_status`]).
    pub validation_status: String,
    /// Enrichment lifecycle state.
    pub enrichment_status: EnrichmentStatus,
    /// Metadata-version counts for the Versions tab.
    pub metadata_version_summary: MetadataVersionSummary,
    /// `manifestations.created_at`.
    pub created_at: OffsetDateTime,
    /// `manifestations.updated_at`.
    pub updated_at: OffsetDateTime,
}

/// Counts surfaced on the book-detail Versions tab.
#[derive(Debug, Clone, Serialize)]
#[non_exhaustive]
#[allow(
    dead_code,
    reason = "consumed alongside BookDetail in the 11a-A.4 slice"
)]
pub struct MetadataVersionSummary {
    /// Number of pending (non-accepted, non-rejected) versions.
    pub pending: u32,
    /// Number of versions accepted into the canonical pointer set.
    pub accepted: u32,
}

/// `/api/works/{id}` response. Lists every manifestation the user
/// can see for a given work, grouped under the work-level prose.
#[derive(Debug, Clone, Serialize)]
#[non_exhaustive]
#[allow(
    dead_code,
    reason = "consumed by the GET /api/works/{id} handler in the 11a-A.4 slice"
)]
pub struct WorkDetail {
    /// `works.id`.
    pub id: Uuid,
    /// `works.title`.
    pub title: String,
    /// Author display names ordered by `work_authors.position`.
    pub authors: Vec<String>,
    /// Long-form description (`works.description`).
    pub description: Option<String>,
    /// BCP-47 language tag (`works.language`).
    pub language: Option<String>,
    /// Series membership; `None` when the work isn't on a series.
    pub series: Option<SeriesRef>,
    /// Manifestations of this work visible to the requesting user.
    pub manifestations: Vec<WorkManifestation>,
}

/// One manifestation row embedded in a [`WorkDetail`] response.
#[derive(Debug, Clone, Serialize)]
#[non_exhaustive]
#[allow(
    dead_code,
    reason = "consumed alongside WorkDetail in the 11a-A.4 slice"
)]
pub struct WorkManifestation {
    /// `manifestations.id`.
    pub id: Uuid,
    /// `manifestations.isbn_13`.
    pub isbn_13: Option<String>,
    /// `manifestations.isbn_10`.
    pub isbn_10: Option<String>,
    /// Pre-signed cover URL.
    pub cover_url: String,
    /// Ingestion lifecycle state.
    pub ingestion_status: IngestionStatus,
    /// Validation lifecycle state (raw DB string — see
    /// [`BookListRow::validation_status`]).
    pub validation_status: String,
    /// Enrichment lifecycle state.
    pub enrichment_status: EnrichmentStatus,
    /// `manifestations.created_at`.
    pub created_at: OffsetDateTime,
}
