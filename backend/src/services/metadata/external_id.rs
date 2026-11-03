//! Typed parsing and validation of external identifier values.
//!
//! Each `(scheme, level)` pair has one canonical form. Raw operator input is
//! trimmed, case-canonicalised where the scheme defines a canonical case, and
//! validated against the scheme's format before it may enter the journal or
//! the registry tables. The database `CHECK` on `external_id` is a structural
//! backstop; this module is the precise gate.
//!
//! Level applicability is part of the rule table because entity level is not
//! derivable from the scheme: Open Library issues work ids (`OL…W`) and
//! edition ids (`OL…M`) under one scheme, while a Google Books volume id only
//! ever names an edition.

// THREAT: external_id values are untrusted input that later travel into
// outbound provider requests (REST path segments, GraphQL variables). The
// charset admitted here must never include path or query metacharacters
// ('/', '?', '#', '%', whitespace), control characters, or non-ASCII, so a
// stored value cannot smuggle a path/query segment into an outbound call
// even if a later encoding step regresses.

use crate::models::external_identifier::IdentifierLevel;

/// How a scheme's raw input is case-canonicalised before validation.
#[derive(Clone, Copy)]
enum CaseFold {
    /// Keep the operator's casing.
    None,
    /// Canonical form is upper-case (ASIN, Open Library, Wikidata).
    Upper,
    /// Canonical form is lower-case (normalised LCCN).
    Lower,
}

/// Format rule for one identifier scheme.
struct SchemeRule {
    id: &'static str,
    fold: CaseFold,
    /// Levels this scheme is valid at, with the level-specific validator.
    /// `None` for a level means the scheme has no identifier at that level.
    work: Option<fn(&str) -> bool>,
    manifestation: Option<fn(&str) -> bool>,
}

fn all_digits(s: &str, min: usize, max: usize) -> bool {
    (min..=max).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_digit())
}

fn all_in(s: &str, min: usize, max: usize, extra: &[u8]) -> bool {
    (min..=max).contains(&s.len())
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || extra.contains(&b))
}

fn is_asin(s: &str) -> bool {
    s.len() == 10
        && s.bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
}

fn is_openlibrary(s: &str, suffix: u8) -> bool {
    let Some(body) = s.strip_prefix("OL") else {
        return false;
    };
    let Some(digits) = body.strip_suffix(suffix as char) else {
        return false;
    };
    all_digits(digits, 1, 16)
}

fn is_openlibrary_work(s: &str) -> bool {
    is_openlibrary(s, b'W')
}

fn is_openlibrary_edition(s: &str) -> bool {
    is_openlibrary(s, b'M')
}

fn is_googlebooks_volume(s: &str) -> bool {
    all_in(s, 1, 40, b"_-")
}

fn is_hardcover(s: &str) -> bool {
    all_in(s, 1, 100, b"-")
}

fn is_numeric_id(s: &str) -> bool {
    all_digits(s, 1, 19)
}

fn is_oclc(s: &str) -> bool {
    all_digits(s, 1, 12)
}

fn is_lccn(s: &str) -> bool {
    let alpha_len = s.bytes().take_while(u8::is_ascii_lowercase).count();
    alpha_len <= 3 && all_digits(&s[alpha_len..], 8, 10)
}

fn is_wikidata(s: &str) -> bool {
    let Some(digits) = s.strip_prefix('Q') else {
        return false;
    };
    all_digits(digits, 1, 16) && !digits.starts_with('0')
}

fn is_calibre(s: &str) -> bool {
    all_in(s, 1, 64, b"-")
}

/// One rule per row of the `identifier_schemes` vocabulary. Order matches the
/// migration seed so drift is easy to spot in review.
const SCHEME_RULES: &[SchemeRule] = &[
    SchemeRule {
        id: "asin",
        fold: CaseFold::Upper,
        work: None,
        manifestation: Some(is_asin),
    },
    SchemeRule {
        id: "oclc",
        fold: CaseFold::None,
        work: None,
        manifestation: Some(is_oclc),
    },
    SchemeRule {
        id: "lccn",
        fold: CaseFold::Lower,
        work: None,
        manifestation: Some(is_lccn),
    },
    SchemeRule {
        id: "googlebooks",
        fold: CaseFold::None,
        work: None,
        manifestation: Some(is_googlebooks_volume),
    },
    SchemeRule {
        id: "openlibrary",
        fold: CaseFold::Upper,
        work: Some(is_openlibrary_work),
        manifestation: Some(is_openlibrary_edition),
    },
    SchemeRule {
        id: "hardcover",
        fold: CaseFold::None,
        work: Some(is_hardcover),
        manifestation: Some(is_hardcover),
    },
    SchemeRule {
        id: "goodreads",
        fold: CaseFold::None,
        work: Some(is_numeric_id),
        manifestation: Some(is_numeric_id),
    },
    SchemeRule {
        id: "librarything",
        fold: CaseFold::None,
        work: Some(is_numeric_id),
        manifestation: Some(is_numeric_id),
    },
    SchemeRule {
        id: "wikidata",
        fold: CaseFold::Upper,
        work: Some(is_wikidata),
        manifestation: Some(is_wikidata),
    },
    SchemeRule {
        id: "calibre",
        fold: CaseFold::None,
        work: None,
        manifestation: Some(is_calibre),
    },
];

fn rule_for(scheme: &str) -> Option<&'static SchemeRule> {
    SCHEME_RULES.iter().find(|r| r.id == scheme)
}

/// Check that `scheme` names a known identifier scheme valid at `level`,
/// without validating a value.
///
/// The clear path uses this so an unknown scheme
/// or a wrong-level address is rejected identically to the set path.
///
/// # Errors
/// Returns a user-facing message naming the unknown scheme or the level the
/// scheme is valid at.
pub fn validate_scheme_level(level: IdentifierLevel, scheme: &str) -> Result<(), String> {
    let rule = rule_for(scheme).ok_or_else(|| format!("unknown identifier scheme '{scheme}'"))?;
    let supported = match level {
        IdentifierLevel::Work => rule.work.is_some(),
        IdentifierLevel::Manifestation => rule.manifestation.is_some(),
    };
    if supported {
        Ok(())
    } else {
        Err(format!(
            "scheme '{scheme}' has no {}-level identifier",
            level.as_str()
        ))
    }
}

/// Split a canonical `identifiers.<level>.<scheme>` field name into its
/// level and scheme.
///
/// Rejects a malformed shape, an unknown level segment,
/// or a scheme unknown at that level. Shared by the manual PATCH dispatch
/// and the enrichment apply path so both address the registry identically.
///
/// # Errors
/// Returns a user-facing message describing the malformed part.
pub fn parse_canonical_field(field: &str) -> Result<(IdentifierLevel, &str), String> {
    let rest = field
        .strip_prefix("identifiers.")
        .ok_or_else(|| format!("'{field}' is not an identifier field"))?;
    let (level_segment, scheme) = rest.split_once('.').ok_or_else(|| {
        format!("identifier field '{field}' must be identifiers.<level>.<scheme>")
    })?;
    let level = IdentifierLevel::from_segment(level_segment).ok_or_else(|| {
        format!("identifier level must be 'work' or 'manifestation', got '{level_segment}'")
    })?;
    validate_scheme_level(level, scheme)?;
    Ok((level, scheme))
}

/// Parse a raw operator- or source-supplied identifier value into the
/// canonical form for `(scheme, level)`.
///
/// Trims surrounding whitespace, applies the scheme's canonical case, and
/// validates the scheme's format at the given level. The returned string is
/// safe for the registry `CHECK` constraint and for structural embedding in
/// outbound provider requests.
///
/// # Errors
/// Returns a user-facing message for an unknown scheme, a level the scheme
/// does not support, or a value that fails the scheme's format.
pub fn parse_external_id(
    level: IdentifierLevel,
    scheme: &str,
    raw: &str,
) -> Result<String, String> {
    let rule = rule_for(scheme).ok_or_else(|| format!("unknown identifier scheme '{scheme}'"))?;
    let validator = match level {
        IdentifierLevel::Work => rule.work,
        IdentifierLevel::Manifestation => rule.manifestation,
    }
    .ok_or_else(|| {
        format!(
            "scheme '{scheme}' has no {}-level identifier",
            level.as_str()
        )
    })?;
    let trimmed = raw.trim();
    let canonical = match rule.fold {
        CaseFold::None => trimmed.to_string(),
        CaseFold::Upper => trimmed.to_ascii_uppercase(),
        CaseFold::Lower => trimmed.to_ascii_lowercase(),
    };
    if validator(&canonical) {
        Ok(canonical)
    } else {
        Err(format!(
            "invalid {} identifier for scheme '{scheme}'",
            level.as_str()
        ))
    }
}
