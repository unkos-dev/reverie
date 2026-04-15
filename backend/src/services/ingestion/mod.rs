pub mod cleanup;
pub mod copier;
pub mod format_filter;
pub mod path_template;
pub mod quarantine;
pub mod watcher;

mod orchestrator;

#[allow(unused_imports)] // ScanResult is part of the public API
pub use orchestrator::{ScanResult, run_watcher, scan_once};
