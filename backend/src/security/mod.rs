//! Response-header security surface for Reverie.
//!
//! - [`csp`]: pure builders for the HTML and API CSP header values.
//! - [`dist_validation`]: startup validation of the frontend dist directory
//!   and its `csp-hashes.json` sidecar.
//! - [`headers`]: the uniform-headers middleware plus the composite
//!   fallback handler that manually attaches per-class CSP headers.
//!
//! # Tier 2 — security-critical
//!
//! All modules in this directory are Tier 2 under the comment policy
//! (`docs/adr/0004-tiered-comment-policy-for-an-open-source-codebase.md`). Threat annotations are
//! expressed inline (`// THREAT:`) and as one-line statements at the top
//! of Tier 1 docstrings. The documented response-header policy lives in
//! `docs/security/content-security-policy.md`; that document and these
//! docstrings must agree, since they are the two surfaces a security
//! auditor will read.

/// Pure CSP header-value builders. Called once at startup.
pub mod csp;

/// CSRF synchronizer-token validating middleware.
pub mod csrf;

/// Frontend dist + CSP-hash sidecar validation. Called once at startup.
pub mod dist_validation;

/// Response-header middleware + composite fallback handler.
pub mod headers;
