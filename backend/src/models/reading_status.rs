//! `ReadingStatus`: closed value set for the Postgres `reading_status` ENUM
//! applied to `reading_state.status`.
//!
//! Wire formats:
//! - Postgres: `reading_status` ENUM type.
//! - JSON: `snake_case` string:
//!   `"want_to_read"` | `"reading"` | `"on_hold"` | `"finished"` | `"abandoned"`.

/// A user's relationship to one book: where they are in reading it.
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
#[sqlx(type_name = "reading_status", rename_all = "snake_case")]
pub enum ReadingStatus {
    /// On the caller's radar but not started.
    WantToRead,
    /// Currently being read. Stamps `started_at` on first transition.
    Reading,
    /// Paused partway through. Timestamps are left as-is.
    OnHold,
    /// Finished. Stamps `finished_at`, `progress_pct = 100`, `last_read_at`.
    Finished,
    /// Given up on. Timestamps are left as-is.
    Abandoned,
}

impl ReadingStatus {
    /// Canonical wire string. Matches the `#[serde(rename_all)]` and
    /// `#[sqlx(rename_all)]` mappings (`Debug` yields the Rust variant name,
    /// `"WantToRead"`, which does not match the Postgres / JSON form). Use
    /// this for log lines and error messages so the three surfaces stay
    /// consistent.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::WantToRead => "want_to_read",
            Self::Reading => "reading",
            Self::OnHold => "on_hold",
            Self::Finished => "finished",
            Self::Abandoned => "abandoned",
        }
    }

    /// Parse a canonical wire string back into a variant, the inverse of
    /// [`Self::as_str`]. `None` for any string that is not one of the five
    /// wire names, so a caller can reject an unknown status token without
    /// routing it through serde.
    pub fn from_wire(raw: &str) -> Option<Self> {
        match raw {
            "want_to_read" => Some(Self::WantToRead),
            "reading" => Some(Self::Reading),
            "on_hold" => Some(Self::OnHold),
            "finished" => Some(Self::Finished),
            "abandoned" => Some(Self::Abandoned),
            _ => None,
        }
    }
}

impl std::fmt::Display for ReadingStatus {
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
            (ReadingStatus::WantToRead, "want_to_read"),
            (ReadingStatus::Reading, "reading"),
            (ReadingStatus::OnHold, "on_hold"),
            (ReadingStatus::Finished, "finished"),
            (ReadingStatus::Abandoned, "abandoned"),
        ] {
            assert_eq!(variant.as_str(), wire);
            assert_eq!(format!("{variant}"), wire);
        }
    }

    #[test]
    fn json_roundtrip_uses_snake_case_string() {
        let status = ReadingStatus::OnHold;
        let json = serde_json::to_string(&status).expect("serialize");
        assert_eq!(json, "\"on_hold\"");
        let back: ReadingStatus = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, ReadingStatus::OnHold);
    }

    #[test]
    fn json_rejects_unknown_variant() {
        let result: Result<ReadingStatus, _> = serde_json::from_str("\"currently_reading\"");
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
        sqlx::query("ALTER TYPE reading_status ADD VALUE 'probe_unknown'")
            .execute(&pool)
            .await
            .expect("alter reading_status enum");

        let result: Result<ReadingStatus, _> =
            sqlx::query_scalar("SELECT 'probe_unknown'::reading_status")
                .fetch_one(&pool)
                .await;
        assert!(
            result.is_err(),
            "expected sqlx decode error for unknown DB variant, got {result:?}"
        );
    }
}
