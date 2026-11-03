//! Canonical cache-key derivation for the metadata enrichment pipeline.
//!
//! Converts raw ISBN strings and title/author pairs into stable, normalised
//! keys so that ISBN-10 and ISBN-13 of the same book, or title strings that
//! differ only in case or whitespace, produce identical keys.

// Phase B building block: callers are wired in Phase C.

/// Return a stable key for the given raw ISBN string.
///
/// Resolves both ISBN-10 and ISBN-13 inputs to their ISBN-13 canonical form
/// using the ISBN crate, then prefixes with `"isbn:"`.  Returns `None` if the
/// string is not a valid ISBN.
#[must_use]
pub fn isbn_key(raw: &str) -> Option<String> {
    let value = match raw.parse::<isbn::Isbn>() {
        Ok(isbn::Isbn::_10(value)) => isbn::Isbn13::from(value),
        Ok(isbn::Isbn::_13(value)) => value,
        Err(_) => return None,
    };
    let compact = value.to_string();
    (compact.starts_with("978") || compact.starts_with("979")).then(|| format!("isbn:{compact}"))
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
#[must_use]
pub fn title_author_key(title: &str, author: &str) -> String {
    let t = normalise_text(title);
    let a = normalise_author(author);
    format!("ta:{t}|{a}")
}

fn normalise_author(raw: &str) -> String {
    // Swap "Last, First" → "First Last" before further normalisation.
    let swapped = if let Some((last, first)) = raw.split_once(',') {
        format!("{} {}", first.trim(), last.trim())
    } else {
        raw.to_string()
    };
    normalise_text(&swapped)
}

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
