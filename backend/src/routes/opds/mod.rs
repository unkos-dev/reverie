//! OPDS 1.2 catalog routes. Mount under `/opds/*` with a Basic-only
//! extractor (RFC 7617), plus a dual-mount cover handler at
//! `/api/books/:id/cover{,/thumb}` behind cookie-or-Basic for the web UI.
//!
//! Scope is URL-based: pair a device at `/opds/library/*` to see the whole
//! library (further filtered by child-account RLS) or at
//! `/opds/shelves/{id}/*` to see only that shelf.

pub mod cursor;
pub mod feed;
pub mod scope;
pub mod xml;

// Filled in during Phases D–G.
pub mod covers;
pub mod download;
pub mod library;
pub mod opensearch;
pub mod root;
pub mod shelves;

use axum::Router;

use crate::config::OpdsConfig;
use crate::state::AppState;

/// Build the `/opds/*` router (feeds + downloads + the OPDS-mount cover
/// handlers). Returns `None` when OPDS is disabled so `main.rs` can skip the
/// mount entirely.
pub fn router_enabled(config: &OpdsConfig) -> Option<Router<AppState>> {
    if !config.enabled {
        return None;
    }
    Some(
        Router::new()
            .merge(root::router())
            .merge(library::router())
            .merge(shelves::router())
            .merge(opensearch::router())
            .merge(download::router())
            .merge(covers::opds_router()),
    )
}

/// The `/api/books/:id/cover{,/thumb}` mount. Behind the cookie-or-Basic
/// [`crate::auth::middleware::CurrentUser`] extractor so the Step 10 web UI
/// can load covers with a session cookie. Always mounted — independent of
/// `config.opds.enabled` — because the web UI needs it regardless of OPDS.
pub fn covers_router() -> Router<AppState> {
    covers::api_router()
}

#[cfg(test)]
mod tests;
