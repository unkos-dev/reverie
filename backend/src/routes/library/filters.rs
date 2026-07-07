//! Typed per-column filter grammar for `GET /api/v1/books` (11c).
//!
//! A flat suffix grammar (`title_contains`, `pages_gte`, `status_any`, ...)
//! adapted from `PostgREST`'s horizontal filtering: one URL param per column
//! condition, the operator carried as a suffix token on the column name so
//! native `URLSearchParams` parses it without a bracket-syntax parser. Every
//! condition ANDs with the vocabulary filters and each other.
//!
//! # Invariants
//! - Column names are never client input: each condition maps to a fixed
//!   SQL fragment here, and every user value travels through `push_bind`.
//!   Injection is unrepresentable by construction, matching the sort
//!   whitelist ([`crate::routes::sort_spec`]).
//! - Reading-status and rating conditions probe `reading_state` with a bare
//!   `EXISTS (... rs.manifestation_id = m.id ...)`, relying on RLS to scope
//!   `rs` to the caller (module invariant, [`super`] doc) rather than an
//!   ad-hoc `user_id` predicate. The `(user_id, manifestation_id)` primary
//!   key makes at most one `rs` row visible per manifestation, so every
//!   status/rating probe refers to the same row.
//! - Text matching is case-insensitive `ILIKE` with backslash-escaped
//!   wildcards ([`escape_like`]) and accent-sensitive, matching the search
//!   and suggest endpoints.

use sha2::{Digest, Sha256};
use sqlx::{Postgres, QueryBuilder};
use time::Date;
use uuid::Uuid;

use crate::error::AppError;
use crate::models::reading_status::ReadingStatus;

use super::{ListParams, MAX_TAG_FILTERS};

/// `deserialize_with` shim for the `created_at_*` date filters. The `time`
/// crate's default serde for [`Date`] reads a component representation, not
/// the `YYYY-MM-DD` string a query param carries, so the date params route
/// through here: an absent field defaults to `None` (never reaching this),
/// and a present value parses via [`parse_iso_date`], failing deserialization
/// (a 400) on a malformed date rather than silently dropping it.
pub(super) mod iso_date_opt {
    use serde::{Deserialize, Deserializer};
    use time::Date;

    /// Deserialize `Option<Date>` from an optional ISO 8601 calendar-date
    /// string.
    ///
    /// # Errors
    /// Returns the deserializer's error when the value is present but is not a
    /// `YYYY-MM-DD` calendar date.
    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<Date>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw: Option<String> = Option::deserialize(deserializer)?;
        raw.map_or(Ok(None), |text| {
            super::parse_iso_date(&text).map(Some).ok_or_else(|| {
                serde::de::Error::custom("expected an ISO 8601 calendar date (YYYY-MM-DD)")
            })
        })
    }
}

/// Parse a `YYYY-MM-DD` calendar date with no feature-gated format machinery:
/// three integer components fed to [`Date::from_calendar_date`], which rejects
/// an out-of-range month or a day past the month's length. `None` on any
/// malformed input (extra components, non-numeric parts, an impossible date).
fn parse_iso_date(text: &str) -> Option<Date> {
    let mut parts = text.split('-');
    let year: i32 = parts.next()?.parse().ok()?;
    let month: u8 = parts.next()?.parse().ok()?;
    let day: u8 = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    let month = time::Month::try_from(month).ok()?;
    Date::from_calendar_date(year, month, day).ok()
}

/// Upper bound on any single text-filter needle. Bounds the worst-case
/// `ILIKE` pattern cost and mirrors the search endpoint's `MAX_Q_LEN`.
const MAX_TEXT_FILTER_CHARS: usize = 200;

/// The `unread` pseudo-value accepted by `status_any` / `status_none`:
/// matches a row with no reading-status set (rating-only rows included),
/// which no real [`ReadingStatus`] wire name can express.
const UNREAD_TOKEN: &str = "unread";

/// Backslash-escape `\`, `%`, and `_` so `raw` is matched literally by
/// `ILIKE` rather than as a wildcard pattern. Postgres's default `LIKE`
/// escape character is already `\`, so callers need no explicit `ESCAPE`
/// clause on the query side.
pub fn escape_like(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for c in raw.chars() {
        if matches!(c, '\\' | '%' | '_') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Semantic validation of the filter params (422 on failure). Type-level
/// failures (bad uuid, non-integer, bad date) are already rejected by serde
/// as a 400 before this runs; this layer catches over-cap value lists,
/// over-long text, out-of-range ratings, negative page bounds, and unknown
/// status tokens.
///
/// # Errors
/// Returns [`AppError::Validation`] on the first rule a param violates.
pub(super) fn validate(p: &ListParams) -> Result<(), AppError> {
    for (param, len) in [
        ("tag", p.tag.len()),
        ("tag_any", p.tag_any.len()),
        ("tag_none", p.tag_none.len()),
        ("genre", p.genre.len()),
        ("genre_any", p.genre_any.len()),
        ("genre_none", p.genre_none.len()),
        ("mood", p.mood.len()),
        ("mood_any", p.mood_any.len()),
        ("mood_none", p.mood_none.len()),
        ("author", p.author.len()),
        ("author_any", p.author_any.len()),
        ("author_none", p.author_none.len()),
        ("status_any", p.status_any.len()),
        ("status_none", p.status_none.len()),
    ] {
        if len > MAX_TAG_FILTERS {
            return Err(AppError::Validation(format!(
                "too many {param} filters: maximum {MAX_TAG_FILTERS}"
            )));
        }
    }

    for (param, value) in [
        ("q", &p.q),
        ("title_contains", &p.title_contains),
        ("title_eq", &p.title_eq),
        ("title_ne", &p.title_ne),
        ("subtitle_contains", &p.subtitle_contains),
        ("isbn_13_contains", &p.isbn_13_contains),
        ("isbn_13_eq", &p.isbn_13_eq),
    ] {
        let Some(text) = value else { continue };
        if text.trim().chars().count() > MAX_TEXT_FILTER_CHARS {
            return Err(AppError::Validation(format!(
                "{param} too long: maximum {MAX_TEXT_FILTER_CHARS} characters"
            )));
        }
    }

    for (param, value) in [("rating_gte", p.rating_gte), ("rating_lte", p.rating_lte)] {
        let Some(rating) = value else { continue };
        if !(1..=5).contains(&rating) {
            return Err(AppError::Validation(format!(
                "{param} out of range: rating is 1 to 5"
            )));
        }
    }

    for (param, value) in [("pages_gte", p.pages_gte), ("pages_lte", p.pages_lte)] {
        let Some(pages) = value else { continue };
        if pages < 0 {
            return Err(AppError::Validation(format!(
                "{param} must not be negative"
            )));
        }
    }

    for (param, list) in [
        ("status_any", &p.status_any),
        ("status_none", &p.status_none),
    ] {
        for token in list {
            if token != UNREAD_TOKEN && ReadingStatus::from_wire(token).is_none() {
                return Err(AppError::Validation(format!(
                    "unknown {param} value: {token}"
                )));
            }
        }
    }

    Ok(())
}

/// Deterministic serialization of the active filter set, feeding the cursor
/// fingerprint ([`filter_fingerprint`]). Param names and multi-value lists
/// are sorted so neither param order nor value order changes the output; a
/// value that differs in any way yields a different string. This shape is
/// load-bearing for cursor stability: changing it invalidates every
/// outstanding cursor, which is acceptable but must be deliberate.
pub(super) fn canonical_filter_string(p: &ListParams) -> String {
    let mut parts: Vec<String> = Vec::new();

    push_multi(&mut parts, "author", &uuids_to_strings(&p.author));
    push_multi(&mut parts, "author_any", &uuids_to_strings(&p.author_any));
    push_multi(&mut parts, "author_none", &uuids_to_strings(&p.author_none));
    push_multi(&mut parts, "tag", &p.tag);
    push_multi(&mut parts, "tag_any", &p.tag_any);
    push_multi(&mut parts, "tag_none", &p.tag_none);
    push_multi(&mut parts, "genre", &p.genre);
    push_multi(&mut parts, "genre_any", &p.genre_any);
    push_multi(&mut parts, "genre_none", &p.genre_none);
    push_multi(&mut parts, "mood", &p.mood);
    push_multi(&mut parts, "mood_any", &p.mood_any);
    push_multi(&mut parts, "mood_none", &p.mood_none);
    push_multi(&mut parts, "status_any", &p.status_any);
    push_multi(&mut parts, "status_none", &p.status_none);

    if let Some(v) = p.series {
        parts.push(format!("series={v}"));
    }
    if let Some(v) = p.shelf {
        parts.push(format!("shelf={v}"));
    }
    push_text_part(&mut parts, "title_contains", p.title_contains.as_deref());
    push_text_part(&mut parts, "title_eq", p.title_eq.as_deref());
    push_text_part(&mut parts, "title_ne", p.title_ne.as_deref());
    push_text_part(
        &mut parts,
        "subtitle_contains",
        p.subtitle_contains.as_deref(),
    );
    push_text_part(
        &mut parts,
        "isbn_13_contains",
        p.isbn_13_contains.as_deref(),
    );
    push_text_part(&mut parts, "isbn_13_eq", p.isbn_13_eq.as_deref());
    push_text_part(&mut parts, "q", p.q.as_deref());

    if let Some(b) = p.subtitle_empty {
        parts.push(format!("subtitle_empty={b}"));
    }
    if let Some(b) = p.isbn_13_empty {
        parts.push(format!("isbn_13_empty={b}"));
    }
    if let Some(b) = p.pages_empty {
        parts.push(format!("pages_empty={b}"));
    }
    if let Some(b) = p.rating_empty {
        parts.push(format!("rating_empty={b}"));
    }
    if let Some(n) = p.pages_gte {
        parts.push(format!("pages_gte={n}"));
    }
    if let Some(n) = p.pages_lte {
        parts.push(format!("pages_lte={n}"));
    }
    if let Some(n) = p.rating_gte {
        parts.push(format!("rating_gte={n}"));
    }
    if let Some(n) = p.rating_lte {
        parts.push(format!("rating_lte={n}"));
    }
    if let Some(d) = p.created_at_gte {
        parts.push(format!("created_at_gte={d}"));
    }
    if let Some(d) = p.created_at_lte {
        parts.push(format!("created_at_lte={d}"));
    }

    parts.sort();
    parts.join("&")
}

/// The cursor's filter fingerprint: the first 16 hex chars (8 bytes) of
/// SHA-256 over [`canonical_filter_string`]. Empty filters hash the empty
/// string, so an unfiltered request has a stable, non-empty fingerprint.
pub(super) fn filter_fingerprint(p: &ListParams) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let canonical = canonical_filter_string(p);
    let digest = Sha256::digest(canonical.as_bytes());
    let mut out = String::with_capacity(16);
    for byte in digest.iter().take(8) {
        out.push(HEX[usize::from(byte >> 4)] as char);
        out.push(HEX[usize::from(byte & 0x0f)] as char);
    }
    out
}

/// Push every typed filter predicate for `p` onto the dynamic list query,
/// after the vocabulary predicates and before the cursor predicate. Empty /
/// absent conditions push nothing. Ordering among the AND-combined
/// predicates does not change semantics.
pub(super) fn push_filter_predicates(qb: &mut QueryBuilder<Postgres>, p: &ListParams) {
    push_author_predicates(qb, p);
    if let Some(series_id) = p.series {
        qb.push(
            " AND EXISTS (SELECT 1 FROM series_works sw2 \
              WHERE sw2.work_id = w.id AND sw2.series_id = ",
        );
        qb.push_bind(series_id);
        qb.push(")");
    }
    if let Some(shelf_id) = p.shelf {
        qb.push(
            " AND EXISTS (SELECT 1 FROM shelf_items si \
              JOIN shelves s ON s.id = si.shelf_id \
              WHERE si.manifestation_id = m.id \
                AND si.shelf_id = ",
        );
        qb.push_bind(shelf_id);
        qb.push(" AND s.user_id = current_setting('app.current_user_id', true)::uuid)");
    }
    push_vocab_predicates(
        qb,
        "COUNT(DISTINCT lower(t.name))",
        "FROM manifestation_tags mt \
          JOIN tags t ON t.id = mt.tag_id \
          WHERE mt.manifestation_id = m.id AND lower(t.name) = ANY(",
        &p.tag,
        &p.tag_any,
        &p.tag_none,
    );
    push_vocab_predicates(
        qb,
        "COUNT(DISTINCT lower(g.name))",
        "FROM manifestation_genres mg \
          JOIN genres g ON g.id = mg.genre_id \
          WHERE mg.manifestation_id = m.id AND lower(g.name) = ANY(",
        &p.genre,
        &p.genre_any,
        &p.genre_none,
    );
    push_vocab_predicates(
        qb,
        "COUNT(DISTINCT lower(md.name))",
        "FROM manifestation_moods mm \
          JOIN moods md ON md.id = mm.mood_id \
          WHERE mm.manifestation_id = m.id AND lower(md.name) = ANY(",
        &p.mood,
        &p.mood_any,
        &p.mood_none,
    );
    push_text_predicates(qb, p);
    push_pages_predicates(qb, p);
    push_created_at_predicates(qb, p);
    push_status_predicates(qb, p);
    push_rating_predicates(qb, p);
    push_q_predicate(qb, p);
}

/// Push the `author` vocabulary triple, role-scoped to `'author'` like the
/// pre-11c single filter: all-of via a distinct-count scalar, any-of via
/// `EXISTS`, none-of via `NOT EXISTS`. Ids are deduped before the all-of
/// count so a repeated id cannot make the predicate unsatisfiable.
fn push_author_predicates(qb: &mut QueryBuilder<Postgres>, p: &ListParams) {
    if !p.author.is_empty() {
        let mut ids = p.author.clone();
        ids.sort_unstable();
        ids.dedup();
        // Caller has validated `ids.len() <= MAX_TAG_FILTERS`, so the
        // `usize -> u32 -> i64` step is exact; the fallback stays defensive.
        let count = i64::from(u32::try_from(ids.len()).unwrap_or(u32::MAX));
        qb.push(
            " AND (SELECT COUNT(DISTINCT wa.author_id) FROM work_authors wa \
              WHERE wa.work_id = w.id AND wa.role = 'author' AND wa.author_id = ANY(",
        );
        qb.push_bind(ids);
        qb.push(")) = ");
        qb.push_bind(count);
    }
    if !p.author_any.is_empty() {
        qb.push(
            " AND EXISTS (SELECT 1 FROM work_authors wa \
              WHERE wa.work_id = w.id AND wa.role = 'author' AND wa.author_id = ANY(",
        );
        qb.push_bind(p.author_any.clone());
        qb.push("))");
    }
    if !p.author_none.is_empty() {
        qb.push(
            " AND NOT EXISTS (SELECT 1 FROM work_authors wa \
              WHERE wa.work_id = w.id AND wa.role = 'author' AND wa.author_id = ANY(",
        );
        qb.push_bind(p.author_none.clone());
        qb.push("))");
    }
}

/// Push up to three vocabulary predicates for one junction+vocabulary pair.
/// `count_expr` and `core` are static SQL fragments (never user input);
/// user-supplied names travel exclusively through `push_bind` and are
/// lowercased to mirror the tables' `lower(name)` identity.
fn push_vocab_predicates(
    qb: &mut QueryBuilder<Postgres>,
    count_expr: &str,
    core: &str,
    all_of: &[String],
    any_of: &[String],
    none_of: &[String],
) {
    let lowered =
        |names: &[String]| -> Vec<String> { names.iter().map(|n| n.to_lowercase()).collect() };
    if !all_of.is_empty() {
        // Dedupe before binding: COUNT(DISTINCT ..) compares against the
        // requested count, so a repeated name would make the predicate
        // unsatisfiable (1 = 2) and silently return zero rows.
        let mut all_of = lowered(all_of);
        all_of.sort_unstable();
        all_of.dedup();
        let count = i64::from(u32::try_from(all_of.len()).unwrap_or(u32::MAX));
        qb.push(" AND (SELECT ");
        qb.push(count_expr);
        qb.push(" ");
        qb.push(core);
        qb.push_bind(all_of);
        qb.push(")) = ");
        qb.push_bind(count);
    }
    if !any_of.is_empty() {
        qb.push(" AND EXISTS (SELECT 1 ");
        qb.push(core);
        qb.push_bind(lowered(any_of));
        qb.push("))");
    }
    if !none_of.is_empty() {
        qb.push(" AND NOT EXISTS (SELECT 1 ");
        qb.push(core);
        qb.push_bind(lowered(none_of));
        qb.push("))");
    }
}

/// Push the text-column conditions (`title`/`subtitle` on `works`, `isbn_13`
/// on `manifestations`). `_contains` wraps the escaped needle in `%…%`,
/// `_eq` binds it wildcard-free, `_ne` negates, `_empty` is a null check.
fn push_text_predicates(qb: &mut QueryBuilder<Postgres>, p: &ListParams) {
    if let Some(needle) = trimmed(p.title_contains.as_deref()) {
        push_ilike_contains(qb, "w.title", needle);
    }
    if let Some(needle) = trimmed(p.title_eq.as_deref()) {
        push_ilike_exact(qb, "w.title", needle, false);
    }
    if let Some(needle) = trimmed(p.title_ne.as_deref()) {
        push_ilike_exact(qb, "w.title", needle, true);
    }
    if let Some(needle) = trimmed(p.subtitle_contains.as_deref()) {
        push_ilike_contains(qb, "w.subtitle", needle);
    }
    if let Some(empty) = p.subtitle_empty {
        push_is_empty(qb, "w.subtitle", empty);
    }
    if let Some(needle) = trimmed(p.isbn_13_contains.as_deref()) {
        push_ilike_contains(qb, "m.isbn_13", needle);
    }
    if let Some(needle) = trimmed(p.isbn_13_eq.as_deref()) {
        push_ilike_exact(qb, "m.isbn_13", needle, false);
    }
    if let Some(empty) = p.isbn_13_empty {
        push_is_empty(qb, "m.isbn_13", empty);
    }
}

/// `AND <col> ILIKE '%needle%'`. `col` is a fixed fragment; `needle` is
/// escaped so `%`/`_`/`\` match literally.
fn push_ilike_contains(qb: &mut QueryBuilder<Postgres>, col: &str, needle: &str) {
    qb.push(" AND ");
    qb.push(col);
    qb.push(" ILIKE ");
    qb.push_bind(format!("%{}%", escape_like(needle)));
}

/// `AND <col> [NOT] ILIKE 'needle'` (no wildcards): a case-insensitive
/// exact match, negated when `negate`.
fn push_ilike_exact(qb: &mut QueryBuilder<Postgres>, col: &str, needle: &str, negate: bool) {
    qb.push(" AND ");
    qb.push(col);
    qb.push(if negate { " NOT ILIKE " } else { " ILIKE " });
    qb.push_bind(escape_like(needle));
}

/// `AND <col> IS [NOT] NULL` for the `_empty` conditions.
fn push_is_empty(qb: &mut QueryBuilder<Postgres>, col: &str, empty: bool) {
    qb.push(" AND ");
    qb.push(col);
    qb.push(if empty { " IS NULL" } else { " IS NOT NULL" });
}

/// Push the `pages` bounds. Both bounds are inclusive; `_empty` is a null
/// check. A NULL page count fails both `>=` and `<=`, so an unpaged row is
/// excluded from a bounded range, which is the intended behavior.
fn push_pages_predicates(qb: &mut QueryBuilder<Postgres>, p: &ListParams) {
    if let Some(n) = p.pages_gte {
        qb.push(" AND m.pages >= ");
        qb.push_bind(n);
    }
    if let Some(n) = p.pages_lte {
        qb.push(" AND m.pages <= ");
        qb.push_bind(n);
    }
    if let Some(empty) = p.pages_empty {
        push_is_empty(qb, "m.pages", empty);
    }
}

/// Push the `created_at` bounds as day-inclusive on both ends without
/// casting the indexed column: `_gte` compares `>= $date` at midnight UTC,
/// `_lte` compares `< ($date + 1 day)` at midnight UTC so the whole upper
/// day is included.
fn push_created_at_predicates(qb: &mut QueryBuilder<Postgres>, p: &ListParams) {
    if let Some(d) = p.created_at_gte {
        qb.push(" AND m.created_at >= ");
        qb.push_bind(d.midnight().assume_utc());
    }
    if let Some(d) = p.created_at_lte {
        // Day-inclusive upper bound: strictly before the next day's midnight.
        // At the representable maximum (`9999-12-31`) there is no next day, so
        // fall back to `<=` the maximum timestamp; no real row reaches it.
        if let Some(next) = d.next_day() {
            qb.push(" AND m.created_at < ");
            qb.push_bind(next.midnight().assume_utc());
        } else {
            qb.push(" AND m.created_at <= ");
            qb.push_bind(time::PrimitiveDateTime::MAX.assume_utc());
        }
    }
}

/// Push the reading-status conditions. `status_any` matches rows satisfying
/// any listed token; `status_none` excludes them. Each token set expands via
/// [`push_status_match`].
fn push_status_predicates(qb: &mut QueryBuilder<Postgres>, p: &ListParams) {
    if !p.status_any.is_empty() {
        qb.push(" AND ");
        push_status_match(qb, &p.status_any);
    }
    if !p.status_none.is_empty() {
        qb.push(" AND NOT ");
        push_status_match(qb, &p.status_none);
    }
}

/// Push a parenthesized boolean matching a row against `tokens`: an `EXISTS`
/// probe for the real statuses OR a `NOT EXISTS` probe for the `unread`
/// pseudo-value (no status set). Caller guarantees `tokens` is non-empty and
/// every token passed [`validate`].
fn push_status_match(qb: &mut QueryBuilder<Postgres>, tokens: &[String]) {
    let (reals, has_unread) = partition_status(tokens);
    let has_reals = !reals.is_empty();
    qb.push("(");
    if has_reals {
        qb.push(
            "EXISTS (SELECT 1 FROM reading_state rs \
              WHERE rs.manifestation_id = m.id AND rs.status::text = ANY(",
        );
        qb.push_bind(reals);
        qb.push("))");
    }
    if has_unread {
        if has_reals {
            qb.push(" OR ");
        }
        qb.push(
            "NOT EXISTS (SELECT 1 FROM reading_state rs \
              WHERE rs.manifestation_id = m.id AND rs.status IS NOT NULL)",
        );
    }
    // Unreachable given the non-empty, validated `tokens`; kept so an empty
    // expansion can never emit the invalid `()`.
    if !has_reals && !has_unread {
        qb.push("TRUE");
    }
    qb.push(")");
}

/// Split status tokens into the real [`ReadingStatus`] wire names (sorted,
/// deduped) and whether the `unread` pseudo-value was requested.
fn partition_status(tokens: &[String]) -> (Vec<String>, bool) {
    let mut reals: Vec<String> = Vec::new();
    let mut has_unread = false;
    for token in tokens {
        if token == UNREAD_TOKEN {
            has_unread = true;
        } else {
            reals.push(token.clone());
        }
    }
    reals.sort_unstable();
    reals.dedup();
    (reals, has_unread)
}

/// Push the `rating` conditions over the caller's RLS-scoped `reading_state`.
/// Bounds are inclusive; `_empty=true` means no rating set (a `NOT EXISTS`),
/// `_empty=false` means any rating set.
fn push_rating_predicates(qb: &mut QueryBuilder<Postgres>, p: &ListParams) {
    if let Some(n) = p.rating_gte {
        qb.push(
            " AND EXISTS (SELECT 1 FROM reading_state rs \
              WHERE rs.manifestation_id = m.id AND rs.rating >= ",
        );
        qb.push_bind(n);
        qb.push(")");
    }
    if let Some(n) = p.rating_lte {
        qb.push(
            " AND EXISTS (SELECT 1 FROM reading_state rs \
              WHERE rs.manifestation_id = m.id AND rs.rating <= ",
        );
        qb.push_bind(n);
        qb.push(")");
    }
    if let Some(empty) = p.rating_empty {
        qb.push(if empty {
            " AND NOT EXISTS (SELECT 1 FROM reading_state rs \
              WHERE rs.manifestation_id = m.id AND rs.rating IS NOT NULL)"
        } else {
            " AND EXISTS (SELECT 1 FROM reading_state rs \
              WHERE rs.manifestation_id = m.id AND rs.rating IS NOT NULL)"
        });
    }
}

/// Push the quick-search filter: a full-text match OR a title trigram match,
/// with no ranking so the active sort order is preserved. Both legs are
/// index-backed (GIN `search_vector`, `GiST` `title` trigram).
fn push_q_predicate(qb: &mut QueryBuilder<Postgres>, p: &ListParams) {
    if let Some(raw) = trimmed(p.q.as_deref()) {
        qb.push(" AND (w.search_vector @@ websearch_to_tsquery('english', ");
        qb.push_bind(raw.to_owned());
        qb.push(") OR w.title ILIKE ");
        qb.push_bind(format!("%{}%", escape_like(raw)));
        qb.push(")");
    }
}

/// Trim an optional text value, yielding `None` when absent or blank so a
/// whitespace-only needle pushes no predicate.
fn trimmed(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|s| !s.is_empty())
}

/// Sorted, deduped canonical entry for a multi-value param, or nothing when
/// the list is empty.
fn push_multi(parts: &mut Vec<String>, name: &str, values: &[String]) {
    if values.is_empty() {
        return;
    }
    let mut sorted: Vec<&str> = values.iter().map(String::as_str).collect();
    sorted.sort_unstable();
    sorted.dedup();
    parts.push(format!("{name}={}", sorted.join(",")));
}

/// Canonical entry for a single text param, trimmed and dropped when blank
/// so it mirrors the predicate's own blank-skip.
fn push_text_part(parts: &mut Vec<String>, name: &str, value: Option<&str>) {
    if let Some(text) = value {
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            parts.push(format!("{name}={trimmed}"));
        }
    }
}

fn uuids_to_strings(ids: &[Uuid]) -> Vec<String> {
    ids.iter()
        .map(|id| id.as_hyphenated().to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params() -> ListParams {
        ListParams::default()
    }

    #[test]
    fn escape_like_escapes_wildcards() {
        assert_eq!(escape_like("a%b_c\\d"), "a\\%b\\_c\\\\d");
        assert_eq!(escape_like("plain"), "plain");
    }

    #[test]
    fn parse_iso_date_accepts_calendar_dates_and_rejects_bad_input() {
        assert_eq!(
            parse_iso_date("2026-06-30").map(|d| d.to_string()),
            Some("2026-06-30".to_owned())
        );
        // Rejections: impossible day, bad month, non-numeric, extra components,
        // and a datetime string (the date filter takes a bare calendar date).
        assert!(parse_iso_date("2026-02-30").is_none());
        assert!(parse_iso_date("2026-13-01").is_none());
        assert!(parse_iso_date("2026-06").is_none());
        assert!(parse_iso_date("2026-06-30-01").is_none());
        assert!(parse_iso_date("2026-06-30T00:00:00Z").is_none());
        assert!(parse_iso_date("nonsense").is_none());
    }

    #[test]
    fn iso_date_opt_deserializes_present_and_absent() {
        #[derive(serde::Deserialize)]
        struct Wrap {
            #[serde(default, deserialize_with = "iso_date_opt::deserialize")]
            d: Option<Date>,
        }
        let present: Wrap = serde_json::from_str(r#"{"d":"2026-06-30"}"#).expect("present parses");
        assert_eq!(
            present.d.map(|d| d.to_string()),
            Some("2026-06-30".to_owned())
        );
        let absent: Wrap = serde_json::from_str("{}").expect("absent parses");
        assert!(absent.d.is_none());
        let bad: Result<Wrap, _> = serde_json::from_str(r#"{"d":"nope"}"#);
        assert!(bad.is_err(), "a malformed date must fail deserialization");
    }

    #[test]
    fn canonical_string_is_order_independent() {
        let mut a = params();
        a.tag_any = vec!["fantasy".into(), "horror".into()];
        a.status_any = vec!["reading".into(), "unread".into()];
        a.pages_gte = Some(300);

        let mut b = params();
        b.tag_any = vec!["horror".into(), "fantasy".into()];
        b.status_any = vec!["unread".into(), "reading".into()];
        b.pages_gte = Some(300);

        assert_eq!(canonical_filter_string(&a), canonical_filter_string(&b));
        assert_eq!(filter_fingerprint(&a), filter_fingerprint(&b));
    }

    #[test]
    fn canonical_string_empty_is_stable_and_blank() {
        assert_eq!(canonical_filter_string(&params()), "");
        // 8 bytes rendered as two hex chars each.
        assert_eq!(filter_fingerprint(&params()).len(), 16);
    }

    #[test]
    fn fingerprint_changes_when_a_filter_changes() {
        let base = filter_fingerprint(&params());
        let mut changed = params();
        changed.title_contains = Some("dune".into());
        assert_ne!(base, filter_fingerprint(&changed));

        let mut lte = params();
        lte.pages_lte = Some(500);
        let mut gte = params();
        gte.pages_gte = Some(500);
        assert_ne!(filter_fingerprint(&lte), filter_fingerprint(&gte));
    }

    #[test]
    fn blank_text_does_not_enter_canonical_string() {
        let mut p = params();
        p.title_contains = Some("   ".into());
        assert_eq!(canonical_filter_string(&p), "");
    }

    #[test]
    fn validate_rejects_over_cap_value_list() {
        let mut p = params();
        p.author_any = (0..=MAX_TAG_FILTERS).map(|_| Uuid::new_v4()).collect();
        assert!(matches!(validate(&p), Err(AppError::Validation(_))));
    }

    #[test]
    fn validate_rejects_over_long_text() {
        let mut p = params();
        p.title_contains = Some("x".repeat(MAX_TEXT_FILTER_CHARS + 1));
        assert!(matches!(validate(&p), Err(AppError::Validation(_))));
    }

    #[test]
    fn validate_rejects_out_of_range_rating() {
        let mut p = params();
        p.rating_gte = Some(6);
        assert!(matches!(validate(&p), Err(AppError::Validation(_))));
        let mut zero = params();
        zero.rating_lte = Some(0);
        assert!(matches!(validate(&zero), Err(AppError::Validation(_))));
    }

    #[test]
    fn validate_rejects_negative_pages() {
        let mut p = params();
        p.pages_gte = Some(-1);
        assert!(matches!(validate(&p), Err(AppError::Validation(_))));
    }

    #[test]
    fn validate_rejects_unknown_status_token() {
        let mut p = params();
        p.status_any = vec!["currently_reading".into()];
        assert!(matches!(validate(&p), Err(AppError::Validation(_))));
    }

    #[test]
    fn validate_accepts_unread_pseudo_value_and_wire_names() {
        let mut p = params();
        p.status_any = vec!["unread".into(), "reading".into(), "finished".into()];
        p.status_none = vec!["abandoned".into()];
        p.rating_gte = Some(1);
        p.rating_lte = Some(5);
        p.title_contains = Some("x".repeat(MAX_TEXT_FILTER_CHARS));
        assert!(validate(&p).is_ok());
    }

    #[test]
    fn partition_status_splits_unread_from_reals() {
        let (reals, has_unread) =
            partition_status(&["unread".into(), "reading".into(), "reading".into()]);
        assert_eq!(reals, vec!["reading".to_owned()]);
        assert!(has_unread);
    }
}
