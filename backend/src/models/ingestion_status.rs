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
    /// [`crate::models::ingestion_job::IngestionJob`]'s `error_message`
    /// field, not on the manifestation row carrying this status.
    Failed,
    /// File was skipped (e.g. duplicate hash, unsupported format under current policy).
    Skipped,
}

impl IngestionStatus {
    /// Wire string for any place that needs the canonical lowercase form.
    /// Matches the `#[serde(rename_all)]` and `#[sqlx(rename_all)]` mappings.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn as_str_matches_serde_lowercase() {
        for (variant, wire) in [
            (IngestionStatus::Pending, "pending"),
            (IngestionStatus::Processing, "processing"),
            (IngestionStatus::Complete, "complete"),
            (IngestionStatus::Failed, "failed"),
            (IngestionStatus::Skipped, "skipped"),
        ] {
            assert_eq!(variant.as_str(), wire);
        }
    }

    #[test]
    fn json_roundtrip_uses_lowercase_string() {
        let status = IngestionStatus::Complete;
        let json = serde_json::to_string(&status).expect("serialize");
        assert_eq!(json, "\"complete\"");
        let back: IngestionStatus = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, IngestionStatus::Complete);
    }

    #[test]
    fn json_rejects_unknown_variant() {
        let result: Result<IngestionStatus, _> = serde_json::from_str("\"resumed\"");
        assert!(result.is_err(), "expected resumed to be rejected");
    }
}
