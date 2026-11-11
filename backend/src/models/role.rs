//! `Role` — closed value set shared across DB and JSON for the `users.role`
//! column.
//!
//! Replaces the prior stringly-typed `role: String` populated via a
//! `role::text` cast over the Postgres `user_role` enum. With this typed
//! enum:
//!
//! - Renaming a Rust variant compile-errors at every consuming site
//!   (e.g. `matches!(self.role, Role::Admin)` in
//!   `auth::middleware::CurrentUser::require_admin`), eliminating the
//!   silent-lockout hazard the typed enum eliminates.
//! - A DB-side variant with no Rust counterpart fails decode loudly via
//!   `sqlx::Type` rather than coercing into an unmatched string.
//!
//! Wire formats:
//! - Postgres: `user_role` ENUM type (see migration
//!   `20260412150001_extensions_enums_and_roles.up.sql`).
//! - JSON: lowercase string literal — "admin" | "adult" | "child".

/// Authorization role assigned to a [`crate::models::user::User`].
///
/// Drives `auth::middleware::CurrentUser::require_admin` and
/// `auth::middleware::CurrentUser::require_not_child`; renaming a variant
/// compile-errors at every consumer, eliminating the silent-lockout
/// hazard.
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
#[sqlx(type_name = "user_role", rename_all = "lowercase")]
pub enum Role {
    /// Full administrative access.
    Admin,
    /// Standard adult user; full library access, no admin actions.
    Adult,
    /// Child profile; sees only the books on its own shelves.
    Child,
}

impl Role {
    /// Wire string for the JSON value and any other place that needs the
    /// canonical lowercase form. Matches the `#[serde(rename_all)]` and
    /// `#[sqlx(rename_all)]` mappings.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Admin => "admin",
            Self::Adult => "adult",
            Self::Child => "child",
        }
    }
}
