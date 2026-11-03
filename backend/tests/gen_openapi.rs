//! Drift gate for the generated OpenAPI spec.
//!
//! The committed `backend/openapi.json` must equal what the handlers produce;
//! changing a documented response without regenerating fails here — the same
//! generate→commit→`--check` contract as `cargo sqlx prepare --check`.
//! `REGEN=1` rewrites the artifact (`REGEN=1 cargo test --test gen_openapi`).
//!
//! Coverage note: a handler wired via `routes!` without `#[utoipa::path]` is a
//! compile error, so the pilot cannot regress to an undocumented endpoint
//! without failing to build — verified by temporarily removing an annotation
//! during development.

use std::path::PathBuf;

fn artifact_path() -> PathBuf {
    // Runtime lookup, not env!(): with a shared cargo target dir a cached
    // binary can carry a compile-time path from another (possibly deleted)
    // checkout. Cargo sets the variable for every `cargo test` run.
    PathBuf::from(
        std::env::var_os("CARGO_MANIFEST_DIR")
            .unwrap_or_else(|| panic!("cargo sets CARGO_MANIFEST_DIR")),
    )
    .join("openapi.json")
}

#[test]
fn openapi_spec_matches_committed_artifact() {
    let rendered = reverie_api::openapi::spec_json().expect("serialize OpenAPI spec");
    let path = artifact_path();

    if std::env::var_os("REGEN").is_some() {
        std::fs::write(&path, &rendered).expect("write openapi.json");
        return;
    }

    let committed = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "read {} (run `REGEN=1 cargo test --test gen_openapi` to create it): {e}",
            path.display()
        )
    });
    assert_eq!(
        committed, rendered,
        "openapi.json is stale — regenerate with `REGEN=1 cargo test --test gen_openapi`; \
         if regeneration does not converge, the test binary itself is stale (shared target dir): \
         force a rebuild with `cargo clean -p reverie-api`"
    );
}
