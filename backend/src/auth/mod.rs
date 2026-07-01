//! Authentication subsystem for Reverie.
//!
//! Provides cookie-or-Basic credential resolution ([`middleware`]), a
//! first-party Postgres session store ([`store`]) and session login/logout
//! helpers ([`session`]) on the maintained `tower-sessions` core (replacing the
//! abandoned `axum-login` + `tower-sessions-sqlx-store` wrappers — ADR
//! `2026-06-04-first-party-session-layer.md`), OIDC provider discovery and
//! client construction ([`oidc`]), role-assertion helpers
//! ([`middleware::CurrentUser`]), the credential-capability axis
//! ([`scope::Scope`]), device-token generation and constant-time
//! verification ([`token`]), a Basic-only extractor for OPDS routes
//! ([`basic_only`]), and the FOUC theme-preference cookie ([`theme_cookie`]).
//!
//! # Tier 2 — security-critical
//!
//! All modules in this directory are Tier 2 under the comment policy
//! (`adr/2026-05-08-tiered-comment-policy.md`). Threat annotations
//! (`// THREAT:`) are present on any non-obvious mitigation.

/// `BasicOnly` extractor: rejects session cookies, requires `Authorization: Basic`.
pub mod basic_only;

/// `CurrentUser` extractor: resolves identity via session cookie or Basic auth.
pub mod middleware;

/// OIDC provider discovery and `OidcClient` construction.
pub mod oidc;

/// Argon2id password hashing/verification for local accounts.
pub mod password;

/// Password policy: length bounds, zxcvbn strength floor, HIBP breach check.
pub mod password_policy;

/// Per-source (per-IP) login rate limiting (keyed governor + client-IP helper).
pub mod rate_limit;

/// Forgot-password recovery: PIN generation + operator-readable host file.
pub mod recovery;

/// `Scope`: credential capability (`read`/`write`/`admin`), orthogonal to role.
pub mod scope;

/// Session login / logout helpers on `tower_sessions::Session`.
pub mod session;

/// First-party Postgres `SessionStore` over `tower_sessions.session`.
pub mod store;

/// FOUC theme-preference cookie (`reverie_theme`): set/read helpers.
pub mod theme_cookie;

/// Device-token generation and SHA-256 constant-time verification.
pub mod token;
