use chrono::format::{Parsed, StrftimeItems, parse};
use regex::Regex;
use std::sync::LazyLock;

#[expect(
    clippy::expect_used,
    reason = "the literal grammar is valid by construction"
)]
static GRAMMAR: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\A[0-9]{4}(?:-[0-9]{2}(?:-[0-9]{2}(?:T[0-9]{2}:[0-9]{2}(?::[0-9]{2}(?:\.[0-9]+)?)?(?:Z|[+-][0-9]{2}:[0-9]{2}))?)?)?\z")
        .expect("publication-date grammar is valid")
});

/// Why non-empty publication text cannot become canonical.
#[derive(Debug, thiserror::Error)]
pub enum PublicationDateError {
    /// The value does not conform to the supported W3C grammar or calendar.
    #[error("Use YYYY-MM-DD or YYYY. Leave empty if unknown.")]
    Malformed,
    /// The four-digit year is outside the common era.
    #[error("year must be between 1 and 9999")]
    YearOutOfRange,
}

/// Validate bibliographic date text without supplying missing components or changing its precision.
///
/// # Errors
/// Returns a typed refusal for non-empty text outside the supported grammar or calendar range.
pub fn normalise(input: &str) -> Result<Option<String>, PublicationDateError> {
    let value = input.trim();
    if value.is_empty() {
        return Ok(None);
    }
    if !GRAMMAR.is_match(value) {
        return Err(PublicationDateError::Malformed);
    }
    let mut parsed = Parsed::new();
    let date_len = value.len().min(10);
    let format = match date_len {
        4 => "%Y",
        7 => "%Y-%m",
        _ => "%Y-%m-%d",
    };
    parse(&mut parsed, &value[..date_len], StrftimeItems::new(format))
        .map_err(|_| PublicationDateError::Malformed)?;
    if parsed.year() == Some(0) {
        return Err(PublicationDateError::YearOutOfRange);
    }
    if parsed.day().is_some() {
        parsed
            .to_naive_date()
            .map_err(|_| PublicationDateError::Malformed)?;
    }
    if value.len() > 10 {
        let time = &value[11..];
        let (time, offset) = time.strip_suffix('Z').map_or_else(
            || {
                let split = time.len() - 6;
                (&time[..split], Some(&time[split + 1..]))
            },
            |time| (time, None),
        );
        let number = |part: &str| {
            part.parse::<u32>()
                .map_err(|_| PublicationDateError::Malformed)
        };
        if number(&time[..2])? > 23
            || number(&time[3..5])? > 59
            || (time.len() > 5 && number(&time[6..8])? > 59)
        {
            return Err(PublicationDateError::Malformed);
        }
        if let Some(offset) = offset
            && (number(&offset[..2])? > 23 || number(&offset[3..])? > 59)
        {
            return Err(PublicationDateError::Malformed);
        }
    }
    Ok(Some(value.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::normalise;

    #[test]
    fn publication_date_normalisation_preserves_precision() {
        for value in [
            "2024",
            "2024-06",
            "2024-02-29",
            "2024-01-15T12:30Z",
            "2024-01-15T12:30:59-05:30",
            "2024-01-15T12:30:59.123456789123+05:30",
            "0001",
            "9999-12-31",
        ] {
            assert_eq!(normalise(value).unwrap().as_deref(), Some(value));
            assert_eq!(
                normalise(&format!(" {value} \n")).unwrap().as_deref(),
                Some(value)
            );
            assert_eq!(
                normalise(&normalise(value).unwrap().unwrap())
                    .unwrap()
                    .as_deref(),
                Some(value)
            );
        }
        for value in [
            "0000",
            "0000-01-01",
            "26-05-01",
            "-9999-01-01",
            "2024-1-15",
            "2024-01-5",
            "2024-00",
            "2024-13",
            "2023-02-29",
            "2024-04-31",
            "2024-01-15junk",
            "2024-01-15T12:30",
            "2024-01-15T24:00Z",
            "2024-01-15T12:60Z",
            "2024-01-15T12:00:60Z",
            "2024-01-15T12:00:00.Z",
            "2024-01-15T12:00+24:00",
            "2024-01-15T12:00+01:60",
            "January 2024",
            "01/02/2024",
            "unknown",
            "2024-01-€€€garbage",
        ] {
            assert!(normalise(value).is_err(), "{value}");
        }
        assert_eq!(normalise(" \n\t").unwrap(), None);
    }
}
