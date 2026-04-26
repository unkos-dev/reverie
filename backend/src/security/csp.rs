//! Pure Content-Security-Policy builders.
//!
//! Called once at startup from `main()` to precompute the `csp_html_header`
//! and `csp_api_header` strings stored on [`crate::config::SecurityConfig`].

/// HTML CSP (per-response value for `text/html` responses). Must allow the
/// Vite-built ES module (`/assets/*.js`) and one known inline FOUC script
/// (pinned by `script_src_hashes`). `style-src 'unsafe-inline'` is a
/// pragmatic concession for Tailwind CSS JIT + Radix portals — documented
/// in `docs/security/content-security-policy.md`.
///
/// Invariant: `script_src_hashes` must be non-empty for production; the
/// dist-validation step rejects an empty sidecar before this builder runs.
/// Each element must be pre-formatted as `sha256-...` / `sha384-...` /
/// `sha512-...` with standard (not base64url) base64 — dist validation
/// enforces the shape.
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
    out.push_str("; font-src 'self' https://cdn.fontshare.com");
    out.push_str("; connect-src 'self'");
    out.push_str("; frame-ancestors 'none'");
    out.push_str("; base-uri 'self'");
    out.push_str("; form-action 'self'");
    out.push_str("; object-src 'none'");
    out.push_str("; upgrade-insecure-requests");
    append_reporting(&mut out, report_endpoint);
    out
}

/// API CSP (per-response value for `application/json` / `application/xml`
/// responses from `/api`, `/auth`, `/health`, `/opds`). Locks every directive
/// to `'none'` — API responses never render; any script / image / frame
/// execution against them is anomalous.
pub fn build_api_csp(report_endpoint: Option<&url::Url>) -> String {
    let mut out = String::from("default-src 'none'; frame-ancestors 'none'; base-uri 'none'");
    append_reporting(&mut out, report_endpoint);
    out
}

fn append_reporting(out: &mut String, report_endpoint: Option<&url::Url>) {
    if let Some(url) = report_endpoint {
        // The URL passed the header-injection guard in SecurityConfig::from_env
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
             font-src 'self' https://cdn.fontshare.com; connect-src 'self'; \
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
}
