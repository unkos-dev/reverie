//! `ContentRating`: closed value set for the Postgres `content_rating` ENUM
//! applied to `manifestations.content_rating`.
//!
//! Wire formats:
//! - Postgres: `content_rating` ENUM type.
//! - JSON: `snake_case` string:
//!   `"everyone"` | `"teen"` | `"mature"` | `"adult"` | `"explicit"`.

/// The audience-suitability classification of a manifestation's content.
///
/// Wire-format invariant: variants serialise to the `snake_case` forms
/// declared in the `#[serde]` and `#[sqlx]` attributes. Unknown DB variants
/// fail decode loudly at the boundary instead of coercing into a string,
/// matching [`crate::models::validation_status::ValidationStatus`].
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
#[sqlx(type_name = "content_rating", rename_all = "snake_case")]
pub enum ContentRating {
    /// Suitable for all audiences.
    Everyone,
    /// Suitable for teen audiences and up.
    Teen,
    /// Contains mature themes.
    Mature,
    /// Suitable for adult audiences only.
    Adult,
    /// Contains explicit content.
    Explicit,
}

impl ContentRating {
    /// Canonical wire string. Matches the `#[serde(rename_all)]` and
    /// `#[sqlx(rename_all)]` mappings (`Debug` yields the Rust variant name,
    /// `"Everyone"`, which does not match the Postgres / JSON form). Use
    /// this for log lines and error messages so the three surfaces stay
    /// consistent.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Everyone => "everyone",
            Self::Teen => "teen",
            Self::Mature => "mature",
            Self::Adult => "adult",
            Self::Explicit => "explicit",
        }
    }
}

impl std::fmt::Display for ContentRating {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}
