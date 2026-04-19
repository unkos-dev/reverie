//! Background metadata writeback to managed EPUB files.
//!
//! Triggered by canonical pointer moves (Step 7's `apply_field` and the
//! accept/revert routes) via the `writeback_jobs` queue.  The worker
//! processes jobs outside any user-facing transaction: it rewrites the OPF,
//! embeds a new cover if needed, re-validates the EPUB, rolls back on
//! regression, and updates `manifestations.current_file_hash` on success.
//!
//! Memory-instinct: every canonical pointer move MUST enqueue exactly one
//! `writeback_jobs` row inside the same transaction that mutates the
//! pointer.  The worker handles deduplication.

pub mod cover_embed;
pub mod error;
pub mod events;
pub mod opf_rewrite;
pub mod orchestrator;
pub mod path_rename;
pub mod queue;

pub use queue::spawn_worker;
