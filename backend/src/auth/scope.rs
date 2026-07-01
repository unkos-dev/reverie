//! `Scope` — credential capability, orthogonal to [`Role`].
//!
//! A session derives its scope set from role at extraction
//! ([`Scope::for_role`]); a personal token carries the explicit scopes chosen
//! at mint, bounded by the owner's role ceiling ([`Scope::grantable_by`]).
//! Enforcement composes by union: a non-admin mutation requires `write`, an
//! admin read requires `admin`, and an admin mutation requires **both**
//! `write` and `admin` (it is a mutation *and* administrative — gating on
//! `admin` alone would let a `[read, admin]` audit token mutate admin
//! endpoints). See `adr/2026-06-23-api-authorization-orthogonal-axes.md`.
//!
//! A dedicated module path is deliberate: `routes::opds::scope::Scope` and
//! `oauth2::Scope` already exist elsewhere in the dependency graph.
//!
//! Wire formats:
//! - Postgres: `scope` ENUM type (see migration
//!   `20260701120000_token_scopes_expiry.up.sql`).
//! - JSON: lowercase string literal — "read" | "write" | "admin".

use std::fmt;
use std::str::FromStr;

use crate::models::role::Role;

/// Credential capability. Composes with [`Role`] (identity gating) and
/// ownership (resource axis) as the three orthogonal authorization axes.
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
#[sqlx(type_name = "scope", rename_all = "lowercase")]
pub enum Scope {
    /// Safe (read-only) operations.
    Read,
    /// Mutating operations.
    Write,
    /// Administrative surface (user management, etc.).
    Admin,
}

impl Scope {
    /// Wire string for the JSON value and DB literal. Matches the
    /// `#[serde(rename_all)]` and `#[sqlx(rename_all)]` mappings.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Write => "write",
            Self::Admin => "admin",
        }
    }

    /// Scope set a session derives from its role. Every role gets at least
    /// `{read, write}` — child restrictions are enforced by
    /// `CurrentUser::require_not_child`, not by withholding scope. Only
    /// [`Role::Admin`] also derives `admin`.
    pub const fn for_role(role: Role) -> &'static [Self] {
        match role {
            Role::Admin => &[Self::Read, Self::Write, Self::Admin],
            Role::Adult | Role::Child => &[Self::Read, Self::Write],
        }
    }

    /// Whether a token owner holding `role` may mint a token carrying this
    /// scope. `read`/`write` are grantable by any role; `admin` requires the
    /// owner to already hold [`Role::Admin`] (a non-admin cannot mint an
    /// admin-scoped token).
    pub const fn grantable_by(self, role: Role) -> bool {
        match self {
            Self::Read | Self::Write => true,
            Self::Admin => matches!(role, Role::Admin),
        }
    }
}

impl fmt::Display for Scope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Error returned by [`Scope`]'s [`std::str::FromStr`] impl when the input
/// does not match a known wire string.
#[derive(Debug, thiserror::Error)]
#[error("unsupported scope '{0}'")]
pub struct ParseScopeError(String);

impl FromStr for Scope {
    type Err = ParseScopeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "read" => Ok(Self::Read),
            "write" => Ok(Self::Write),
            "admin" => Ok(Self::Admin),
            other => Err(ParseScopeError(other.to_owned())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn as_str_matches_serde_lowercase() {
        for (variant, wire) in [
            (Scope::Read, "read"),
            (Scope::Write, "write"),
            (Scope::Admin, "admin"),
        ] {
            assert_eq!(variant.as_str(), wire);
            assert_eq!(format!("{variant}"), wire);
        }
    }

    #[test]
    fn json_roundtrip_uses_lowercase_string() {
        let scope = Scope::Write;
        let json = serde_json::to_string(&scope).expect("serialize");
        assert_eq!(json, "\"write\"");
        let back: Scope = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, Scope::Write);
    }

    #[test]
    fn json_rejects_unknown_variant() {
        let result: Result<Scope, _> = serde_json::from_str("\"superuser\"");
        assert!(result.is_err(), "expected superuser to be rejected");
    }

    #[test]
    fn from_str_rejects_unknown_variant() {
        assert!(Scope::from_str("superuser").is_err());
        assert!(Scope::from_str("READ").is_err()); // case sensitive
        assert_eq!(Scope::from_str("read").unwrap(), Scope::Read);
    }

    #[test]
    fn for_role_derives_expected_set() {
        assert_eq!(
            Scope::for_role(Role::Admin),
            &[Scope::Read, Scope::Write, Scope::Admin]
        );
        assert_eq!(Scope::for_role(Role::Adult), &[Scope::Read, Scope::Write]);
        assert_eq!(Scope::for_role(Role::Child), &[Scope::Read, Scope::Write]);
    }

    #[test]
    fn grantable_by_enforces_admin_ceiling() {
        assert!(Scope::Read.grantable_by(Role::Adult));
        assert!(Scope::Write.grantable_by(Role::Adult));
        assert!(!Scope::Admin.grantable_by(Role::Adult));
        assert!(!Scope::Admin.grantable_by(Role::Child));
        assert!(Scope::Admin.grantable_by(Role::Admin));
    }
}
