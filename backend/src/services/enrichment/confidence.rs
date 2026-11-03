//! Confidence score calculation for the metadata enrichment pipeline.
//!
//! Scores are in `[0.0, 1.0]` for manual sources and `[0.0, 0.99]` for all
//! other sources, ensuring automated enrichment never reaches the manual-entry
//! ceiling even with maximum agreement boost.

// Phase B building block: callers are wired in Phase C.

/// Base confidence multiplier for a metadata source.
///
/// Returns 0.30 for any unknown source (conservative default).
#[must_use]
pub fn base_source(source: &str) -> f32 {
    match source {
        "manual" => 1.00,
        "hardcover" => 0.85,
        "openlibrary" => 0.80,
        "googlebooks" => 0.75,
        "opf" => 0.50,
        // "ai" and any unknown source both get the conservative 0.30 default.
        _ => 0.30,
    }
}

/// Accuracy modifier based on how the match was made.
///
/// Returns 0.50 for any unknown match type (conservative default).
#[must_use]
pub fn match_modifier(match_type: &str) -> f32 {
    match match_type {
        // A provider-native id fetches the exact record, so it carries the
        // same certainty as an ISBN match.
        "isbn" | "external_id" => 1.00,
        "title_author_exact" => 0.90,
        "title_author_fuzzy" => 0.75,
        // "title" and any unknown match type both get the conservative 0.50 default.
        _ => 0.50,
    }
}

/// Agreement boost multiplier based on how many sources concur.
///
/// * 0 or 1 source → 1.00 (no boost)
/// * 2 sources      → 1.10
/// * 3+ sources     → 1.20
#[must_use]
pub const fn agreement_boost(quorum: u32) -> f32 {
    match quorum {
        0 | 1 => 1.00,
        2 => 1.10,
        _ => 1.20,
    }
}

/// Compute the combined confidence score for a metadata observation.
///
/// Formula: `base_source(source) * match_modifier(match_type) * agreement_boost(quorum)`.
///
/// Clamped to `[0.0, 1.00]` for `"manual"` source, `[0.0, 0.99]` for all others.
#[must_use]
pub fn score(source: &str, match_type: &str, quorum: u32) -> f32 {
    let raw = base_source(source) * match_modifier(match_type) * agreement_boost(quorum);
    if source == "manual" {
        raw.clamp(0.0, 1.00)
    } else {
        raw.clamp(0.0, 0.99)
    }
}
