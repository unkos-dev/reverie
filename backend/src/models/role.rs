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
//!   silent-lockout hazard described in UNK-108.
//! - A DB-side variant with no Rust counterpart fails decode loudly via
//!   `sqlx::Type` rather than coercing into an unmatched string.
//!
//! Wire formats:
//! - Postgres: `user_role` ENUM type (see migration
//!   `20260412150001_extensions_enums_and_roles.up.sql`).
//! - JSON: lowercase string literal — "admin" | "adult" | "child".

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, sqlx::Type,
)]
#[serde(rename_all = "lowercase")]
#[sqlx(type_name = "user_role", rename_all = "lowercase")]
pub enum Role {
    Admin,
    Adult,
    Child,
}

impl Role {
    /// Wire string for the JSON value and any other place that needs the
    /// canonical lowercase form. Matches the `#[serde(rename_all)]` and
    /// `#[sqlx(rename_all)]` mappings.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Admin => "admin",
            Self::Adult => "adult",
            Self::Child => "child",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn as_str_matches_serde_lowercase() {
        // Wire-format invariant. UNK-105 cross-stack drift guard: any
        // future frontend mirror of this enum depends on these exact
        // literals.
        assert_eq!(Role::Admin.as_str(), "admin");
        assert_eq!(Role::Adult.as_str(), "adult");
        assert_eq!(Role::Child.as_str(), "child");
    }

    #[test]
    fn json_roundtrip_uses_lowercase_string() {
        let role = Role::Admin;
        let json = serde_json::to_string(&role).expect("serialize");
        assert_eq!(json, "\"admin\"");
        let back: Role = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, Role::Admin);
    }

    #[test]
    fn json_rejects_unknown_variant() {
        let result: Result<Role, _> = serde_json::from_str("\"superadmin\"");
        assert!(result.is_err(), "expected superadmin to be rejected");
    }
}
