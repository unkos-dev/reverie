//! `ZIP` archive integrity layer (Layer 1) and the `ZipHandle` backing store.
//!
//! Checks the archive file size (2 GB cap) before the file is read, opens the
//! archive with `rawzip`, rejects trailing data the locator would otherwise
//! silently tolerate, and checks the declared central-directory entry count
//! against the cap before any header is parsed. The central directory is
//! then walked as a counted iteration that refuses at the cap, checking
//! every entry for a valid `UTF-8` name, path safety, a duplicate name, an
//! allowed compression method (Stored or Deflate), per-entry uncompressed
//! size (500 MB cap), aggregate uncompressed size (2 GB cap), and
//! extractability. After the counted iteration, the whole archive is checked
//! for data preceding the first entry and for a counted total that differs
//! from the declared count. Entries passing all checks are recorded in
//! `ZipHandle::entries`; the raw bytes are kept in `ZipHandle::bytes` so
//! upper layers can re-read entries without additional filesystem I/O.
//!
//! All size checks use the `ZIP` central-directory declared size to bound
//! allocation, plus a lying-central-directory probe for small entries.

use std::collections::HashSet;
use std::io::Read;
use std::path::Path;

use flate2::bufread::DeflateDecoder;
use rawzip::{CompressionMethod, ZipArchive, ZipSliceArchive};

use super::{
    Issue, IssueKind, Layer, MAX_AGGREGATE_UNCOMPRESSED_BYTES, MAX_ARCHIVE_BYTES,
    MAX_ENTRY_UNCOMPRESSED_BYTES, MAX_ZIP_ENTRIES, Severity,
};

/// Maximum bytes to search backwards from the end of the file for the
/// end-of-central-directory signature: the fixed 22-byte record plus the
/// `ZIP` specification's maximum comment length.
const MAX_EOCD_SEARCH_SPACE: u64 = 22 + 65_535;

/// Lightweight handle returned by `zip_layer` so upper layers can re-open the archive.
pub struct ZipHandle {
    /// Raw bytes of the entire archive (read once; `ZIP` seeks into this).
    pub bytes: Vec<u8>,
    /// Names of all successfully readable entries.
    pub entries: Vec<String>,
}

const fn empty_handle() -> ZipHandle {
    ZipHandle {
        bytes: Vec::new(),
        entries: Vec::new(),
    }
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
        return Ok(empty_handle());
    }

    let bytes = std::fs::read(path)?;

    // THREAT: every ambiguity the locator tolerates instead of rejecting is a
    // rejection here. The entry-count guard below is only sound if this layer
    // never opens an archive interpretation the writer side would not itself
    // have produced.
    let Ok(archive) =
        ZipArchive::with_max_search_space(MAX_EOCD_SEARCH_SPACE).locate_in_slice(bytes)
    else {
        issues.push(Issue {
            layer: Layer::Zip,
            severity: Severity::Irrecoverable,
            kind: IssueKind::CorruptEntry {
                entry_name: "<archive>".to_string(),
            },
        });
        return Ok(empty_handle());
    };

    // The locator accepts a comment that ends before the true end of the
    // file; `end_offset` does not rely on self-reported sizes, so unaccounted
    // trailing data is a rejection here rather than a silently ignored tail.
    if archive.end_offset() != u64::try_from(archive.get_ref().len()).unwrap_or(u64::MAX) {
        issues.push(Issue {
            layer: Layer::Zip,
            severity: Severity::Irrecoverable,
            kind: IssueKind::CorruptEntry {
                entry_name: "<archive>".to_string(),
            },
        });
        return Ok(empty_handle());
    }

    if archive.entries_hint() > u64::try_from(MAX_ZIP_ENTRIES).unwrap_or(u64::MAX) {
        issues.push(Issue {
            layer: Layer::Zip,
            severity: Severity::Irrecoverable,
            kind: IssueKind::EntryCapExceeded {
                count: archive.entries_hint(),
                limit: MAX_ZIP_ENTRIES,
            },
        });
        return Ok(empty_handle());
    }

    validate_entries(&archive, issues).map_or_else(
        || Ok(empty_handle()),
        |entries| {
            Ok(ZipHandle {
                bytes: archive.into_inner(),
                entries,
            })
        },
    )
}

/// Validates every central-directory entry via a counted iteration that
/// refuses at `MAX_ZIP_ENTRIES + 1`, then checks the whole archive for a
/// prelude and for a counted total that differs from the declared count in
/// either direction. Returns the readable entry names in directory order, or
/// `None` after pushing an `Irrecoverable` issue for the first entry or
/// check that fails.
#[expect(
    clippy::too_many_lines,
    reason = "one linear guard-clause sequence per entry sharing loop state (seen names, aggregate size, prelude tracking); splitting would require threading that state through helper signatures and obscure the check order"
)]
fn validate_entries(
    archive: &ZipSliceArchive<Vec<u8>>,
    issues: &mut Vec<Issue>,
) -> Option<Vec<String>> {
    let mut entries = Vec::new();
    let mut seen_names: HashSet<String> = HashSet::new();
    let mut aggregate_size: u64 = 0;
    let mut min_local_header_offset: Option<u64> = None;
    let mut count: usize = 0;

    let mut iter = archive.entries();
    loop {
        let record = match iter.next_entry() {
            Ok(Some(record)) => record,
            Ok(None) => break,
            Err(_) => {
                issues.push(Issue {
                    layer: Layer::Zip,
                    severity: Severity::Irrecoverable,
                    kind: IssueKind::CorruptEntry {
                        entry_name: "<central directory>".to_string(),
                    },
                });
                return None;
            }
        };

        count += 1;
        if count > MAX_ZIP_ENTRIES {
            issues.push(Issue {
                layer: Layer::Zip,
                severity: Severity::Irrecoverable,
                kind: IssueKind::EntryCapExceeded {
                    count: u64::try_from(count).unwrap_or(u64::MAX),
                    limit: MAX_ZIP_ENTRIES,
                },
            });
            return None;
        }

        let file_path = record.file_path();
        let name_bytes: &[u8] = file_path.as_ref();
        let Ok(name) = std::str::from_utf8(name_bytes) else {
            issues.push(Issue {
                layer: Layer::Zip,
                severity: Severity::Irrecoverable,
                kind: IssueKind::CorruptEntry {
                    entry_name: String::from_utf8_lossy(name_bytes).into_owned(),
                },
            });
            return None;
        };
        let name = name.to_string();

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

        if !seen_names.insert(name.clone()) {
            issues.push(Issue {
                layer: Layer::Zip,
                severity: Severity::Irrecoverable,
                kind: IssueKind::DuplicateEntry { entry_name: name },
            });
            return None;
        }

        let method = record.compression_method();
        if method != CompressionMethod::STORE && method != CompressionMethod::DEFLATE {
            issues.push(Issue {
                layer: Layer::Zip,
                severity: Severity::Irrecoverable,
                kind: IssueKind::UnsupportedCompression {
                    entry_name: name,
                    method: method.as_u16(),
                },
            });
            return None;
        }

        // Per-entry size check (use the central directory's uncompressed hint)
        let uncompressed = record.uncompressed_size_hint();
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

        min_local_header_offset = Some(min_local_header_offset.map_or_else(
            || record.local_header_offset(),
            |min| min.min(record.local_header_offset()),
        ));

        let Ok(slice_entry) = archive.get_entry(record.wayfinder()) else {
            issues.push(Issue {
                layer: Layer::Zip,
                severity: Severity::Irrecoverable,
                kind: IssueKind::CorruptEntry { entry_name: name },
            });
            return None;
        };

        // C3: Extractability check — cap the probe to min(declared+1, 4096) to
        // avoid allocating the full declared size (up to 500 MB) per entry.
        // Preserves lying-central-directory detection for small declared sizes.
        let probe_cap = uncompressed.saturating_add(1).min(4_096);
        let mut buf = Vec::new();
        let probe_result = if method == CompressionMethod::DEFLATE {
            DeflateDecoder::new(slice_entry.data())
                .take(probe_cap)
                .read_to_end(&mut buf)
        } else {
            slice_entry.data().take(probe_cap).read_to_end(&mut buf)
        };
        if probe_result.is_err() {
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

    let prelude = min_local_header_offset.unwrap_or_else(|| archive.directory_offset());
    if prelude != 0 {
        issues.push(Issue {
            layer: Layer::Zip,
            severity: Severity::Irrecoverable,
            kind: IssueKind::PreludeBeforeArchive { bytes: prelude },
        });
        return None;
    }

    if u64::try_from(count).unwrap_or(u64::MAX) != archive.entries_hint() {
        issues.push(Issue {
            layer: Layer::Zip,
            severity: Severity::Irrecoverable,
            kind: IssueKind::CorruptEntry {
                entry_name: "<central directory>".to_string(),
            },
        });
        return None;
    }

    Some(entries)
}

/// Read a specific entry from the archive bytes.
///
/// Returns `None` if the entry is absent, uses a compression method other
/// than Stored or Deflate, or its decompressed bytes fail `ZIP`'s declared
/// `CRC-32` or size.
#[must_use]
pub fn read_entry(handle: &ZipHandle, entry_name: &str) -> Option<Vec<u8>> {
    read_entry_from_bytes(&handle.bytes, entry_name)
}

/// Read one entry directly from raw archive bytes.
///
/// For call sites that hold bytes without a [`ZipHandle`] (the repair pass
/// and the writeback orchestrator). Returns `None` on any locator, entry, or
/// verification failure.
#[must_use]
pub fn read_entry_from_bytes(bytes: &[u8], entry_name: &str) -> Option<Vec<u8>> {
    let archive = ZipArchive::with_max_search_space(MAX_EOCD_SEARCH_SPACE)
        .locate_in_slice(bytes)
        .ok()?;

    let mut iter = archive.entries();
    let record = loop {
        let record = iter.next_entry().ok()??;
        let file_path = record.file_path();
        let name_bytes: &[u8] = file_path.as_ref();
        if name_bytes == entry_name.as_bytes() {
            break record;
        }
    };

    let method = record.compression_method();
    if method != CompressionMethod::STORE && method != CompressionMethod::DEFLATE {
        return None;
    }

    let slice_entry = archive.get_entry(record.wayfinder()).ok()?;
    let cap = MAX_ENTRY_UNCOMPRESSED_BYTES + 1;
    let mut buf = Vec::new();
    let result = if method == CompressionMethod::DEFLATE {
        let decoder = DeflateDecoder::new(slice_entry.data());
        slice_entry
            .verifying_reader(decoder)
            .take(cap)
            .read_to_end(&mut buf)
    } else {
        slice_entry
            .verifying_reader(slice_entry.data())
            .take(cap)
            .read_to_end(&mut buf)
    };
    result.ok()?;
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

    /// Byte offset of each central-directory file-header record in `bytes`,
    /// found by scanning for the central-directory signature (distinct from
    /// the local-file-header signature, so a simple scan is unambiguous for
    /// these test fixtures).
    fn cd_entry_offsets(bytes: &[u8]) -> Vec<usize> {
        const SIG: [u8; 4] = [0x50, 0x4b, 0x01, 0x02];
        bytes
            .windows(4)
            .enumerate()
            .filter_map(|(i, w)| (w == SIG).then_some(i))
            .collect()
    }

    /// Byte offset of the classic end-of-central-directory record.
    fn find_classic_eocd(bytes: &[u8]) -> usize {
        const SIG: [u8; 4] = [0x50, 0x4b, 0x05, 0x06];
        bytes.windows(4).rposition(|w| w == SIG).unwrap()
    }

    /// Patch the declared entry-count hint to `new_total`. Prefers the
    /// `ZIP64` record's "entries on this disk" field (offset 24) when a
    /// `ZIP64` locator precedes the classic record: `rawzip`'s declared-count
    /// hint reads that field, not the "total entries across all disks" field
    /// at offset 32, so both are patched defensively when a locator is
    /// found.
    fn patch_entry_count_hint(bytes: &mut [u8], new_total: u16) {
        const ZIP64_LOCATOR_SIG: [u8; 4] = [0x50, 0x4b, 0x06, 0x07];
        const ZIP64_EOCD_SIG: [u8; 4] = [0x50, 0x4b, 0x06, 0x06];

        let eocd = find_classic_eocd(bytes);
        if eocd >= 20 {
            let locator = eocd - 20;
            if bytes[locator..locator + 4] == ZIP64_LOCATOR_SIG {
                let zip64_offset =
                    u64::from_le_bytes(bytes[locator + 8..locator + 16].try_into().unwrap());
                let zip64_pos = usize::try_from(zip64_offset).unwrap();
                if bytes[zip64_pos..zip64_pos + 4] == ZIP64_EOCD_SIG {
                    let total = u64::from(new_total).to_le_bytes();
                    bytes[zip64_pos + 24..zip64_pos + 32].copy_from_slice(&total);
                    bytes[zip64_pos + 32..zip64_pos + 40].copy_from_slice(&total);
                    return;
                }
            }
        }

        let patched = new_total.to_le_bytes();
        bytes[eocd + 10] = patched[0];
        bytes[eocd + 11] = patched[1];
    }

    fn write_temp(bytes: &[u8]) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.epub");
        std::fs::write(&path, bytes).unwrap();
        (dir, path)
    }

    #[test]
    fn path_traversal_is_quarantined() {
        let bytes = make_zip(&[("../evil.xhtml", b"bad")]);
        let (_dir, path) = write_temp(&bytes);
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
        let (_dir, path) = write_temp(&bytes);
        let mut issues = Vec::new();
        let handle = validate(&path, &mut issues).unwrap();
        assert!(issues.is_empty());
        assert_eq!(handle.entries, vec!["OEBPS/content.opf"]);
    }

    #[test]
    fn corrupt_zip_emits_irrecoverable() {
        let (_dir, path) = write_temp(b"not a zip file");
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
        let (_dir, path) = write_temp(&bytes);
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
        let (_dir, path) = write_temp(&bytes);
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
    fn counted_iteration_cap_fires_despite_understated_hint() {
        // A real 20,001-entry archive whose end record understates the count
        // as exactly the cap: the pre-loop hint check must not fire (it is
        // not over cap), so the counted iteration itself must refuse at
        // header 20,001.
        let names: Vec<String> = (0..=MAX_ZIP_ENTRIES).map(|i| format!("e{i}.txt")).collect();
        let entries: Vec<(&str, &[u8])> = names.iter().map(|n| (n.as_str(), &b""[..])).collect();
        let mut bytes = make_zip(&entries);
        patch_entry_count_hint(&mut bytes, u16::try_from(MAX_ZIP_ENTRIES).unwrap());
        let (_dir, path) = write_temp(&bytes);
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
    fn entries_hint_over_cap_is_refused_before_iteration() {
        let count = u64::try_from(MAX_ZIP_ENTRIES + 1).unwrap();
        let bytes = zip64_eocd_tail(count);
        let (_dir, path) = write_temp(&bytes);
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

    #[test]
    fn three_deflated_entries_validate_clean_in_order() {
        let bytes = make_zip(&[
            ("a.txt", b"first entry payload data, long enough to deflate"),
            (
                "b.txt",
                b"second entry payload data, long enough to deflate",
            ),
            ("c.txt", b"third entry payload data, long enough to deflate"),
        ]);
        let (_dir, path) = write_temp(&bytes);
        let mut issues = Vec::new();
        let handle = validate(&path, &mut issues).unwrap();
        assert!(issues.is_empty());
        assert_eq!(
            handle.entries,
            vec![
                "a.txt".to_string(),
                "b.txt".to_string(),
                "c.txt".to_string()
            ]
        );
    }

    #[test]
    fn empty_archive_validates_clean() {
        // No entries at all: entries_hint() is 0 and the prelude falls back
        // to directory_offset() rather than min(local_header_offset).
        let buf = std::io::Cursor::new(Vec::new());
        let w = zip::ZipWriter::new(buf);
        let bytes = w.finish().unwrap().into_inner();
        let (_dir, path) = write_temp(&bytes);
        let mut issues = Vec::new();
        let handle = validate(&path, &mut issues).unwrap();
        assert!(issues.is_empty());
        assert!(handle.entries.is_empty());
    }

    #[test]
    fn directory_entry_and_file_validate_clean_in_order() {
        let buf = std::io::Cursor::new(Vec::new());
        let mut w = zip::ZipWriter::new(buf);
        let dir_opts: zip::write::FileOptions<zip::write::ExtendedFileOptions> =
            zip::write::FileOptions::default();
        w.add_directory("OEBPS", dir_opts).unwrap();
        let file_opts: zip::write::FileOptions<zip::write::ExtendedFileOptions> =
            zip::write::FileOptions::default();
        w.start_file("OEBPS/content.opf", file_opts).unwrap();
        w.write_all(b"<package/>").unwrap();
        let bytes = w.finish().unwrap().into_inner();
        let (_dir, path) = write_temp(&bytes);
        let mut issues = Vec::new();
        let handle = validate(&path, &mut issues).unwrap();
        assert!(issues.is_empty());
        assert_eq!(
            handle.entries,
            vec!["OEBPS/".to_string(), "OEBPS/content.opf".to_string()]
        );
    }

    #[test]
    fn trailing_one_byte_is_quarantined() {
        let mut bytes = make_zip(&[("a.txt", b"hello world")]);
        bytes.push(0xAA);
        let (_dir, path) = write_temp(&bytes);
        let mut issues = Vec::new();
        let handle = validate(&path, &mut issues).unwrap();
        assert!(issues.iter().any(|i| {
            i.severity == Severity::Irrecoverable
                && matches!(&i.kind, IssueKind::CorruptEntry { .. })
        }));
        assert!(handle.bytes.is_empty());
    }

    #[test]
    fn trailing_70000_bytes_is_quarantined() {
        let mut bytes = make_zip(&[("a.txt", b"hello world")]);
        bytes.extend(std::iter::repeat_n(0xAAu8, 70_000));
        let (_dir, path) = write_temp(&bytes);
        let mut issues = Vec::new();
        let handle = validate(&path, &mut issues).unwrap();
        assert!(issues.iter().any(|i| {
            i.severity == Severity::Irrecoverable
                && matches!(&i.kind, IssueKind::CorruptEntry { .. })
        }));
        assert!(handle.bytes.is_empty());
    }

    #[test]
    fn archive_with_comment_validates_clean() {
        let buf = std::io::Cursor::new(Vec::new());
        let mut w = zip::ZipWriter::new(buf);
        let opts: zip::write::FileOptions<zip::write::ExtendedFileOptions> =
            zip::write::FileOptions::default();
        w.start_file("a.txt", opts).unwrap();
        w.write_all(b"hello world").unwrap();
        w.set_comment("a deliberate archive comment").unwrap();
        let bytes = w.finish().unwrap().into_inner();
        let (_dir, path) = write_temp(&bytes);
        let mut issues = Vec::new();
        let handle = validate(&path, &mut issues).unwrap();
        assert!(issues.is_empty());
        assert_eq!(handle.entries, vec!["a.txt".to_string()]);
    }

    #[test]
    fn decoy_end_record_with_bogus_offset_is_quarantined() {
        let mut bytes = make_zip(&[("a.txt", b"hello world")]);
        // Decoy classic end-of-central-directory record declaring one entry
        // at a bogus central-directory offset.
        let mut tail = Vec::new();
        tail.extend_from_slice(&[0x50, 0x4b, 0x05, 0x06]);
        tail.extend_from_slice(&0u16.to_le_bytes());
        tail.extend_from_slice(&0u16.to_le_bytes());
        tail.extend_from_slice(&1u16.to_le_bytes());
        tail.extend_from_slice(&1u16.to_le_bytes());
        tail.extend_from_slice(&46u32.to_le_bytes());
        tail.extend_from_slice(&12_345u32.to_le_bytes());
        tail.extend_from_slice(&0u16.to_le_bytes());
        bytes.extend_from_slice(&tail);
        let (_dir, path) = write_temp(&bytes);
        let mut issues = Vec::new();
        let handle = validate(&path, &mut issues).unwrap();
        assert!(issues.iter().any(|i| {
            i.severity == Severity::Irrecoverable
                && matches!(&i.kind, IssueKind::CorruptEntry { .. })
        }));
        assert!(handle.bytes.is_empty());
    }

    #[test]
    fn directory_longer_than_hint_is_quarantined() {
        let mut bytes = make_zip(&[("a.txt", b"one"), ("b.txt", b"two"), ("c.txt", b"three")]);
        let eocd = find_classic_eocd(&bytes);
        let hint = u16::from_le_bytes([bytes[eocd + 10], bytes[eocd + 11]]);
        let patched = (hint - 1).to_le_bytes();
        bytes[eocd + 10] = patched[0];
        bytes[eocd + 11] = patched[1];
        let (_dir, path) = write_temp(&bytes);
        let mut issues = Vec::new();
        let handle = validate(&path, &mut issues).unwrap();
        assert!(issues.iter().any(|i| {
            i.severity == Severity::Irrecoverable
                && matches!(&i.kind, IssueKind::CorruptEntry { .. })
        }));
        assert!(handle.bytes.is_empty());
    }

    #[test]
    fn directory_shorter_than_hint_is_quarantined() {
        let mut bytes = make_zip(&[("a.txt", b"one"), ("b.txt", b"two"), ("c.txt", b"three")]);
        let eocd = find_classic_eocd(&bytes);
        let hint = u16::from_le_bytes([bytes[eocd + 10], bytes[eocd + 11]]);
        let patched = (hint + 1).to_le_bytes();
        bytes[eocd + 10] = patched[0];
        bytes[eocd + 11] = patched[1];
        let (_dir, path) = write_temp(&bytes);
        let mut issues = Vec::new();
        let handle = validate(&path, &mut issues).unwrap();
        assert!(issues.iter().any(|i| {
            i.severity == Severity::Irrecoverable
                && matches!(&i.kind, IssueKind::CorruptEntry { .. })
        }));
        assert!(handle.bytes.is_empty());
    }

    #[test]
    fn prepended_junk_before_archive_is_quarantined() {
        let mut bytes = vec![0xAAu8; 4];
        bytes.extend(make_zip(&[("a.txt", b"hello world")]));
        let (_dir, path) = write_temp(&bytes);
        let mut issues = Vec::new();
        let handle = validate(&path, &mut issues).unwrap();
        assert!(issues.iter().any(|i| {
            i.severity == Severity::Irrecoverable
                && matches!(&i.kind, IssueKind::PreludeBeforeArchive { bytes } if *bytes == 4)
        }));
        assert!(handle.bytes.is_empty());
    }

    #[test]
    fn duplicate_entry_names_are_quarantined() {
        let mut bytes = make_zip(&[("a.txt", b"one"), ("b.txt", b"two")]);
        let offsets = cd_entry_offsets(&bytes);
        assert_eq!(offsets.len(), 2);
        // Patch the second entry's central-directory name to match the
        // first's; both names are 5 bytes, so nothing else shifts.
        let name_start = offsets[1] + 46;
        bytes[name_start] = b'a';
        let (_dir, path) = write_temp(&bytes);
        let mut issues = Vec::new();
        let handle = validate(&path, &mut issues).unwrap();
        assert!(issues.iter().any(|i| {
            i.severity == Severity::Irrecoverable
                && matches!(&i.kind, IssueKind::DuplicateEntry { entry_name } if entry_name == "a.txt")
        }));
        assert!(handle.bytes.is_empty());
    }

    #[test]
    fn non_utf8_name_is_quarantined() {
        let mut bytes = make_zip(&[("x.txt", b"data")]);
        let offsets = cd_entry_offsets(&bytes);
        assert_eq!(offsets.len(), 1);
        let name_start = offsets[0] + 46;
        bytes[name_start] = 0xFF;
        let (_dir, path) = write_temp(&bytes);
        let mut issues = Vec::new();
        let handle = validate(&path, &mut issues).unwrap();
        assert!(issues.iter().any(|i| {
            i.severity == Severity::Irrecoverable
                && matches!(&i.kind, IssueKind::CorruptEntry { .. })
        }));
        assert!(handle.bytes.is_empty());
    }

    #[test]
    fn unsupported_compression_method_is_quarantined() {
        let buf = std::io::Cursor::new(Vec::new());
        let mut w = zip::ZipWriter::new(buf);
        let opts: zip::write::FileOptions<zip::write::ExtendedFileOptions> =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Bzip2);
        w.start_file("a.txt", opts).unwrap();
        w.write_all(b"payload").unwrap();
        let bytes = w.finish().unwrap().into_inner();
        let (_dir, path) = write_temp(&bytes);
        let mut issues = Vec::new();
        let handle = validate(&path, &mut issues).unwrap();
        assert!(issues.iter().any(|i| {
            i.severity == Severity::Irrecoverable
                && matches!(
                    &i.kind,
                    IssueKind::UnsupportedCompression { entry_name, method }
                        if entry_name == "a.txt" && *method == 12
                )
        }));
        assert!(handle.bytes.is_empty());
    }

    #[test]
    fn read_entry_rejects_wrong_crc() {
        let mut bytes = make_zip(&[(
            "a.txt",
            b"payload long enough to be worth deflating in this test",
        )]);
        let offsets = cd_entry_offsets(&bytes);
        assert_eq!(offsets.len(), 1);
        // CRC-32 field is at offset+16 in the central directory record.
        let crc_start = offsets[0] + 16;
        bytes[crc_start] ^= 0xFF;
        let handle = ZipHandle {
            bytes,
            entries: vec!["a.txt".to_string()],
        };
        assert!(read_entry(&handle, "a.txt").is_none());
    }

    #[test]
    fn read_entry_returns_byte_identical_stored_and_deflated() {
        let stored_data = b"stored payload, no compression applied".to_vec();
        let deflated_data = b"deflated payload data data data data data data data data".to_vec();

        let buf = std::io::Cursor::new(Vec::new());
        let mut w = zip::ZipWriter::new(buf);
        let stored_opts: zip::write::FileOptions<zip::write::ExtendedFileOptions> =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);
        w.start_file("stored.bin", stored_opts).unwrap();
        w.write_all(&stored_data).unwrap();
        let deflated_opts: zip::write::FileOptions<zip::write::ExtendedFileOptions> =
            zip::write::FileOptions::default();
        w.start_file("deflated.bin", deflated_opts).unwrap();
        w.write_all(&deflated_data).unwrap();
        let bytes = w.finish().unwrap().into_inner();

        let (_dir, path) = write_temp(&bytes);
        let mut issues = Vec::new();
        let handle = validate(&path, &mut issues).unwrap();
        assert!(issues.is_empty());

        assert_eq!(
            read_entry(&handle, "stored.bin").as_deref(),
            Some(stored_data.as_slice())
        );
        assert_eq!(
            read_entry(&handle, "deflated.bin").as_deref(),
            Some(deflated_data.as_slice())
        );
    }

    #[test]
    fn stored_entry_larger_than_probe_cap_validates_clean_and_reads_back_whole() {
        // 8192 bytes is larger than the 4096-byte probe cap, so the probe
        // reads only a prefix; the lying-directory check must not fire just
        // because the entry is bigger than the probe, only when the probe
        // cap itself equals declared+1 and fills completely.
        let data = vec![0xABu8; 8_192];
        let buf = std::io::Cursor::new(Vec::new());
        let mut w = zip::ZipWriter::new(buf);
        let opts: zip::write::FileOptions<zip::write::ExtendedFileOptions> =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);
        w.start_file("big.bin", opts).unwrap();
        w.write_all(&data).unwrap();
        let bytes = w.finish().unwrap().into_inner();

        let (_dir, path) = write_temp(&bytes);
        let mut issues = Vec::new();
        let handle = validate(&path, &mut issues).unwrap();
        assert!(issues.is_empty());
        assert_eq!(
            read_entry(&handle, "big.bin").as_deref(),
            Some(data.as_slice())
        );
    }
}
