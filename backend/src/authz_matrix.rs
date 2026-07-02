//! Authz matrix: deny-by-default scope declarations plus a per-operation
//! enforcement grid over the scope hierarchy (`read` < `write` < `admin`).
//! See `adr/2026-06-23-api-authorization-orthogonal-axes.md`.
//!
//! Each operation declares the least scope it needs. The grid asserts, per
//! operation, that a credential at the required scope (or higher) passes and
//! a credential one level below is rejected, so a declared requirement that
//! has no matching runtime gate cannot slip through green.
//!
//! Lives inside the crate (rather than `backend/tests/`) because it needs
//! [`crate::test_support`], which is `#[cfg(test)] pub(crate)` and therefore
//! invisible to the separate integration-test crates under `backend/tests/`.
//!
//! Parses the in-process OpenAPI spec ([`crate::openapi::spec_json`]) rather
//! than the committed `docs/openapi.json`, so the grid always tests the
//! annotations the running server actually serves.

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

/// The single scope an operation requires: the highest in its declared set.
/// Scopes are a hierarchy, so a set like `[read, write]` requires `write`
/// (the ceiling), and a credential holding `write` or higher clears it.
/// `None` when the operation declares no scope (a public or auth-only op).
fn required_scope(op: &Operation) -> Option<Scope> {
    op.scopes.iter().copied().max()
}

/// The scope one level below `scope`, or `None` for the `read` floor.
const fn one_below(scope: Scope) -> Option<Scope> {
    match scope {
        Scope::Read => None,
        Scope::Write => Some(Scope::Read),
        Scope::Admin => Some(Scope::Write),
    }
}

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
/// and visible rather than a silent skip; the lint also fails if an entry
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
                    || required_scope(op).is_none_or(|s| s < Scope::Write),
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

// Hierarchy grid: for every operation with a required scope, assert a
// credential at exactly that scope passes, a credential at the top of the
// hierarchy (`admin`) always passes, and a credential one level below the
// requirement is rejected (403). The one-below case is what proves a
// declared requirement has a real runtime gate: without a gate, the lower
// credential would slip through and this test fails. Read-floor operations
// (required scope `read`) have no lower level to reject; the scopeless-token
// test below is their negative control.
#[sqlx::test(migrations = "./migrations")]
async fn scope_grid_enforces_the_hierarchy(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let (admin_id, _basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;

    let ops = parse_operations();
    let gated: Vec<(&Operation, Scope)> = ops
        .iter()
        .filter_map(|op| required_scope(op).map(|s| (op, s)))
        .collect();
    assert!(!gated.is_empty(), "expected at least one scope-gated op");

    for (op, required) in gated {
        let path = substitute_path_params(&op.path);
        let body = body_for(&op.method, &op.path);

        // Positive: a token at the required scope must not 403.
        let at_required = call_with_scoped_token(
            &server,
            &app_pool,
            admin_id,
            &[required],
            &op.method,
            &path,
            body.as_ref(),
        )
        .await;
        assert_ne!(
            at_required,
            StatusCode::FORBIDDEN,
            "{} {} rejected a token holding its required scope {required:?}",
            op.method,
            op.path
        );

        // Positive: the top of the hierarchy clears every gate.
        let at_top = call_with_scoped_token(
            &server,
            &app_pool,
            admin_id,
            &[Scope::Admin],
            &op.method,
            &path,
            body.as_ref(),
        )
        .await;
        assert_ne!(
            at_top,
            StatusCode::FORBIDDEN,
            "{} {} rejected an admin-scoped token (top of the hierarchy)",
            op.method,
            op.path
        );

        // Negative: one level below the requirement must 403. Proves the
        // gate exists; a missing gate would let this token through.
        if let Some(below) = one_below(required) {
            let at_below = call_with_scoped_token(
                &server,
                &app_pool,
                admin_id,
                &[below],
                &op.method,
                &path,
                body.as_ref(),
            )
            .await;
            assert_eq!(
                at_below,
                StatusCode::FORBIDDEN,
                "{} {} did not 403 a token one level below its requirement (held {below:?}, needs {required:?}), got {at_below}",
                op.method,
                op.path
            );
        }
    }
}

// A `read`-only token is blocked from every mutation. A mutation requires
// `write` or higher, and `read` sits below `write`, so the floor credential
// cannot mutate. Checked against every mutating op in the spec.
#[sqlx::test(migrations = "./migrations")]
async fn read_token_blocked_from_every_mutation(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let (admin_id, _basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;

    let ops = parse_operations();
    let mutations: Vec<&Operation> = ops
        .iter()
        .filter(|op| required_scope(op).is_some_and(|s| s >= Scope::Write))
        .collect();
    assert!(!mutations.is_empty(), "expected at least one mutation op");

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
            "a read-only token must be blocked from {} {}, got {status}",
            op.method,
            op.path
        );
    }
}

// A `write` token is blocked from every admin operation. `admin` is the top
// of the hierarchy and `write` sits below it, so a write credential cannot
// reach admin surface. Checked against every admin op (reads and mutations).
#[sqlx::test(migrations = "./migrations")]
async fn write_token_blocked_from_every_admin_op(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let (admin_id, _basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;

    let ops = parse_operations();
    let admin_ops: Vec<&Operation> = ops
        .iter()
        .filter(|op| required_scope(op) == Some(Scope::Admin))
        .collect();
    assert!(!admin_ops.is_empty(), "expected at least one admin op");

    for op in admin_ops {
        let path = substitute_path_params(&op.path);
        let body = body_for(&op.method, &op.path);
        let status = call_with_scoped_token(
            &server,
            &app_pool,
            admin_id,
            &[Scope::Write],
            &op.method,
            &path,
            body.as_ref(),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::FORBIDDEN,
            "a write token must be blocked from admin op {} {}, got {status}",
            op.method,
            op.path
        );
    }
}

// A scopeless token authenticates as nothing: `read` is the floor, so a
// credential carrying no scope is rejected at the auth seam and reaches no
// operation, reads included. Minted directly at the model layer to bypass
// the mint handler's empty-set guard, so this proves the seam-level floor
// (`resolve_device_token`), not just the route validation.
#[sqlx::test(migrations = "./migrations")]
async fn scopeless_token_rejected_by_every_op(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let (admin_id, _basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;

    let ops = parse_operations();
    assert!(!ops.is_empty(), "expected at least one /api/v1 op");

    for op in &ops {
        let path = substitute_path_params(&op.path);
        let body = body_for(&op.method, &op.path);
        let status = call_with_scoped_token(
            &server,
            &app_pool,
            admin_id,
            &[],
            &op.method,
            &path,
            body.as_ref(),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::UNAUTHORIZED,
            "a scopeless token must be rejected at auth for {} {}, got {status}",
            op.method,
            op.path
        );
    }
}

// JWT matrix: the same grid, but the credential is an RFC 9068 access token
// resolved through auth::jwt::JwtValidator + auth::middleware::resolve_jwt
// rather than a minted device token. Proves the scope hierarchy and the
// role/scope orthogonality hold identically for the JWT path -- a claimed
// scope is clamped to the caller's role ceiling exactly like a
// minted token's scope set, so every gate above enforces the same floor
// regardless of which credential type presented it. An absent `scope` claim
// resolving to the full role-derived set (session parity) is already
// covered by `auth::middleware::tests::absent_scope_claim_grants_full_role_ceiling`
// and by every op this grid passes with no claim set below -- no separate
// integration test needed for that path alone.

use crate::models::user;
use crate::test_support::oidc_mock::{MockOidcProvider, now_unix};

const JWT_MATRIX_AUDIENCE: &str = "reverie-authz-matrix";

/// Build a JWT-validator-configured server against a freshly started mock
/// `IdP`. Each JWT-matrix test gets its own mock: the issuer is embedded both
/// in the validator's config and in every token this mock signs.
async fn jwt_server(
    app_pool: &PgPool,
    ingestion_pool: &PgPool,
) -> (axum_test::TestServer, MockOidcProvider) {
    let mock = MockOidcProvider::start("").await;
    mock.mount_resource_server_jwks().await;
    let server = test_support::db::server_with_jwt_validator(
        app_pool,
        ingestion_pool,
        &mock,
        JWT_MATRIX_AUDIENCE,
        false,
    )
    .await;
    (server, mock)
}

/// Seed an OIDC identity at `issuer`/`subject` and promote it to `admin` --
/// the JWT-matrix counterpart to `test_support::db::create_admin_and_basic_auth`,
/// but at the mock's dynamic issuer rather than the fixed test issuer that
/// helper uses (a signed token's `iss` must match the validator's configured
/// issuer).
async fn seed_admin_identity(app_pool: &PgPool, issuer: &str, subject: &str) -> uuid::Uuid {
    let seeded = user::upsert_from_oidc(app_pool, issuer, subject, "Matrix JWT Admin", None)
        .await
        .expect("seed identity");
    sqlx::query!(
        "UPDATE users SET role = 'admin'::user_role WHERE id = $1",
        seeded.id,
    )
    .execute(app_pool)
    .await
    .expect("promote to admin");
    seeded.id
}

/// Seed an OIDC identity and demote it to `child` -- the JWT-matrix
/// counterpart to `test_support::db::create_child_user_and_basic_auth`.
async fn seed_child_identity(app_pool: &PgPool, issuer: &str, subject: &str) -> uuid::Uuid {
    let seeded = user::upsert_from_oidc(app_pool, issuer, subject, "Matrix JWT Child", None)
        .await
        .expect("seed identity");
    sqlx::query!(
        "UPDATE users SET role = 'child'::user_role, is_child = TRUE WHERE id = $1",
        seeded.id,
    )
    .execute(app_pool)
    .await
    .expect("demote to child");
    seeded.id
}

/// Sign an access token for `subject`. `scope` becomes the space-delimited
/// RFC 8693 `scope` claim when present; omitted entirely when `None` (the
/// absent-claim path resolves to the caller's full role-derived set).
fn sign_scoped_token(mock: &MockOidcProvider, subject: &str, scope: Option<&[Scope]>) -> String {
    let now = now_unix();
    let mut claims = serde_json::json!({
        "iss": mock.issuer(),
        "aud": JWT_MATRIX_AUDIENCE,
        "sub": subject,
        "exp": now + 300,
        "iat": now,
    });
    if let Some(scopes) = scope {
        let joined = scopes
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        claims["scope"] = serde_json::json!(joined);
    }
    mock.sign_access_token(&claims, |_| {})
}

async fn call_with_jwt(
    server: &axum_test::TestServer,
    token: &str,
    method: &str,
    path: &str,
    body: Option<&Value>,
) -> StatusCode {
    send(server, method, path, body, &format!("Bearer {token}")).await
}

// Hierarchy grid over JWT callers: identical structure to
// scope_grid_enforces_the_hierarchy, but the credential is a signed access
// token whose `scope` claim is clamped against an admin-role identity's
// ceiling instead of a device token's minted scope set.
#[sqlx::test(migrations = "./migrations")]
async fn jwt_scope_grid_enforces_the_hierarchy(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (server, mock) = jwt_server(&app_pool, &ingestion_pool).await;
    let subject = format!("jwt-matrix-hierarchy-{}", uuid::Uuid::new_v4());
    seed_admin_identity(&app_pool, mock.issuer(), &subject).await;

    let ops = parse_operations();
    let gated: Vec<(&Operation, Scope)> = ops
        .iter()
        .filter_map(|op| required_scope(op).map(|s| (op, s)))
        .collect();
    assert!(!gated.is_empty(), "expected at least one scope-gated op");

    for (op, required) in gated {
        let path = substitute_path_params(&op.path);
        let body = body_for(&op.method, &op.path);

        let at_required = call_with_jwt(
            &server,
            &sign_scoped_token(&mock, &subject, Some(&[required])),
            &op.method,
            &path,
            body.as_ref(),
        )
        .await;
        assert_ne!(
            at_required,
            StatusCode::FORBIDDEN,
            "{} {} rejected a JWT claiming its required scope {required:?}",
            op.method,
            op.path
        );

        let at_top = call_with_jwt(
            &server,
            &sign_scoped_token(&mock, &subject, Some(&[Scope::Admin])),
            &op.method,
            &path,
            body.as_ref(),
        )
        .await;
        assert_ne!(
            at_top,
            StatusCode::FORBIDDEN,
            "{} {} rejected an admin-scope-claiming JWT (top of the hierarchy)",
            op.method,
            op.path
        );

        if let Some(below) = one_below(required) {
            let at_below = call_with_jwt(
                &server,
                &sign_scoped_token(&mock, &subject, Some(&[below])),
                &op.method,
                &path,
                body.as_ref(),
            )
            .await;
            assert_eq!(
                at_below,
                StatusCode::FORBIDDEN,
                "{} {} did not 403 a JWT claiming one level below its requirement \
                 (claimed {below:?}, needs {required:?}), got {at_below}",
                op.method,
                op.path
            );
        }
    }
}

// Invariant-5 orthogonality: an admin-ROLE identity presenting a JWT that
// claims only `read` is blocked from every mutation, exactly as a
// role-derived admin session would NOT be -- proving the claimed scope, not
// the role, is what a mutation gate enforces for the JWT path. This is the
// same op set `read_token_blocked_from_every_mutation` exercises for device
// tokens.
#[sqlx::test(migrations = "./migrations")]
async fn jwt_read_scope_claim_blocked_from_every_mutation(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (server, mock) = jwt_server(&app_pool, &ingestion_pool).await;
    let subject = format!("jwt-matrix-orthogonality-{}", uuid::Uuid::new_v4());
    seed_admin_identity(&app_pool, mock.issuer(), &subject).await;
    let token = sign_scoped_token(&mock, &subject, Some(&[Scope::Read]));

    let ops = parse_operations();
    let mutations: Vec<&Operation> = ops
        .iter()
        .filter(|op| required_scope(op).is_some_and(|s| s >= Scope::Write))
        .collect();
    assert!(!mutations.is_empty(), "expected at least one mutation op");

    for op in mutations {
        let path = substitute_path_params(&op.path);
        let body = body_for(&op.method, &op.path);
        let status = call_with_jwt(&server, &token, &op.method, &path, body.as_ref()).await;
        assert_eq!(
            status,
            StatusCode::FORBIDDEN,
            "an admin-role JWT claiming only read must be blocked from {} {}, got {status}",
            op.method,
            op.path
        );
    }
}

// A write-scope-claiming JWT is blocked from every admin operation, mirroring
// write_token_blocked_from_every_admin_op for the JWT path.
#[sqlx::test(migrations = "./migrations")]
async fn jwt_write_scope_claim_blocked_from_every_admin_op(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (server, mock) = jwt_server(&app_pool, &ingestion_pool).await;
    let subject = format!("jwt-matrix-write-ceiling-{}", uuid::Uuid::new_v4());
    seed_admin_identity(&app_pool, mock.issuer(), &subject).await;
    let token = sign_scoped_token(&mock, &subject, Some(&[Scope::Write]));

    let ops = parse_operations();
    let admin_ops: Vec<&Operation> = ops
        .iter()
        .filter(|op| required_scope(op) == Some(Scope::Admin))
        .collect();
    assert!(!admin_ops.is_empty(), "expected at least one admin op");

    for op in admin_ops {
        let path = substitute_path_params(&op.path);
        let body = body_for(&op.method, &op.path);
        let status = call_with_jwt(&server, &token, &op.method, &path, body.as_ref()).await;
        assert_eq!(
            status,
            StatusCode::FORBIDDEN,
            "a write-scope-claiming JWT must be blocked from admin op {} {}, got {status}",
            op.method,
            op.path
        );
    }
}

// A child-role identity's JWT carries no scope claim (role parity: the full
// {read, write} role-derived set, same as a child's session or device
// token), which clears create_shelf's `write` scope gate -- but the handler
// also calls require_not_child(), a second, orthogonal axis no scope claim
// can satisfy. Proves require_not_child rejects a JWT caller exactly as it
// rejects a session or device-token caller, not just the scope axis.
#[sqlx::test(migrations = "./migrations")]
async fn child_role_jwt_subject_to_require_not_child_gates(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (server, mock) = jwt_server(&app_pool, &ingestion_pool).await;
    let subject = format!("jwt-matrix-child-{}", uuid::Uuid::new_v4());
    seed_child_identity(&app_pool, mock.issuer(), &subject).await;
    let token = sign_scoped_token(&mock, &subject, None);

    let status = call_with_jwt(
        &server,
        &token,
        "POST",
        "/api/v1/shelves",
        Some(&serde_json::json!({"name": "Matrix Shelf"})),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "a child-role JWT must be rejected by require_not_child even though its \
         role-derived scope clears the write gate, got {status}"
    );
}

// Role-ceiling (non-admin cannot mint an admin-scoped token) is covered by
// `routes::tokens::tests::create_token_rejects_admin_scope_ceiling`, not
// duplicated here.
