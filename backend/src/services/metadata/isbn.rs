//! `ISBN-10` and `ISBN-13` checksum validation, conversion, and normalisation.
//!
//! Accepts raw identifier strings in common `OPF` formats (`urn:isbn:`, `ISBN:`,
//! hyphenated, space-separated). All public functions normalise input before
//! validation, so callers do not need to pre-strip hyphens or prefixes.

/// Normalised `ISBN` identifiers extracted from an `OPF` identifier field.
///
/// Both forms are populated when derivable: an `ISBN-13` with `978` prefix can
/// yield an `ISBN-10`, and any valid `ISBN-10` is converted to `ISBN-13`.
/// When neither checksum validates, `valid` is `false` and both fields are
/// `None`.
#[derive(Debug, Clone)]
pub struct IsbnResult {
    /// Normalised `ISBN-10` (digits only, `X` check digit uppercase), if present and valid.
    pub isbn_10: Option<String>,
    /// Normalised `ISBN-13` (digits only), if present and valid.
    pub isbn_13: Option<String>,
    /// `true` when at least one form passed checksum validation.
    pub valid: bool,
}

/// Strip hyphens, spaces, and common prefixes. Uppercase for ISBN-10 X digit.
fn normalise(raw: &str) -> String {
    let s = raw.trim();
    // Strip common prefixes
    let s = s
        .strip_prefix("urn:isbn:")
        .or_else(|| s.strip_prefix("URN:ISBN:"))
        .or_else(|| s.strip_prefix("isbn:"))
        .or_else(|| s.strip_prefix("ISBN:"))
        .or_else(|| s.strip_prefix("ISBN "))
        .unwrap_or(s);
    s.chars()
        .filter(|c| !matches!(c, '-' | ' '))
        .collect::<String>()
        .to_ascii_uppercase()
}

/// Validate ISBN-10 checksum: sum of digit\[i\] * (10 - i) for i=0..10, mod 11 == 0.
/// Check digit may be 'X' (value 10).
pub fn validate_isbn10(isbn: &str) -> bool {
    let isbn = normalise(isbn);
    if isbn.len() != 10 {
        return false;
    }
    let mut sum: u32 = 0;
    for (i, c) in isbn.chars().enumerate() {
        let val = if i == 9 && c == 'X' {
            10
        } else if let Some(d) = c.to_digit(10) {
            d
        } else {
            return false;
        };
        sum += val * (10 - u32::try_from(i).unwrap_or(0));
    }
    sum.is_multiple_of(11)
}

/// Validate ISBN-13 checksum: alternating 1/3 weights, mod 10 == 0.
pub fn validate_isbn13(isbn: &str) -> bool {
    let isbn = normalise(isbn);
    if isbn.len() != 13 {
        return false;
    }
    let mut sum: u32 = 0;
    for (i, c) in isbn.chars().enumerate() {
        let Some(d) = c.to_digit(10) else {
            return false;
        };
        let weight = if i % 2 == 0 { 1 } else { 3 };
        sum += d * weight;
    }
    sum.is_multiple_of(10)
}

/// Convert a valid ISBN-10 to ISBN-13. Returns None if input is not a valid ISBN-10.
pub fn isbn10_to_isbn13(isbn10: &str) -> Option<String> {
    let isbn10 = normalise(isbn10);
    if !validate_isbn10(&isbn10) {
        return None;
    }
    // ISBN-13 = "978" + first 9 digits of ISBN-10 + new check digit
    let prefix = format!("978{}", &isbn10[..9]);
    let mut sum: u32 = 0;
    for (i, c) in prefix.chars().enumerate() {
        let d = c.to_digit(10)?;
        let weight = if i % 2 == 0 { 1 } else { 3 };
        sum += d * weight;
    }
    let check = (10 - (sum % 10)) % 10;
    Some(format!("{prefix}{check}"))
}

/// Validate `raw` as an `ISBN-10` and return its normalised form (digits only,
/// uppercase `X` check digit, hyphens/spaces/prefixes stripped). `None` when the
/// length or check digit is invalid.
///
/// Used by the metadata PATCH handler to store ISBNs in the same digits-only
/// shape the ingestion pipeline produces, so rematch's exact-equality join
/// finds ingested twins regardless of operator input formatting.
pub fn checked_isbn10(raw: &str) -> Option<String> {
    validate_isbn10(raw).then(|| normalise(raw))
}

/// Validate `raw` as an `ISBN-13` and return its normalised form (digits only,
/// hyphens/spaces/prefixes stripped). `None` when the length or check digit is
/// invalid. See [`checked_isbn10`] for the normalisation rationale.
pub fn checked_isbn13(raw: &str) -> Option<String> {
    validate_isbn13(raw).then(|| normalise(raw))
}

/// Parse a raw identifier string: strip prefixes, normalise, detect length, validate.
pub fn parse_isbn(raw: &str) -> IsbnResult {
    let normalised = normalise(raw);

    if normalised.len() == 13 && validate_isbn13(&normalised) {
        // Try to derive ISBN-10 if it starts with 978
        let isbn_10 = if normalised.starts_with("978") {
            isbn13_to_isbn10(&normalised)
        } else {
            None
        };
        return IsbnResult {
            isbn_10,
            isbn_13: Some(normalised),
            valid: true,
        };
    }

    if normalised.len() == 10 && validate_isbn10(&normalised) {
        let isbn_13 = isbn10_to_isbn13(&normalised);
        return IsbnResult {
            isbn_10: Some(normalised),
            isbn_13,
            valid: true,
        };
    }

    IsbnResult {
        isbn_10: None,
        isbn_13: None,
        valid: false,
    }
}

/// Convert ISBN-13 (978-prefix only) back to ISBN-10.
fn isbn13_to_isbn10(isbn13: &str) -> Option<String> {
    if !isbn13.starts_with("978") || isbn13.len() != 13 {
        return None;
    }
    let core = &isbn13[3..12]; // 9 digits after "978", before check digit
    let mut sum: u32 = 0;
    for (i, c) in core.chars().enumerate() {
        let d = c.to_digit(10)?;
        sum += d * (10 - u32::try_from(i).unwrap_or(0));
    }
    let check = (11 - (sum % 11)) % 11;
    let check_char = if check == 10 {
        'X'
    } else {
        char::from_digit(check, 10)?
    };
    Some(format!("{core}{check_char}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn isbn10_valid() {
        assert!(validate_isbn10("0-306-40615-2"));
    }

    #[test]
    fn isbn10_with_x() {
        assert!(validate_isbn10("0-8044-2957-X"));
    }

    #[test]
    fn isbn10_invalid() {
        assert!(!validate_isbn10("0-306-40615-0"));
    }

    #[test]
    fn isbn13_valid() {
        assert!(validate_isbn13("978-0-306-40615-7"));
    }

    #[test]
    fn isbn13_invalid() {
        assert!(!validate_isbn13("978-0-306-40615-0"));
    }

    #[test]
    fn isbn10_to_13_conversion() {
        let result = isbn10_to_isbn13("0306406152").unwrap();
        assert_eq!(result, "9780306406157");
    }

    #[test]
    fn isbn13_to_10_conversion() {
        let result = isbn13_to_isbn10("9780306406157").unwrap();
        assert_eq!(result, "0306406152");
    }

    #[test]
    fn parse_isbn_from_urn() {
        let result = parse_isbn("urn:isbn:9780306406157");
        assert!(result.valid);
        assert_eq!(result.isbn_13.as_deref(), Some("9780306406157"));
        assert_eq!(result.isbn_10.as_deref(), Some("0306406152"));
    }

    #[test]
    fn parse_isbn10() {
        let result = parse_isbn("0-306-40615-2");
        assert!(result.valid);
        assert_eq!(result.isbn_10.as_deref(), Some("0306406152"));
        assert_eq!(result.isbn_13.as_deref(), Some("9780306406157"));
    }

    #[test]
    fn parse_non_isbn() {
        let result = parse_isbn("urn:uuid:12345-abcde");
        assert!(!result.valid);
        assert!(result.isbn_10.is_none());
        assert!(result.isbn_13.is_none());
    }

    #[test]
    fn parse_empty() {
        let result = parse_isbn("");
        assert!(!result.valid);
    }

    #[test]
    fn isbn10_x_check_digit_parsed() {
        let result = parse_isbn("0-8044-2957-X");
        assert!(result.valid);
        assert_eq!(result.isbn_10.as_deref(), Some("080442957X"));
    }

    #[test]
    fn checked_isbn13_normalises_valid() {
        assert_eq!(
            checked_isbn13("978-0-306-40615-7").as_deref(),
            Some("9780306406157")
        );
    }

    #[test]
    fn checked_isbn13_rejects_bad_check_digit() {
        assert_eq!(checked_isbn13("9780306406150"), None);
    }

    #[test]
    fn checked_isbn10_normalises_and_uppercases_x() {
        assert_eq!(
            checked_isbn10("0-8044-2957-x").as_deref(),
            Some("080442957X")
        );
    }

    #[test]
    fn checked_isbn10_rejects_wrong_length() {
        assert_eq!(checked_isbn10("12345"), None);
    }
}
