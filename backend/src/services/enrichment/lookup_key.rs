//! Canonical cache-key derivation for the metadata enrichment pipeline.
//!
//! Converts raw ISBN strings and title/author pairs into stable, normalised
//! keys so that ISBN-10 and ISBN-13 of the same book, or title strings that
//! differ only in case or whitespace, produce identical keys.

// Phase B building block: callers are wired in Phase C.

use crate::services::metadata::isbn::parse_isbn;

/// Return a stable key for the given raw ISBN string.
///
/// Resolves both ISBN-10 and ISBN-13 inputs to their ISBN-13 canonical form
/// via [`parse_isbn`], then prefixes with `"isbn:"`.  Returns `None` if the
/// string is not a valid ISBN.
pub fn isbn_key(raw: &str) -> Option<String> {
    let result = parse_isbn(raw);
    result.isbn_13.map(|s| format!("isbn:{s}"))
}

/// Return a stable key combining a title and author.
///
/// Normalisation applied to both title and author:
/// * Trim leading/trailing whitespace.
/// * Collapse internal whitespace runs to a single space.
/// * Convert to ASCII lowercase.
/// * Strip punctuation (keep alphanumerics and spaces).
///
/// Author-specific normalisation: if the author is in "Last, First" form
/// (contains a comma), swap to "First Last" order before the above steps.
///
/// Returns a key of the form `"ta:{title}|{author}"`.
#[allow(dead_code)] // title/author fallback is covered by tests; orchestrator wiring pending phase D.
pub fn title_author_key(title: &str, author: &str) -> String {
    let t = normalise_text(title);
    let a = normalise_author(author);
    format!("ta:{t}|{a}")
}

#[allow(dead_code)]
fn normalise_author(raw: &str) -> String {
    // Swap "Last, First" → "First Last" before further normalisation.
    let swapped = if let Some((last, first)) = raw.split_once(',') {
        format!("{} {}", first.trim(), last.trim())
    } else {
        raw.to_string()
    };
    normalise_text(&swapped)
}

#[allow(dead_code)]
fn normalise_text(s: &str) -> String {
    let lower = s.to_ascii_lowercase();
    // Strip punctuation: keep only alphanumeric chars and spaces.
    let stripped: String = lower
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == ' ' {
                c
            } else {
                ' '
            }
        })
        .collect();
    // Collapse whitespace and trim.
    stripped.split_whitespace().collect::<Vec<&str>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn isbn10_and_isbn13_same_book_produce_same_key() {
        let key10 = isbn_key("0306406152");
        let key13 = isbn_key("9780306406157");
        assert!(key10.is_some());
        assert_eq!(key10, key13);
        assert_eq!(key10.unwrap(), "isbn:9780306406157");
    }

    #[test]
    fn isbn_key_with_hyphens() {
        let key = isbn_key("978-0-306-40615-7");
        assert_eq!(key.as_deref(), Some("isbn:9780306406157"));
    }

    #[test]
    fn invalid_isbn_returns_none() {
        assert!(isbn_key("1234567890").is_none());
        assert!(isbn_key("not-an-isbn").is_none());
        assert!(isbn_key("").is_none());
    }

    #[test]
    fn title_author_key_whitespace_and_case_insensitive() {
        let a = title_author_key("Dune", "Frank Herbert");
        let b = title_author_key("  dune  ", "frank herbert");
        assert_eq!(a, b);
    }

    #[test]
    fn title_author_key_last_first_swapped() {
        let a = title_author_key("Dune", "Frank Herbert");
        let b = title_author_key("  dune  ", "Herbert, Frank");
        assert_eq!(a, b);
    }

    #[test]
    fn title_author_key_punctuation_stripped() {
        let a = title_author_key("Dune", "Frank Herbert");
        let b = title_author_key("Dune!", "Frank. Herbert");
        assert_eq!(a, b);
    }

    #[test]
    fn title_author_key_internal_whitespace_collapsed() {
        let a = title_author_key("Dune", "Frank Herbert");
        let b = title_author_key("Dune", "Frank  Herbert");
        assert_eq!(a, b);
    }

    #[test]
    fn title_author_key_format() {
        let k = title_author_key("Dune", "Frank Herbert");
        assert_eq!(k, "ta:dune|frank herbert");
    }

    #[test]
    fn title_author_key_accented_chars_stripped_to_space() {
        // Non-ASCII chars are not alphanumeric in ASCII sense; they become spaces.
        let a = title_author_key("Dune", "Frank Herbert");
        // é → space → collapses with trim
        let b = title_author_key("Dune", "Fränk Hérbert");
        // Not equal — accents affect the name — but the function is stable.
        // Check the accented form produces a stable key.
        let c = title_author_key("Dune", "Fränk Hérbert");
        assert_eq!(b, c);
        // Accented ≠ plain (different names)
        assert_ne!(a, b);
    }
}
