//! `ManifestationFormat` — closed value set for the Postgres `manifestation_format` ENUM.
//!
//! Replaces the prior stringly-typed `format: String` populated via a
//! `format::text` SQL cast plus the `SUPPORTED_FORMATS: &[&str]` validation
//! const. With this typed enum:
//!
//! - The DB schema and Rust both reference the same closed set; renaming a
//!   variant compile-errors at every consuming site.
//! - The env-var parser (`REVERIE_FORMAT_PRIORITY`) rejects unknown values
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
/// `REVERIE_FORMAT_PRIORITY` env-var parser, and download-path content
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
/// when the input does not match a known wire string. The wrapped value
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn as_str_matches_serde_lowercase() {
        for (variant, wire) in [
            (ManifestationFormat::Epub, "epub"),
            (ManifestationFormat::Pdf, "pdf"),
            (ManifestationFormat::Mobi, "mobi"),
            (ManifestationFormat::Azw3, "azw3"),
            (ManifestationFormat::Cbz, "cbz"),
            (ManifestationFormat::Cbr, "cbr"),
        ] {
            assert_eq!(variant.as_str(), wire);
            assert_eq!(format!("{variant}"), wire);
        }
    }

    #[test]
    fn json_roundtrip_uses_lowercase_string() {
        let fmt = ManifestationFormat::Epub;
        let json = serde_json::to_string(&fmt).expect("serialize");
        assert_eq!(json, "\"epub\"");
        let back: ManifestationFormat = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, ManifestationFormat::Epub);
    }

    #[test]
    fn json_rejects_unknown_variant() {
        let result: Result<ManifestationFormat, _> = serde_json::from_str("\"docx\"");
        assert!(result.is_err(), "expected docx to be rejected");
    }

    #[test]
    fn from_str_rejects_unknown_variant() {
        assert!(ManifestationFormat::from_str("docx").is_err());
        assert!(ManifestationFormat::from_str("EPUB").is_err()); // case sensitive
        assert_eq!(
            ManifestationFormat::from_str("epub").unwrap(),
            ManifestationFormat::Epub
        );
    }

    /// Loud-failure regression. Simulates the failure mode where
    /// the DB `manifestation_format` enum gains a value that has no Rust
    /// counterpart (e.g. an operator runs an out-of-band `ALTER TYPE`, or a
    /// future migration lands ahead of the matching Rust change). `sqlx::Type`
    /// must surface this as a decode error, not silently coerce.
    #[sqlx::test(migrations = "./migrations")]
    async fn decode_fails_for_unknown_db_variant(pool: sqlx::PgPool) {
        // CARVE-OUT: runtime sqlx::query is intentional. The ALTER
        // TYPE is DDL (macros can't validate it), and the SELECT references a
        // variant ('djvu') deliberately not in the prepare-time schema — the
        // entire point of the test is to exercise the unknown-variant decode
        // path. Compile-time macros would refuse to validate.
        sqlx::query("ALTER TYPE manifestation_format ADD VALUE 'djvu'")
            .execute(&pool)
            .await
            .expect("alter manifestation_format enum");

        let result: Result<ManifestationFormat, _> =
            sqlx::query_scalar("SELECT 'djvu'::manifestation_format")
                .fetch_one(&pool)
                .await;
        assert!(
            result.is_err(),
            "expected sqlx decode error for unknown DB variant, got {result:?}"
        );
    }
}
