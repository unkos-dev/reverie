//! `ManifestationFormat` — closed value set for the Postgres `manifestation_format` ENUM.
//!
//! Replaces the prior stringly-typed `format: String` populated via a
//! `format::text` SQL cast plus the `SUPPORTED_FORMATS: &[&str]` validation
//! const. With this typed enum:
//!
//! - The DB schema and Rust both reference the same closed set; renaming a
//!   variant compile-errors at every consuming site.
//! - The env-var parser (`REVERIE_ACCEPTED_FORMATS`) rejects unknown values
//!   loudly via `FromStr` instead of silently coercing.
//! - `sqlx::Type` decode of an unknown DB variant returns an error rather
//!   than fabricating a string.
//!
//! Wire formats:
//! - Postgres: `manifestation_format` ENUM type (see migration
//!   `20260412150001_extensions_enums_and_roles.up.sql`).
//! - JSON / config / file extensions: lowercase string —
//!   "epub" | "pdf" | "mobi" | "azw3" | "cbz" | "cbr".

use std::fmt;
use std::str::FromStr;

/// Canonical file format of a manifestation.
///
/// Closed set shared by the Postgres `manifestation_format` `ENUM`, the
/// `REVERIE_ACCEPTED_FORMATS` env-var parser, and download-path content
/// negotiation. Extending the set requires both a Rust variant and a
/// matching `ALTER TYPE … ADD VALUE` migration.
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
    schemars::JsonSchema,
    utoipa::ToSchema,
)]
#[serde(rename_all = "lowercase")]
#[sqlx(type_name = "manifestation_format", rename_all = "lowercase")]
pub enum ManifestationFormat {
    /// EPUB 2 / 3 reflowable e-book.
    Epub,
    /// Portable Document Format.
    Pdf,
    /// Mobipocket / older Kindle format.
    Mobi,
    /// Amazon Kindle Format 8 (`.azw3`).
    Azw3,
    /// Comic Book ZIP archive.
    Cbz,
    /// Comic Book RAR archive.
    Cbr,
}

impl ManifestationFormat {
    /// Wire string for the JSON value, env config, and file-extension
    /// matching. Matches the `#[serde(rename_all)]` and
    /// `#[sqlx(rename_all)]` mappings.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Epub => "epub",
            Self::Pdf => "pdf",
            Self::Mobi => "mobi",
            Self::Azw3 => "azw3",
            Self::Cbz => "cbz",
            Self::Cbr => "cbr",
        }
    }
}

impl fmt::Display for ManifestationFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Error returned by [`ManifestationFormat`]'s [`std::str::FromStr`] impl
/// when the input does not match a known wire string.
///
/// The wrapped value
/// is the offending input, surfaced through the [`std::error::Error`]
/// `Display` for user-facing diagnostics.
#[derive(Debug, thiserror::Error)]
#[error("unsupported manifestation_format '{0}'")]
pub struct ParseManifestationFormatError(String);

impl FromStr for ManifestationFormat {
    type Err = ParseManifestationFormatError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "epub" => Ok(Self::Epub),
            "pdf" => Ok(Self::Pdf),
            "mobi" => Ok(Self::Mobi),
            "azw3" => Ok(Self::Azw3),
            "cbz" => Ok(Self::Cbz),
            "cbr" => Ok(Self::Cbr),
            other => Err(ParseManifestationFormatError(other.to_owned())),
        }
    }
}
