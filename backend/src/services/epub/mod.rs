//! EPUB structural validation and auto-repair pipeline.
//!
//! Entry point: [`validate_and_repair`]. Runs 5 sequential layers
//! (ZIP → container → OPF → XHTML → cover) and optionally re-packages
//! the archive if repairs were made.

use std::path::Path;

pub mod container_layer;
pub mod cover_layer;
pub mod opf_layer;
pub mod repack;
pub mod repair;
pub mod xhtml_layer;
pub mod zip_layer;

// ── Error type ───────────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum EpubError {
    #[error("ZIP I/O error: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("XML parse error: {0}")]
    Xml(#[from] quick_xml::Error),
    #[error("tempfile error: {0}")]
    TempFile(#[from] tempfile::PersistError),
}

// ── Issue types ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Layer {
    Zip,
    Container,
    Opf,
    Xhtml,
    Cover,
}

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
// Fields are intentionally public API for callers in future pipeline steps.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum IssueKind {
    /// ZIP entry contains path traversal components or absolute path.
    PathTraversal { entry_name: String },
    /// ZIP entry or aggregate uncompressed size exceeds limit.
    ZipBomb {
        entry_name: String,
        size: u64,
        limit: u64,
    },
    /// ZIP entry is unreadable (corrupt data).
    CorruptEntry { entry_name: String },
    /// `META-INF/container.xml` absent; OPF path provided if regeneratable.
    MissingContainer { opf_candidate: Option<String> },
    /// OPF path extracted from container.xml fails path-safety check.
    UnsafeOpfPath { path: String },
    /// Spine entry references an item not in the manifest.
    BrokenSpineRef { idref: String },
    /// Manifest href fails path-safety check.
    UnsafeManifestHref { href: String },
    /// EPUB has more spine items than the 500-item cap.
    SpineCapExceeded { count: usize },
    /// XML file declared/detected encoding mismatch, was transcoded.
    EncodingMismatch {
        entry_name: String,
        declared: String,
        detected: String,
    },
    /// XML file has ambiguous encoding (conditions for safe transcode not met).
    AmbiguousEncoding { entry_name: String },
    /// XML parse error in a spine document.
    MalformedXhtml { entry_name: String, detail: String },
    /// Cover file referenced in OPF does not exist in the archive.
    MissingCover { href: String },
    /// Cover file exists but is not a decodable JPEG or PNG.
    UndecodableCover { href: String },
}

// All fields are public API for callers; not all are read within this crate yet.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct Issue {
    pub layer: Layer,
    pub severity: Severity,
    pub kind: IssueKind,
}

/// Overall validation outcome. Determines how the ingestion pipeline handles the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationOutcome {
    /// All layers passed with no issues.
    Clean,
    /// One or more issues were automatically repaired; re-packaged ZIP is valid.
    Repaired,
    /// One or more non-critical issues; file usable but not fully conformant.
    Degraded,
    /// Irrecoverable issue; file must be quarantined.
    Quarantined,
}

#[derive(Debug, Clone)]
pub struct ValidationReport {
    pub issues: Vec<Issue>,
    pub outcome: ValidationOutcome,
    /// W3C accessibility metadata from OPF `<meta>` elements (read-only).
    pub accessibility_metadata: Option<serde_json::Value>,
    /// Parsed OPF data including Dublin Core metadata.
    pub opf_data: Option<opf_layer::OpfData>,
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
pub(crate) fn is_safe_path(path: &str) -> bool {
    let upper = path.to_ascii_uppercase();
    !path.contains("..")
        && !upper.contains("%2E%2E")
        && !path.contains('\\')
        && !path.starts_with('/')
        && !upper.starts_with("%2F")
}

// ── Configuration ─────────────────────────────────────────────────────────────

/// Hard limits for ZIP bomb detection.
/// Per-entry limit: 500 MB. Aggregate limit: 2 GB.
pub const MAX_ENTRY_UNCOMPRESSED_BYTES: u64 = 500 * 1024 * 1024;
pub const MAX_AGGREGATE_UNCOMPRESSED_BYTES: u64 = 2 * 1024 * 1024 * 1024;

/// Maximum spine items before skipping XHTML validation (→ Degraded).
pub const MAX_SPINE_ITEMS: usize = 500;

// ── Entry point ───────────────────────────────────────────────────────────────

/// Validate and optionally repair an EPUB at the given path.
///
/// This function is synchronous — call it from `tokio::task::spawn_blocking`.
///
/// # Return value
///
/// Returns a [`ValidationReport`] describing all issues found and the overall
/// outcome. `Quarantined` means the caller must move the file to quarantine.
/// `Repaired` means the file at `path` has been atomically replaced with the
/// repaired version. `Degraded` and `Clean` leave the file untouched.
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
        });
    }

    // Layer 3: OPF
    let opf_data = opf_layer::validate(&zip_result, opf_path.as_deref(), &mut issues);

    // Layer 4: XHTML
    xhtml_layer::validate(&zip_result, opf_data.as_ref(), &mut issues);

    // Layer 5: Cover
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
    })
}
