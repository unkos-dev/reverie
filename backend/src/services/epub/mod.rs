//! `EPUB` structural validation and auto-repair pipeline.
//!
//! Pure opened-file inspection runs five layers: ZIP, container, OPF, XHTML and cover.
//! Repair instructions and metadata changes compose into one candidate. Publication
//! validates and hashes the finished archive before replacing the source.

use std::fs::File;
#[cfg(test)]
use std::path::Path;

/// `META-INF/container.xml` parsing and `OPF` path location (Layer 2).
pub mod container_layer;
/// Cover-image decodability validation — `JPEG`/`PNG` only (Layer 5).
pub mod cover_layer;
/// `OPF` manifest and spine parsing, Dublin Core metadata extraction (Layer 3).
pub mod opf_layer;
/// Low-level `ZIP` repack helper used by the repair layer.
pub mod repack;
/// Entry repair instructions applied during candidate repack.
pub mod repair;
/// `XHTML` spine-document encoding and well-formedness checks (Layer 4).
pub mod xhtml_layer;
/// `ZIP` archive reading and the [`zip_layer::ZipHandle`] type (Layer 1 backing store).
pub mod zip_layer;

// ── Error type ───────────────────────────────────────────────────────────────

/// Fatal errors that abort the `EPUB` validation pipeline.
///
/// Layer-level structural problems (corrupt entries, path traversal, etc.) are
/// represented as [`IssueKind`] variants, not as `EpubError`s — only
/// unrecoverable I/O or `ZIP` machinery failures reach this type.
#[derive(Debug, thiserror::Error)]
pub enum EpubError {
    /// Raised only by the repack and repair paths; Layer 1 reports a corrupt archive as an `Irrecoverable` issue.
    #[error("ZIP I/O error: {0}")]
    Zip(#[from] zip::result::ZipError),
    /// Filesystem I/O error reading or writing the archive.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    /// `quick_xml` parse error surfaced during repack `XML` rewriting.
    #[error("XML parse error: {0}")]
    Xml(#[from] quick_xml::Error),
    /// Candidate validation rejected the rewrite before publication.
    #[error("candidate validation regressed: {0}")]
    CandidateRejected(String),
    /// A required repair could not produce its promised entry.
    #[error("required repair failed: {0}")]
    Repair(String),
    /// Replacement returned an error after the candidate was accepted.
    #[error("publication or durability uncertain for accepted hash {hash}: {error}")]
    PublicationUncertain {
        /// Hash of the accepted candidate.
        hash: String,
        /// Error returned by the maintained replacement operation.
        error: Box<Self>,
    },
}

impl EpubError {
    /// Whether the error is a verdict on the file's content rather than a fault
    /// in Reverie's own storage or publication.
    #[must_use]
    pub const fn is_file_defect(&self) -> bool {
        match self {
            Self::CandidateRejected(_) | Self::Repair(_) | Self::Xml(_) => true,
            Self::Zip(error) => !matches!(error, zip::result::ZipError::Io(_)),
            Self::Io(_) | Self::PublicationUncertain { .. } => false,
        }
    }
}

// ── Issue types ───────────────────────────────────────────────────────────────

/// The pipeline layer that detected an `Issue`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Layer {
    /// `ZIP` archive integrity layer.
    Zip,
    /// `META-INF/container.xml` parsing layer.
    Container,
    /// `OPF` package document parsing layer.
    Opf,
    /// `XHTML` spine-document validation layer.
    Xhtml,
    /// Cover-image decodability layer.
    Cover,
}

/// How serious an `Issue` is and whether it has been resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Severity {
    /// File cannot be used; ingestion preserves the original with a rejection reason.
    Irrecoverable,
    /// Issue was automatically repaired.
    Repaired,
    /// Issue present but file is still usable; stored as-is.
    Degraded,
}

/// Which OCF container rule the `mimetype` entry breaks.
#[derive(Debug, Clone)]
pub enum MimetypeProblem {
    /// No entry named `mimetype` exists.
    Missing,
    /// The entry exists but its local header is not the archive's first bytes.
    NotFirst,
    /// The entry's local header declares a method other than Stored.
    Compressed,
    /// The entry's local header carries an extra field.
    ExtraField,
    /// The content is not exactly `application/epub+zip`.
    Content,
}

/// Repair-relevant context for each issue kind.
/// Each variant carries the data needed to apply the corresponding fix.
#[derive(Debug, Clone)]
pub enum IssueKind {
    /// `ZIP` entry contains path traversal components or absolute path.
    PathTraversal {
        /// Offending entry name as recorded in the `ZIP` central directory.
        entry_name: String,
    },
    /// `ZIP` entry or aggregate uncompressed size exceeds limit.
    ZipBomb {
        /// Offending entry name (or aggregate sentinel).
        entry_name: String,
        /// Observed uncompressed size in bytes.
        size: u64,
        /// Configured limit that was exceeded.
        limit: u64,
    },
    /// `ZIP` central directory declares more entries than the archive cap.
    EntryCapExceeded {
        /// Declared entry count.
        count: u64,
        /// Configured limit that was exceeded.
        limit: usize,
    },
    /// Archive file size exceeds the cap, so it is never read.
    ArchiveTooLarge {
        /// Observed file size in bytes.
        size: u64,
        /// Configured limit that was exceeded.
        limit: u64,
    },
    /// `ZIP` entry is unreadable (corrupt data).
    CorruptEntry {
        /// Offending entry name.
        entry_name: String,
    },
    /// Data precedes the start of the `ZIP` archive within the file.
    PreludeBeforeArchive {
        /// Length of the prelude in bytes.
        bytes: u64,
    },
    /// Two `ZIP` central-directory entries declare the same name.
    DuplicateEntry {
        /// The name shared by more than one entry.
        entry_name: String,
    },
    /// `ZIP` entry uses a compression method other than Stored or Deflate.
    UnsupportedCompression {
        /// Offending entry name.
        entry_name: String,
        /// Raw `ZIP` compression method identifier.
        method: u16,
    },
    /// `ZIP` entry is encrypted; `OCF` containers must not use `ZIP` encryption.
    EncryptedEntry {
        /// Offending entry name.
        entry_name: String,
    },
    /// `OCF` `mimetype` entry breaks a container rule; repack rewrites it.
    InvalidMimetype {
        /// The rule broken.
        problem: MimetypeProblem,
    },
    /// `META-INF/container.xml` absent; `OPF` path provided if regeneratable.
    MissingContainer {
        /// Best-guess `OPF` path that the repair pass might use; `None` when no
        /// candidate could be inferred.
        opf_candidate: Option<String>,
    },
    /// `OPF` path extracted from `container.xml` fails path-safety check.
    UnsafeOpfPath {
        /// Offending `OPF` path string.
        path: String,
    },
    /// Spine entry references an item not in the manifest.
    BrokenSpineRef {
        /// Spine `idref` value with no matching manifest entry.
        idref: String,
    },
    /// Manifest href fails path-safety check.
    UnsafeManifestHref {
        /// Offending href value.
        href: String,
    },
    /// `EPUB` has more spine items than the 500-item cap.
    SpineCapExceeded {
        /// Observed spine item count.
        count: usize,
    },
    /// `XML` file declared/detected encoding mismatch, was transcoded.
    EncodingMismatch {
        /// Offending entry name.
        entry_name: String,
        /// Encoding the file declared in its prologue or meta tag.
        declared: String,
        /// Always `"UTF-8"`: the check parses as UTF-8 and performs no detection.
        detected: String,
    },
    /// `XML` file has ambiguous encoding (conditions for safe transcode not met).
    AmbiguousEncoding {
        /// Offending entry name.
        entry_name: String,
    },
    /// `XML` parse error in a spine document.
    MalformedXhtml {
        /// Offending entry name.
        entry_name: String,
        /// Parser error detail (one-line summary).
        detail: String,
    },
    /// Cover file referenced in `OPF` does not exist in the archive.
    MissingCover {
        /// Manifest href that resolved to no archive entry.
        href: String,
    },
    /// Cover file exists but is not a decodable `JPEG` or `PNG`.
    UndecodableCover {
        /// Manifest href whose bytes failed image decode.
        href: String,
    },
}

/// A single validation finding produced by one pipeline layer.
#[derive(Debug, Clone)]
pub struct Issue {
    /// The pipeline layer that detected this issue.
    pub layer: Layer,
    /// How severe the issue is and whether it was repaired.
    pub severity: Severity,
    /// Structured context describing the specific problem.
    pub kind: IssueKind,
}

/// Overall validation outcome. Determines how the ingestion pipeline handles the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationOutcome {
    /// All layers passed with no issues.
    Clean,
    /// One or more issues were automatically repaired; re-packaged `ZIP` is valid.
    Repaired,
    /// One or more non-critical issues; file usable but not fully conformant.
    Degraded,
    /// Irrecoverable issue; file must be quarantined.
    Quarantined,
}

/// Complete output of `validate_and_repair`: all issues found and the overall disposition.
#[derive(Debug, Clone)]
pub struct ValidationReport {
    /// All issues found across all pipeline layers, in discovery order.
    pub issues: Vec<Issue>,
    /// Overall disposition of the file after all layers have run.
    pub outcome: ValidationOutcome,
    /// W3C accessibility metadata from `OPF` `<meta>` elements (read-only).
    pub accessibility_metadata: Option<serde_json::Value>,
    /// Parsed `OPF` data including Dublin Core metadata.
    pub opf_data: Option<opf_layer::OpfData>,
    /// Whether Layer 5 found a usable embedded cover (declared, present, and
    /// decodable, or an `SVG` that rasterizes to a visible image). Always
    /// `false` for `Quarantined` reports, whether or not Layer 5 ran: a
    /// quarantined report deliberately carries no salvaged signal, matching
    /// the `None` returned for `accessibility_metadata` and `opf_data` at the
    /// same sites, and any later re-examination of the file re-runs the
    /// validator instead of trusting a report for a file that failed
    /// structural validation.
    pub has_usable_embedded_cover: bool,
}

// ── Shared utilities ──────────────────────────────────────────────────────────

/// Returns `true` if the path is safe to use within an archive.
///
/// Rejects:
/// - `..` (parent directory traversal)
/// - `%2e%2e` / `%2E%2E` (percent-encoded traversal, any case)
/// - `\` (Windows-style separator that unzippers may interpret as `/`)
/// - Leading `/` (absolute path)
/// - Leading `%2F` / `%2f` (percent-encoded leading slash)
#[must_use]
pub fn is_safe_path(path: &str) -> bool {
    let upper = path.to_ascii_uppercase();
    !path.contains("..")
        && !upper.contains("%2E%2E")
        && !path.contains('\\')
        && !path.starts_with('/')
        && !upper.starts_with("%2F")
}

// ── Configuration ─────────────────────────────────────────────────────────────

/// Hard limits for `ZIP` bomb detection.
/// Per-entry limit: 500 MB. Aggregate limit: 2 GB.
pub const MAX_ENTRY_UNCOMPRESSED_BYTES: u64 = 500 * 1024 * 1024;
/// Aggregate uncompressed-size cap across all entries; prevents slow-extraction `ZIP` bombs.
pub const MAX_AGGREGATE_UNCOMPRESSED_BYTES: u64 = 2 * 1024 * 1024 * 1024;

/// Maximum spine items before skipping `XHTML` validation (emits `Degraded`).
pub const MAX_SPINE_ITEMS: usize = 500;

/// Maximum `ZIP` central-directory entries before the archive is quarantined.
pub const MAX_ZIP_ENTRIES: usize = 20_000;

/// Maximum archive file size before the file is quarantined unread.
pub const MAX_ARCHIVE_BYTES: u64 = MAX_AGGREGATE_UNCOMPRESSED_BYTES;

// ── Entry point ───────────────────────────────────────────────────────────────

/// Inspect an opened archive without changing its bytes.
///
/// Repairable findings are instructions, not evidence of completed repair.
///
/// # Errors
/// Returns an I/O error if the source handle cannot be inspected.
pub fn inspect(file: File) -> Result<(zip_layer::ZipHandle, ValidationReport), EpubError> {
    let mut issues: Vec<Issue> = Vec::new();

    // Layer 1: ZIP integrity
    let zip_result = zip_layer::validate(file, &mut issues)?;
    if issues.iter().any(|i| i.severity == Severity::Irrecoverable) {
        return Ok((
            zip_result,
            ValidationReport {
                issues,
                outcome: ValidationOutcome::Quarantined,
                accessibility_metadata: None,
                opf_data: None,
                has_usable_embedded_cover: false,
            },
        ));
    }

    zip_layer::verify_entries(&zip_result, &mut issues);
    if issues.iter().any(|i| i.severity == Severity::Irrecoverable) {
        return Ok((
            zip_result,
            ValidationReport {
                issues,
                outcome: ValidationOutcome::Quarantined,
                accessibility_metadata: None,
                opf_data: None,
                has_usable_embedded_cover: false,
            },
        ));
    }

    // Layer 2: container.xml
    let opf_path = container_layer::validate(&zip_result, &mut issues);
    if issues.iter().any(|i| i.severity == Severity::Irrecoverable) {
        return Ok((
            zip_result,
            ValidationReport {
                issues,
                outcome: ValidationOutcome::Quarantined,
                accessibility_metadata: None,
                opf_data: None,
                has_usable_embedded_cover: false,
            },
        ));
    }

    // Layer 3: OPF
    let opf_data = opf_layer::validate(&zip_result, opf_path.as_deref(), &mut issues);
    if opf_data.is_none() {
        issues.push(Issue {
            layer: Layer::Opf,
            severity: Severity::Irrecoverable,
            kind: IssueKind::CorruptEntry {
                entry_name: opf_path.clone().unwrap_or_default(),
            },
        });
    }

    // Layer 4: XHTML
    xhtml_layer::validate(&zip_result, opf_data.as_ref(), &mut issues);

    // Layer 5: Cover
    let has_usable_embedded_cover =
        cover_layer::validate(&zip_result, opf_data.as_ref(), &mut issues);

    // Determine outcome and repair if needed
    let has_irrecoverable = issues.iter().any(|i| i.severity == Severity::Irrecoverable);
    let has_repairable = issues.iter().any(|i| i.severity == Severity::Repaired);
    let has_degraded = issues.iter().any(|i| i.severity == Severity::Degraded);

    if has_irrecoverable {
        return Ok((
            zip_result,
            ValidationReport {
                issues,
                outcome: ValidationOutcome::Quarantined,
                accessibility_metadata: None,
                opf_data: None,
                has_usable_embedded_cover: false,
            },
        ));
    }

    let accessibility_metadata = opf_data
        .as_ref()
        .and_then(|d| d.accessibility_metadata.clone());

    let outcome = if has_repairable {
        ValidationOutcome::Repaired
    } else if has_degraded {
        ValidationOutcome::Degraded
    } else {
        ValidationOutcome::Clean
    };

    Ok((
        zip_result,
        ValidationReport {
            issues,
            outcome,
            accessibility_metadata,
            opf_data,
            has_usable_embedded_cover,
        },
    ))
}

/// Validate an opened archive without modifying it.
///
/// # Errors
/// Returns an I/O error if the opened source cannot be inspected.
pub fn validate(file: File) -> Result<ValidationReport, EpubError> {
    inspect(file).map(|(_, report)| report)
}

/// Validation with optional evidence of a completed repair publication.
pub struct Validated {
    /// Final report, retaining repaired status when repair was published.
    pub report: ValidationReport,
    /// Candidate hash and size, available only after successful publication.
    pub rewritten: Option<(String, u64)>,
}

/// Validate and, when required, publish one repaired candidate.
///
/// # Errors
/// Returns validation, repair, candidate rejection or publication errors.
pub fn validate_and_repair(
    file: File,
    parent: &cap_std::fs::Dir,
    basename: &std::ffi::OsStr,
) -> Result<Validated, EpubError> {
    let (handle, report) = inspect(file)?;
    if report.outcome != ValidationOutcome::Repaired {
        return Ok(Validated {
            report,
            rewritten: None,
        });
    }
    let repairs = repair::RepairPlan::from_report(&report);
    let mut published = repack::publish(parent, basename, &report, |candidate| {
        repack::with_modifications(
            &handle,
            candidate,
            None,
            None,
            &std::collections::HashMap::new(),
            &[],
            &repairs,
        )
    })?;
    published.report.outcome = ValidationOutcome::Repaired;
    published.report.issues.extend(
        report
            .issues
            .into_iter()
            .filter(|issue| issue.severity == Severity::Repaired),
    );
    Ok(Validated {
        report: published.report,
        rewritten: Some((published.hash, published.size)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn validate_and_repair(path: &Path) -> Result<ValidationReport, EpubError> {
        let parent = cap_std::fs::Dir::open_ambient_dir(
            path.parent().unwrap(),
            cap_std::ambient_authority(),
        )?;
        super::validate_and_repair(File::open(path)?, &parent, path.file_name().unwrap())
            .map(|published| published.report)
    }
    use std::io::{Read, Write};
    use zip::write::{ExtendedFileOptions, FileOptions};
    use zip::{ZipArchive, ZipWriter};

    const CONTAINER_XML: &[u8] = br#"<?xml version="1.0" encoding="UTF-8"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles>
    <rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/>
  </rootfiles>
</container>"#;

    const CONTENT_OPF: &[u8] = br#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0">
  <metadata/>
  <manifest/>
  <spine/>
</package>"#;

    fn file_fixture(
        chapters: &[(&str, &[u8])],
        broken: bool,
        bad_mimetype: bool,
    ) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("book.epub");
        let mut writer = ZipWriter::new(File::create(&path).unwrap());
        let options = FileOptions::<ExtendedFileOptions>::default();
        writer
            .start_file(
                "mimetype",
                options.clone().compression_method(if bad_mimetype {
                    zip::CompressionMethod::Deflated
                } else {
                    zip::CompressionMethod::Stored
                }),
            )
            .unwrap();
        writer.write_all(repack::MIMETYPE_CONTENT).unwrap();
        writer
            .start_file("META-INF/container.xml", options.clone())
            .unwrap();
        writer.write_all(CONTAINER_XML).unwrap();
        let mut manifest = String::new();
        let mut spine = String::new();
        for (i, (name, _)) in chapters.iter().enumerate() {
            use std::fmt::Write;
            write!(
                manifest,
                "<item id=\"ch{i}\" href=\"{name}\" media-type=\"application/xhtml+xml\"/>"
            )
            .unwrap();
            write!(spine, "<itemref idref=\"ch{i}\"/>").unwrap();
        }
        let broken = if broken {
            "<itemref idref=\"missing\"/>"
        } else {
            ""
        };
        let opf = format!(
            "<package xmlns:dc=\"http://purl.org/dc/elements/1.1/\"><metadata><dc:title>Old</dc:title></metadata><manifest>{manifest}</manifest><spine>{spine}{broken}</spine></package>"
        );
        writer
            .start_file("OEBPS/content.opf", options.clone())
            .unwrap();
        writer.write_all(opf.as_bytes()).unwrap();
        for (name, bytes) in chapters {
            writer
                .start_file(format!("OEBPS/{name}"), options.clone())
                .unwrap();
            writer.write_all(bytes).unwrap();
        }
        writer.finish().unwrap();
        (dir, path)
    }

    #[test]
    fn file_backed_epub_admitted_index_and_refused_archive() {
        let (_dir, path) = file_fixture(&[("chapter.xhtml", b"<html/>")], false, false);
        let (handle, report) = inspect(File::open(&path).unwrap()).unwrap();
        assert_eq!(report.outcome, ValidationOutcome::Clean);
        assert_eq!(
            zip_layer::read_entry(&handle, "OEBPS/chapter.xhtml").unwrap(),
            b"<html/>"
        );
        assert!(zip_layer::read_entry(&handle, "unadmitted").is_none());
        File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_len(MAX_ARCHIVE_BYTES + 1)
            .unwrap();
        let report = super::validate(File::open(&path).unwrap()).unwrap();
        assert_eq!(report.outcome, ValidationOutcome::Quarantined);
        assert!(matches!(
            report.issues[0].kind,
            IssueKind::ArchiveTooLarge { .. }
        ));
    }

    #[test]
    fn file_backed_epub_multiple_encoding_repairs() {
        let first =
            b"<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?><html><body>\xe9</body></html>";
        let second = b"<?xml version=\"1.0\" encoding=\"windows-1252\"?><html><body>\x93hello\x94</body></html>";
        let (dir, path) = file_fixture(
            &[("first.xhtml", first), ("second.xhtml", second)],
            false,
            false,
        );
        let parent =
            cap_std::fs::Dir::open_ambient_dir(dir.path(), cap_std::ambient_authority()).unwrap();
        let result = super::validate_and_repair(
            File::open(&path).unwrap(),
            &parent,
            path.file_name().unwrap(),
        )
        .unwrap();
        assert_eq!(result.report.outcome, ValidationOutcome::Repaired);
        assert!(result.rewritten.is_some());
        let (handle, report) = inspect(File::open(&path).unwrap()).unwrap();
        assert_eq!(report.outcome, ValidationOutcome::Clean);
        for (name, content) in [("first.xhtml", "é"), ("second.xhtml", "“hello”")] {
            let bytes = zip_layer::read_entry(&handle, &format!("OEBPS/{name}")).unwrap();
            let text = std::str::from_utf8(&bytes).unwrap();
            assert!(text.contains("encoding=\"UTF-8\""));
            assert!(text.contains(content));
        }
    }

    #[test]
    fn file_backed_epub_ambiguous_encoding_stays_degraded_without_rewrite() {
        let (dir, path) = file_fixture(&[("chapter.xhtml", b"\xe9\xe0\xf3")], false, false);
        let original = std::fs::read(&path).unwrap();
        let parent =
            cap_std::fs::Dir::open_ambient_dir(dir.path(), cap_std::ambient_authority()).unwrap();
        let result = super::validate_and_repair(
            File::open(&path).unwrap(),
            &parent,
            path.file_name().unwrap(),
        )
        .unwrap();
        assert_eq!(result.report.outcome, ValidationOutcome::Degraded);
        assert!(result.rewritten.is_none());
        assert_eq!(std::fs::read(&path).unwrap(), original);
    }

    #[test]
    fn file_backed_epub_mixed_repair_and_degraded_is_accepted() {
        let (dir, path) = file_fixture(&[("chapter.xhtml", b"<html><body></html>")], false, true);
        let parent =
            cap_std::fs::Dir::open_ambient_dir(dir.path(), cap_std::ambient_authority()).unwrap();
        let result = super::validate_and_repair(
            File::open(&path).unwrap(),
            &parent,
            path.file_name().unwrap(),
        )
        .unwrap();
        assert_eq!(result.report.outcome, ValidationOutcome::Repaired);
        assert!(
            result
                .report
                .issues
                .iter()
                .any(|issue| issue.severity == Severity::Degraded)
        );
        assert_eq!(
            super::validate(File::open(&path).unwrap()).unwrap().outcome,
            ValidationOutcome::Degraded
        );
    }

    #[test]
    fn file_backed_epub_spine_repair_composes_metadata() {
        let (dir, path) = file_fixture(&[("chapter.xhtml", b"<html/>")], true, false);
        let (handle, source) = inspect(File::open(&path).unwrap()).unwrap();
        let repairs = repair::RepairPlan::from_report(&source);
        let opf = repairs
            .replacement(&handle, "OEBPS/content.opf")
            .unwrap()
            .unwrap();
        let opf = crate::services::writeback::opf_rewrite::transform(
            &opf,
            &crate::services::writeback::opf_rewrite::Target {
                title: Some("New"),
                ..Default::default()
            },
        )
        .unwrap();
        let parent =
            cap_std::fs::Dir::open_ambient_dir(dir.path(), cap_std::ambient_authority()).unwrap();
        repack::publish(&parent, path.file_name().unwrap(), &source, |file| {
            repack::with_modifications(
                &handle,
                file,
                Some("OEBPS/content.opf"),
                Some(&opf),
                &std::collections::HashMap::new(),
                &[],
                &repairs,
            )
        })
        .unwrap();
        let (handle, report) = inspect(File::open(&path).unwrap()).unwrap();
        assert_eq!(report.outcome, ValidationOutcome::Clean);
        let final_opf =
            String::from_utf8(zip_layer::read_entry(&handle, "OEBPS/content.opf").unwrap())
                .unwrap();
        assert!(final_opf.contains("<dc:title>New</dc:title>"));
        assert!(!final_opf.contains("idref=\"missing\""));
    }

    #[test]
    fn file_backed_epub_source_handle_survives_replacement() {
        let (_dir, path) =
            file_fixture(&[("chapter.xhtml", b"<html>original</html>")], false, false);
        let (handle, _) = inspect(File::open(&path).unwrap()).unwrap();
        let replacement = path.with_extension("replacement");
        std::fs::write(&replacement, b"new object").unwrap();
        std::fs::rename(&replacement, &path).unwrap();
        assert_eq!(
            zip_layer::read_entry(&handle, "OEBPS/chapter.xhtml").unwrap(),
            b"<html>original</html>"
        );
    }

    #[test]
    fn candidate_publication_accepts_final_archive_and_hash() {
        let (dir, path) = file_fixture(&[("chapter.xhtml", b"<html/>")], false, false);
        let (handle, source) = inspect(File::open(&path).unwrap()).unwrap();
        let parent =
            cap_std::fs::Dir::open_ambient_dir(dir.path(), cap_std::ambient_authority()).unwrap();
        let opf = crate::services::writeback::opf_rewrite::transform(
            &zip_layer::read_entry(&handle, "OEBPS/content.opf").unwrap(),
            &crate::services::writeback::opf_rewrite::Target {
                title: Some("Published"),
                ..Default::default()
            },
        )
        .unwrap();
        let published = repack::publish(&parent, path.file_name().unwrap(), &source, |file| {
            repack::with_modifications(
                &handle,
                file,
                Some("OEBPS/content.opf"),
                Some(&opf),
                &std::collections::HashMap::new(),
                &[],
                &repair::RepairPlan::default(),
            )
        })
        .unwrap();
        assert_eq!(published.report.outcome, ValidationOutcome::Clean);
        assert_eq!(
            published.report.opf_data.unwrap().title.as_deref(),
            Some("Published")
        );
        let mut final_file = File::open(&path).unwrap();
        assert_eq!(published.hash, repack::hash_file(&mut final_file).unwrap());
        assert_eq!(published.size, final_file.metadata().unwrap().len());
    }

    #[test]
    fn candidate_publication_rejects_regression_before_replacement() {
        let (dir, path) = file_fixture(&[("chapter.xhtml", b"<html/>")], false, false);
        let original = std::fs::read(&path).unwrap();
        let (handle, source) = inspect(File::open(&path).unwrap()).unwrap();
        let parent =
            cap_std::fs::Dir::open_ambient_dir(dir.path(), cap_std::ambient_authority()).unwrap();
        let replacements = std::collections::HashMap::from([(
            "OEBPS/chapter.xhtml".into(),
            b"<html><body></html>".to_vec(),
        )]);
        let result = repack::publish(&parent, path.file_name().unwrap(), &source, |file| {
            repack::with_modifications(
                &handle,
                file,
                None,
                None,
                &replacements,
                &[],
                &repair::RepairPlan::default(),
            )
        });
        assert!(matches!(result, Err(EpubError::CandidateRejected(_))));
        assert_eq!(std::fs::read(&path).unwrap(), original);
    }

    #[test]
    fn candidate_publication_validator_error_leaves_source_untouched() {
        let (dir, path) = file_fixture(&[], false, false);
        let original = std::fs::read(&path).unwrap();
        let (handle, source) = inspect(File::open(&path).unwrap()).unwrap();
        let parent =
            cap_std::fs::Dir::open_ambient_dir(dir.path(), cap_std::ambient_authority()).unwrap();
        let result = repack::publish_with_validator(
            &parent,
            path.file_name().unwrap(),
            &source,
            |file| {
                repack::with_modifications(
                    &handle,
                    file,
                    None,
                    None,
                    &std::collections::HashMap::new(),
                    &[],
                    &repair::RepairPlan::default(),
                )
            },
            |_| Err(std::io::Error::other("validator failed").into()),
        );
        assert!(matches!(result, Err(EpubError::Io(_))));
        assert_eq!(std::fs::read(&path).unwrap(), original);
    }

    #[test]
    fn candidate_publication_required_repair_error_leaves_source_untouched() {
        let (dir, path) = file_fixture(&[("chapter.xhtml", b"<html/>")], false, false);
        let original = std::fs::read(&path).unwrap();
        let (handle, mut source) = inspect(File::open(&path).unwrap()).unwrap();
        source.issues.push(Issue {
            layer: Layer::Xhtml,
            severity: Severity::Repaired,
            kind: IssueKind::EncodingMismatch {
                entry_name: "OEBPS/chapter.xhtml".into(),
                declared: "unsupported-encoding".into(),
                detected: "UTF-8".into(),
            },
        });
        let repairs = repair::RepairPlan::from_report(&source);
        let parent =
            cap_std::fs::Dir::open_ambient_dir(dir.path(), cap_std::ambient_authority()).unwrap();
        let result = repack::publish(&parent, path.file_name().unwrap(), &source, |file| {
            repack::with_modifications(
                &handle,
                file,
                None,
                None,
                &std::collections::HashMap::new(),
                &[],
                &repairs,
            )
        });
        assert!(matches!(result, Err(EpubError::Repair(_))));
        assert_eq!(std::fs::read(&path).unwrap(), original);
    }

    #[test]
    fn file_backed_epub_unreadable_container_is_replaced_once() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("book.epub");
        let mut writer = ZipWriter::new(File::create(&path).unwrap());
        writer
            .start_file(
                "mimetype",
                FileOptions::<ExtendedFileOptions>::default()
                    .compression_method(zip::CompressionMethod::Stored),
            )
            .unwrap();
        writer.write_all(repack::MIMETYPE_CONTENT).unwrap();
        writer
            .start_file(
                "META-INF/container.xml",
                FileOptions::<ExtendedFileOptions>::default(),
            )
            .unwrap();
        writer.write_all(b"unreadable container").unwrap();
        writer
            .start_file(
                "OEBPS/content.opf",
                FileOptions::<ExtendedFileOptions>::default(),
            )
            .unwrap();
        writer.write_all(CONTENT_OPF).unwrap();
        writer.finish().unwrap();
        let mut bytes = std::fs::read(&path).unwrap();
        let offset = bytes
            .windows(4)
            .enumerate()
            .find_map(|(offset, signature)| {
                (signature == [0x50, 0x4b, 1, 2]
                    && bytes.get(offset + 46..offset + 68) == Some(b"META-INF/container.xml"))
                .then_some(offset)
            })
            .unwrap();
        bytes[offset + 16] ^= 1;
        std::fs::write(&path, bytes).unwrap();
        let report = validate_and_repair(&path).unwrap();
        assert_eq!(report.outcome, ValidationOutcome::Repaired);
        let (handle, final_report) = inspect(File::open(&path).unwrap()).unwrap();
        assert_eq!(final_report.outcome, ValidationOutcome::Clean);
        assert_eq!(
            handle
                .entries
                .iter()
                .filter(|name| name.as_str() == "META-INF/container.xml")
                .count(),
            1
        );
    }
    /// A structurally otherwise-valid `EPUB` whose only problem is a
    /// `mimetype` entry that is neither first nor stored, so the mimetype
    /// rules are the sole source of the `Repaired` issues.
    fn make_epub_with_bad_mimetype() -> Vec<u8> {
        let buf = std::io::Cursor::new(Vec::new());
        let mut w = ZipWriter::new(buf);
        let default_opts: FileOptions<ExtendedFileOptions> = FileOptions::default();

        w.start_file("META-INF/container.xml", default_opts.clone())
            .unwrap();
        w.write_all(CONTAINER_XML).unwrap();

        w.start_file("OEBPS/content.opf", default_opts).unwrap();
        w.write_all(CONTENT_OPF).unwrap();

        // mimetype deliberately last and Deflated: violates position and
        // compression, but nothing else.
        let mimetype_opts: FileOptions<ExtendedFileOptions> =
            FileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        w.start_file(repack::MIMETYPE_ENTRY, mimetype_opts).unwrap();
        w.write_all(repack::MIMETYPE_CONTENT).unwrap();

        w.finish().unwrap().into_inner()
    }

    #[test]
    fn invalid_mimetype_is_repaired_end_to_end() {
        let bytes = make_epub_with_bad_mimetype();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.epub");
        std::fs::write(&path, &bytes).unwrap();

        let report = validate_and_repair(&path).unwrap();
        assert_eq!(report.outcome, ValidationOutcome::Repaired);
        assert!(report.issues.iter().any(|i| matches!(
            &i.kind,
            IssueKind::InvalidMimetype {
                problem: MimetypeProblem::NotFirst
            }
        )));
        assert!(report.issues.iter().any(|i| matches!(
            &i.kind,
            IssueKind::InvalidMimetype {
                problem: MimetypeProblem::Compressed
            }
        )));

        let repacked = std::fs::read(&path).unwrap();
        let mut archive = ZipArchive::new(std::io::Cursor::new(repacked)).unwrap();
        {
            let mut first = archive.by_index(0).unwrap();
            assert_eq!(first.name(), repack::MIMETYPE_ENTRY);
            assert_eq!(first.compression(), zip::CompressionMethod::Stored);
            let mut content = Vec::new();
            first.read_to_end(&mut content).unwrap();
            assert_eq!(content, repack::MIMETYPE_CONTENT);
        }

        // A second pass over the repacked file reports no InvalidMimetype issue.
        let report2 = validate_and_repair(&path).unwrap();
        assert!(
            !report2
                .issues
                .iter()
                .any(|i| matches!(&i.kind, IssueKind::InvalidMimetype { .. }))
        );
        assert_eq!(report2.outcome, ValidationOutcome::Clean);
    }

    fn quarantine_of(entries: &[(&str, &[u8])]) -> ValidationReport {
        let mut writer = ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let options = FileOptions::<ExtendedFileOptions>::default()
            .compression_method(zip::CompressionMethod::Stored);
        writer.start_file("mimetype", options.clone()).unwrap();
        writer.write_all(repack::MIMETYPE_CONTENT).unwrap();
        for (name, bytes) in entries {
            writer.start_file(*name, options.clone()).unwrap();
            writer.write_all(bytes).unwrap();
        }
        let bytes = writer.finish().unwrap().into_inner();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("book.epub");
        std::fs::write(&path, &bytes).unwrap();
        let report = validate_and_repair(&path).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        report
    }

    fn assert_irrecoverable(report: &ValidationReport, expected: impl Fn(&IssueKind) -> bool) {
        assert_eq!(report.outcome, ValidationOutcome::Quarantined);
        assert!(
            report
                .issues
                .iter()
                .any(|i| i.severity == Severity::Irrecoverable && expected(&i.kind)),
            "{:?}",
            report.issues
        );
    }

    #[test]
    fn only_content_verdicts_are_file_defects() {
        let io = || std::io::Error::other("storage");
        assert!(EpubError::CandidateRejected("regressed".into()).is_file_defect());
        assert!(EpubError::Repair("no entry".into()).is_file_defect());
        assert!(EpubError::Zip(zip::result::ZipError::FileNotFound).is_file_defect());
        assert!(!EpubError::Zip(zip::result::ZipError::Io(io())).is_file_defect());
        assert!(!EpubError::Io(io()).is_file_defect());
        assert!(
            !EpubError::PublicationUncertain {
                hash: "h".into(),
                error: Box::new(EpubError::Io(io())),
            }
            .is_file_defect()
        );
    }

    #[test]
    fn archive_without_container_or_package_document_is_quarantined() {
        let report = quarantine_of(&[("OEBPS/chapter.xhtml", b"<html/>")]);
        assert_irrecoverable(&report, |kind| {
            matches!(
                kind,
                IssueKind::MissingContainer {
                    opf_candidate: None
                }
            )
        });
    }

    #[test]
    fn empty_archive_is_quarantined() {
        let report = quarantine_of(&[]);
        assert_irrecoverable(&report, |kind| {
            matches!(
                kind,
                IssueKind::MissingContainer {
                    opf_candidate: None
                }
            )
        });
    }

    #[test]
    fn container_naming_no_package_document_is_quarantined() {
        let report = quarantine_of(&[(
            "META-INF/container.xml",
            b"<container><rootfiles/></container>",
        )]);
        assert_irrecoverable(
            &report,
            |kind| matches!(kind, IssueKind::CorruptEntry { entry_name } if entry_name == "META-INF/container.xml"),
        );
    }

    #[test]
    fn package_document_absent_from_archive_is_quarantined() {
        let report = quarantine_of(&[("META-INF/container.xml", CONTAINER_XML)]);
        assert_irrecoverable(
            &report,
            |kind| matches!(kind, IssueKind::CorruptEntry { entry_name } if entry_name == "OEBPS/content.opf"),
        );
    }

    #[test]
    fn package_document_failing_to_parse_is_quarantined() {
        let report = quarantine_of(&[
            ("META-INF/container.xml", CONTAINER_XML),
            ("OEBPS/content.opf", b"<package><metadata></package>"),
        ]);
        assert_irrecoverable(
            &report,
            |kind| matches!(kind, IssueKind::CorruptEntry { entry_name } if entry_name == "OEBPS/content.opf"),
        );
    }

    #[test]
    fn truncated_package_document_is_quarantined() {
        let report = quarantine_of(&[
            ("META-INF/container.xml", CONTAINER_XML),
            (
                "OEBPS/content.opf",
                b"<package xmlns:dc=\"http://purl.org/dc/elements/1.1/\"><metadata><dc:title>Cut off",
            ),
        ]);
        assert_irrecoverable(
            &report,
            |kind| matches!(kind, IssueKind::CorruptEntry { entry_name } if entry_name == "OEBPS/content.opf"),
        );
    }

    #[test]
    fn crc_failure_in_a_content_entry_is_quarantined() {
        let (_dir, path) = file_fixture(&[("chapter.xhtml", b"<html/>")], false, false);
        let mut bytes = std::fs::read(&path).unwrap();
        let last_directory_record = bytes
            .windows(4)
            .rposition(|window| window == [0x50, 0x4b, 0x01, 0x02])
            .unwrap();
        bytes[last_directory_record + 16] ^= 0xFF;
        std::fs::write(&path, &bytes).unwrap();
        let report = validate_and_repair(&path).unwrap();
        assert_irrecoverable(
            &report,
            |kind| matches!(kind, IssueKind::CorruptEntry { entry_name } if entry_name == "OEBPS/chapter.xhtml"),
        );
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
    }
}
