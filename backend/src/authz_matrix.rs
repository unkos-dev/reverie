//! Authz matrix: deny-by-default scope declarations + per-scope enforcement
//! grid + invariant-5 orthogonality (S4 —
//! `adr/2026-06-23-api-authorization-orthogonal-axes.md`).
//!
//! Lives inside the crate (rather than `backend/tests/`) because it needs
//! [`crate::test_support`], which is `#[cfg(test)] pub(crate)` and therefore
//! invisible to the separate integration-test crates under `backend/tests/`.
//!
//! Parses the IN-PROCESS OpenAPI spec ([`crate::openapi::spec_json`]), not
//! the committed `docs/openapi.json` — that artifact's regeneration is
//! deferred to the final S4 commit, so reading the committed file here would
//! assert against stale annotations while the sweep across route files
//! lands over several commits.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use axum::http::StatusCode;
use serde_json::Value;
use sqlx::PgPool;

use crate::auth::scope::Scope;
use crate::auth::token;
use crate::models::device_token;
use crate::test_support;

/// One `/api/v1` operation as declared in the OpenAPI spec: method, path
/// template, and the scope array shared by every security-requirement
/// alternative. Every `security(...)` annotation in this codebase lists
/// `session_cookie` / `device_token_bearer` / `opds_basic` with the
/// identical scope array (they are alternative transports for the same
/// credential, not different capability levels), so reading the first
/// requirement object's scope array is sufficient.
struct Operation {
    method: String,
    path: String,
    scopes: Vec<Scope>,
}

const HTTP_METHODS: &[&str] = &[
    "get", "post", "put", "patch", "delete", "head", "options", "trace",
];

fn parse_operations() -> Vec<Operation> {
    let rendered = crate::openapi::spec_json().expect("serialize OpenAPI spec");
    let doc: Value = serde_json::from_str(&rendered).expect("valid JSON");
    let paths = doc["paths"].as_object().expect("paths object");

    let mut ops = Vec::new();
    for (path, methods) in paths {
        if !path.starts_with("/api/v1") {
            continue;
        }
        let methods = methods.as_object().expect("path item object");
        for (method, op) in methods {
            if !HTTP_METHODS.contains(&method.as_str()) {
                continue;
            }
            let scopes = op["security"]
                .as_array()
                .and_then(|reqs| {
                    reqs.iter()
                        .find_map(|req| req.as_object().and_then(|o| o.values().next()))
                })
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|s| s.as_str())
                        .map(|s| {
                            s.parse::<Scope>()
                                .unwrap_or_else(|_| panic!("unknown scope literal {s:?}"))
                        })
                        .collect()
                })
                .unwrap_or_default();
            ops.push(Operation {
                method: method.to_uppercase(),
                path: path.clone(),
                scopes,
            });
        }
    }
    ops
}

/// Endpoints intentionally excluded from the mutating-verb `write` lint: a
/// mutating HTTP verb used for a semantically read-only operation. Explicit
/// and visible rather than a silent skip — the lint also fails if an entry
/// here matches no parsed operation (a stale allow-list).
const METHOD_LINT_ALLOWLIST: &[&str] = &["POST /api/v1/manifestations/{id}/enrichment/dry-run"];

#[test]
fn every_api_v1_op_declares_a_scope() {
    let ops = parse_operations();
    assert!(!ops.is_empty(), "expected at least one /api/v1 operation");
    let violations: Vec<String> = ops
        .iter()
        .filter(|op| op.scopes.is_empty())
        .map(|op| format!("{} {}", op.method, op.path))
        .collect();
    assert!(
        violations.is_empty(),
        "deny-by-default violation: these /api/v1 ops declare no scope requirement: {violations:?}"
    );
}

#[test]
fn mutating_verb_ops_require_write_scope() {
    let ops = parse_operations();
    let mutating = ["POST", "PUT", "PATCH", "DELETE"];
    let mut allowlist_seen = std::collections::HashSet::new();

    let violations: Vec<String> = ops
        .iter()
        .filter(|op| mutating.contains(&op.method.as_str()))
        .filter(|op| {
            let key = format!("{} {}", op.method, op.path);
            METHOD_LINT_ALLOWLIST
                .iter()
                .find(|a| **a == key)
                .map_or_else(
                    || !op.scopes.contains(&Scope::Write),
                    |allowed| {
                        allowlist_seen.insert(*allowed);
                        false
                    },
                )
        })
        .map(|op| format!("{} {}", op.method, op.path))
        .collect();
    assert!(
        violations.is_empty(),
        "mutating-verb ops must declare `write` scope (or join the explicit allow-list): {violations:?}"
    );
    for allowed in METHOD_LINT_ALLOWLIST {
        assert!(
            allowlist_seen.contains(allowed),
            "method-lint allow-list entry {allowed:?} matched no parsed operation -- stale entry"
        );
    }
}

/// Replace every `{param}` path segment with a fixed nil UUID. Every gated
/// handler in this sweep runs its scope check before any DB lookup, so a
/// non-existent id still reaches the gate and 403s there rather than 404ing
/// first (a prerequisite for this grid to work at all -- see backend
/// mandatory-reading notes on gate placement).
fn substitute_path_params(path: &str) -> String {
    let mut out = String::new();
    let mut chars = path.chars();
    while let Some(c) = chars.next() {
        if c == '{' {
            for c2 in chars.by_ref() {
                if c2 == '}' {
                    break;
                }
            }
            out.push_str("00000000-0000-0000-0000-000000000000");
        } else {
            out.push(c);
        }
    }
    out
}

/// Minimal-but-valid request body per mutating operation. `Json<T>`-extractor
/// handlers reject a malformed/absent body before the handler runs at all
/// (before any scope check), so this grid needs a body that actually
/// deserializes for every such op -- a malformed body would 422 and produce
/// a false read on the scope assertion.
fn body_for(method: &str, path: &str) -> Option<Value> {
    match (method, path) {
        ("POST", "/api/v1/tokens") => Some(serde_json::json!({"name": "Matrix Test"})),
        ("POST", "/api/v1/shelves") => Some(serde_json::json!({"name": "Matrix Shelf"})),
        ("PATCH", "/api/v1/shelves/{id}") => Some(serde_json::json!({"name": "Renamed"})),
        ("POST", "/api/v1/shelves/{id}/items") => Some(serde_json::json!({
            "manifestation_id": "00000000-0000-0000-0000-000000000000"
        })),
        ("PUT", "/api/v1/shelves/{id}/items") => Some(serde_json::json!({"items": []})),
        (
            "POST",
            "/api/v1/manifestations/{id}/metadata/accept"
            | "/api/v1/manifestations/{id}/metadata/reject",
        ) => Some(serde_json::json!({
            "version_id": "00000000-0000-0000-0000-000000000000"
        })),
        ("POST", "/api/v1/manifestations/{id}/metadata/revert") => {
            Some(serde_json::json!({"field_name": "title"}))
        }
        (
            "POST",
            "/api/v1/manifestations/{id}/metadata/lock"
            | "/api/v1/manifestations/{id}/metadata/unlock",
        ) => Some(serde_json::json!({
            "field_name": "title",
            "entity_type": "manifestation"
        })),
        ("PATCH", "/api/v1/books/{id}/metadata") => {
            Some(serde_json::json!({"fields": {"title": "Matrix Title"}}))
        }
        ("PUT", "/api/v1/settings") => Some(serde_json::json!({"enrichment_enabled": true})),
        ("POST", "/api/v1/users") => Some(serde_json::json!({
            "email": "matrix-test@example.com",
            "display_name": "Matrix User",
            "role": "adult",
            "password": "MatrixSecret123!"
        })),
        ("PUT", "/api/v1/users/{id}/role") => Some(serde_json::json!({"role": "adult"})),
        ("PUT", "/api/v1/users/{id}/child-status") => Some(serde_json::json!({"is_child": false})),
        ("PATCH", "/api/v1/users/{id}") => Some(serde_json::json!({"display_name": "Matrix User"})),
        ("PUT", "/api/v1/users/{id}/account-status") => {
            Some(serde_json::json!({"disabled": false}))
        }
        ("POST", "/api/v1/users/{id}/password-reset") => {
            Some(serde_json::json!({"new_password": "MatrixSecret123!"}))
        }
        ("POST", "/api/v1/account/password") => Some(serde_json::json!({
            "current_password": "OldMatrixSecret123!",
            "new_password": "NewMatrixSecret456!"
        })),
        _ => None,
    }
}

async fn send(
    server: &axum_test::TestServer,
    method: &str,
    path: &str,
    body: Option<&Value>,
    bearer: &str,
) -> StatusCode {
    let mut req = match method {
        "GET" => server.get(path),
        "POST" => server.post(path),
        "PUT" => server.put(path),
        "PATCH" => server.patch(path),
        "DELETE" => server.delete(path),
        other => panic!("unsupported method {other}"),
    }
    .add_header(axum::http::header::AUTHORIZATION, bearer.to_owned());
    if let Some(b) = body {
        req = req.json(b);
    }
    req.await.status_code()
}

/// Mint a token holding exactly `scopes` for `user_id`, run one request, then
/// revoke it immediately. Mint-use-revoke rather than minting once per test
/// keeps the caller's active-token count near zero across the whole grid
/// (dozens of iterations), so `MAX_TOKENS_PER_USER` (10) is never at risk --
/// a purely test-harness concern, unrelated to the cap logic under test
/// elsewhere (`models::device_token` / `routes::tokens`).
async fn call_with_scoped_token(
    server: &axum_test::TestServer,
    pool: &PgPool,
    user_id: uuid::Uuid,
    scopes: &[Scope],
    method: &str,
    path: &str,
    body: Option<&Value>,
) -> StatusCode {
    let (plaintext, hash) = token::generate_device_token();
    let dt = device_token::create_with_limit(pool, user_id, "matrix", &hash, scopes, None)
        .await
        .expect("mint matrix test token");
    let bearer = format!("Bearer {}{}.{}", token::TOKEN_PREFIX, dt.id, plaintext);

    let status = send(server, method, path, body, &bearer).await;

    device_token::revoke(pool, dt.id, user_id)
        .await
        .expect("revoke matrix test token");

    status
}

/// Per-op-per-required-scope grid (finding C1): for every op whose scope
/// array includes `write` or `admin` (i.e. every op that actually has a
/// runtime `require_scope`/`require_scopes` gate -- plain non-admin reads are
/// annotation-only by design and have nothing to grid-test), assert (a) a
/// token holding every declared scope is NOT rejected by scope, and (b) for
/// each declared scope, a token missing exactly that one scope IS rejected
/// (403). This subsumes read-token-blocked, `[read,write]`-vs-admin, and
/// `[read,admin]`-vs-mutation as special cases, and proves every declared
/// scope is actually enforced, not just present in the spec.
#[sqlx::test(migrations = "./migrations")]
async fn scope_grid_enforces_every_declared_requirement(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let (admin_id, _basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;

    let ops = parse_operations();
    let gated: Vec<&Operation> = ops
        .iter()
        .filter(|op| op.scopes.contains(&Scope::Write) || op.scopes.contains(&Scope::Admin))
        .collect();
    assert!(
        !gated.is_empty(),
        "expected at least one scope-gated /api/v1 op"
    );

    for op in gated {
        let path = substitute_path_params(&op.path);
        let body = body_for(&op.method, &op.path);

        // Positive control: holding every required scope must not 403 --
        // proves the assertions below test enforcement, not a broken harness
        // that 403s everything regardless of scope.
        let status = call_with_scoped_token(
            &server,
            &app_pool,
            admin_id,
            &op.scopes,
            &op.method,
            &path,
            body.as_ref(),
        )
        .await;
        assert_ne!(
            status,
            StatusCode::FORBIDDEN,
            "{} {} rejected a token holding every declared scope {:?} (got 403 -- the gate is likely checking the wrong scope set)",
            op.method,
            op.path,
            op.scopes
        );

        // Negative: missing any single declared scope must 403.
        for missing in &op.scopes {
            let held: Vec<Scope> = op.scopes.iter().copied().filter(|s| s != missing).collect();
            let status = call_with_scoped_token(
                &server,
                &app_pool,
                admin_id,
                &held,
                &op.method,
                &path,
                body.as_ref(),
            )
            .await;
            assert_eq!(
                status,
                StatusCode::FORBIDDEN,
                "{} {} did not 403 a token missing {missing:?} (held {held:?}), got {status}",
                op.method,
                op.path
            );
        }
    }
}

/// Scope is orthogonal to role: an admin-role user presenting a read-scoped
/// token is blocked from every mutation despite `role == Admin` -- proves
/// scope is not derived from role. Checked against every mutation op in the
/// spec (non-admin and admin alike), not just one representative endpoint.
#[sqlx::test(migrations = "./migrations")]
async fn invariant5_read_token_blocked_from_every_mutation_despite_admin_role(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let (admin_id, _basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;

    let ops = parse_operations();
    let mutations: Vec<&Operation> = ops
        .iter()
        .filter(|op| op.scopes.contains(&Scope::Write))
        .collect();
    assert!(
        !mutations.is_empty(),
        "expected at least one mutation op to test scope/role orthogonality against"
    );

    for op in mutations {
        let path = substitute_path_params(&op.path);
        let body = body_for(&op.method, &op.path);
        let status = call_with_scoped_token(
            &server,
            &app_pool,
            admin_id,
            &[Scope::Read],
            &op.method,
            &path,
            body.as_ref(),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::FORBIDDEN,
            "scope/role orthogonality violated: admin role + read-only token must be blocked from {} {} despite role=admin, got {status}",
            op.method,
            op.path
        );
    }
}

/// Finding D1: a `[read, admin]` audit-style token must be rejected by every
/// mutation, including admin mutations -- `admin` scope alone does not imply
/// `write`. Without this, a `[read,admin]` token (a legitimate read-only
/// audit credential an admin can mint) could mutate admin endpoints if they
/// were gated on `admin` alone instead of `write AND admin`.
#[sqlx::test(migrations = "./migrations")]
async fn read_admin_token_rejected_by_every_mutation_d1(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let (admin_id, _basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;

    let ops = parse_operations();
    let mutations: Vec<&Operation> = ops
        .iter()
        .filter(|op| op.scopes.contains(&Scope::Write))
        .collect();
    assert!(!mutations.is_empty());

    for op in mutations {
        let path = substitute_path_params(&op.path);
        let body = body_for(&op.method, &op.path);
        let status = call_with_scoped_token(
            &server,
            &app_pool,
            admin_id,
            &[Scope::Read, Scope::Admin],
            &op.method,
            &path,
            body.as_ref(),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::FORBIDDEN,
            "D1 violated: a [read,admin] token must be blocked from {} {} (missing write), got {status}",
            op.method,
            op.path
        );
    }
}

/// Finding C1: a non-admin `[read, write]` token must be rejected by every
/// admin operation (reads and mutations alike) -- `write` scope alone does
/// not imply `admin`.
#[sqlx::test(migrations = "./migrations")]
async fn read_write_token_rejected_by_every_admin_op_c1(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let (admin_id, _basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;

    let ops = parse_operations();
    let admin_ops: Vec<&Operation> = ops
        .iter()
        .filter(|op| op.scopes.contains(&Scope::Admin))
        .collect();
    assert!(
        !admin_ops.is_empty(),
        "expected at least one admin op to test C1 against"
    );

    for op in admin_ops {
        let path = substitute_path_params(&op.path);
        let body = body_for(&op.method, &op.path);
        let status = call_with_scoped_token(
            &server,
            &app_pool,
            admin_id,
            &[Scope::Read, Scope::Write],
            &op.method,
            &path,
            body.as_ref(),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::FORBIDDEN,
            "C1 violated: a non-admin [read,write] token must be blocked from {} {} (missing admin), got {status}",
            op.method,
            op.path
        );
    }
}

// Role-ceiling (non-admin cannot mint an admin-scoped token) is covered by
// `routes::tokens::tests::create_token_rejects_admin_scope_ceiling` (S4
// commit 5) -- not duplicated here.
