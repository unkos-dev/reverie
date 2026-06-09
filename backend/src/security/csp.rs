//! Pure Content-Security-Policy builders for Reverie.
//!
//! Called once at startup from `reverie_api::run` to precompute the
//! `csp_html_header` and `csp_api_header` `HeaderValue`s stored on
//! [`crate::config::SecurityConfig`]. Kept pure (no I/O, no state) so the
//! shape of the policy is auditable in one file.
//!
//! # Tier 2 — security-critical
//!
//! These builders define what scripts the browser will execute on a Reverie
//! instance. Drift between the HTML CSP and the inline-script hashes shipped
//! by the Vite plugin (`frontend/vite-plugins/csp-hash.ts`) silently breaks
//! script execution; drift between the API CSP and the response classes that
//! attach it weakens the route-class differentiation that motivates having
//! two policies. The startup `dist_validation` step closes the first drift
//! channel; reviewer discipline is the only check on the second.

/// Build the HTML CSP — the per-response policy attached to `text/html`
/// responses (SPA fallback + `/assets/*`).
///
/// Allowed surfaces:
/// - `default-src 'self'` — baseline for any directive not set explicitly
///   below.
/// - `script-src 'self'` plus the inline-script hashes the Vite
///   `reverie-csp-hash` plugin extracts at build time (the current source
///   is the FOUC theme bootstrap in `frontend/src/fouc/fouc.js`).
/// - `style-src 'self' 'unsafe-inline'` — pragmatic concession for Tailwind
///   CSS JIT + Radix UI portals that emit style attributes at runtime. The
///   risk surface is XSS-style-attribute injection; mitigations are
///   covered in `docs/security/content-security-policy.md`.
/// - `img-src 'self' data:` — `data:` permits the inline blur-up
///   placeholders on cover images.
/// - `font-src 'self'` — self-hosted variable woff2 files under
///   `frontend/public/fonts/`. Operators wanting CDN fonts must edit
///   this builder (no runtime knob — see `docs/security/content-security-policy.md`
///   § Fonts).
/// - `connect-src 'self'` — `fetch` / `XMLHttpRequest` / WebSocket
///   targets restricted to same-origin (the SPA's own `/api`, `/auth`,
///   `/opds` calls).
/// - `frame-ancestors 'none'`, `base-uri 'self'`, `form-action 'self'`,
///   `object-src 'none'` — clickjacking, base-tag-injection, and plugin
///   embedding all denied.
/// - `upgrade-insecure-requests` — promotes mixed-content to HTTPS where
///   possible.
///
/// Threat: `'unsafe-inline'` on `style-src` is the one explicit attack
/// surface this builder accepts. Any future move to nonce-based or
/// hash-based style-src would be a strict improvement.
///
/// # Invariants
///
/// `script_src_hashes` must be non-empty for production; the dist-validation
/// step rejects an empty sidecar before this builder runs. Each element must
/// be pre-formatted as `sha256-...` / `sha384-...` / `sha512-...` with
/// standard (RFC 4648 §4) base64 — dist validation enforces the shape.
pub fn build_html_csp(script_src_hashes: &[String], report_endpoint: Option<&url::Url>) -> String {
    let mut script_src = String::from("script-src 'self'");
    for h in script_src_hashes {
        script_src.push_str(" '");
        script_src.push_str(h);
        script_src.push('\'');
    }

    let mut out = String::with_capacity(512);
    out.push_str("default-src 'self'; ");
    out.push_str(&script_src);
    out.push_str("; style-src 'self' 'unsafe-inline'");
    out.push_str("; img-src 'self' data:");
    out.push_str("; font-src 'self'");
    out.push_str("; connect-src 'self'");
    out.push_str("; frame-ancestors 'none'");
    out.push_str("; base-uri 'self'");
    out.push_str("; form-action 'self'");
    out.push_str("; object-src 'none'");
    out.push_str("; upgrade-insecure-requests");
    append_reporting(&mut out, report_endpoint);
    out
}

/// Build the API CSP — the per-response policy attached to
/// `application/json` / `application/xml` responses on `/api`, `/auth`,
/// `/health`, `/opds`.
///
/// Sets `default-src 'none'` (which covers all inheriting fetch
/// directives — `script-src`, `img-src`, `connect-src`, etc.) plus
/// explicit `frame-ancestors 'none'` and `base-uri 'none'` for the two
/// directives that do not inherit from `default-src`. API responses
/// never render in a document context; any script execution, image
/// fetch, or frame embedding against them is anomalous and the policy
/// reports it via `report-to` / `report-uri` when configured.
///
/// Threat: a single shared CSP across HTML and API responses would force
/// the laxer HTML policy onto API responses, broadening the implicit attack
/// surface to data-only endpoints. Route-class differentiation prevents
/// that.
pub fn build_api_csp(report_endpoint: Option<&url::Url>) -> String {
    let mut out = String::from("default-src 'none'; frame-ancestors 'none'; base-uri 'none'");
    append_reporting(&mut out, report_endpoint);
    out
}

fn append_reporting(out: &mut String, report_endpoint: Option<&url::Url>) {
    if let Some(url) = report_endpoint {
        // The URL passed the header-injection guard in `de_csp_endpoint`
        // (no `"` `;` CR or LF); `as_str()` renders the canonical form.
        out.push_str("; report-to csp-endpoint");
        out.push_str("; report-uri ");
        out.push_str(url.as_str());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(s: &str) -> Vec<String> {
        vec![s.to_owned()]
    }

    fn url(s: &str) -> url::Url {
        url::Url::parse(s).unwrap()
    }

    #[test]
    fn html_one_hash_no_reporting() {
        let got = build_html_csp(&h("sha256-ABCD"), None);
        assert_eq!(
            got,
            "default-src 'self'; script-src 'self' 'sha256-ABCD'; \
             style-src 'self' 'unsafe-inline'; img-src 'self' data:; \
             font-src 'self'; connect-src 'self'; \
             frame-ancestors 'none'; base-uri 'self'; form-action 'self'; \
             object-src 'none'; upgrade-insecure-requests"
                .replace("             ", "")
        );
    }

    #[test]
    fn html_three_hashes_no_reporting() {
        let hashes = vec![
            "sha256-AAAA".to_owned(),
            "sha256-BBBB".to_owned(),
            "sha384-CCCC".to_owned(),
        ];
        let got = build_html_csp(&hashes, None);
        assert!(
            got.contains("script-src 'self' 'sha256-AAAA' 'sha256-BBBB' 'sha384-CCCC';"),
            "unexpected: {got}"
        );
    }

    #[test]
    fn html_with_reporting() {
        let got = build_html_csp(&h("sha256-ABCD"), Some(&url("https://log.example/csp")));
        assert!(got.ends_with("; report-to csp-endpoint; report-uri https://log.example/csp"));
    }

    #[test]
    fn api_without_reporting() {
        assert_eq!(
            build_api_csp(None),
            "default-src 'none'; frame-ancestors 'none'; base-uri 'none'"
        );
    }

    #[test]
    fn api_with_reporting() {
        let got = build_api_csp(Some(&url("https://log.example/csp")));
        assert_eq!(
            got,
            "default-src 'none'; frame-ancestors 'none'; base-uri 'none'; \
             report-to csp-endpoint; report-uri https://log.example/csp"
                .replace("             ", "")
        );
    }

    #[test]
    fn builder_outputs_are_valid_header_values() {
        // Locks in the startup contract: main() converts these strings into
        // axum HeaderValue with .unwrap_or_else(panic). If a future builder
        // change introduces a byte outside the HTTP visible-ASCII range this
        // test catches it before production startup does.
        let report = url("https://log.example/csp");
        let cases: &[(&str, String)] = &[
            ("build_api_csp(None)", build_api_csp(None)),
            ("build_api_csp(Some)", build_api_csp(Some(&report))),
            (
                "build_html_csp(hashes, None)",
                build_html_csp(&h("sha256-YWJjZA=="), None),
            ),
            (
                "build_html_csp(hashes, Some)",
                build_html_csp(&h("sha256-YWJjZA=="), Some(&report)),
            ),
        ];
        for (label, value) in cases {
            axum::http::HeaderValue::from_str(value).unwrap_or_else(|e| {
                panic!("{label} produced invalid HTTP header value ({e}): {value:?}")
            });
        }
    }
}
