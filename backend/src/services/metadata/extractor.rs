//! Transforms OpfData into structured metadata ready for DB storage.

use crate::services::epub::opf_layer;

use super::{inversion, isbn, sanitiser};

#[derive(Debug, Clone)]
pub struct ExtractedMetadata {
    pub title: Option<String>,
    pub sort_title: Option<String>,
    pub description: Option<String>,
    pub language: Option<String>,
    pub creators: Vec<ExtractedCreator>,
    pub publisher: Option<String>,
    pub pub_date: Option<time::Date>,
    pub isbn: Option<isbn::IsbnResult>,
    pub subjects: Vec<String>,
    pub series: Option<SeriesInfo>,
    /// Consumed by the enrichment confidence scorer (Step 7 task 14).
    #[allow(dead_code)]
    pub inversion: Option<inversion::InversionResult>,
    /// Confidence score 0.0-1.0 based on field completeness.
    pub confidence: f32,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ExtractedCreator {
    pub name: String,
    pub sort_name: String,
    pub role: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SeriesInfo {
    pub name: String,
    pub position: Option<f64>,
}

/// Extract and sanitise metadata from parsed OPF data.
pub fn extract(opf: &opf_layer::OpfData) -> ExtractedMetadata {
    let title = opf
        .title
        .as_deref()
        .map(sanitiser::sanitise)
        .filter(|s| !s.is_empty());
    // TODO: article stripping ("The", "A", "An") deferred — lowercasing only for now
    let sort_title = title.as_deref().map(|t| t.to_lowercase());
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
    if let Some(last_space) = name.rfind(' ') {
        let given = &name[..last_space];
        let surname = &name[last_space + 1..];
        format!("{surname}, {given}")
    } else {
        name.to_string()
    }
}

/// Map OPF role codes to author_role enum values.
fn map_role(role: Option<&str>) -> String {
    match role {
        Some("aut") => "author".into(),
        Some("edt") => "editor".into(),
        Some("trl") => "translator".into(),
        Some("nrt") => "narrator".into(),
        _ => "author".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::epub::opf_layer::{Creator, OpfData, SeriesMeta};
    use std::collections::HashMap;

    fn empty_opf() -> OpfData {
        OpfData {
            manifest: HashMap::new(),
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
