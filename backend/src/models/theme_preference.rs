//! `ThemePreference` — closed value set shared across DB, JSON, and cookie.
//!
//! The set is mirrored in `frontend/src/lib/theme/cookie.ts` as a TypeScript
//! union literal.
//!
//! Wire formats:
//! - Postgres: `theme_preference` ENUM type (see migration
//!   `20260427000001_add_theme_preference.up.sql`).
//! - JSON / Cookie: lowercase string literal — "system" | "light" | "dark".

/// Per-user theme selection mirrored across DB, JSON, and the FOUC theme cookie.
///
/// The variant strings declared here are authoritative across all three
/// surfaces; wire-format drift between them is a defect.
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
#[sqlx(type_name = "theme_preference", rename_all = "lowercase")]
pub enum ThemePreference {
    /// Follow the user-agent's `prefers-color-scheme` (default).
    System,
    /// Force the light palette regardless of user-agent preference.
    Light,
    /// Force the dark palette regardless of user-agent preference.
    Dark,
}

impl ThemePreference {
    /// Wire string for the cookie value and any other place that needs the
    /// canonical lowercase form. Matches the `#[serde(rename_all)]` and
    /// `#[sqlx(rename_all)]` mappings.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn as_str_matches_serde_lowercase() {
        // Wire-format invariant: the frontend
        // `ThemePreference = "system" | "light" | "dark"` union depends on
        // these exact literals.
        assert_eq!(ThemePreference::System.as_str(), "system");
        assert_eq!(ThemePreference::Light.as_str(), "light");
        assert_eq!(ThemePreference::Dark.as_str(), "dark");
    }

    #[test]
    fn json_roundtrip_uses_lowercase_string() {
        let pref = ThemePreference::Dark;
        let json = serde_json::to_string(&pref).expect("serialize");
        assert_eq!(json, "\"dark\"");
        let back: ThemePreference = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, ThemePreference::Dark);
    }

    #[test]
    fn json_rejects_unknown_variant() {
        let result: Result<ThemePreference, _> = serde_json::from_str("\"purple\"");
        assert!(result.is_err(), "expected purple to be rejected");
    }
}
