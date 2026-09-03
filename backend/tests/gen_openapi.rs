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

use std::path::PathBuf;

fn artifact_path() -> PathBuf {
    // Runtime lookup, not env!(): with a shared cargo target dir a cached
    // binary can carry a compile-time path from another (possibly deleted)
    // checkout. Cargo sets the variable for every `cargo test` run.
    PathBuf::from(
        std::env::var_os("CARGO_MANIFEST_DIR")
            .unwrap_or_else(|| panic!("cargo sets CARGO_MANIFEST_DIR")),
    )
    .join("../docs/openapi.json")
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

    // securitySchemes: session cookie (JSON data API) + HTTP Basic (OPDS) +
    // HTTP Bearer (personal device tokens) + HTTP Bearer (resource-server JWTs).
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
    assert_eq!(
        schemes["device_token_bearer"]["type"], "http",
        "device_token_bearer is an http scheme"
    );
    assert_eq!(schemes["device_token_bearer"]["scheme"], "bearer");
    assert_eq!(
        schemes["oidc_jwt_bearer"]["type"], "http",
        "oidc_jwt_bearer is an http scheme"
    );
    assert_eq!(schemes["oidc_jwt_bearer"]["scheme"], "bearer");

    // Hard-rule-6: all four schemes must document the HTTPS-in-production
    // requirement (Basic/Bearer credentials and session cookies are
    // cleartext-exposed otherwise).
    for scheme in [
        "session_cookie",
        "opds_basic",
        "device_token_bearer",
        "oidc_jwt_bearer",
    ] {
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

    // Every /api/v1 op carries an explicit per-operation `security` declaring
    // its required scope. The authz-matrix test (`src/authz_matrix.rs`) is the
    // completeness backstop (deny-by-default: it fails if any /api/v1 op lacks
    // a declared scope). This is a read, so it requires only `read` on all
    // four schemes.
    assert_eq!(
        doc["paths"]["/api/v1/books"]["get"]["security"],
        serde_json::json!([
            {"session_cookie": ["read"]},
            {"device_token_bearer": ["read"]},
            {"oidc_jwt_bearer": ["read"]},
            {"opds_basic": ["read"]},
        ]),
        "GET /api/v1/books must declare its read-scope security requirement"
    );

    // Response DTO schemas are registered as components (auto-collected via routes!).
    let schemas = &doc["components"]["schemas"];
    for schema in [
        "BookListResponse",
        // BookListRow must be a standalone component for the `created_at` guard
        // below to assert against a real schema: if utoipa stopped emitting it,
        // `schemas["BookListRow"]` would be null and the field assertions would
        // fail here first, loudly.
        "BookListRow",
        "BookDetail",
        "WorkDetail",
        "SearchResponse",
    ] {
        assert!(
            schemas.get(schema).is_some(),
            "{schema} schema component present"
        );
    }

    // The `?sort=` param is a free-form JSON:API stack string validated
    // server-side against the sort whitelist, not an enum `$ref`. Assert the
    // query param exists and is a plain string so a regression back to an
    // enum component (or a dropped param) fails loudly.
    let params = doc["paths"]["/api/v1/books"]["get"]["parameters"]
        .as_array()
        .expect("GET /api/v1/books parameters array");
    let sort_param = params
        .iter()
        .find(|p| p["name"] == "sort" && p["in"] == "query")
        .expect("GET /api/v1/books documents a `sort` query param");
    assert_eq!(
        sort_param["schema"]["type"], "string",
        "the sort param is a free-form string, not an enum ref"
    );

    // The detail route documents its 404 (RLS-hidden / missing) against ProblemDetails.
    assert!(
        doc["paths"]["/api/v1/books/{id}"]["get"]["responses"]
            .get("404")
            .is_some(),
        "GET /api/v1/books/{{id}} documents a 404 response"
    );

    // Edge guard (a): `created_at` is RFC 3339 on the wire (it sources the
    // "Added" sort column), so it must appear in the documented schema as a
    // date-time string.
    let created_at = &schemas["BookListRow"]["properties"]["created_at"];
    assert_eq!(
        created_at["type"], "string",
        "BookListRow.created_at is documented as a string"
    );
    assert_eq!(
        created_at["format"], "date-time",
        "BookListRow.created_at is documented as an RFC 3339 date-time"
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

    let data_routes = [
        "/api/v1/series/{id}",
        "/api/v1/dashboard/stats",
        "/api/v1/dashboard/activity",
    ];

    // The three series + dashboard data routes are documented.
    let paths = doc["paths"].as_object().expect("paths object");
    for path in data_routes {
        assert!(paths.contains_key(path), "{path} documented");
    }

    // Every /api/v1 op declares its required scope explicitly. series/{id} is
    // a plain read (`read`); the dashboard ops are admin-only reads (`admin`).
    // The authz-matrix test (`src/authz_matrix.rs`) is the completeness
    // backstop.
    assert_eq!(
        doc["paths"]["/api/v1/series/{id}"]["get"]["security"],
        serde_json::json!([
            {"session_cookie": ["read"]},
            {"device_token_bearer": ["read"]},
            {"oidc_jwt_bearer": ["read"]},
            {"opds_basic": ["read"]},
        ]),
        "GET /api/v1/series/{{id}} must declare its read-scope security requirement"
    );
    for path in ["/api/v1/dashboard/stats", "/api/v1/dashboard/activity"] {
        assert_eq!(
            doc["paths"][path]["get"]["security"],
            serde_json::json!([
                {"session_cookie": ["admin"]},
                {"device_token_bearer": ["admin"]},
                {"oidc_jwt_bearer": ["admin"]},
                {"opds_basic": ["admin"]},
            ]),
            "GET {path} must declare its admin-scope security requirement"
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

    // Each 200 response body references its own DTO — pins the operation→schema
    // wiring so a copy-paste swap between the two adjacent dashboard handlers
    // (body = StatsResponse <-> ActivityResponse) is caught here, not only by the
    // byte-drift gate against a possibly-regenerated artifact.
    for (path, dto) in [
        ("/api/v1/series/{id}", "SeriesDetail"),
        ("/api/v1/dashboard/stats", "StatsResponse"),
        ("/api/v1/dashboard/activity", "ActivityResponse"),
    ] {
        assert_eq!(
            doc["paths"][path]["get"]["responses"]["200"]["content"]["application/json"]["schema"]
                ["$ref"],
            serde_json::json!(format!("#/components/schemas/{dto}")),
            "GET {path} 200 body must reference {dto}"
        );
    }

    // series/{id} documents its 404 (missing / no-linked-works / no-visible-
    // manifestation, existence-not-leaked) against ProblemDetails.
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
