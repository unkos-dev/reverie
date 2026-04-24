//! Response-header security surface (UNK-106).
//!
//! - [`csp`]: pure builders for the HTML and API CSP header values.
//! - [`dist_validation`]: startup validation of the frontend dist directory
//!   and its `csp-hashes.json` sidecar.
//! - [`headers`]: the uniform-headers middleware plus the composite
//!   fallback handler that manually attaches per-class CSP headers.

pub mod csp;
pub mod dist_validation;
pub mod headers;
