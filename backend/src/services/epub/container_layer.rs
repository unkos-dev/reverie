use quick_xml::Reader;
use quick_xml::events::Event;

use super::{
    Issue, IssueKind, Layer, Severity,
    zip_layer::{ZipHandle, read_entry},
};

const CONTAINER_PATH: &str = "META-INF/container.xml";

/// Parse container.xml and return the OPF path.
///
/// If container.xml is missing, scans for a `.opf` file and regenerates.
/// Appends issues to `issues`. Returns `None` only if no OPF can be found at all.
pub fn validate(handle: &ZipHandle, issues: &mut Vec<Issue>) -> Option<String> {
    if let Some(bytes) = read_entry(handle, CONTAINER_PATH) {
        extract_opf_path(&bytes, issues)
    } else {
        // Attempt regeneration: scan for .opf file
        let candidate = handle.entries.iter().find(|e| e.ends_with(".opf")).cloned();

        issues.push(Issue {
            layer: Layer::Container,
            severity: Severity::Repaired,
            kind: IssueKind::MissingContainer {
                opf_candidate: candidate.clone(),
            },
        });

        candidate.as_ref().and_then(|c| {
            // C4: validate path safety using the shared helper (covers percent-encoded
            // traversal and backslashes in addition to plain `..` and leading `/`).
            if !super::is_safe_path(c) {
                issues.push(Issue {
                    layer: Layer::Container,
                    severity: Severity::Irrecoverable,
                    kind: IssueKind::UnsafeOpfPath { path: c.clone() },
                });
                None
            } else {
                Some(c.clone())
            }
        })
    }
}

/// Extract the OPF `full-path` attribute from container.xml bytes.
fn extract_opf_path(bytes: &[u8], issues: &mut Vec<Issue>) -> Option<String> {
    let xml = std::str::from_utf8(bytes).ok()?;
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    loop {
        match reader.read_event().ok()? {
            Event::Empty(e) | Event::Start(e) if e.name().as_ref() == b"rootfile" => {
                if let Some(path) = e
                    .attributes()
                    .flatten()
                    .find(|a| a.key.as_ref() == b"full-path")
                {
                    let raw = std::str::from_utf8(&path.value).ok()?.to_string();
                    // C4: path safety check via shared helper.
                    if !super::is_safe_path(&raw) {
                        issues.push(Issue {
                            layer: Layer::Container,
                            severity: Severity::Irrecoverable,
                            kind: IssueKind::UnsafeOpfPath { path: raw },
                        });
                        return None;
                    }
                    return Some(raw);
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::epub::zip_layer::ZipHandle;

    fn make_handle_with_entries(entries: &[(&str, Vec<u8>)]) -> ZipHandle {
        ZipHandle {
            bytes: {
                use std::io::Write;
                let buf = std::io::Cursor::new(Vec::new());
                let mut w = zip::ZipWriter::new(buf);
                for (name, data) in entries {
                    let opts: zip::write::FileOptions<zip::write::ExtendedFileOptions> =
                        zip::write::FileOptions::default();
                    w.start_file(*name, opts).unwrap();
                    w.write_all(data).unwrap();
                }
                w.finish().unwrap().into_inner()
            },
            entries: entries.iter().map(|(n, _)| n.to_string()).collect(),
        }
    }

    #[test]
    fn missing_container_with_opf_emits_repaired() {
        let handle = make_handle_with_entries(&[("OEBPS/content.opf", b"<package/>".to_vec())]);
        let mut issues = Vec::new();
        let result = validate(&handle, &mut issues);
        assert!(result.is_some());
        assert_eq!(result.unwrap(), "OEBPS/content.opf");
        assert!(issues.iter().any(|i| {
            i.severity == Severity::Repaired
                && matches!(&i.kind, IssueKind::MissingContainer { .. })
        }));
    }

    #[test]
    fn unsafe_opf_path_in_container_xml_emits_irrecoverable() {
        // P4: container.xml whose full-path contains a traversal sequence must
        // push UnsafeOpfPath(Irrecoverable) and return None.
        let container_xml = br#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles>
    <rootfile full-path="../evil.opf" media-type="application/oebps-package+xml"/>
  </rootfiles>
</container>"#;
        let handle =
            make_handle_with_entries(&[("META-INF/container.xml", container_xml.to_vec())]);
        let mut issues = Vec::new();
        let result = validate(&handle, &mut issues);
        assert!(result.is_none(), "expected None for unsafe OPF path");
        assert!(issues.iter().any(|i| {
            i.severity == Severity::Irrecoverable
                && matches!(&i.kind, IssueKind::UnsafeOpfPath { .. })
        }));
    }

    #[test]
    fn valid_container_returns_opf_path() {
        let container_xml = br#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles>
    <rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/>
  </rootfiles>
</container>"#;
        let handle =
            make_handle_with_entries(&[("META-INF/container.xml", container_xml.to_vec())]);
        let mut issues = Vec::new();
        let result = validate(&handle, &mut issues);
        assert_eq!(result.as_deref(), Some("OEBPS/content.opf"));
        assert!(issues.is_empty());
    }
}
