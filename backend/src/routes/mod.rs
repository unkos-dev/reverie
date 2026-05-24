//! HTTP route handlers grouped by domain.
//!
//! Each submodule exposes a `pub fn router() -> Router<AppState>` (or a
//! `_enabled` variant returning `Option<Router<…>>` when the mount is
//! conditional) consumed by `crate::build_router_with_session_store`
//! to assemble the application router.

/// Authentication endpoints (OIDC login / callback / logout, session
/// `/auth/me` + theme update).
pub mod auth;
/// Sort-aware base64url pagination cursor (shared across `/api/*`
/// list endpoints).
pub mod cursor;
/// Manifestation enrichment trigger / dry-run / status endpoints.
pub mod enrichment;
/// Liveness + readiness probes.
pub mod health;
/// Library-scan trigger endpoint.
pub mod ingestion;
/// Library JSON API (`/api/books`, `/api/books/{id}`, `/api/works/{id}`).
pub mod library;
/// Metadata-version review endpoints (accept / reject / revert / lock).
pub mod metadata;
/// OPDS 1.2 catalog routes (feeds, downloads, cover dual-mount).
pub mod opds;
/// Series JSON API (`/api/series/{id}`).
pub mod series;
/// Shelves CRUD JSON API (`/api/shelves*`).
pub mod shelves;
/// Single-page-app asset-serving router (`/assets/*` mount).
pub mod spa;
/// Per-user device-token issue / list / revoke endpoints.
pub mod tokens;
