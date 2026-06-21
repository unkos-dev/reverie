//! Response-header policy configuration ([`SecurityConfig`]): CSP, HSTS,
//! reporting endpoint, and the raw-string CSP-endpoint deserializer.

use validator::{Validate, ValidationError};

/// Response-header policy.
///
/// CSP values are stored as precomputed `HeaderValue`s. They depend on
/// `validate_frontend_dist` reading the on-disk FOUC script to derive its
/// hash, so `csp_api_header` and `csp_html_header` are left as `None` after
/// deserialization and finalised by `main()` via
/// [`crate::security::csp::build_api_csp`] /
/// [`crate::security::csp::build_html_csp`]. A non-conformant CSP string
/// panics in `main()` rather than silently dropping the header at request
/// time.
///
/// HSTS and Reporting-Endpoints are derived from the booleans / URL stored
/// here via [`Self::hsts_header_value`] and
/// [`Self::reporting_endpoints_header_value`]. Both compose static-ASCII
/// strings from validated inputs and panic on the impossible case (a
/// programming invariant has been violated and we want to know).
///
/// A `SecurityConfig` obtained directly from the config pipeline (without the
/// CSP-finalisation pass) emits no `Content-Security-Policy` on either
/// route class (both fields stay `None`); HSTS and Reporting-Endpoints
/// are still applied because they are derived on demand.
#[derive(Debug, Clone, Default, serde::Deserialize, schemars::JsonSchema, Validate)]
#[serde(default)]
#[validate(schema(function = "validate_security_config"))]
pub struct SecurityConfig {
    /// Whether the deployment is fronted by a TLS-terminating reverse
    /// proxy (`REVERIE_BEHIND_HTTPS`, default `false`). Gates HSTS
    /// emission: never emitted on plaintext HTTP because the browser
    /// would refuse the next TLS-less request to this host.
    pub behind_https: bool,
    /// Whether the HSTS header carries `; includeSubDomains`
    /// (`REVERIE_HSTS_INCLUDE_SUBDOMAINS`, default `false`). Requires
    /// `behind_https=true`.
    pub hsts_include_subdomains: bool,
    /// Whether the HSTS header carries `; preload`
    /// (`REVERIE_HSTS_PRELOAD`, default `false`). Requires
    /// `hsts_include_subdomains=true` (chrome.com / hstspreload.org
    /// rules).
    pub hsts_preload: bool,
    /// Optional CSP-violation reporting endpoint
    /// (`REVERIE_CSP_REPORT_ENDPOINT`). Pre-validated at startup to
    /// reject `"`/`;`/CR/LF (header-injection guard) and any scheme
    /// other than `http`/`https`.
    #[serde(default, deserialize_with = "de_csp_endpoint")]
    pub csp_report_endpoint: Option<url::Url>,
    /// Optional path to the built frontend dist directory
    /// (`REVERIE_FRONTEND_DIST_PATH`). When set, the SPA assets router
    /// is mounted and the FOUC-script hash is read at startup to seed
    /// the HTML CSP.
    pub frontend_dist_path: Option<std::path::PathBuf>,
    /// Precomputed `Content-Security-Policy` header for HTML
    /// responses. `None` after [`crate::config::Config::from_env`]; finalised by
    /// [`crate::run`] from the FOUC-script hash + reporting endpoint.
    #[serde(skip)]
    #[schemars(skip)]
    pub csp_html_header: Option<axum::http::HeaderValue>,
    /// Precomputed `Content-Security-Policy` header for API
    /// responses. `None` after [`crate::config::Config::from_env`]; finalised by
    /// [`crate::run`] from the reporting endpoint
    /// (`default-src 'none'`-rooted, no script-src hashes).
    #[serde(skip)]
    #[schemars(skip)]
    pub csp_api_header: Option<axum::http::HeaderValue>,
}

impl SecurityConfig {
    /// HSTS response-header value. `None` when `behind_https=false` — the
    /// middleware must not emit HSTS on plaintext HTTP or the browser would
    /// refuse to talk to the host on its next TLS-less request. The composed
    /// string is static ASCII; `from_str` panics on the impossible case so
    /// any future composition bug surfaces loudly instead of silently
    /// dropping the header.
    pub fn hsts_header_value(&self) -> Option<axum::http::HeaderValue> {
        if !self.behind_https {
            return None;
        }
        let mut v = String::from("max-age=31536000");
        if self.hsts_include_subdomains {
            v.push_str("; includeSubDomains");
        }
        if self.hsts_preload {
            v.push_str("; preload");
        }
        Some(axum::http::HeaderValue::from_str(&v).unwrap_or_else(|e| {
            panic!("HSTS string is not a valid HTTP header value ({e}): {v:?}")
        }))
    }

    /// `Reporting-Endpoints: csp-endpoint="<url>"`. `None` when
    /// `csp_report_endpoint` is unset. The URL was validated at deserialize
    /// time by `de_csp_endpoint` (no `"` `;` CR or LF; valid `url::Url`); `as_str()`
    /// returns the canonical percent-encoded form. `from_str` panics on the
    /// impossible case rather than silently dropping the header.
    pub fn reporting_endpoints_header_value(&self) -> Option<axum::http::HeaderValue> {
        let url = self.csp_report_endpoint.as_ref()?;
        let v = format!("csp-endpoint=\"{}\"", url.as_str());
        Some(axum::http::HeaderValue::from_str(&v).unwrap_or_else(|e| {
            panic!("Reporting-Endpoints value is not a valid HTTP header value ({e}): {v:?}")
        }))
    }
}

/// HSTS precondition ladder (chrome.com / hstspreload.org rules): subdomains
/// requires HTTPS; preload requires subdomains. Never emit HSTS on plaintext.
///
/// THREAT: HSTS pins a host to HTTPS in the browser for `max-age`. Emitting it
/// (or `includeSubDomains`/`preload`) on a plaintext-reachable deployment would
/// brick the host — the browser refuses the next TLS-less request and there is
/// no in-band recovery. This ladder rejects those combinations at config-load
/// time rather than letting a misconfiguration self-DoS the instance.
fn validate_security_config(sec: &SecurityConfig) -> Result<(), ValidationError> {
    if sec.hsts_include_subdomains && !sec.behind_https {
        let mut e = ValidationError::new("hsts_subdomains_requires_https");
        e.add_param("var".into(), &"REVERIE_HSTS_INCLUDE_SUBDOMAINS");
        e.message = Some("requires REVERIE_BEHIND_HTTPS=true".into());
        return Err(e);
    }
    if sec.hsts_preload && !sec.hsts_include_subdomains {
        let mut e = ValidationError::new("hsts_preload_requires_subdomains");
        e.add_param("var".into(), &"REVERIE_HSTS_PRELOAD");
        e.message = Some("requires REVERIE_HSTS_INCLUDE_SUBDOMAINS=true".into());
        return Err(e);
    }
    Ok(())
}

/// Deserialize `REVERIE_CSP_REPORT_ENDPOINT` into `Option<url::Url>` with the
/// header-injection guard applied to the RAW string BEFORE `url::Url::parse`.
///
/// THREAT: this URL is emitted into the `Reporting-Endpoints` response header.
/// `url::Url::parse` percent-encodes (`"` → `%22`) and rejects CR/LF with a
/// generic message, so a guard placed on the parsed `Url` (or a
/// `#[validate(custom)]` on the typed value) would silently pass quote
/// injection and surface the wrong error for CR/LF. The raw-string char guard
/// therefore runs first, then the scheme allowlist — order mirrors the former
/// imperative path exactly. See `adr/2026-06-09-declarative-config-stack.md`
/// (GOTCHA-CSPRAW) and the `security_report_endpoint_injection_chars_errors`
/// test (3 forms).
fn de_csp_endpoint<'de, D>(de: D) -> Result<Option<url::Url>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = <String as serde::Deserialize>::deserialize(de)?;
    if s.is_empty() {
        return Ok(None);
    }
    // THREAT: reject quote / semicolon / CR / LF on the raw string — these
    // would split or escape the response header value (header injection).
    if s.chars().any(|c| matches!(c, '"' | ';' | '\r' | '\n')) {
        return Err(serde::de::Error::custom("must not contain \" ; CR or LF"));
    }
    let parsed = url::Url::parse(&s).map_err(serde::de::Error::custom)?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(serde::de::Error::custom(format!(
            "scheme must be http or https, got '{}'",
            parsed.scheme()
        )));
    }
    Ok(Some(parsed))
}
