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
//!   `(added_at, manifestation_id)` boundary for
//!   `GET /api/v1/shelves/{id}` items.
//!
//! No HMAC — same trust model as the OPDS cursor.

use base64ct::{Base64UrlUnpadded, Encoding};
use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::routes::sort_spec::{SortSpec, SortValueKind};

/// One typed boundary value inside a [`SortCursor`].
///
/// Externally tagged in the JSON payload so a forged cursor cannot smuggle a
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
    Ts(DateTime<Utc>),
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
    /// Fingerprint of the active filter set the cursor was minted under
    /// (a short hex digest supplied by the list handler). Binds the
    /// cursor to its filters exactly as `spec` binds it to the sort
    /// stack: [`Self::parse_for`] rejects a cursor whose fingerprint
    /// differs from the current request's with
    /// [`CursorError::FilterMismatch`], so a cursor replayed after the
    /// filters changed cannot walk a boundary computed under the old set.
    #[serde(rename = "f")]
    pub(crate) filter_fp: String,
}

/// Parse and encode failures for the cursor family.
///
/// Parsing yields the input-shape variants. Encoding yields only
/// [`Self::SerializePayload`], and only for [`SortCursor`]: the
/// pipe-format cursors write their timestamp infallibly.
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
    /// Cursor's embedded filter fingerprint did not match the request's
    /// active filter set (e.g. a page-2 cursor replayed after a filter
    /// was added or removed). Mapped to the same 422 as
    /// [`Self::SortMismatch`]: the boundary it encodes was computed under
    /// a different filter set, so continuing the walk would skip or
    /// repeat rows.
    #[error("cursor filter mismatch")]
    FilterMismatch,
    /// A [`SortCursor`] payload failed JSON serialization during encode.
    #[error("cursor payload not serializable")]
    SerializePayload(#[from] serde_json::Error),
}

impl SortCursor {
    /// Encode as a base64url-unpadded `?cursor=` value.
    ///
    /// # Errors
    ///
    /// Returns [`CursorError::SerializePayload`] if the payload fails
    /// JSON serialization. No inhabited value of this type reaches that
    /// path: the key variants are strings, integers, booleans, uuids,
    /// and timestamps, none of which `serde_json` rejects, and a year
    /// outside RFC 3339's `-9999..=9999` range serializes to the signed
    /// ISO 8601 form rather than failing. The fallible signature is kept
    /// so a future key variant cannot silently introduce a panic here.
    pub fn encode(&self) -> Result<String, CursorError> {
        let json = serde_json::to_string(self)?;
        Ok(Base64UrlUnpadded::encode_string(
            format!("m|{json}").as_bytes(),
        ))
    }

    /// Parse a base64url cursor and assert it was minted under `spec` and
    /// `filter_fp` (the fingerprint of the request's active filter set).
    ///
    /// # Errors
    ///
    /// Returns the matching [`CursorError`] variant for bad base64,
    /// non-UTF-8 bytes, a missing delimiter, a foreign tag byte, an
    /// undecodable JSON payload, a key list whose arity or value
    /// types disagree with `spec`'s columns, an embedded spec string
    /// minted under a different stack ([`CursorError::SortMismatch`]), or
    /// an embedded filter fingerprint minted under a different filter set
    /// ([`CursorError::FilterMismatch`]).
    pub fn parse_for(raw: &str, spec: &SortSpec, filter_fp: &str) -> Result<Self, CursorError> {
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
        if cursor.filter_fp != filter_fp {
            return Err(CursorError::FilterMismatch);
        }
        validate_keys(&cursor.keys, spec)?;
        Ok(cursor)
    }

    /// Build a cursor for `spec` and `filter_fp` on the mint path,
    /// validating that `keys` match the spec's arity, per-level value
    /// kinds, and null-legality before it can be encoded. Sharing
    /// [`validate_keys`] with [`Self::parse_for`] means a minted cursor
    /// can never be one its own decode would reject.
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
        filter_fp: String,
    ) -> Result<Self, CursorError> {
        validate_keys(&keys, spec)?;
        Ok(Self {
            spec: spec.canonical(),
            keys,
            id,
            filter_fp,
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
    #[must_use]
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
/// Carries `(added_at, manifestation_id)` matching the items
/// query's `ORDER BY added_at ASC, manifestation_id ASC`.
/// `added_at` is not unique per shelf (the table's
/// only unique key is the `(shelf_id, manifestation_id)` PK), so the
/// `manifestation_id` final tiebreaker is what keeps the walk total —
/// without it a page boundary between two same-instant rows would
/// drop one.
///
/// Wire encoding: base64url(unpadded) over
/// `si|<rfc3339>|<uuid>` — no free-text field, plain splits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShelfItemCursor {
    /// Boundary item's `shelf_items.added_at`.
    pub added_at: DateTime<Utc>,
    /// Tiebreaker `shelf_items.manifestation_id`.
    pub manifestation_id: Uuid,
}

impl ShelfItemCursor {
    /// Encode as a base64url-unpadded `?cursor=` value.
    ///
    /// The timestamp is written with the same RFC 3339 spelling serde
    /// gives a `DateTime<Utc>` on the wire, so a cursor timestamp and a
    /// response-body timestamp for the same instant are byte-identical.
    #[must_use]
    pub fn encode(&self) -> String {
        let ts = self.added_at.to_rfc3339_opts(SecondsFormat::AutoSi, true);
        let payload = format!("si|{ts}|{}", self.manifestation_id.as_hyphenated());
        Base64UrlUnpadded::encode_string(payload.as_bytes())
    }

    /// Parse a base64url cursor produced by [`Self::encode`].
    ///
    /// # Errors
    ///
    /// Returns the matching [`CursorError`] variant for bad base64,
    /// non-UTF-8 bytes, a missing delimiter, a foreign tag byte, or a
    /// malformed timestamp / uuid.
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
        let (ts, id_str) = rest.split_once('|').ok_or(CursorError::MalformedKey)?;
        let added_at = DateTime::parse_from_rfc3339(ts)
            .map_err(|_| CursorError::MalformedKey)?
            .with_timezone(&Utc);
        let manifestation_id = Uuid::parse_str(id_str).map_err(|_| CursorError::MalformedKey)?;
        Ok(Self {
            added_at,
            manifestation_id,
        })
    }
}

/// Keyset boundary for an id-ordered admin list filtered by one value.
///
/// Wire encoding: base64url(unpadded) over `<tag>|<filter>|<uuid>`. The
/// `filter` is the request's canonical filter value (empty when none), and
/// `tag` names the endpoint, so a cursor replayed against another endpoint
/// or under a different filter is rejected rather than walking a boundary
/// computed for another set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilteredIdCursor {
    /// Canonical filter value the cursor was minted under.
    pub filter: String,
    /// Boundary row id (rows are ordered by id ascending).
    pub id: Uuid,
}

impl FilteredIdCursor {
    /// Encode as a base64url-unpadded `?cursor=` value for `tag`.
    #[must_use]
    pub fn encode(&self, tag: &str) -> String {
        let payload = format!("{tag}|{}|{}", self.filter, self.id.as_hyphenated());
        Base64UrlUnpadded::encode_string(payload.as_bytes())
    }

    /// Parse a cursor minted by [`Self::encode`] under `tag` and `filter`.
    ///
    /// # Errors
    ///
    /// Returns the matching [`CursorError`] variant for bad base64,
    /// non-UTF-8 bytes, a missing delimiter, a foreign tag, a malformed uuid,
    /// or a filter that differs from the request's ([`CursorError::FilterMismatch`]).
    pub fn parse(s: &str, tag: &str, filter: &str) -> Result<Self, CursorError> {
        let mut buf = vec![0u8; s.len()];
        let decoded = Base64UrlUnpadded::decode(s.as_bytes(), &mut buf)
            .map_err(|_| CursorError::InvalidBase64)?;
        let decoded_str = std::str::from_utf8(decoded).map_err(|_| CursorError::InvalidUtf8)?;
        let (found_tag, rest) = decoded_str
            .split_once('|')
            .ok_or(CursorError::MissingDelimiter)?;
        if found_tag != tag {
            return Err(CursorError::UnknownTag);
        }
        let (found_filter, id_str) = rest.rsplit_once('|').ok_or(CursorError::MalformedKey)?;
        let id = Uuid::parse_str(id_str).map_err(|_| CursorError::MalformedKey)?;
        if found_filter != filter {
            return Err(CursorError::FilterMismatch);
        }
        Ok(Self {
            filter: found_filter.to_owned(),
            id,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::routes::sort_spec::SortSpec;

    const NO_FILTERS: &str = "";

    fn spec_of(raw: &str) -> SortSpec {
        SortSpec::parse(raw).expect("valid spec")
    }

    fn cursor_under(spec: &SortSpec, keys: Vec<CursorValue>) -> SortCursor {
        SortCursor {
            spec: spec.canonical(),
            keys,
            id: Uuid::new_v4(),
            filter_fp: NO_FILTERS.to_owned(),
        }
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
            SortCursor::parse_for(&encoded, &spec_of("author,title"), NO_FILTERS),
            Err(CursorError::SortMismatch)
        ));
    }

    #[test]
    fn shelf_rejects_foreign_tags() {
        let ts = DateTime::parse_from_rfc3339("2026-05-22T09:30:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let id = Uuid::new_v4();
        let books = SortCursor {
            spec: SortSpec::default().canonical(),
            keys: vec![CursorValue::Ts(ts)],
            id,
            filter_fp: NO_FILTERS.to_owned(),
        }
        .encode()
        .expect("encode");
        assert!(matches!(
            ShelfCursor::parse(&books),
            Err(CursorError::UnknownTag)
        ));
        let item = ShelfItemCursor {
            added_at: ts,
            manifestation_id: id,
        }
        .encode();
        assert!(matches!(
            ShelfCursor::parse(&item),
            Err(CursorError::UnknownTag)
        ));
    }
}
