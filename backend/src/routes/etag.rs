//! Shared hash-based ETag mechanics for optimistic-concurrency endpoints.
//!
//! Complements [`crate::routes::shelves`]'s timestamp-based scheme (its
//! entity-tag is the shelf's `updated_at`, unrelated to this module), which
//! stays on that encoding out of scope for this change. Endpoints in this
//! module hash a caller-supplied serde representation of the fields a PATCH
//! can modify, so the tag changes exactly when that representation does and
//! never leaks a raw timestamp.
//!
//! Every caller feeds its own dedicated, fixed-field-order struct into
//! `hash_etag` so the hash input is an explicit, auditable contract rather
//! than an incidental byproduct of a DTO also used for JSON output.

use axum::http::header::{ETAG, IF_MATCH};
use axum::http::{HeaderMap, HeaderValue};
use axum::response::{IntoResponse, Response};
use base64ct::{Base64UrlUnpadded, Encoding};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::error::AppError;

/// SHA-256 digest bytes kept after truncation, before base64url encoding.
/// 16 bytes (128 bits) is enough collision resistance for a concurrency
/// check between the same handful of concurrent editors and keeps the
/// header value compact.
const ETAG_HASH_BYTES: usize = 16;

/// Hash a canonical serde representation of a resource's editable state into
/// a quoted strong entity-tag (RFC 9110 §8.8.3).
///
/// `state` must be a dedicated struct with a fixed field order covering at
/// least every field the paired PATCH endpoint can modify, never the raw
/// `updated_at` column. Serialises via `serde_json`, SHA-256s the bytes,
/// truncates to `ETAG_HASH_BYTES`, and base64url-encodes without padding.
pub fn hash_etag<T: Serialize>(state: &T) -> Result<HeaderValue, AppError> {
    let json = serde_json::to_vec(state).map_err(|e| AppError::Internal(e.into()))?;
    let digest = Sha256::digest(&json);
    let encoded = Base64UrlUnpadded::encode_string(&digest[..ETAG_HASH_BYTES]);
    HeaderValue::from_str(&format!("\"{encoded}\"")).map_err(|e| AppError::Internal(e.into()))
}

/// Parse a strong `If-Match` entity-tag from the request headers.
///
/// The accepted grammar is exactly one quoted strong entity-tag. RFC 9110's
/// `*` wildcard form is deliberately rejected: this API's
/// optimistic-concurrency contract is a single tag echoing a prior
/// response's `ETag`, and accepting the wildcard would let a caller opt out
/// of the freshness check it exists to enforce. A comma-separated
/// entity-tag list is rejected the same way: the etagc grammar in RFC 9110
/// §8.8.3 excludes `"`, so a quote inside the outer quotes can only mean a
/// list or garbage, never a valid tag.
///
/// Returns the header's quoted wire form verbatim (not the inner bytes) so
/// callers compare it byte-for-byte against a freshly computed
/// [`hash_etag`] value.
///
/// - `Ok(None)`: header absent. Every current caller treats this as
///   `AppError::IfMatchRequired` (428) via `.ok_or(...)`; `None` stays a
///   distinct case here so parsing and precondition enforcement remain
///   separately testable.
/// - `Ok(Some(_))`: a well-formed strong entity-tag, still quoted.
/// - `Err(AppError::MalformedHeader)`: malformed value, a weak validator
///   (`W/"..."`) (RFC 9110 §13.1.2 requires strong comparison for
///   `If-Match`), a list of entity-tags, the `*` wildcard, or the header
///   sent as more than one instance (semantically the same list form).
pub fn parse_if_match(headers: &HeaderMap) -> Result<Option<String>, AppError> {
    if headers.get_all(IF_MATCH).iter().count() > 1 {
        return Err(AppError::MalformedHeader(
            "If-Match must be sent as a single header instance carrying one entity-tag".into(),
        ));
    }
    let Some(raw) = headers.get(IF_MATCH) else {
        return Ok(None);
    };
    let value = raw
        .to_str()
        .map_err(|_| AppError::MalformedHeader("If-Match header must be ASCII".into()))?
        .trim();
    if value.starts_with("W/") {
        return Err(AppError::MalformedHeader(
            "weak entity-tags (W/\"...\") not accepted for If-Match".into(),
        ));
    }
    if value.len() < 2 || !value.starts_with('"') || !value.ends_with('"') {
        return Err(AppError::MalformedHeader(
            "If-Match value must be a quoted entity-tag".into(),
        ));
    }
    if value[1..value.len() - 1].contains('"') {
        return Err(AppError::MalformedHeader(
            "If-Match value must be a single quoted entity-tag".into(),
        ));
    }
    Ok(Some(value.to_owned()))
}

/// Build the 412 response for a stale `If-Match`, carrying the resource's
/// current `ETag` so the caller can resync in one round trip instead of
/// issuing a follow-up `GET`.
pub fn if_match_mismatch(current_etag: &HeaderValue) -> Response {
    let mut response = AppError::IfMatchMismatch.into_response();
    response.headers_mut().insert(ETAG, current_etag.clone());
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Serialize;

    #[derive(Serialize)]
    struct Fixture {
        a: &'static str,
        b: i32,
    }

    #[test]
    fn hash_etag_is_quoted_and_stable() {
        let f = Fixture { a: "x", b: 1 };
        let one = hash_etag(&f).unwrap();
        let two = hash_etag(&f).unwrap();
        assert_eq!(one, two);
        let s = one.to_str().unwrap();
        assert!(s.starts_with('"') && s.ends_with('"'));
    }

    #[test]
    fn hash_etag_changes_with_state() {
        let a = hash_etag(&Fixture { a: "x", b: 1 }).unwrap();
        let b = hash_etag(&Fixture { a: "x", b: 2 }).unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn parse_if_match_absent_is_none() {
        let headers = HeaderMap::new();
        assert!(parse_if_match(&headers).unwrap().is_none());
    }

    #[test]
    fn parse_if_match_rejects_weak_validator() {
        let mut headers = HeaderMap::new();
        headers.insert(IF_MATCH, HeaderValue::from_static("W/\"abc\""));
        assert!(parse_if_match(&headers).is_err());
    }

    #[test]
    fn parse_if_match_rejects_unquoted() {
        let mut headers = HeaderMap::new();
        headers.insert(IF_MATCH, HeaderValue::from_static("abc"));
        assert!(parse_if_match(&headers).is_err());
    }

    #[test]
    fn parse_if_match_rejects_entity_tag_list() {
        let mut headers = HeaderMap::new();
        headers.insert(IF_MATCH, HeaderValue::from_static("\"abc\", \"def\""));
        assert!(parse_if_match(&headers).is_err());
    }

    #[test]
    fn parse_if_match_accepts_strong_tag() {
        let mut headers = HeaderMap::new();
        headers.insert(IF_MATCH, HeaderValue::from_static("\"abc\""));
        assert_eq!(
            parse_if_match(&headers).unwrap(),
            Some("\"abc\"".to_owned())
        );
    }

    #[test]
    fn parse_if_match_rejects_repeated_header_instances() {
        let mut headers = HeaderMap::new();
        headers.append(IF_MATCH, HeaderValue::from_static("\"abc\""));
        headers.append(IF_MATCH, HeaderValue::from_static("\"def\""));
        assert!(parse_if_match(&headers).is_err());
    }
}
