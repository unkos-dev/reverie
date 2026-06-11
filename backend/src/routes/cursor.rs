//! Sort-aware base64url pagination cursors for the JSON API surface.
//!
//! Unlike the OPDS module's [`crate::routes::opds::cursor::Cursor`] —
//! which is hard-wired to `(created_at, id)` because OPDS feeds only
//! ever sort by recency — the JSON `/api/v1/books` surface lets clients
//! pick a sort axis (`recent` | `title` | `author`). The cursor key
//! therefore has to carry both the boundary value *and* which axis
//! produced it, so that a client cannot replay a `sort=recent`
//! cursor against `sort=title` and get a different page of rows than
//! they expected.
//!
//! Wire encoding: base64url(unpadded) over `<tag>|<key>`, where
//! `<tag>` is a short ASCII tag identifying the cursor type and
//! `<key>` is its textual key. [`crate::routes::cursor::CursorKey`]
//! uses the single-byte tags `r` | `t` | `a`:
//!
//! - `Recent`: `<rfc3339>|<manifestation_id>`
//! - `Title`:  `<sort_title>|<work_id>|<manifestation_id>`
//! - `Author`: `s|<sort_name>|<work_id>|<manifestation_id>`
//!   (non-NULL bucket) or `n|<work_id>|<manifestation_id>` (NULL
//!   bucket — pre-enrichment stubs)
//!
//! The `manifestation_id` final tiebreaker on `Title` and `Author`
//! exists because `(sort_title, work_id)` and
//! `(sort_name, work_id)` are not unique — a single work can carry
//! several manifestations sharing the work-level sort key.
//!
//! Mismatched-variant cursors (e.g. `?sort=title` with a cursor
//! carrying the `r` tag) are rejected with
//! [`crate::routes::cursor::CursorError::SortMismatch`] so callers
//! cannot confuse the server about which key space they're walking.
//!
//! The shelves surface (UNK-374) adds two fixed-shape cursors in the
//! same encoding family, distinguished by their own tag bytes so a
//! books cursor cannot be replayed against a shelves endpoint (and
//! vice versa):
//!
//! - [`crate::routes::cursor::ShelfCursor`] (`sh` tag): `(is_system,
//!   name, id)` boundary for `GET /api/v1/shelves`.
//! - [`crate::routes::cursor::ShelfItemCursor`] (`si` tag):
//!   `(position, added_at, manifestation_id)` boundary for
//!   `GET /api/v1/shelves/{id}` items.
//!
//! No HMAC — same trust model as the OPDS cursor.

use base64ct::{Base64UrlUnpadded, Encoding};
use serde::Deserialize;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use uuid::Uuid;

/// Sort axis selectable via `?sort=...` on `/api/v1/books`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
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
#[non_exhaustive]
pub enum CursorKey {
    /// Recent-sort cursor: `(manifestations.created_at, manifestations.id)`.
    Recent {
        /// Boundary row's `manifestations.created_at`.
        created_at: OffsetDateTime,
        /// Tie-breaker `manifestations.id`.
        id: Uuid,
    },
    /// Title-sort cursor:
    /// `(works.sort_title, works.id, manifestations.id)`.
    ///
    /// The `(sort_title, work_id)` pair is not unique — a single
    /// work can carry multiple manifestations (epub + pdf of the
    /// same edition, format-priority duplicates). Two such rows
    /// share their sort key, so the `manifestation_id` third
    /// tiebreaker is required to keep cursor pagination total: a
    /// page boundary falling between them would otherwise drop the
    /// second row from page N+1.
    Title {
        /// Boundary work's `sort_title`.
        sort_title: String,
        /// Boundary `works.id`.
        work_id: Uuid,
        /// Final-tiebreaker `manifestations.id` (manifestation
        /// primary key — always unique per row).
        manifestation_id: Uuid,
    },
    /// Author-sort cursor:
    /// `(authors.sort_name, works.id, manifestations.id)` of the
    /// first author (`work_authors.position = 0`).
    ///
    /// `sort_name` is `Option<String>` because the ORDER BY uses
    /// `NULLS LAST`: works without authors (pre-enrichment stubs)
    /// cluster at the tail of the sort, and the cursor predicate has
    /// to distinguish "advance through the non-NULL bucket" from
    /// "advance through the NULL bucket" — encoding NULL as `""`
    /// collapses the two and silently drops rows under three-valued
    /// SQL comparison. The `manifestation_id` tiebreaker exists for
    /// the same reason as [`Self::Title`] — same work, multiple
    /// manifestations share a sort key.
    Author {
        /// Boundary first-author's `sort_name`; `None` when the
        /// boundary row has no first author (NULL bucket).
        sort_name: Option<String>,
        /// Boundary `works.id`.
        work_id: Uuid,
        /// Final-tiebreaker `manifestations.id`.
        manifestation_id: Uuid,
    },
}

/// Parse and encode failures for [`CursorKey`].
///
/// Parsing ([`CursorKey::parse_for`]) yields the input-shape variants;
/// encoding ([`CursorKey::encode`]) yields [`Self::FormatTimestamp`].
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
    /// A `Recent` cursor's `created_at` had a year outside RFC 3339's
    /// representable range (`-9999..=9999`) during encode.
    #[error("timestamp not representable as RFC 3339")]
    FormatTimestamp(#[from] time::error::Format),
}

impl CursorKey {
    /// Encode this cursor key as a base64url-unpadded string suitable
    /// for use in a `?cursor=` query parameter.
    ///
    /// # Errors
    ///
    /// Returns [`CursorError::FormatTimestamp`] if a [`Self::Recent`]
    /// cursor's `created_at` has a year outside RFC 3339's
    /// `-9999..=9999` range. Rust-side `OffsetDateTime` caps years at
    /// 9999, so this is unreachable for timestamps written through
    /// Reverie; it can only trigger for a `TIMESTAMPTZ` mutated
    /// out-of-band (raw `psql`, a migration) to a year > 9999. The
    /// non-timestamp variants never fail.
    pub fn encode(&self) -> Result<String, CursorError> {
        let payload = match self {
            Self::Recent { created_at, id } => {
                let ts = created_at.format(&Rfc3339)?;
                format!("r|{ts}|{}", id.as_hyphenated())
            }
            Self::Title {
                sort_title,
                work_id,
                manifestation_id,
            } => {
                format!(
                    "t|{sort_title}|{}|{}",
                    work_id.as_hyphenated(),
                    manifestation_id.as_hyphenated()
                )
            }
            // Author cursors carry a sub-tag (`s` = some / `n` = none)
            // so the NULL-bucket boundary survives base64 round-trip.
            Self::Author {
                sort_name: Some(value),
                work_id,
                manifestation_id,
            } => {
                format!(
                    "a|s|{value}|{}|{}",
                    work_id.as_hyphenated(),
                    manifestation_id.as_hyphenated()
                )
            }
            Self::Author {
                sort_name: None,
                work_id,
                manifestation_id,
            } => {
                format!(
                    "a|n|{}|{}",
                    work_id.as_hyphenated(),
                    manifestation_id.as_hyphenated()
                )
            }
        };
        Ok(Base64UrlUnpadded::encode_string(payload.as_bytes()))
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
                let (head, manifestation_str) =
                    rest.rsplit_once('|').ok_or(CursorError::MalformedKey)?;
                let (value, work_str) = head.rsplit_once('|').ok_or(CursorError::MalformedKey)?;
                let work_id = Uuid::parse_str(work_str).map_err(|_| CursorError::MalformedKey)?;
                let manifestation_id =
                    Uuid::parse_str(manifestation_str).map_err(|_| CursorError::MalformedKey)?;
                Self::Title {
                    sort_title: value.to_owned(),
                    work_id,
                    manifestation_id,
                }
            }
            "a" => {
                if sort != SortMode::Author {
                    return Err(CursorError::SortMismatch);
                }
                let (sub_tag, sub_rest) = rest.split_once('|').ok_or(CursorError::MalformedKey)?;
                match sub_tag {
                    "s" => {
                        let (head, manifestation_str) =
                            sub_rest.rsplit_once('|').ok_or(CursorError::MalformedKey)?;
                        let (value, work_str) =
                            head.rsplit_once('|').ok_or(CursorError::MalformedKey)?;
                        let work_id =
                            Uuid::parse_str(work_str).map_err(|_| CursorError::MalformedKey)?;
                        let manifestation_id = Uuid::parse_str(manifestation_str)
                            .map_err(|_| CursorError::MalformedKey)?;
                        Self::Author {
                            sort_name: Some(value.to_owned()),
                            work_id,
                            manifestation_id,
                        }
                    }
                    "n" => {
                        let (work_str, manifestation_str) =
                            sub_rest.rsplit_once('|').ok_or(CursorError::MalformedKey)?;
                        let work_id =
                            Uuid::parse_str(work_str).map_err(|_| CursorError::MalformedKey)?;
                        let manifestation_id = Uuid::parse_str(manifestation_str)
                            .map_err(|_| CursorError::MalformedKey)?;
                        Self::Author {
                            sort_name: None,
                            work_id,
                            manifestation_id,
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

/// Keyset boundary for `GET /api/v1/shelves` (UNK-374).
///
/// Carries the `(is_system, name, id)` triple matching the endpoint's
/// `ORDER BY is_system DESC, name ASC, id ASC`. The `id` tiebreaker is
/// required because `(user_id, is_system, name)` carries no uniqueness
/// constraint — two shelves may share a name.
///
/// Wire encoding: base64url(unpadded) over `sh|<t/f>|<name>|<uuid>`.
/// `name` is user-controlled and may contain `|`; the parser peels the
/// fixed-shape head (`sh`, the `is_system` flag) with `split_once` and
/// the trailing uuid with `rsplit_once`, so pipes inside the name
/// survive the round-trip (same strategy as [`CursorKey::Title`]).
///
/// # Keyset predicate
///
/// The sort is mixed-direction (`is_system` DESC, the rest ASC), so a
/// single row-tuple comparison against this cursor is WRONG — it would
/// silently return rows from the wrong side of the `is_system` flip.
/// Consumers must expand into the two-arm OR:
///
/// ```sql
/// (is_system < $1 OR (is_system = $1 AND (name, id) > ($2, $3)))
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShelfCursor {
    /// Boundary shelf's `is_system` flag (sorted DESC: system first).
    pub is_system: bool,
    /// Boundary shelf's `name`.
    pub name: String,
    /// Tiebreaker `shelves.id`.
    pub id: Uuid,
}

impl ShelfCursor {
    /// Encode as a base64url-unpadded `?cursor=` value.
    pub fn encode(&self) -> String {
        let flag = if self.is_system { "t" } else { "f" };
        let payload = format!("sh|{flag}|{}|{}", self.name, self.id.as_hyphenated());
        Base64UrlUnpadded::encode_string(payload.as_bytes())
    }

    /// Parse a base64url cursor produced by [`Self::encode`].
    ///
    /// # Errors
    ///
    /// Returns the matching [`CursorError`] variant for bad base64,
    /// non-UTF-8 bytes, a missing delimiter, a foreign tag byte (e.g.
    /// a books-list cursor replayed against the shelves endpoint), or
    /// a malformed flag / uuid.
    pub fn parse(s: &str) -> Result<Self, CursorError> {
        let mut buf = vec![0u8; s.len()];
        let decoded = Base64UrlUnpadded::decode(s.as_bytes(), &mut buf)
            .map_err(|_| CursorError::InvalidBase64)?;
        let decoded_str = std::str::from_utf8(decoded).map_err(|_| CursorError::InvalidUtf8)?;
        let (tag, rest) = decoded_str
            .split_once('|')
            .ok_or(CursorError::MissingDelimiter)?;
        if tag != "sh" {
            return Err(CursorError::UnknownTag);
        }
        let (flag, rest) = rest.split_once('|').ok_or(CursorError::MalformedKey)?;
        let is_system = match flag {
            "t" => true,
            "f" => false,
            _ => return Err(CursorError::MalformedKey),
        };
        let (name, id_str) = rest.rsplit_once('|').ok_or(CursorError::MalformedKey)?;
        let id = Uuid::parse_str(id_str).map_err(|_| CursorError::MalformedKey)?;
        Ok(Self {
            is_system,
            name: name.to_owned(),
            id,
        })
    }
}

/// Keyset boundary for the items page of `GET /api/v1/shelves/{id}`
/// (UNK-374).
///
/// Carries `(position, added_at, manifestation_id)` matching the items
/// query's `ORDER BY position ASC, added_at ASC, manifestation_id ASC`.
/// Neither `position` nor `added_at` is unique per shelf (the table's
/// only unique key is the `(shelf_id, manifestation_id)` PK), so the
/// `manifestation_id` final tiebreaker is what keeps the walk total —
/// without it a page boundary between two same-position rows would
/// drop one.
///
/// Wire encoding: base64url(unpadded) over
/// `si|<position>|<rfc3339>|<uuid>` — no free-text field, plain splits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShelfItemCursor {
    /// Boundary item's `shelf_items.position`.
    pub position: i32,
    /// Boundary item's `shelf_items.added_at`.
    pub added_at: OffsetDateTime,
    /// Tiebreaker `shelf_items.manifestation_id`.
    pub manifestation_id: Uuid,
}

impl ShelfItemCursor {
    /// Encode as a base64url-unpadded `?cursor=` value.
    ///
    /// # Errors
    ///
    /// Returns [`CursorError::FormatTimestamp`] if `added_at` has a
    /// year outside RFC 3339's `-9999..=9999` range (out-of-band DB
    /// mutation only — see [`CursorKey::encode`]).
    pub fn encode(&self) -> Result<String, CursorError> {
        let ts = self.added_at.format(&Rfc3339)?;
        let payload = format!(
            "si|{}|{ts}|{}",
            self.position,
            self.manifestation_id.as_hyphenated()
        );
        Ok(Base64UrlUnpadded::encode_string(payload.as_bytes()))
    }

    /// Parse a base64url cursor produced by [`Self::encode`].
    ///
    /// # Errors
    ///
    /// Returns the matching [`CursorError`] variant for bad base64,
    /// non-UTF-8 bytes, a missing delimiter, a foreign tag byte, or a
    /// malformed position / timestamp / uuid.
    pub fn parse(s: &str) -> Result<Self, CursorError> {
        let mut buf = vec![0u8; s.len()];
        let decoded = Base64UrlUnpadded::decode(s.as_bytes(), &mut buf)
            .map_err(|_| CursorError::InvalidBase64)?;
        let decoded_str = std::str::from_utf8(decoded).map_err(|_| CursorError::InvalidUtf8)?;
        let (tag, rest) = decoded_str
            .split_once('|')
            .ok_or(CursorError::MissingDelimiter)?;
        if tag != "si" {
            return Err(CursorError::UnknownTag);
        }
        let (pos_str, rest) = rest.split_once('|').ok_or(CursorError::MalformedKey)?;
        let position: i32 = pos_str.parse().map_err(|_| CursorError::MalformedKey)?;
        let (ts, id_str) = rest.split_once('|').ok_or(CursorError::MalformedKey)?;
        let added_at =
            OffsetDateTime::parse(ts, &Rfc3339).map_err(|_| CursorError::MalformedKey)?;
        let manifestation_id = Uuid::parse_str(id_str).map_err(|_| CursorError::MalformedKey)?;
        Ok(Self {
            position,
            added_at,
            manifestation_id,
        })
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
        let encoded = key.encode().expect("encode");
        let parsed = CursorKey::parse_for(&encoded, SortMode::Recent).expect("roundtrip");
        assert_eq!(parsed, key);
    }

    #[test]
    fn title_roundtrip() {
        let key = CursorKey::Title {
            sort_title: "neuromancer".into(),
            work_id: Uuid::new_v4(),
            manifestation_id: Uuid::new_v4(),
        };
        let encoded = key.encode().expect("encode");
        let parsed = CursorKey::parse_for(&encoded, SortMode::Title).expect("roundtrip");
        assert_eq!(parsed, key);
    }

    #[test]
    fn author_roundtrip_some() {
        let key = CursorKey::Author {
            sort_name: Some("gibson, william".into()),
            work_id: Uuid::new_v4(),
            manifestation_id: Uuid::new_v4(),
        };
        let encoded = key.encode().expect("encode");
        let parsed = CursorKey::parse_for(&encoded, SortMode::Author).expect("roundtrip");
        assert_eq!(parsed, key);
    }

    #[test]
    fn author_roundtrip_none() {
        let key = CursorKey::Author {
            sort_name: None,
            work_id: Uuid::new_v4(),
            manifestation_id: Uuid::new_v4(),
        };
        let encoded = key.encode().expect("encode");
        let parsed = CursorKey::parse_for(&encoded, SortMode::Author).expect("roundtrip");
        assert_eq!(parsed, key);
    }

    #[test]
    fn author_roundtrip_with_pipe_in_value() {
        // `|` is the encoding delimiter; the parser must peel the
        // two trailing UUIDs by `rsplit_once` *twice* before the
        // remainder is treated as `sort_name`, so pipes inside the
        // sort_name survive the round-trip.
        let key = CursorKey::Author {
            sort_name: Some("weird|name|with|pipes".into()),
            work_id: Uuid::new_v4(),
            manifestation_id: Uuid::new_v4(),
        };
        let encoded = key.encode().expect("encode");
        let parsed = CursorKey::parse_for(&encoded, SortMode::Author).expect("roundtrip");
        assert_eq!(parsed, key);
    }

    #[test]
    fn rejects_cross_sort_replay() {
        let ts = OffsetDateTime::parse("2026-05-22T09:30:00Z", &Rfc3339).unwrap();
        let id = Uuid::new_v4();
        let recent = CursorKey::Recent { created_at: ts, id }
            .encode()
            .expect("encode");
        assert!(matches!(
            CursorKey::parse_for(&recent, SortMode::Title),
            Err(CursorError::SortMismatch)
        ));
        let title = CursorKey::Title {
            sort_title: "x".into(),
            work_id: id,
            manifestation_id: id,
        }
        .encode()
        .expect("encode");
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

    #[test]
    fn shelf_roundtrip() {
        let key = ShelfCursor {
            is_system: true,
            name: "Reading Now".into(),
            id: Uuid::new_v4(),
        };
        let parsed = ShelfCursor::parse(&key.encode()).expect("roundtrip");
        assert_eq!(parsed, key);
    }

    #[test]
    fn shelf_roundtrip_with_pipe_in_name() {
        let key = ShelfCursor {
            is_system: false,
            name: "weird|shelf|name".into(),
            id: Uuid::new_v4(),
        };
        let parsed = ShelfCursor::parse(&key.encode()).expect("roundtrip");
        assert_eq!(parsed, key);
    }

    #[test]
    fn shelf_rejects_foreign_tags() {
        let ts = OffsetDateTime::parse("2026-05-22T09:30:00Z", &Rfc3339).unwrap();
        let id = Uuid::new_v4();
        let books = CursorKey::Recent { created_at: ts, id }
            .encode()
            .expect("encode");
        assert!(matches!(
            ShelfCursor::parse(&books),
            Err(CursorError::UnknownTag)
        ));
        let item = ShelfItemCursor {
            position: 3,
            added_at: ts,
            manifestation_id: id,
        }
        .encode()
        .expect("encode");
        assert!(matches!(
            ShelfCursor::parse(&item),
            Err(CursorError::UnknownTag)
        ));
    }

    #[test]
    fn shelf_rejects_malformed_flag() {
        let encoded =
            Base64UrlUnpadded::encode_string(b"sh|x|name|550e8400-e29b-41d4-a716-446655440000");
        assert!(matches!(
            ShelfCursor::parse(&encoded),
            Err(CursorError::MalformedKey)
        ));
    }

    #[test]
    fn shelf_item_roundtrip() {
        let ts = OffsetDateTime::parse("2026-05-22T09:30:00Z", &Rfc3339).unwrap();
        let key = ShelfItemCursor {
            position: 7,
            added_at: ts,
            manifestation_id: Uuid::new_v4(),
        };
        let encoded = key.encode().expect("encode");
        let parsed = ShelfItemCursor::parse(&encoded).expect("roundtrip");
        assert_eq!(parsed, key);
    }

    #[test]
    fn shelf_item_rejects_garbage() {
        assert!(matches!(
            ShelfItemCursor::parse("!!!not-b64!!!"),
            Err(CursorError::InvalidBase64)
        ));
        let bad_pos = Base64UrlUnpadded::encode_string(
            b"si|notanint|2026-05-22T09:30:00Z|550e8400-e29b-41d4-a716-446655440000",
        );
        assert!(matches!(
            ShelfItemCursor::parse(&bad_pos),
            Err(CursorError::MalformedKey)
        ));
        let shelf = ShelfCursor {
            is_system: false,
            name: "x".into(),
            id: Uuid::new_v4(),
        }
        .encode();
        assert!(matches!(
            ShelfItemCursor::parse(&shelf),
            Err(CursorError::UnknownTag)
        ));
    }
}
