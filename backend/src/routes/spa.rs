//! SPA asset-serving router.
//!
//! Mounts `ServeDir` under `/assets/*` when a frontend dist directory is
//! configured. Returns `None` in API-only dev (when Vite serves the frontend).
//!
//! **Why not `fallback_service` for SPA index.html?** Axum 0.8 panics when
//! `.merge()` combines two routers that both carry a fallback. The composite
//! router in `main.rs` owns the single `.fallback(composite_fallback)` that
//! dispatches between JSON-404 for reserved-prefix misses and SPA
//! `index.html` for everything else. This router is therefore limited to
//! matched `/assets/*` requests.
//!
//! The rest of the dist tree does not live under `/assets`: Vite copies
//! `public/` to the dist root, so the fonts, the brand assets and the
//! favicons are served from paths this router never matches. Those requests
//! reach the composite fallback, which calls `try_dist_file` below before
//! deciding the path is an SPA route.

use std::path::Path;

use axum::Router;
use axum::body::Body;
use axum::http::Request;
use axum::response::Response;
use tower::ServiceExt as _;
use tower_http::services::ServeDir;

use crate::state::AppState;

/// Build the SPA asset-serving router for `dist_path`. Returns `None`
/// when no dist path is configured (API-only dev mode where Vite serves
/// the frontend directly).
pub fn router_enabled(dist_path: Option<&Path>) -> Option<Router<AppState>> {
    let dist = dist_path?;
    let assets_dir = dist.join("assets");
    Some(Router::new().nest_service("/assets", ServeDir::new(assets_dir)))
}

/// Serve `req` from the dist tree, or return `None` when it names no file
/// there.
///
/// `None` is the SPA-route answer: a deep link like `/library/anything`
/// matches no file and must receive `index.html` so the client router can
/// resolve it. Callers therefore cannot treat `None` as an error. 404 is
/// the only `ServeDir` status treated as "no such file"; every other
/// status is an answer about a real file and passes through unchanged,
/// including a 304 on a matching `If-None-Match`, a 412 on a failed
/// `If-Unmodified-Since`, a 416 on an unsatisfiable `Range`, and a 405 on
/// a non-GET, since RFC 9110 identifies the target resource by URI alone
/// and both non-reserved route classes serve static content that only
/// supports GET and HEAD.
///
/// Directory requests deliberately do not resolve to their `index.html`.
/// Without that, `/` would be answered from here rather than by the
/// caller's SPA path, splitting one response into two code paths that
/// would have to be kept in step.
///
/// THREAT: this widens byte-serving from `/assets` to the whole dist tree,
/// so path traversal is the risk that matters. `ServeDir` resolves the
/// request path against the root and rejects anything that escapes it,
/// including percent-encoded traversal, which is why the traversal defence
/// is delegated rather than hand-rolled here.
pub async fn try_dist_file(dist: &Path, req: Request<Body>) -> Option<Response> {
    let resp = ServeDir::new(dist)
        .append_index_html_on_directories(false)
        .oneshot(req)
        .await
        .ok()?;
    if resp.status() == axum::http::StatusCode::NOT_FOUND {
        None
    } else {
        Some(resp.map(Body::new))
    }
}
