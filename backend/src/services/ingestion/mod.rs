//! File ingestion pipeline: watch → filter → copy → validate → persist.
//!
//! Entry points are [`crate::services::ingestion::run_watcher`] (long-running
//! daemon) and [`crate::services::ingestion::scan_once`] (one-shot scan for
//! manual triggers). Internal orchestration lives in the private `orchestrator`
//! module; the public submodules expose the reusable building blocks.

/// Source-file cleanup after a successful ingestion batch.
pub mod cleanup;
/// Atomic, `SHA-256`-verified file copy from the ingestion drop-zone to the library.
pub mod copier;
/// Format-priority filter: selects the best format when multiple editions share a stem.
pub mod format_filter;
/// Library path template rendering and filename-heuristic extraction.
pub mod path_template;
/// Quarantine: moves rejected files out of the pipeline and writes a `JSON` sidecar.
pub mod quarantine;
/// Filesystem watcher: debounces `notify` events and forwards batches to the orchestrator.
pub mod watcher;

mod orchestrator;

pub use orchestrator::{ScanResult, run_watcher, scan_once};
