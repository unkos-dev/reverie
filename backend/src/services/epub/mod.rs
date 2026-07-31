//! `EPUB` structural validation and auto-repair pipeline.
//!
//! Entry point: `validate_and_repair`. Runs 5 sequential layers
//! (`ZIP` → container → `OPF` → `XHTML` → cover) and optionally re-packages
//! the archive if repairs were made. Each layer appends `Issue`s to a
//! shared `Vec`; the overall `ValidationOutcome` is derived from the
//! worst-severity issue across all layers.

use std::path::Path;

/// `META-INF/container.xml` parsing and `OPF` path location (Layer 2).
pub mod container_layer;
/// Cover-image decodability validation — `JPEG`/`PNG` only (Layer 5).
pub mod cover_layer;
/// `OPF` manifest and spine parsing, Dublin Core metadata extraction (Layer 3).
pub mod opf_layer;
/// Low-level `ZIP` repack helper used by the repair layer.
pub mod repack;
/// High-level repair orchestrator: applies `Repaired`-severity fixes and atomically
/// replaces the source file.
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
    /// `zip` crate error (corrupt central directory, unsupported compression, etc.).
    #[error("ZIP I/O error: {0}")]
    Zip(#[from] zip::result::ZipError),
    /// Filesystem I/O error reading or writing the archive.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    /// `quick_xml` parse error surfaced during repack `XML` rewriting.
    #[error("XML parse error: {0}")]
    Xml(#[from] quick_xml::Error),
    /// `tempfile` persist error when atomically replacing the source file.
    #[error("tempfile error: {0}")]
    TempFile(#[from] tempfile::PersistError),
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
    /// File cannot be used; must be quarantined.
    Irrecoverable,
    /// Issue was automatically repaired.
    Repaired,
    /// Issue present but file is still usable; stored as-is.
    Degraded,
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
    /// `ZIP` entry is unreadable (corrupt data).
    CorruptEntry {
        /// Offending entry name.
        entry_name: String,
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
        /// Encoding heuristic detection found in the bytes.
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
    /// decodable/parsable). `false` for `Quarantined` reports, since Layers
    /// 1-4 return before Layer 5 ever runs.
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

// ── Entry point ───────────────────────────────────────────────────────────────

/// Validate and optionally repair an `EPUB` at the given path.
///
/// This function is synchronous — call it from `tokio::task::spawn_blocking`.
///
/// # Return value
///
/// Returns a [`ValidationReport`] describing all issues found and the overall
/// outcome. `Quarantined` means the caller must move the file to quarantine.
/// `Repaired` means the file at `path` has been atomically replaced with the
/// repaired version. `Degraded` and `Clean` leave the file untouched.
///
/// # Errors
///
/// Returns [`EpubError::Io`] if the file at `path` cannot be read from the
/// filesystem. Returns [`EpubError::Zip`] only when the repair pass
/// ([`repair::repackage`]) hits a `ZIP` failure while rewriting the archive;
/// structural archive invalidity detected by the Layer 1 scan is recorded as
/// an `Irrecoverable` issue and surfaced via [`ValidationOutcome::Quarantined`]
/// rather than as an error. Returns [`EpubError::TempFile`] if the repaired
/// archive cannot be atomically persisted over `path`.
pub fn validate_and_repair(path: &Path) -> Result<ValidationReport, EpubError> {
    let mut issues: Vec<Issue> = Vec::new();

    // Layer 1: ZIP integrity
    let zip_result = zip_layer::validate(path, &mut issues)?;
    if issues.iter().any(|i| i.severity == Severity::Irrecoverable) {
        return Ok(ValidationReport {
            issues,
            outcome: ValidationOutcome::Quarantined,
            accessibility_metadata: None,
            opf_data: None,
            has_usable_embedded_cover: false,
        });
    }

    // Layer 2: container.xml
    let opf_path = container_layer::validate(&zip_result, &mut issues);
    if issues.iter().any(|i| i.severity == Severity::Irrecoverable) {
        return Ok(ValidationReport {
            issues,
            outcome: ValidationOutcome::Quarantined,
            accessibility_metadata: None,
            opf_data: None,
            has_usable_embedded_cover: false,
        });
    }

    // Layer 3: OPF
    let opf_data = opf_layer::validate(&zip_result, opf_path.as_deref(), &mut issues);

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
        return Ok(ValidationReport {
            issues,
            outcome: ValidationOutcome::Quarantined,
            accessibility_metadata: None,
            opf_data: None,
            has_usable_embedded_cover: false,
        });
    }

    let accessibility_metadata = opf_data
        .as_ref()
        .and_then(|d| d.accessibility_metadata.clone());

    if has_repairable {
        let opf_path_str = opf_data.as_ref().map(|d| d.opf_path.as_str());
        repair::repackage(path, &issues, opf_path_str)?;
        return Ok(ValidationReport {
            issues,
            outcome: ValidationOutcome::Repaired,
            accessibility_metadata,
            opf_data,
            has_usable_embedded_cover,
        });
    }

    let outcome = if has_degraded {
        ValidationOutcome::Degraded
    } else {
        ValidationOutcome::Clean
    };

    Ok(ValidationReport {
        issues,
        outcome,
        accessibility_metadata,
        opf_data,
        has_usable_embedded_cover,
    })
}
