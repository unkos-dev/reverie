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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn as_str_matches_serde_snake_case() {
        for (variant, wire) in [
            (ContentRating::Everyone, "everyone"),
            (ContentRating::Teen, "teen"),
            (ContentRating::Mature, "mature"),
            (ContentRating::Adult, "adult"),
            (ContentRating::Explicit, "explicit"),
        ] {
            assert_eq!(variant.as_str(), wire);
            assert_eq!(format!("{variant}"), wire);
        }
    }

    #[test]
    fn json_roundtrip_uses_snake_case_string() {
        let rating = ContentRating::Mature;
        let json = serde_json::to_string(&rating).expect("serialize");
        assert_eq!(json, "\"mature\"");
        let back: ContentRating = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, ContentRating::Mature);
    }

    #[test]
    fn json_rejects_unknown_variant() {
        let result: Result<ContentRating, _> = serde_json::from_str("\"restricted\"");
        assert!(result.is_err(), "expected unknown variant to be rejected");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn decode_fails_for_unknown_db_variant(pool: sqlx::PgPool) {
        // CARVE-OUT: runtime sqlx::query is intentional. The ALTER TYPE is
        // DDL (macros can't validate it), and the SELECT references a
        // variant ('probe_unknown') deliberately not in the prepare-time
        // schema; that is the entire point of the test, to exercise the
        // unknown-variant decode path. Compile-time macros would refuse to
        // validate.
        sqlx::query("ALTER TYPE content_rating ADD VALUE 'probe_unknown'")
            .execute(&pool)
            .await
            .expect("alter content_rating enum");

        let result: Result<ContentRating, _> =
            sqlx::query_scalar("SELECT 'probe_unknown'::content_rating")
                .fetch_one(&pool)
                .await;
        assert!(
            result.is_err(),
            "expected sqlx decode error for unknown DB variant, got {result:?}"
        );
    }
}
