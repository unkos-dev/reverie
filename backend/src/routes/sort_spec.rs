//! Whitelisted sort columns and the `?sort=` grammar for `/api/v1/books`.
//!
//! Client sort strings are JSON:API-style comma-separated stacks
//! (`"author,-created_at"`): each field names a whitelisted column, an
//! optional leading `-` reverses that level to descending, and the order
//! of appearance sets priority. [`SortSpec::parse`] is the sole entry
//! point from untrusted input - nothing outside this module resolves a
//! client string against SQL directly. Only [`SortColumn::sql_expr`]'s
//! fixed `&'static str` fragments ever reach a `QueryBuilder`.
//!
//! A column enters [`SortColumn`] only once its ordering indexes exist.
//! Nullable columns need both an ascending and a `DESC NULLS LAST`
//! composite index, because Postgres's default DESC ordering is NULLS
//! FIRST and cannot ride a backward scan of an ascending index.

/// Maximum number of levels a sort stack may carry.
pub const MAX_SORT_LEVELS: usize = 3;

/// A column the `?sort=` parameter is permitted to reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum SortColumn {
    /// Work title, alphabetical (`works.sort_title`).
    Title,
    /// First author's sort name (`works.first_author_sort_name`).
    Author,
    /// Manifestation ingestion timestamp (`manifestations.created_at`).
    CreatedAt,
    /// Manifestation page count (`manifestations.pages`).
    Pages,
}

impl SortColumn {
    /// The `?sort=` field name this column answers to.
    pub const fn wire_name(self) -> &'static str {
        match self {
            Self::Title => "title",
            Self::Author => "author",
            Self::CreatedAt => "created_at",
            Self::Pages => "pages",
        }
    }

    /// Resolve a wire field name against the whitelist. Case-sensitive:
    /// only the exact lowercase names returned by [`Self::wire_name`]
    /// match.
    pub fn from_wire(value: &str) -> Option<Self> {
        match value {
            "title" => Some(Self::Title),
            "author" => Some(Self::Author),
            "created_at" => Some(Self::CreatedAt),
            "pages" => Some(Self::Pages),
            _ => None,
        }
    }

    /// The SQL expression this column orders and filters by. A fixed
    /// `&'static str` - never built from client input.
    pub const fn sql_expr(self) -> &'static str {
        match self {
            Self::Title => "w.sort_title",
            Self::Author => "w.first_author_sort_name",
            Self::CreatedAt => "m.created_at",
            Self::Pages => "m.pages",
        }
    }

    /// Whether this column's SQL expression can be `NULL`, requiring the
    /// cascade predicate's null-bucket branch and a `NULLS LAST` ORDER BY.
    pub const fn nullable(self) -> bool {
        matches!(self, Self::Author | Self::Pages)
    }

    /// The row alias this column is selected under in the list query,
    /// used to read the boundary value back off a result row when
    /// minting the next page's cursor.
    pub const fn select_alias(self) -> &'static str {
        match self {
            Self::Title => "sort_title",
            Self::Author => "first_author_sort_name",
            Self::CreatedAt => "created_at",
            Self::Pages => "pages",
        }
    }
}

/// Ascending or descending order for one [`SortLevel`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortDirection {
    /// `ASC`.
    Asc,
    /// `DESC`.
    Desc,
}

impl SortDirection {
    /// The `ORDER BY` keyword for this direction.
    pub const fn sql(self) -> &'static str {
        match self {
            Self::Asc => "ASC",
            Self::Desc => "DESC",
        }
    }

    /// The comparison operator a keyset "advance past the boundary"
    /// predicate uses under this direction.
    pub const fn comparison_op(self) -> char {
        match self {
            Self::Asc => '>',
            Self::Desc => '<',
        }
    }
}

/// One level of a sort stack: a whitelisted column plus its direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SortLevel {
    /// The whitelisted column this level orders by.
    pub column: SortColumn,
    /// This level's direction.
    pub direction: SortDirection,
}

/// A validated sort stack for `?sort=` on `/api/v1/books`.
///
/// Levels are ordered by priority - the first level is the primary sort
/// key. [`Self::default`] is a single level, `-created_at`, matching the
/// endpoint's pre-stack "recent" behavior byte-for-byte.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SortSpec {
    levels: Vec<SortLevel>,
}

impl Default for SortSpec {
    fn default() -> Self {
        Self {
            levels: vec![SortLevel {
                column: SortColumn::CreatedAt,
                direction: SortDirection::Desc,
            }],
        }
    }
}

impl SortSpec {
    /// Parse a JSON:API sort string (e.g. `"author,-created_at"`) against
    /// the [`SortColumn`] whitelist.
    ///
    /// # Errors
    /// Returns [`SortSpecError::Empty`] for an empty string,
    /// [`SortSpecError::UnknownField`] for a field outside the whitelist
    /// (including an empty field from a stray comma or a lone `-`),
    /// [`SortSpecError::Duplicate`] when a column appears twice, and
    /// [`SortSpecError::TooManyLevels`] past [`MAX_SORT_LEVELS`].
    pub fn parse(raw: &str) -> Result<Self, SortSpecError> {
        if raw.is_empty() {
            return Err(SortSpecError::Empty);
        }
        let fields: Vec<&str> = raw.split(',').collect();
        if fields.len() > MAX_SORT_LEVELS {
            return Err(SortSpecError::TooManyLevels);
        }
        let mut levels = Vec::with_capacity(fields.len());
        for field in fields {
            let (direction, name) = field
                .strip_prefix('-')
                .map_or((SortDirection::Asc, field), |rest| {
                    (SortDirection::Desc, rest)
                });
            let column = SortColumn::from_wire(name)
                .ok_or_else(|| SortSpecError::UnknownField(name.to_owned()))?;
            if levels
                .iter()
                .any(|level: &SortLevel| level.column == column)
            {
                return Err(SortSpecError::Duplicate(column.wire_name()));
            }
            levels.push(SortLevel { column, direction });
        }
        Ok(Self { levels })
    }

    /// The stack's levels, in priority order.
    pub fn levels(&self) -> &[SortLevel] {
        &self.levels
    }

    /// Render this spec back to its JSON:API wire string, e.g.
    /// `"author,-created_at"`. Used to echo `?sort=` in `next_cursor`
    /// links and as the cursor's spec tag.
    pub fn canonical(&self) -> String {
        self.levels
            .iter()
            .map(|level| match level.direction {
                SortDirection::Asc => level.column.wire_name().to_owned(),
                SortDirection::Desc => format!("-{}", level.column.wire_name()),
            })
            .collect::<Vec<_>>()
            .join(",")
    }
}

/// Parse failures for [`SortSpec::parse`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum SortSpecError {
    /// The `?sort=` value was an empty string.
    #[error("sort parameter must not be empty")]
    Empty,
    /// A field did not match any [`SortColumn`] wire name.
    #[error("unknown sort field: {0}")]
    UnknownField(String),
    /// The same column appeared at more than one level.
    #[error("duplicate sort column: {0}")]
    Duplicate(&'static str),
    /// The stack exceeded [`MAX_SORT_LEVELS`].
    #[error("sort stack exceeds the maximum of 3 levels")]
    TooManyLevels,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_single_level_ascending() {
        let spec = SortSpec::parse("title").expect("parse");
        assert_eq!(
            spec.levels(),
            &[SortLevel {
                column: SortColumn::Title,
                direction: SortDirection::Asc
            }]
        );
    }

    #[test]
    fn parses_single_level_descending() {
        let spec = SortSpec::parse("-created_at").expect("parse");
        assert_eq!(
            spec.levels(),
            &[SortLevel {
                column: SortColumn::CreatedAt,
                direction: SortDirection::Desc
            }]
        );
    }

    #[test]
    fn parses_two_levels_mixed_direction() {
        let spec = SortSpec::parse("author,-created_at").expect("parse");
        assert_eq!(
            spec.levels(),
            &[
                SortLevel {
                    column: SortColumn::Author,
                    direction: SortDirection::Asc
                },
                SortLevel {
                    column: SortColumn::CreatedAt,
                    direction: SortDirection::Desc
                }
            ]
        );
    }

    #[test]
    fn parses_three_levels() {
        let spec = SortSpec::parse("author,-created_at,title").expect("parse");
        assert_eq!(spec.levels().len(), 3);
        assert_eq!(spec.levels()[2].column, SortColumn::Title);
    }

    #[test]
    fn canonical_round_trips_normalized_input() {
        for s in [
            "title",
            "-created_at",
            "author,-created_at",
            "author,-created_at,title",
        ] {
            let spec = SortSpec::parse(s).expect("parse");
            assert_eq!(spec.canonical(), s);
        }
    }

    #[test]
    fn default_is_descending_created_at() {
        assert_eq!(SortSpec::default().canonical(), "-created_at");
    }

    #[test]
    fn rejects_empty_string() {
        assert_eq!(SortSpec::parse(""), Err(SortSpecError::Empty));
    }

    #[test]
    fn rejects_lone_dash() {
        assert_eq!(
            SortSpec::parse("-"),
            Err(SortSpecError::UnknownField(String::new()))
        );
    }

    #[test]
    fn rejects_unknown_field() {
        assert_eq!(
            SortSpec::parse("isbn_13"),
            Err(SortSpecError::UnknownField("isbn_13".into()))
        );
    }

    #[test]
    fn rejects_duplicate_column() {
        assert_eq!(
            SortSpec::parse("title,-title"),
            Err(SortSpecError::Duplicate("title"))
        );
    }

    #[test]
    fn rejects_more_than_three_levels() {
        assert_eq!(
            SortSpec::parse("title,author,created_at,pages"),
            Err(SortSpecError::TooManyLevels)
        );
    }

    #[test]
    fn rejects_trailing_comma() {
        assert_eq!(
            SortSpec::parse("title,"),
            Err(SortSpecError::UnknownField(String::new()))
        );
    }

    #[test]
    fn rejects_uppercase_field_case_sensitively() {
        assert_eq!(
            SortSpec::parse("Title"),
            Err(SortSpecError::UnknownField("Title".into()))
        );
    }
}
