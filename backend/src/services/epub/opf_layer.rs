use quick_xml::Reader;
use quick_xml::events::Event;
use std::collections::{HashMap, HashSet};

use super::{
    Issue, IssueKind, Layer, Severity,
    zip_layer::{ZipHandle, read_entry},
};

pub struct OpfData {
    /// All manifest items: id → href
    pub manifest: HashMap<String, String>,
    /// Spine idrefs (after removing broken refs)
    pub spine_idrefs: Vec<String>,
    /// OPF path within the archive (needed by repair and other layers)
    pub opf_path: String,
    /// Raw W3C accessibility metadata from `<meta>` elements, if any
    pub accessibility_metadata: Option<serde_json::Value>,
}

/// Validate the OPF file. Returns `None` if OPF cannot be read.
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

    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    loop {
        match reader.read_event().ok()? {
            // EPUB 3 text-content meta: <meta property="schema:accessMode">textual</meta>
            // Must come BEFORE general Event::Start arm to avoid shadowing.
            Event::Start(e) if e.name().as_ref() == b"meta" => {
                let e = e.into_owned(); // release reader buffer borrow before read_text
                let prop = e
                    .attributes()
                    .flatten()
                    .find(|a| a.key.as_ref() == b"property")
                    .and_then(|a| std::str::from_utf8(&a.value).ok().map(|s| s.to_string()));
                let content_attr = e
                    .attributes()
                    .flatten()
                    .find(|a| a.key.as_ref() == b"content")
                    .and_then(|a| std::str::from_utf8(&a.value).ok().map(|s| s.to_string()));
                if let Some(prop) = prop
                    && (prop.starts_with("schema:access") || prop.starts_with("dcterms:"))
                {
                    let val = content_attr.or_else(|| {
                        reader
                            .read_text(e.name())
                            .ok()
                            .map(|t| t.trim().to_string())
                            .filter(|s| !s.is_empty())
                    });
                    if let Some(v) = val {
                        accessibility_meta.insert(prop, serde_json::Value::String(v));
                    }
                }
            }
            // EPUB 2 attribute-style meta: <meta name="..." content="..."/>
            Event::Empty(e) if e.name().as_ref() == b"meta" => {
                let prop = e
                    .attributes()
                    .flatten()
                    .find(|a| a.key.as_ref() == b"property")
                    .and_then(|a| std::str::from_utf8(&a.value).ok().map(|s| s.to_string()));
                let content = e
                    .attributes()
                    .flatten()
                    .find(|a| a.key.as_ref() == b"content")
                    .and_then(|a| std::str::from_utf8(&a.value).ok().map(|s| s.to_string()));
                if let Some(prop) = prop
                    && (prop.starts_with("schema:access") || prop.starts_with("dcterms:"))
                    && let Some(v) = content
                {
                    accessibility_meta.insert(prop, serde_json::Value::String(v));
                }
            }
            // General arm — meta already handled by guarded arms above
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
                        if !super::is_safe_path(href) {
                            issues.push(Issue {
                                layer: Layer::Opf,
                                severity: Severity::Degraded,
                                kind: IssueKind::UnsafeManifestHref { href: href.clone() },
                            });
                        } else {
                            manifest.insert(id.clone(), href.clone());
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

    Some(OpfData {
        manifest,
        spine_idrefs: valid_spine,
        opf_path: path.to_string(),
        accessibility_metadata,
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
}
