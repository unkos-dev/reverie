//! SPA asset-serving router (UNK-106).
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

use std::path::Path;

use axum::Router;
use tower_http::services::ServeDir;

use crate::state::AppState;

pub fn router_enabled(dist_path: Option<&Path>) -> Option<Router<AppState>> {
    let dist = dist_path?;
    let assets_dir = dist.join("assets");
    Some(Router::new().nest_service("/assets", ServeDir::new(assets_dir)))
}
