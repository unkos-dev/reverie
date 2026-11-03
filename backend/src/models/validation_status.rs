//! `ValidationStatus` — closed value set for the Postgres `validation_status`
//! ENUM applied to `manifestations.validation_status`.
//!
//! Defensive type-safety, continuing the `sqlx::Type` enum series:
//! pre-migration this was the last DB enum
//! exposed as a raw `String` field on the public API DTOs ([`BookListRow`],
//! [`BookDetail`], [`WorkManifestation`]), so an unknown DB variant flowed to
//! the wire as an opaque string. `sqlx::Type` decode of an unknown variant now
//! returns an error rather than coercing into a string. (`ingestion_status`
//! and `enrichment_status` carry typed `sqlx::Type` impls on write but are
//! still `String`-decoded then re-parsed in the list read path — see
//! `crate::routes::library`.)
//!
//! [`BookListRow`]: crate::models::library::BookListRow
//! [`BookDetail`]: crate::models::library::BookDetail
//! [`WorkManifestation`]: crate::models::library::WorkManifestation
//!
//! The value set reconciles three previously-divergent vocabularies onto the
//! authoritative domain enum
//! [`ValidationOutcome`](crate::services::epub::ValidationOutcome): the stored
//! label `clean` (renamed from `valid`) matches `ValidationOutcome::Clean`,
//! eliminating the name translation in the ingestion orchestrator (the mapping
//! arm is now `Clean => Clean`, not `Clean => "valid"`). `quarantined` is
//! deliberately absent — a quarantined file is deleted, so no manifestation
//! row is ever written. See
//! `docs/adr/0013-a-typed-validationstatus-enum-reconciles-the-vocabulary.md`.
//!
//! Wire formats:
//! - Postgres: `validation_status` ENUM type.
//! - JSON: lowercase string —
//!   `"pending"` | `"clean"` | `"repaired"` | `"degraded"` | `"failed"`.

/// Outcome of structural validation for a single manifestation.
///
/// Wire-format invariant: variants serialise to the lowercase forms declared
/// in the `#[serde]` and `#[sqlx]` attributes. `clean`, `repaired`, and
/// `degraded` are all stored-and-served outcomes on one quality tier — `clean`
/// means *no issues found*, not *the only valid state*. Unknown DB variants
/// fail decode loudly at the boundary instead of coercing into a string.
///
/// `#[non_exhaustive]` is deliberately absent: `reverie_api` is a single
/// lib+bin crate with no foreign downstream matching on this enum, and the
/// loud-decode-failure design *wants* the compile error when a
/// [`ValidationOutcome`](crate::services::epub::ValidationOutcome) variant is
/// added without a matching arm — `non_exhaustive` would suppress exactly that.
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
#[sqlx(type_name = "validation_status", rename_all = "lowercase")]
pub enum ValidationStatus {
    /// Row exists; validation has not run (column default). Includes
    /// formats with no structural validator (anything non-EPUB), which
    /// stay `pending` until a validator for their format exists.
    Pending,
    /// Validation found no issues.
    Clean,
    /// Validation found issues that were automatically repaired.
    Repaired,
    /// Validation found issues that are tolerated; the file is still served.
    Degraded,
    /// The validator itself could not run to completion (internal error —
    /// IO failure, crash), so nothing is known about the file's structural
    /// quality. Distinct from [`Self::Degraded`] (validator ran, found
    /// tolerable issues) and from [`Self::Pending`] (validator never
    /// attempted) so operators can monitor validator failures
    /// directly. The file is still ingested and served.
    Failed,
}

impl ValidationStatus {
    /// Canonical wire string. Matches the `#[serde(rename_all)]` and
    /// `#[sqlx(rename_all)]` mappings — `Debug` yields the Rust variant name
    /// (`"Clean"`), which does not match the Postgres / JSON form. Use this for
    /// log lines and error messages so the three surfaces stay consistent.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Clean => "clean",
            Self::Repaired => "repaired",
            Self::Degraded => "degraded",
            Self::Failed => "failed",
        }
    }
}

impl std::fmt::Display for ValidationStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}
