//! `EnrichmentStatus` — closed value set for the Postgres `enrichment_status`
//! ENUM applied to `manifestations.enrichment_status`.
//!
//! Defensive type-safety (UNK-173, extends UNK-107): pre-migration,
//! reads decoded as `String` and matched against literals with a
//! `_ => {}` catch-all that silently dropped unknown variants.
//! `sqlx::Type` decode of an unknown DB variant now returns an error
//! rather than coercing into an unmatched string, and Rust-side
//! `match` arms are exhaustive at compile time.
//!
//! Wire formats:
//! - Postgres: `enrichment_status` ENUM (see migration
//!   `20260417120000_metadata_enrichment.up.sql`).
//! - JSON: lowercase string —
//!   `"pending"` | `"in_progress"` | `"complete"` | `"failed"` | `"skipped"`.

/// Lifecycle state of metadata enrichment for a single manifestation.
///
/// Wire-format invariant: variants serialise to the `snake_case` forms
/// declared in the `#[serde]` and `#[sqlx]` attributes. Drift from
/// either side surfaces as a decode error at the boundary, never as a
/// silently coerced unknown value.
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
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "enrichment_status", rename_all = "snake_case")]
pub enum EnrichmentStatus {
    /// Enqueued; no enrichment attempted yet.
    Pending,
    /// Worker picked up the row and is running.
    InProgress,
    /// Enrichment finished successfully.
    Complete,
    /// Enrichment failed; the row records the failure but is not retried automatically.
    Failed,
    /// Manifestation deliberately bypassed enrichment (e.g. operator override).
    Skipped,
}

impl EnrichmentStatus {
    /// Canonical wire string. Matches the `#[serde(rename_all)]` and
    /// `#[sqlx(rename_all)]` mappings — `Debug` formatting yields the Rust
    /// variant name (`"InProgress"`), which does not match the Postgres /
    /// JSON form. Use this for log lines and error messages so the three
    /// surfaces stay consistent.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::InProgress => "in_progress",
            Self::Complete => "complete",
            Self::Failed => "failed",
            Self::Skipped => "skipped",
        }
    }
}

impl std::fmt::Display for EnrichmentStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn as_str_matches_serde_snake_case() {
        for (variant, wire) in [
            (EnrichmentStatus::Pending, "pending"),
            (EnrichmentStatus::InProgress, "in_progress"),
            (EnrichmentStatus::Complete, "complete"),
            (EnrichmentStatus::Failed, "failed"),
            (EnrichmentStatus::Skipped, "skipped"),
        ] {
            assert_eq!(variant.as_str(), wire);
            assert_eq!(format!("{variant}"), wire);
        }
    }

    #[test]
    fn json_roundtrip_uses_snake_case_string() {
        let status = EnrichmentStatus::InProgress;
        let json = serde_json::to_string(&status).expect("serialize");
        assert_eq!(json, "\"in_progress\"");
        let back: EnrichmentStatus = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, EnrichmentStatus::InProgress);
    }

    #[test]
    fn json_rejects_unknown_variant() {
        let result: Result<EnrichmentStatus, _> = serde_json::from_str("\"resumed\"");
        assert!(result.is_err(), "expected resumed to be rejected");
    }
}
