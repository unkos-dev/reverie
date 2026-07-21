//! Drift gate for the committed config JSON Schema (`config.schema.json`).
//!
//! The committed artifact must equal what `reverie print-config-schema`
//! emits; editing a config field's doc, default, or shape without
//! regenerating fails here, the same generate→commit→check contract as
//! `gen_openapi` and `gen_config_ref`. `REGEN=1` rewrites the artifact
//! instead of asserting (`REGEN=1 cargo test --test gen_config_schema`).
//!
//! On mismatch the failure names the regeneration command instead of
//! dumping both documents: the diff is one regeneration away locally, and
//! schema prose printed into a CI log can trip the platform's
//! secret-pattern redaction, garbling the output.

use std::path::PathBuf;

fn artifact_path() -> PathBuf {
    // Runtime lookup, not env!(): with a shared cargo target dir a cached
    // binary can carry a compile-time path from another (possibly deleted)
    // checkout. Cargo sets the variable for every `cargo test` run.
    PathBuf::from(
        std::env::var_os("CARGO_MANIFEST_DIR")
            .unwrap_or_else(|| panic!("cargo sets CARGO_MANIFEST_DIR")),
    )
    .join("config.schema.json")
}

#[test]
fn config_schema_matches_committed_artifact() {
    let rendered = reverie_api::config_schema_json().expect("render config schema");
    let path = artifact_path();

    if std::env::var_os("REGEN").is_some() {
        std::fs::write(&path, &rendered).expect("write config schema");
        return;
    }

    let committed = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "read {} (run `REGEN=1 cargo test --test gen_config_schema` to create it): {e}",
            path.display()
        )
    });
    assert!(
        committed == rendered,
        "config.schema.json is stale: regenerate with \
         `REGEN=1 cargo test --test gen_config_schema` and commit the result; \
         if regeneration does not converge, the test binary itself is stale (shared target dir): \
         force a rebuild with `cargo clean -p reverie-api`"
    );
}
