//! OPDS 1.2 catalog routes. Mount under `/opds/*` with a Basic-only
//! extractor (RFC 7617), plus a dual-mount cover handler at
//! `/api/v1/books/:id/cover{,/thumb}` behind cookie-or-Basic for the web UI.
//!
//! Scope is URL-based: pair a device at `/opds/library/*` to see the whole
//! library (further filtered by child-account RLS) or at
//! `/opds/shelves/{id}/*` to see only that shelf.

pub mod covers;
pub mod cursor;
pub mod download;
pub mod feed;
pub mod library;
pub mod opensearch;
pub mod root;
pub mod scope;
pub mod shelves;
pub mod xml;

use axum::Router;
use utoipa_axum::router::OpenApiRouter;

use crate::config::OpdsConfig;
use crate::state::AppState;

/// Build the full `/opds/*` documented router (feeds + downloads + the
/// OPDS-mount cover handlers), unconditionally. The OpenAPI spec is built
/// from this regardless of `config.opds.enabled` — the contract documents
/// the OPDS surface even on instances where the operator has it switched
/// off; only the runtime mount (see [`router_enabled`]) is config-gated.
pub fn openapi_router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .merge(root::router())
        .merge(library::router())
        .merge(shelves::router())
        .merge(opensearch::router())
        .merge(download::router())
        .merge(covers::opds_router())
}

/// The runtime half of [`openapi_router`]. Returns `None` when OPDS is
/// disabled so `crate::build_router` can skip the mount entirely; the spec
/// half is merged unconditionally in `crate::openapi::spec_json`.
pub fn router_enabled(config: &OpdsConfig) -> Option<Router<AppState>> {
    config.enabled.then(|| openapi_router().split_for_parts().0)
}

/// The `/api/v1/books/:id/cover{,/thumb}` mount. Behind the cookie-or-Basic
/// [`crate::auth::middleware::CurrentUser`] extractor so the web UI
/// can load covers with a session cookie. Always mounted — independent of
/// `config.opds.enabled` — because the web UI needs it regardless of OPDS.
/// Merged into the documented pilot router in `crate::openapi`.
pub fn covers_router() -> OpenApiRouter<AppState> {
    covers::api_router()
}

#[cfg(test)]
mod tests;
