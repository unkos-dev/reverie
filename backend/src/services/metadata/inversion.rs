//! Heuristic title-author inversion detection.
//!
//! Detects cases where the title field looks like "Lastname, Firstname" and
//! an author field looks like a book title. Advisory only — results are stored
//! as draft metadata, never auto-applied.

/// Fields consumed by the enrichment confidence scorer (Step 7 task 14).
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct InversionResult {
    pub probable_author: String,
    pub probable_title: String,
}

/// Check if a title looks like "Lastname, Firstname" (a swapped author name).
/// Returns Some if inversion is detected with a matching author that looks like a title.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_inversion_with_title_like_author() {
        let result = detect_inversion("Smith, John", &["The Great Adventure".into()]);
        assert!(result.is_some());
        let r = result.unwrap();
        assert_eq!(r.probable_author, "John Smith");
        assert_eq!(r.probable_title, "The Great Adventure");
    }

    #[test]
    fn no_inversion_without_title_candidate() {
        // "Smith, John" looks like an inverted name, but without an author that
        // looks like a title there's nothing to suggest as a replacement.
        let result = detect_inversion("Smith, John", &[]);
        assert!(result.is_none());
    }

    #[test]
    fn no_inversion_normal_title() {
        let result = detect_inversion("The Great Gatsby", &["F. Scott Fitzgerald".into()]);
        assert!(result.is_none());
    }

    #[test]
    fn no_inversion_murder_she_wrote() {
        // "She Wrote" has 2 words after comma — but "Murder" before comma looks like
        // a title word not a surname. However our heuristic only checks word count
        // after comma. "She Wrote" = 2 words, which is fine for a given name.
        // But "Murder" IS a single capitalised word. The key protection is that
        // we also need an author that looks like a title.
        let result = detect_inversion("Murder, She Wrote", &["Angela Lansbury".into()]);
        assert!(result.is_none());
    }

    #[test]
    fn no_inversion_no_comma() {
        let result = detect_inversion("The Hobbit", &["Tolkien".into()]);
        assert!(result.is_none());
    }

    #[test]
    fn no_inversion_multi_word_before_comma() {
        let result = detect_inversion("A Tale, Of Two Cities", &[]);
        assert!(result.is_none());
    }
}
