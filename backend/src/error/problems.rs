//! RFC 9457 (obsoletes RFC 7807) problem-type URI registry.
//!
//! One `const` per [`crate::error::AppError`] variant. URIs are stable
//! identifiers — they do not need to dereference at first
//! (RFC 9457 §3.1). The host portion uses the placeholder
//! `reverie.example` until the OSS release pins a canonical project
//! URL; the slugs are the load-bearing part and stay frozen across
//! that swap.
//!
//! Convention: each problem-type URI is `{PROBLEM_BASE}/{slug}`. The
//! [`problem_type`] helper assembles the full URI from a slug.

/// Base URI prefix shared by every Reverie problem-type identifier.
///
/// Placeholder host: `reverie.example`. Swap on OSS release; the
/// slugs identified by [`NOT_FOUND`], [`UNAUTHORIZED`], etc. remain
/// stable across the swap.
pub const PROBLEM_BASE: &str = "https://reverie.example/probs";

/// Resource not found (RLS-hidden rows resolve to this, not
/// `forbidden`, to avoid leaking resource existence).
pub const NOT_FOUND: &str = "not-found";

/// Caller is unauthenticated. JSON API variant — does not emit
/// `WWW-Authenticate: Basic`; see also [`crate::error::AppError::BasicAuthRequired`].
pub const UNAUTHORIZED: &str = "unauthorized";

/// Caller is authenticated but lacks the role or policy to perform
/// the requested action.
pub const FORBIDDEN: &str = "forbidden";

/// Caller is unauthenticated on an OPDS route. Carries `WWW-Authenticate:
/// Basic` (RFC 7617) alongside this RFC 9457 body; see
/// [`crate::error::AppError::BasicAuthRequired`].
pub const BASIC_AUTH_REQUIRED: &str = "basic-auth-required";

/// Request body or query failed validation; `detail` carries the
/// caller-visible message.
pub const VALIDATION: &str = "validation";

/// `X-CSRF-Token` header missing on a mutating-verb request under
/// `/api/v1/*`. HTTP 428 Precondition Required.
pub const CSRF_MISSING: &str = "csrf-missing";

/// `X-CSRF-Token` header present but does not match the session
/// token (constant-time compare). HTTP 403 Forbidden.
pub const CSRF_MISMATCH: &str = "csrf-mismatch";

/// `If-Match` header missing on a precondition-protected endpoint.
/// HTTP 428 Precondition Required.
pub const IF_MATCH_REQUIRED: &str = "if-match-required";

/// `If-Match` header present but `ETag` does not match resource
/// state. HTTP 412 Precondition Failed.
pub const IF_MATCH_MISMATCH: &str = "if-match-mismatch";

/// Caller attempted to mutate a system-managed shelf (`is_system =
/// TRUE`).
///
/// System shelves are append-only from the operator side —
/// users can add or remove items, but the shelf row itself cannot be
/// renamed or deleted. HTTP 409 Conflict.
pub const SYSTEM_SHELF_IMMUTABLE: &str = "system-shelf-immutable";

/// A query-string parameter failed to deserialize (e.g. a malformed
/// UUID in `?author=`, `?series=`, `?shelf=`).
///
/// Distinct from
/// [`VALIDATION`] (422, business-rule/value rejection): this is a
/// syntactic decode failure at the extractor boundary, so it maps to
/// HTTP 400 Bad Request.
pub const MALFORMED_QUERY: &str = "malformed-query";

/// A request header failed its grammar or usage contract (e.g. a
/// malformed, weak, or list-form `If-Match` entity-tag).
///
/// RFC 9110 scopes
/// 422 to request content, so a rejected header value is a syntactic
/// decode failure, not a business-rule rejection, and maps to HTTP 400
/// Bad Request like [`MALFORMED_QUERY`] rather than [`VALIDATION`].
pub const MALFORMED_HEADER: &str = "malformed-header";

/// Per-source login rate limit exceeded
/// ([`crate::error::AppError::RateLimited`]). HTTP 429.
pub const RATE_LIMITED: &str = "rate-limited";

/// First-run setup attempted after an administrator already exists
/// ([`crate::error::AppError::SetupAlreadyComplete`]). HTTP 409.
pub const SETUP_ALREADY_COMPLETE: &str = "setup-already-complete";

/// Account creation or self-registration rejected: the email is already in use
/// ([`crate::error::AppError::EmailConflict`]). HTTP 409.
pub const EMAIL_CONFLICT: &str = "email-conflict";

/// Method not supported on an otherwise-matched route
/// ([`crate::error::AppError::MethodNotAllowed`]). HTTP 405; the `Allow`
/// header (added by axum) lists the methods registered for the path.
pub const METHOD_NOT_ALLOWED: &str = "method-not-allowed";

/// Generic internal error (anything wrapped in
/// [`crate::error::AppError::Internal`]). `detail` is a fixed
/// non-leaking string; the inner cause is `tracing::error!`-logged
/// for operator triage.
pub const INTERNAL: &str = "internal";

/// Assemble a full problem-type URI from a slug.
///
/// `problem_type("not-found")` →
/// `"https://reverie.example/probs/not-found"`.
#[must_use]
pub fn problem_type(slug: &str) -> String {
    format!("{PROBLEM_BASE}/{slug}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn problem_type_assembles_full_uri() {
        assert_eq!(
            problem_type(NOT_FOUND),
            "https://reverie.example/probs/not-found"
        );
        assert_eq!(
            problem_type(CSRF_MISMATCH),
            "https://reverie.example/probs/csrf-mismatch"
        );
    }
}
