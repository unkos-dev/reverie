//! HTTP route handlers grouped by domain.
//!
//! Each submodule exposes a `pub fn router() -> Router<AppState>` (or a
//! `_enabled` variant returning `Option<Router<…>>` when the mount is
//! conditional) consumed by `crate::build_router_with_session_store`
//! to assemble the application router.

/// Authentication endpoints (OIDC login / callback / logout, session
/// `/auth/me` + theme update).
pub mod auth;
/// Sort-aware base64url pagination cursor (shared across `/api/v1/*`
/// list endpoints).
pub mod cursor;
/// Admin-only library-health dashboard (`/api/v1/dashboard/*`).
pub mod dashboard;
/// Manifestation enrichment trigger / dry-run / status endpoints.
pub mod enrichment;
/// Liveness + readiness probes.
pub mod health;
/// Library-scan trigger endpoint.
pub mod ingestion;
/// Library JSON API (`/api/v1/books`, `/api/v1/books/{id}`, `/api/v1/works/{id}`).
pub mod library;
/// Metadata-version review endpoints (accept / reject / revert / lock).
pub mod metadata;
/// OPDS 1.2 catalog routes (feeds, downloads, cover dual-mount).
pub mod opds;
/// Per-user display preferences (`/auth/me/preferences`).
pub mod preferences;
/// Per-user reading state JSON API (`/api/v1/books/{id}/reading`).
pub mod reading;
/// Series JSON API (`/api/v1/series/{id}`).
pub mod series;
/// Admin-only persisted settings (`/api/v1/settings`).
pub mod settings;
/// Shelves CRUD JSON API (`/api/v1/shelves*`).
pub mod shelves;
/// Whitelisted sort columns and the `?sort=` grammar for `/api/v1/books`.
pub mod sort_spec;
/// Single-page-app asset-serving router (`/assets/*` mount).
pub mod spa;
/// Typeahead suggest endpoints for metadata vocabularies (`/api/v1/{genres,moods,tags,authors,series,publishers}/suggest`).
pub mod suggest;
/// Per-user device-token issue / list / revoke endpoints.
pub mod tokens;
/// Admin-only user management (`/api/v1/users*`).
pub mod users;
