use std::io::Read;
use std::path::Path;
use zip::ZipArchive;

use super::{
    Issue, IssueKind, Layer, MAX_AGGREGATE_UNCOMPRESSED_BYTES, MAX_ENTRY_UNCOMPRESSED_BYTES,
    Severity,
};

/// Lightweight handle returned by zip_layer so upper layers can re-open the archive.
pub struct ZipHandle {
    /// Raw bytes of the entire archive (read once; ZIP seeks into this).
    pub bytes: Vec<u8>,
    /// Names of all successfully readable entries.
    pub entries: Vec<String>,
}

/// Validate ZIP integrity, path safety, and size bounds.
///
/// Returns a [`ZipHandle`] on success. Appends [`Issue`]s to `issues`.
/// If any `Irrecoverable` issue is added, the caller short-circuits.
pub fn validate(path: &Path, issues: &mut Vec<Issue>) -> Result<ZipHandle, super::EpubError> {
    let bytes = std::fs::read(path)?;
    let mut entries = Vec::new();

    'zip: {
        let cursor = std::io::Cursor::new(&bytes[..]);
        let mut archive = match ZipArchive::new(cursor) {
            Ok(a) => a,
            Err(_) => {
                issues.push(Issue {
                    layer: Layer::Zip,
                    severity: Severity::Irrecoverable,
                    kind: IssueKind::CorruptEntry {
                        entry_name: "<archive>".to_string(),
                    },
                });
                break 'zip;
            }
        };

        let mut aggregate_size: u64 = 0;

        for i in 0..archive.len() {
            // D1: use match instead of `?` so corrupt entries push an Irrecoverable
            // issue and break out of the labeled block rather than propagating Err
            // up to the caller (which would misclassify as "degraded").
            let file = match archive.by_index(i) {
                Ok(f) => f,
                Err(_) => {
                    issues.push(Issue {
                        layer: Layer::Zip,
                        severity: Severity::Irrecoverable,
                        kind: IssueKind::CorruptEntry {
                            entry_name: format!("entry[{i}]"),
                        },
                    });
                    break 'zip;
                }
            };
            let name = file.name().to_string();

            // C4: path traversal check — covers plain `..`, percent-encoded variants
            // (%2e%2e in any case), Windows backslashes, and absolute paths.
            if !super::is_safe_path(&name) {
                issues.push(Issue {
                    layer: Layer::Zip,
                    severity: Severity::Irrecoverable,
                    kind: IssueKind::PathTraversal { entry_name: name },
                });
                break 'zip;
            }

            // Per-entry size check (use size() — uncompressed — before extracting)
            let uncompressed = file.size();
            if uncompressed > MAX_ENTRY_UNCOMPRESSED_BYTES {
                issues.push(Issue {
                    layer: Layer::Zip,
                    severity: Severity::Irrecoverable,
                    kind: IssueKind::ZipBomb {
                        entry_name: name,
                        size: uncompressed,
                        limit: MAX_ENTRY_UNCOMPRESSED_BYTES,
                    },
                });
                break 'zip;
            }

            aggregate_size = aggregate_size.saturating_add(uncompressed);
            if aggregate_size > MAX_AGGREGATE_UNCOMPRESSED_BYTES {
                issues.push(Issue {
                    layer: Layer::Zip,
                    severity: Severity::Irrecoverable,
                    kind: IssueKind::ZipBomb {
                        entry_name: name,
                        size: aggregate_size,
                        limit: MAX_AGGREGATE_UNCOMPRESSED_BYTES,
                    },
                });
                break 'zip;
            }

            // C3: Extractability check — cap the probe to min(declared+1, 4096) to
            // avoid allocating the full declared size (up to 500 MB) per entry.
            // Preserves lying-central-directory detection for small declared sizes.
            let probe_cap = uncompressed.saturating_add(1).min(4_096);
            let mut buf = Vec::new();
            if file.take(probe_cap).read_to_end(&mut buf).is_err() {
                issues.push(Issue {
                    layer: Layer::Zip,
                    severity: Severity::Irrecoverable,
                    kind: IssueKind::CorruptEntry { entry_name: name },
                });
                break 'zip;
            }

            // Detect lying central directory for small entries: if the probe cap
            // equals declared+1 and buf filled to cap, actual size > declared.
            if probe_cap == uncompressed.saturating_add(1) && buf.len() as u64 == probe_cap {
                issues.push(Issue {
                    layer: Layer::Zip,
                    severity: Severity::Irrecoverable,
                    kind: IssueKind::ZipBomb {
                        entry_name: name,
                        size: buf.len() as u64,
                        limit: MAX_ENTRY_UNCOMPRESSED_BYTES,
                    },
                });
                break 'zip;
            }

            entries.push(name);
        }
    } // archive dropped here; borrow on `bytes` released

    Ok(ZipHandle { bytes, entries })
}

/// Read a specific entry from the archive bytes. Returns None if not found.
pub fn read_entry(handle: &ZipHandle, entry_name: &str) -> Option<Vec<u8>> {
    let cursor = std::io::Cursor::new(&handle.bytes[..]);
    let mut archive = ZipArchive::new(cursor).ok()?;
    let file = archive.by_name(entry_name).ok()?;
    let mut buf = Vec::new();
    file.take(MAX_ENTRY_UNCOMPRESSED_BYTES + 1)
        .read_to_end(&mut buf)
        .ok()?;
    Some(buf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::epub::{IssueKind, Severity};
    use std::io::Write;

    fn make_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let buf = std::io::Cursor::new(Vec::new());
        let mut w = zip::ZipWriter::new(buf);
        for (name, data) in entries {
            let opts: zip::write::FileOptions<zip::write::ExtendedFileOptions> =
                zip::write::FileOptions::default();
            w.start_file(*name, opts).unwrap();
            w.write_all(data).unwrap();
        }
        w.finish().unwrap().into_inner()
    }

    #[test]
    fn path_traversal_is_quarantined() {
        let bytes = make_zip(&[("../evil.xhtml", b"bad")]);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.epub");
        std::fs::write(&path, &bytes).unwrap();
        let mut issues = Vec::new();
        let _ = validate(&path, &mut issues).unwrap();
        assert!(issues.iter().any(|i| {
            i.severity == Severity::Irrecoverable
                && matches!(&i.kind, IssueKind::PathTraversal { .. })
        }));
    }

    #[test]
    fn clean_zip_produces_no_issues() {
        let bytes = make_zip(&[("OEBPS/content.opf", b"<package/>")]);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.epub");
        std::fs::write(&path, &bytes).unwrap();
        let mut issues = Vec::new();
        let handle = validate(&path, &mut issues).unwrap();
        assert!(issues.is_empty());
        assert_eq!(handle.entries, vec!["OEBPS/content.opf"]);
    }

    #[test]
    fn corrupt_zip_emits_irrecoverable() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.epub");
        std::fs::write(&path, b"not a zip file").unwrap();
        let mut issues = Vec::new();
        let _ = validate(&path, &mut issues).unwrap();
        assert!(issues.iter().any(|i| {
            i.severity == Severity::Irrecoverable
                && matches!(&i.kind, IssueKind::CorruptEntry { .. })
        }));
    }
}
