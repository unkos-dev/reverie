//! OPDS catalogue configuration ([`OpdsConfig`]) and its validators.

use validator::{Validate, ValidationError};

/// OPDS catalog configuration. When `enabled`, `/opds/*` is mounted behind a
/// Basic-only extractor and `public_url` must be set — feeds emit absolute URLs
/// rooted at `public_url`.
///
/// Note: the dual-mounted cover handlers at `/api/v1/books/:id/cover{,/thumb}` are
/// mounted independently of `enabled` because the web UI (Step 10) needs them
/// regardless of OPDS availability.
#[derive(Debug, Clone, serde::Deserialize, schemars::JsonSchema, Validate)]
#[serde(default)]
#[validate(schema(function = "validate_opds_config"))]
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
    /// `"Reverie OPDS"`). Validated to exclude `"` to keep the header
    /// well-formed.
    #[validate(custom(function = "validate_realm"))]
    pub realm: String,
    /// Externally-visible base URL the catalogue emits absolute links
    /// rooted at (`REVERIE_PUBLIC_URL`). Required when `enabled=true`;
    /// optional otherwise.
    pub public_url: Option<url::Url>,
}

impl Default for OpdsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            page_size: 50,
            realm: "Reverie OPDS".into(),
            public_url: None,
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

/// Reject a `"` in the OPDS `realm` — it flows into a `WWW-Authenticate: Basic
/// realm="…"` header and an embedded quote would split the value.
///
/// THREAT: `realm` is emitted into the `WWW-Authenticate` response header on
/// 401s from `/opds/*`; an embedded `"` terminates the quoted value early and
/// opens header-value injection. Rejected here at config-load time.
fn validate_realm(realm: &str) -> Result<(), ValidationError> {
    if realm.contains('"') {
        let mut e = ValidationError::new("realm_quote");
        e.message = Some("must not contain '\"'".into());
        return Err(e);
    }
    Ok(())
}

/// OPDS cross-field rule: when the catalogue is enabled it emits absolute URLs
/// rooted at `public_url`, so `public_url` is required.
fn validate_opds_config(opds: &OpdsConfig) -> Result<(), ValidationError> {
    if opds.enabled && opds.public_url.is_none() {
        let mut e = ValidationError::new("public_url_required");
        e.add_param("var".into(), &"REVERIE_PUBLIC_URL");
        e.message = Some("required when REVERIE_OPDS_ENABLED=true".into());
        return Err(e);
    }
    Ok(())
}
