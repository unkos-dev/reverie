//! Metadata enrichment pipeline — three-layer architecture (journal, policy,
//! canonical pointers).  See plans/BLUEPRINT.md Step 7.

pub mod cache;
pub mod confidence;
pub mod cover_download;
pub mod dry_run;
pub mod field_lock;
pub mod http;
pub mod lookup_key;
pub mod orchestrator;
pub mod policy;
pub mod queue;
pub mod sources;
pub mod value_hash;

pub use queue::spawn_queue;
