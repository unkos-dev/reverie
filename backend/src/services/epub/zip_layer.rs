//! `ZIP` archive integrity layer (Layer 1) and the `ZipHandle` backing store.
//!
//! Checks the archive file size (2 GB cap) before the file is read, then the
//! declared central-directory entry count (20,000 cap) from the archive tail
//! before the archive is parsed, then reads the entire archive into memory
//! once and checks every entry for path traversal, per-entry uncompressed
//! size (500 MB cap), aggregate uncompressed size (2 GB cap), and
//! extractability. Entries passing all checks are recorded in
//! `ZipHandle::entries`; the raw bytes are kept in `ZipHandle::bytes` so
//! upper layers can re-open the archive without additional filesystem I/O.
//!
//! All size checks use the `ZIP` central-directory declared size to bound
//! allocation, plus a lying-central-directory probe for small entries.

use std::io::Read;
use std::path::Path;
use zip::ZipArchive;

use super::{
    Issue, IssueKind, Layer, MAX_AGGREGATE_UNCOMPRESSED_BYTES, MAX_ARCHIVE_BYTES,
    MAX_ENTRY_UNCOMPRESSED_BYTES, MAX_ZIP_ENTRIES, Severity,
};

/// Lightweight handle returned by `zip_layer` so upper layers can re-open the archive.
pub struct ZipHandle {
    /// Raw bytes of the entire archive (read once; `ZIP` seeks into this).
    pub bytes: Vec<u8>,
    /// Names of all successfully readable entries.
    pub entries: Vec<String>,
}

/// Validate `ZIP` integrity, path safety, and size bounds.
///
/// Returns a `ZipHandle` on success. Appends `Issue`s to `issues`.
/// If any `Irrecoverable` issue is added, the caller short-circuits.
///
/// # Errors
///
/// Returns `EpubError::Io` if the file at `path` cannot be read from
/// the filesystem. A corrupt central directory or unreadable entry is recorded
/// as an `IssueKind::CorruptEntry` issue rather than returned as an
/// error — the function still returns `Ok` with those issues appended.
pub fn validate(path: &Path, issues: &mut Vec<Issue>) -> Result<ZipHandle, super::EpubError> {
    let size = std::fs::metadata(path)?.len();
    if size > MAX_ARCHIVE_BYTES {
        issues.push(Issue {
            layer: Layer::Zip,
            severity: Severity::Irrecoverable,
            kind: IssueKind::ArchiveTooLarge {
                size,
                limit: MAX_ARCHIVE_BYTES,
            },
        });
        return Ok(ZipHandle {
            bytes: Vec::new(),
            entries: Vec::new(),
        });
    }

    let bytes = std::fs::read(path)?;
    let mut entries = Vec::new();

    if let Some(count) = declared_entry_count(&bytes)
        && count > u64::try_from(MAX_ZIP_ENTRIES).unwrap_or(u64::MAX)
    {
        issues.push(Issue {
            layer: Layer::Zip,
            severity: Severity::Irrecoverable,
            kind: IssueKind::EntryCapExceeded {
                count,
                limit: MAX_ZIP_ENTRIES,
            },
        });
        return Ok(ZipHandle {
            bytes: Vec::new(),
            entries: Vec::new(),
        });
    }

    'zip: {
        let cursor = std::io::Cursor::new(&bytes[..]);
        let Ok(mut archive) = ZipArchive::new(cursor) else {
            issues.push(Issue {
                layer: Layer::Zip,
                severity: Severity::Irrecoverable,
                kind: IssueKind::CorruptEntry {
                    entry_name: "<archive>".to_string(),
                },
            });
            break 'zip;
        };

        let count = archive.len();
        if count > MAX_ZIP_ENTRIES {
            issues.push(Issue {
                layer: Layer::Zip,
                severity: Severity::Irrecoverable,
                kind: IssueKind::EntryCapExceeded {
                    count: u64::try_from(count).unwrap_or(u64::MAX),
                    limit: MAX_ZIP_ENTRIES,
                },
            });
            break 'zip;
        }

        if let Some(found) = validate_entries(&mut archive, issues) {
            entries = found;
        }
    } // archive dropped here; borrow on `bytes` released

    Ok(ZipHandle { bytes, entries })
}

/// Validates every central-directory entry; returns the readable entry names,
/// or `None` after pushing an `Irrecoverable` issue for the first entry that fails.
fn validate_entries(
    archive: &mut ZipArchive<std::io::Cursor<&[u8]>>,
    issues: &mut Vec<Issue>,
) -> Option<Vec<String>> {
    let mut entries = Vec::new();
    let mut aggregate_size: u64 = 0;

    for i in 0..archive.len() {
        // D1: use match instead of `?` so a corrupt entry pushes an Irrecoverable
        // issue and returns None rather than propagating Err up to the caller
        // (which would misclassify as "degraded").
        let Ok(file) = archive.by_index(i) else {
            issues.push(Issue {
                layer: Layer::Zip,
                severity: Severity::Irrecoverable,
                kind: IssueKind::CorruptEntry {
                    entry_name: format!("entry[{i}]"),
                },
            });
            return None;
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
            return None;
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
            return None;
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
            return None;
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
            return None;
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
            return None;
        }

        entries.push(name);
    }

    Some(entries)
}

/// Declared central-directory entry count from the archive tail, so the cap
/// is applied before the archive is parsed; `None` when no end record is found.
fn declared_entry_count(bytes: &[u8]) -> Option<u64> {
    const EOCD_SIG: [u8; 4] = [0x50, 0x4b, 0x05, 0x06];
    const EOCD_LOCATOR_SIG: [u8; 4] = [0x50, 0x4b, 0x06, 0x07];
    const ZIP64_EOCD_SIG: [u8; 4] = [0x50, 0x4b, 0x06, 0x06];

    let len = bytes.len();
    let hi = len.checked_sub(22)?;
    let lo = len.saturating_sub(22 + 65_535);

    let mut pos = hi;
    let eocd_pos = loop {
        if bytes[pos..pos + 4] == EOCD_SIG {
            let comment_len = u16::from_le_bytes(bytes[pos + 20..pos + 22].try_into().ok()?);
            if pos + 22 + usize::from(comment_len) == len {
                break pos;
            }
        }
        if pos == lo {
            return None;
        }
        pos -= 1;
    };

    let classic_count = u16::from_le_bytes(bytes[eocd_pos + 10..eocd_pos + 12].try_into().ok()?);

    // The zip64 locator, if present, is the fixed-size record directly before the EOCD.
    if eocd_pos >= 20 {
        let locator = eocd_pos - 20;
        if bytes[locator..locator + 4] == EOCD_LOCATOR_SIG {
            let zip64_offset =
                u64::from_le_bytes(bytes[locator + 8..locator + 16].try_into().ok()?);
            if let Ok(zip64_pos) = usize::try_from(zip64_offset)
                && let Some(zip64_end) = zip64_pos.checked_add(56)
                && zip64_end <= len
                && bytes[zip64_pos..zip64_pos + 4] == ZIP64_EOCD_SIG
            {
                let total =
                    u64::from_le_bytes(bytes[zip64_pos + 32..zip64_pos + 40].try_into().ok()?);
                return Some(total);
            }
        }
    }

    Some(u64::from(classic_count))
}

/// Read a specific entry from the archive bytes. Returns None if not found.
#[must_use]
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

    #[test]
    fn entry_count_at_cap_passes() {
        let names: Vec<String> = (0..MAX_ZIP_ENTRIES).map(|i| format!("e{i}.txt")).collect();
        let entries: Vec<(&str, &[u8])> = names.iter().map(|n| (n.as_str(), &b""[..])).collect();
        let bytes = make_zip(&entries);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.epub");
        std::fs::write(&path, &bytes).unwrap();
        let mut issues = Vec::new();
        let handle = validate(&path, &mut issues).unwrap();
        assert!(
            !issues
                .iter()
                .any(|i| matches!(&i.kind, IssueKind::EntryCapExceeded { .. }))
        );
        assert_eq!(handle.entries.len(), MAX_ZIP_ENTRIES);
    }

    #[test]
    fn entry_count_over_cap_is_quarantined() {
        let names: Vec<String> = (0..=MAX_ZIP_ENTRIES).map(|i| format!("e{i}.txt")).collect();
        let entries: Vec<(&str, &[u8])> = names.iter().map(|n| (n.as_str(), &b""[..])).collect();
        let bytes = make_zip(&entries);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.epub");
        std::fs::write(&path, &bytes).unwrap();
        let mut issues = Vec::new();
        let handle = validate(&path, &mut issues).unwrap();
        assert!(issues.iter().any(|i| {
            i.severity == Severity::Irrecoverable
                && matches!(
                    &i.kind,
                    IssueKind::EntryCapExceeded { count, limit }
                        if *count == u64::try_from(MAX_ZIP_ENTRIES + 1).unwrap()
                            && *limit == MAX_ZIP_ENTRIES
                )
        }));
        assert!(handle.entries.is_empty());
        assert!(handle.bytes.is_empty());
    }

    #[test]
    fn oversized_archive_is_quarantined_unread() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.epub");
        let file = std::fs::File::create(&path).unwrap();
        file.set_len(MAX_ARCHIVE_BYTES + 1).unwrap();
        let mut issues = Vec::new();
        let handle = validate(&path, &mut issues).unwrap();
        assert!(issues.iter().any(|i| {
            i.severity == Severity::Irrecoverable
                && matches!(
                    &i.kind,
                    IssueKind::ArchiveTooLarge { size, limit }
                        if *size == MAX_ARCHIVE_BYTES + 1 && *limit == MAX_ARCHIVE_BYTES
                )
        }));
        assert!(handle.bytes.is_empty());
    }

    #[test]
    fn declared_count_matches_classic_archive() {
        let bytes = make_zip(&[("a.txt", b""), ("b.txt", b""), ("c.txt", b"")]);
        assert_eq!(declared_entry_count(&bytes), Some(3));
    }

    fn zip64_eocd_tail(total_entries: u64) -> Vec<u8> {
        let mut tail = Vec::new();

        // ZIP64 end of central directory record (56 bytes).
        tail.extend_from_slice(&[0x50, 0x4b, 0x06, 0x06]);
        tail.extend_from_slice(&44u64.to_le_bytes());
        tail.extend_from_slice(&0u16.to_le_bytes());
        tail.extend_from_slice(&0u16.to_le_bytes());
        tail.extend_from_slice(&0u32.to_le_bytes());
        tail.extend_from_slice(&0u32.to_le_bytes());
        tail.extend_from_slice(&total_entries.to_le_bytes());
        tail.extend_from_slice(&total_entries.to_le_bytes());
        tail.extend_from_slice(&0u64.to_le_bytes());
        tail.extend_from_slice(&0u64.to_le_bytes());

        // ZIP64 end of central directory locator (20 bytes).
        tail.extend_from_slice(&[0x50, 0x4b, 0x06, 0x07]);
        tail.extend_from_slice(&0u32.to_le_bytes());
        tail.extend_from_slice(&0u64.to_le_bytes());
        tail.extend_from_slice(&1u32.to_le_bytes());

        // Classic end of central directory record (22 bytes).
        tail.extend_from_slice(&[0x50, 0x4b, 0x05, 0x06]);
        tail.extend_from_slice(&0u16.to_le_bytes());
        tail.extend_from_slice(&0u16.to_le_bytes());
        tail.extend_from_slice(&0xFFFFu16.to_le_bytes());
        tail.extend_from_slice(&0xFFFFu16.to_le_bytes());
        tail.extend_from_slice(&0u32.to_le_bytes());
        tail.extend_from_slice(&0u32.to_le_bytes());
        tail.extend_from_slice(&0u16.to_le_bytes());

        tail
    }

    #[test]
    fn declared_count_reads_zip64_record() {
        let bytes = zip64_eocd_tail(70_000);
        assert_eq!(declared_entry_count(&bytes), Some(70_000));
    }

    #[test]
    fn declared_count_ignores_garbage() {
        assert_eq!(declared_entry_count(b"not a zip file"), None);

        let mut bytes = Vec::new();
        bytes.extend_from_slice(&[0x50, 0x4b, 0x05, 0x06]);
        bytes.extend_from_slice(&0u16.to_le_bytes());
        bytes.extend_from_slice(&0u16.to_le_bytes());
        bytes.extend_from_slice(&3u16.to_le_bytes());
        bytes.extend_from_slice(&3u16.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&5u16.to_le_bytes());
        bytes.extend_from_slice(b"hello");
        assert_eq!(declared_entry_count(&bytes), Some(3));
    }

    #[test]
    fn declared_count_over_cap_is_refused_before_parse() {
        let count = u64::try_from(MAX_ZIP_ENTRIES + 1).unwrap();
        let bytes = zip64_eocd_tail(count);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.epub");
        std::fs::write(&path, &bytes).unwrap();
        let mut issues = Vec::new();
        let handle = validate(&path, &mut issues).unwrap();
        assert!(issues.iter().any(|i| {
            i.severity == Severity::Irrecoverable
                && matches!(
                    &i.kind,
                    IssueKind::EntryCapExceeded { count: c, limit }
                        if *c == count && *limit == MAX_ZIP_ENTRIES
                )
        }));
        assert!(
            !issues
                .iter()
                .any(|i| matches!(&i.kind, IssueKind::CorruptEntry { .. }))
        );
        assert!(handle.bytes.is_empty());
    }
}
