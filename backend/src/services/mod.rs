//! Business-logic services orchestrating the application's domain work.
//!
//! Each submodule owns one major pipeline phase or capability — the
//! ingestion watcher (`ingestion`), third-party metadata fetch +
//! cache + dry-run (`enrichment`), EPUB layered reader/writer
//! (`epub`), cover acquisition + resize (`covers`), metadata
//! drafting + sanitisation (`metadata`), and canonical-metadata flush
//! back into source files (`writeback`).

/// Cover-image extraction, resize, and on-disk cache lookup.
pub mod covers;
pub mod enrichment;
/// EPUB layered reader/writer: zip-level entry IO, container, OPF,
/// cover injection, XHTML extraction, repack, and validation/repair.
pub mod epub;
pub mod ingestion;
/// Metadata extraction, drafting, sanitisation, ISBN normalisation,
/// and the value-vs-canonical inversion helpers.
pub mod metadata;
/// Periodic reaper deleting expired rows from `tower_sessions.session`.
pub mod session_sweep;
/// Persisted settings load, save, validation, and LISTEN/NOTIFY reload.
pub mod settings;
pub mod writeback;
