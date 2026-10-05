//! Entry repair instructions for candidate repack.
//!
//! Encoding conversion is produced one entry at a time. Required spine and container
//! repairs compose with metadata changes before candidate validation.

use super::{EpubError, Issue, IssueKind, Severity, ValidationReport, zip_layer};

/// Instructions for one candidate, without retaining converted chapters.
#[derive(Default)]
pub struct RepairPlan {
    broken_refs: Vec<String>,
    encodings: Vec<(String, String)>,
    container_opf: Option<String>,
    opf_path: Option<String>,
}

impl RepairPlan {
    /// Capture existing repair instructions from a source report.
    #[must_use]
    pub fn from_report(report: &ValidationReport) -> Self {
        Self::from_issues(
            &report.issues,
            report.opf_data.as_ref().map(|opf| opf.opf_path.as_str()),
        )
    }

    fn from_issues(issues: &[Issue], opf_path: Option<&str>) -> Self {
        let mut plan = Self {
            opf_path: opf_path.map(str::to_owned),
            ..Self::default()
        };
        for issue in issues
            .iter()
            .filter(|issue| issue.severity == Severity::Repaired)
        {
            match &issue.kind {
                IssueKind::BrokenSpineRef { idref } => plan.broken_refs.push(idref.clone()),
                IssueKind::EncodingMismatch {
                    entry_name,
                    declared,
                    ..
                } => plan.encodings.push((entry_name.clone(), declared.clone())),
                IssueKind::MissingContainer { opf_candidate } => {
                    plan.container_opf.clone_from(opf_candidate);
                }
                _ => {}
            }
        }
        plan
    }

    /// Produce the repair for one entry and release its conversion after writing.
    ///
    /// # Errors
    /// Returns an error when a required source entry or conversion is unavailable.
    pub fn replacement(
        &self,
        handle: &zip_layer::ZipHandle,
        name: &str,
    ) -> Result<Option<Vec<u8>>, EpubError> {
        if name == "META-INF/container.xml"
            && let Some(opf) = &self.container_opf
        {
            return Ok(Some(generate_container_xml(opf).into_bytes()));
        }
        let encoding = self.encodings.iter().find(|(entry, _)| entry == name);
        let spine = self.opf_path.as_deref() == Some(name) && !self.broken_refs.is_empty();
        if encoding.is_none() && !spine {
            return Ok(None);
        }
        let raw = zip_layer::read_entry(handle, name)
            .ok_or_else(|| EpubError::Repair(format!("unreadable entry {name}")))?;
        let bytes = if let Some((_, encoding)) = encoding {
            transcode_to_utf8(&raw, encoding).ok_or_else(|| {
                EpubError::Repair(format!("encoding conversion failed for {name}"))
            })?
        } else {
            raw
        };
        if spine {
            Ok(Some(rewrite_opf_remove_broken_spine(
                &bytes,
                &self.broken_refs,
            )?))
        } else {
            Ok(Some(bytes))
        }
    }

    /// Missing container instructions, emitted only when no source entry exists.
    pub(super) fn container_addition(&self, handle: &zip_layer::ZipHandle) -> Option<Vec<u8>> {
        (!handle
            .entries
            .iter()
            .any(|name| name == "META-INF/container.xml"))
        .then(|| {
            self.container_opf
                .as_ref()
                .map(|opf| generate_container_xml(opf).into_bytes())
        })
        .flatten()
    }
}

/// Rewrite `OPF` `XML` removing `<itemref>` elements whose `idref` is in `broken_refs`.
fn rewrite_opf_remove_broken_spine(
    opf_bytes: &[u8],
    broken_refs: &[String],
) -> Result<Vec<u8>, EpubError> {
    let xml =
        std::str::from_utf8(opf_bytes).map_err(|error| EpubError::Repair(error.to_string()))?;
    let mut reader = quick_xml::Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut output = quick_xml::Writer::new(Vec::new());
    let mut skip_depth = 0_u32;
    loop {
        let event = reader
            .read_event()
            .map_err(|error| EpubError::Repair(error.to_string()))?;
        match event {
            quick_xml::events::Event::Empty(ref e) | quick_xml::events::Event::Start(ref e)
                if e.name().as_ref() == "itemref" =>
            {
                let idref = e
                    .attributes()
                    .flatten()
                    .find(|a| a.key.as_ref() == "idref")
                    .map(|a| a.value.into_owned());
                if skip_depth > 0
                    || idref
                        .as_deref()
                        .is_some_and(|id| broken_refs.iter().any(|r| r == id))
                {
                    if matches!(event, quick_xml::events::Event::Start(_)) {
                        skip_depth += 1;
                    }
                    continue;
                }
            }
            quick_xml::events::Event::End(ref e)
                if e.name().as_ref() == "itemref" && skip_depth > 0 =>
            {
                skip_depth -= 1;
                continue;
            }
            quick_xml::events::Event::Eof => break,
            _ => {}
        }
        if skip_depth == 0 {
            output
                .write_event(event.into_owned())
                .map_err(|error| EpubError::Repair(error.to_string()))?;
        }
    }
    Ok(output.into_inner())
}

fn transcode_to_utf8(bytes: &[u8], declared_enc: &str) -> Option<Vec<u8>> {
    let encoding = encoding_rs::Encoding::for_label(declared_enc.as_bytes())?;
    let (decoded, _, had_errors) = encoding.decode(bytes);
    if had_errors {
        return None;
    }

    // C5: Replace encoding declaration in both double-quoted and single-quoted forms.
    // Plain str::replace is case-sensitive and matches the exact declared string,
    // which round-trips correctly from detect_declared_encoding.
    let utf8_str = decoded
        .replace(
            &format!("encoding=\"{declared_enc}\""),
            "encoding=\"UTF-8\"",
        )
        .replace(&format!("encoding='{declared_enc}'"), "encoding='UTF-8'");
    Some(utf8_str.into_bytes())
}

/// Escape `XML` special characters in `s` for use in an attribute value.
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn generate_container_xml(opf_path: &str) -> String {
    // C2: escape the OPF path before interpolating into XML to prevent injection
    // via ZIP entry names that contain XML-significant characters (", <, >, &, ').
    let escaped = xml_escape(opf_path);
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles>
    <rootfile full-path="{escaped}" media-type="application/oebps-package+xml"/>
  </rootfiles>
</container>"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use zip::write::{ExtendedFileOptions, FileOptions};
    fn repackage(path: &Path, issues: &[Issue], opf_path: Option<&str>) -> Result<(), EpubError> {
        let (handle, report) = super::super::inspect(std::fs::File::open(path)?)?;
        let plan = RepairPlan::from_issues(issues, opf_path);
        let parent = cap_std::fs::Dir::open_ambient_dir(
            path.parent().unwrap(),
            cap_std::ambient_authority(),
        )?;
        super::super::repack::publish(&parent, path.file_name().unwrap(), &report, |file| {
            super::super::repack::with_modifications(
                &handle,
                file,
                None,
                None,
                &std::collections::HashMap::new(),
                &[],
                &plan,
            )
        })?;
        Ok(())
    }
    use crate::services::epub::repack::MIMETYPE_ENTRY;
    use crate::services::epub::{IssueKind, Layer, Severity};
    use std::io::Write;
    use zip::{ZipArchive, ZipWriter};

    fn make_epub(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let buf = std::io::Cursor::new(Vec::new());
        let mut w = ZipWriter::new(buf);
        for (name, data) in entries {
            let opts: FileOptions<ExtendedFileOptions> = FileOptions::default();
            w.start_file(*name, opts).unwrap();
            w.write_all(data).unwrap();
        }
        w.finish().unwrap().into_inner()
    }

    #[test]
    fn repackage_adds_container_xml_when_missing() {
        let opf_content = b"<package><manifest/><spine/></package>";
        let epub_bytes = make_epub(&[("OEBPS/content.opf", opf_content)]);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.epub");
        std::fs::write(&path, &epub_bytes).unwrap();

        let issues = vec![Issue {
            layer: Layer::Container,
            severity: Severity::Repaired,
            kind: IssueKind::MissingContainer {
                opf_candidate: Some("OEBPS/content.opf".to_string()),
            },
        }];

        repackage(&path, &issues, Some("OEBPS/content.opf")).unwrap();

        // Verify container.xml is in the repacked archive
        let repacked = std::fs::read(&path).unwrap();
        let cursor = std::io::Cursor::new(repacked);
        let mut archive = ZipArchive::new(cursor).unwrap();
        assert!(archive.by_name("META-INF/container.xml").is_ok());
    }

    #[test]
    fn repackage_mimetype_is_first_and_stored() {
        let epub_bytes = make_epub(&[("OEBPS/content.opf", b"<package/>")]);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.epub");
        std::fs::write(&path, &epub_bytes).unwrap();

        let issues = vec![Issue {
            layer: Layer::Container,
            severity: Severity::Repaired,
            kind: IssueKind::MissingContainer {
                opf_candidate: Some("OEBPS/content.opf".to_string()),
            },
        }];

        repackage(&path, &issues, Some("OEBPS/content.opf")).unwrap();

        let repacked = std::fs::read(&path).unwrap();
        let cursor = std::io::Cursor::new(repacked);
        let mut archive = ZipArchive::new(cursor).unwrap();
        let first = archive.by_index(0).unwrap();
        assert_eq!(first.name(), MIMETYPE_ENTRY);
        assert_eq!(first.compression(), zip::CompressionMethod::Stored);
    }

    #[test]
    fn rewrite_opf_removes_broken_spine_ref() {
        let opf = br#"<package>
<spine>
<itemref idref="ch1"/>
<itemref idref="ch2"/>
</spine>
</package>"#;
        let result = rewrite_opf_remove_broken_spine(opf, &["ch2".to_string()]).unwrap();
        let result_str = std::str::from_utf8(&result).unwrap();
        assert!(result_str.contains("ch1"));
        assert!(!result_str.contains("ch2"));
    }
}
