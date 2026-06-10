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

/// Returns `true` when the operation declares no authentication requirement.
/// OAS 3.1's canonical opt-out is `[{}]` — a single empty requirement object —
/// which is what utoipa emits (pinned exactly by the byte-for-byte drift gate
/// above). A bare `[]` also passes here, via the vacuous `all(…)` on an empty
/// array — an implementation tolerance against emitter changes, not an OAS
/// equivalence.
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

    // Hard-rule-6: both schemes must document the HTTPS-in-production requirement
    // (Basic credentials / session cookies are cleartext-exposed otherwise).
    for scheme in ["session_cookie", "opds_basic"] {
        let description = schemes[scheme]["description"].as_str().unwrap_or("");
        assert!(
            description.contains("HTTPS"),
            "{scheme} must document the HTTPS requirement, got {description:?}"
        );
    }

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

#[test]
fn spec_covers_library_routes() {
    let rendered = reverie_api::openapi::spec_json().expect("serialize OpenAPI spec");
    let doc: serde_json::Value = serde_json::from_str(&rendered).expect("valid JSON");

    // All four library data routes are documented.
    let paths = doc["paths"].as_object().expect("paths object");
    for path in [
        "/api/v1/books",
        "/api/v1/books/{id}",
        "/api/v1/works/{id}",
        "/api/v1/search",
    ] {
        assert!(paths.contains_key(path), "{path} documented");
    }

    // Authed routes inherit the document-level session_cookie default — they must
    // NOT carry an operation-level `security` key (inherit-by-omission, the
    // deny-by-default contract; only public ops opt out). See `SecurityAddon`.
    assert!(
        doc["paths"]["/api/v1/books"]["get"]
            .get("security")
            .is_none(),
        "GET /api/v1/books must inherit the global security default (no op-level security)"
    );

    // Response DTO schemas are registered as components (auto-collected via routes!).
    let schemas = &doc["components"]["schemas"];
    for schema in [
        "BookListResponse",
        // BookListRow must be a standalone component for the `created_at` guard
        // below to be non-vacuous: if utoipa stopped emitting it, `schemas["BookListRow"]`
        // would be null and the guard would pass against nothing. Assert presence here
        // so that regression fails loudly first.
        "BookListRow",
        "BookDetail",
        "WorkDetail",
        "SearchResponse",
        // SortMode is referenced by the list `?sort=` param via `$ref`; it must be
        // registered as a component or the docs-site `$ref` parse fails (the
        // byte-drift gate does not catch a dangling-but-consistent ref).
        "SortMode",
    ] {
        assert!(
            schemas.get(schema).is_some(),
            "{schema} schema component present"
        );
    }

    // The detail route documents its 404 (RLS-hidden / missing) against ProblemDetails.
    assert!(
        doc["paths"]["/api/v1/books/{id}"]["get"]["responses"]
            .get("404")
            .is_some(),
        "GET /api/v1/books/{{id}} documents a 404 response"
    );

    // Edge guard (a): `created_at` is `#[serde(skip)]`/`#[schema(ignore)]` on
    // BookListRow — it must not leak into the documented schema.
    let book_list_row_props = &schemas["BookListRow"]["properties"];
    assert!(
        book_list_row_props.get("created_at").is_none(),
        "BookListRow schema must not expose created_at (serde-skipped cursor key)"
    );

    // Edge guard (b): the list 200 response documents the RFC 8288 Link header.
    assert!(
        doc["paths"]["/api/v1/books"]["get"]["responses"]["200"]["headers"]
            .get("Link")
            .is_some(),
        "GET /api/v1/books 200 must document the Link pagination header"
    );
}

#[test]
fn spec_covers_series_dashboard_routes() {
    let rendered = reverie_api::openapi::spec_json().expect("serialize OpenAPI spec");
    let doc: serde_json::Value = serde_json::from_str(&rendered).expect("valid JSON");

    // The three series + dashboard data routes are documented.
    let paths = doc["paths"].as_object().expect("paths object");
    for path in [
        "/api/v1/series/{id}",
        "/api/v1/dashboard/stats",
        "/api/v1/dashboard/activity",
    ] {
        assert!(paths.contains_key(path), "{path} documented");
    }

    // All three are authed routes: they inherit the document-level session_cookie
    // default and must NOT carry an operation-level `security` key (inherit-by-
    // omission, the deny-by-default contract; only public ops opt out). The admin
    // gate on dashboard is an authorization layer documented as a 403 below — not a
    // separate security scheme.
    for path in [
        "/api/v1/series/{id}",
        "/api/v1/dashboard/stats",
        "/api/v1/dashboard/activity",
    ] {
        assert!(
            doc["paths"][path]["get"].get("security").is_none(),
            "GET {path} must inherit the global security default (no op-level security)"
        );
    }

    // Response DTO schemas are registered as components (auto-collected via routes!),
    // including the nested-only StatusCount/MetadataCoverage — proving routes! walked
    // the transitive DTO graph, not just the top-level response bodies.
    let schemas = &doc["components"]["schemas"];
    for schema in [
        "SeriesDetail",
        "SeriesWork",
        "StatsResponse",
        "FormatBucket",
        "StatusCount",
        "MetadataCoverage",
        "ActivityResponse",
        "BatchRow",
    ] {
        assert!(
            schemas.get(schema).is_some(),
            "{schema} schema component present"
        );
    }

    // series/{id} documents its 404 (missing / no-visible-manifestation, existence-
    // not-leaked) against ProblemDetails.
    assert!(
        doc["paths"]["/api/v1/series/{id}"]["get"]["responses"]
            .get("404")
            .is_some(),
        "GET /api/v1/series/{{id}} documents a 404 response"
    );

    // Both dashboard ops document a 403 — the admin gate's only contract signal
    // (this cluster's novel assertion vs the library cluster's 404-only).
    for path in ["/api/v1/dashboard/stats", "/api/v1/dashboard/activity"] {
        assert!(
            doc["paths"][path]["get"]["responses"].get("403").is_some(),
            "GET {path} documents a 403 response (admin gate)"
        );
    }
}
