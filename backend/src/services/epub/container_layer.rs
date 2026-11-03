//! `META-INF/container.xml` parsing layer (Layer 2).
//!
//! Locates the `OPF` package-document path for the rest of the pipeline.
//! If `container.xml` is absent, the layer scans for a `*.opf` file and
//! regenerates the path rather than failing immediately. Path-safety checks
//! (`..` traversal, percent-encoded variants, Windows separators) run on
//! every candidate path before it is returned.

use quick_xml::Reader;
use quick_xml::events::Event;

use super::{
    Issue, IssueKind, Layer, Severity,
    zip_layer::{ZipHandle, read_entry},
};

pub(super) const CONTAINER_PATH: &str = "META-INF/container.xml";

/// Parse `container.xml` and return the `OPF` path.
///
/// If `container.xml` is missing, scans for a `.opf` file and regenerates.
/// Appends issues to `issues`. Returns `None` if no `OPF` can be found (an
/// `Irrecoverable` `MissingContainer` or `CorruptEntry` issue is recorded),
/// or if the discovered candidate fails the path-safety check (an
/// `UnsafeOpfPath` `Irrecoverable` issue is recorded in that case).
pub fn validate(handle: &ZipHandle, issues: &mut Vec<Issue>) -> Option<String> {
    if let Some(bytes) = read_entry(handle, CONTAINER_PATH) {
        let recorded = issues.len();
        let path = extract_opf_path(&bytes, issues);
        if path.is_none() && issues.len() == recorded {
            issues.push(Issue {
                layer: Layer::Container,
                severity: Severity::Irrecoverable,
                kind: IssueKind::CorruptEntry {
                    entry_name: CONTAINER_PATH.to_string(),
                },
            });
        }
        path
    } else {
        // Attempt regeneration: scan for .opf file
        let candidate = handle
            .entries
            .iter()
            .find(|e| e.to_ascii_lowercase().ends_with(".opf"))
            .cloned();

        issues.push(Issue {
            layer: Layer::Container,
            severity: if candidate.is_some() {
                Severity::Repaired
            } else {
                Severity::Irrecoverable
            },
            kind: IssueKind::MissingContainer {
                opf_candidate: candidate.clone(),
            },
        });

        candidate.as_ref().and_then(|c| {
            // C4: validate path safety using the shared helper (covers percent-encoded
            // traversal and backslashes in addition to plain `..` and leading `/`).
            if super::is_safe_path(c) {
                Some(c.clone())
            } else {
                issues.push(Issue {
                    layer: Layer::Container,
                    severity: Severity::Irrecoverable,
                    kind: IssueKind::UnsafeOpfPath { path: c.clone() },
                });
                None
            }
        })
    }
}

/// Extract the `OPF` `full-path` attribute from `container.xml` bytes.
fn extract_opf_path(bytes: &[u8], issues: &mut Vec<Issue>) -> Option<String> {
    let xml = std::str::from_utf8(bytes).ok()?;
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut depth: usize = 0;
    let mut has_root = false;
    let mut selected_path = None;

    loop {
        let event = reader.read_event().ok()?;
        if depth == 0 && matches!(&event, Event::Start(_) | Event::Empty(_)) {
            if has_root {
                return None;
            }
            has_root = true;
        }
        match &event {
            Event::Start(_) => depth += 1,
            Event::End(_) => depth = depth.checked_sub(1)?,
            Event::Eof => return if depth == 0 { selected_path } else { None },
            Event::Text(_) | Event::CData(_) | Event::GeneralRef(_) if depth == 0 => return None,
            _ => {}
        }
        match event {
            Event::Empty(e) | Event::Start(e)
                if selected_path.is_none() && e.name().as_ref() == "rootfile" =>
            {
                if let Some(path) = e
                    .attributes()
                    .flatten()
                    .find(|a| a.key.as_ref() == "full-path")
                {
                    let raw = path.value.into_owned();
                    // C4: path safety check via shared helper.
                    if !super::is_safe_path(&raw) {
                        issues.push(Issue {
                            layer: Layer::Container,
                            severity: Severity::Irrecoverable,
                            kind: IssueKind::UnsafeOpfPath { path: raw },
                        });
                        return None;
                    }
                    selected_path = Some(raw);
                }
            }
            _ => {}
        }
    }
}
