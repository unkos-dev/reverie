//! base64url-encoded pagination cursor.
//!
//! Encodes `(created_at, id)` tuples that the acquisition feeds sort on:
//! `ORDER BY created_at DESC, id DESC`. The cursor points at the last row of
//! the previous page — the next SELECT uses it as an inclusive-exclusive
//! upper bound via `WHERE (created_at, id) < ($1, $2)`.
//!
//! No HMAC: homelab trust model. Cursor timestamps already leak via the feed
//! body, and an attacker flipping a cursor gets a different page of rows they
//! can already see — authoritative scoping lives in RLS, not the cursor.

use base64ct::{Base64UrlUnpadded, Encoding};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cursor {
    pub created_at: OffsetDateTime,
    pub id: Uuid,
}

#[derive(Debug, thiserror::Error)]
pub enum CursorError {
    #[error("invalid base64url")]
    InvalidBase64,
    #[error("missing delimiter")]
    MissingDelimiter,
    #[error("invalid timestamp")]
    InvalidTimestamp,
    #[error("invalid uuid")]
    InvalidUuid,
    #[error("invalid utf-8")]
    InvalidUtf8,
}

impl Cursor {
    pub fn encode(&self) -> String {
        let ts = self
            .created_at
            .format(&Rfc3339)
            .expect("OffsetDateTime always formats as Rfc3339");
        let payload = format!("{ts}|{}", self.id.as_hyphenated());
        Base64UrlUnpadded::encode_string(payload.as_bytes())
    }

    pub fn parse(s: &str) -> Result<Self, CursorError> {
        let mut buf = vec![0u8; s.len()];
        let decoded = Base64UrlUnpadded::decode(s.as_bytes(), &mut buf)
            .map_err(|_| CursorError::InvalidBase64)?;
        let decoded_str = std::str::from_utf8(decoded).map_err(|_| CursorError::InvalidUtf8)?;
        let (ts, uuid) = decoded_str
            .split_once('|')
            .ok_or(CursorError::MissingDelimiter)?;
        let created_at =
            OffsetDateTime::parse(ts, &Rfc3339).map_err(|_| CursorError::InvalidTimestamp)?;
        let id = Uuid::parse_str(uuid).map_err(|_| CursorError::InvalidUuid)?;
        Ok(Cursor { created_at, id })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_parse_roundtrip() {
        let ts = OffsetDateTime::parse("2026-04-21T09:30:00Z", &Rfc3339).unwrap();
        let id = Uuid::new_v4();
        let c = Cursor { created_at: ts, id };
        let encoded = c.encode();
        let parsed = Cursor::parse(&encoded).expect("round-trip parse");
        assert_eq!(parsed.created_at, ts);
        assert_eq!(parsed.id, id);
    }

    #[test]
    fn rejects_invalid_base64() {
        assert!(matches!(
            Cursor::parse("!!!not-base64url!!!"),
            Err(CursorError::InvalidBase64)
        ));
    }

    #[test]
    fn rejects_missing_delimiter() {
        let encoded = Base64UrlUnpadded::encode_string(b"no-pipe-here");
        assert!(matches!(
            Cursor::parse(&encoded),
            Err(CursorError::MissingDelimiter)
        ));
    }

    #[test]
    fn rejects_bad_timestamp() {
        let encoded = Base64UrlUnpadded::encode_string(
            b"not-a-timestamp|550e8400-e29b-41d4-a716-446655440000",
        );
        assert!(matches!(
            Cursor::parse(&encoded),
            Err(CursorError::InvalidTimestamp)
        ));
    }

    #[test]
    fn rejects_bad_uuid() {
        let encoded = Base64UrlUnpadded::encode_string(b"2026-04-21T09:30:00Z|not-a-uuid");
        assert!(matches!(
            Cursor::parse(&encoded),
            Err(CursorError::InvalidUuid)
        ));
    }
}
