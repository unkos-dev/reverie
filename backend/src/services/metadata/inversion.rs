//! Heuristic title-author inversion detection.
//!
//! Detects cases where the title field looks like "Lastname, Firstname" and
//! an author field looks like a book title. Advisory only — results are stored
//! as draft metadata, never auto-applied.

/// Fields consumed by the enrichment confidence scorer.
///
/// Advisory signal only — the result is stored as a draft metadata row and
/// never auto-applied. A human review step (or future policy rule) decides
/// whether to promote `probable_author` / `probable_title` to canonical.
#[derive(Debug, Clone)]
pub struct InversionResult {
    /// Reconstructed author name in natural order (e.g. `"John Smith"`).
    pub probable_author: String,
    /// The author field value that looks like a book title.
    pub probable_title: String,
}

/// Check if a title looks like "Lastname, Firstname" (a swapped author name).
/// Returns Some if inversion is detected with a matching author that looks like a title.
#[must_use]
pub fn detect_inversion(title: &str, authors: &[String]) -> Option<InversionResult> {
    let (before_comma, after_comma) = title.split_once(',')?;
    let before = before_comma.trim();
    let after = after_comma.trim();

    // Must have exactly two parts
    if after.is_empty() || before.is_empty() {
        return None;
    }

    // Before-comma part should look like a surname: single word, capitalised, <20 chars
    if before.contains(' ') || before.len() > 20 {
        return None;
    }
    if !before.chars().next()?.is_uppercase() {
        return None;
    }

    // After-comma part should look like a given name: 1-3 words (handles initials like "J. R. R.")
    let after_words: Vec<&str> = after.split_whitespace().collect();
    if after_words.is_empty() || after_words.len() > 4 {
        return None;
    }

    // Now check if any author field looks like a book title (>4 words or contains articles)
    let title_like_words = ["the", "a", "an", "of", "and", "in", "to", "for"];
    for author in authors {
        let words: Vec<&str> = author.split_whitespace().collect();
        let has_title_words = words
            .iter()
            .any(|w| title_like_words.contains(&w.to_lowercase().as_str()));
        if words.len() > 4 || (words.len() > 2 && has_title_words) {
            return Some(InversionResult {
                probable_author: format!("{after} {before}"),
                probable_title: author.clone(),
            });
        }
    }

    // Even without a title-like author, "Lastname, Firstname" is suspicious enough
    // Without an author that looks like a title, we can't suggest a replacement —
    // returning an empty probable_title would create a useless draft row.
    None
}
