//! Response-header middleware wiring (UNK-106).
//!
//! Three concerns, three surfaces:
//! - [`security_headers`] — outermost uniform middleware. Sets XCTO,
//!   Referrer-Policy, Permissions-Policy, X-Frame-Options, and (conditional)
//!   HSTS + Reporting-Endpoints. Applied on the composite router.
//! - [`api_csp_layer`] / [`html_csp_layer`] — per-router middleware that
//!   writes `Content-Security-Policy` from the precomputed string on
//!   `SecurityConfig`. Attached to matched routes only.
//! - [`composite_fallback`] — the single fallback handler for the composite.
//!   Path-prefix-dispatches to either a JSON 404 + API CSP (reserved
//!   prefixes) or an `index.html` + HTML CSP (SPA fallback). Neither the
//!   per-router CSP layers nor any other middleware attaches CSP to fallback
//!   responses, so the handler sets CSP itself.

use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{HeaderName, HeaderValue, StatusCode, Uri, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use crate::state::AppState;

/// Baseline permissions policy — denies every high-risk browser capability.
/// Adjust only when a specific feature lands that legitimately needs one of
/// these (and document the exception in `docs/security/content-security-policy.md`).
const PERMISSIONS_POLICY_VALUE: &str = "camera=(), microphone=(), geolocation=(), \
     payment=(), usb=(), midi=(), magnetometer=(), accelerometer=(), gyroscope=()";

const RESERVED_PREFIXES: &[&str] = &["/api", "/auth", "/health", "/opds"];

/// Uniform security headers middleware — applies to every response from the
/// composite router (including the composite fallback).
pub async fn security_headers(
    State(state): State<AppState>,
    req: Request<Body>,
    next: Next,
) -> Response {
    let mut resp = next.run(req).await;
    let headers = resp.headers_mut();

    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    headers.insert(
        HeaderName::from_static("permissions-policy"),
        HeaderValue::from_static(PERMISSIONS_POLICY_VALUE),
    );
    headers.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));

    if let Some(hsts) = state.config.security.hsts_header_value() {
        headers.insert(header::STRICT_TRANSPORT_SECURITY, hsts);
    }
    if let Some(re) = state.config.security.reporting_endpoints_header_value() {
        headers.insert(HeaderName::from_static("reporting-endpoints"), re);
    }
    resp
}

/// Sets `Content-Security-Policy` to the API CSP for all responses from the
/// API-like router (matched routes under `/api`, `/auth`, `/health`, `/opds`).
/// Unmatched responses flow through the composite fallback which attaches the
/// correct CSP manually — this layer does not see them.
pub async fn api_csp_layer(
    State(state): State<AppState>,
    req: Request<Body>,
    next: Next,
) -> Response {
    let mut resp = next.run(req).await;
    if let Ok(v) = HeaderValue::from_str(&state.config.security.csp_api_header) {
        resp.headers_mut()
            .insert(header::CONTENT_SECURITY_POLICY, v);
    }
    resp
}

/// Sets `Content-Security-Policy` to the HTML CSP on responses from the
/// matched `/assets/*` routes. Does NOT cover SPA `index.html` responses —
/// those come from the composite fallback, which attaches HTML CSP directly
/// via [`attach_html_csp`]. When the HTML CSP is not configured (API-only
/// dev runs), no header is written — dev mode relies on Vite's own
/// `server.headers` block.
pub async fn html_csp_layer(
    State(state): State<AppState>,
    req: Request<Body>,
    next: Next,
) -> Response {
    let mut resp = next.run(req).await;
    if let Some(h) = state.config.security.csp_html_header.as_deref()
        && let Ok(v) = HeaderValue::from_str(h)
    {
        resp.headers_mut()
            .insert(header::CONTENT_SECURITY_POLICY, v);
    }
    resp
}

/// Composite-router fallback. Axum 0.8 panics when `.merge()` combines two
/// routers each carrying a fallback, so this is the single fallback on the
/// merged composite. It mirrors what the per-router CSP layers would have
/// set: API CSP for reserved-prefix 404s, HTML CSP for SPA deep-links.
pub async fn composite_fallback(State(state): State<AppState>, uri: Uri) -> Response {
    if is_reserved_prefix(uri.path()) {
        api_404_with_csp(&state)
    } else {
        spa_fallback_response(&state).await
    }
}

/// 404 JSON + API CSP. Mirrors [`crate::error::AppError::NotFound`] shape
/// (`{"error":"not found"}`). Written here instead of reusing `AppError` so
/// the CSP header can be attached without extra layering.
pub fn api_404_with_csp(state: &AppState) -> Response {
    let mut resp = (
        StatusCode::NOT_FOUND,
        axum::Json(serde_json::json!({"error": "not found"})),
    )
        .into_response();
    attach_api_csp(&mut resp, state);
    resp
}

/// SPA index.html + HTML CSP. When `frontend_dist_path` is unset (API-only
/// dev), falls through to a plain 404 with no CSP. On unexpected I/O failure
/// (disk gone, permissions broken) also plain-404 — the operator has bigger
/// problems and the failure is logged at `warn` to keep visibility.
async fn spa_fallback_response(state: &AppState) -> Response {
    let Some(dist) = state.config.security.frontend_dist_path.as_ref() else {
        return plain_404();
    };
    let index = dist.join("index.html");
    let bytes = match tokio::fs::read(&index).await {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!(
                path = %index.display(),
                error = %e,
                "SPA fallback: index.html read failed",
            );
            return plain_404();
        }
    };
    let mut resp = (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        bytes,
    )
        .into_response();
    attach_html_csp(&mut resp, state);
    resp
}

fn plain_404() -> Response {
    (StatusCode::NOT_FOUND, "not found").into_response()
}

fn attach_api_csp(resp: &mut Response, state: &AppState) {
    if let Ok(v) = HeaderValue::from_str(&state.config.security.csp_api_header) {
        resp.headers_mut()
            .insert(header::CONTENT_SECURITY_POLICY, v);
    }
}

fn attach_html_csp(resp: &mut Response, state: &AppState) {
    if let Some(h) = state.config.security.csp_html_header.as_deref()
        && let Ok(v) = HeaderValue::from_str(h)
    {
        resp.headers_mut()
            .insert(header::CONTENT_SECURITY_POLICY, v);
    }
}

fn is_reserved_prefix(path: &str) -> bool {
    for p in RESERVED_PREFIXES {
        if path == *p {
            return true;
        }
        if let Some(rest) = path.strip_prefix(p)
            && rest.starts_with('/')
        {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_reserved_prefix_matches_bare_and_subpaths() {
        assert!(is_reserved_prefix("/api"));
        assert!(is_reserved_prefix("/api/"));
        assert!(is_reserved_prefix("/api/books"));
        assert!(is_reserved_prefix("/api/books/9999/covr"));
        assert!(is_reserved_prefix("/auth"));
        assert!(is_reserved_prefix("/auth/callback"));
        assert!(is_reserved_prefix("/health"));
        assert!(is_reserved_prefix("/health/ready"));
        assert!(is_reserved_prefix("/opds"));
        assert!(is_reserved_prefix("/opds/library"));
    }

    #[test]
    fn is_reserved_prefix_rejects_spa_paths() {
        assert!(!is_reserved_prefix("/"));
        assert!(!is_reserved_prefix("/library"));
        assert!(!is_reserved_prefix("/library/book/1"));
        assert!(!is_reserved_prefix("/settings"));
        assert!(!is_reserved_prefix("/apis-nothing-to-see-here")); // not `/api` prefix
        assert!(!is_reserved_prefix("/apiology")); // not `/api/`
        assert!(!is_reserved_prefix("/authed")); // not `/auth/`
    }

    #[test]
    fn permissions_policy_value_contains_sensitive_features() {
        for cap in ["camera", "microphone", "geolocation", "payment"] {
            assert!(
                PERMISSIONS_POLICY_VALUE.contains(cap),
                "permissions policy missing '{cap}'"
            );
        }
    }

    // ---------- Integration tests (UNK-106 Tasks 13 + 14) ----------
    //
    // These exercise the full composite router via `test_support::test_server()`
    // and a sibling `test_server_with_security()` helper that injects a custom
    // `SecurityConfig`. No DB is required for any of these — they hit /health,
    // /api/__nope__, and SPA paths.
    use crate::auth::backend::AuthBackend;
    use crate::build_router;
    use crate::config::SecurityConfig;
    use crate::test_support;
    use axum_test::TestServer;
    use std::fs;
    use tempfile::TempDir;

    /// Build a TestServer with `security` replacing the defaults in
    /// `test_config()`. The `csp_api_header` / `csp_html_header` strings are
    /// caller-responsibility — simulate what `main()` would compute.
    fn test_server_with_security(security: SecurityConfig) -> TestServer {
        let mut config = test_support::test_config();
        config.security = security;
        let state = crate::state::AppState {
            pool: sqlx::PgPool::connect_lazy("postgres://invalid").unwrap(),
            ingestion_pool: sqlx::PgPool::connect_lazy("postgres://invalid").unwrap(),
            config,
            oidc_client: test_support::test_oidc_client(),
        };
        let auth_backend = AuthBackend {
            pool: state.pool.clone(),
        };
        TestServer::new(build_router(state, auth_backend))
    }

    /// Materialise a minimal dist/ tree in a TempDir: `index.html`,
    /// `csp-hashes.json`, `assets/`. Used by SPA-serving tests.
    fn fixture_dist(index_html_body: &[u8]) -> TempDir {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("index.html"), index_html_body).unwrap();
        fs::create_dir_all(tmp.path().join("assets")).unwrap();
        fs::write(tmp.path().join("assets").join("main.js"), b"// placeholder").unwrap();
        fs::write(
            tmp.path().join("csp-hashes.json"),
            br#"{"script-src-hashes":["sha256-AAAA"]}"#,
        )
        .unwrap();
        tmp
    }

    // --- Uniform headers ---

    #[tokio::test]
    async fn health_has_uniform_headers() {
        let server = test_support::test_server();
        let r = server.get("/health").await;
        r.assert_status_ok();
        assert_eq!(
            r.headers().get("x-content-type-options").unwrap(),
            "nosniff"
        );
        assert_eq!(r.headers().get("referrer-policy").unwrap(), "no-referrer");
        assert_eq!(r.headers().get("x-frame-options").unwrap(), "DENY");
        assert!(
            r.headers()
                .get("permissions-policy")
                .unwrap()
                .to_str()
                .unwrap()
                .contains("camera=()")
        );
        // HSTS absent by default (behind_https = false in test_config).
        assert!(r.headers().get("strict-transport-security").is_none());
        // Reporting-Endpoints absent by default (no csp_report_endpoint).
        assert!(r.headers().get("reporting-endpoints").is_none());
    }

    // --- API CSP ---

    #[tokio::test]
    async fn matched_api_route_has_api_csp() {
        let mut security = crate::test_support::test_config().security;
        security.csp_api_header =
            "default-src 'none'; frame-ancestors 'none'; base-uri 'none'".into();
        let server = test_server_with_security(security);
        let r = server.get("/health").await;
        r.assert_status_ok();
        let csp = r
            .headers()
            .get("content-security-policy")
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned();
        assert!(csp.contains("default-src 'none'"), "unexpected CSP: {csp}");
        assert!(csp.contains("frame-ancestors 'none'"));
    }

    // --- Composite fallback: reserved-prefix 404 JSON + API CSP ---

    #[tokio::test]
    async fn api_typo_returns_json_404_with_api_csp() {
        let mut security = crate::test_support::test_config().security;
        security.csp_api_header =
            "default-src 'none'; frame-ancestors 'none'; base-uri 'none'".into();
        let server = test_server_with_security(security);
        let r = server.get("/api/__nope__").await;
        r.assert_status(axum::http::StatusCode::NOT_FOUND);
        let body = r.text();
        assert!(
            body.contains("\"error\""),
            "expected json error body, got: {body}"
        );
        assert!(body.contains("not found"));
        let csp = r
            .headers()
            .get("content-security-policy")
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned();
        assert!(csp.contains("default-src 'none'"));
    }

    #[tokio::test]
    async fn deep_api_typo_returns_404_with_api_csp() {
        let server = test_server_with_security(SecurityConfig {
            csp_api_header: "default-src 'none'; frame-ancestors 'none'; base-uri 'none'".into(),
            ..crate::test_support::test_config().security
        });
        let r = server.get("/api/books/9999/covr").await;
        r.assert_status(axum::http::StatusCode::NOT_FOUND);
        assert!(
            r.headers()
                .get("content-security-policy")
                .unwrap()
                .to_str()
                .unwrap()
                .contains("default-src 'none'"),
        );
    }

    #[tokio::test]
    async fn auth_typo_returns_json_404_with_api_csp() {
        let server = test_server_with_security(SecurityConfig {
            csp_api_header: "default-src 'none'; frame-ancestors 'none'; base-uri 'none'".into(),
            ..crate::test_support::test_config().security
        });
        let r = server.get("/auth/__nope__").await;
        r.assert_status(axum::http::StatusCode::NOT_FOUND);
        assert!(r.text().contains("not found"));
    }

    #[tokio::test]
    async fn health_typo_returns_json_404_with_api_csp() {
        let server = test_server_with_security(SecurityConfig {
            csp_api_header: "default-src 'none'; frame-ancestors 'none'; base-uri 'none'".into(),
            ..crate::test_support::test_config().security
        });
        let r = server.get("/health/__nope__").await;
        r.assert_status(axum::http::StatusCode::NOT_FOUND);
        assert!(r.text().contains("not found"));
    }

    #[tokio::test]
    async fn opds_typo_returns_json_404_with_api_csp() {
        // OPDS is disabled in test_config, so /opds/* doesn't match a route;
        // it must fall through to the composite fallback and come back as
        // reserved-prefix JSON 404, NOT SPA html.
        let server = test_server_with_security(SecurityConfig {
            csp_api_header: "default-src 'none'; frame-ancestors 'none'; base-uri 'none'".into(),
            ..crate::test_support::test_config().security
        });
        let r = server.get("/opds/__nope__").await;
        r.assert_status(axum::http::StatusCode::NOT_FOUND);
        assert!(r.text().contains("not found"));
        let csp = r
            .headers()
            .get("content-security-policy")
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned();
        assert!(csp.contains("default-src 'none'"));
    }

    // --- Composite fallback: SPA index.html + HTML CSP ---

    #[tokio::test]
    async fn spa_deep_link_serves_index_html_with_html_csp() {
        let html = b"<!doctype html><title>fixture</title>";
        let dist = fixture_dist(html);
        let security = SecurityConfig {
            frontend_dist_path: Some(dist.path().to_path_buf()),
            csp_html_header: Some("default-src 'self'; script-src 'self' 'sha256-AAAA'".into()),
            csp_api_header: "default-src 'none'".into(),
            ..crate::test_support::test_config().security
        };
        let server = test_server_with_security(security);
        let r = server.get("/library/anything").await;
        r.assert_status_ok();
        assert_eq!(
            r.headers().get("content-type").unwrap(),
            "text/html; charset=utf-8"
        );
        let body = r.as_bytes();
        assert_eq!(body.as_ref(), html.as_ref());
        let csp = r
            .headers()
            .get("content-security-policy")
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned();
        assert!(csp.contains("'sha256-AAAA'"), "unexpected CSP: {csp}");
    }

    #[tokio::test]
    async fn root_serves_index_html_with_html_csp() {
        let html = b"<!doctype html><title>root</title>";
        let dist = fixture_dist(html);
        let server = test_server_with_security(SecurityConfig {
            frontend_dist_path: Some(dist.path().to_path_buf()),
            csp_html_header: Some("default-src 'self'".into()),
            csp_api_header: "default-src 'none'".into(),
            ..crate::test_support::test_config().security
        });
        let r = server.get("/").await;
        r.assert_status_ok();
        assert_eq!(r.as_bytes().as_ref(), html.as_ref());
    }

    #[tokio::test]
    async fn assets_served_with_html_csp() {
        let dist = fixture_dist(b"<!doctype html>");
        let server = test_server_with_security(SecurityConfig {
            frontend_dist_path: Some(dist.path().to_path_buf()),
            csp_html_header: Some("default-src 'self'".into()),
            csp_api_header: "default-src 'none'".into(),
            ..crate::test_support::test_config().security
        });
        let r = server.get("/assets/main.js").await;
        r.assert_status_ok();
        let csp = r
            .headers()
            .get("content-security-policy")
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned();
        assert!(csp.contains("default-src 'self'"), "unexpected: {csp}");
    }

    #[tokio::test]
    async fn spa_fallback_without_dist_returns_plain_404() {
        // No frontend_dist_path — SPA mount is skipped, unmatched path hits
        // the composite fallback which has nothing to serve.
        let server = test_support::test_server();
        let r = server.get("/library/anything").await;
        r.assert_status(axum::http::StatusCode::NOT_FOUND);
        // Uniform headers still apply.
        assert_eq!(
            r.headers().get("x-content-type-options").unwrap(),
            "nosniff"
        );
    }

    // --- HSTS composition ---

    #[tokio::test]
    async fn hsts_absent_when_plaintext() {
        let server = test_support::test_server();
        let r = server.get("/health").await;
        assert!(r.headers().get("strict-transport-security").is_none());
    }

    #[tokio::test]
    async fn hsts_present_behind_https_base_value() {
        let server = test_server_with_security(SecurityConfig {
            behind_https: true,
            ..crate::test_support::test_config().security
        });
        let r = server.get("/health").await;
        assert_eq!(
            r.headers().get("strict-transport-security").unwrap(),
            "max-age=31536000"
        );
    }

    #[tokio::test]
    async fn hsts_subdomains_includes_directive() {
        let server = test_server_with_security(SecurityConfig {
            behind_https: true,
            hsts_include_subdomains: true,
            ..crate::test_support::test_config().security
        });
        let r = server.get("/health").await;
        assert_eq!(
            r.headers().get("strict-transport-security").unwrap(),
            "max-age=31536000; includeSubDomains"
        );
    }

    #[tokio::test]
    async fn hsts_preload_stack_full() {
        let server = test_server_with_security(SecurityConfig {
            behind_https: true,
            hsts_include_subdomains: true,
            hsts_preload: true,
            ..crate::test_support::test_config().security
        });
        let r = server.get("/health").await;
        assert_eq!(
            r.headers().get("strict-transport-security").unwrap(),
            "max-age=31536000; includeSubDomains; preload"
        );
    }

    // --- Reporting-Endpoints ---

    #[tokio::test]
    async fn reporting_endpoints_emits_when_configured() {
        let server = test_server_with_security(SecurityConfig {
            csp_report_endpoint: Some(url::Url::parse("https://log.example/csp").unwrap()),
            ..crate::test_support::test_config().security
        });
        let r = server.get("/health").await;
        assert_eq!(
            r.headers().get("reporting-endpoints").unwrap(),
            r#"csp-endpoint="https://log.example/csp""#
        );
    }
}
