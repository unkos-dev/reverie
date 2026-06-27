//! Drift gate for the generated configuration reference (docs-as-done, UNK-370).
//!
//! The committed `configuration.mdx` must equal what the schema renderer
//! produces; editing a config field's doc/default/range without regenerating
//! the page fails here — the same generate→commit→`--check` contract as
//! `cargo sqlx prepare --check`. `REGEN=1` rewrites the artifact instead of
//! asserting (`REGEN=1 cargo test --test gen_config_ref`).

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::PathBuf;

fn artifact_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../docs/src/content/docs/reference/configuration.mdx")
}

fn row<'a>(md: &'a str, var: &str) -> &'a str {
    let prefix = format!("| `{var}` |");
    md.lines()
        .find(|line| line.starts_with(&prefix))
        .unwrap_or_else(|| panic!("no table row for `{var}`"))
}

#[test]
fn config_reference_matches_committed_artifact() {
    let rendered = reverie_api::config::reference_markdown().expect("render config reference");
    let path = artifact_path();

    if std::env::var_os("REGEN").is_some() {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create reference directory");
        }
        std::fs::write(&path, &rendered).expect("write config reference");
        return;
    }

    let committed = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "read {} (run `REGEN=1 cargo test --test gen_config_ref` to create it): {e}",
            path.display()
        )
    });
    assert_eq!(
        committed, rendered,
        "config reference is stale — regenerate with `REGEN=1 cargo test --test gen_config_ref`"
    );
}

#[test]
fn required_and_secret_vars_render_correctly() {
    let md = reverie_api::config::reference_markdown().expect("render config reference");

    // A required, non-secret scalar appears by name and is marked required.
    assert!(row(&md, "DATABASE_URL").contains("| Yes |"));

    // A secret variable is documented by name, with an EMPTY default cell; its
    // value is never rendered. OIDC is conditionally required, so
    // its secret renders Conditional, not Yes.
    let secret = row(&md, "OIDC_CLIENT_SECRET");
    assert!(
        secret.contains("| Conditional |"),
        "OIDC secret is conditional"
    );
    assert!(
        secret.contains("Conditional |  |"),
        "secret default cell is empty"
    );

    // A non-required scalar keeps its real default.
    assert!(row(&md, "REVERIE_PORT").contains("`3000`"));

    // The conditional migration DSN is labelled Conditional, not Yes/No.
    assert!(row(&md, "DATABASE_URL_MIGRATION").contains("| Conditional |"));

    // The RUST_LOG alias entry still renders (ENV_MAP carries both names).
    let _ = row(&md, "RUST_LOG");
}
