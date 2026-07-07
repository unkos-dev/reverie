//! Sort-aware base64url pagination cursors for the JSON API surface.
//!
//! Unlike the OPDS module's [`crate::routes::opds::cursor::Cursor`],
//! which is hard-wired to `(created_at, id)` because OPDS feeds only
//! ever sort by recency, the JSON `/api/v1/books` surface accepts a
//! multi-level sort stack ([`crate::routes::sort_spec::SortSpec`]).
//! The books cursor therefore has to carry every level's boundary
//! value *and* the canonical spec string that produced them, so that
//! a client cannot replay a cursor minted under one stack against
//! another and get a different page of rows than they expected.
//!
//! Wire encoding: base64url(unpadded) over `<tag>|<key>`, where
//! `<tag>` is a short ASCII tag identifying the cursor type and
//! `<key>` is its textual key.
//!
//! [`crate::routes::cursor::SortCursor`] (`m` tag) carries a JSON
//! payload rather than pipe-delimited fields: a sort stack can hold
//! several free-text boundary values (title and author sort keys),
//! which makes delimiter-anchored splitting ambiguous, while JSON
//! brings escaping and typed values for free. The payload is the
//! canonical spec string, one typed boundary value per level
//! ([`crate::routes::cursor::CursorValue`], in level order), and the
//! `manifestations.id` final tiebreaker. The tiebreaker is what keeps
//! pagination total: work-level sort keys (`sort_title`,
//! `first_author_sort_name`) are shared by sibling manifestations of
//! a single work, and `created_at` / `pages` values can collide too.
//!
//! Cursors whose embedded spec differs from the request's validated
//! `?sort=` stack are rejected with
//! [`crate::routes::cursor::CursorError::SortMismatch`] so callers
//! cannot confuse the server about which key space they're walking.
//!
//! The shelves surface adds two fixed-shape cursors in the
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
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use uuid::Uuid;

use crate::routes::sort_spec::{SortSpec, SortValueKind};

/// One typed boundary value inside a [`SortCursor`], externally
/// tagged in the JSON payload so a forged cursor cannot smuggle a
/// value of the wrong SQL type past the spec check in
/// [`SortCursor::parse_for`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CursorValue {
    /// Text sort key (`sort_title`, `first_author_sort_name`).
    /// `None` marks a NULL-bucket boundary row: the ORDER BY uses
    /// `NULLS LAST`, and the cursor has to distinguish "advance
    /// through the non-NULL bucket" from "advance through the NULL
    /// bucket". Encoding NULL as `""` would collapse the two and
    /// silently drop rows under three-valued SQL comparison.
    Text(Option<String>),
    /// Timestamp sort key (`created_at`), RFC 3339 on the wire.
    Ts(#[serde(with = "time::serde::rfc3339")] OffsetDateTime),
    /// Integer sort key (`pages`); `None` marks a NULL-bucket
    /// boundary row, as with [`Self::Text`].
    Int(Option<i32>),
}

impl CursorValue {
    /// The [`SortValueKind`] this boundary value carries, matched against
    /// its level's [`crate::routes::sort_spec::SortColumn::value_kind`] so
    /// a forged cursor cannot smuggle a value of the wrong SQL type.
    const fn kind(&self) -> SortValueKind {
        match self {
            Self::Text(_) => SortValueKind::Text,
            Self::Ts(_) => SortValueKind::Timestamp,
            Self::Int(_) => SortValueKind::Int,
        }
    }

    /// Whether this value sits in a nullable column's NULLS LAST bucket.
    const fn is_null(&self) -> bool {
        matches!(self, Self::Text(None) | Self::Int(None))
    }
}

/// Keyset boundary for `GET /api/v1/books` under a validated sort
/// stack.
///
/// Wire encoding: base64url(unpadded) over `m|<json>`, where `<json>`
/// is the `serde_json` serialization of this struct. The embedded
/// `spec` string binds the cursor to the exact stack (columns,
/// directions, level order) it was minted under; [`Self::parse_for`]
/// rejects every other stack with [`CursorError::SortMismatch`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SortCursor {
    /// Canonical spec string
    /// ([`crate::routes::sort_spec::SortSpec::canonical`]) the cursor
    /// was minted under. Crate-private: mint through [`Self::for_spec`]
    /// so an internally inconsistent cursor cannot be built.
    pub(crate) spec: String,
    /// One boundary value per spec level, in level order.
    pub(crate) keys: Vec<CursorValue>,
    /// Final-tiebreaker `manifestations.id` (primary key, always
    /// unique per row).
    pub(crate) id: Uuid,
}

/// Parse and encode failures for the cursor family.
///
/// Parsing yields the input-shape variants; encoding yields
/// [`Self::FormatTimestamp`] (pipe-format cursors) or
/// [`Self::SerializePayload`] ([`SortCursor`]).
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
    /// Tag byte didn't match the cursor type being parsed.
    #[error("unknown tag byte")]
    UnknownTag,
    /// Decoded key half had the wrong shape for its tag (missing `|`
    /// between value and uuid, a malformed timestamp / uuid, or a
    /// [`SortCursor`] JSON payload that fails to decode or whose keys
    /// disagree with the spec's arity or column types).
    #[error("malformed key")]
    MalformedKey,
    /// Cursor's embedded sort spec did not match the sort stack the
    /// request asked for (e.g. `?sort=title` with a cursor minted
    /// under `?sort=-title`).
    #[error("cursor sort mismatch")]
    SortMismatch,
    /// A cursor timestamp had a year outside RFC 3339's representable
    /// range (`-9999..=9999`) during encode.
    #[error("timestamp not representable as RFC 3339")]
    FormatTimestamp(#[from] time::error::Format),
    /// A [`SortCursor`] payload failed JSON serialization during
    /// encode (a [`CursorValue::Ts`] year outside RFC 3339's
    /// representable range).
    #[error("cursor payload not serializable")]
    SerializePayload(#[from] serde_json::Error),
}

impl SortCursor {
    /// Encode as a base64url-unpadded `?cursor=` value.
    ///
    /// # Errors
    ///
    /// Returns [`CursorError::SerializePayload`] if a
    /// [`CursorValue::Ts`] key has a year outside RFC 3339's
    /// `-9999..=9999` range. Rust-side `OffsetDateTime` caps years at
    /// 9999, so this is unreachable for timestamps written through
    /// Reverie; it can only trigger for a `TIMESTAMPTZ` mutated
    /// out-of-band (raw `psql`, a migration) to a year > 9999. The
    /// non-timestamp values never fail.
    pub fn encode(&self) -> Result<String, CursorError> {
        let json = serde_json::to_string(self)?;
        Ok(Base64UrlUnpadded::encode_string(
            format!("m|{json}").as_bytes(),
        ))
    }

    /// Parse a base64url cursor and assert it was minted under `spec`.
    ///
    /// # Errors
    ///
    /// Returns the matching [`CursorError`] variant for bad base64,
    /// non-UTF-8 bytes, a missing delimiter, a foreign tag byte, an
    /// undecodable JSON payload, a key list whose arity or value
    /// types disagree with `spec`'s columns, or an embedded spec
    /// string minted under a different stack.
    pub fn parse_for(raw: &str, spec: &SortSpec) -> Result<Self, CursorError> {
        let mut buf = vec![0u8; raw.len()];
        let decoded = Base64UrlUnpadded::decode(raw.as_bytes(), &mut buf)
            .map_err(|_| CursorError::InvalidBase64)?;
        let decoded_str = std::str::from_utf8(decoded).map_err(|_| CursorError::InvalidUtf8)?;
        let (tag, rest) = decoded_str
            .split_once('|')
            .ok_or(CursorError::MissingDelimiter)?;
        if tag != "m" {
            return Err(CursorError::UnknownTag);
        }
        let cursor: Self = serde_json::from_str(rest).map_err(|_| CursorError::MalformedKey)?;
        if cursor.spec != spec.canonical() {
            return Err(CursorError::SortMismatch);
        }
        validate_keys(&cursor.keys, spec)?;
        Ok(cursor)
    }

    /// Build a cursor for `spec` on the mint path, validating that `keys`
    /// match the spec's arity, per-level value kinds, and null-legality
    /// before it can be encoded. Sharing [`validate_keys`] with
    /// [`Self::parse_for`] means a minted cursor can never be one its own
    /// decode would reject.
    ///
    /// # Errors
    ///
    /// Returns [`CursorError::MalformedKey`] if `keys` disagree with
    /// `spec`'s arity, a level's value kind, or place a NULL boundary
    /// under a non-nullable column.
    pub(crate) fn for_spec(
        spec: &SortSpec,
        keys: Vec<CursorValue>,
        id: Uuid,
    ) -> Result<Self, CursorError> {
        validate_keys(&keys, spec)?;
        Ok(Self {
            spec: spec.canonical(),
            keys,
            id,
        })
    }
}

/// Check a cursor key list against `spec`: one key per level, each key's
/// value kind matching its level's column, and a NULL boundary only under
/// a nullable column (a non-nullable column never yields NULL, so a cursor
/// claiming one is forged or corrupt and would hand the query assembler a
/// column whose cascade has no null branch). Shared by the decode path
/// ([`SortCursor::parse_for`]) and the encode path ([`SortCursor::for_spec`]).
fn validate_keys(keys: &[CursorValue], spec: &SortSpec) -> Result<(), CursorError> {
    let levels = spec.levels();
    if keys.len() != levels.len() {
        return Err(CursorError::MalformedKey);
    }
    for (key, level) in keys.iter().zip(levels) {
        if key.kind() != level.column.value_kind() {
            return Err(CursorError::MalformedKey);
        }
        if key.is_null() && !level.column.nullable() {
            return Err(CursorError::MalformedKey);
        }
    }
    Ok(())
}

/// Keyset boundary for `GET /api/v1/shelves`.
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
/// survive the round-trip.
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

/// Keyset boundary for the items page of `GET /api/v1/shelves/{id}`.
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
    /// mutation only; see [`SortCursor::encode`]).
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

    use crate::routes::sort_spec::SortSpec;

    fn spec_of(raw: &str) -> SortSpec {
        SortSpec::parse(raw).expect("valid spec")
    }

    fn cursor_under(spec: &SortSpec, keys: Vec<CursorValue>) -> SortCursor {
        SortCursor {
            spec: spec.canonical(),
            keys,
            id: Uuid::new_v4(),
        }
    }

    #[test]
    fn sort_roundtrip_ts() {
        let spec = SortSpec::default();
        let ts = OffsetDateTime::parse("2026-05-22T09:30:00Z", &Rfc3339).unwrap();
        let cursor = cursor_under(&spec, vec![CursorValue::Ts(ts)]);
        let encoded = cursor.encode().expect("encode");
        let parsed = SortCursor::parse_for(&encoded, &spec).expect("roundtrip");
        assert_eq!(parsed, cursor);
    }

    #[test]
    fn sort_roundtrip_text_some() {
        let spec = spec_of("title");
        let cursor = cursor_under(&spec, vec![CursorValue::Text(Some("neuromancer".into()))]);
        let encoded = cursor.encode().expect("encode");
        let parsed = SortCursor::parse_for(&encoded, &spec).expect("roundtrip");
        assert_eq!(parsed, cursor);
    }

    #[test]
    fn sort_roundtrip_text_none() {
        let spec = spec_of("author");
        let cursor = cursor_under(&spec, vec![CursorValue::Text(None)]);
        let encoded = cursor.encode().expect("encode");
        let parsed = SortCursor::parse_for(&encoded, &spec).expect("roundtrip");
        assert_eq!(parsed, cursor);
    }

    #[test]
    fn sort_roundtrip_int_some() {
        let spec = spec_of("-pages");
        let cursor = cursor_under(&spec, vec![CursorValue::Int(Some(466))]);
        let encoded = cursor.encode().expect("encode");
        let parsed = SortCursor::parse_for(&encoded, &spec).expect("roundtrip");
        assert_eq!(parsed, cursor);
    }

    #[test]
    fn sort_roundtrip_int_none() {
        let spec = spec_of("pages");
        let cursor = cursor_under(&spec, vec![CursorValue::Int(None)]);
        let encoded = cursor.encode().expect("encode");
        let parsed = SortCursor::parse_for(&encoded, &spec).expect("roundtrip");
        assert_eq!(parsed, cursor);
    }

    #[test]
    fn sort_roundtrip_three_levels() {
        let spec = spec_of("author,-created_at,title");
        let ts = OffsetDateTime::parse("2026-05-22T09:30:00Z", &Rfc3339).unwrap();
        let cursor = cursor_under(
            &spec,
            vec![
                CursorValue::Text(Some("gibson, william".into())),
                CursorValue::Ts(ts),
                CursorValue::Text(Some("neuromancer".into())),
            ],
        );
        let encoded = cursor.encode().expect("encode");
        let parsed = SortCursor::parse_for(&encoded, &spec).expect("roundtrip");
        assert_eq!(parsed, cursor);
    }

    #[test]
    fn sort_roundtrip_hostile_title() {
        // `|` collides with the tag delimiter, `,`/quotes with JSON
        // syntax; serde escaping must carry all of them through.
        let spec = spec_of("title");
        let hostile = "wei|rd, \"quoted\" \\ 本のタイトル".to_owned();
        let cursor = cursor_under(&spec, vec![CursorValue::Text(Some(hostile))]);
        let encoded = cursor.encode().expect("encode");
        let parsed = SortCursor::parse_for(&encoded, &spec).expect("roundtrip");
        assert_eq!(parsed, cursor);
    }

    #[test]
    fn rejects_spec_direction_mismatch() {
        let minted = spec_of("title");
        let cursor = cursor_under(&minted, vec![CursorValue::Text(Some("x".into()))]);
        let encoded = cursor.encode().expect("encode");
        assert!(matches!(
            SortCursor::parse_for(&encoded, &spec_of("-title")),
            Err(CursorError::SortMismatch)
        ));
    }

    #[test]
    fn rejects_spec_order_mismatch() {
        let minted = spec_of("title,author");
        let cursor = cursor_under(
            &minted,
            vec![
                CursorValue::Text(Some("x".into())),
                CursorValue::Text(Some("y".into())),
            ],
        );
        let encoded = cursor.encode().expect("encode");
        assert!(matches!(
            SortCursor::parse_for(&encoded, &spec_of("author,title")),
            Err(CursorError::SortMismatch)
        ));
    }

    #[test]
    fn rejects_spec_column_mismatch() {
        let minted = spec_of("title");
        let cursor = cursor_under(&minted, vec![CursorValue::Text(Some("x".into()))]);
        let encoded = cursor.encode().expect("encode");
        assert!(matches!(
            SortCursor::parse_for(&encoded, &spec_of("author")),
            Err(CursorError::SortMismatch)
        ));
    }

    #[test]
    fn rejects_wrong_arity() {
        let spec = spec_of("title,author");
        let cursor = cursor_under(&spec, vec![CursorValue::Text(Some("x".into()))]);
        let encoded = cursor.encode().expect("encode");
        assert!(matches!(
            SortCursor::parse_for(&encoded, &spec),
            Err(CursorError::MalformedKey)
        ));
    }

    #[test]
    fn rejects_wrong_value_type() {
        let spec = spec_of("title");
        let cursor = cursor_under(&spec, vec![CursorValue::Int(Some(3))]);
        let encoded = cursor.encode().expect("encode");
        assert!(matches!(
            SortCursor::parse_for(&encoded, &spec),
            Err(CursorError::MalformedKey)
        ));
    }

    #[test]
    fn rejects_null_boundary_for_non_nullable_column() {
        let spec = spec_of("title");
        let cursor = cursor_under(&spec, vec![CursorValue::Text(None)]);
        let encoded = cursor.encode().expect("encode");
        assert!(matches!(
            SortCursor::parse_for(&encoded, &spec),
            Err(CursorError::MalformedKey)
        ));
    }

    #[test]
    fn rejects_garbage_base64() {
        assert!(matches!(
            SortCursor::parse_for("!!!not-b64!!!", &SortSpec::default()),
            Err(CursorError::InvalidBase64)
        ));
    }

    #[test]
    fn rejects_malformed_json_payload() {
        let encoded = Base64UrlUnpadded::encode_string(b"m|not-json");
        assert!(matches!(
            SortCursor::parse_for(&encoded, &SortSpec::default()),
            Err(CursorError::MalformedKey)
        ));
    }

    #[test]
    fn rejects_legacy_tags() {
        let legacy: [&[u8]; 3] = [
            b"r|2026-05-22T09:30:00Z|550e8400-e29b-41d4-a716-446655440000",
            b"t|x|550e8400-e29b-41d4-a716-446655440000|550e8400-e29b-41d4-a716-446655440000",
            b"a|s|x|550e8400-e29b-41d4-a716-446655440000|550e8400-e29b-41d4-a716-446655440000",
        ];
        for payload in legacy {
            let encoded = Base64UrlUnpadded::encode_string(payload);
            assert!(matches!(
                SortCursor::parse_for(&encoded, &SortSpec::default()),
                Err(CursorError::UnknownTag)
            ));
        }
    }

    #[test]
    fn rejects_unknown_tag() {
        let encoded =
            Base64UrlUnpadded::encode_string(b"z|whatever|550e8400-e29b-41d4-a716-446655440000");
        assert!(matches!(
            SortCursor::parse_for(&encoded, &SortSpec::default()),
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
        let books = SortCursor {
            spec: SortSpec::default().canonical(),
            keys: vec![CursorValue::Ts(ts)],
            id,
        }
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
