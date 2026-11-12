//! OPDS catalogue configuration ([`OpdsConfig`]) and its validators.

use validator::{Validate, ValidationError};

/// OPDS catalog configuration. When `enabled`, `/opds/*` is mounted behind a
/// Basic-only extractor and the root `public_url` must be set: feeds emit
/// absolute URLs rooted at it.
///
/// Note: the dual-mounted cover handlers at `/api/v1/books/:id/cover{,/thumb}` are
/// mounted independently of `enabled` because the web UI needs them
/// regardless of OPDS availability.
#[derive(Debug, Clone, serde::Deserialize, schemars::JsonSchema, Validate)]
#[serde(default)]
pub struct OpdsConfig {
    /// Whether the `/opds/*` routes are mounted
    /// (`REVERIE_OPDS_ENABLED`, default `true`).
    pub enabled: bool,
    /// Default page size for paginated feeds (`REVERIE_OPDS_PAGE_SIZE`,
    /// default `50`; valid range 1-500).
    #[validate(range(min = 1, max = 500, message = "must be between 1 and 500"))]
    pub page_size: u32,
    /// `WWW-Authenticate: Basic realm=...` value emitted on 401
    /// responses from `/opds/*` (`REVERIE_OPDS_REALM`, default
    /// `"Reverie OPDS"`). Must be a valid HTTP header value without
    /// embedded quotes; backslashes are escaped in the challenge.
    #[validate(custom(function = "validate_realm"))]
    pub realm: String,
}

impl Default for OpdsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            page_size: 50,
            realm: "Reverie OPDS".into(),
        }
    }
}

// ---------------------------------------------------------------------------
// Declarative validators (validator 0.20). Range checks are field attributes;
// cross-field checks are struct-level `schema` functions. Cross-field errors
// carry the offending env-var name as a `"var"` param because the validator
// error tree keys struct-level failures under the opaque `"__all__"` key,
// which the reverse field→env-name map cannot resolve. Single-field checks
// (`validate_realm`) need no `"var"` param — their tree path reverse-maps
// natively (`opds.realm` → `REVERIE_OPDS_REALM`).
// ---------------------------------------------------------------------------

/// Validate the OPDS realm before it becomes a Basic challenge header.
///
/// THREAT: quotes can split the challenge and invalid header bytes can suppress it.
fn validate_realm(realm: &str) -> Result<(), ValidationError> {
    if realm.contains('"') {
        let mut e = ValidationError::new("realm_quote");
        e.message = Some("must not contain '\"'".into());
        return Err(e);
    }
    if axum::http::HeaderValue::from_str(realm).is_err() {
        let mut e = ValidationError::new("realm_header");
        e.message = Some("must be a valid HTTP header value".into());
        return Err(e);
    }
    Ok(())
}
