//! Build script for `reverie-api`.
//!
//! Sole purpose: make Cargo rebuild the crate when a migration file changes.
//!
//! `sqlx::migrate!("./migrations")` (see `src/db.rs`) embeds the migration set
//! into the binary at compile time. Cargo only re-runs a `cargo:rerun-if-changed`
//! directive emitted from a build script — a macro expanded in library code
//! cannot register watched paths. Without this script, adding or editing a
//! migration touches no `.rs` file, so `cargo run -- migrate` keeps running a
//! stale embedded set and reports "already up to date". Watching the migrations
//! directory here forces a recompile so the embedded set stays current.

fn main() {
    println!("cargo:rerun-if-changed=migrations");
}
