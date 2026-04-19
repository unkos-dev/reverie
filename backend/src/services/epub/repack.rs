//! Shared ZIP repack helper for EPUB mutations.
//!
//! Preserves the EPUB spec's mimetype-first / stored constraint, preserves
//! per-entry compression (Stored entries stay Stored), and offers three
//! mutation knobs: OPF replacement, arbitrary binary-entry replacement
//! (e.g. cover image), and new-entry additions (e.g. regenerated
//! container.xml or a freshly-inserted cover manifest target).
//!
//! Callers are responsible for the final atomic rename of the returned
//! [`NamedTempFile`] onto the destination path.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::Path;

use tempfile::NamedTempFile;
use zip::write::{ExtendedFileOptions, FileOptions};
use zip::{ZipArchive, ZipWriter};

use super::{EpubError, MAX_ENTRY_UNCOMPRESSED_BYTES};

pub(super) const MIMETYPE_ENTRY: &str = "mimetype";
pub(super) const MIMETYPE_CONTENT: &[u8] = b"application/epub+zip";

/// Re-package the EPUB at `src_path` applying the provided mutations.
///
/// Writes to a fresh [`NamedTempFile`] in `dest_dir` (so the caller can
/// persist to a different directory on path-rename, or back to `src_path`'s
/// directory for in-place updates).  The caller owns the atomic rename.
///
/// - `opf_path` + `opf_replacement`: when both are Some, the ZIP entry whose
///   name equals `opf_path` is replaced with `opf_replacement` bytes.
/// - `binary_replacements`: entry-name → bytes overrides for any non-OPF
///   entry (e.g. a cover image). Entries in this map REPLACE existing
///   entries; they do not add new ones.
/// - `additions`: new ZIP entries to append after all existing entries have
///   been copied.  Use this for entries absent from the source (e.g. a
///   regenerated `META-INF/container.xml` or a freshly-inserted cover
///   manifest target).
///
/// Per-entry compression is preserved: an entry that was Stored in the source
/// stays Stored in the output; a Deflated entry stays Deflated.  The sole
/// exception is the required `mimetype` entry, which is always emitted first
/// with `CompressionMethod::Stored`.
pub fn with_modifications(
    src_path: &Path,
    dest_dir: &Path,
    opf_path: Option<&str>,
    opf_replacement: Option<&[u8]>,
    binary_replacements: &HashMap<String, Vec<u8>>,
    additions: &[(String, Vec<u8>, FileOptions<ExtendedFileOptions>)],
) -> Result<NamedTempFile, EpubError> {
    let bytes = std::fs::read(src_path)?;
    let temp = NamedTempFile::new_in(dest_dir)?;
    {
        let cursor = std::io::Cursor::new(&bytes[..]);
        let mut archive = ZipArchive::new(cursor)?;
        let mut writer = ZipWriter::new(&temp);

        // mimetype MUST be first and stored per EPUB spec.
        let stored: FileOptions<ExtendedFileOptions> =
            FileOptions::default().compression_method(zip::CompressionMethod::Stored);
        writer.start_file(MIMETYPE_ENTRY, stored)?;
        writer.write_all(MIMETYPE_CONTENT)?;

        for i in 0..archive.len() {
            let file = archive.by_index(i)?;
            let name = file.name().to_string();
            if name == MIMETYPE_ENTRY {
                continue;
            }

            let compression = file.compression();
            let bytes_to_write: Vec<u8> = if opf_path == Some(name.as_str())
                && let Some(repl) = opf_replacement
            {
                repl.to_vec()
            } else if let Some(replacement) = binary_replacements.get(&name) {
                replacement.clone()
            } else {
                let mut buf = Vec::new();
                file.take(MAX_ENTRY_UNCOMPRESSED_BYTES + 1)
                    .read_to_end(&mut buf)?;
                buf
            };

            let opts: FileOptions<ExtendedFileOptions> =
                FileOptions::default().compression_method(compression);
            writer.start_file(&name, opts)?;
            writer.write_all(&bytes_to_write)?;
        }

        for (name, entry_bytes, opts) in additions {
            writer.start_file(name, opts.clone())?;
            writer.write_all(entry_bytes)?;
        }
        writer.finish()?;
    }
    Ok(temp)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn write_entry(
        w: &mut ZipWriter<Cursor<Vec<u8>>>,
        name: &str,
        data: &[u8],
        compression: zip::CompressionMethod,
    ) {
        let opts: FileOptions<ExtendedFileOptions> =
            FileOptions::default().compression_method(compression);
        w.start_file(name, opts).unwrap();
        w.write_all(data).unwrap();
    }

    fn build_epub(entries: &[(&str, &[u8], zip::CompressionMethod)]) -> Vec<u8> {
        let buf = Cursor::new(Vec::new());
        let mut w = ZipWriter::new(buf);
        for (name, data, compression) in entries {
            write_entry(&mut w, name, data, *compression);
        }
        w.finish().unwrap().into_inner()
    }

    fn write_to_temp(bytes: &[u8]) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("in.epub");
        std::fs::write(&path, bytes).unwrap();
        (dir, path)
    }

    #[test]
    fn round_trip_preserves_mimetype_first_stored() {
        let bytes = build_epub(&[
            (
                MIMETYPE_ENTRY,
                MIMETYPE_CONTENT,
                zip::CompressionMethod::Stored,
            ),
            (
                "OEBPS/content.opf",
                b"<package/>",
                zip::CompressionMethod::Deflated,
            ),
        ]);
        let (dir, path) = write_to_temp(&bytes);
        let temp = with_modifications(&path, dir.path(), None, None, &HashMap::new(), &[]).unwrap();
        let out_bytes = std::fs::read(temp.path()).unwrap();
        let mut ar = ZipArchive::new(Cursor::new(&out_bytes[..])).unwrap();
        let first = ar.by_index(0).unwrap();
        assert_eq!(first.name(), MIMETYPE_ENTRY);
        assert_eq!(first.compression(), zip::CompressionMethod::Stored);
    }

    #[test]
    fn round_trip_preserves_per_entry_compression() {
        let bytes = build_epub(&[
            (
                MIMETYPE_ENTRY,
                MIMETYPE_CONTENT,
                zip::CompressionMethod::Stored,
            ),
            (
                "OEBPS/content.opf",
                b"<package/>",
                zip::CompressionMethod::Deflated,
            ),
            (
                "images/cover.jpg",
                &[0xff; 64],
                zip::CompressionMethod::Stored,
            ),
        ]);
        let (dir, path) = write_to_temp(&bytes);
        let temp = with_modifications(&path, dir.path(), None, None, &HashMap::new(), &[]).unwrap();
        let out_bytes = std::fs::read(temp.path()).unwrap();
        let mut ar = ZipArchive::new(Cursor::new(&out_bytes[..])).unwrap();
        for i in 0..ar.len() {
            let f = ar.by_index(i).unwrap();
            let expected = match f.name() {
                MIMETYPE_ENTRY => zip::CompressionMethod::Stored,
                "OEBPS/content.opf" => zip::CompressionMethod::Deflated,
                "images/cover.jpg" => zip::CompressionMethod::Stored,
                other => panic!("unexpected entry {other}"),
            };
            assert_eq!(
                f.compression(),
                expected,
                "entry {} compression mismatch",
                f.name()
            );
        }
    }

    #[test]
    fn replaces_opf_when_provided() {
        let bytes = build_epub(&[
            (
                MIMETYPE_ENTRY,
                MIMETYPE_CONTENT,
                zip::CompressionMethod::Stored,
            ),
            (
                "OEBPS/content.opf",
                br#"<package><metadata><dc:title>Old</dc:title></metadata></package>"#,
                zip::CompressionMethod::Deflated,
            ),
        ]);
        let (dir, path) = write_to_temp(&bytes);
        let replacement = br#"<package><metadata><dc:title>New</dc:title></metadata></package>"#;
        let temp = with_modifications(
            &path,
            dir.path(),
            Some("OEBPS/content.opf"),
            Some(replacement),
            &HashMap::new(),
            &[],
        )
        .unwrap();
        let out = std::fs::read(temp.path()).unwrap();
        let mut ar = ZipArchive::new(Cursor::new(&out[..])).unwrap();
        let mut s = String::new();
        ar.by_name("OEBPS/content.opf")
            .unwrap()
            .read_to_string(&mut s)
            .unwrap();
        assert!(s.contains("<dc:title>New</dc:title>"), "got: {s}");
    }

    #[test]
    fn replaces_arbitrary_binary_entry() {
        let bytes = build_epub(&[
            (
                MIMETYPE_ENTRY,
                MIMETYPE_CONTENT,
                zip::CompressionMethod::Stored,
            ),
            (
                "OEBPS/content.opf",
                b"<package/>",
                zip::CompressionMethod::Deflated,
            ),
            (
                "images/cover.jpg",
                b"OLD_BYTES",
                zip::CompressionMethod::Stored,
            ),
        ]);
        let (dir, path) = write_to_temp(&bytes);
        let mut replacements = HashMap::new();
        replacements.insert("images/cover.jpg".to_string(), b"NEW_BYTES".to_vec());
        let temp = with_modifications(&path, dir.path(), None, None, &replacements, &[]).unwrap();
        let out = std::fs::read(temp.path()).unwrap();
        let mut ar = ZipArchive::new(Cursor::new(&out[..])).unwrap();
        let mut buf = Vec::new();
        ar.by_name("images/cover.jpg")
            .unwrap()
            .read_to_end(&mut buf)
            .unwrap();
        assert_eq!(buf, b"NEW_BYTES");
    }

    #[test]
    fn appends_new_entry_via_additions() {
        let bytes = build_epub(&[
            (
                MIMETYPE_ENTRY,
                MIMETYPE_CONTENT,
                zip::CompressionMethod::Stored,
            ),
            (
                "OEBPS/content.opf",
                b"<package/>",
                zip::CompressionMethod::Deflated,
            ),
        ]);
        let (dir, path) = write_to_temp(&bytes);
        let additions = vec![(
            "META-INF/container.xml".to_string(),
            b"<container/>".to_vec(),
            {
                let opts: FileOptions<ExtendedFileOptions> =
                    FileOptions::default().compression_method(zip::CompressionMethod::Deflated);
                opts
            },
        )];
        let temp =
            with_modifications(&path, dir.path(), None, None, &HashMap::new(), &additions).unwrap();
        let out = std::fs::read(temp.path()).unwrap();
        let mut ar = ZipArchive::new(Cursor::new(&out[..])).unwrap();
        let mut buf = Vec::new();
        ar.by_name("META-INF/container.xml")
            .unwrap()
            .read_to_end(&mut buf)
            .unwrap();
        assert_eq!(buf, b"<container/>");
    }
}
