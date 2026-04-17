//! Metadata enrichment pipeline — three-layer architecture (journal, policy,
//! canonical pointers).  See plans/BLUEPRINT.md Step 7.

pub mod cache;
pub mod confidence;
pub mod cover_download;
pub mod http;
pub mod lookup_key;
pub mod policy;
pub mod value_hash;
