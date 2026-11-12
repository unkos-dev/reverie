//! Candidate publication and file-backed ZIP repack for EPUB mutations.
//!
//! Untouched entries retain their names, compression and compressed payload. The maintained
//! replacement operation validates and hashes a finished candidate, then syncs,
//! replaces and syncs its opened parent.

use super::{EpubError, Severity, ValidationReport, repair::RepairPlan, zip_layer::ZipHandle};
use cap_std::fs::Dir;
use cap_std_ext::dirext::CapStdExtDirExt;
use rawzip::path::{EntryPath, ZipFilePath};
use rawzip::{CompressionMethod, DataDescriptorOutput, ZipArchive, ZipArchiveWriter};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::hash::BuildHasher;
use std::io::{Read, Seek, Write};

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
                return Err(EpubError::CandidateRejected(
                    format!("source={:?} candidate={:?}", source.outcome, report.outcome),
                    report.issues,
                ));
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
    additions: &[(String, Vec<u8>, CompressionMethod)],
    repairs: &RepairPlan,
) -> Result<(), EpubError> {
    let mut buffer = vec![0; rawzip::RECOMMENDED_BUFFER_SIZE];
    let archive = ZipArchive::from_file(source.file()?, &mut buffer)?;
    let mut writer = ZipArchiveWriter::new(candidate);
    write_entry(
        &mut writer,
        MIMETYPE_ENTRY,
        MIMETYPE_CONTENT,
        CompressionMethod::STORE,
    )?;
    let mut entries = archive.entries(&mut buffer);
    while let Some(entry) = entries.next_entry()? {
        let path = entry.file_path();
        let name = std::str::from_utf8(path.as_ref())
            .map_err(|error| rawzip::Error::from(rawzip::ErrorKind::InvalidUtf8(error)))?;
        if name == MIMETYPE_ENTRY {
            continue;
        }
        let replacement = if opf_path == Some(name) {
            opf_replacement.map(<[u8]>::to_vec)
        } else {
            None
        };
        let replacement = match replacement.or_else(|| binary_replacements.get(name).cloned()) {
            Some(bytes) => Some(bytes),
            None => repairs.replacement(source, name)?,
        };
        if let Some(bytes) = replacement {
            write_entry(&mut writer, name, &bytes, entry.compression_method())?;
        } else if entry.is_dir() {
            writer.new_dir(entry_path(name)?).create()?;
        } else {
            let source_entry = archive.get_entry(entry.wayfinder())?;
            let (mut output, _) = writer
                .new_file(entry_path(name)?)
                .compression_method(entry.compression_method())
                .start()?;
            std::io::copy(&mut source_entry.reader(), &mut output)?;
            output.finish(DataDescriptorOutput::new(
                entry.crc32(),
                entry.uncompressed_size_hint(),
            ))?;
        }
    }
    if let Some(container) = repairs.container_addition(source) {
        write_entry(
            &mut writer,
            "META-INF/container.xml",
            &container,
            CompressionMethod::DEFLATE,
        )?;
    }
    for (name, bytes, compression) in additions {
        write_entry(&mut writer, name, bytes, *compression)?;
    }
    writer.finish()?;
    Ok(())
}

fn entry_path(name: &str) -> Result<EntryPath<'_>, rawzip::Error> {
    let path = ZipFilePath::from_str(name);
    if path.as_str() != name {
        return Err(rawzip::ErrorKind::InvalidInput {
            msg: "repack would change an archive entry name".into(),
        }
        .into());
    }
    Ok(path.into())
}

fn write_entry<W: Write>(
    writer: &mut ZipArchiveWriter<W>,
    name: &str,
    bytes: &[u8],
    compression: CompressionMethod,
) -> Result<(), EpubError> {
    let (mut entry, config) = writer
        .new_file(entry_path(name)?)
        .compression_method(compression)
        .start()?;
    let descriptor = if compression == CompressionMethod::DEFLATE {
        let encoder =
            flate2::write::DeflateEncoder::new(&mut entry, flate2::Compression::default());
        let mut data = config.wrap(encoder);
        data.write_all(bytes)?;
        let (encoder, descriptor) = data.finish()?;
        encoder.finish()?;
        descriptor
    } else {
        let mut data = config.wrap(&mut entry);
        data.write_all(bytes)?;
        data.finish()?.1
    };
    entry.finish(descriptor)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use tempfile::NamedTempFile;

    fn with_modifications<S: BuildHasher>(
        path: &Path,
        dest: &Path,
        opf: Option<&str>,
        bytes: Option<&[u8]>,
        replacements: &HashMap<String, Vec<u8>, S>,
        additions: &[(String, Vec<u8>, CompressionMethod)],
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

    fn write_to_temp(bytes: &[u8]) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("in.epub");
        std::fs::write(&path, bytes).unwrap();
        (dir, path)
    }

    #[test]
    fn file_backed_epub_untouched_entries_are_copied_verbatim() {
        let content = "wonderful compressible words ".repeat(300);
        let stored = vec![0x42u8; 300];
        let bytes = crate::test_support::zip_fixture(&[
            (MIMETYPE_ENTRY, MIMETYPE_CONTENT, CompressionMethod::STORE),
            (
                "OEBPS/content.opf",
                content.as_bytes(),
                CompressionMethod::DEFLATE,
            ),
            ("images/couverture-é.jpg", &stored, CompressionMethod::STORE),
        ]);
        let (dir, path) = write_to_temp(&bytes);
        let temp = with_modifications(&path, dir.path(), None, None, &HashMap::new(), &[]).unwrap();
        let out_bytes = std::fs::read(temp.path()).unwrap();
        let source = ZipArchive::from_slice(&bytes).unwrap();
        let output = ZipArchive::from_slice(&out_bytes).unwrap();
        let mut sources = source.entries();
        let mut outputs = output.entries();
        while let Some(entry) = sources.next_entry().unwrap() {
            let expected = if entry.file_path().as_ref() == b"OEBPS/content.opf" {
                CompressionMethod::DEFLATE
            } else {
                CompressionMethod::STORE
            };
            assert_eq!(entry.compression_method(), expected);
            let copied = outputs.next_entry().unwrap().unwrap();
            assert_eq!(entry.file_path(), copied.file_path());
            assert_eq!(entry.flags().is_utf8(), copied.flags().is_utf8());
            assert_eq!(entry.compression_method(), copied.compression_method());
            assert_eq!(entry.crc32(), copied.crc32());
            assert_eq!(
                entry.uncompressed_size_hint(),
                copied.uncompressed_size_hint()
            );
            assert_eq!(
                source.get_entry(entry.wayfinder()).unwrap().data(),
                output.get_entry(copied.wayfinder()).unwrap().data()
            );
        }
        assert!(outputs.next_entry().unwrap().is_none());
    }
}
