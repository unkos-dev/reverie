//! Shared `ZIP` repack helper for `EPUB` mutations.
//!
//! Preserves the `EPUB` spec's mimetype-first / stored constraint, copies
//! every untouched entry through with its compressed bytes and metadata
//! intact, and offers three mutation knobs: `OPF` replacement, arbitrary
//! binary-entry replacement (e.g. cover image), and new-entry additions
//! (e.g. regenerated `container.xml` or a freshly-inserted cover manifest
//! target).
//!
//! Callers are responsible for the final atomic rename of the returned
//! `NamedTempFile` onto the destination path.

use std::collections::HashMap;
use std::hash::BuildHasher;
use std::io::Write;
use std::path::Path;

use tempfile::NamedTempFile;
use zip::write::{ExtendedFileOptions, FileOptions};
use zip::{ZipArchive, ZipWriter};

use super::EpubError;

pub(super) const MIMETYPE_ENTRY: &str = "mimetype";
pub(super) const MIMETYPE_CONTENT: &[u8] = b"application/epub+zip";

/// Re-package the `EPUB` at `src_path` applying the provided mutations.
///
/// Writes to a fresh `NamedTempFile` in `dest_dir` (so the caller can
/// persist to a different directory on path-rename, or back to `src_path`'s
/// directory for in-place updates).  The caller owns the atomic rename.
///
/// - `opf_path` + `opf_replacement`: when both are Some, the `ZIP` entry whose
///   name equals `opf_path` is replaced with `opf_replacement` bytes.
/// - `binary_replacements`: entry-name → bytes overrides for any non-`OPF`
///   entry (e.g. a cover image). Entries in this map REPLACE existing
///   entries; they do not add new ones.
/// - `additions`: new `ZIP` entries to append after all existing entries have
///   been copied.  Use this for entries absent from the source (e.g. a
///   regenerated `META-INF/container.xml` or a freshly-inserted cover
///   manifest target).
///
/// Untouched entries are copied with their compressed bytes and metadata intact.
///
/// # Errors
///
/// Returns [`EpubError::Io`] if `src_path` cannot be read or if the temp file
/// cannot be created in `dest_dir`. Returns [`EpubError::Zip`] if
/// `ZipArchive::new` fails to parse the source archive or if `ZipWriter`
/// encounters an error while writing an entry.
pub fn with_modifications<S: BuildHasher>(
    src_path: &Path,
    dest_dir: &Path,
    opf_path: Option<&str>,
    opf_replacement: Option<&[u8]>,
    binary_replacements: &HashMap<String, Vec<u8>, S>,
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

            if opf_path == Some(name.as_str())
                && let Some(repl) = opf_replacement
            {
                writer.start_file(&name, FileOptions::<ExtendedFileOptions>::default())?;
                writer.write_all(repl)?;
            } else if let Some(replacement) = binary_replacements.get(&name) {
                writer.start_file(&name, FileOptions::<ExtendedFileOptions>::default())?;
                writer.write_all(replacement)?;
            } else {
                writer.raw_copy_file(file)?;
            }
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
    use std::io::{Cursor, Read};

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
                MIMETYPE_ENTRY | "images/cover.jpg" => zip::CompressionMethod::Stored,
                "OEBPS/content.opf" => zip::CompressionMethod::Deflated,
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
                br"<package><metadata><dc:title>Old</dc:title></metadata></package>",
                zip::CompressionMethod::Deflated,
            ),
        ]);
        let (dir, path) = write_to_temp(&bytes);
        let replacement = br"<package><metadata><dc:title>New</dc:title></metadata></package>";
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

    #[test]
    fn untouched_entries_are_copied_verbatim() {
        // A historical timestamp and a non-default compression level, distinct
        // from what a fresh `start_file` write would produce, so this test
        // cannot pass by both sides coincidentally landing on the same "now"
        // stamp or the same deflate output.
        let historical = zip::DateTime::from_date_and_time(2001, 2, 3, 4, 5, 6).unwrap();
        let compressible = "wonderful compressible words ".repeat(300);
        let stored_bytes = vec![0x42u8; 300];

        let mimetype_opts: FileOptions<ExtendedFileOptions> =
            FileOptions::default().compression_method(zip::CompressionMethod::Stored);
        let opf_opts: FileOptions<ExtendedFileOptions> = FileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .compression_level(Some(1))
            .last_modified_time(historical);
        let cover_opts: FileOptions<ExtendedFileOptions> = FileOptions::default()
            .compression_method(zip::CompressionMethod::Stored)
            .last_modified_time(historical);

        let mut w = ZipWriter::new(Cursor::new(Vec::new()));
        w.start_file(MIMETYPE_ENTRY, mimetype_opts).unwrap();
        w.write_all(MIMETYPE_CONTENT).unwrap();
        w.start_file("OEBPS/content.opf", opf_opts).unwrap();
        w.write_all(compressible.as_bytes()).unwrap();
        w.start_file("images/cover.jpg", cover_opts).unwrap();
        w.write_all(&stored_bytes).unwrap();
        let bytes = w.finish().unwrap().into_inner();

        let (dir, path) = write_to_temp(&bytes);
        let temp = with_modifications(&path, dir.path(), None, None, &HashMap::new(), &[]).unwrap();
        let out_bytes = std::fs::read(temp.path()).unwrap();

        let mut src_ar = ZipArchive::new(Cursor::new(bytes.as_slice())).unwrap();
        let mut out_ar = ZipArchive::new(Cursor::new(out_bytes.as_slice())).unwrap();

        for (name, expected_compression) in [
            ("OEBPS/content.opf", zip::CompressionMethod::Deflated),
            ("images/cover.jpg", zip::CompressionMethod::Stored),
        ] {
            let src_index = src_ar.index_for_name(name).unwrap();
            let out_index = out_ar.index_for_name(name).unwrap();

            let (src_compression, src_crc, src_modified) = {
                let f = src_ar.by_index(src_index).unwrap();
                (f.compression(), f.crc32(), f.last_modified())
            };
            // Guards against a Stored-only fixture passing vacuously: the
            // Deflated entry must still be Deflated before its bytes are compared.
            assert_eq!(
                src_compression, expected_compression,
                "fixture built with unexpected compression for {name}"
            );

            let (out_compression, out_crc, out_modified) = {
                let f = out_ar.by_index(out_index).unwrap();
                (f.compression(), f.crc32(), f.last_modified())
            };
            assert_eq!(
                out_compression, src_compression,
                "{name} compression changed"
            );
            assert_eq!(out_crc, src_crc, "{name} crc32 changed");
            assert_eq!(out_modified, src_modified, "{name} last_modified changed");

            let mut src_raw = Vec::new();
            src_ar
                .by_index_raw(src_index)
                .unwrap()
                .read_to_end(&mut src_raw)
                .unwrap();
            let mut out_raw = Vec::new();
            out_ar
                .by_index_raw(out_index)
                .unwrap()
                .read_to_end(&mut out_raw)
                .unwrap();
            assert_eq!(out_raw, src_raw, "{name} compressed bytes changed");
        }
    }

    #[test]
    fn binary_replacement_rewrites_entry_present_in_source() {
        let original = "original original original ".repeat(500);
        let bytes = build_epub(&[
            (
                MIMETYPE_ENTRY,
                MIMETYPE_CONTENT,
                zip::CompressionMethod::Stored,
            ),
            (
                "images/cover.jpg",
                original.as_bytes(),
                zip::CompressionMethod::Deflated,
            ),
        ]);
        let (dir, path) = write_to_temp(&bytes);
        let replacement = b"replacement bytes, not the original payload".to_vec();
        let mut replacements = HashMap::new();
        replacements.insert("images/cover.jpg".to_string(), replacement.clone());
        let temp = with_modifications(&path, dir.path(), None, None, &replacements, &[]).unwrap();
        let out_bytes = std::fs::read(temp.path()).unwrap();

        let mut out_ar = ZipArchive::new(Cursor::new(out_bytes.as_slice())).unwrap();
        let mut buf = Vec::new();
        out_ar
            .by_name("images/cover.jpg")
            .unwrap()
            .read_to_end(&mut buf)
            .unwrap();
        assert_eq!(buf, replacement);

        let mut src_ar = ZipArchive::new(Cursor::new(bytes.as_slice())).unwrap();
        let src_index = src_ar.index_for_name("images/cover.jpg").unwrap();
        let mut src_raw = Vec::new();
        src_ar
            .by_index_raw(src_index)
            .unwrap()
            .read_to_end(&mut src_raw)
            .unwrap();
        let out_index = out_ar.index_for_name("images/cover.jpg").unwrap();
        let mut out_raw = Vec::new();
        out_ar
            .by_index_raw(out_index)
            .unwrap()
            .read_to_end(&mut out_raw)
            .unwrap();
        assert_ne!(
            out_raw, src_raw,
            "a replaced entry must not be a raw copy of the source"
        );
    }

    #[test]
    fn zero_length_stored_entry_copies_through() {
        let bytes = build_epub(&[
            (
                MIMETYPE_ENTRY,
                MIMETYPE_CONTENT,
                zip::CompressionMethod::Stored,
            ),
            ("OEBPS/empty.txt", b"", zip::CompressionMethod::Stored),
        ]);
        let (dir, path) = write_to_temp(&bytes);
        let temp = with_modifications(&path, dir.path(), None, None, &HashMap::new(), &[]).unwrap();
        let out_bytes = std::fs::read(temp.path()).unwrap();
        let mut ar = ZipArchive::new(Cursor::new(out_bytes.as_slice())).unwrap();
        let mut f = ar.by_name("OEBPS/empty.txt").unwrap();
        assert_eq!(f.compression(), zip::CompressionMethod::Stored);
        assert_eq!(f.size(), 0);
        let mut buf = Vec::new();
        f.read_to_end(&mut buf).unwrap();
        assert!(buf.is_empty());
    }

    #[test]
    fn data_descriptor_source_repacks_clean_and_reads_back_identical() {
        // A non-seekable sink forces zip to write a trailing data descriptor
        // (bit 3 of the general-purpose flag) rather than back-filling sizes
        // into the local header, so raw_copy_file must carry sizes forward
        // itself rather than trusting the header it read.
        let mut w = zip::ZipWriter::new_stream(Vec::new());
        let mimetype_opts: FileOptions<ExtendedFileOptions> =
            FileOptions::default().compression_method(zip::CompressionMethod::Stored);
        w.start_file(MIMETYPE_ENTRY, mimetype_opts).unwrap();
        w.write_all(MIMETYPE_CONTENT).unwrap();

        let content = "streamed content streamed content ".repeat(200);
        let opf_opts: FileOptions<ExtendedFileOptions> = FileOptions::default();
        w.start_file("OEBPS/content.opf", opf_opts).unwrap();
        w.write_all(content.as_bytes()).unwrap();

        let bytes = w.finish().unwrap().into_inner();

        // Confirm the fixture actually exercises a data descriptor rather
        // than passing vacuously on a seekable-equivalent archive: bit 3 of
        // the first local file header's general-purpose flag (offset 6).
        assert_eq!(&bytes[0..4], 0x0403_4b50u32.to_le_bytes().as_slice());
        let general_purpose_flag = u16::from_le_bytes([bytes[6], bytes[7]]);
        assert_ne!(
            general_purpose_flag & 0x0008,
            0,
            "fixture must use a data descriptor"
        );

        let (dir, path) = write_to_temp(&bytes);
        let temp = with_modifications(&path, dir.path(), None, None, &HashMap::new(), &[]).unwrap();
        let out_path = temp.path().to_path_buf();

        let mut issues = Vec::new();
        crate::services::epub::zip_layer::validate(&out_path, &mut issues).unwrap();
        assert!(
            issues.is_empty(),
            "repacked archive should validate clean: {issues:?}"
        );

        let out_bytes = std::fs::read(&out_path).unwrap();
        let mut ar = ZipArchive::new(Cursor::new(out_bytes.as_slice())).unwrap();
        let mut buf = Vec::new();
        ar.by_name("OEBPS/content.opf")
            .unwrap()
            .read_to_end(&mut buf)
            .unwrap();
        assert_eq!(buf, content.as_bytes());
    }
}
