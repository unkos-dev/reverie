//! Closed failure classes persisted with a failed enrichment attempt.
//!
//! `manifestations.enrichment_failures` holds a JSON array of
//! [`FailureEntry`] values, primary first. The classes are the only
//! description of a failure that leaves the server: the raw
//! `enrichment_error` text can carry internal error detail.

/// Why one metadata source, or the run itself, failed.
#[derive(
    Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum FailureClass {
    /// The source did not answer within the request timeout.
    Timeout,
    /// The source signalled quota exhaustion.
    RateLimited,
    /// The source answered with an unexpected non-success status.
    SourceError,
    /// The source had no record for the book.
    NotFound,
    /// The request failed or the reply could not be read.
    Unreachable,
    /// The enrichment run failed for a reason internal to Reverie.
    Internal,
    /// A failure recorded without a class.
    Unspecified,
}

impl FailureClass {
    /// Every class, in a stable display order.
    pub const ALL: [Self; 7] = [
        Self::Timeout,
        Self::RateLimited,
        Self::SourceError,
        Self::NotFound,
        Self::Unreachable,
        Self::Internal,
        Self::Unspecified,
    ];

    /// The wire and storage spelling of the class.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Timeout => "timeout",
            Self::RateLimited => "rate_limited",
            Self::SourceError => "source_error",
            Self::NotFound => "not_found",
            Self::Unreachable => "unreachable",
            Self::Internal => "internal",
            Self::Unspecified => "unspecified",
        }
    }
}

/// One failure: the source it came from and its class. `source` is absent
/// for failures that belong to no source (`internal`, `unspecified`).
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct FailureEntry {
    /// Registry key of the failing source, if the failure has one.
    pub source: Option<String>,
    /// Why it failed.
    pub class: FailureClass,
}
