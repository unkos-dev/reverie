//! Capability-based ingestion with one discovery, readiness and attempt owner.
//!
//! [`CoordinatorHandle`] submits discovery commands to [`run_watcher`].

/// Unchanged source cleanup and deletion-ancestor pruning.
pub mod cleanup;
/// Independent source acquisition and contained candidate publication.
pub mod copier;

/// Library path template rendering and filename-heuristic extraction.
pub mod path_template;

/// Filesystem watcher: debounces `notify` events and forwards batches to the orchestrator.
pub mod watcher;

mod orchestrator;

pub use orchestrator::{CoordinatorHandle, DiscoveryResult, coordinator_channel, run_watcher};
