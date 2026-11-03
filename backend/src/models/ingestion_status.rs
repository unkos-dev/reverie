//! `IngestionStatus` — closed value set for the Postgres `ingestion_status`
//! ENUM applied to `manifestations.ingestion_status`.
//!
//! Defensive type-safety: there is no current Rust-side `String`
//! field for this enum; values are written as SQL literals
//! (`'complete'::ingestion_status`) at INSERT time and never read back into
//! Rust. Introducing the type lets future read paths decode loudly via
//! `sqlx::Type` and replaces the SQL literal pattern with bindable values.
//!
//! Wire formats:
//! - Postgres: `ingestion_status` ENUM type (see migration
//!   `20260412150001_extensions_enums_and_roles.up.sql`).
//! - JSON: lowercase string —
//!   "pending" | "processing" | "complete" | "failed" | "skipped".

/// Lifecycle state of a single manifestation's ingestion pipeline run.
///
/// Wire-format invariant: variants serialise to the lowercase forms
/// declared in the `#[serde]` and `#[sqlx]` attributes; unknown DB
/// variants fail decode loudly instead of coercing into a string.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    serde::Serialize,
    serde::Deserialize,
    sqlx::Type,
    utoipa::ToSchema,
)]
#[serde(rename_all = "lowercase")]
#[sqlx(type_name = "ingestion_status", rename_all = "lowercase")]
pub enum IngestionStatus {
    /// File enqueued for ingestion.
    Pending,
    /// Worker is parsing and inserting the manifestation row.
    Processing,
    /// Ingestion finished successfully.
    Complete,
    /// Ingestion failed; the cause is recorded on the corresponding
    /// `ingestion_jobs.error_message`
    /// field, not on the manifestation row carrying this status.
    Failed,
    /// File was skipped (e.g. duplicate hash, unsupported format under current policy).
    Skipped,
}

impl IngestionStatus {
    /// Wire string for any place that needs the canonical lowercase form.
    /// Matches the `#[serde(rename_all)]` and `#[sqlx(rename_all)]` mappings.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Processing => "processing",
            Self::Complete => "complete",
            Self::Failed => "failed",
            Self::Skipped => "skipped",
        }
    }
}
