//! `IdentityProvider`: closed value set for the `user_identities.provider`
//! column, mapping the Postgres `identity_provider` ENUM.
//!
//! Mirrors [`crate::models::role::Role`]: a typed [`sqlx::Type`] wrapper so a
//! DB-side variant with no Rust counterpart fails decode loudly rather than
//! coercing into an unmatched string. The set is static (federated OIDC only);
//! local password credentials live in `local_credentials`, not here.
//!
//! Wire formats:
//! - Postgres: `identity_provider` ENUM type.
//! - JSON: lowercase string literal, "oidc".

/// Mechanism backing an external identity link on
/// [`crate::models::user_identities::UserIdentity`].
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
#[sqlx(type_name = "identity_provider", rename_all = "lowercase")]
pub enum IdentityProvider {
    /// `OpenID` Connect federated identity, keyed on `(issuer, subject)`.
    Oidc,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_roundtrip_uses_lowercase_string() {
        let provider = IdentityProvider::Oidc;
        let json = serde_json::to_string(&provider).expect("serialize");
        assert_eq!(json, "\"oidc\"");
        let back: IdentityProvider = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, IdentityProvider::Oidc);
    }

    #[test]
    fn json_rejects_unknown_variant() {
        let result: Result<IdentityProvider, _> = serde_json::from_str("\"saml\"");
        assert!(result.is_err(), "expected saml to be rejected");
    }
}
