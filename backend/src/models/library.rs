//! Response DTOs for `/api/v1/books`, `/api/v1/books/{id}`, `/api/v1/works/{id}`.
//!
//! Wire-format conventions follow the JSON-API conventions ADR
//! (`docs/adr/0011-json-api-conventions-for-the-browser-facing-rest-surface.md`): snake_case field names
//! (no `serde(rename_all)`), `Option<T>` for nullable fields (no
//! `skip_serializing_if`), RFC 3339 timestamps via the `time` crate
//! default. Mirrors [`crate::models::user::User`] shape.
//!
//! [`crate::models::library::BookListRow`] is assembled by hand from the
//! dynamic `QueryBuilder` row in `routes/library::list` (it has no
//! `sqlx::FromRow` derive) and doubles as the API response item. The
//! `created_at` field is the recent-sort cursor key and, as RFC 3339 on
//! the wire, the value behind the "Added" sort column in the frontend
//! `BookListItem` interface in `frontend/src/api/books.ts`.
//!
//! The multi-value slots (`authors`, `contributors`, `tags`, `genres`,
//! `moods`, and the caller's `reading_state`) load via separate batch
//! queries (`ANY($1::uuid[])`) after the page rows arrive: joining any
//! of them into the paginated base query would emit one row per pair
//! and break `LIMIT` and the cursor math.

use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use uuid::Uuid;

use crate::models::content_rating::ContentRating;
use crate::models::contributor_role::ContributorRole;
use crate::models::enrichment_status::EnrichmentStatus;
use crate::models::external_identifier::IdentifierLevel;
use crate::models::ingestion_status::IngestionStatus;
use crate::models::reading_state::ReadingStateSummary;
use crate::models::validation_status::ValidationStatus;

/// Series membership for a manifestation. Embedded into both
/// [`BookListRow`] and [`BookDetail`]; `None` when the work isn't on
/// any series.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
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

/// One non-author contributor surfaced on a book projection.
///
/// Authors
/// stay in the flat `authors` display array; this slot carries the other
/// `work_authors` roles so per-role grid columns need no extra fetch.
/// Ordered by role (enum declaration order), then contributor position.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
#[non_exhaustive]
pub struct ContributorRef {
    /// Contributor display name (`authors.name`).
    pub name: String,
    /// Contribution role (`work_authors.role`); never
    /// [`ContributorRole::Author`] on this slot.
    pub role: ContributorRole,
}

/// One external-source identifier surfaced on a book projection.
/// Registry rows for providers hidden via the `provider_visibility`
/// setting are filtered out before this DTO is built.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
#[non_exhaustive]
pub struct ExternalIdRef {
    /// FRBR level the id attaches to: `work` ids are shared across
    /// editions, `manifestation` ids name this edition specifically.
    pub level: IdentifierLevel,
    /// Identifier scheme (`identifier_schemes.id`, e.g. `openlibrary`).
    pub scheme: String,
    /// The identifier value on that scheme.
    pub external_id: String,
}

/// One provider's aggregate rating surfaced on a book projection.
///
/// Ratings are per-edition and per-source; each provider is
/// authoritative for its own scale, so no cross-source aggregate is
/// computed. Hidden providers are filtered out before this DTO is built.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
#[non_exhaustive]
pub struct ExternalRatingRef {
    /// Rating provider (`rating_sources.id`, e.g. `googlebooks`).
    pub source: String,
    /// The provider's score on its own scale.
    pub rating: f32,
    /// The provider's maximum score (e.g. 5).
    pub rating_scale: f32,
    /// Number of reviews backing the score; 0 when unreported.
    pub review_count: i32,
    /// When the enrichment pipeline last refreshed this value.
    pub fetched_at: DateTime<Utc>,
}

/// One row of a paginated book list.
///
/// Assembled by hand from the
/// dynamic `QueryBuilder` row in `routes/library::list`, then enriched
/// with the batch-loaded multi-value slots and serialised straight to
/// JSON.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
#[non_exhaustive]
pub struct BookListRow {
    /// `manifestations.id` — the canonical book id on the wire.
    pub id: Uuid,
    /// `works.id` of the parent work. Lets the client navigate to
    /// `/api/v1/works/{work_id}` from a list row without an extra fetch.
    pub work_id: Uuid,
    /// `works.title` of the parent work.
    pub title: String,
    /// `works.subtitle`, when declared.
    pub subtitle: Option<String>,
    /// Author display names ordered by `work_authors.position`. Empty
    /// when the work has no authors yet (pre-enrichment stub), and never
    /// includes editors/translators/narrators — no role substitutes for
    /// another in display.
    pub authors: Vec<String>,
    /// Non-author contributors (editor/translator/narrator) of the parent
    /// work, ordered by role then position. Batch-loaded alongside
    /// `authors`; empty when the work has none.
    pub contributors: Vec<ContributorRef>,
    /// Series membership; `None` when the work isn't on a series.
    pub series: Option<SeriesRef>,
    /// `manifestations.isbn_13`, when known.
    pub isbn_13: Option<String>,
    /// `manifestations.pages`, when known.
    pub pages: Option<i32>,
    /// Tag names attached to the manifestation, sorted by name.
    /// Batch-loaded per page.
    pub tags: Vec<String>,
    /// Genre names attached to the manifestation, sorted by name.
    /// Batch-loaded per page.
    pub genres: Vec<String>,
    /// Mood names attached to the manifestation, sorted by name.
    /// Batch-loaded per page.
    pub moods: Vec<String>,
    /// Audience-suitability rating (`manifestations.content_rating`);
    /// `None` when unrated.
    pub content_rating: Option<ContentRating>,
    /// Cover thumbnail URL — relative path served by the
    /// `/api/v1/books/{id}/cover/thumb` handler under the caller's
    /// session. Not pre-signed; access is gated by the session cookie.
    /// Backend constructs it so the frontend has a single source of
    /// truth for the cover surface.
    pub cover_url: String,
    /// Ingestion lifecycle state.
    pub ingestion_status: IngestionStatus,
    /// Validation lifecycle state.
    pub validation_status: ValidationStatus,
    /// Enrichment lifecycle state.
    pub enrichment_status: EnrichmentStatus,
    /// Caller's reading state for this book; `None` when unread (no
    /// `reading_state` row). Batch-loaded alongside `authors`.
    pub reading_state: Option<ReadingStateSummary>,
    /// External-source identifiers for visible providers: the parent
    /// work's ids followed by this edition's, each sorted by scheme.
    /// Batch-loaded per page; providers hidden via `provider_visibility`
    /// are absent.
    pub external_ids: Vec<ExternalIdRef>,
    /// Per-provider aggregate ratings for visible providers, sorted by
    /// source. Batch-loaded per page.
    pub external_ratings: Vec<ExternalRatingRef>,
    /// `manifestations.created_at`. RFC 3339 on the wire; also the
    /// recent-sort cursor key and the value behind the "Added" sort column.
    pub created_at: DateTime<Utc>,
}

/// `/api/v1/books/{id}` response. Carries the [`BookListRow`] fields
/// plus the work-level prose and metadata-version summary surfaced
/// in the book-detail UI.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
#[non_exhaustive]
pub struct BookDetail {
    /// `manifestations.id`.
    pub id: Uuid,
    /// `works.id`.
    pub work_id: Uuid,
    /// `works.title`.
    pub title: String,
    /// `works.subtitle`, when declared.
    pub subtitle: Option<String>,
    /// Author display names ordered by `work_authors.position`.
    pub authors: Vec<String>,
    /// Non-author contributors (editor/translator/narrator) of the parent
    /// work, ordered by role then position; empty when the work has none.
    pub contributors: Vec<ContributorRef>,
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
    /// `manifestations.pages`, when known.
    pub pages: Option<i32>,
    /// `manifestations.publisher` — canonical publisher string. Surfaced
    /// on `BookDetail` so the manual-edit dialog can confirm clears
    /// (`EditMetadataDialog.canonicalEditableFields`).
    pub publisher: Option<String>,
    /// `manifestations.pub_date` formatted as an ISO 8601 calendar date.
    /// Surfaced for the manual-edit dialog's clear-confirmation flow.
    pub pub_date: Option<String>,
    /// Cover thumbnail URL — relative path under
    /// `/api/v1/books/{id}/cover/thumb`, session-cookie gated. See
    /// [`BookListRow::cover_url`].
    pub cover_url: String,
    /// Tag names attached to the manifestation.
    pub tags: Vec<String>,
    /// Genre names attached to the manifestation.
    pub genres: Vec<String>,
    /// Mood names attached to the manifestation.
    pub moods: Vec<String>,
    /// Audience-suitability rating (`manifestations.content_rating`);
    /// `None` when unrated.
    pub content_rating: Option<ContentRating>,
    /// Ingestion lifecycle state.
    pub ingestion_status: IngestionStatus,
    /// Validation lifecycle state.
    pub validation_status: ValidationStatus,
    /// Enrichment lifecycle state.
    pub enrichment_status: EnrichmentStatus,
    /// External-source identifiers for visible providers: the parent
    /// work's ids followed by this edition's, each sorted by scheme.
    /// Providers hidden via `provider_visibility` are absent.
    pub external_ids: Vec<ExternalIdRef>,
    /// Per-provider aggregate ratings for visible providers, sorted by
    /// source.
    pub external_ratings: Vec<ExternalRatingRef>,
    /// Metadata-version counts for the Versions tab.
    pub metadata_version_summary: MetadataVersionSummary,
    /// Pending `metadata_versions` rows for this manifestation. Filtered
    /// to `status = 'pending'` AND not currently promoted as a canonical
    /// pointer. Ordered by `last_seen_at DESC`. Empty when the operator
    /// has no drafts to review. Surfaced for the Versions-tab UI; the
    /// summary counts above remain so clients can render the tab badge
    /// without parsing the full list.
    pub metadata_versions: Vec<MetadataVersionRow>,
    /// `manifestations.created_at`. RFC 3339 on the wire, which the
    /// frontend `BookDetailSchema` requires.
    pub created_at: DateTime<Utc>,
    /// `manifestations.updated_at`. RFC 3339 on the wire (see `created_at`).
    pub updated_at: DateTime<Utc>,
}

/// One pending draft row surfaced on the Versions tab.
///
/// Serialised as
/// JSON in the `metadata_versions` array of `GET /api/v1/books/{id}`;
/// the row shape is decoded from the `metadata_versions` table
/// (migration `20260412150003_series_and_metadata`) with the canonical
/// columns plus a stringified enum.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
#[non_exhaustive]
pub struct MetadataVersionRow {
    /// `metadata_versions.id` — primary key for accept/reject calls.
    pub id: Uuid,
    /// Canonical field name (`title`, `description`, `language`,
    /// `publisher`, `pub_date`, `isbn_10`, `isbn_13`, `cover`).
    pub field_name: String,
    /// Source identifier — `openlibrary`, `google_books`, `manual`, etc.
    pub source: String,
    /// Proposed value, untyped JSON; field-specific shape (string for
    /// title/description, ISO date for `pub_date`, …).
    pub new_value: Value,
    /// Always `pending` for rows surfaced here; promotion lives on
    /// canonical pointer columns, not this enum.
    pub status: String,
    /// Confidence in `[0.0, 1.0]` from the enrichment pipeline.
    pub confidence_score: f32,
    /// Match-type tag from the enrichment pipeline (`isbn`, `title`, …).
    pub match_type: String,
    /// Number of times the pipeline has observed this exact value.
    pub observation_count: i32,
}

/// Counts surfaced on the book-detail Versions tab.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
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

/// `/api/v1/works/{id}` response. Lists every manifestation the user
/// can see for a given work, grouped under the work-level prose.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
#[non_exhaustive]
pub struct WorkDetail {
    /// `works.id`.
    pub id: Uuid,
    /// `works.title`.
    pub title: String,
    /// `works.subtitle`, when declared.
    pub subtitle: Option<String>,
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

/// `GET /api/v1/search` envelope (11b).
///
/// Wraps a flat result list — the
/// frontend groups by [`SearchHit::kind`] client-side. No cursor:
/// search is bounded by `LIMIT` server-side; pagination is a follow-up
/// if user-research warrants it.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
#[non_exhaustive]
pub struct SearchResponse {
    /// Result rows, ranked DESC by hybrid `ts_rank_cd + similarity`.
    pub items: Vec<SearchHit>,
}

/// One result row of `GET /api/v1/search`.
///
/// Carries the bare minimum the
/// command-palette UI needs: a kind tag for grouping, identifiers for
/// navigation, a short display label, and an optional `ts_headline`
/// snippet with non-HTML ASCII STX (`\u{0002}`) / ETX (`\u{0003}`)
/// markers so the React renderer can avoid `dangerouslySetInnerHTML`.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
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
    /// ASCII STX (`\u{0002}`) / ETX (`\u{0003}`) start/stop markers
    /// around matched terms. Control codepoints, not valid UTF-8 text,
    /// so they cannot collide with user typography. `None` when the
    /// hit was trigram-only (no tsquery match → headline would be the
    /// raw text without highlighting).
    pub snippet: Option<String>,
    /// Cover thumbnail URL for `book` results — relative path under
    /// `/api/v1/books/{id}/cover/thumb`, session-cookie gated. `None`
    /// when the hit kind has no cover surface (future author/series
    /// variants).
    pub cover_url: Option<String>,
}

/// Tag identifying which entity a [`SearchHit`] points at. Serialised
/// in `snake_case` to match the rest of the JSON API conventions.
#[derive(Debug, Clone, Copy, Serialize, utoipa::ToSchema)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum SearchHitKind {
    /// Manifestation hit — id is the `manifestations.id`.
    Book,
}

/// One manifestation row embedded in a [`WorkDetail`] response.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
#[non_exhaustive]
pub struct WorkManifestation {
    /// `manifestations.id`.
    pub id: Uuid,
    /// `manifestations.isbn_13`.
    pub isbn_13: Option<String>,
    /// `manifestations.isbn_10`.
    pub isbn_10: Option<String>,
    /// `manifestations.pages`, when known.
    pub pages: Option<i32>,
    /// Cover thumbnail URL — relative path under
    /// `/api/v1/books/{id}/cover/thumb`, session-cookie gated. See
    /// [`BookListRow::cover_url`].
    pub cover_url: String,
    /// Ingestion lifecycle state.
    pub ingestion_status: IngestionStatus,
    /// Validation lifecycle state.
    pub validation_status: ValidationStatus,
    /// Enrichment lifecycle state.
    pub enrichment_status: EnrichmentStatus,
    /// `manifestations.created_at`. RFC 3339 on the wire, which the
    /// frontend `WorkManifestationSchema` requires.
    pub created_at: DateTime<Utc>,
}
