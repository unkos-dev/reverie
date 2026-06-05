//! `OPF` package-document parsing layer (Layer 3).
//!
//! Parses the `OPF` file to extract the manifest (id → href map), the reading
//! spine, Dublin Core bibliographic metadata, W3C accessibility `<meta>`
//! elements, and series metadata from both calibre (`<meta name="calibre:…">`)
//! and `EPUB3` `belongs-to-collection`. Broken spine refs (idrefs not in the
//! manifest) are removed and recorded as `Repaired` issues. Manifest hrefs
//! that fail the path-safety check are dropped and recorded as `Degraded`.

use quick_xml::Reader;
use quick_xml::events::Event;
use std::collections::{HashMap, HashSet};

use super::{
    Issue, IssueKind, Layer, Severity,
    zip_layer::{ZipHandle, read_entry},
};

/// A Dublin Core `dc:creator` entry with an optional `MARC` relator role.
#[derive(Debug, Clone)]
pub struct Creator {
    /// The creator's name as it appears in the `OPF` metadata.
    pub name: String,
    /// Optional `MARC` relator code (e.g. `"aut"`, `"edt"`) from `opf:role` or `role` attribute.
    pub role: Option<String>,
}

/// Series affiliation metadata sourced from calibre or `EPUB3` collection elements.
#[derive(Debug, Clone)]
pub struct SeriesMeta {
    /// Series title (calibre `calibre:series` or `EPUB3` `belongs-to-collection`).
    pub name: String,
    /// Position within the series, if declared.
    pub position: Option<f64>,
}

/// All data extracted from the `OPF` package document in a single parse pass.
#[derive(Debug, Clone)]
pub struct OpfData {
    /// All manifest items: id → href
    pub manifest: HashMap<String, String>,
    /// Spine idrefs (after removing broken refs)
    pub spine_idrefs: Vec<String>,
    /// `OPF` path within the archive (needed by repair and other layers)
    pub opf_path: String,
    /// Raw W3C accessibility metadata from `<meta>` elements, if any
    pub accessibility_metadata: Option<serde_json::Value>,
    /// Dublin Core: title
    pub title: Option<String>,
    /// Dublin Core: creators with optional role
    pub creators: Vec<Creator>,
    /// Dublin Core: description (may contain HTML)
    pub description: Option<String>,
    /// Dublin Core: publisher
    pub publisher: Option<String>,
    /// Dublin Core: date (raw string)
    pub date: Option<String>,
    /// Dublin Core: language
    pub language: Option<String>,
    /// Dublin Core: all identifier values (ISBNs, URNs, etc.)
    pub identifiers: Vec<String>,
    /// Dublin Core: subject values
    pub subjects: Vec<String>,
    /// Series metadata (calibre or `EPUB3` collection)
    pub series_meta: Option<SeriesMeta>,
}

/// Extract the local name from a possibly-namespaced element name.
/// e.g. b"dc:title" → b"title", b"title" → b"title"
fn local_name(name: &[u8]) -> &[u8] {
    name.iter()
        .position(|&b| b == b':')
        .map_or(name, |pos| &name[pos + 1..])
}

/// Parse the `OPF` package document at `opf_path` and return structured metadata.
///
/// Extracts the manifest, spine, Dublin Core fields, W3C accessibility `<meta>`
/// elements, and series metadata in a single `quick_xml` pass. Broken spine
/// idrefs (not present in the manifest) are removed and recorded as `Repaired`
/// issues. Manifest hrefs failing the path-safety check are dropped with a
/// `Degraded` issue.
///
/// Returns `None` if `opf_path` is `None`, the entry cannot be read from
/// `handle`, or the entry bytes are not valid `UTF-8`.
#[allow(
    clippy::too_many_lines,
    reason = "OPF parser handles the full EPUB 2/3 metadata element set in one pass; the per-element cases are mechanical and cannot meaningfully be split without introducing a second parse pass"
)]
pub fn validate(
    handle: &ZipHandle,
    opf_path: Option<&str>,
    issues: &mut Vec<Issue>,
) -> Option<OpfData> {
    let path = opf_path?;
    let bytes = read_entry(handle, path)?;
    let xml = std::str::from_utf8(&bytes).ok()?;

    let mut manifest: HashMap<String, String> = HashMap::new();
    let mut spine_idrefs: Vec<String> = Vec::new();
    let mut accessibility_meta: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();

    // Dublin Core fields
    let mut title: Option<String> = None;
    let mut creators: Vec<Creator> = Vec::new();
    let mut description: Option<String> = None;
    let mut publisher: Option<String> = None;
    let mut date: Option<String> = None;
    let mut language: Option<String> = None;
    let mut identifiers: Vec<String> = Vec::new();
    let mut subjects: Vec<String> = Vec::new();

    // Series metadata (calibre or EPUB 3).
    // EPUB 3 group-position is matched to its collection by `refines` target,
    // independent of document order: positions are buffered into a map keyed
    // by the refines target (`#<collection-id>`) as they are seen, then joined
    // to the collection's id at the end of the pass (UNK-234).
    let mut calibre_series_name: Option<String> = None;
    let mut calibre_series_index: Option<f64> = None;
    let mut epub3_collection_name: Option<String> = None;
    let mut epub3_collection_id: Option<String> = None;
    let mut group_positions: HashMap<String, f64> = HashMap::new();

    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    loop {
        match reader.read_event().ok()? {
            // EPUB 3 text-content meta: <meta property="schema:accessMode">textual</meta>
            // Also handles belongs-to-collection and group-position.
            // Must come BEFORE general Event::Start arm to avoid shadowing.
            Event::Start(e) if e.name().as_ref() == b"meta" => {
                let e = e.into_owned(); // release reader buffer borrow before read_text
                let prop = e
                    .attributes()
                    .flatten()
                    .find(|a| a.key.as_ref() == b"property")
                    .and_then(|a| {
                        std::str::from_utf8(&a.value)
                            .ok()
                            .map(std::string::ToString::to_string)
                    });
                let content_attr = e
                    .attributes()
                    .flatten()
                    .find(|a| a.key.as_ref() == b"content")
                    .and_then(|a| {
                        std::str::from_utf8(&a.value)
                            .ok()
                            .map(std::string::ToString::to_string)
                    });
                let id_attr = e
                    .attributes()
                    .flatten()
                    .find(|a| a.key.as_ref() == b"id")
                    .and_then(|a| {
                        std::str::from_utf8(&a.value)
                            .ok()
                            .map(std::string::ToString::to_string)
                    });
                let refines_attr = e
                    .attributes()
                    .flatten()
                    .find(|a| a.key.as_ref() == b"refines")
                    .and_then(|a| {
                        std::str::from_utf8(&a.value)
                            .ok()
                            .map(std::string::ToString::to_string)
                    });

                if let Some(ref prop) = prop {
                    if prop == "belongs-to-collection" {
                        let text = reader
                            .read_text(e.name())
                            .ok()
                            .and_then(|t| t.decode().ok())
                            .map(|t| t.trim().to_string())
                            .filter(|s| !s.is_empty());
                        if let Some(name) = text {
                            epub3_collection_name = Some(name);
                            epub3_collection_id = id_attr;
                        }
                        continue;
                    }
                    if prop == "group-position" {
                        // Buffer the position keyed by its refines target,
                        // regardless of whether the collection has been seen
                        // yet — resolved against the collection id at end of
                        // pass so element order doesn't matter (UNK-234).
                        if let Some(refines) = refines_attr {
                            let text = content_attr.or_else(|| {
                                reader
                                    .read_text(e.name())
                                    .ok()
                                    .and_then(|t| t.decode().ok())
                                    .map(|t| t.trim().to_string())
                                    .filter(|s| !s.is_empty())
                            });
                            if let Some(pos) = text.and_then(|t| t.parse::<f64>().ok()) {
                                group_positions.insert(refines, pos);
                            }
                        }
                        continue;
                    }
                    if prop.starts_with("schema:access") || prop.starts_with("dcterms:") {
                        let val = content_attr.or_else(|| {
                            reader
                                .read_text(e.name())
                                .ok()
                                .and_then(|t| t.decode().ok())
                                .map(|t| t.trim().to_string())
                                .filter(|s| !s.is_empty())
                        });
                        if let Some(v) = val {
                            accessibility_meta.insert(prop.clone(), serde_json::Value::String(v));
                        }
                    }
                }
            }
            // EPUB 2 attribute-style meta: <meta name="..." content="..."/>
            Event::Empty(e) if e.name().as_ref() == b"meta" => {
                let prop = e
                    .attributes()
                    .flatten()
                    .find(|a| a.key.as_ref() == b"property")
                    .and_then(|a| {
                        std::str::from_utf8(&a.value)
                            .ok()
                            .map(std::string::ToString::to_string)
                    });
                let name_attr = e
                    .attributes()
                    .flatten()
                    .find(|a| a.key.as_ref() == b"name")
                    .and_then(|a| {
                        std::str::from_utf8(&a.value)
                            .ok()
                            .map(std::string::ToString::to_string)
                    });
                let content = e
                    .attributes()
                    .flatten()
                    .find(|a| a.key.as_ref() == b"content")
                    .and_then(|a| {
                        std::str::from_utf8(&a.value)
                            .ok()
                            .map(std::string::ToString::to_string)
                    });
                let refines_attr = e
                    .attributes()
                    .flatten()
                    .find(|a| a.key.as_ref() == b"refines")
                    .and_then(|a| {
                        std::str::from_utf8(&a.value)
                            .ok()
                            .map(std::string::ToString::to_string)
                    });

                // EPUB 3 self-closing group-position: value lives in the
                // `content` attr, no text body (quick-xml fires Event::Empty,
                // not Event::Start). Mirrors the Event::Start arm's
                // buffer-and-resolve (UNK-315, the self-closing gap UNK-234
                // left open).
                if let Some(ref prop) = prop
                    && prop == "group-position"
                {
                    if let Some(refines) = refines_attr
                        && let Some(pos) = content.as_deref().and_then(|c| c.parse::<f64>().ok())
                    {
                        group_positions.insert(refines, pos);
                    }
                    continue;
                }

                // Accessibility meta via property attribute
                if let Some(ref prop) = prop
                    && (prop.starts_with("schema:access") || prop.starts_with("dcterms:"))
                    && let Some(ref v) = content
                {
                    accessibility_meta.insert(prop.clone(), serde_json::Value::String(v.clone()));
                }

                // Calibre series meta via name attribute
                if let Some(ref name) = name_attr
                    && let Some(ref c) = content
                {
                    match name.as_str() {
                        "calibre:series" => calibre_series_name = Some(c.clone()),
                        "calibre:series_index" => calibre_series_index = c.parse::<f64>().ok(),
                        _ => {}
                    }
                }
            }
            // Dublin Core elements: <dc:title>, <dc:creator>, etc.
            Event::Start(e)
                if matches!(
                    local_name(e.name().as_ref()),
                    b"title"
                        | b"creator"
                        | b"description"
                        | b"publisher"
                        | b"date"
                        | b"language"
                        | b"identifier"
                        | b"subject"
                ) && e.name().as_ref() != b"meta" =>
            {
                let local = local_name(e.name().as_ref()).to_vec();
                // Extract opf:role for creators (EPUB 2).
                // EPUB 3 uses <meta refines="#id" property="role"> — not resolved here (MVP).
                let role = e
                    .attributes()
                    .flatten()
                    .find(|a| {
                        let k = a.key.as_ref();
                        k == b"opf:role" || k == b"role"
                    })
                    .and_then(|a| {
                        std::str::from_utf8(&a.value)
                            .ok()
                            .map(std::string::ToString::to_string)
                    });

                let e = e.into_owned();
                let text = reader
                    .read_text(e.name())
                    .ok()
                    .and_then(|t| t.decode().ok())
                    .map(|t| t.trim().to_string())
                    .filter(|s| !s.is_empty());

                if let Some(text) = text {
                    match local.as_slice() {
                        b"title" if title.is_none() => title = Some(text),
                        b"creator" => creators.push(Creator { name: text, role }),
                        b"description" if description.is_none() => description = Some(text),
                        b"publisher" if publisher.is_none() => publisher = Some(text),
                        b"date" if date.is_none() => date = Some(text),
                        b"language" if language.is_none() => language = Some(text),
                        b"identifier" => identifiers.push(text),
                        b"subject" => subjects.push(text),
                        _ => {}
                    }
                }
            }
            // General arm — meta and DC already handled by guarded arms above
            Event::Empty(e) | Event::Start(e) => match e.name().as_ref() {
                b"item" => {
                    let attrs: HashMap<String, String> = e
                        .attributes()
                        .flatten()
                        .filter_map(|a| {
                            let k = std::str::from_utf8(a.key.as_ref()).ok()?.to_string();
                            let v = std::str::from_utf8(&a.value).ok()?.to_string();
                            Some((k, v))
                        })
                        .collect();

                    if let (Some(id), Some(href)) = (attrs.get("id"), attrs.get("href")) {
                        // C4: validate href path safety via shared helper.
                        if super::is_safe_path(href) {
                            manifest.insert(id.clone(), href.clone());
                        } else {
                            issues.push(Issue {
                                layer: Layer::Opf,
                                severity: Severity::Degraded,
                                kind: IssueKind::UnsafeManifestHref { href: href.clone() },
                            });
                        }
                    }
                }
                b"itemref" => {
                    if let Some(idref) = e
                        .attributes()
                        .flatten()
                        .find(|a| a.key.as_ref() == b"idref")
                        && let Ok(v) = std::str::from_utf8(&idref.value)
                    {
                        spine_idrefs.push(v.to_string());
                    }
                }
                _ => {}
            },
            Event::Eof => break,
            _ => {}
        }
    }

    // Validate spine refs against manifest
    let manifest_ids: HashSet<&String> = manifest.keys().collect();
    let mut valid_spine: Vec<String> = Vec::new();
    for idref in &spine_idrefs {
        if manifest_ids.contains(idref) {
            valid_spine.push(idref.clone());
        } else {
            issues.push(Issue {
                layer: Layer::Opf,
                severity: Severity::Repaired,
                kind: IssueKind::BrokenSpineRef {
                    idref: idref.clone(),
                },
            });
        }
    }

    let accessibility_metadata = if accessibility_meta.is_empty() {
        None
    } else {
        Some(serde_json::Value::Object(accessibility_meta))
    };

    // Resolve series: prefer calibre (more common), fall back to EPUB 3 collection
    let series_meta = calibre_series_name
        .map(|name| SeriesMeta {
            name,
            position: calibre_series_index,
        })
        .or_else(|| {
            epub3_collection_name.map(|name| {
                let position = epub3_collection_id
                    .as_ref()
                    .and_then(|id| group_positions.get(&format!("#{id}")).copied());
                SeriesMeta { name, position }
            })
        });

    Some(OpfData {
        manifest,
        spine_idrefs: valid_spine,
        opf_path: path.to_string(),
        accessibility_metadata,
        title,
        creators,
        description,
        publisher,
        date,
        language,
        identifiers,
        subjects,
        series_meta,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::epub::zip_layer::ZipHandle;

    fn make_handle(opf_content: &[u8]) -> ZipHandle {
        use std::io::Write;
        let buf = std::io::Cursor::new(Vec::new());
        let mut w = zip::ZipWriter::new(buf);
        let opts: zip::write::FileOptions<zip::write::ExtendedFileOptions> =
            zip::write::FileOptions::default();
        w.start_file("OEBPS/content.opf", opts).unwrap();
        w.write_all(opf_content).unwrap();
        let bytes = w.finish().unwrap().into_inner();
        ZipHandle {
            bytes,
            entries: vec!["OEBPS/content.opf".to_string()],
        }
    }

    #[test]
    fn broken_spine_ref_emits_repaired_issue() {
        let opf = br#"<package>
            <manifest>
                <item id="ch1" href="ch1.xhtml" media-type="application/xhtml+xml"/>
            </manifest>
            <spine>
                <itemref idref="ch1"/>
                <itemref idref="ch2"/>
            </spine>
        </package>"#;
        let handle = make_handle(opf);
        let mut issues = Vec::new();
        let result = validate(&handle, Some("OEBPS/content.opf"), &mut issues);
        assert!(result.is_some());
        let data = result.unwrap();
        assert_eq!(data.spine_idrefs, vec!["ch1"]);
        assert!(issues.iter().any(|i| {
            i.severity == Severity::Repaired
                && matches!(&i.kind, IssueKind::BrokenSpineRef { idref } if idref == "ch2")
        }));
    }

    #[test]
    fn epub3_accessibility_meta_parsed() {
        let opf = br#"<package>
            <metadata>
                <meta property="schema:accessMode">textual</meta>
            </metadata>
            <manifest/>
            <spine/>
        </package>"#;
        let handle = make_handle(opf);
        let mut issues = Vec::new();
        let result = validate(&handle, Some("OEBPS/content.opf"), &mut issues);
        assert!(result.is_some());
        let data = result.unwrap();
        let meta = data.accessibility_metadata.unwrap();
        assert_eq!(meta["schema:accessMode"], "textual");
    }

    #[test]
    fn dc_metadata_extracted() {
        let opf = br#"<package xmlns:dc="http://purl.org/dc/elements/1.1/">
            <metadata>
                <dc:title>The Hobbit</dc:title>
                <dc:creator opf:role="aut">J. R. R. Tolkien</dc:creator>
                <dc:description>A fantasy novel</dc:description>
                <dc:publisher>Allen &amp; Unwin</dc:publisher>
                <dc:date>1937-09-21</dc:date>
                <dc:language>en</dc:language>
                <dc:identifier>urn:isbn:9780547928227</dc:identifier>
                <dc:identifier>urn:uuid:12345</dc:identifier>
                <dc:subject>Fantasy</dc:subject>
                <dc:subject>Adventure</dc:subject>
            </metadata>
            <manifest/>
            <spine/>
        </package>"#;
        let handle = make_handle(opf);
        let mut issues = Vec::new();
        let result = validate(&handle, Some("OEBPS/content.opf"), &mut issues);
        let data = result.unwrap();
        assert_eq!(data.title.as_deref(), Some("The Hobbit"));
        assert_eq!(data.creators.len(), 1);
        assert_eq!(data.creators[0].name, "J. R. R. Tolkien");
        assert_eq!(data.creators[0].role.as_deref(), Some("aut"));
        assert_eq!(data.description.as_deref(), Some("A fantasy novel"));
        assert_eq!(data.publisher.as_deref(), Some("Allen &amp; Unwin"));
        assert_eq!(data.date.as_deref(), Some("1937-09-21"));
        assert_eq!(data.language.as_deref(), Some("en"));
        assert_eq!(data.identifiers.len(), 2);
        assert_eq!(data.subjects, vec!["Fantasy", "Adventure"]);
    }

    #[test]
    fn calibre_series_meta_extracted() {
        let opf = br#"<package>
            <metadata>
                <dc:title>The Two Towers</dc:title>
                <meta name="calibre:series" content="The Lord of the Rings"/>
                <meta name="calibre:series_index" content="2"/>
            </metadata>
            <manifest/>
            <spine/>
        </package>"#;
        let handle = make_handle(opf);
        let mut issues = Vec::new();
        let result = validate(&handle, Some("OEBPS/content.opf"), &mut issues);
        let data = result.unwrap();
        let series = data.series_meta.unwrap();
        assert_eq!(series.name, "The Lord of the Rings");
        assert_eq!(series.position, Some(2.0));
    }

    #[test]
    fn epub3_collection_series_extracted() {
        let opf = br##"<package>
            <metadata>
                <dc:title>A Game of Thrones</dc:title>
                <meta property="belongs-to-collection" id="c01">A Song of Ice and Fire</meta>
                <meta refines="#c01" property="group-position">1</meta>
            </metadata>
            <manifest/>
            <spine/>
        </package>"##;
        let handle = make_handle(opf);
        let mut issues = Vec::new();
        let result = validate(&handle, Some("OEBPS/content.opf"), &mut issues);
        let data = result.unwrap();
        let series = data.series_meta.unwrap();
        assert_eq!(series.name, "A Song of Ice and Fire");
        assert_eq!(series.position, Some(1.0));
    }

    #[test]
    fn epub3_collection_series_reversed_order() {
        // group-position appears BEFORE belongs-to-collection in document
        // order — valid per EPUB3, emitted by some toolchains. Position must
        // still be captured (UNK-234).
        let opf = br##"<package>
            <metadata>
                <dc:title>A Clash of Kings</dc:title>
                <meta refines="#c01" property="group-position">3</meta>
                <meta property="belongs-to-collection" id="c01">A Song of Ice and Fire</meta>
            </metadata>
            <manifest/>
            <spine/>
        </package>"##;
        let handle = make_handle(opf);
        let mut issues = Vec::new();
        let result = validate(&handle, Some("OEBPS/content.opf"), &mut issues);
        let data = result.unwrap();
        let series = data.series_meta.unwrap();
        assert_eq!(series.name, "A Song of Ice and Fire");
        assert_eq!(series.position, Some(3.0));
    }

    #[test]
    fn epub3_collection_series_self_closing_position() {
        // Self-closing group-position carries its value in a `content` attr
        // (no text body → quick-xml yields Event::Empty, not Event::Start).
        // Valid per EPUB3 and emitted by some toolchains (UNK-315).
        let opf = br##"<package>
            <metadata>
                <dc:title>A Game of Thrones</dc:title>
                <meta property="belongs-to-collection" id="c01">A Song of Ice and Fire</meta>
                <meta refines="#c01" property="group-position" content="3"/>
            </metadata>
            <manifest/>
            <spine/>
        </package>"##;
        let handle = make_handle(opf);
        let mut issues = Vec::new();
        let result = validate(&handle, Some("OEBPS/content.opf"), &mut issues);
        let data = result.unwrap();
        let series = data.series_meta.unwrap();
        assert_eq!(series.name, "A Song of Ice and Fire");
        assert_eq!(series.position, Some(3.0));
    }

    #[test]
    fn epub3_collection_series_self_closing_reversed_order() {
        // Self-closing group-position BEFORE belongs-to-collection: order
        // independence must hold for the Event::Empty path too (UNK-315).
        let opf = br##"<package>
            <metadata>
                <dc:title>A Clash of Kings</dc:title>
                <meta refines="#c01" property="group-position" content="2"/>
                <meta property="belongs-to-collection" id="c01">A Song of Ice and Fire</meta>
            </metadata>
            <manifest/>
            <spine/>
        </package>"##;
        let handle = make_handle(opf);
        let mut issues = Vec::new();
        let result = validate(&handle, Some("OEBPS/content.opf"), &mut issues);
        let data = result.unwrap();
        let series = data.series_meta.unwrap();
        assert_eq!(series.name, "A Song of Ice and Fire");
        assert_eq!(series.position, Some(2.0));
    }

    #[test]
    fn epub3_collection_series_self_closing_non_numeric_position() {
        // A malformed (non-numeric) content must not suppress the series name:
        // position degrades to None, series_meta itself stays Some (UNK-315).
        let opf = br##"<package>
            <metadata>
                <dc:title>A Storm of Swords</dc:title>
                <meta property="belongs-to-collection" id="c01">A Song of Ice and Fire</meta>
                <meta refines="#c01" property="group-position" content="three"/>
            </metadata>
            <manifest/>
            <spine/>
        </package>"##;
        let handle = make_handle(opf);
        let mut issues = Vec::new();
        let result = validate(&handle, Some("OEBPS/content.opf"), &mut issues);
        let data = result.unwrap();
        let series = data.series_meta.unwrap();
        assert_eq!(series.name, "A Song of Ice and Fire");
        assert!(series.position.is_none());
    }

    #[test]
    fn epub3_collection_series_self_closing_fractional_position() {
        // EPUB3 allows fractional positions (0.5 prologues, 1.5 interludes);
        // the field is f64 end-to-end and must not truncate (UNK-315).
        let opf = br##"<package>
            <metadata>
                <dc:title>The Hedge Knight</dc:title>
                <meta property="belongs-to-collection" id="c01">A Song of Ice and Fire</meta>
                <meta refines="#c01" property="group-position" content="1.5"/>
            </metadata>
            <manifest/>
            <spine/>
        </package>"##;
        let handle = make_handle(opf);
        let mut issues = Vec::new();
        let result = validate(&handle, Some("OEBPS/content.opf"), &mut issues);
        let data = result.unwrap();
        let series = data.series_meta.unwrap();
        assert_eq!(series.position, Some(1.5));
    }

    #[test]
    fn empty_metadata_returns_none_fields() {
        let opf = br"<package>
            <metadata/>
            <manifest/>
            <spine/>
        </package>";
        let handle = make_handle(opf);
        let mut issues = Vec::new();
        let result = validate(&handle, Some("OEBPS/content.opf"), &mut issues);
        let data = result.unwrap();
        assert!(data.title.is_none());
        assert!(data.creators.is_empty());
        assert!(data.description.is_none());
        assert!(data.identifiers.is_empty());
        assert!(data.series_meta.is_none());
    }

    #[test]
    fn multiple_creators_with_roles() {
        let opf = br#"<package xmlns:dc="http://purl.org/dc/elements/1.1/">
            <metadata>
                <dc:creator opf:role="aut">Author One</dc:creator>
                <dc:creator opf:role="edt">Editor Two</dc:creator>
                <dc:creator>No Role Three</dc:creator>
            </metadata>
            <manifest/>
            <spine/>
        </package>"#;
        let handle = make_handle(opf);
        let mut issues = Vec::new();
        let result = validate(&handle, Some("OEBPS/content.opf"), &mut issues);
        let data = result.unwrap();
        assert_eq!(data.creators.len(), 3);
        assert_eq!(data.creators[0].role.as_deref(), Some("aut"));
        assert_eq!(data.creators[1].role.as_deref(), Some("edt"));
        assert!(data.creators[2].role.is_none());
    }
}
