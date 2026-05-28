//! `ValidationStatus` — closed value set for the Postgres `validation_status`
//! ENUM applied to `manifestations.validation_status`.
//!
//! Defensive type-safety (UNK-276, closes the `sqlx::Type` enum series begun
//! under UNK-107 / UNK-173): pre-migration this was the last DB enum decoded
//! into Rust as a raw `String`, so an unknown DB variant flowed to the wire as
//! an opaque string. `sqlx::Type` decode of an unknown variant now returns an
//! error rather than coercing into a string.
//!
//! The value set reconciles three previously-divergent vocabularies onto the
//! authoritative domain enum
//! [`ValidationOutcome`](crate::services::epub::ValidationOutcome): the stored
//! label `clean` (renamed from `valid`) matches `ValidationOutcome::Clean`,
//! making the ingestion orchestrator mapping an identity. `quarantined` is
//! deliberately absent — a quarantined file is deleted, so no manifestation
//! row is ever written. See
//! `adr/2026-05-28-validation-status-vocabulary.md`.
//!
//! Wire formats:
//! - Postgres: `validation_status` ENUM type (see migration
//!   `20260528153728_rename_validation_status_valid_to_clean.up.sql` and the
//!   consolidated initial schema).
//! - JSON: lowercase string —
//!   `"pending"` | `"clean"` | `"repaired"` | `"degraded"`.

/// Outcome of structural validation for a single manifestation.
///
/// Wire-format invariant: variants serialise to the lowercase forms declared
/// in the `#[serde]` and `#[sqlx]` attributes. `clean`, `repaired`, and
/// `degraded` are all stored-and-served outcomes on one quality tier — `clean`
/// means *no issues found*, not *the only valid state*. Unknown DB variants
/// fail decode loudly at the boundary instead of coercing into a string.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, sqlx::Type,
)]
#[serde(rename_all = "lowercase")]
#[sqlx(type_name = "validation_status", rename_all = "lowercase")]
pub enum ValidationStatus {
    /// Row exists; validation has not run yet (column default).
    Pending,
    /// Validation found no issues.
    Clean,
    /// Validation found issues that were automatically repaired.
    Repaired,
    /// Validation found issues that are tolerated; the file is still served.
    Degraded,
}

impl ValidationStatus {
    /// Canonical wire string. Matches the `#[serde(rename_all)]` and
    /// `#[sqlx(rename_all)]` mappings — `Debug` yields the Rust variant name
    /// (`"Clean"`), which does not match the Postgres / JSON form. Use this for
    /// log lines and error messages so the three surfaces stay consistent.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Clean => "clean",
            Self::Repaired => "repaired",
            Self::Degraded => "degraded",
        }
    }
}

impl std::fmt::Display for ValidationStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn as_str_matches_serde_lowercase() {
        for (variant, wire) in [
            (ValidationStatus::Pending, "pending"),
            (ValidationStatus::Clean, "clean"),
            (ValidationStatus::Repaired, "repaired"),
            (ValidationStatus::Degraded, "degraded"),
        ] {
            assert_eq!(variant.as_str(), wire);
            assert_eq!(format!("{variant}"), wire);
        }
    }

    #[test]
    fn json_roundtrip_uses_lowercase_string() {
        let status = ValidationStatus::Clean;
        let json = serde_json::to_string(&status).expect("serialize");
        assert_eq!(json, "\"clean\"");
        let back: ValidationStatus = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, ValidationStatus::Clean);
    }

    #[test]
    fn json_rejects_unknown_variant() {
        let result: Result<ValidationStatus, _> = serde_json::from_str("\"valid\"");
        assert!(result.is_err(), "expected legacy 'valid' to be rejected");
    }

    /// Loud-failure regression for UNK-276 (mirrors the UNK-107
    /// `manifestation_format` probe). Simulates the DB `validation_status` enum
    /// gaining a value with no Rust counterpart (out-of-band `ALTER TYPE`, or a
    /// migration landing ahead of the matching Rust change). `sqlx::Type` must
    /// surface this as a decode error, not silently coerce.
    #[sqlx::test(migrations = "./migrations")]
    async fn decode_fails_for_unknown_db_variant(pool: sqlx::PgPool) {
        // CARVE-OUT (UNK-167): runtime sqlx::query is intentional. The ALTER
        // TYPE is DDL (macros can't validate it), and the SELECT references a
        // variant ('probe_unknown') deliberately not in the prepare-time schema
        // — the entire point of the test is to exercise the unknown-variant
        // decode path. Compile-time macros would refuse to validate.
        sqlx::query("ALTER TYPE validation_status ADD VALUE 'probe_unknown'")
            .execute(&pool)
            .await
            .expect("alter validation_status enum");

        let result: Result<ValidationStatus, _> =
            sqlx::query_scalar("SELECT 'probe_unknown'::validation_status")
                .fetch_one(&pool)
                .await;
        assert!(
            result.is_err(),
            "expected sqlx decode error for unknown DB variant, got {result:?}"
        );
    }
}
