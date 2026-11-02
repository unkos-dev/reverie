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
/// series name) have been sanitised (`HTML` stripped, whitespace
/// normalised); entity and character-reference decoding happened earlier,
/// at the `OPF` parse layer, so sanitisation receives already-decoded
/// character data. The `language` field is passed through unchanged because
/// it is a structured `BCP 47` token, not free-form text; callers must
/// validate it themselves before trusting it. Optional fields are `None`
/// when absent or empty after sanitisation; callers must not treat empty
/// strings as valid values.
#[derive(Debug, Clone)]
pub struct ExtractedMetadata {
    /// Sanitised display title, or `None` if the `OPF` title was absent or empty.
    pub title: Option<String>,
    /// Lowercased sort key derived from `title`; absent when `title` is absent.
    pub sort_title: Option<String>,
    /// Sanitised declared subtitle, or `None` if absent (no colon-split heuristics).
    pub subtitle: Option<String>,
    /// Sanitised book description / synopsis.
    pub description: Option<String>,
    /// `BCP 47` language tag as declared in the `OPF` (e.g. `"en"`, `"fr"`).
    pub language: Option<String>,
    /// Ordered list of sanitised creators (authors, editors, translators, narrators).
    pub creators: Vec<ExtractedCreator>,
    /// Contributors with an unmapped or absent relator code, staged for
    /// manual review rather than guessed as `"author"`.
    pub unmapped_contributors: Vec<UnmappedContributor>,
    /// Page count parsed from `schema:numberOfPages` (a community convention,
    /// not `EPUB 3.3` core). `None` when absent, non-numeric, or not positive.
    pub pages: Option<i32>,
    /// Sanitised publisher name.
    pub publisher: Option<String>,
    /// Non-empty publication text selected from `OPF` `<dc:date>`, retained
    /// before validation and canonical promotion.
    pub pub_date: Option<String>,
    /// First valid `ISBN` found among the `OPF` identifiers. `None` when no
    /// recognisable valid `ISBN` was present (the extractor selects the first
    /// `IsbnResult` whose `valid` flag is set, so an invalid-only identifier
    /// list yields `None` rather than a `valid = false` result).
    pub isbn: Option<isbn::IsbnResult>,
    /// Sanitised subject/genre tags.
    pub subjects: Vec<String>,
    /// Series name and position parsed from Calibre-style `OPF` series metadata.
    pub series: Option<SeriesInfo>,
    /// Consumed by the enrichment confidence scorer.
    pub inversion: Option<inversion::InversionResult>,
    /// Confidence score 0.0-1.0 based on field completeness.
    pub confidence: f32,
}

impl ExtractedMetadata {
    /// First creator carrying the `author` role, in document order.
    ///
    /// `creators` is role-mixed and document-ordered, so an editor or
    /// translator can precede the author; consumers that need "the author"
    /// (work matching, library path rendering) must never fall back to a
    /// non-author role.
    #[must_use]
    pub fn first_author(&self) -> Option<&ExtractedCreator> {
        self.creators.iter().find(|c| c.role == "author")
    }
}

/// A single contributor (author, editor, translator, narrator) extracted from `OPF`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ExtractedCreator {
    /// Sanitised display name as it appears in the `OPF`.
    pub name: String,
    /// Sort key in `"Surname, Given"` form; single-word names are returned as-is.
    pub sort_name: String,
    /// Contributor role mapped from a `MARC` relator code: `"author"`,
    /// `"editor"`, `"translator"`, or `"narrator"`. A `dc:creator` with no
    /// declared role also maps to `"author"`; unmapped codes never guess a
    /// role here and instead surface via [`ExtractedMetadata::unmapped_contributors`].
    pub role: String,
}

/// A contributor whose relator code did not map to a tracked role, or who had
/// no code at all (a bare `dc:contributor`). Staged for manual review rather
/// than guessed.
#[derive(Debug, Clone, serde::Serialize)]
pub struct UnmappedContributor {
    /// Sanitised display name as it appears in the `OPF`.
    pub name: String,
    /// The raw, unmapped relator code, or `None` when no code was declared.
    pub code: Option<String>,
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
    let subtitle = opf
        .subtitle
        .as_deref()
        .map(sanitiser::sanitise)
        .filter(|s| !s.is_empty());
    let pages = opf.number_of_pages.as_deref().and_then(|raw| {
        let parsed = raw.trim().parse::<i32>().ok().filter(|n| *n > 0);
        if parsed.is_none() {
            tracing::debug!(raw, "numberOfPages not a positive integer, discarding");
        }
        parsed
    });

    let pub_date = opf
        .date
        .as_ref()
        .filter(|value| !value.trim().is_empty())
        .cloned();

    // Parse ISBNs from identifiers — keep the first valid one
    let isbn = opf
        .identifiers
        .iter()
        .map(|id| isbn::parse_isbn(id))
        .find(|r| r.valid);

    let (creators, unmapped_contributors) = collect_creators(&opf.creators);

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

    let mut meta = ExtractedMetadata {
        title,
        sort_title,
        subtitle,
        description,
        language,
        creators,
        unmapped_contributors,
        pages,
        publisher,
        pub_date,
        isbn,
        subjects,
        series,
        inversion,
        confidence: 0.0,
    };
    meta.confidence = completeness_confidence(&meta);
    meta
}

/// Map creators/contributors to role-tagged [`ExtractedCreator`]s plus the
/// staged unmapped set. Each declared role on a creator/contributor yields
/// one entry; no role at all defaults `dc:creator` to `"author"` but leaves
/// `dc:contributor` unmapped (never guessed). Unmapped or unknown relator
/// codes are staged, not dropped. Same (name, role) pair appearing twice
/// keeps only the first occurrence, preserving document order.
fn collect_creators(
    raw: &[opf_layer::Creator],
) -> (Vec<ExtractedCreator>, Vec<UnmappedContributor>) {
    let mut creators: Vec<ExtractedCreator> = Vec::new();
    let mut unmapped: Vec<UnmappedContributor> = Vec::new();
    let mut seen_roles: std::collections::HashSet<(String, String)> =
        std::collections::HashSet::new();

    let push_unique = |creators: &mut Vec<ExtractedCreator>,
                       seen: &mut std::collections::HashSet<(String, String)>,
                       name: &str,
                       role: &str| {
        if seen.insert((name.to_string(), role.to_string())) {
            creators.push(ExtractedCreator {
                name: name.to_string(),
                sort_name: generate_sort_name(name),
                role: role.to_string(),
            });
        }
    };

    for c in raw {
        let name = sanitiser::sanitise(&c.name);
        if c.roles.is_empty() {
            if c.from_contributor {
                tracing::info!(name = %name, "unmapped contributor: no relator code declared");
                unmapped.push(UnmappedContributor { name, code: None });
            } else {
                push_unique(&mut creators, &mut seen_roles, &name, "author");
            }
            continue;
        }
        for code in &c.roles {
            if let Some(role) = map_relator(code) {
                push_unique(&mut creators, &mut seen_roles, &name, role);
            } else {
                tracing::info!(code = code.as_str(), name = %name, "unmapped contributor relator code");
                unmapped.push(UnmappedContributor {
                    name: name.clone(),
                    code: Some(code.clone()),
                });
            }
        }
    }
    (creators, unmapped)
}

/// Completeness-based confidence: base 0.3, +0.1 per present field
/// (`subjects` contributes +0.05), capped at 1.0.
fn completeness_confidence(meta: &ExtractedMetadata) -> f32 {
    let mut confidence: f32 = 0.3;
    if meta.title.is_some() {
        confidence += 0.1;
    }
    if !meta.creators.is_empty() {
        confidence += 0.1;
    }
    if meta.isbn.is_some() {
        confidence += 0.1;
    }
    if meta.publisher.is_some() {
        confidence += 0.1;
    }
    if meta.pub_date.is_some() {
        confidence += 0.1;
    }
    if meta.description.is_some() {
        confidence += 0.1;
    }
    if !meta.subjects.is_empty() {
        confidence += 0.05;
    }
    confidence.min(1.0)
}

/// Generate sort name: "J. R. R. Tolkien" → "Tolkien, J. R. R."
/// Single-word names are returned as-is. Shared with the PATCH contributors
/// path (`routes::metadata`) so manually-entered names sort the same way as
/// extracted ones; do not duplicate this logic elsewhere.
pub(crate) fn generate_sort_name(name: &str) -> String {
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

/// Map a `MARC` relator code to a tracked `author_role` value. Unmapped codes
/// (including `"aut"`'s absence of a special case — see below) return `None`
/// so callers stage them instead of guessing a role.
fn map_relator(code: &str) -> Option<&'static str> {
    match code {
        "aut" => Some("author"),
        "edt" => Some("editor"),
        "trl" => Some("translator"),
        "nrt" => Some("narrator"),
        _ => None,
    }
}
