//! Per-user library display preferences and the installation defaults they
//! fall back to.
//!
//! Every group is an override: `None` means the user has not customised it
//! and inherits the installation default. The defaults live here, once, as
//! Rust constants and travel to the client inside every response, so no
//! second copy of them exists on the other side of the wire to drift.
//!
//! Wire formats:
//! - Postgres: `library_density` / `library_view` ENUM types and a nullable
//!   `text` `sort_stack`.
//! - JSON: lowercase string literals.

use crate::routes::sort_spec::{SortSpec, SortSpecError};

/// Row height for the library table view.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    sqlx::Type,
    utoipa::ToSchema,
)]
#[serde(rename_all = "lowercase")]
#[sqlx(type_name = "library_density", rename_all = "lowercase")]
pub enum LibraryDensity {
    /// Roomier rows (default).
    Comfortable,
    /// Tighter rows, more books per screen.
    Compact,
}

/// Which library presentation the user last chose.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    sqlx::Type,
    utoipa::ToSchema,
)]
#[serde(rename_all = "lowercase")]
#[sqlx(type_name = "library_view", rename_all = "lowercase")]
pub enum LibraryView {
    /// Cover grid (default).
    Grid,
    /// Tabular list.
    Table,
}

/// Installation default density for an account that has not chosen one.
pub const DEFAULT_DENSITY: LibraryDensity = LibraryDensity::Comfortable;

/// Installation default view for an account that has not chosen one.
pub const DEFAULT_VIEW: LibraryView = LibraryView::Grid;

/// Installation default hidden-column set: nothing hidden.
pub const DEFAULT_HIDDEN_COLUMNS: &[&str] = &[];

/// Installation default sort, in the `?sort=` wire grammar.
///
/// Kept equal to
/// [`SortSpec::default`] by `default_sort_stack_matches_the_sort_parser`, so
/// a user who has never chosen a sort gets exactly what the list endpoint
/// would have applied anyway.
pub const DEFAULT_SORT_STACK: &str = "-created_at";

/// Upper bound on how many column keys one account may store.
///
/// The catalog
/// currently offers eight hideable columns; the cap exists so the column is
/// not an unbounded write target, not to constrain the catalog. Mirrored by
/// the `user_preferences_hidden_columns_bounded` CHECK.
pub const MAX_HIDDEN_COLUMNS: usize = 64;

/// Longest single column key accepted.
pub const MAX_COLUMN_KEY_CHARS: usize = 64;

/// The caller's overrides.
///
/// A `None` group inherits the matching constant
/// above; the client is told which is which rather than being handed a
/// resolved value, because per-group reset and any future "has this user
/// customised anything?" question both need the distinction.
#[derive(Debug, Default, Clone, PartialEq, Eq, serde::Serialize, utoipa::ToSchema)]
pub struct PreferenceOverrides {
    /// Column keys hidden in the table view; `null` to inherit.
    pub hidden_columns: Option<Vec<String>>,
    /// Chosen row density; `null` to inherit.
    pub density: Option<LibraryDensity>,
    /// Chosen library view; `null` to inherit.
    pub view: Option<LibraryView>,
    /// Default sort in the `?sort=` wire grammar; `null` to inherit.
    pub sort_stack: Option<String>,
}

/// The installation defaults, echoed alongside the overrides.
#[derive(Debug, Clone, Copy, serde::Serialize, utoipa::ToSchema)]
pub struct PreferenceDefaults {
    /// Columns hidden for an account that has not chosen.
    #[schema(value_type = Vec<String>)]
    pub hidden_columns: &'static [&'static str],
    /// Density for an account that has not chosen.
    pub density: LibraryDensity,
    /// View for an account that has not chosen.
    pub view: LibraryView,
    /// Sort for an account that has not chosen.
    #[schema(value_type = String)]
    pub sort_stack: &'static str,
}

impl PreferenceDefaults {
    /// The installation defaults as served. Const so the values cannot be
    /// assembled differently anywhere else.
    #[must_use]
    pub const fn installation() -> Self {
        Self {
            hidden_columns: DEFAULT_HIDDEN_COLUMNS,
            density: DEFAULT_DENSITY,
            view: DEFAULT_VIEW,
            sort_stack: DEFAULT_SORT_STACK,
        }
    }
}

/// Why a submitted preference value was refused. Each variant maps to a 422
/// with its `Display` text as the problem detail.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum PreferenceError {
    /// A `sort_stack` value outside the `?sort=` grammar or whitelist.
    #[error("sort_stack: {0}")]
    SortStack(#[from] SortSpecError),
    /// More column keys than [`MAX_HIDDEN_COLUMNS`].
    #[error("hidden_columns must hold at most {MAX_HIDDEN_COLUMNS} keys")]
    TooManyHiddenColumns,
    /// A column key outside the accepted shape.
    #[error(
        "hidden_columns entries must be 1 to {MAX_COLUMN_KEY_CHARS} characters of \
         lowercase letters, digits, or underscores"
    )]
    MalformedColumnKey,
}

/// Check a submitted `sort_stack` against the same whitelist and level cap
/// the `?sort=` parameter uses, so a stored default can never name a column
/// the list endpoint would refuse.
///
/// # Errors
/// [`PreferenceError::SortStack`] when the value is empty, names an unknown
/// field, repeats a column, or exceeds the level cap.
pub fn validate_sort_stack(raw: &str) -> Result<(), PreferenceError> {
    SortSpec::parse(raw)?;
    Ok(())
}

/// Check a submitted hidden-column set.
///
/// Unknown-but-well-formed keys are accepted on purpose: the table view
/// intersects stored keys with the columns it actually has, so a key from an
/// older or newer catalog is inert. Only the count and each key's shape are
/// enforced, which is what keeps the column bounded and free of values that
/// would never be a column key.
///
/// # Errors
/// [`PreferenceError::TooManyHiddenColumns`] past [`MAX_HIDDEN_COLUMNS`], and
/// [`PreferenceError::MalformedColumnKey`] for an empty, overlong, or
/// out-of-charset key.
pub fn validate_hidden_columns(keys: &[String]) -> Result<(), PreferenceError> {
    if keys.len() > MAX_HIDDEN_COLUMNS {
        return Err(PreferenceError::TooManyHiddenColumns);
    }
    let well_formed = |key: &String| {
        !key.is_empty()
            && key.chars().count() <= MAX_COLUMN_KEY_CHARS
            && key
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    };
    if keys.iter().all(well_formed) {
        Ok(())
    } else {
        Err(PreferenceError::MalformedColumnKey)
    }
}
