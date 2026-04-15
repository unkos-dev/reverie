# Plan: EPUB Structural Validation and Auto-Repair

## Summary

Implement a 5-layer pure Rust EPUB validation pipeline (ZIP → container.xml → OPF → XHTML → cover) with auto-repair, integrating into the existing ingestion orchestrator between file copy and DB insert. Files that pass or are repaired proceed normally; files with non-critical issues are marked `degraded`; irrecoverable files are quarantined using the existing `quarantine_file` infrastructure.

## User Story

As an ingestion pipeline, I want to validate and repair EPUB files after they are copied to the library, so that the library only contains structurally sound books and all issues are recorded for diagnosis.

## Problem → Solution

Currently the orchestrator copies EPUBs to the library and immediately marks them `complete` in the DB with `validation_status = 'pending'` (the default, never updated). → After copy, run the 5-layer validation pipeline. Map the outcome to `validation_status` (`valid`/`repaired`/`degraded`/quarantine) and store it on the `manifestations` row. Store W3C accessibility metadata in the new `accessibility_metadata` JSONB column.

## Metadata

- **Complexity**: Large
- **Source PRD**: `plans/BLUEPRINT.md`
- **PRD Phase**: Step 5
- **Estimated Files**: 12 new + 3 modified

---

## UX Design

N/A — internal pipeline change. No user-facing UI changes in this step.

---

## Mandatory Reading

| Priority | File | Lines | Why |
|---|---|---|---|
| P0 (critical) | `backend/src/services/ingestion/orchestrator.rs` | 195-330 | Integration point: `process_file` — validation slots between copy (step 3) and CTE insert (step 5) |
| P0 (critical) | `backend/src/services/ingestion/copier.rs` | all | Atomic write pattern to mirror in `repair.rs` |
| P0 (critical) | `backend/src/services/ingestion/quarantine.rs` | all | Existing quarantine function to reuse; sidecar JSON format |
| P1 (important) | `backend/src/services/ingestion/mod.rs` | all | Module visibility pattern to mirror for `epub/mod.rs` |
| P1 (important) | `backend/src/models/ingestion_job.rs` | all | sqlx query pattern: cast enums to `::text`, use `query_as` |
| P1 (important) | `backend/migrations/20260412150001_extensions_enums_and_roles.up.sql` | all | Existing `validation_status` enum values |
| P1 (important) | `backend/migrations/20260412150002_core_tables.up.sql` | 50-90 | `manifestations` table schema |
| P2 (reference) | `backend/Cargo.toml` | all | Existing dependencies; new deps go here |
| P2 (reference) | `backend/src/error.rs` | all | `AppError` wraps anyhow; epub errors propagate cleanly |
| P2 (reference) | `plans/BLUEPRINT.md` | Step 5 | Authoritative task list |

## External Documentation

| Topic | Source | Key Takeaway |
|---|---|---|
| `zip` crate API | crates.io/crates/zip | `ZipArchive::new`, `ZipFile::name()`, `ZipFile::size()` (uncompressed), `by_index` |
| `quick-xml` reading | docs.rs/quick-xml | Use `Reader::from_str` / `Reader::from_reader`; event-based (`Event::Start`, `Event::Text`, `Event::End`) |
| `quick-xml` writing | docs.rs/quick-xml | Use `Writer` with `BytesStart`, `BytesEnd`, `BytesText` |
| `encoding_rs` | docs.rs/encoding_rs | `Encoding::for_label`, `decode_without_bom_handling` returns `(Cow<str>, EncoderResult, bool)` — third field is `had_errors` |
| `image` crate | docs.rs/image | `image::load_from_memory` with `default-features = false, features = ["jpeg", "png"]` |

---

## Patterns to Mirror

### ATOMIC_WRITE
```rust
// SOURCE: backend/src/services/ingestion/copier.rs:56-88
// Create temp file in same directory as destination (guarantees same filesystem → atomic rename)
let temp = NamedTempFile::new_in(dest_dir)?;
// ... write to temp ...
temp.persist(&final_path)?;  // atomic rename
```

### QUARANTINE_CALL
```rust
// SOURCE: backend/src/services/ingestion/orchestrator.rs:318-325
async fn quarantine_async(source: &Path, quarantine_path: &Path, reason: &str) {
    let source = source.to_path_buf();
    let qpath = quarantine_path.to_path_buf();
    let reason = reason.to_string();
    let _ = tokio::task::spawn_blocking(move || {
        if let Err(e) = quarantine::quarantine_file(&source, &qpath, &reason) {
            tracing::error!(error = %e, "quarantine failed");
        }
    })
    .await;
}
```

### SPAWN_BLOCKING_PATTERN
```rust
// SOURCE: backend/src/services/ingestion/orchestrator.rs:237-261
let result = {
    let path = path.clone();
    tokio::task::spawn_blocking(move || {
        // synchronous file I/O here
        do_something(&path)
    })
    .await
};
match result {
    Ok(Ok(value)) => { /* use value */ }
    Ok(Err(e)) => return ProcessResult::Failed(format!("operation failed: {e}")),
    Err(e) => return ProcessResult::Failed(format!("spawn_blocking panicked: {e}")),
}
```

### THISERROR_ERROR
```rust
// SOURCE: backend/src/services/ingestion/copier.rs:18-30
#[derive(Debug, thiserror::Error)]
pub enum CopyError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("SHA-256 mismatch: source_hash={source_hash}, dest_hash={dest_hash}")]
    HashMismatch { source_hash: String, dest_hash: String },
    #[error("tempfile persist failed: {0}")]
    Persist(#[from] tempfile::PersistError),
}
```

### SQLX_ENUM_CAST
```rust
// SOURCE: backend/src/models/ingestion_job.rs:19-32
// Reading enums from DB: cast to ::text so sqlx maps to String
sqlx::query_as::<_, Model>(
    "SELECT status::text, ... FROM table WHERE id = $1"
)
// Writing enums to DB: cast literal or bind string to enum type
sqlx::query("UPDATE manifestations SET validation_status = $1::validation_status WHERE id = $2")
    .bind(status_str)   // &str matches the enum variant name
    .bind(id)
```

### MODULE_STRUCTURE
```rust
// SOURCE: backend/src/services/ingestion/mod.rs
pub mod zip_layer;
pub mod container_layer;
pub mod opf_layer;
pub mod xhtml_layer;
pub mod cover_layer;
pub mod repair;

// Types live in mod.rs — they are the public contract
#[derive(Debug, Clone)]
pub struct ValidationReport { ... }

pub fn validate_and_repair(path: &Path) -> Result<ValidationReport, EpubError> { ... }
```

### TRACING_PATTERN
```rust
// SOURCE: backend/src/services/ingestion/orchestrator.rs:50-57
tracing::info!(processed = r.processed, failed = r.failed, "batch complete");
tracing::error!(error = %e, "quarantine failed");
tracing::warn!(path = %dest, error = %e, "failed to remove orphaned file");
```

### TEST_STRUCTURE
```rust
// SOURCE: backend/src/services/ingestion/copier.rs:100+
#[cfg(test)]
mod tests {
    use super::*;

    #[test]  // unit tests: no #[ignore]
    fn descriptive_test_name() {
        let dir = tempfile::tempdir().unwrap();
        // arrange, act, assert
    }

    #[tokio::test]
    #[ignore]  // DB tests only: requires running postgres
    async fn db_test() { ... }
}
```

---

## Files to Change

| File | Action | Justification |
|---|---|---|
| `backend/migrations/20260415000001_epub_validation.up.sql` | CREATE | Add `'degraded'` to `validation_status` enum + `accessibility_metadata` JSONB column |
| `backend/migrations/20260415000001_epub_validation.down.sql` | CREATE | Remove `accessibility_metadata` column; document enum value cannot be removed |
| `backend/Cargo.toml` | UPDATE | Add `zip`, `quick-xml`, `encoding_rs`, `image` dependencies |
| `backend/src/services/epub/mod.rs` | CREATE | Public types (`Issue`, `ValidationReport`) + `validate_and_repair` entry point |
| `backend/src/services/epub/zip_layer.rs` | CREATE | Layer 1: ZIP integrity + path traversal + size limit checks |
| `backend/src/services/epub/container_layer.rs` | CREATE | Layer 2: `META-INF/container.xml` parse + OPF path extraction + auto-regenerate |
| `backend/src/services/epub/opf_layer.rs` | CREATE | Layer 3: OPF parse, spine validation, manifest href safety, accessibility metadata |
| `backend/src/services/epub/xhtml_layer.rs` | CREATE | Layer 4: XHTML parse, encoding detection, unclosed tag repair |
| `backend/src/services/epub/cover_layer.rs` | CREATE | Layer 5: cover declaration lookup + JPEG/PNG decode verify |
| `backend/src/services/epub/repair.rs` | CREATE | Re-package ZIP atomically after collecting all Issues |
| `backend/src/services/mod.rs` | UPDATE | Add `pub mod epub;` |
| `backend/src/services/ingestion/orchestrator.rs` | UPDATE | Insert validation between copy and CTE DB insert; pass `validation_status` to INSERT |

## NOT Building

- EPUB 3 Media Overlay (SMIL) validation
- NCX/spine ordering correctness (Step 6 reads OPF; ordering is metadata concern)
- Full WCAG accessibility audit — read-only metadata extraction only (DESIGN_BRIEF §5.3)
- Async streaming validation — all validation is synchronous in `spawn_blocking`
- Any HTTP endpoint — this is pipeline-internal only
- Re-validation of existing library files — validation runs once on ingestion

---

## Step-by-Step Tasks

### Task 1: Add Migration

- **ACTION**: Create `.up.sql` and `.down.sql` migration files for the schema changes required by validation.
- **IMPLEMENT**:

  **`backend/migrations/20260415000001_epub_validation.up.sql`**:
  ```sql
  -- sqlx:disable-transaction
  -- ALTER TYPE ... ADD VALUE cannot run inside a PostgreSQL transaction.
  -- sqlx wraps every migration in a transaction by default — this pragma disables that.

  -- Add 'degraded' to validation_status enum.
  -- NOTE: PostgreSQL cannot remove enum values once added if any rows use them.
  -- Roll back before any EPUBs are ingested to keep the rollback clean.
  ALTER TYPE validation_status ADD VALUE IF NOT EXISTS 'degraded';

  -- Add accessibility metadata JSONB column (read-only, sourced from OPF)
  ALTER TABLE manifestations ADD COLUMN accessibility_metadata JSONB;
  ```

  **`backend/migrations/20260415000001_epub_validation.down.sql`**:
  ```sql
  -- Remove accessibility_metadata column (safe regardless of row contents)
  ALTER TABLE manifestations DROP COLUMN IF EXISTS accessibility_metadata;

  -- NOTE: 'degraded' enum value CANNOT be removed from PostgreSQL without a full
  -- type rebuild. To truly roll back, restore the DB from a backup taken before
  -- the migration was applied, or rebuild the type:
  --   ALTER TABLE manifestations ALTER COLUMN validation_status TYPE TEXT;
  --   DROP TYPE validation_status;
  --   CREATE TYPE validation_status AS ENUM ('pending', 'valid', 'invalid', 'repaired');
  --   ALTER TABLE manifestations ALTER COLUMN validation_status
  --     TYPE validation_status USING validation_status::validation_status;
  -- Only do this if no rows have validation_status = 'degraded'.
  ```

- **GOTCHA**: `-- sqlx:disable-transaction` is already in the `.up.sql` above. This pragma is **required** — without it sqlx wraps the migration in a transaction and Postgres rejects `ALTER TYPE ... ADD VALUE` with "cannot run inside a transaction block". The `IF NOT EXISTS` guard makes re-runs idempotent.
- **VALIDATE**: `sqlx migrate run` completes without error. `psql -c "\dT+ validation_status"` shows `degraded` in the enum. `\d manifestations` shows `accessibility_metadata jsonb`.

---

### Task 2: Add Cargo Dependencies

- **ACTION**: Add four new dependencies to `backend/Cargo.toml`.
- **IMPLEMENT**: In the `[dependencies]` section, add in alphabetical order:
  ```toml
  encoding_rs = "0.8"
  image = { version = "0.25", default-features = false, features = ["jpeg", "png"] }
  quick-xml = "0.37"
  zip = "2"
  ```
- **GOTCHA**: `image` must have `default-features = false`. Without it, the crate pulls in TIFF, WebP, BMP, and other codec dependencies that balloon compile time and binary size. Only JPEG and PNG cover images are supported; other formats → `Degraded`.
- **GOTCHA**: Do NOT add `features = ["serialize"]` to `quick-xml`. The plan uses the event-based reader/writer API exclusively — the serde derive feature is unused and adds unnecessary compile weight.
- **VALIDATE**: `cargo check` passes. `cargo tree | grep -E "^(zip|quick-xml|encoding_rs|image)"` shows the four crates.

---

### Task 3: Define Core Types in `epub/mod.rs`

- **ACTION**: Create `backend/src/services/epub/mod.rs` with all shared types and the public entry point.
- **IMPLEMENT**:

```rust
//! EPUB structural validation and auto-repair pipeline.
//!
//! Entry point: [`validate_and_repair`]. Runs 5 sequential layers
//! (ZIP → container → OPF → XHTML → cover) and optionally re-packages
//! the archive if repairs were made.

use std::path::Path;

pub mod container_layer;
pub mod cover_layer;
pub mod opf_layer;
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
#[derive(Debug, Clone)]
pub enum IssueKind {
    /// ZIP entry contains path traversal components or absolute path.
    PathTraversal { entry_name: String },
    /// ZIP entry or aggregate uncompressed size exceeds limit.
    ZipBomb { entry_name: String, size: u64, limit: u64 },
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
    EncodingMismatch { entry_name: String, declared: String, detected: String },
    /// XML file has ambiguous encoding (conditions for safe transcode not met).
    AmbiguousEncoding { entry_name: String },
    /// XML parse error in a spine document.
    MalformedXhtml { entry_name: String, detail: String },
    /// Cover file referenced in OPF does not exist in the archive.
    MissingCover { href: String },
    /// Cover file exists but is not a decodable JPEG or PNG.
    UndecodableCover { href: String },
}

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
        });
    }

    // Layer 2: container.xml
    let opf_path = container_layer::validate(&zip_result, &mut issues);
    if issues.iter().any(|i| i.severity == Severity::Irrecoverable) {
        return Ok(ValidationReport {
            issues,
            outcome: ValidationOutcome::Quarantined,
            accessibility_metadata: None,
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
        });
    }

    let accessibility_metadata = opf_data.as_ref().and_then(|d| d.accessibility_metadata.clone());

    if has_repairable {
        let opf_path_str = opf_data.as_ref().map(|d| d.opf_path.as_str());
        repair::repackage(path, &issues, opf_path_str)?;
        return Ok(ValidationReport {
            issues,
            outcome: ValidationOutcome::Repaired,
            accessibility_metadata,
        });
    }

    let outcome = if has_degraded {
        ValidationOutcome::Degraded
    } else {
        ValidationOutcome::Clean
    };

    Ok(ValidationReport { issues, outcome, accessibility_metadata })
}
```

- **MIRROR**: THISERROR_ERROR, MODULE_STRUCTURE
- **IMPORTS**: `std::path::Path`, `serde_json::Value` (already in Cargo.toml)
- **GOTCHA**: All `pub mod` declarations in `mod.rs` must be listed before any `use` statements when modules reference each other.
- **VALIDATE**: `cargo check` — zero errors or warnings.

---

### Task 4: `zip_layer.rs` — Layer 1

- **ACTION**: Create `backend/src/services/epub/zip_layer.rs`.
- **IMPLEMENT**:

```rust
use std::io::Read;
use std::path::Path;
use zip::ZipArchive;

use super::{
    Issue, IssueKind, Layer, Severity,
    MAX_AGGREGATE_UNCOMPRESSED_BYTES, MAX_ENTRY_UNCOMPRESSED_BYTES,
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
                    kind: IssueKind::CorruptEntry { entry_name: "<archive>".to_string() },
                });
                break 'zip;
            }
        };

        let mut aggregate_size: u64 = 0;

        for i in 0..archive.len() {
            let file = archive.by_index(i)?;
            let name = file.name().to_string();

            // Path traversal check
            if name.contains("..") || name.starts_with('/') {
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

            // Extractability check — bound the read to catch lying central directories
            let mut buf = Vec::new();
            if file.take(MAX_ENTRY_UNCOMPRESSED_BYTES + 1).read_to_end(&mut buf).is_err() {
                issues.push(Issue {
                    layer: Layer::Zip,
                    severity: Severity::Irrecoverable,
                    kind: IssueKind::CorruptEntry { entry_name: name },
                });
                break 'zip;
            }

            // Detect lying central directory: buf filled to cap means actual > declared
            if buf.len() == (MAX_ENTRY_UNCOMPRESSED_BYTES + 1) as usize {
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
    let mut file = archive.by_name(entry_name).ok()?;
    let mut buf = Vec::new();
    file.take(MAX_ENTRY_UNCOMPRESSED_BYTES + 1).read_to_end(&mut buf).ok()?;
    Some(buf)
}
```

- **MIRROR**: THISERROR_ERROR
- **GOTCHA**: `ZipFile::size()` returns the *uncompressed* size from the central directory — a malicious ZIP can lie. Detect lying headers by reading with `take(MAX+1)` and checking `buf.len() == (MAX+1) as usize` after `read_to_end` succeeds: if the buffer filled to the cap, actual size exceeds declared size.
- **GOTCHA**: Use a labeled block `'zip: { ... }` so that `archive` is dropped before `bytes` is moved into the return value. Using `return Ok(ZipHandle { bytes, entries })` inside the loop fails borrow-check because `archive` borrows `&bytes[..]`. Use `break 'zip` instead and build the `Ok(...)` after the block.
- **GOTCHA**: `ZipArchive::new` failure is caught explicitly with `match` and pushed as `Severity::Irrecoverable` — do NOT propagate with `?` as that would skip the issues vec and return `Err`, causing the orchestrator to mark the file degraded without quarantining.
- **VALIDATE**: `cargo check`. Unit test: craft a ZIP with a `..` entry name, verify `Irrecoverable` issue. Unit test: pass a non-ZIP file, verify `Irrecoverable` CorruptEntry issue is pushed (not `Err` returned).

---

### Task 5: `container_layer.rs` — Layer 2

- **ACTION**: Create `backend/src/services/epub/container_layer.rs`.
- **IMPLEMENT**:

```rust
use quick_xml::events::Event;
use quick_xml::Reader;

use super::{zip_layer::{ZipHandle, read_entry}, Issue, IssueKind, Layer, Severity};

const CONTAINER_PATH: &str = "META-INF/container.xml";

/// Parse container.xml and return the OPF path.
///
/// If container.xml is missing, scans for a `.opf` file and regenerates.
/// Appends issues to `issues`. Returns `None` only if no OPF can be found at all.
pub fn validate(handle: &ZipHandle, issues: &mut Vec<Issue>) -> Option<String> {
    if let Some(bytes) = read_entry(handle, CONTAINER_PATH) {
        extract_opf_path(&bytes, issues)
    } else {
        // Attempt regeneration: scan for .opf file
        let candidate = handle.entries.iter().find(|e| e.ends_with(".opf")).cloned();

        issues.push(Issue {
            layer: Layer::Container,
            severity: Severity::Repaired,
            kind: IssueKind::MissingContainer {
                opf_candidate: candidate.clone(),
            },
        });

        candidate.as_ref().and_then(|c| {
            // Validate path safety of the discovered OPF
            if c.contains("..") || c.starts_with('/') {
                issues.push(Issue {
                    layer: Layer::Container,
                    severity: Severity::Irrecoverable,
                    kind: IssueKind::UnsafeOpfPath { path: c.clone() },
                });
                None
            } else {
                Some(c.clone())
            }
        })
    }
}

/// Extract the OPF `full-path` attribute from container.xml bytes.
fn extract_opf_path(bytes: &[u8], issues: &mut Vec<Issue>) -> Option<String> {
    let xml = std::str::from_utf8(bytes).ok()?;
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    loop {
        match reader.read_event().ok()? {
            Event::Empty(e) | Event::Start(e) if e.name().as_ref() == b"rootfile" => {
                if let Some(path) = e.attributes().flatten().find(|a| {
                    a.key.as_ref() == b"full-path"
                }) {
                    let raw = std::str::from_utf8(&path.value).ok()?.to_string();
                    // Path safety check
                    if raw.contains("..") || raw.starts_with('/') {
                        issues.push(Issue {
                            layer: Layer::Container,
                            severity: Severity::Irrecoverable,
                            kind: IssueKind::UnsafeOpfPath { path: raw },
                        });
                        return None;
                    }
                    return Some(raw);
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    None
}
```

- **MIRROR**: THISERROR_ERROR
- **GOTCHA**: `quick-xml`'s `read_event()` is fallible but returns `Result`. Use `.ok()?` or match explicitly. The `?` operator propagates errors; using `ok()?` silently stops on XML error (appropriate here since container.xml is small and errors mean it's malformed).
- **VALIDATE**: `cargo check`. Unit test: ZIP with no `container.xml` but one `.opf` file → `Repaired` issue, OPF path returned.

---

### Task 6: `opf_layer.rs` — Layer 3

- **ACTION**: Create `backend/src/services/epub/opf_layer.rs`.
- **IMPLEMENT**: Parse the OPF, extract spine/manifest, validate spine references, validate manifest hrefs, extract accessibility metadata.

```rust
use quick_xml::events::Event;
use quick_xml::Reader;
use std::collections::{HashMap, HashSet};

use super::{zip_layer::{ZipHandle, read_entry}, Issue, IssueKind, Layer, Severity};

pub struct OpfData {
    /// All manifest items: id → href
    pub manifest: HashMap<String, String>,
    /// Spine idrefs (after removing broken refs)
    pub spine_idrefs: Vec<String>,
    /// OPF path within the archive (needed by repair and other layers)
    pub opf_path: String,
    /// Raw W3C accessibility metadata from `<meta>` elements, if any
    pub accessibility_metadata: Option<serde_json::Value>,
}

/// Validate the OPF file. Returns `None` if OPF cannot be read.
pub fn validate(
    handle: &ZipHandle,
    opf_path: Option<&str>,
    issues: &mut Vec<Issue>,
) -> Option<OpfData> {
    let path = opf_path?;
    let bytes = read_entry(handle, path)?;
    let xml = std::str::from_utf8(&bytes).ok()?;

    let mut manifest: HashMap<String, String> = HashMap::new();
    let mut spine_idrefs: Vec<String> = Vec::new();
    let mut accessibility_meta: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();

    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    loop {
        match reader.read_event().ok()? {
            // EPUB 3 text-content meta: <meta property="schema:accessMode">textual</meta>
            // Must come BEFORE general Event::Start arm to avoid shadowing.
            Event::Start(e) if e.name().as_ref() == b"meta" => {
                let e = e.into_owned(); // release reader buffer borrow before read_text
                let prop = e.attributes().flatten()
                    .find(|a| a.key.as_ref() == b"property")
                    .and_then(|a| std::str::from_utf8(&a.value).ok().map(|s| s.to_string()));
                let content_attr = e.attributes().flatten()
                    .find(|a| a.key.as_ref() == b"content")
                    .and_then(|a| std::str::from_utf8(&a.value).ok().map(|s| s.to_string()));
                if let Some(prop) = prop {
                    if prop.starts_with("schema:access") || prop.starts_with("dcterms:") {
                        let val = content_attr.or_else(|| {
                            reader.read_text(e.name()).ok()
                                .map(|t| t.trim().to_string())
                                .filter(|s| !s.is_empty())
                        });
                        if let Some(v) = val {
                            accessibility_meta.insert(prop, serde_json::Value::String(v));
                        }
                    }
                }
            }
            // EPUB 2 attribute-style meta: <meta name="..." content="..."/>
            Event::Empty(e) if e.name().as_ref() == b"meta" => {
                let prop = e.attributes().flatten()
                    .find(|a| a.key.as_ref() == b"property")
                    .and_then(|a| std::str::from_utf8(&a.value).ok().map(|s| s.to_string()));
                let content = e.attributes().flatten()
                    .find(|a| a.key.as_ref() == b"content")
                    .and_then(|a| std::str::from_utf8(&a.value).ok().map(|s| s.to_string()));
                if let Some(prop) = prop {
                    if prop.starts_with("schema:access") || prop.starts_with("dcterms:") {
                        if let Some(v) = content {
                            accessibility_meta.insert(prop, serde_json::Value::String(v));
                        }
                    }
                }
            }
            // General arm — meta already handled by guarded arms above
            Event::Empty(e) | Event::Start(e) => match e.name().as_ref() {
                b"item" => {
                    let attrs: HashMap<String, String> = e
                        .attributes()
                        .flatten()
                        .filter_map(|a| {
                            let k = std::str::from_utf8(a.key.as_ref()).ok()?.to_string();
                            let v = std::str::from_utf8(&a.value).ok()?.to_string();
                            Some((k, v))
                        })
                        .collect();

                    if let (Some(id), Some(href)) = (attrs.get("id"), attrs.get("href")) {
                        // Validate href path safety
                        if href.contains("..") || href.starts_with('/') {
                            issues.push(Issue {
                                layer: Layer::Opf,
                                severity: Severity::Degraded,
                                kind: IssueKind::UnsafeManifestHref { href: href.clone() },
                            });
                        } else {
                            manifest.insert(id.clone(), href.clone());
                        }
                    }
                }
                b"itemref" => {
                    if let Some(idref) = e.attributes().flatten().find(|a| {
                        a.key.as_ref() == b"idref"
                    }) {
                        if let Ok(v) = std::str::from_utf8(&idref.value) {
                            spine_idrefs.push(v.to_string());
                        }
                    }
                }
                _ => {}
            },
            Event::Eof => break,
            _ => {}
        }
    }

    // Validate spine refs against manifest
    let manifest_ids: HashSet<&String> = manifest.keys().collect();
    let mut valid_spine: Vec<String> = Vec::new();
    for idref in &spine_idrefs {
        if manifest_ids.contains(idref) {
            valid_spine.push(idref.clone());
        } else {
            issues.push(Issue {
                layer: Layer::Opf,
                severity: Severity::Repaired,
                kind: IssueKind::BrokenSpineRef { idref: idref.clone() },
            });
        }
    }

    let accessibility_metadata = if accessibility_meta.is_empty() {
        None
    } else {
        Some(serde_json::Value::Object(accessibility_meta))
    };

    Some(OpfData {
        manifest,
        spine_idrefs: valid_spine,
        opf_path: path.to_string(),
        accessibility_metadata,
    })
}
```

- **GOTCHA**: `quick-xml`'s attribute iterator yields `Result<Attribute, _>`; use `.flatten()` to skip malformed ones silently.
- **GOTCHA**: EPUB 3 text-content style `<meta property="schema:accessMode">textual</meta>` requires `e.into_owned()` BEFORE calling `reader.read_text(e.name())`. Without `into_owned()`, the `BytesStart<'_>` borrows the reader's internal buffer, making `reader.read_text()` impossible (simultaneous mutable + immutable borrow). Guarded match arms (`Event::Start(e) if e.name().as_ref() == b"meta"`) must appear BEFORE the general `Event::Start(e)` arm to avoid the general arm shadowing the specific one.
- **VALIDATE**: Unit test: OPF with one broken spine ref → issue added, valid spine has one fewer entry. Unit test: OPF with EPUB 3 `<meta property="schema:accessMode">textual</meta>` → accessibility_metadata contains `{"schema:accessMode": "textual"}`.

---

### Task 7: `xhtml_layer.rs` — Layer 4

- **ACTION**: Create `backend/src/services/epub/xhtml_layer.rs`.
- **IMPLEMENT**: Validate up to 500 spine XHTML documents. Apply conservative encoding repair rule.

```rust
use quick_xml::Reader;

use super::{
    zip_layer::{ZipHandle, read_entry},
    opf_layer::OpfData,
    Issue, IssueKind, Layer, Severity, MAX_SPINE_ITEMS,
};

/// Validate XHTML spine documents.
pub fn validate(handle: &ZipHandle, opf_data: Option<&OpfData>, issues: &mut Vec<Issue>) {
    let Some(opf) = opf_data else { return };

    if opf.spine_idrefs.len() > MAX_SPINE_ITEMS {
        issues.push(Issue {
            layer: Layer::Xhtml,
            severity: Severity::Degraded,
            kind: IssueKind::SpineCapExceeded { count: opf.spine_idrefs.len() },
        });
        return;
    }

    // Determine base path from OPF path (for resolving relative hrefs)
    let opf_dir = opf.opf_path
        .rfind('/')
        .map(|i| &opf.opf_path[..i])
        .unwrap_or("");

    for idref in &opf.spine_idrefs {
        let Some(href) = opf.manifest.get(idref) else { continue };
        let entry_path = if opf_dir.is_empty() {
            href.clone()
        } else {
            format!("{opf_dir}/{href}")
        };

        let Some(bytes) = read_entry(handle, &entry_path) else { continue };

        validate_xhtml_document(&bytes, &entry_path, issues);
    }
}

fn validate_xhtml_document(bytes: &[u8], entry_name: &str, issues: &mut Vec<Issue>) {
    // Conservative encoding repair rule:
    // Only transcode if ALL THREE conditions hold:
    // (a) XML declaration or BOM explicitly declares a non-UTF-8 encoding
    // (b) file fails UTF-8 parse
    // (c) decoding under declared encoding succeeds cleanly

    let declared_encoding = detect_declared_encoding(bytes);

    // Condition (b): try UTF-8 parse
    let utf8_ok = std::str::from_utf8(bytes).is_ok();

    if !utf8_ok {
        if let Some(enc_label) = &declared_encoding {
            // Condition (a): declared encoding present. Try condition (c).
            if let Some(encoding) = encoding_rs::Encoding::for_label(enc_label.as_bytes()) {
                let (decoded, _enc, had_errors) = encoding.decode(bytes);
                if !had_errors {
                    // All three conditions met: emit Repaired
                    issues.push(Issue {
                        layer: Layer::Xhtml,
                        severity: Severity::Repaired,
                        kind: IssueKind::EncodingMismatch {
                            entry_name: entry_name.to_string(),
                            declared: enc_label.clone(),
                            detected: "UTF-8".to_string(),
                        },
                    });
                    // Validate the decoded content as XML
                    validate_xml_parse(decoded.as_bytes(), entry_name, issues);
                    return;
                }
            }
        }
        // Conditions not fully met → Degraded, do not transcode
        issues.push(Issue {
            layer: Layer::Xhtml,
            severity: Severity::Degraded,
            kind: IssueKind::AmbiguousEncoding { entry_name: entry_name.to_string() },
        });
        return;
    }

    validate_xml_parse(bytes, entry_name, issues);
}

/// Parse XML and report structural errors as Degraded issues.
fn validate_xml_parse(bytes: &[u8], entry_name: &str, issues: &mut Vec<Issue>) {
    let Ok(xml) = std::str::from_utf8(bytes) else { return };
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    loop {
        match reader.read_event() {
            Ok(quick_xml::events::Event::Eof) => break,
            Err(e) => {
                issues.push(Issue {
                    layer: Layer::Xhtml,
                    severity: Severity::Degraded,
                    kind: IssueKind::MalformedXhtml {
                        entry_name: entry_name.to_string(),
                        detail: e.to_string(),
                    },
                });
                break;
            }
            _ => {}
        }
    }
}

/// Extract encoding declared in XML declaration (`<?xml ... encoding="..." ?>`) or BOM.
fn detect_declared_encoding(bytes: &[u8]) -> Option<String> {
    // BOM detection
    if bytes.starts_with(b"\xFF\xFE") { return Some("UTF-16LE".to_string()); }
    if bytes.starts_with(b"\xFE\xFF") { return Some("UTF-16BE".to_string()); }

    // XML declaration: look for encoding="..." in first 200 bytes
    let prefix = std::str::from_utf8(&bytes[..bytes.len().min(200)]).ok()?;
    let decl_start = prefix.find("<?xml")?;
    let decl_end = prefix[decl_start..].find("?>")?;
    let decl = &prefix[decl_start..decl_start + decl_end + 2];

    let enc_start = decl.find("encoding=\"").or_else(|| decl.find("encoding='"))?;
    let after = &decl[enc_start + 10..];
    let quote_char = decl.chars().nth(enc_start + 9)?;
    let enc_end = after.find(quote_char)?;
    Some(after[..enc_end].to_string())
}
```

- **GOTCHA**: `encoding_rs::Encoding::decode` returns `(Cow<str>, &Encoding, bool)` — the third field `had_errors` is `true` if replacement characters were inserted. Only proceed if `had_errors == false`.
- **GOTCHA**: `quick-xml` returns parse errors from `read_event()` but continues on recoverable errors. A single `Err` variant means an unrecoverable XML structural issue. Check this in the loop.
- **VALIDATE**: Unit test: Latin-1 bytes with `<?xml encoding="ISO-8859-1"?>`, non-UTF-8 bytes → `Repaired` issue. Unit test: non-UTF-8 bytes with no declaration → `AmbiguousEncoding` (Degraded).

---

### Task 8: `cover_layer.rs` — Layer 5

- **ACTION**: Create `backend/src/services/epub/cover_layer.rs`.
- **IMPLEMENT**:

```rust
use super::{
    zip_layer::{ZipHandle, read_entry},
    opf_layer::OpfData,
    Issue, IssueKind, Layer, Severity,
};

/// Validate the cover image (JPEG or PNG only).
pub fn validate(handle: &ZipHandle, opf_data: Option<&OpfData>, issues: &mut Vec<Issue>) {
    let Some(opf) = opf_data else { return };

    let cover_href = find_cover_href(opf);
    let Some(href) = cover_href else { return }; // No cover declared — not an error

    let opf_dir = opf.opf_path
        .rfind('/')
        .map(|i| &opf.opf_path[..i])
        .unwrap_or("");
    let entry_path = if opf_dir.is_empty() {
        href.clone()
    } else {
        format!("{opf_dir}/{href}")
    };

    let Some(bytes) = read_entry(handle, &entry_path) else {
        issues.push(Issue {
            layer: Layer::Cover,
            severity: Severity::Degraded,
            kind: IssueKind::MissingCover { href: href.clone() },
        });
        return;
    };

    // Attempt to decode as JPEG or PNG. Other formats → Degraded.
    // image crate compiled with default-features = false, features = ["jpeg", "png"] only.
    match image::load_from_memory(&bytes) {
        Ok(_) => {} // decodable — no issue
        Err(_) => {
            issues.push(Issue {
                layer: Layer::Cover,
                severity: Severity::Degraded,
                kind: IssueKind::UndecodableCover { href: href.clone() },
            });
        }
    }
}

/// Find the cover image href from OPF manifest/metadata.
/// Checks: manifest item with `properties="cover-image"` (EPUB 3),
/// then manifest item with `id="cover-image"` or `id="cover"`.
fn find_cover_href(opf: &OpfData) -> Option<String> {
    // Check manifest for cover-image id (simple heuristic)
    for id in &["cover-image", "cover", "Cover", "Cover-Image"] {
        if let Some(href) = opf.manifest.get(*id) {
            return Some(href.clone());
        }
    }
    None
}
```

- **GOTCHA**: `image::load_from_memory` will only attempt JPEG and PNG decode since those are the only features compiled in. If the file is a WebP or BMP, it will return an `Err` — this correctly maps to `Degraded`.
- **GOTCHA**: The cover detection heuristic above is minimal. Real EPUBs use `<meta name="cover" content="cover-image-id"/>` in EPUB 2 or `properties="cover-image"` attribute in EPUB 3. The OPF layer would need to be extended to parse these fully. For Step 5, the id-based lookup is sufficient.
- **VALIDATE**: Unit test: ZIP with a cover entry that is valid PNG bytes → no issue. ZIP with cover entry containing random bytes → `UndecodableCover`.

---

### Task 9: `repair.rs` — Repair Orchestration

- **ACTION**: Create `backend/src/services/epub/repair.rs`.
- **IMPLEMENT**:

```rust
use std::io::{Read, Write};
use std::path::Path;
use tempfile::NamedTempFile;
use zip::write::{ExtendedFileOptions, FileOptions};
use zip::{ZipArchive, ZipWriter};

use super::{EpubError, Issue, IssueKind, Severity};

const MIMETYPE_ENTRY: &str = "mimetype";
const MIMETYPE_CONTENT: &[u8] = b"application/epub+zip";

/// Re-package the EPUB at `path` applying all `Repaired`-severity issues.
///
/// Writes to a temp file in the same directory, then `rename()`s over `path`
/// atomically. If re-packaging fails, `path` is left untouched.
pub fn repackage(path: &Path, issues: &[Issue], opf_path: Option<&str>) -> Result<(), EpubError> {
    let dir = path.parent().unwrap_or(Path::new("."));
    let bytes = std::fs::read(path)?;

    let broken_refs: Vec<String> = issues.iter().filter_map(|i| {
        if let IssueKind::BrokenSpineRef { idref } = &i.kind { Some(idref.clone()) } else { None }
    }).collect();

    let encoding_fixes: Vec<(String, String)> = issues.iter().filter_map(|i| {
        if let IssueKind::EncodingMismatch { entry_name, declared, .. } = &i.kind {
            Some((entry_name.clone(), declared.clone()))
        } else { None }
    }).collect();

    let missing_container = issues.iter().any(|i| {
        matches!(&i.kind, IssueKind::MissingContainer { .. })
    });

    let opf_candidate: Option<String> = issues.iter().find_map(|i| {
        if let IssueKind::MissingContainer { opf_candidate } = &i.kind {
            opf_candidate.clone()
        } else { None }
    });

    // Pre-compute the rewritten OPF bytes (if needed) before opening the archive,
    // so the borrow on `bytes` can be dropped before we open the ZipArchive.
    let rewritten_opf: Option<(String, Vec<u8>)> = if !broken_refs.is_empty() {
        if let Some(opf) = opf_path {
            let cursor = std::io::Cursor::new(&bytes[..]);
            let mut ar = ZipArchive::new(cursor)?;
            let mut opf_bytes = Vec::new();
            ar.by_name(opf)?.read_to_end(&mut opf_bytes)?;
            let rewritten = rewrite_opf_remove_broken_spine(&opf_bytes, &broken_refs);
            Some((opf.to_string(), rewritten))
        } else {
            None
        }
    } else {
        None
    };

    // Build new ZIP into temp file
    let temp = NamedTempFile::new_in(dir)?;
    {
        let cursor = std::io::Cursor::new(&bytes[..]);
        let mut archive = ZipArchive::new(cursor)?;
        let mut writer = ZipWriter::new(&temp);

        // mimetype MUST be first and stored (not deflated) per EPUB spec
        let stored: FileOptions<ExtendedFileOptions> = FileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        writer.start_file(MIMETYPE_ENTRY, stored)?;
        writer.write_all(MIMETYPE_CONTENT)?;

        // Copy all entries except mimetype, applying fixes
        for i in 0..archive.len() {
            let mut file = archive.by_index(i)?;
            let name = file.name().to_string();

            if name == MIMETYPE_ENTRY { continue; } // already written

            let mut entry_bytes = Vec::new();
            file.read_to_end(&mut entry_bytes)?;

            // Use rewritten OPF bytes if this is the OPF entry
            let final_bytes = if let Some((ref opf_name, ref rewritten)) = rewritten_opf {
                if &name == opf_name {
                    rewritten.clone()
                } else {
                    entry_bytes
                }
            } else if let Some((_, declared_enc)) = encoding_fixes.iter()
                .find(|(n, _)| n == &name)
            {
                transcode_to_utf8(&entry_bytes, declared_enc).unwrap_or(entry_bytes)
            } else {
                entry_bytes
            };

            let options: FileOptions<ExtendedFileOptions> = FileOptions::default();
            writer.start_file(&name, options)?;
            writer.write_all(&final_bytes)?;
        }

        // Add regenerated container.xml if it was missing
        if missing_container {
            if let Some(opf_path) = &opf_candidate {
                let container_xml = generate_container_xml(opf_path);
                let options: FileOptions<ExtendedFileOptions> = FileOptions::default();
                writer.start_file("META-INF/container.xml", options)?;
                writer.write_all(container_xml.as_bytes())?;
            }
        }

        writer.finish()?;
    }

    // Atomic rename over destination
    temp.persist(path).map_err(EpubError::TempFile)?;
    Ok(())
}

/// Rewrite OPF XML removing `<itemref>` elements whose `idref` is in `broken_refs`.
fn rewrite_opf_remove_broken_spine(opf_bytes: &[u8], broken_refs: &[String]) -> Vec<u8> {
    let xml = match std::str::from_utf8(opf_bytes) {
        Ok(s) => s,
        Err(_) => return opf_bytes.to_vec(),
    };
    let mut reader = quick_xml::Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut output = quick_xml::Writer::new(Vec::new());
    let mut skip_itemref = false;
    loop {
        match reader.read_event() {
            Ok(quick_xml::events::Event::Empty(e)) if e.name().as_ref() == b"itemref" => {
                let idref = e.attributes().flatten()
                    .find(|a| a.key.as_ref() == b"idref")
                    .and_then(|a| std::str::from_utf8(&a.value).ok().map(|s| s.to_string()));
                if idref.as_deref().map_or(false, |id| broken_refs.iter().any(|r| r == id)) {
                    continue;
                }
                let _ = output.write_event(quick_xml::events::Event::Empty(e.into_owned()));
            }
            Ok(quick_xml::events::Event::Start(e)) if e.name().as_ref() == b"itemref" => {
                let idref = e.attributes().flatten()
                    .find(|a| a.key.as_ref() == b"idref")
                    .and_then(|a| std::str::from_utf8(&a.value).ok().map(|s| s.to_string()));
                if idref.as_deref().map_or(false, |id| broken_refs.iter().any(|r| r == id)) {
                    skip_itemref = true;
                } else {
                    let _ = output.write_event(quick_xml::events::Event::Start(e.into_owned()));
                }
            }
            Ok(quick_xml::events::Event::End(e)) if e.name().as_ref() == b"itemref" => {
                if skip_itemref { skip_itemref = false; }
                else { let _ = output.write_event(quick_xml::events::Event::End(e.into_owned())); }
            }
            Ok(quick_xml::events::Event::Eof) => break,
            Ok(e) => {
                if !skip_itemref { let _ = output.write_event(e.into_owned()); }
            }
            Err(_) => return opf_bytes.to_vec(),
        }
    }
    output.into_inner()
}

fn transcode_to_utf8(bytes: &[u8], declared_enc: &str) -> Option<Vec<u8>> {
    let encoding = encoding_rs::Encoding::for_label(declared_enc.as_bytes())?;
    let (decoded, _, had_errors) = encoding.decode(bytes);
    if had_errors { return None; }

    // Re-encode as UTF-8: replace encoding declaration with UTF-8
    let utf8_str = decoded.replace(
        &format!("encoding=\"{declared_enc}\""),
        "encoding=\"UTF-8\"",
    );
    Some(utf8_str.into_bytes())
}

fn generate_container_xml(opf_path: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles>
    <rootfile full-path="{opf_path}" media-type="application/oebps-package+xml"/>
  </rootfiles>
</container>"#
    )
}
```

- **MIRROR**: ATOMIC_WRITE
- **GOTCHA**: `NamedTempFile::new_in(dir)` creates the temp file in the same directory as `path`. This is required for `rename()` to be atomic (same filesystem). Creating in `/tmp` may be a different filesystem and would fall back to copy+delete.
- **GOTCHA**: `temp.persist(path)` consumes the `NamedTempFile`. If it fails, it returns a `PersistError` which wraps both the IO error and the original `NamedTempFile` — the temp file still exists and will be cleaned up when the error is dropped.
- **GOTCHA**: `repackage` takes `opf_path: Option<&str>` — the OPF path discovered by container_layer (or found via heuristic). This is required so `rewrite_opf_remove_broken_spine` knows which ZIP entry is the OPF. Pass `opf_data.as_ref().map(|o| o.opf_path.as_str())` from the `validate_and_repair` call site.
- **GOTCHA**: In `rewrite_opf_remove_broken_spine`, all passthrough events in the `quick-xml` write loop need `.into_owned()` to avoid borrow conflicts between the reader's internal buffer and the writer. Without it, passthrough `Event::Text` / `Event::Start` etc. borrow the reader, preventing subsequent `read_event()` calls.
- **VALIDATE**: Unit test: ZIP with path-traversal entry → `repackage` not called (quarantine path). Unit test: ZIP with missing container.xml → after repackage, archive contains `META-INF/container.xml`. Unit test: OPF with broken spine ref idref "ch2" → after `repackage`, OPF XML in archive no longer contains `idref="ch2"`. Test structural invariants: mimetype is first, stored.

---

### Task 10: Update `src/services/mod.rs`

- **ACTION**: Add `pub mod epub;` to the services module.
- **IMPLEMENT**: In `backend/src/services/mod.rs`, change:
  ```rust
  //! Business logic services.

  pub mod ingestion;
  ```
  to:
  ```rust
  //! Business logic services.

  pub mod epub;
  pub mod ingestion;
  ```
- **VALIDATE**: `cargo check` — no unresolved module errors.

---

### Task 11: Integrate Validation into `orchestrator.rs`

- **ACTION**: Add EPUB validation between the format check (Step 4) and the CTE DB insert (Step 5) in `process_file`. Thread `validation_status` through to the INSERT.
- **IMPLEMENT**:

In `orchestrator.rs`, add the import at the top:
```rust
use crate::services::epub::{self, ValidationOutcome};
```

Insertion point: AFTER `let format_str = ext.as_str();` (the line after the `!SUPPORTED_FORMATS` early-return block) and BEFORE the `// Step 5: Create work + manifestation` comment. The `ext` variable must be in scope — it is only defined in Step 4 of `process_file`. Do NOT insert before Step 4.

Add **Step 4.5** (between Step 4 and Step 5):

```rust
    // Step 4.5: EPUB structural validation and auto-repair.
    // Only applies to EPUB files; other formats pass through as 'valid'.
    let (validation_status_str, accessibility_metadata): (&'static str, Option<serde_json::Value>) =
        if ext == "epub" {
            let lib_file = library_path.join(&final_relative);
            let validation = {
                let lib_file = lib_file.clone();
                tokio::task::spawn_blocking(move || epub::validate_and_repair(&lib_file)).await
            };

            match validation {
                Ok(Ok(report)) => {
                    tracing::info!(
                        path = %lib_file.display(),
                        outcome = ?report.outcome,
                        issues = report.issues.len(),
                        "epub validation complete"
                    );
                    // Destructure before consuming report in inner match to avoid use-after-move.
                    let a11y = report.accessibility_metadata;
                    let issues = report.issues;
                    match report.outcome {
                        ValidationOutcome::Quarantined => {
                            // Remove the already-copied library file before quarantining.
                            let lib_file_str = lib_file.display().to_string();
                            let _ = tokio::task::spawn_blocking(move || {
                                if let Err(e) = std::fs::remove_file(&lib_file_str) {
                                    tracing::warn!(
                                        path = %lib_file_str,
                                        error = %e,
                                        "failed to remove library file for quarantined EPUB"
                                    );
                                }
                            })
                            .await;
                            let reason = issues
                                .iter()
                                .map(|i| format!("{:?}", i.kind))
                                .collect::<Vec<_>>()
                                .join("; ");
                            quarantine_async(&source, &quarantine_path, &reason).await;
                            return ProcessResult::Failed(format!("EPUB quarantined: {reason}"));
                        }
                        ValidationOutcome::Clean => ("valid", a11y),
                        ValidationOutcome::Repaired => ("repaired", a11y),
                        ValidationOutcome::Degraded => ("degraded", a11y),
                    }
                }
                Ok(Err(e)) => {
                    tracing::warn!(error = %e, "epub validation error; proceeding as degraded");
                    ("degraded", None)
                }
                Err(e) => return ProcessResult::Failed(format!("spawn_blocking panicked: {e}")),
            }
        } else {
            ("valid", None)
        };
```

Then update the CTE INSERT (Step 5) to include `validation_status`:
```rust
    let result = sqlx::query(
        "WITH new_work AS ( \
            INSERT INTO works (title, sort_title) VALUES ($1, $2) RETURNING id \
         ) \
         INSERT INTO manifestations \
             (work_id, format, file_path, file_hash, file_size_bytes, ingestion_status, \
              validation_status, accessibility_metadata) \
         SELECT id, $3::manifestation_format, $4, $5, $6, \
                'complete'::ingestion_status, $7::validation_status, $8 FROM new_work",
    )
    .bind(&title)
    .bind(&title)
    .bind(format_str)
    .bind(&dest_path_str)
    .bind(&copy_result.sha256)
    .bind(copy_result.file_size as i64)
    .bind(validation_status_str)          // $7 — &'static str, cast to validation_status
    .bind(accessibility_metadata)         // $8 — Option<serde_json::Value>, sqlx json feature
    .execute(pool)
    .await;
```

- **MIRROR**: SPAWN_BLOCKING_PATTERN, SQLX_ENUM_CAST, TRACING_PATTERN
- **GOTCHA**: `ext` is derived from `vars.get("ext")` — this variable is in scope at the right place. Double-check the variable binding order in the original function.
- **GOTCHA**: `validation_status_str` is a `&'static str` which is fine for `.bind()` since sqlx accepts `impl Encode`.
- **GOTCHA**: `library_path.join(&final_relative)` constructs the path to the already-copied file. This is the path the validation function receives — not the source path.
- **VALIDATE**: `cargo check`. Integration test (marked `#[ignore]`): ingest a clean EPUB → `validation_status = 'valid'`. Ingest a zero-byte `.epub` (corrupt ZIP) → quarantined, no manifestation row.

---

### Task 12: Add Migration File

Already covered in Task 1. Verify with:
```bash
sqlx migrate run --database-url "postgres://tome:tome@localhost:5433/tome_dev"
```

---

### Task 13: Write Tests

- **ACTION**: Add `#[cfg(test)]` modules to each layer file and an integration test in `orchestrator.rs`.
- **IMPLEMENT**: Tests below are representative; write them in each module file.

**`zip_layer.rs` tests** (synchronous, no `#[ignore]`):
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::epub::{IssueKind, Severity};

    fn make_epub(entries: &[(&str, &[u8])]) -> Vec<u8> {
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
        use std::io::Write;
        let bytes = make_epub(&[("../evil.xhtml", b"bad")]);
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
        use std::io::Write;
        let bytes = make_epub(&[("OEBPS/content.opf", b"<package/>")]);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.epub");
        std::fs::write(&path, &bytes).unwrap();
        let mut issues = Vec::new();
        let handle = validate(&path, &mut issues).unwrap();
        assert!(issues.is_empty());
        assert_eq!(handle.entries, vec!["OEBPS/content.opf"]);
    }
}
```

**`xhtml_layer.rs` tests**:
```rust
#[test]
fn latin1_declared_non_utf8_bytes_emits_repaired() {
    // Craft bytes with XML declaration claiming ISO-8859-1 that are not valid UTF-8
    let mut bytes: Vec<u8> = b"<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?><html/>".to_vec();
    bytes.push(0xE9); // é in Latin-1, invalid as UTF-8 continuation
    let mut issues = Vec::new();
    validate_xhtml_document(&bytes, "test.xhtml", &mut issues);
    assert!(issues.iter().any(|i| matches!(&i.kind, IssueKind::EncodingMismatch { .. })));
}

#[test]
fn non_utf8_no_declaration_emits_degraded() {
    let bytes: Vec<u8> = vec![0xE9, 0xE0, 0xF3]; // not valid UTF-8, no XML decl
    let mut issues = Vec::new();
    validate_xhtml_document(&bytes, "test.xhtml", &mut issues);
    assert!(issues.iter().any(|i| i.severity == Severity::Degraded));
}
```

**`orchestrator.rs` integration tests** (add to existing `#[cfg(test)]` block, marked `#[ignore]`):
```rust
#[tokio::test]
#[ignore]
async fn corrupt_epub_is_quarantined() {
    let pool = sqlx::PgPool::connect(&db_url()).await.expect("connect");
    let ingestion = tempfile::tempdir().unwrap();
    let library = tempfile::tempdir().unwrap();
    let quarantine = tempfile::tempdir().unwrap();

    // A zero-byte file is not a valid ZIP → quarantine
    let source = ingestion.path().join("Author - BadBook.epub");
    std::fs::write(&source, b"").unwrap();

    let config = test_config_for(
        ingestion.path().to_str().unwrap(),
        library.path().to_str().unwrap(),
        quarantine.path().to_str().unwrap(),
    );
    let result = scan_once(&config, &pool).await.unwrap();
    assert_eq!(result.failed, 1);
    assert_eq!(result.processed, 0);

    // Verify the source was quarantined
    let q_files: Vec<_> = std::fs::read_dir(quarantine.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .collect();
    assert!(!q_files.is_empty(), "expected quarantine file");
}
```

```rust
#[tokio::test]
#[ignore]
async fn epub_missing_container_is_repaired() {
    let pool = sqlx::PgPool::connect(&db_url()).await.expect("connect");
    let ingestion = tempfile::tempdir().unwrap();
    let library = tempfile::tempdir().unwrap();
    let quarantine = tempfile::tempdir().unwrap();

    // Build a valid EPUB ZIP with no META-INF/container.xml
    let opf_content = b"<package><metadata/><manifest>\
        <item id=\"x\" href=\"x.xhtml\" media-type=\"application/xhtml+xml\"/>\
        </manifest><spine><itemref idref=\"x\"/></spine></package>";
    let epub_bytes = make_epub(&[
        ("OEBPS/content.opf", opf_content as &[u8]),
        ("OEBPS/x.xhtml", b"<html/>" as &[u8]),
    ]);
    let source = ingestion.path().join("Author - GoodBook.epub");
    std::fs::write(&source, &epub_bytes).unwrap();

    let config = test_config_for(
        ingestion.path().to_str().unwrap(),
        library.path().to_str().unwrap(),
        quarantine.path().to_str().unwrap(),
    );
    let result = scan_once(&config, &pool).await.unwrap();
    assert_eq!(result.processed, 1);
    assert_eq!(result.failed, 0);

    let dest = library.path().join("Author/GoodBook.epub");
    let status: Option<String> = sqlx::query_scalar(
        "SELECT validation_status::text FROM manifestations WHERE file_path = $1"
    )
    .bind(dest.to_str().unwrap())
    .fetch_optional(&pool).await.unwrap();
    assert_eq!(status.as_deref(), Some("repaired"));

    cleanup_test_data(&pool, dest.to_str().unwrap(), source.to_str().unwrap()).await;
}
```

- **VALIDATE**: `cargo test` (unit tests pass without DB). `cargo test -- --ignored` for DB tests.

---

### Task 14: Surface Accessibility Metadata

- **ACTION**: Verify the full pipeline end-to-end — no code changes needed at this step; Task 11 already extracts `accessibility_metadata` from the `ValidationReport` and binds it as `$8` in the CTE INSERT.
- **How it flows**: `opf_layer::validate` calls `collect_accessibility_meta` → populates `ValidationReport.accessibility_metadata: Option<serde_json::Value>` → Task 11 destructures `let a11y = report.accessibility_metadata` before the outcome match → bound as `$8` in the INSERT.
- **GOTCHA**: sqlx's `json` feature (already present in `Cargo.toml`) allows binding `Option<serde_json::Value>` directly. Do NOT convert to `String` first — that produces escaped JSON text, not JSONB.
- **GOTCHA**: DESIGN_BRIEF §5.3: accessibility metadata is **read-only**. Never auto-inject or synthesize WCAG conformance claims — store exactly what the OPF declares.
- **VALIDATE**: Ingest an EPUB with `<meta property="schema:accessMode">textual</meta>` in OPF; verify `accessibility_metadata` column is populated in the DB. Ingest an EPUB without accessibility metadata; verify the column is NULL.

---

## Testing Strategy

### Unit Tests

| Test | Input | Expected Output | Edge Case? |
|---|---|---|---|
| Path traversal entry | ZIP with `../evil.xhtml` entry | `Irrecoverable` + `PathTraversal` issue | Yes |
| ZIP bomb single entry | ZIP with entry `size()` = 600 MB | `Irrecoverable` + `ZipBomb` issue | Yes |
| Corrupt ZIP | Zero-byte file | `Irrecoverable` + `CorruptEntry` issue, `Quarantined` outcome | Yes |
| Clean EPUB | Valid ZIP structure | No issues, `Clean` outcome | No |
| Missing container.xml | ZIP with only `.opf` | `Repaired` + `MissingContainer`, OPF path returned | Yes |
| Broken spine ref | OPF spine refs unknown manifest id | `Repaired` + `BrokenSpineRef`, valid_spine shorter | No |
| >500 spine items | OPF with 501 spine entries | `Degraded` + `SpineCapExceeded` | Yes |
| Latin-1 declared, non-UTF-8 bytes | XML with `encoding="ISO-8859-1"` | `Repaired` + `EncodingMismatch` | No |
| Non-UTF-8, no declaration | Raw Latin-1 bytes, no XML decl | `Degraded` + `AmbiguousEncoding` | Yes |
| Missing cover file | Cover href → entry not in archive | `Degraded` + `MissingCover` | No |
| Invalid cover image | Cover file present but not JPEG/PNG | `Degraded` + `UndecodableCover` | No |
| Atomic repair smoke | Repaired EPUB after repackage | `mimetype` first + uncompressed, `container.xml` present | No |

### Edge Cases Checklist

- [x] Path traversal ZIP entries (`..`, leading `/`)
- [x] ZIP bomb (oversized uncompressed entry)
- [x] Corrupt ZIP (unreadable entries)
- [x] Missing `META-INF/container.xml`
- [x] OPF path contains path traversal
- [x] >500 spine items
- [x] Encoding mismatch (declared + non-UTF-8 + clean decode)
- [x] Ambiguous encoding (non-UTF-8 with no declaration)
- [x] Cover file not in archive
- [x] Cover file not decodable as JPEG/PNG
- [x] Non-EPUB file (PDF) → `valid` (validation bypassed)
- [x] Repair writes to temp then renames (never in-place)

---

## Validation Commands

### Static Analysis

```bash
cd backend && cargo check
```
EXPECT: Zero errors, zero warnings.

```bash
cd backend && cargo clippy -- -D warnings
```
EXPECT: Zero warnings.

### Unit Tests

```bash
cd backend && cargo test services::epub
```
EXPECT: All unit tests pass. No `#[ignore]` tests run.

### Full Test Suite

```bash
cd backend && cargo test
```
EXPECT: No regressions. Existing `#[ignore]` tests remain ignored.

### Migration Validation

```bash
DATABASE_URL=postgres://tome:tome@localhost:5433/tome_dev sqlx migrate run
```
EXPECT: Migration `20260415000001_epub_validation` applied successfully.

```bash
DATABASE_URL=postgres://tome:tome@localhost:5433/tome_dev psql -c "\dT+ validation_status"
```
EXPECT: `degraded` in the enum value list.

### Manual Validation

- [ ] Ingest a real valid EPUB → `validation_status = 'valid'` in manifestations table
- [ ] Ingest a zero-byte `.epub` file → quarantined, `ingestion_jobs.status = 'failed'`, no manifestation row
- [ ] Ingest an EPUB with only `.opf` and no `META-INF/container.xml` → `validation_status = 'repaired'`
- [ ] After repackage, verify `mimetype` is first entry and uncompressed (EPUB spec requirement)

---

## Acceptance Criteria

- [ ] All tasks completed
- [ ] `cargo test services::epub` passes with no failures
- [ ] `cargo clippy -- -D warnings` passes
- [ ] Migration applies cleanly and down migration documents enum caveat
- [ ] Clean EPUB → `validation_status = 'valid'`
- [ ] Missing container.xml EPUB → `validation_status = 'repaired'`
- [ ] Broken spine ref EPUB → `validation_status = 'repaired'`
- [ ] Encoding mismatch EPUB (all 3 conditions) → `validation_status = 'repaired'`
- [ ] Corrupt ZIP EPUB → quarantined, `ingestion_jobs.status = 'failed'`
- [ ] Path-traversal ZIP → quarantined
- [ ] ZIP bomb → quarantined
- [ ] Re-packaged EPUBs pass structural check (mimetype first, container.xml present, OPF parseable)
- [ ] Accessibility metadata stored in `manifestations.accessibility_metadata`

## Completion Checklist

- [ ] Code follows module structure of `services/ingestion/`
- [ ] All file I/O in `spawn_blocking`
- [ ] Repair uses `NamedTempFile::new_in(same_dir)` + `persist()`
- [ ] Quarantine calls existing `quarantine::quarantine_file`
- [ ] `ingestion_pool` passed to validation-related DB queries (inherited from orchestrator)
- [ ] `thiserror` error types, not `anyhow`, for `EpubError`
- [ ] Tracing structured logging throughout
- [ ] No `println!` / `eprintln!`
- [ ] `#[ignore]` on all tests requiring a running database
- [ ] Self-contained — no questions needed during implementation

## Risks

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| `ALTER TYPE ADD VALUE` blocked in transaction | Mitigated | Migration fails | `-- sqlx:disable-transaction` pragma already in `.up.sql` — no action needed |
| `zip` crate API differences (v1 vs v2) | Low | Compile error | Check `ZipWriter::FileOptions` type signature in v2 |
| `quick-xml` event API breaking changes between versions | Low | Compile error | Pin to `"0.37"` and verify against docs |
| Repair of broken spine refs requires OPF XML rewrite | Mitigated | Full repair | `rewrite_opf_remove_broken_spine` implemented in `repair.rs`; broken `<itemref>` elements removed from OPF during repackage |
| Non-EPUB files in ingestion dir bypass validation cleanly | Expected | None | `validation_status = 'valid'` for non-EPUBs is correct |

## Notes

- The `validation_status` enum in the DB maps to `ValidationOutcome` as: `Clean→valid`, `Repaired→repaired`, `Degraded→degraded`, `Quarantined→(no row, ProcessResult::Failed)`.
- The migration uses `ALTER TYPE ... ADD VALUE` which cannot run inside a transaction. The `-- sqlx:disable-transaction` pragma is already the first line of the `.up.sql` file. The `IF NOT EXISTS` guard makes re-runs idempotent.
- `image` crate with `default-features = false, features = ["jpeg", "png"]` will error at compile time if any other format is referenced. This is intentional — it enforces the constraint at the type system level.
- Broken spine refs are fully repaired in Step 5: `rewrite_opf_remove_broken_spine` removes the broken `<itemref>` elements from the OPF during repackage, so the file on disk contains a valid spine after repair. Step 6 (metadata extraction) does not need to handle broken idrefs.
- DESIGN_BRIEF §5.3 forbids auto-injecting WCAG conformance statements — the accessibility metadata extraction is strictly read-only. Never write to `accessibility_metadata` except from OPF source.
