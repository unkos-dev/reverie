//! Confidence score calculation for the metadata enrichment pipeline.
//!
//! Scores are in `[0.0, 1.0]` for manual sources and `[0.0, 0.99]` for all
//! other sources, ensuring automated enrichment never reaches the manual-entry
//! ceiling even with maximum agreement boost.

// Phase B building block: callers are wired in Phase C.

/// Base confidence multiplier for a metadata source.
///
/// Returns 0.30 for any unknown source (conservative default).
pub fn base_source(source: &str) -> f32 {
    match source {
        "manual" => 1.00,
        "hardcover" => 0.85,
        "openlibrary" => 0.80,
        "googlebooks" => 0.75,
        "opf" => 0.50,
        "ai" => 0.30,
        _ => 0.30,
    }
}

/// Accuracy modifier based on how the match was made.
///
/// Returns 0.50 for any unknown match type (conservative default).
pub fn match_modifier(match_type: &str) -> f32 {
    match match_type {
        "isbn" => 1.00,
        "title_author_exact" => 0.90,
        "title_author_fuzzy" => 0.75,
        "title" => 0.50,
        _ => 0.50,
    }
}

/// Agreement boost multiplier based on how many sources concur.
///
/// * 0 or 1 source → 1.00 (no boost)
/// * 2 sources      → 1.10
/// * 3+ sources     → 1.20
pub fn agreement_boost(quorum: u32) -> f32 {
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
pub fn score(source: &str, match_type: &str, quorum: u32) -> f32 {
    let raw = base_source(source) * match_modifier(match_type) * agreement_boost(quorum);
    if source == "manual" {
        raw.clamp(0.0, 1.00)
    } else {
        raw.clamp(0.0, 0.99)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manual_isbn_quorum1_is_one() {
        assert_eq!(score("manual", "isbn", 1), 1.00_f32);
    }

    #[test]
    fn openlibrary_isbn_quorum3_under_ceiling() {
        let s = score("openlibrary", "isbn", 3);
        // 0.80 * 1.00 * 1.20 = 0.96 — below ceiling but test verifies it
        assert!((s - 0.96).abs() < 1e-6, "expected ~0.96, got {s}");
        assert!(s <= 0.99, "score {s} exceeds automated ceiling 0.99");
    }

    #[test]
    fn hardcover_isbn_quorum3_clamped_to_ceiling() {
        // 0.85 * 1.00 * 1.20 = 1.02 pre-clamp → clamped to 0.99
        let s = score("hardcover", "isbn", 3);
        assert_eq!(s, 0.99_f32);
    }

    #[test]
    fn googlebooks_title_quorum0() {
        // 0.75 * 0.50 * 1.00 = 0.375
        let s = score("googlebooks", "title", 0);
        assert!((s - 0.375).abs() < 1e-6, "expected 0.375, got {s}");
    }

    #[test]
    fn unknown_source_clamped_by_modifier_and_boost() {
        // unknown → 0.30, title → 0.50, quorum 2 → 1.10 → 0.165
        let s = score("unknownsource", "title", 2);
        assert!((s - 0.165).abs() < 1e-6, "expected 0.165, got {s}");
        assert!(s <= 0.99);
    }

    #[test]
    fn unknown_match_type_treated_as_0_50() {
        let s_known = score("openlibrary", "title", 1);
        let s_unknown = score("openlibrary", "totally_unknown_match", 1);
        assert_eq!(s_known, s_unknown);
    }

    #[test]
    fn agreement_boost_boundaries() {
        assert_eq!(agreement_boost(0), 1.00_f32);
        assert_eq!(agreement_boost(1), 1.00_f32);
        assert_eq!(agreement_boost(2), 1.10_f32);
        assert_eq!(agreement_boost(3), 1.20_f32);
        assert_eq!(agreement_boost(100), 1.20_f32);
    }
}
