use quick_xml::Reader;

use super::{
    Issue, IssueKind, Layer, MAX_SPINE_ITEMS, Severity,
    opf_layer::OpfData,
    zip_layer::{ZipHandle, read_entry},
};

/// Validate XHTML spine documents.
pub fn validate(handle: &ZipHandle, opf_data: Option<&OpfData>, issues: &mut Vec<Issue>) {
    let Some(opf) = opf_data else { return };

    if opf.spine_idrefs.len() > MAX_SPINE_ITEMS {
        issues.push(Issue {
            layer: Layer::Xhtml,
            severity: Severity::Degraded,
            kind: IssueKind::SpineCapExceeded {
                count: opf.spine_idrefs.len(),
            },
        });
        return;
    }

    // Determine base path from OPF path (for resolving relative hrefs)
    let opf_dir = opf
        .opf_path
        .rfind('/')
        .map(|i| &opf.opf_path[..i])
        .unwrap_or("");

    for idref in &opf.spine_idrefs {
        let Some(href) = opf.manifest.get(idref) else {
            continue;
        };
        let entry_path = if opf_dir.is_empty() {
            href.clone()
        } else {
            format!("{opf_dir}/{href}")
        };

        let Some(bytes) = read_entry(handle, &entry_path) else {
            continue;
        };

        validate_xhtml_document(&bytes, &entry_path, issues);
    }
}

pub(crate) fn validate_xhtml_document(bytes: &[u8], entry_name: &str, issues: &mut Vec<Issue>) {
    // Conservative encoding repair rule:
    // Only transcode if ALL THREE conditions hold:
    // (a) XML declaration or BOM explicitly declares a non-UTF-8 encoding
    // (b) file fails UTF-8 parse
    // (c) decoding under declared encoding succeeds cleanly

    let declared_encoding = detect_declared_encoding(bytes);

    // Condition (b): try UTF-8 parse
    let utf8_ok = std::str::from_utf8(bytes).is_ok();

    if !utf8_ok {
        if let Some(enc_label) = &declared_encoding {
            // Condition (a): declared encoding present. Try condition (c).
            if let Some(encoding) = encoding_rs::Encoding::for_label(enc_label.as_bytes()) {
                let (decoded, _enc, had_errors) = encoding.decode(bytes);
                if !had_errors {
                    // All three conditions met: emit Repaired
                    issues.push(Issue {
                        layer: Layer::Xhtml,
                        severity: Severity::Repaired,
                        kind: IssueKind::EncodingMismatch {
                            entry_name: entry_name.to_string(),
                            declared: enc_label.clone(),
                            detected: "UTF-8".to_string(),
                        },
                    });
                    // Validate the decoded content as XML
                    validate_xml_parse(decoded.as_bytes(), entry_name, issues);
                    return;
                }
            }
        }
        // Conditions not fully met → Degraded, do not transcode
        issues.push(Issue {
            layer: Layer::Xhtml,
            severity: Severity::Degraded,
            kind: IssueKind::AmbiguousEncoding {
                entry_name: entry_name.to_string(),
            },
        });
        return;
    }

    validate_xml_parse(bytes, entry_name, issues);
}

/// Parse XML and report structural errors as Degraded issues.
fn validate_xml_parse(bytes: &[u8], entry_name: &str, issues: &mut Vec<Issue>) {
    let Ok(xml) = std::str::from_utf8(bytes) else {
        return;
    };
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    loop {
        match reader.read_event() {
            Ok(quick_xml::events::Event::Eof) => break,
            Err(e) => {
                issues.push(Issue {
                    layer: Layer::Xhtml,
                    severity: Severity::Degraded,
                    kind: IssueKind::MalformedXhtml {
                        entry_name: entry_name.to_string(),
                        detail: e.to_string(),
                    },
                });
                break;
            }
            _ => {}
        }
    }
}

/// Extract encoding declared in XML declaration (`<?xml ... encoding="..." ?>`) or BOM.
fn detect_declared_encoding(bytes: &[u8]) -> Option<String> {
    // BOM detection
    if bytes.starts_with(b"\xFF\xFE") {
        return Some("UTF-16LE".to_string());
    }
    if bytes.starts_with(b"\xFE\xFF") {
        return Some("UTF-16BE".to_string());
    }

    // XML declaration: look for encoding="..." in first 200 bytes.
    // Use from_utf8_lossy so non-UTF-8 bytes after the declaration don't cause early return.
    // XML declarations are pure ASCII and appear before any content bytes.
    let prefix_cow = String::from_utf8_lossy(&bytes[..bytes.len().min(200)]);
    let prefix = prefix_cow.as_ref();
    let decl_start = prefix.find("<?xml")?;
    let decl_end = prefix[decl_start..].find("?>")?;
    let decl = &prefix[decl_start..decl_start + decl_end + 2];

    let enc_start = decl
        .find("encoding=\"")
        .or_else(|| decl.find("encoding='"))?;
    let after = &decl[enc_start + 10..];
    let quote_char = decl.chars().nth(enc_start + 9)?;
    let enc_end = after.find(quote_char)?;
    Some(after[..enc_end].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::epub::{IssueKind, Severity};

    #[test]
    fn latin1_declared_non_utf8_bytes_emits_repaired() {
        // Craft bytes with XML declaration claiming ISO-8859-1 that are not valid UTF-8
        let mut bytes: Vec<u8> = b"<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?><html/>".to_vec();
        bytes.push(0xE9); // é in Latin-1, invalid as UTF-8 continuation
        let mut issues = Vec::new();
        validate_xhtml_document(&bytes, "test.xhtml", &mut issues);
        assert!(
            issues
                .iter()
                .any(|i| matches!(&i.kind, IssueKind::EncodingMismatch { .. })),
            "expected EncodingMismatch issue, got: {issues:?}"
        );
    }

    #[test]
    fn non_utf8_no_declaration_emits_degraded() {
        let bytes: Vec<u8> = vec![0xE9, 0xE0, 0xF3]; // not valid UTF-8, no XML decl
        let mut issues = Vec::new();
        validate_xhtml_document(&bytes, "test.xhtml", &mut issues);
        assert!(issues.iter().any(|i| i.severity == Severity::Degraded));
    }
}
