//! Candidate publication and file-backed ZIP repack for EPUB mutations.
//!
//! Untouched entries retain their compressed payload and metadata. The maintained
//! replacement operation validates and hashes a finished candidate, then syncs,
//! replaces and syncs its opened parent.

use super::{EpubError, Severity, ValidationReport, repair::RepairPlan, zip_layer::ZipHandle};
use cap_std::fs::Dir;
use cap_std_ext::dirext::CapStdExtDirExt;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::hash::BuildHasher;
use std::io::{Read, Seek, Write};
use zip::write::{ExtendedFileOptions, FileOptions};
use zip::{ZipArchive, ZipWriter};

pub(super) const MIMETYPE_ENTRY: &str = "mimetype";
pub(super) const MIMETYPE_CONTENT: &[u8] = b"application/epub+zip";

/// Evidence derived from the finished, accepted candidate.
pub struct Published {
    /// Pure validation of the final bytes.
    pub report: ValidationReport,
    /// SHA-256 of the final archive.
    pub hash: String,
    /// Length from the candidate handle.
    pub size: u64,
}

/// Stream a seekable file's SHA-256 from its beginning.
///
/// # Errors
/// Returns seek or read errors.
pub fn hash_file(file: &mut std::fs::File) -> std::io::Result<String> {
    file.rewind()?;
    let mut buffer = vec![0; 64 * 1024];
    let mut hasher = Sha256::new();
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    let mut hash = String::with_capacity(64);
    for byte in hasher.finalize() {
        use std::fmt::Write;
        write!(hash, "{byte:02x}").map_err(std::io::Error::other)?;
    }
    Ok(hash)
}

fn remaining_severity(report: &ValidationReport) -> u8 {
    if report
        .issues
        .iter()
        .any(|issue| issue.severity == Severity::Irrecoverable)
    {
        2
    } else {
        u8::from(
            report
                .issues
                .iter()
                .any(|issue| issue.severity == Severity::Degraded),
        )
    }
}

/// Build, validate and publish one candidate beneath its opened parent.
///
/// # Errors
/// Candidate errors leave the source untouched. An error after acceptance reports publication uncertainty.
pub fn publish(
    parent: &Dir,
    basename: &std::ffi::OsStr,
    source: &ValidationReport,
    build: impl FnOnce(&mut cap_std::fs::File) -> Result<(), EpubError>,
) -> Result<Published, EpubError> {
    publish_with_validator(parent, basename, source, build, super::validate)
}

pub(super) fn publish_with_validator(
    parent: &Dir,
    basename: &std::ffi::OsStr,
    source: &ValidationReport,
    build: impl FnOnce(&mut cap_std::fs::File) -> Result<(), EpubError>,
    validate: impl FnOnce(std::fs::File) -> Result<ValidationReport, EpubError>,
) -> Result<Published, EpubError> {
    let mut accepted_hash = None;
    let result =
        parent.atomic_replace_with(basename, |candidate| -> Result<Published, EpubError> {
            build(candidate.get_mut().as_file_mut())?;
            candidate.flush()?;
            let mut file = candidate.get_ref().as_file().try_clone()?.into_std();
            let report = validate(file.try_clone()?)?;
            if remaining_severity(&report) == 2
                || remaining_severity(&report) > remaining_severity(source)
                || report
                    .issues
                    .iter()
                    .any(|issue| issue.severity == Severity::Repaired)
            {
                return Err(EpubError::CandidateRejected(format!(
                    "source={:?} candidate={:?}",
                    source.outcome, report.outcome
                )));
            }
            let hash = hash_file(&mut file)?;
            let size = file.metadata()?.len();
            accepted_hash = Some(hash.clone());
            Ok(Published { report, hash, size })
        });
    match (result, accepted_hash) {
        (Ok(published), _) => Ok(published),
        (Err(error), Some(hash)) => Err(EpubError::PublicationUncertain {
            hash,
            error: Box::new(error),
        }),
        (Err(error), None) => Err(error),
    }
}

/// Repack an admitted archive into the caller's random-access candidate.
///
/// # Errors
/// Returns source read, repair or ZIP writing errors before publication.
pub fn with_modifications<S: BuildHasher>(
    source: &ZipHandle,
    candidate: &mut (impl Write + Seek),
    opf_path: Option<&str>,
    opf_replacement: Option<&[u8]>,
    binary_replacements: &HashMap<String, Vec<u8>, S>,
    additions: &[(String, Vec<u8>, FileOptions<ExtendedFileOptions>)],
    repairs: &RepairPlan,
) -> Result<(), EpubError> {
    let mut file = source.file()?;
    file.rewind()?;
    let mut archive = ZipArchive::new(file)?;
    let mut writer = ZipWriter::new(candidate);
    let stored = FileOptions::<ExtendedFileOptions>::default()
        .compression_method(zip::CompressionMethod::Stored);
    writer.start_file(MIMETYPE_ENTRY, stored)?;
    writer.write_all(MIMETYPE_CONTENT)?;
    for i in 0..archive.len() {
        let file = archive.by_index(i)?;
        let name = file.name().to_owned();
        if name == MIMETYPE_ENTRY {
            continue;
        }
        let replacement = if opf_path == Some(name.as_str()) {
            opf_replacement.map(<[u8]>::to_vec)
        } else {
            None
        };
        let replacement = match replacement.or_else(|| binary_replacements.get(&name).cloned()) {
            Some(bytes) => Some(bytes),
            None => repairs.replacement(source, &name)?,
        };
        if let Some(bytes) = replacement {
            let options = FileOptions::<ExtendedFileOptions>::default()
                .compression_method(file.compression());
            writer.start_file(&name, options)?;
            writer.write_all(&bytes)?;
        } else {
            writer.raw_copy_file(file)?;
        }
    }
    if let Some(container) = repairs.container_addition(source) {
        writer.start_file(
            "META-INF/container.xml",
            FileOptions::<ExtendedFileOptions>::default(),
        )?;
        writer.write_all(&container)?;
    }
    for (name, bytes, options) in additions {
        writer.start_file(name, options.clone())?;
        writer.write_all(bytes)?;
    }
    writer.finish()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Read};
    use std::path::Path;
    use tempfile::NamedTempFile;

    fn with_modifications<S: BuildHasher>(
        path: &Path,
        dest: &Path,
        opf: Option<&str>,
        bytes: Option<&[u8]>,
        replacements: &HashMap<String, Vec<u8>, S>,
        additions: &[(String, Vec<u8>, FileOptions<ExtendedFileOptions>)],
    ) -> Result<NamedTempFile, EpubError> {
        let handle =
            super::super::zip_layer::validate(std::fs::File::open(path)?, &mut Vec::new())?;
        let mut candidate = NamedTempFile::new_in(dest)?;
        super::with_modifications(
            &handle,
            candidate.as_file_mut(),
            opf,
            bytes,
            replacements,
            additions,
            &RepairPlan::default(),
        )?;
        Ok(candidate)
    }

    #[test]
    fn candidate_publication_maintained_handle_qualification() {
        use cap_std_ext::dirext::CapStdExtDirExt;
        use sha2::{Digest, Sha256};
        use std::io::{Seek, SeekFrom};

        let tmp = tempfile::tempdir().unwrap();
        let parent =
            cap_std::fs::Dir::open_ambient_dir(tmp.path(), cap_std::ambient_authority()).unwrap();
        for content in [b"creation".as_slice(), b"replacement".as_slice()] {
            let mut accepted = None;
            parent
                .atomic_replace_with("book.epub", |candidate| -> std::io::Result<()> {
                    let mut writer = ZipWriter::new(candidate.get_mut().as_file_mut());
                    writer.start_file(
                        MIMETYPE_ENTRY,
                        FileOptions::<ExtendedFileOptions>::default()
                            .compression_method(zip::CompressionMethod::Stored),
                    )?;
                    writer.write_all(MIMETYPE_CONTENT)?;
                    writer
                        .start_file("chapter.txt", FileOptions::<ExtendedFileOptions>::default())?;
                    writer.write_all(content)?;
                    writer.finish()?;
                    candidate.flush()?;
                    let mut file = candidate.get_ref().as_file().try_clone()?.into_std();
                    let mut buffer = vec![0; rawzip::RECOMMENDED_BUFFER_SIZE];
                    let archive =
                        rawzip::ZipArchive::with_max_search_space(MAX_EOCD_SEARCH_SPACE_FOR_TEST)
                            .locate_in_file(file.try_clone()?, &mut buffer)
                            .map_err(|(_, e)| std::io::Error::other(e))?;
                    assert_eq!(archive.entries_hint(), 2);
                    file.seek(SeekFrom::Start(0))?;
                    let mut hasher = Sha256::new();
                    let mut hash_buffer = vec![0; 64 * 1024];
                    loop {
                        let n = file.read(&mut hash_buffer)?;
                        if n == 0 {
                            break;
                        }
                        hasher.update(&hash_buffer[..n]);
                    }
                    accepted = Some((hasher.finalize().to_vec(), file.metadata()?.len()));
                    Ok(())
                })
                .unwrap();
            let persisted = parent.read("book.epub").unwrap();
            assert_eq!(
                accepted.unwrap(),
                (Sha256::digest(&persisted).to_vec(), persisted.len() as u64)
            );
            let mut archive = ZipArchive::new(Cursor::new(persisted)).unwrap();
            let mut actual = Vec::new();
            archive
                .by_name("chapter.txt")
                .unwrap()
                .read_to_end(&mut actual)
                .unwrap();
            assert_eq!(actual, content);
        }
    }

    const MAX_EOCD_SEARCH_SPACE_FOR_TEST: u64 = 22 + 65_535;

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
    fn binary_replacement_keeps_stored_compression() {
        let bytes = build_epub(&[
            (
                MIMETYPE_ENTRY,
                MIMETYPE_CONTENT,
                zip::CompressionMethod::Stored,
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
        let f = ar.by_name("images/cover.jpg").unwrap();
        assert_eq!(f.compression(), zip::CompressionMethod::Stored);
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
    fn file_backed_epub_untouched_entries_are_copied_verbatim() {
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
        assert_eq!(buf, Vec::<u8>::new());
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
        crate::services::epub::zip_layer::validate(
            std::fs::File::open(&out_path).unwrap(),
            &mut issues,
        )
        .unwrap();
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
