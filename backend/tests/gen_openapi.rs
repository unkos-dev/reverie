//! Drift gate for the generated OpenAPI spec (docs-as-done, UNK-370).
//!
//! The committed `docs/openapi.json` must equal what the handlers produce;
//! changing a documented response without regenerating fails here — the same
//! generate→commit→`--check` contract as `cargo sqlx prepare --check`.
//! `REGEN=1` rewrites the artifact (`REGEN=1 cargo test --test gen_openapi`).
//!
//! Coverage note: a handler wired via `routes!` without `#[utoipa::path]` is a
//! compile error, so the pilot cannot regress to an undocumented endpoint
//! without failing to build — verified by temporarily removing an annotation
//! during development.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::PathBuf;

fn artifact_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../docs/openapi.json")
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
        "openapi.json is stale — regenerate with `REGEN=1 cargo test --test gen_openapi`"
    );
}

#[test]
fn spec_is_openapi_31_with_pilot_paths() {
    let rendered = reverie_api::openapi::spec_json().expect("serialize OpenAPI spec");
    let doc: serde_json::Value = serde_json::from_str(&rendered).expect("valid JSON");

    let version = doc["openapi"].as_str().expect("openapi version string");
    assert!(
        version.starts_with("3.1"),
        "expected OpenAPI 3.1.x, got {version}"
    );

    let paths = doc["paths"].as_object().expect("paths object");
    assert!(paths.contains_key("/health"), "/health documented");
    assert!(
        paths.contains_key("/health/ready"),
        "/health/ready documented"
    );

    // The shared error schema is registered as a component.
    assert!(
        doc["components"]["schemas"].get("ProblemDetails").is_some(),
        "ProblemDetails schema present"
    );
}

/// A security requirement list is "public" when it requires no scheme: either an
/// empty array (`[]`) or a single empty requirement object (`[{}]`). Both are
/// valid OAS spellings of "no authentication", and utoipa's exact emission is
/// pinned by the byte-for-byte drift gate above — this semantic check stays
/// tolerant of which one it is.
fn requires_no_auth(security: &serde_json::Value) -> bool {
    security.as_array().is_some_and(|reqs| {
        reqs.iter()
            .all(|req| req.as_object().is_some_and(serde_json::Map::is_empty))
    })
}

#[test]
fn spec_declares_security_model() {
    let rendered = reverie_api::openapi::spec_json().expect("serialize OpenAPI spec");
    let doc: serde_json::Value = serde_json::from_str(&rendered).expect("valid JSON");

    // securitySchemes: session cookie (JSON data API) + HTTP Basic (OPDS).
    let schemes = &doc["components"]["securitySchemes"];
    assert_eq!(
        schemes["session_cookie"]["type"], "apiKey",
        "session_cookie is an apiKey scheme"
    );
    assert_eq!(schemes["session_cookie"]["in"], "cookie");
    assert_eq!(
        schemes["session_cookie"]["name"], "id",
        "cookie name matches the tower-sessions default"
    );
    assert_eq!(
        schemes["opds_basic"]["type"], "http",
        "opds_basic is an http scheme"
    );
    assert_eq!(schemes["opds_basic"]["scheme"], "basic");

    // Document-level default: every operation requires the session cookie unless
    // it overrides (deny-by-default; OWASP fail-safe). A forgotten per-op
    // annotation therefore documents-as-authed, never as-public.
    let global = doc["security"]
        .as_array()
        .expect("document-level security array");
    assert!(
        global.iter().any(|req| req.get("session_cookie").is_some()),
        "document default requires session_cookie, got {global:?}"
    );

    // The operational probes are the explicit public opt-out.
    for path in ["/health", "/health/ready"] {
        let security = &doc["paths"][path]["get"]["security"];
        assert!(
            requires_no_auth(security),
            "{path} GET must opt out of the global default (security: [] / [{{}}]), got {security:?}"
        );
    }
}
