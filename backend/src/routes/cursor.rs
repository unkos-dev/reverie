//! Sort-aware base64url pagination cursor for the JSON library API.
//!
//! Unlike the OPDS module's [`crate::routes::opds::cursor::Cursor`] —
//! which is hard-wired to `(created_at, id)` because OPDS feeds only
//! ever sort by recency — the JSON `/api/books` surface lets clients
//! pick a sort axis (`recent` | `title` | `author`). The cursor key
//! therefore has to carry both the boundary value *and* which axis
//! produced it, so that a client cannot replay a `sort=recent`
//! cursor against `sort=title` and get a different page of rows than
//! they expected.
//!
//! Wire encoding: base64url(unpadded) over `<tag>|<key>`, where
//! `<tag>` is a single byte (`r` | `t` | `a`) identifying the
//! [`CursorKey`] variant and `<key>` is the variant's textual key
//! (`<rfc3339>|<uuid>` for `Recent`, `<value>|<uuid>` for `Title` /
//! `Author`). Mismatched-variant cursors (e.g. `?sort=title` with a
//! cursor carrying the `r` tag) are rejected with
//! [`CursorError::SortMismatch`] so callers cannot confuse the
//! server about which key space they're walking.
//!
//! No HMAC — same trust model as the OPDS cursor.

use base64ct::{Base64UrlUnpadded, Encoding};
use serde::Deserialize;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use uuid::Uuid;

/// Sort axis selectable via `?sort=...` on `/api/books`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SortMode {
    /// Newest manifestations first (`manifestations.created_at DESC`).
    #[default]
    Recent,
    /// Alphabetical by work sort title (`works.sort_title ASC`).
    Title,
    /// Alphabetical by the first author's sort name.
    Author,
}

/// Cursor key tagged with the sort axis that produced it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CursorKey {
    /// Recent-sort cursor: `(manifestations.created_at, manifestations.id)`.
    Recent {
        /// Boundary row's `manifestations.created_at`.
        created_at: OffsetDateTime,
        /// Tie-breaker `manifestations.id`.
        id: Uuid,
    },
    /// Title-sort cursor: `(works.sort_title, works.id)`.
    Title {
        /// Boundary work's `sort_title`.
        sort_title: String,
        /// Tie-breaker `works.id`.
        id: Uuid,
    },
    /// Author-sort cursor: `(authors.sort_name, works.id)` of the
    /// first author (`work_authors.position = 0`).
    ///
    /// `sort_name` is `Option<String>` because the ORDER BY uses
    /// `NULLS LAST`: works without authors (pre-enrichment stubs)
    /// cluster at the tail of the sort, and the cursor predicate has
    /// to distinguish "advance through the non-NULL bucket" from
    /// "advance through the NULL bucket" — encoding NULL as `""`
    /// collapses the two and silently drops rows under three-valued
    /// SQL comparison.
    Author {
        /// Boundary first-author's `sort_name`; `None` when the
        /// boundary row has no first author (NULL bucket).
        sort_name: Option<String>,
        /// Tie-breaker `works.id`.
        id: Uuid,
    },
}

/// Parse failures from [`CursorKey::parse_for`].
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum CursorError {
    /// Input was not valid base64url.
    #[error("invalid base64url")]
    InvalidBase64,
    /// Decoded payload was not valid UTF-8.
    #[error("invalid utf-8")]
    InvalidUtf8,
    /// Decoded payload had no `<tag>|<key>` split.
    #[error("missing tag delimiter")]
    MissingDelimiter,
    /// Tag byte didn't match any [`CursorKey`] variant.
    #[error("unknown tag byte")]
    UnknownTag,
    /// Decoded key half had the wrong shape for its tag (missing `|`
    /// between value and uuid, or a malformed timestamp / uuid).
    #[error("malformed key")]
    MalformedKey,
    /// Cursor tag did not match the sort axis the request asked for
    /// (e.g. `?sort=title` with a recent-tagged cursor).
    #[error("cursor sort mismatch")]
    SortMismatch,
}

impl CursorKey {
    /// Encode this cursor key as a base64url-unpadded string suitable
    /// for use in a `?cursor=` query parameter.
    pub fn encode(&self) -> String {
        let payload = match self {
            Self::Recent { created_at, id } => {
                #[allow(
                    clippy::expect_used,
                    reason = "OffsetDateTime sourced from Postgres always formats as RFC 3339; year-out-of-range is the only failure and cannot occur for DB-stored timestamps"
                )]
                let ts = created_at
                    .format(&Rfc3339)
                    .expect("OffsetDateTime always formats as Rfc3339");
                format!("r|{ts}|{}", id.as_hyphenated())
            }
            Self::Title { sort_title, id } => {
                format!("t|{sort_title}|{}", id.as_hyphenated())
            }
            // Author cursors carry a sub-tag (`s` = some / `n` = none)
            // so the NULL-bucket boundary survives base64 round-trip.
            Self::Author {
                sort_name: Some(value),
                id,
            } => {
                format!("a|s|{value}|{}", id.as_hyphenated())
            }
            Self::Author {
                sort_name: None,
                id,
            } => {
                format!("a|n|{}", id.as_hyphenated())
            }
        };
        Base64UrlUnpadded::encode_string(payload.as_bytes())
    }

    /// Parse a base64url cursor and assert its tag matches `sort`.
    ///
    /// # Errors
    ///
    /// Returns the matching [`CursorError`] variant for bad base64,
    /// non-UTF-8 bytes, malformed key shape, unknown tag bytes, or a
    /// tag/sort mismatch (`?sort=title` with a recent-tagged cursor).
    pub fn parse_for(s: &str, sort: SortMode) -> Result<Self, CursorError> {
        let mut buf = vec![0u8; s.len()];
        let decoded = Base64UrlUnpadded::decode(s.as_bytes(), &mut buf)
            .map_err(|_| CursorError::InvalidBase64)?;
        let decoded_str = std::str::from_utf8(decoded).map_err(|_| CursorError::InvalidUtf8)?;
        let (tag, rest) = decoded_str
            .split_once('|')
            .ok_or(CursorError::MissingDelimiter)?;
        let key = match tag {
            "r" => {
                if sort != SortMode::Recent {
                    return Err(CursorError::SortMismatch);
                }
                let (ts, uuid) = rest.split_once('|').ok_or(CursorError::MalformedKey)?;
                let created_at =
                    OffsetDateTime::parse(ts, &Rfc3339).map_err(|_| CursorError::MalformedKey)?;
                let id = Uuid::parse_str(uuid).map_err(|_| CursorError::MalformedKey)?;
                Self::Recent { created_at, id }
            }
            "t" => {
                if sort != SortMode::Title {
                    return Err(CursorError::SortMismatch);
                }
                let (value, uuid) = rest.rsplit_once('|').ok_or(CursorError::MalformedKey)?;
                let id = Uuid::parse_str(uuid).map_err(|_| CursorError::MalformedKey)?;
                Self::Title {
                    sort_title: value.to_owned(),
                    id,
                }
            }
            "a" => {
                if sort != SortMode::Author {
                    return Err(CursorError::SortMismatch);
                }
                let (sub_tag, sub_rest) = rest.split_once('|').ok_or(CursorError::MalformedKey)?;
                match sub_tag {
                    "s" => {
                        let (value, uuid) =
                            sub_rest.rsplit_once('|').ok_or(CursorError::MalformedKey)?;
                        let id = Uuid::parse_str(uuid).map_err(|_| CursorError::MalformedKey)?;
                        Self::Author {
                            sort_name: Some(value.to_owned()),
                            id,
                        }
                    }
                    "n" => {
                        let id =
                            Uuid::parse_str(sub_rest).map_err(|_| CursorError::MalformedKey)?;
                        Self::Author {
                            sort_name: None,
                            id,
                        }
                    }
                    _ => return Err(CursorError::MalformedKey),
                }
            }
            _ => return Err(CursorError::UnknownTag),
        };
        Ok(key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_roundtrip() {
        let ts = OffsetDateTime::parse("2026-05-22T09:30:00Z", &Rfc3339).unwrap();
        let id = Uuid::new_v4();
        let key = CursorKey::Recent { created_at: ts, id };
        let encoded = key.encode();
        let parsed = CursorKey::parse_for(&encoded, SortMode::Recent).expect("roundtrip");
        assert_eq!(parsed, key);
    }

    #[test]
    fn title_roundtrip() {
        let id = Uuid::new_v4();
        let key = CursorKey::Title {
            sort_title: "neuromancer".into(),
            id,
        };
        let encoded = key.encode();
        let parsed = CursorKey::parse_for(&encoded, SortMode::Title).expect("roundtrip");
        assert_eq!(parsed, key);
    }

    #[test]
    fn author_roundtrip_some() {
        let id = Uuid::new_v4();
        let key = CursorKey::Author {
            sort_name: Some("gibson, william".into()),
            id,
        };
        let encoded = key.encode();
        let parsed = CursorKey::parse_for(&encoded, SortMode::Author).expect("roundtrip");
        assert_eq!(parsed, key);
    }

    #[test]
    fn author_roundtrip_none() {
        let id = Uuid::new_v4();
        let key = CursorKey::Author {
            sort_name: None,
            id,
        };
        let encoded = key.encode();
        let parsed = CursorKey::parse_for(&encoded, SortMode::Author).expect("roundtrip");
        assert_eq!(parsed, key);
    }

    #[test]
    fn author_roundtrip_with_pipe_in_value() {
        // `|` is the encoding delimiter; rsplit_once on the trailing
        // uuid must peel correctly even when the sort_name contains
        // pipes.
        let id = Uuid::new_v4();
        let key = CursorKey::Author {
            sort_name: Some("weird|name|with|pipes".into()),
            id,
        };
        let encoded = key.encode();
        let parsed = CursorKey::parse_for(&encoded, SortMode::Author).expect("roundtrip");
        assert_eq!(parsed, key);
    }

    #[test]
    fn rejects_cross_sort_replay() {
        let ts = OffsetDateTime::parse("2026-05-22T09:30:00Z", &Rfc3339).unwrap();
        let id = Uuid::new_v4();
        let recent = CursorKey::Recent { created_at: ts, id }.encode();
        assert!(matches!(
            CursorKey::parse_for(&recent, SortMode::Title),
            Err(CursorError::SortMismatch)
        ));
        let title = CursorKey::Title {
            sort_title: "x".into(),
            id,
        }
        .encode();
        assert!(matches!(
            CursorKey::parse_for(&title, SortMode::Recent),
            Err(CursorError::SortMismatch)
        ));
    }

    #[test]
    fn rejects_garbage_base64() {
        assert!(matches!(
            CursorKey::parse_for("!!!not-b64!!!", SortMode::Recent),
            Err(CursorError::InvalidBase64)
        ));
    }

    #[test]
    fn rejects_unknown_tag() {
        let encoded =
            Base64UrlUnpadded::encode_string(b"z|whatever|550e8400-e29b-41d4-a716-446655440000");
        assert!(matches!(
            CursorKey::parse_for(&encoded, SortMode::Recent),
            Err(CursorError::UnknownTag)
        ));
    }
}
