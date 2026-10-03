//! Capability-based ingestion with one discovery, readiness and attempt owner.
//!
//! [`CoordinatorHandle`] submits discovery commands to [`run_watcher`].

/// Source-file cleanup after a successful ingestion batch.
pub mod cleanup;
/// Atomic, `SHA-256`-verified file copy from the ingestion drop-zone to the library.
pub mod copier;

/// Library path template rendering and filename-heuristic extraction.
pub mod path_template;

/// Filesystem watcher: debounces `notify` events and forwards batches to the orchestrator.
pub mod watcher;

mod orchestrator;

#[cfg(test)]
pub(crate) use orchestrator::scan_once;
pub use orchestrator::{CoordinatorHandle, DiscoveryResult, coordinator_channel, run_watcher};
