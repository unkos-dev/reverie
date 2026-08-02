//! `ContributorRole`: closed value set for the Postgres `author_role` ENUM
//! applied to `work_authors.role`.
//!
//! Wire formats:
//! - Postgres: `author_role` ENUM type (see migration
//!   `20260526000000_initial_schema.up.sql`).
//! - JSON: `snake_case` string:
//!   `"author"` | `"editor"` | `"translator"` | `"narrator"`.

/// How one person contributed to a work.
///
/// Wire-format invariant: variants serialise to the `snake_case` forms
/// declared in the `#[serde]` and `#[sqlx]` attributes. Unknown DB variants
/// fail decode loudly at the boundary instead of coercing into a string,
/// matching [`crate::models::validation_status::ValidationStatus`].
///
/// Variant order mirrors the Postgres declaration order, which is also the
/// order the enum compares in `ORDER BY` — contributor lists sorted by role
/// on the DB side surface in this same order on the wire.
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
#[sqlx(type_name = "author_role", rename_all = "snake_case")]
pub enum ContributorRole {
    /// Wrote the work. Feeds the `authors` display array, never the
    /// `contributors` slot.
    Author,
    /// Edited the work (anthologies, collections).
    Editor,
    /// Translated the work into this edition's language.
    Translator,
    /// Narrated the audiobook edition.
    Narrator,
}
