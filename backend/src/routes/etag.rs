//! Shared hash-based ETag mechanics for optimistic-concurrency endpoints.
//!
//! Distinct from [`crate::routes::shelves`], whose entity-tag is the shelf's
//! `updated_at` and whose `If-Match` parsing is its own. Endpoints in this
//! module hash a caller-supplied serde representation of the fields a PATCH
//! can modify, so the tag changes exactly when that representation does and
//! never leaks a raw timestamp.
//!
//! Also distinct from the static-resource cache validators `tower-http`
//! serves for the frontend dist tree, under `/assets` and through the
//! composite fallback: this module's single-strong-tag rule governs
//! conditional writes on API resources only.
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

/// One strong entity-tag, held in the quoted wire form it was received in.
///
/// Constructing this type is the only way to get past
/// [`parse_if_match`], so a value of it is proof that the bytes satisfied
/// the RFC 9110 §8.8.3 `entity-tag` grammar and carried no weak prefix.
/// Comparison lives on the type rather than at the call sites so no
/// endpoint can accidentally reintroduce a weak or normalising match.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StrongEntityTag(Box<[u8]>);

impl StrongEntityTag {
    /// Whether this tag and `current` are the same entity-tag under RFC 9110
    /// §13.1.2 strong comparison: both strong, with octet-identical opaque
    /// content. Strongness holds by construction on this side and by
    /// [`hash_etag`] on the other, so octet equality of the quoted forms is
    /// the whole comparison.
    ///
    /// The opaque content is compared as received. Reverie never normalises
    /// it, because an entity-tag is opaque to everyone but its issuer.
    #[must_use]
    pub fn matches(&self, current: &HeaderValue) -> bool {
        *self.0 == *current.as_bytes()
    }
}

/// Whether `byte` is RFC 9110 §8.8.3 `etagc`, the grammar for the octets an
/// entity-tag carries between its delimiting DQUOTEs:
/// `etagc = %x21 / %x23-7E / obs-text`, that is every VCHAR except DQUOTE
/// (`%x22`) plus `obs-text` (`%x80-FF`). SP, HTAB, the C0 controls, and DEL
/// are therefore not entity-tag content.
const fn is_etagc(byte: u8) -> bool {
    matches!(byte, 0x21 | 0x23..=0x7E | 0x80..=0xFF)
}

/// Strip leading and trailing RFC 9110 §5.6.3 `OWS` (SP and HTAB only).
///
/// Deliberately not `str::trim`: this field is a byte string, and Unicode
/// whitespace trimming would both mishandle `obs-text` and strip octets
/// that are legitimate entity-tag content.
fn trim_ows(bytes: &[u8]) -> &[u8] {
    let start = bytes
        .iter()
        .position(|&b| b != b' ' && b != b'\t')
        .unwrap_or(bytes.len());
    let end = bytes
        .iter()
        .rposition(|&b| b != b' ' && b != b'\t')
        .map_or(start, |i| i + 1);
    &bytes[start..end]
}

/// Parse one strong entity-tag from a raw header field value.
///
/// Operates on octets rather than a `&str`, because RFC 9110 §8.8.3 defines
/// `etagc` over bytes: `obs-text` (`%x80-FF`) is valid entity-tag content
/// and is not valid UTF-8, so reading the field as a string would reject
/// well-formed tags while admitting SP, which the grammar excludes.
///
/// Split out from [`parse_if_match`] so the grammar is testable over inputs
/// a `HeaderValue` cannot represent: `http` rejects the C0 controls and DEL
/// at header-construction time, so those bytes never reach this parser in
/// production and the checks against them are defence in depth.
fn parse_strong_entity_tag(raw: &[u8]) -> Result<StrongEntityTag, AppError> {
    let value = trim_ows(raw);
    // Rejected before the quoting check so the wildcard, a valid RFC 9110
    // form, gets its own reason rather than "not quoted".
    if value == b"*" {
        return Err(AppError::MalformedHeader(
            "If-Match must carry one entity-tag; the * wildcard is not accepted".into(),
        ));
    }
    if value.starts_with(b"W/") {
        return Err(AppError::MalformedHeader(
            "weak entity-tags (W/\"...\") not accepted for If-Match".into(),
        ));
    }
    let Some(opaque) = value
        .strip_prefix(b"\"")
        .and_then(|rest| rest.strip_suffix(b"\""))
    else {
        return Err(AppError::MalformedHeader(
            "If-Match value must be a quoted entity-tag".into(),
        ));
    };
    // `etagc` excludes DQUOTE, so an inner quote is never tag content: it
    // marks the boundary between list members, or garbage. Checked ahead of
    // the general grammar check to name the list case specifically.
    if opaque.contains(&b'"') {
        return Err(AppError::MalformedHeader(
            "If-Match must carry exactly one entity-tag; lists are not accepted".into(),
        ));
    }
    if !opaque.iter().copied().all(is_etagc) {
        return Err(AppError::MalformedHeader(
            "If-Match entity-tag contains an octet outside the RFC 9110 etagc grammar".into(),
        ));
    }
    Ok(StrongEntityTag(value.into()))
}

/// Parse a strong `If-Match` entity-tag from the request headers.
///
/// The accepted grammar is the full RFC 9110 §8.8.3 `entity-tag`, minus the
/// weak form; the policy layered over it is exactly one such tag. RFC
/// 9110's `*` wildcard is deliberately rejected: this API's
/// optimistic-concurrency contract is a single tag echoing a prior
/// response's `ETag`, and accepting the wildcard would let a caller opt out
/// of the freshness check it exists to enforce. Entity-tag lists and
/// repeated field instances (semantically the same list form) are rejected
/// for the same reason. Those three are well-formed HTTP that this API
/// refuses by policy; both they and outright grammar violations are 400,
/// per the status-selection rule in the JSON API conventions ADR.
///
/// - `Ok(None)`: header absent. Every current caller treats this as
///   `AppError::IfMatchRequired` (428) via `.ok_or(...)`; `None` stays a
///   distinct case here so parsing and precondition enforcement remain
///   separately testable.
/// - `Ok(Some(_))`: one well-formed strong entity-tag.
/// - `Err(AppError::MalformedHeader)`: a grammar violation, a weak
///   validator (`W/"..."`) (RFC 9110 §13.1.2 requires strong comparison for
///   `If-Match`), a list of entity-tags, the `*` wildcard, or the header
///   sent as more than one instance.
pub fn parse_if_match(headers: &HeaderMap) -> Result<Option<StrongEntityTag>, AppError> {
    if headers.get_all(IF_MATCH).iter().count() > 1 {
        return Err(AppError::MalformedHeader(
            "If-Match must be sent as a single header instance carrying one entity-tag".into(),
        ));
    }
    let Some(raw) = headers.get(IF_MATCH) else {
        return Ok(None);
    };
    parse_strong_entity_tag(raw.as_bytes()).map(Some)
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

    /// Every `etagc` byte class, exercised as one tag: `%x21`, the ends of
    /// `%x23-7E`, and the ends of `obs-text`.
    const EVERY_ETAGC_CLASS: &[u8] = b"\"!\x23~\x80\xff\"";

    #[test]
    fn parse_strong_entity_tag_grammar_table() {
        // (label, raw field value, accepted). Byte literals rather than
        // `HeaderValue`s because `http` refuses to build a header carrying a
        // C0 control or DEL, so those rows are only reachable here.
        let cases: &[(&str, &[u8], bool)] = &[
            ("empty tag is a valid entity-tag", b"\"\"", true),
            ("ordinary opaque tag", b"\"abc\"", true),
            ("every etagc byte class", EVERY_ETAGC_CLASS, true),
            ("obs-text lower bound", b"\"\x80\"", true),
            ("obs-text upper bound", b"\"\xff\"", true),
            (
                "surrounding OWS is not tag content",
                b"  \t\"abc\"\t ",
                true,
            ),
            ("SP is not etagc", b"\"a b\"", false),
            ("HTAB inside the quotes is not etagc", b"\"a\tb\"", false),
            ("NUL is not etagc", b"\"a\x00b\"", false),
            ("C0 control is not etagc", b"\"a\x1fb\"", false),
            ("DEL is not etagc", b"\"a\x7fb\"", false),
            (
                "inner DQUOTE can only be a list",
                b"\"abc\", \"def\"",
                false,
            ),
            ("weak validator", b"W/\"abc\"", false),
            ("wildcard", b"*", false),
            ("unquoted", b"abc", false),
            ("missing closing quote", b"\"abc", false),
            ("missing opening quote", b"abc\"", false),
            ("lone quote", b"\"", false),
            ("empty field value", b"", false),
            ("OWS only", b" \t ", false),
        ];
        for (label, raw, accepted) in cases {
            assert_eq!(
                parse_strong_entity_tag(raw).is_ok(),
                *accepted,
                "{label}: {raw:?}"
            );
        }
    }

    #[test]
    fn parse_strong_entity_tag_preserves_opaque_bytes() {
        let tag = parse_strong_entity_tag(EVERY_ETAGC_CLASS).unwrap();
        assert_eq!(&*tag.0, EVERY_ETAGC_CLASS);
    }

    #[test]
    fn parse_strong_entity_tag_drops_only_surrounding_ows() {
        let tag = parse_strong_entity_tag(b" \t\"abc\" \t").unwrap();
        assert_eq!(&*tag.0, b"\"abc\"");
    }

    #[test]
    fn strong_comparison_is_octet_exact() {
        let tag = parse_strong_entity_tag(b"\"abc\"").unwrap();
        assert!(tag.matches(&HeaderValue::from_static("\"abc\"")));
        assert!(!tag.matches(&HeaderValue::from_static("\"abz\"")));
        // Not normalised: the opaque content is the issuer's to interpret.
        assert!(!tag.matches(&HeaderValue::from_static("\"ABC\"")));
    }

    #[test]
    fn strong_comparison_holds_for_obs_text() {
        let tag = parse_strong_entity_tag(b"\"\x80\"").unwrap();
        assert!(tag.matches(&HeaderValue::from_bytes(b"\"\x80\"").unwrap()));
        assert!(!tag.matches(&HeaderValue::from_bytes(b"\"\x81\"").unwrap()));
    }

    #[test]
    fn parse_if_match_absent_is_none() {
        let headers = HeaderMap::new();
        assert!(parse_if_match(&headers).unwrap().is_none());
    }

    #[test]
    fn parse_if_match_accepts_one_strong_tag() {
        let mut headers = HeaderMap::new();
        headers.insert(IF_MATCH, HeaderValue::from_static("\"abc\""));
        let tag = parse_if_match(&headers).unwrap().unwrap();
        assert!(tag.matches(&HeaderValue::from_static("\"abc\"")));
    }

    #[test]
    fn parse_if_match_rejects_repeated_header_instances() {
        let mut headers = HeaderMap::new();
        headers.append(IF_MATCH, HeaderValue::from_static("\"abc\""));
        headers.append(IF_MATCH, HeaderValue::from_static("\"def\""));
        assert!(parse_if_match(&headers).is_err());
    }

    // Two valid instances are still rejected: repeating the field is the
    // list form, and the reason must not depend on the values being
    // individually well-formed.
    #[test]
    fn parse_if_match_rejects_repeated_instances_before_parsing_either() {
        let mut headers = HeaderMap::new();
        headers.append(IF_MATCH, HeaderValue::from_static("\"abc\""));
        headers.append(IF_MATCH, HeaderValue::from_static("\"abc\""));
        assert!(parse_if_match(&headers).is_err());
    }
}
