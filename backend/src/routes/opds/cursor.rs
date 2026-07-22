//! base64url-encoded pagination cursors.
//!
//! [`Cursor`] encodes `(created_at, id)` tuples that the acquisition feeds
//! sort on: `ORDER BY created_at DESC, id DESC`. The cursor points at the
//! last row of the previous page — the next SELECT uses it as an
//! inclusive-exclusive upper bound via `WHERE (created_at, id) < ($1, $2)`.
//!
//! [`NameCursor`] encodes `(sort_name, id)` for the authors /
//! series navigation feeds, which sort `ORDER BY sort_name ASC, id ASC` —
//! lower bound via `WHERE (sort_name, id) > ($1, $2)`. It carries an `n|`
//! tag byte so the two cursor families cannot be replayed against each
//! other's feeds.
//!
//! No HMAC: self-hosted trusted-network deployment model. Cursor timestamps
//! already leak via the feed body, and an attacker flipping a cursor gets a
//! different page of rows they can already see — authoritative scoping lives
//! in RLS, not the cursor.

use base64ct::{Base64UrlUnpadded, Encoding};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use uuid::Uuid;

/// `(created_at, id)` tuple identifying the boundary row of a feed page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cursor {
    /// Boundary row's `manifestations.created_at`.
    pub created_at: OffsetDateTime,
    /// Boundary row's `manifestations.id` (tie-breaker for identical
    /// `created_at`).
    pub id: Uuid,
}

/// Parse and encode failures for [`Cursor`].
///
/// Parsing ([`Cursor::parse`]) yields the input-shape variants; encoding
/// ([`Cursor::encode`]) yields [`Self::FormatTimestamp`].
#[derive(Debug, thiserror::Error)]
pub enum CursorError {
    /// Input was not valid base64url.
    #[error("invalid base64url")]
    InvalidBase64,
    /// Decoded payload had no `|` between the timestamp and uuid halves.
    #[error("missing delimiter")]
    MissingDelimiter,
    /// Timestamp half failed `Rfc3339` parse.
    #[error("invalid timestamp")]
    InvalidTimestamp,
    /// Uuid half failed parse.
    #[error("invalid uuid")]
    InvalidUuid,
    /// Decoded bytes were not valid UTF-8.
    #[error("invalid utf-8")]
    InvalidUtf8,
    /// Payload carried a foreign tag byte (e.g. a recency cursor fed
    /// to a name-sorted feed, or vice versa).
    #[error("unknown tag byte")]
    UnknownTag,
    /// `created_at` had a year outside RFC 3339's representable range
    /// (`-9999..=9999`) during encode.
    #[error("timestamp not representable as RFC 3339")]
    FormatTimestamp(#[from] time::error::Format),
}

impl Cursor {
    /// Encode this cursor as a base64url-encoded `<rfc3339>|<uuid>`
    /// string suitable for use in a feed `?cursor=` query parameter.
    ///
    /// # Errors
    ///
    /// Returns [`CursorError::FormatTimestamp`] if `created_at` has a
    /// year outside RFC 3339's `-9999..=9999` range. Rust-side
    /// `OffsetDateTime` caps years at 9999, so this is unreachable for
    /// timestamps written through Reverie; it can only trigger for a
    /// `TIMESTAMPTZ` mutated out-of-band (raw `psql`, a migration) to a
    /// year > 9999.
    pub fn encode(&self) -> Result<String, CursorError> {
        let ts = self.created_at.format(&Rfc3339)?;
        let payload = format!("{ts}|{}", self.id.as_hyphenated());
        Ok(Base64UrlUnpadded::encode_string(payload.as_bytes()))
    }

    /// Parse a base64url-encoded cursor string back into `(created_at, id)`.
    ///
    /// # Errors
    ///
    /// Returns the matching [`CursorError`] variant for invalid base64,
    /// missing delimiter, malformed timestamp, malformed uuid, or
    /// non-UTF-8 decoded bytes.
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
        Ok(Self { created_at, id })
    }
}

/// `(sort_name, id)` tuple identifying the boundary row of a
/// name-sorted navigation feed page (authors / series).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NameCursor {
    /// Boundary row's `sort_name`.
    pub sort_name: String,
    /// Boundary row's id (tie-breaker — `sort_name` carries no
    /// uniqueness constraint).
    pub id: Uuid,
}

impl NameCursor {
    /// Encode as a base64url `n|<sort_name>|<uuid>` string suitable
    /// for a feed `?cursor=` query parameter.
    pub fn encode(&self) -> String {
        let payload = format!("n|{}|{}", self.sort_name, self.id.as_hyphenated());
        Base64UrlUnpadded::encode_string(payload.as_bytes())
    }

    /// Parse a base64url cursor produced by [`Self::encode`].
    ///
    /// `sort_name` derives from external metadata and may contain `|`;
    /// the parser peels the tag with `split_once` and the trailing uuid
    /// with `rsplit_once`, so pipes inside the name survive the
    /// round-trip.
    ///
    /// # Errors
    ///
    /// Returns the matching [`CursorError`] variant for invalid base64,
    /// non-UTF-8 bytes, a missing delimiter, a foreign tag byte, or a
    /// malformed uuid.
    pub fn parse(s: &str) -> Result<Self, CursorError> {
        let mut buf = vec![0u8; s.len()];
        let decoded = Base64UrlUnpadded::decode(s.as_bytes(), &mut buf)
            .map_err(|_| CursorError::InvalidBase64)?;
        let decoded_str = std::str::from_utf8(decoded).map_err(|_| CursorError::InvalidUtf8)?;
        let (tag, rest) = decoded_str
            .split_once('|')
            .ok_or(CursorError::MissingDelimiter)?;
        if tag != "n" {
            return Err(CursorError::UnknownTag);
        }
        let (sort_name, id_str) = rest.rsplit_once('|').ok_or(CursorError::MissingDelimiter)?;
        let id = Uuid::parse_str(id_str).map_err(|_| CursorError::InvalidUuid)?;
        Ok(Self {
            sort_name: sort_name.to_owned(),
            id,
        })
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
        let encoded = c.encode().expect("encode");
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

    #[test]
    fn name_roundtrip() {
        let c = NameCursor {
            sort_name: "le guin, ursula k.".into(),
            id: Uuid::new_v4(),
        };
        let parsed = NameCursor::parse(&c.encode()).expect("roundtrip");
        assert_eq!(parsed, c);
    }

    #[test]
    fn name_roundtrip_with_pipe_in_value() {
        let c = NameCursor {
            sort_name: "weird|name|with|pipes".into(),
            id: Uuid::new_v4(),
        };
        let parsed = NameCursor::parse(&c.encode()).expect("roundtrip");
        assert_eq!(parsed, c);
    }

    #[test]
    fn name_rejects_recency_cursor() {
        let ts = OffsetDateTime::parse("2026-04-21T09:30:00Z", &Rfc3339).unwrap();
        let recency = Cursor {
            created_at: ts,
            id: Uuid::new_v4(),
        }
        .encode()
        .expect("encode");
        // A recency cursor's first segment is an RFC 3339 timestamp,
        // never the literal `n` tag.
        assert!(matches!(
            NameCursor::parse(&recency),
            Err(CursorError::UnknownTag)
        ));
    }

    #[test]
    fn recency_rejects_name_cursor() {
        let name = NameCursor {
            sort_name: "x".into(),
            id: Uuid::new_v4(),
        }
        .encode();
        // `n` is not a parsable timestamp, so the recency parser
        // refuses the name cursor rather than walking the wrong keyspace.
        assert!(matches!(
            Cursor::parse(&name),
            Err(CursorError::InvalidTimestamp)
        ));
    }
}
