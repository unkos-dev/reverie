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
use chrono::{DateTime, SecondsFormat, Utc};
use uuid::Uuid;

/// `(created_at, id)` tuple identifying the boundary row of a feed page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cursor {
    /// Boundary row's `manifestations.created_at`.
    pub created_at: DateTime<Utc>,
    /// Boundary row's `manifestations.id` (tie-breaker for identical
    /// `created_at`).
    pub id: Uuid,
}

/// Parse failures for [`Cursor`]. Encoding ([`Cursor::encode`]) is
/// infallible.
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
}

impl Cursor {
    /// Encode this cursor as a base64url-encoded `<rfc3339>|<uuid>`
    /// string suitable for use in a feed `?cursor=` query parameter.
    ///
    /// The timestamp is written with the same RFC 3339 spelling serde
    /// gives a `DateTime<Utc>` on the wire, so a cursor timestamp and a
    /// feed-entry timestamp for the same instant are byte-identical.
    #[must_use]
    pub fn encode(&self) -> String {
        let ts = self.created_at.to_rfc3339_opts(SecondsFormat::AutoSi, true);
        let payload = format!("{ts}|{}", self.id.as_hyphenated());
        Base64UrlUnpadded::encode_string(payload.as_bytes())
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
        let created_at = DateTime::parse_from_rfc3339(ts)
            .map_err(|_| CursorError::InvalidTimestamp)?
            .with_timezone(&Utc);
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
    #[must_use]
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
