use std::collections::HashMap;
use std::io::Read;
use std::path::Path;
use zip::ZipArchive;
use zip::write::{ExtendedFileOptions, FileOptions};

use super::repack;
use super::{EpubError, Issue, IssueKind};

/// Re-package the EPUB at `path` applying all `Repaired`-severity issues.
///
/// Writes to a temp file in the same directory, then `rename()`s over `path`
/// atomically. If re-packaging fails, `path` is left untouched.
pub fn repackage(path: &Path, issues: &[Issue], opf_path: Option<&str>) -> Result<(), EpubError> {
    let dir = path.parent().unwrap_or(Path::new("."));

    let broken_refs: Vec<String> = issues
        .iter()
        .filter_map(|i| {
            if let IssueKind::BrokenSpineRef { idref } = &i.kind {
                Some(idref.clone())
            } else {
                None
            }
        })
        .collect();

    let encoding_fixes: Vec<(String, String)> = issues
        .iter()
        .filter_map(|i| {
            if let IssueKind::EncodingMismatch {
                entry_name,
                declared,
                ..
            } = &i.kind
            {
                Some((entry_name.clone(), declared.clone()))
            } else {
                None
            }
        })
        .collect();

    let missing_container = issues
        .iter()
        .any(|i| matches!(&i.kind, IssueKind::MissingContainer { .. }));

    let opf_candidate: Option<String> = issues.iter().find_map(|i| {
        if let IssueKind::MissingContainer { opf_candidate } = &i.kind {
            opf_candidate.clone()
        } else {
            None
        }
    });

    // Build a `binary_replacements` map for encoding fixes applied to
    // non-OPF entries.  OPF encoding fixes are folded into the rewritten_opf
    // below, so they are NOT added to this map.
    let mut binary_replacements: HashMap<String, Vec<u8>> = HashMap::new();
    let bytes = std::fs::read(path)?;
    for (entry_name, declared_enc) in &encoding_fixes {
        if Some(entry_name.as_str()) == opf_path {
            continue;
        }
        let cursor = std::io::Cursor::new(&bytes[..]);
        let mut ar = ZipArchive::new(cursor)?;
        let entry_bytes: Option<Vec<u8>> = match ar.by_name(entry_name) {
            Ok(entry) => {
                let mut buf = Vec::new();
                entry
                    .take(super::MAX_ENTRY_UNCOMPRESSED_BYTES + 1)
                    .read_to_end(&mut buf)?;
                Some(buf)
            }
            Err(_) => None,
        };
        if let Some(raw) = entry_bytes
            && let Some(transcoded) = transcode_to_utf8(&raw, declared_enc)
        {
            binary_replacements.insert(entry_name.clone(), transcoded);
        }
    }

    // OPF replacement: chain encoding fix + spine rewrite when both apply.
    let rewritten_opf: Option<Vec<u8>> = if !broken_refs.is_empty() {
        if let Some(opf) = opf_path {
            let cursor = std::io::Cursor::new(&bytes[..]);
            let mut ar = ZipArchive::new(cursor)?;
            let mut opf_bytes = Vec::new();
            ar.by_name(opf)?
                .take(super::MAX_ENTRY_UNCOMPRESSED_BYTES + 1)
                .read_to_end(&mut opf_bytes)?;
            let opf_bytes = if let Some((_, enc)) = encoding_fixes.iter().find(|(n, _)| n == opf) {
                transcode_to_utf8(&opf_bytes, enc).unwrap_or(opf_bytes)
            } else {
                opf_bytes
            };
            Some(rewrite_opf_remove_broken_spine(&opf_bytes, &broken_refs))
        } else {
            None
        }
    } else if let Some(opf) = opf_path {
        // No spine rewrite but OPF may still need an encoding fix.
        encoding_fixes
            .iter()
            .find(|(n, _)| n == opf)
            .and_then(|(_, enc)| {
                let cursor = std::io::Cursor::new(&bytes[..]);
                let mut ar = ZipArchive::new(cursor).ok()?;
                let mut opf_bytes = Vec::new();
                ar.by_name(opf)
                    .ok()?
                    .take(super::MAX_ENTRY_UNCOMPRESSED_BYTES + 1)
                    .read_to_end(&mut opf_bytes)
                    .ok()?;
                transcode_to_utf8(&opf_bytes, enc)
            })
    } else {
        None
    };

    // Regenerated container.xml is appended as an addition when missing.
    let mut additions: Vec<(String, Vec<u8>, FileOptions<ExtendedFileOptions>)> = Vec::new();
    if missing_container && let Some(opf_path_str) = &opf_candidate {
        let container_xml = generate_container_xml(opf_path_str);
        let opts: FileOptions<ExtendedFileOptions> = FileOptions::default();
        additions.push((
            "META-INF/container.xml".to_string(),
            container_xml.into_bytes(),
            opts,
        ));
    }

    // Release the bytes borrow before the helper re-reads the source.
    drop(bytes);

    let temp = repack::with_modifications(
        path,
        dir,
        opf_path,
        rewritten_opf.as_deref(),
        &binary_replacements,
        &additions,
    )?;
    temp.persist(path).map_err(EpubError::TempFile)?;
    Ok(())
}

/// Rewrite OPF XML removing `<itemref>` elements whose `idref` is in `broken_refs`.
fn rewrite_opf_remove_broken_spine(opf_bytes: &[u8], broken_refs: &[String]) -> Vec<u8> {
    let xml = match std::str::from_utf8(opf_bytes) {
        Ok(s) => s,
        Err(_) => return opf_bytes.to_vec(),
    };
    let mut reader = quick_xml::Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut output = quick_xml::Writer::new(Vec::new());
    // C6: use a depth counter instead of a bool so that malformed OPF with nested
    // <itemref> elements (impossible in valid EPUB but possible in corrupted input)
    // does not reset the skip flag prematurely on the first end tag.
    let mut skip_depth: u32 = 0;
    loop {
        match reader.read_event() {
            Ok(quick_xml::events::Event::Empty(e)) if e.name().as_ref() == b"itemref" => {
                if skip_depth > 0 {
                    continue; // inside a skipped subtree
                }
                let idref = e
                    .attributes()
                    .flatten()
                    .find(|a| a.key.as_ref() == b"idref")
                    .and_then(|a| std::str::from_utf8(&a.value).ok().map(|s| s.to_string()));
                if idref
                    .as_deref()
                    .is_some_and(|id| broken_refs.iter().any(|r| r == id))
                {
                    continue;
                }
                let _ = output.write_event(quick_xml::events::Event::Empty(e.into_owned()));
            }
            Ok(quick_xml::events::Event::Start(e)) if e.name().as_ref() == b"itemref" => {
                let idref = e
                    .attributes()
                    .flatten()
                    .find(|a| a.key.as_ref() == b"idref")
                    .and_then(|a| std::str::from_utf8(&a.value).ok().map(|s| s.to_string()));
                if skip_depth > 0
                    || idref
                        .as_deref()
                        .is_some_and(|id| broken_refs.iter().any(|r| r == id))
                {
                    skip_depth += 1;
                } else {
                    let _ = output.write_event(quick_xml::events::Event::Start(e.into_owned()));
                }
            }
            Ok(quick_xml::events::Event::End(e)) if e.name().as_ref() == b"itemref" => {
                if skip_depth > 0 {
                    skip_depth -= 1;
                } else {
                    let _ = output.write_event(quick_xml::events::Event::End(e.into_owned()));
                }
            }
            Ok(quick_xml::events::Event::Eof) => break,
            Ok(e) => {
                if skip_depth == 0 {
                    let _ = output.write_event(e.into_owned());
                }
            }
            Err(_) => return opf_bytes.to_vec(),
        }
    }
    output.into_inner()
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

/// Escape XML special characters in `s` for use in an attribute value.
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
    use crate::services::epub::repack::MIMETYPE_ENTRY;
    use crate::services::epub::{IssueKind, Layer, Severity};
    use std::io::Write;
    use zip::ZipWriter;

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
        let result = rewrite_opf_remove_broken_spine(opf, &["ch2".to_string()]);
        let result_str = std::str::from_utf8(&result).unwrap();
        assert!(result_str.contains("ch1"));
        assert!(!result_str.contains("ch2"));
    }
}
