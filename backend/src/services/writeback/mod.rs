//! Background metadata writeback to managed `EPUB` files.
//!
//! Triggered by canonical pointer moves (Step 7's `apply_field` and the
//! accept/revert routes) via the `writeback_jobs` queue.  The worker
//! processes jobs outside any user-facing transaction: it rewrites the `OPF`,
//! embeds a new cover if needed, re-validates the `EPUB`, rolls back on
//! regression, and updates `manifestations.current_file_hash` on success.
//!
//! Memory-instinct: every canonical pointer move MUST enqueue exactly one
//! `writeback_jobs` row inside the same transaction that mutates the
//! pointer.  The worker handles deduplication.

/// `EPUB` cover-image replacement and insertion planning.
pub mod cover_embed;
/// Module-boundary error type for the writeback pipeline.
pub mod error;
/// Webhook terminal-event dispatch: stable event ids + dedupe (delivery
/// stub until Step 12).
pub mod events;
/// Pure-function `OPF` XML metadata rewriter.
pub mod opf_rewrite;
/// Per-job writeback orchestrator: load → transform → repack → rename → validate.
pub mod orchestrator;
/// Atomic on-disk rename helpers with cross-filesystem fallback.
pub mod path_rename;
/// Background queue worker: claim, execute, and bookkeep `writeback_jobs` rows.
pub mod queue;

/// Start the background writeback queue worker.  See [`queue::spawn_worker`].
pub use queue::spawn_worker;
