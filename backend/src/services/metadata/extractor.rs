//! Transforms `OpfData` into structured metadata ready for DB storage.
//!
//! The single entry point `extract` sanitises every text field via
//! [`crate::services::metadata::sanitiser`], resolves `ISBN` identifiers,
//! detects title-author inversion, and computes a completeness-based
//! confidence score. The output is passed downstream to `draft::write_drafts`.

use crate::services::epub::opf_layer;

use super::{inversion, isbn, sanitiser};

/// Fully processed metadata derived from a single `OPF` document.
///
/// Free-form text fields (title, description, creators, publisher, subjects,
/// series name) have been sanitised (entities decoded, `HTML` stripped,
/// whitespace normalised). The `language` field is passed through unchanged
/// because it is a structured `BCP 47` token, not free-form text — callers
/// must validate it themselves before trusting it. Optional fields are
/// `None` when absent or empty after sanitisation — callers must not treat
/// empty strings as valid values.
#[derive(Debug, Clone)]
pub struct ExtractedMetadata {
    /// Sanitised display title, or `None` if the `OPF` title was absent or empty.
    pub title: Option<String>,
    /// Lowercased sort key derived from `title`; absent when `title` is absent.
    pub sort_title: Option<String>,
    /// Sanitised book description / synopsis.
    pub description: Option<String>,
    /// `BCP 47` language tag as declared in the `OPF` (e.g. `"en"`, `"fr"`).
    pub language: Option<String>,
    /// Ordered list of sanitised creators (authors, editors, translators).
    pub creators: Vec<ExtractedCreator>,
    /// Sanitised publisher name.
    pub publisher: Option<String>,
    /// Publication date parsed from `OPF` `<dc:date>` in `YYYY`, `YYYY-MM`,
    /// or `YYYY-MM-DD` format. Partial dates default to the first of month/year.
    pub pub_date: Option<time::Date>,
    /// First valid `ISBN` found among the `OPF` identifiers. `None` when no
    /// recognisable valid `ISBN` was present (the extractor selects the first
    /// `IsbnResult` whose `valid` flag is set, so an invalid-only identifier
    /// list yields `None` rather than a `valid = false` result).
    pub isbn: Option<isbn::IsbnResult>,
    /// Sanitised subject/genre tags.
    pub subjects: Vec<String>,
    /// Series name and position parsed from Calibre-style `OPF` series metadata.
    pub series: Option<SeriesInfo>,
    /// Consumed by the enrichment confidence scorer (Step 7 task 14).
    #[allow(dead_code)]
    pub inversion: Option<inversion::InversionResult>,
    /// Confidence score 0.0-1.0 based on field completeness.
    pub confidence: f32,
}

/// A single contributor (author, editor, translator, narrator) extracted from `OPF`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ExtractedCreator {
    /// Sanitised display name as it appears in the `OPF`.
    pub name: String,
    /// Sort key in `"Surname, Given"` form; single-word names are returned as-is.
    pub sort_name: String,
    /// Contributor role mapped from the `OPF` `relator` code: `"author"`,
    /// `"editor"`, `"translator"`, or `"narrator"`. Unknown codes map to `"author"`.
    pub role: String,
}

/// Series membership parsed from Calibre-style `OPF` series metadata.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SeriesInfo {
    /// Sanitised series name.
    pub name: String,
    /// Position within the series (e.g. `1.0`, `2.5`). `None` when absent.
    pub position: Option<f64>,
}

/// Build an [`ExtractedMetadata`] from a parsed `OPF` document.
///
/// Applies the full sanitisation pipeline to every text field, resolves
/// `ISBN` identifiers, generates sort keys, detects title-author inversion,
/// and computes a completeness-based confidence score (base 0.3 + 0.1 per
/// present field, except `subjects` which contributes +0.05; capped at 1.0).
pub fn extract(opf: &opf_layer::OpfData) -> ExtractedMetadata {
    let title = opf
        .title
        .as_deref()
        .map(sanitiser::sanitise)
        .filter(|s| !s.is_empty());
    // TODO: article stripping ("The", "A", "An") deferred — lowercasing only for now
    let sort_title = title.as_deref().map(str::to_lowercase);
    let description = opf
        .description
        .as_deref()
        .map(sanitiser::sanitise)
        .filter(|s| !s.is_empty());
    let publisher = opf
        .publisher
        .as_deref()
        .map(sanitiser::sanitise)
        .filter(|s| !s.is_empty());
    let language = opf.language.clone();

    // Parse date: try YYYY-MM-DD, YYYY-MM, YYYY
    let pub_date = opf.date.as_deref().and_then(parse_date);

    // Parse ISBNs from identifiers — keep the first valid one
    let isbn = opf
        .identifiers
        .iter()
        .map(|id| isbn::parse_isbn(id))
        .find(|r| r.valid);

    // Map creators
    let creators: Vec<ExtractedCreator> = opf
        .creators
        .iter()
        .map(|c| {
            let name = sanitiser::sanitise(&c.name);
            let sort_name = generate_sort_name(&name);
            let role = map_role(c.role.as_deref());
            ExtractedCreator {
                name,
                sort_name,
                role,
            }
        })
        .collect();

    let subjects: Vec<String> = opf
        .subjects
        .iter()
        .map(|s| sanitiser::sanitise(s))
        .filter(|s| !s.is_empty())
        .collect();

    let series = opf.series_meta.as_ref().and_then(|s| {
        let name = sanitiser::sanitise(&s.name);
        if name.is_empty() {
            None
        } else {
            Some(SeriesInfo {
                name,
                position: s.position,
            })
        }
    });

    // Inversion detection
    let author_names: Vec<String> = creators.iter().map(|c| c.name.clone()).collect();
    let inversion = title
        .as_deref()
        .and_then(|t| inversion::detect_inversion(t, &author_names));

    // Confidence: base 0.3, +0.1 per present field, cap at 1.0
    let mut confidence: f32 = 0.3;
    if title.is_some() {
        confidence += 0.1;
    }
    if !creators.is_empty() {
        confidence += 0.1;
    }
    if isbn.is_some() {
        confidence += 0.1;
    }
    if publisher.is_some() {
        confidence += 0.1;
    }
    if pub_date.is_some() {
        confidence += 0.1;
    }
    if description.is_some() {
        confidence += 0.1;
    }
    if !subjects.is_empty() {
        confidence += 0.05;
    }
    let confidence = confidence.min(1.0);

    ExtractedMetadata {
        title,
        sort_title,
        description,
        language,
        creators,
        publisher,
        pub_date,
        isbn,
        subjects,
        series,
        inversion,
        confidence,
    }
}

/// Try to parse a date string in common OPF formats.
fn parse_date(s: &str) -> Option<time::Date> {
    let s = s.trim();
    // YYYY-MM-DD
    if let Ok(d) = time::Date::parse(
        s,
        &time::macros::format_description!("[year]-[month]-[day]"),
    ) {
        return Some(d);
    }
    // YYYY-MM (default to 1st of month)
    if s.len() >= 7 && s.chars().nth(4) == Some('-') {
        let padded = format!("{s}-01");
        if let Ok(d) = time::Date::parse(
            &padded,
            &time::macros::format_description!("[year]-[month]-[day]"),
        ) {
            return Some(d);
        }
    }
    // YYYY (default to Jan 1)
    if s.len() == 4
        && let Ok(year) = s.parse::<i32>()
    {
        return time::Date::from_calendar_date(year, time::Month::January, 1).ok();
    }
    None
}

/// Generate sort name: "J. R. R. Tolkien" → "Tolkien, J. R. R."
/// Single-word names are returned as-is.
fn generate_sort_name(name: &str) -> String {
    let name = name.trim();
    name.rfind(' ').map_or_else(
        || name.to_string(),
        |last_space| {
            let given = &name[..last_space];
            let surname = &name[last_space + 1..];
            format!("{surname}, {given}")
        },
    )
}

/// Map OPF role codes to `author_role` enum values.
fn map_role(role: Option<&str>) -> String {
    match role {
        Some("edt") => "editor".into(),
        Some("trl") => "translator".into(),
        Some("nrt") => "narrator".into(),
        // "aut", unknown OPF roles, and None all map to "author".
        _ => "author".into(),
    }
}

#[cfg(test)]
#[allow(
    clippy::float_cmp,
    reason = "test code: both sides are the same f32 literal propagated through identical rounding, so bitwise comparison is reliable"
)]
mod tests {
    use super::*;
    use crate::services::epub::opf_layer::{Creator, OpfData, SeriesMeta};
    use std::collections::HashMap;

    fn empty_opf() -> OpfData {
        OpfData {
            manifest: HashMap::new(),
            cover_href: None,
            spine_idrefs: vec![],
            opf_path: "OEBPS/content.opf".into(),
            accessibility_metadata: None,
            title: None,
            creators: vec![],
            description: None,
            publisher: None,
            date: None,
            language: None,
            identifiers: vec![],
            subjects: vec![],
            series_meta: None,
        }
    }

    #[test]
    fn extract_full_metadata() {
        let opf = OpfData {
            title: Some("The Hobbit".into()),
            creators: vec![Creator {
                name: "J. R. R. Tolkien".into(),
                role: Some("aut".into()),
            }],
            description: Some("<p>A fantasy novel</p>".into()),
            publisher: Some("Allen &amp; Unwin".into()),
            date: Some("1937-09-21".into()),
            language: Some("en".into()),
            identifiers: vec!["urn:isbn:9780547928227".into()],
            subjects: vec!["Fantasy".into()],
            series_meta: Some(SeriesMeta {
                name: "Middle-earth".into(),
                position: Some(1.0),
            }),
            ..empty_opf()
        };
        let m = extract(&opf);
        assert_eq!(m.title.as_deref(), Some("The Hobbit"));
        assert_eq!(m.sort_title.as_deref(), Some("the hobbit"));
        assert_eq!(m.description.as_deref(), Some("A fantasy novel"));
        assert_eq!(m.publisher.as_deref(), Some("Allen & Unwin"));
        assert!(m.pub_date.is_some());
        assert_eq!(m.creators[0].name, "J. R. R. Tolkien");
        assert_eq!(m.creators[0].sort_name, "Tolkien, J. R. R.");
        assert_eq!(m.creators[0].role, "author");
        assert!(m.isbn.as_ref().is_some_and(|i| i.valid));
        assert_eq!(m.series.as_ref().unwrap().name, "Middle-earth");
        assert!(m.confidence > 0.8);
    }

    #[test]
    fn extract_minimal_metadata() {
        let m = extract(&empty_opf());
        assert!(m.title.is_none());
        assert!(m.creators.is_empty());
        assert!(m.isbn.is_none());
        assert_eq!(m.confidence, 0.3);
    }

    #[test]
    fn date_parsing_variants() {
        assert!(parse_date("2020-01-15").is_some());
        assert!(parse_date("2020-01").is_some());
        assert!(parse_date("2020").is_some());
        assert!(parse_date("not-a-date").is_none());
        assert!(parse_date("").is_none());
    }

    #[test]
    fn sort_name_generation() {
        assert_eq!(generate_sort_name("J. R. R. Tolkien"), "Tolkien, J. R. R.");
        assert_eq!(generate_sort_name("Tolkien"), "Tolkien");
        assert_eq!(generate_sort_name("Mary Shelley"), "Shelley, Mary");
    }

    #[test]
    fn role_mapping() {
        assert_eq!(map_role(Some("aut")), "author");
        assert_eq!(map_role(Some("edt")), "editor");
        assert_eq!(map_role(Some("trl")), "translator");
        assert_eq!(map_role(Some("nrt")), "narrator");
        assert_eq!(map_role(Some("ill")), "author"); // unknown → author
        assert_eq!(map_role(None), "author");
    }

    #[test]
    fn multi_author_extraction() {
        let opf = OpfData {
            creators: vec![
                Creator {
                    name: "Author One".into(),
                    role: Some("aut".into()),
                },
                Creator {
                    name: "Editor Two".into(),
                    role: Some("edt".into()),
                },
            ],
            ..empty_opf()
        };
        let m = extract(&opf);
        assert_eq!(m.creators.len(), 2);
        assert_eq!(m.creators[1].role, "editor");
    }
}
