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
    /// Cover thumbnail URL — relative path served by the
    /// `/api/books/{id}/cover/thumb` handler under the caller's
    /// session. Not pre-signed; access is gated by the session cookie.
    /// Backend constructs it so the frontend has a single source of
    /// truth for the cover surface.
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
    /// Cover thumbnail URL — relative path under
    /// `/api/books/{id}/cover/thumb`, session-cookie gated. See
    /// [`BookListRow::cover_url`].
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
pub struct MetadataVersionSummary {
    /// Number of `metadata_versions` rows with `status = 'pending'`
    /// for this manifestation — i.e. unresolved drafts in the journal.
    pub pending: u32,
    /// Number of fields whose canonical pointer is currently set on
    /// this manifestation or its work — one entry per non-NULL
    /// `*_version_id` slot. Each slot binds a single
    /// `metadata_versions` row, so this also counts distinct
    /// "accepted" versions in play.
    pub accepted: u32,
}

/// `/api/works/{id}` response. Lists every manifestation the user
/// can see for a given work, grouped under the work-level prose.
#[derive(Debug, Clone, Serialize)]
#[non_exhaustive]
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

/// `GET /api/search` envelope (11b). Wraps a flat result list — the
/// frontend groups by [`SearchHit::kind`] client-side. No cursor:
/// search is bounded by `LIMIT` server-side; pagination is a follow-up
/// if user-research warrants it.
#[derive(Debug, Clone, Serialize)]
#[non_exhaustive]
pub struct SearchResponse {
    /// Result rows, ranked DESC by hybrid `ts_rank_cd + similarity`.
    pub items: Vec<SearchHit>,
}

/// One result row of `GET /api/search`. Carries the bare minimum the
/// command-palette UI needs: a kind tag for grouping, identifiers for
/// navigation, a short display label, and an optional `ts_headline`
/// snippet with non-HTML markers (`‹›`) so the React renderer can
/// avoid `dangerouslySetInnerHTML`.
#[derive(Debug, Clone, Serialize)]
#[non_exhaustive]
pub struct SearchHit {
    /// Result kind. Currently always `"book"`; `"author"` and
    /// `"series"` ship in a follow-up that fans the hybrid CTE over
    /// the existing `authors.name` / `series.name` trigram indexes.
    pub kind: SearchHitKind,
    /// Primary id — `manifestations.id` for `book`, `authors.id` for
    /// `author`, `series.id` for `series`.
    pub id: Uuid,
    /// Parent work id when `kind = "book"`, else `None`.
    pub work_id: Option<Uuid>,
    /// Display label — work title for `book`.
    pub title: String,
    /// Author display names for `book` results.
    pub authors: Vec<String>,
    /// `ts_headline` snippet from the work's title+description with
    /// `‹›` start/stop markers around matched terms. `None` when the
    /// hit was trigram-only (no tsquery match → headline would be the
    /// raw text without highlighting).
    pub snippet: Option<String>,
    /// Cover thumbnail URL for `book` results — relative path under
    /// `/api/books/{id}/cover/thumb`, session-cookie gated. `None`
    /// when the hit kind has no cover surface (future author/series
    /// variants).
    pub cover_url: Option<String>,
}

/// Tag identifying which entity a [`SearchHit`] points at. Serialised
/// in `snake_case` to match the rest of the JSON API conventions.
#[derive(Debug, Clone, Copy, Serialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum SearchHitKind {
    /// Manifestation hit — id is the `manifestations.id`.
    Book,
}

/// One manifestation row embedded in a [`WorkDetail`] response.
#[derive(Debug, Clone, Serialize)]
#[non_exhaustive]
pub struct WorkManifestation {
    /// `manifestations.id`.
    pub id: Uuid,
    /// `manifestations.isbn_13`.
    pub isbn_13: Option<String>,
    /// `manifestations.isbn_10`.
    pub isbn_10: Option<String>,
    /// Cover thumbnail URL — relative path under
    /// `/api/books/{id}/cover/thumb`, session-cookie gated. See
    /// [`BookListRow::cover_url`].
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
