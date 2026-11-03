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
    #[must_use]
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
    #[must_use]
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
