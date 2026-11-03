//! Integration tests for `/api/v1/users*` admin endpoints.

use axum::http::{HeaderName, HeaderValue, StatusCode};
use base64ct::Encoding as _;
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::problems;
use crate::test_support;

fn auth(header: &str) -> (HeaderName, HeaderValue) {
    (
        axum::http::header::AUTHORIZATION,
        HeaderValue::from_str(header).expect("ascii auth header"),
    )
}

// ---------- GET /api/v1/users ----------

// ---------- PUT /api/v1/users/{id}/role ----------

#[sqlx::test(migrations = "./migrations")]
async fn role_transitions_sync_child_status_and_invalidate_sessions(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin_id, admin_basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let target_id = test_support::db::create_adult_with_password(
        &app_pool,
        "role-transitions",
        "role-transitions@example.com",
        OLD_PW,
    )
    .await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let mut target_server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    target_server.save_cookies();

    for (index, role) in ["child", "admin", "child", "adult"].iter().enumerate() {
        let login = target_server
            .post("/auth/local/login")
            .json(&json!({"email": "role-transitions@example.com", "password": OLD_PW}))
            .await;
        assert_eq!(login.status_code(), StatusCode::NO_CONTENT);
        assert_eq!(
            target_server.get("/auth/me").await.status_code(),
            StatusCode::OK
        );
        let response = server
            .put(&format!("/api/v1/users/{target_id}/role"))
            .add_header(auth(&admin_basic).0, auth(&admin_basic).1)
            .json(&json!({"role": role}))
            .await;
        assert_eq!(response.status_code(), StatusCode::OK);
        let body: serde_json::Value = response.json();
        assert_eq!(body["role"], *role);
        assert_eq!(body["is_child"], *role == "child");
        let stored = crate::models::user::find_by_id(&app_pool, target_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stored.role.as_str(), *role);
        assert_eq!(stored.is_child, *role == "child");
        assert_eq!(stored.session_version, i32::try_from(index + 1).unwrap());
        assert_eq!(
            target_server.get("/auth/me").await.status_code(),
            StatusCode::UNAUTHORIZED
        );
    }
}

// ---------- PUT /api/v1/users/{id}/child-status ----------

#[sqlx::test(migrations = "./migrations")]
async fn toggle_child_on_sets_role_child(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin_id, admin_basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let (adult_id, _) =
        test_support::db::create_adult_and_basic_auth(&app_pool, "child-toggle").await;

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let r = server
        .put(&format!("/api/v1/users/{adult_id}/child-status"))
        .add_header(auth(&admin_basic).0, auth(&admin_basic).1)
        .json(&json!({"is_child": true}))
        .await;
    assert_eq!(r.status_code(), StatusCode::OK);
    let body: serde_json::Value = r.json();
    assert_eq!(body["is_child"], true);
    assert_eq!(body["role"], "child");

    // session_version bumped
    let sv: i32 = sqlx::query_scalar!(
        r#"SELECT session_version AS "sv!" FROM users WHERE id = $1"#,
        adult_id,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(
        sv > 0,
        "session_version should be bumped after child toggle"
    );
}

// ---------- PATCH /api/v1/users/{id} ----------

#[sqlx::test(migrations = "./migrations")]
async fn patch_user_invalid_email_format_returns_422(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin_id, admin_basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let (adult_id, _) = test_support::db::create_adult_and_basic_auth(&app_pool, "bad-email").await;

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    for bad in [
        "notanemail",
        "a@",
        "@domain.com",
        "a@.com",
        // Display-name and domain-literal forms. `EmailAddress::is_valid`
        // (default options) accepts these and would store the angle/bracket-bearing
        // string raw; `is_addr_spec` rejects them. Exercised here so a revert of the
        // PATCH path back to `is_valid` is caught at the route level, not just in the
        // `is_addr_spec` unit test.
        "Bob <bob@example.com>",
        "bob@[127.0.0.1]",
        "\"john doe\"@example.com",
        "\"john\"@example.com",
        "jane@homelab",
        "jane!books@example.com",
        "jane#books@example.com",
        "jane%books@example.com",
        "jane/books@example.com",
        "josé@example.com",
        "",
    ] {
        if bad.is_empty() {
            // Empty string is a separate validation path tested elsewhere.
            continue;
        }
        let r = server
            .patch(&format!("/api/v1/users/{adult_id}"))
            .add_header(auth(&admin_basic).0, auth(&admin_basic).1)
            .json(&json!({"email": bad}))
            .await;
        let problem = test_support::assert_problem(
            &r,
            problems::VALIDATION,
            StatusCode::UNPROCESSABLE_ENTITY,
        );
        assert!(
            problem["detail"].as_str().unwrap().contains("valid"),
            "expected 'valid' in detail for input {bad:?}, got: {:?}",
            problem["detail"]
        );
    }
}

// ---------- SECURITY: concurrent last-admin demotion ----------

// ---------- 403 guards: child-status PUT and PATCH ----------

// ---------- PATCH whitespace validation ----------

// ---------- child/role sync: mirror branch ----------

// ---------- PATCH session_version bump policy ----------

// ---------- POST /api/v1/users (create / invite) ----------

/// A passphrase that clears the zxcvbn floor. The breach check is disabled in
/// the test config, so this never reaches HIBP.
const STRONG_PW: &str = "correct-horse-battery-staple-7!";

// ---------- PUT /api/v1/users/{id}/account-status ----------

#[sqlx::test(migrations = "./migrations")]
async fn admin_disable_then_re_enable_user(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin_id, admin_basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let (target_id, target_basic) =
        test_support::db::create_adult_and_basic_auth(&app_pool, "status-target").await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

    // The target authenticates before being disabled.
    let before = server
        .get("/auth/me")
        .add_header(auth(&target_basic).0, auth(&target_basic).1)
        .await;
    assert_eq!(before.status_code(), StatusCode::OK);

    let disable = server
        .put(&format!("/api/v1/users/{target_id}/account-status"))
        .add_header(auth(&admin_basic).0, auth(&admin_basic).1)
        .json(&json!({"disabled": true}))
        .await;
    assert_eq!(disable.status_code(), StatusCode::OK);
    assert_eq!(disable.json::<serde_json::Value>()["disabled"], true);

    // The target's token is now inert.
    let after_disable = server
        .get("/auth/me")
        .add_header(auth(&target_basic).0, auth(&target_basic).1)
        .await;
    assert_eq!(after_disable.status_code(), StatusCode::UNAUTHORIZED);

    // Re-enabling restores access.
    let enable = server
        .put(&format!("/api/v1/users/{target_id}/account-status"))
        .add_header(auth(&admin_basic).0, auth(&admin_basic).1)
        .json(&json!({"disabled": false}))
        .await;
    assert_eq!(enable.status_code(), StatusCode::OK);
    assert_eq!(enable.json::<serde_json::Value>()["disabled"], false);

    let after_enable = server
        .get("/auth/me")
        .add_header(auth(&target_basic).0, auth(&target_basic).1)
        .await;
    assert_eq!(after_enable.status_code(), StatusCode::OK);
}

// ---------- POST /api/v1/account/password (self-service) ----------

const OLD_PW: &str = "old";

#[sqlx::test(migrations = "./migrations")]
async fn change_own_password_does_not_overwrite_concurrent_reset(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (id, basic) = test_support::db::create_adult_and_basic_auth(&app_pool, "change-race").await;
    let old_hash = crate::auth::password::hash_password(OLD_PW.as_bytes()).unwrap();
    crate::models::local_credentials::set_password(&app_pool, id, &old_hash)
        .await
        .unwrap();
    let reset_hash = crate::auth::password::hash_password(b"independent reset passphrase").unwrap();
    let mut reset = app_pool.begin().await.unwrap();
    sqlx::query_scalar!("SELECT id FROM users WHERE id = $1 FOR UPDATE", id)
        .fetch_one(&mut *reset)
        .await
        .unwrap();
    crate::models::local_credentials::set_password(&mut *reset, id, &reset_hash)
        .await
        .unwrap();
    crate::models::user::increment_session_version(&mut *reset, id)
        .await
        .unwrap();

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let change = tokio::spawn(async move {
        server
            .post("/api/v1/account/password")
            .add_header(auth(&basic).0, auth(&basic).1)
            .json(&json!({"current_password": OLD_PW, "new_password": STRONG_PW}))
            .await
    });
    test_support::wait_for_blocked_query(
        &pool,
        "SELECT id FROM users WHERE id = $1 FOR NO KEY UPDATE%",
        1,
    )
    .await;
    reset.commit().await.unwrap();
    let response = change.await.unwrap();
    assert_eq!(response.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
    let credential = crate::models::local_credentials::find_by_user_id(&app_pool, id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(credential.password_hash, reset_hash);
    assert_eq!(
        crate::models::user::find_by_id(&app_pool, id)
            .await
            .unwrap()
            .unwrap()
            .session_version,
        1
    );
}

fn csrf_header(token: &str) -> (HeaderName, HeaderValue) {
    (
        HeaderName::from_static("x-csrf-token"),
        HeaderValue::from_str(token).expect("ascii csrf token"),
    )
}

#[sqlx::test(migrations = "./migrations")]
async fn change_own_password_succeeds_and_forces_reauth(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    test_support::db::create_adult_with_password(
        &app_pool,
        "change-happy",
        "change-happy@example.com",
        OLD_PW,
    )
    .await;
    let mut server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    server.save_cookies();
    server
        .post("/auth/local/login")
        .json(&json!({"email": "change-happy@example.com", "password": OLD_PW}))
        .await;
    let me: serde_json::Value = server.get("/auth/me").await.json();
    let token = me["csrf_token"].as_str().unwrap().to_owned();

    let r = server
        .post("/api/v1/account/password")
        .add_header(csrf_header(&token).0, csrf_header(&token).1)
        .json(&json!({"current_password": OLD_PW, "new_password": STRONG_PW}))
        .await;
    assert_eq!(r.status_code(), StatusCode::OK);

    // session_version bumped: the current session is invalidated (forced re-auth).
    assert_eq!(
        server.get("/auth/me").await.status_code(),
        StatusCode::UNAUTHORIZED,
        "a self-service password change forces re-authentication"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn change_own_password_wrong_current_returns_422(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    test_support::db::create_adult_with_password(
        &app_pool,
        "change-wrong",
        "change-wrong@example.com",
        OLD_PW,
    )
    .await;
    let mut server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    server.save_cookies();
    server
        .post("/auth/local/login")
        .json(&json!({"email": "change-wrong@example.com", "password": OLD_PW}))
        .await;
    let me: serde_json::Value = server.get("/auth/me").await.json();
    let token = me["csrf_token"].as_str().unwrap().to_owned();

    let r = server
        .post("/api/v1/account/password")
        .add_header(csrf_header(&token).0, csrf_header(&token).1)
        .json(&json!({"current_password": "not the password", "new_password": STRONG_PW}))
        .await;
    test_support::assert_problem(&r, problems::VALIDATION, StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test(migrations = "./migrations")]
async fn change_own_password_oidc_only_returns_422(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    // An OIDC-provisioned account: a device token but no local credential.
    let (_id, basic) =
        test_support::db::create_adult_and_basic_auth(&app_pool, "change-oidc").await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

    // Basic auth is CSRF-exempt, so no token is needed.
    let r = server
        .post("/api/v1/account/password")
        .add_header(auth(&basic).0, auth(&basic).1)
        .json(&json!({"current_password": "anything", "new_password": STRONG_PW}))
        .await;
    test_support::assert_problem(&r, problems::VALIDATION, StatusCode::UNPROCESSABLE_ENTITY);
}

async fn credential_link_admin(
    app_pool: &PgPool,
    ingestion_pool: &PgPool,
) -> (axum_test::TestServer, String, Uuid) {
    let id = test_support::db::create_adult_with_password(
        app_pool,
        "Admin",
        "link-admin@example.test",
        STRONG_PW,
    )
    .await;
    sqlx::query!(
        "UPDATE users SET role = 'admin'::user_role WHERE id = $1",
        id
    )
    .execute(app_pool)
    .await
    .unwrap();
    let mut server = test_support::db::server_with_real_pools(app_pool, ingestion_pool);
    server.save_cookies();
    let login = server
        .post("/auth/local/login")
        .json(&json!({"email":"link-admin@example.test","password":STRONG_PW}))
        .await;
    assert_eq!(login.status_code(), StatusCode::NO_CONTENT);
    let csrf = server.get("/auth/me").await.json::<serde_json::Value>()["csrf_token"]
        .as_str()
        .unwrap()
        .to_owned();
    (server, csrf, id)
}

#[sqlx::test(migrations = "./migrations")]
async fn credential_link_create_issue_cancel_and_response_producers(pool: PgPool) {
    let app = test_support::db::app_pool_for(&pool).await;
    let ingestion = test_support::db::ingestion_pool_for(&pool).await;
    let (server, csrf, _) = credential_link_admin(&app, &ingestion).await;
    let created = server
        .post("/api/v1/users")
        .add_header(csrf_header(&csrf).0, csrf_header(&csrf).1)
        .json(&json!({"email":"invite@example.test","display_name":"Invite","role":"adult"}))
        .await;
    assert_eq!(created.status_code(), StatusCode::CREATED);
    assert_eq!(created.headers()["cache-control"], "no-store");
    let body: serde_json::Value = created.json();
    let id: Uuid = serde_json::from_value(body["user"]["id"].clone()).unwrap();
    let first = body["link"]["token"].as_str().unwrap();
    assert_eq!(body["user"]["has_password"], false);
    assert_eq!(
        body["user"]["credential_link_expires_at"],
        body["link"]["expires_at"]
    );
    assert!(
        !crate::models::local_credentials::exists_for_user(&app, id)
            .await
            .unwrap()
    );
    let issued = server
        .post(&format!("/api/v1/users/{id}/credential-links"))
        .add_header(csrf_header(&csrf).0, csrf_header(&csrf).1)
        .await;
    assert_eq!(issued.status_code(), StatusCode::CREATED);
    assert_eq!(issued.headers()["cache-control"], "no-store");
    assert!(
        crate::models::credential_link::lookup(&app, first)
            .await
            .unwrap()
            .is_none()
    );
    let expiry = issued.json::<serde_json::Value>()["expires_at"].clone();
    for response in [
        server
            .put(&format!("/api/v1/users/{id}/role"))
            .add_header(csrf_header(&csrf).0, csrf_header(&csrf).1)
            .json(&json!({"role":"adult"}))
            .await,
        server
            .put(&format!("/api/v1/users/{id}/child-status"))
            .add_header(csrf_header(&csrf).0, csrf_header(&csrf).1)
            .json(&json!({"is_child":true}))
            .await,
        server
            .put(&format!("/api/v1/users/{id}/account-status"))
            .add_header(csrf_header(&csrf).0, csrf_header(&csrf).1)
            .json(&json!({"disabled":false}))
            .await,
        server
            .patch(&format!("/api/v1/users/{id}"))
            .add_header(csrf_header(&csrf).0, csrf_header(&csrf).1)
            .json(&json!({"display_name":"Updated"}))
            .await,
    ] {
        assert_eq!(response.status_code(), StatusCode::OK);
        let user: serde_json::Value = response.json();
        assert_eq!(user["has_password"], false);
        assert_eq!(user["credential_link_expires_at"], expiry);
    }
    let phc = crate::auth::password::hash_password(STRONG_PW.as_bytes()).unwrap();
    crate::models::local_credentials::set_password(&app, id, &phc)
        .await
        .unwrap();
    let listed: Vec<serde_json::Value> = server.get("/api/v1/users").await.json();
    let target = listed.iter().find(|u| u["id"] == id.to_string()).unwrap();
    assert_eq!(target["has_password"], true);
    assert_eq!(target["credential_link_expires_at"], expiry);
    for _ in 0..2 {
        assert_eq!(
            server
                .delete(&format!("/api/v1/users/{id}/credential-links"))
                .add_header(csrf_header(&csrf).0, csrf_header(&csrf).1)
                .await
                .status_code(),
            StatusCode::NO_CONTENT
        );
    }
    let listed: Vec<serde_json::Value> = server.get("/api/v1/users").await.json();
    assert!(
        listed.iter().find(|u| u["id"] == id.to_string()).unwrap()["credential_link_expires_at"]
            .is_null()
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn credential_link_issuance_gates(pool: PgPool) {
    let app = test_support::db::app_pool_for(&pool).await;
    let ingestion = test_support::db::ingestion_pool_for(&pool).await;
    let (id, basic) = test_support::db::create_admin_and_basic_auth(&app).await;
    let decoded = base64ct::Base64::decode_vec(basic.strip_prefix("Basic ").unwrap()).unwrap();
    let credentials = String::from_utf8(decoded).unwrap();
    let bearer = format!("Bearer {}", credentials.replace(':', "."));
    let server = test_support::db::server_with_real_pools(&app, &ingestion);
    for header in [basic, bearer] {
        for response in [
            server
                .post("/api/v1/users")
                .add_header(auth(&header).0, auth(&header).1)
                .json(&json!({"email":"a@example.test","display_name":"A","role":"adult"}))
                .await,
            server
                .post(&format!("/api/v1/users/{id}/credential-links"))
                .add_header(auth(&header).0, auth(&header).1)
                .await,
            server
                .delete(&format!("/api/v1/users/{id}/credential-links"))
                .add_header(auth(&header).0, auth(&header).1)
                .await,
        ] {
            assert_eq!(response.status_code(), StatusCode::FORBIDDEN);
        }
    }
    let (session, csrf, _) = credential_link_admin(&app, &ingestion).await;
    let no_email = session
        .post(&format!("/api/v1/users/{id}/credential-links"))
        .add_header(csrf_header(&csrf).0, csrf_header(&csrf).1)
        .await;
    assert_eq!(no_email.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        no_email.json::<serde_json::Value>()["detail"],
        "the account has no email address"
    );
    crate::models::user::disable_account(&app, id)
        .await
        .unwrap();
    let disabled = session
        .post(&format!("/api/v1/users/{id}/credential-links"))
        .add_header(csrf_header(&csrf).0, csrf_header(&csrf).1)
        .await;
    assert_eq!(
        disabled.json::<serde_json::Value>()["detail"],
        "the account is disabled"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn credential_link_local_auth_disabled_all_endpoints(pool: PgPool) {
    let app = test_support::db::app_pool_for(&pool).await;
    let ingestion = test_support::db::ingestion_pool_for(&pool).await;
    let (id, basic) = test_support::db::create_admin_and_basic_auth(&app).await;
    let mut config = test_support::test_config();
    config.local_auth_enabled = false;
    let server = test_support::db::server_with_config(&app, &ingestion, config);
    for response in [
        server
            .post("/api/v1/users")
            .add_header(auth(&basic).0, auth(&basic).1)
            .json(&json!({"email":"a@example.test","display_name":"A","role":"adult"}))
            .await,
        server
            .post(&format!("/api/v1/users/{id}/credential-links"))
            .add_header(auth(&basic).0, auth(&basic).1)
            .await,
        server
            .delete(&format!("/api/v1/users/{id}/credential-links"))
            .add_header(auth(&basic).0, auth(&basic).1)
            .await,
        server
            .post("/auth/credential-links/lookups")
            .json(&json!({"token":"unknown"}))
            .await,
        server
            .post("/auth/credential-links/redemptions")
            .json(&json!({"token":"unknown","password":STRONG_PW}))
            .await,
    ] {
        assert_eq!(response.status_code(), StatusCode::NOT_FOUND);
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn credential_link_invalid_cases_are_indistinguishable(pool: PgPool) {
    let app = test_support::db::app_pool_for(&pool).await;
    let ingestion = test_support::db::ingestion_pool_for(&pool).await;
    let server = test_support::db::server_with_real_pools(&app, &ingestion);
    let mut tokens = vec![
        "A".repeat(43),
        String::new(),
        "A".repeat(42),
        "A".repeat(44),
        format!("{}=", "A".repeat(42)),
        format!("{}+", "A".repeat(42)),
        format!("{} ", "A".repeat(42)),
        "B".repeat(43),
        "é".repeat(43),
    ];
    for case in ["expired", "used", "disabled", "no-email"] {
        let id = crate::models::user::create_local(
            &app,
            &format!("{case}@example.test"),
            case,
            crate::models::role::Role::Adult,
            None,
        )
        .await
        .unwrap()
        .id;
        let (token, _) = crate::models::credential_link::issue(
            &mut app.acquire().await.unwrap(),
            id,
            crate::models::credential_link::CREDENTIAL_LINK_TTL,
        )
        .await
        .unwrap();
        match case {
            "expired" => {
                sqlx::query!(
                    "UPDATE credential_links SET expires_at = now() WHERE user_id = $1",
                    id
                )
                .execute(&app)
                .await
                .unwrap();
            }
            "used" => {
                assert!(
                    crate::models::credential_link::consume(
                        &mut app.acquire().await.unwrap(),
                        id,
                        &token
                    )
                    .await
                    .unwrap()
                );
            }
            "disabled" => {
                crate::models::user::disable_account(&app, id)
                    .await
                    .unwrap();
            }
            "no-email" => {
                sqlx::query!("UPDATE users SET email = NULL WHERE id = $1", id)
                    .execute(&app)
                    .await
                    .unwrap();
            }
            _ => unreachable!(),
        }
        tokens.push(token);
    }
    for path in [
        "/auth/credential-links/lookups",
        "/auth/credential-links/redemptions",
    ] {
        let mut expected = None;
        for token in &tokens {
            let response = server
                .post(path)
                .json(&json!({"token":token,"password":STRONG_PW}))
                .await;
            assert_eq!(response.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
            let body: serde_json::Value = response.json();
            assert_eq!(body["detail"], "invalid or expired link");
            if let Some(ref expected) = expected {
                assert_eq!(&body, expected);
            } else {
                expected = Some(body);
            }
        }
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn credential_link_redemption_password_sessions_throttle_and_replacement_proofs(
    pool: PgPool,
) {
    let app = test_support::db::app_pool_for(&pool).await;
    let ingestion = test_support::db::ingestion_pool_for(&pool).await;
    let id = test_support::db::create_adult_with_password(
        &app,
        "Holder",
        "holder@example.test",
        STRONG_PW,
    )
    .await;
    let mut signed_in = test_support::db::server_with_real_pools(&app, &ingestion);
    signed_in.save_cookies();
    assert_eq!(
        signed_in
            .post("/auth/local/login")
            .json(&json!({"email":"holder@example.test","password":STRONG_PW}))
            .await
            .status_code(),
        StatusCode::NO_CONTENT
    );
    let before_version = crate::models::user::find_by_id(&app, id)
        .await
        .unwrap()
        .unwrap()
        .session_version;
    sqlx::query!("UPDATE local_login_throttle SET completed_failures = ARRAY[now()], locked_until = now() + interval '15 minutes' WHERE email_lower = 'holder@example.test'")
        .execute(&app).await.unwrap();
    let before = sqlx::query_scalar!(
        "SELECT to_jsonb(t) FROM local_login_throttle t WHERE email_lower = 'holder@example.test'"
    )
    .fetch_one(&app)
    .await
    .unwrap();
    let mut tx = app.begin().await.unwrap();
    crate::models::account_email_proof::lock_account(&mut tx, id)
        .await
        .unwrap()
        .unwrap();
    let recovery = crate::models::account_email_proof::issue(
        &mut tx,
        id,
        crate::models::account_email_proof::ProofPurpose::Recovery,
        "holder@example.test",
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    let (token, _) = crate::models::credential_link::issue(
        &mut app.acquire().await.unwrap(),
        id,
        crate::models::credential_link::CREDENTIAL_LINK_TTL,
    )
    .await
    .unwrap();
    let server = test_support::db::server_with_real_pools(&app, &ingestion);
    let lookup = server
        .post("/auth/credential-links/lookups")
        .json(&json!({"token":token}))
        .await;
    assert_eq!(lookup.headers()["cache-control"], "no-store");
    assert_eq!(
        lookup.json::<serde_json::Value>(),
        json!({"email":"holder@example.test","has_password":true})
    );
    let result = server
        .post("/auth/credential-links/redemptions")
        .json(&json!({"token":token,"password":"lithe cobalt meadow drift"}))
        .await;
    assert_eq!(result.status_code(), StatusCode::NO_CONTENT);
    assert!(!result.headers().contains_key("set-cookie"));
    assert!(!result.headers().contains_key("retry-after"));
    assert_eq!(
        server.get("/auth/me").await.status_code(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        signed_in.get("/auth/me").await.status_code(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        crate::models::user::find_by_id(&app, id)
            .await
            .unwrap()
            .unwrap()
            .session_version,
        before_version + 1
    );
    let credential = crate::models::local_credentials::find_by_user_id(&app, id)
        .await
        .unwrap()
        .unwrap();
    crate::auth::password::verify_password(b"lithe cobalt meadow drift", &credential.password_hash)
        .unwrap();
    assert!(
        crate::models::credential_link::lookup(&app, &token)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        crate::models::account_email_proof::live(
            &mut app.acquire().await.unwrap(),
            id,
            crate::models::account_email_proof::ProofPurpose::Recovery,
        )
        .await
        .unwrap()
        .is_none()
    );
    assert!(
        crate::models::credential_link::lookup(&app, &recovery.token)
            .await
            .unwrap()
            .is_none()
    );

    let after = sqlx::query_scalar!(
        "SELECT to_jsonb(t) FROM local_login_throttle t WHERE email_lower = 'holder@example.test'"
    )
    .fetch_one(&app)
    .await
    .unwrap();
    assert_eq!(before, after);
}

#[sqlx::test(migrations = "./migrations")]
async fn credential_link_policy_rejection_and_concurrent_redemption(pool: PgPool) {
    let app = test_support::db::app_pool_for(&pool).await;
    let ingestion = test_support::db::ingestion_pool_for(&pool).await;
    let id = crate::models::user::create_local(
        &app,
        "new@example.test",
        "New",
        crate::models::role::Role::Adult,
        None,
    )
    .await
    .unwrap()
    .id;
    let (token, _) = crate::models::credential_link::issue(
        &mut app.acquire().await.unwrap(),
        id,
        crate::models::credential_link::CREDENTIAL_LINK_TTL,
    )
    .await
    .unwrap();
    let server = test_support::db::server_with_real_pools(&app, &ingestion);
    assert_eq!(
        server
            .post("/auth/credential-links/lookups")
            .json(&json!({"token":token}))
            .await
            .json::<serde_json::Value>(),
        json!({"email":"new@example.test","has_password":false})
    );
    assert_eq!(
        server
            .post("/auth/credential-links/redemptions")
            .json(&json!({"token":token,"password":"short"}))
            .await
            .status_code(),
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert!(
        crate::models::credential_link::lookup(&app, &token)
            .await
            .unwrap()
            .is_some()
    );
    let body = json!({"token":token,"password":STRONG_PW});
    let (a, b) = tokio::join!(
        server
            .post("/auth/credential-links/redemptions")
            .json(&body),
        server
            .post("/auth/credential-links/redemptions")
            .json(&body)
    );
    let statuses = [a.status_code(), b.status_code()];
    assert!(statuses.contains(&StatusCode::NO_CONTENT));
    assert!(statuses.contains(&StatusCode::UNPROCESSABLE_ENTITY));
    assert!(
        crate::models::local_credentials::exists_for_user(&app, id)
            .await
            .unwrap()
    );
    assert_eq!(
        crate::models::user::find_by_id(&app, id)
            .await
            .unwrap()
            .unwrap()
            .session_version,
        1
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn credential_link_recovery_allowance_not_password_allowance(pool: PgPool) {
    let mut state = test_support::test_state();
    state.pool = test_support::db::app_pool_for(&pool).await;
    state.ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    state.config.trusted_client_ip_header = Some("x-forwarded-for".into());
    state.recovery_limiter =
        crate::auth::rate_limit::build_login_limiter(std::num::NonZeroU32::new(1).unwrap());
    state.login_limiter =
        crate::auth::rate_limit::build_login_limiter(std::num::NonZeroU32::new(1).unwrap());
    let ip = "192.0.2.9".parse().unwrap();
    state.login_limiter.check_key(&ip).unwrap();
    let id = crate::models::user::create_local(
        &state.pool,
        "budget@example.test",
        "Budget",
        crate::models::role::Role::Adult,
        None,
    )
    .await
    .unwrap()
    .id;
    let (token, _) = crate::models::credential_link::issue(
        &mut state.pool.acquire().await.unwrap(),
        id,
        crate::models::credential_link::CREDENTIAL_LINK_TTL,
    )
    .await
    .unwrap();
    let server = axum_test::TestServer::new(crate::build_router(state.clone()));
    let result = server
        .post("/auth/credential-links/redemptions")
        .add_header(
            HeaderName::from_static("x-forwarded-for"),
            HeaderValue::from_static("192.0.2.9"),
        )
        .json(&json!({"token":token,"password":STRONG_PW}))
        .await;
    assert_eq!(result.status_code(), StatusCode::NO_CONTENT);
    assert!(state.recovery_limiter.check_key(&ip).is_err());
    assert_eq!(
        server
            .post("/auth/credential-links/lookups")
            .add_header(
                HeaderName::from_static("x-forwarded-for"),
                HeaderValue::from_static("192.0.2.9")
            )
            .json(&json!({"token":token}))
            .await
            .status_code(),
        StatusCode::TOO_MANY_REQUESTS
    );
    assert_eq!(
        server
            .post("/auth/credential-links/redemptions")
            .add_header(
                HeaderName::from_static("x-forwarded-for"),
                HeaderValue::from_static("192.0.2.9")
            )
            .json(&json!({"token":token,"password":STRONG_PW}))
            .await
            .status_code(),
        StatusCode::TOO_MANY_REQUESTS
    );
    let lookup_ip = "192.0.2.10".parse().unwrap();
    assert_eq!(
        server
            .post("/auth/credential-links/lookups")
            .add_header(
                HeaderName::from_static("x-forwarded-for"),
                HeaderValue::from_static("192.0.2.10")
            )
            .json(&json!({"token":"invalid"}))
            .await
            .status_code(),
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert!(state.recovery_limiter.check_key(&lookup_ip).is_err());
    assert!(state.login_limiter.check_key(&lookup_ip).is_ok());
}

#[sqlx::test(migrations = "./migrations")]
async fn credential_link_malformed_tokens_spend_recovery_allowance_first(pool: PgPool) {
    let mut state = test_support::test_state();
    state.pool = test_support::db::app_pool_for(&pool).await;
    state.ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    state.config.trusted_client_ip_header = Some("x-forwarded-for".into());
    state.recovery_limiter =
        crate::auth::rate_limit::build_login_limiter(std::num::NonZeroU32::new(1).unwrap());
    let server = axum_test::TestServer::new(crate::build_router(state));
    for (path, source) in [
        ("/auth/credential-links/lookups", "192.0.2.20"),
        ("/auth/credential-links/redemptions", "192.0.2.21"),
    ] {
        for expected in [
            StatusCode::UNPROCESSABLE_ENTITY,
            StatusCode::TOO_MANY_REQUESTS,
        ] {
            let response = server
                .post(path)
                .add_header(
                    HeaderName::from_static("x-forwarded-for"),
                    HeaderValue::from_str(source).unwrap(),
                )
                .json(&json!({"token":"malformed", "password": STRONG_PW}))
                .await;
            assert_eq!(response.status_code(), expected);
            if expected == StatusCode::UNPROCESSABLE_ENTITY {
                assert_eq!(
                    response.json::<serde_json::Value>()["detail"],
                    "invalid or expired link"
                );
            }
        }
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn password_ceiling_login_and_own_change_preserve_full_unicode_input(pool: PgPool) {
    let mut state = test_support::test_state();
    state.pool = test_support::db::app_pool_for(&pool).await;
    state.ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    state.config.password_min_zxcvbn_score = 0;
    state.hash_limiter = crate::auth::password::HashLimiter::new(1);
    let password = "🦀".repeat(256);
    test_support::db::create_adult_with_password(
        &state.pool,
        "Ceiling",
        "ceiling@example.test",
        &password,
    )
    .await;
    let mut server = axum_test::TestServer::new(crate::build_router(state.clone()));
    server.save_cookies();
    let held = state.hash_limiter.acquire().await.unwrap();
    let response = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        server
            .post("/auth/local/login")
            .json(&json!({"email":"ceiling@example.test", "password":format!("{password}x")})),
    )
    .await
    .unwrap();
    assert_eq!(response.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        response.json::<serde_json::Value>()["detail"],
        "invalid email or password"
    );
    drop(held);
    assert_eq!(
        server
            .post("/auth/local/login")
            .json(&json!({"email":"ceiling@example.test", "password":password}))
            .await
            .status_code(),
        StatusCode::NO_CONTENT
    );
    let me: serde_json::Value = server.get("/auth/me").await.json();
    let csrf = me["csrf_token"].as_str().unwrap();
    let held = state.hash_limiter.acquire().await.unwrap();
    for (current, next) in [
        (format!("{password}x"), STRONG_PW.to_owned()),
        (password.clone(), "é".repeat(257)),
    ] {
        let response = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            server
                .post("/api/v1/account/password")
                .add_header(csrf_header(csrf).0, csrf_header(csrf).1)
                .json(&json!({"current_password":current, "new_password":next})),
        )
        .await
        .unwrap();
        assert_eq!(response.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(
            response.json::<serde_json::Value>()["detail"],
            "password must be at most 256 characters"
        );
    }
    drop(held);
    assert_eq!(
        server
            .post("/api/v1/account/password")
            .add_header(csrf_header(csrf).0, csrf_header(csrf).1)
            .json(&json!({"current_password":password, "new_password":"é".repeat(256)}))
            .await
            .status_code(),
        StatusCode::OK
    );
}

// ---------- authentication mail ----------

mod mail_flow {
    use super::*;
    use crate::models::account_email_proof::{ProofPurpose, lock_account};
    use crate::models::user;
    use crate::services::mail::dispatch::Outbox;
    use crate::services::mail::queue::{MailRuntime, process_next};
    use crate::services::mail::render::{self, Context, LinkKind, Notice};
    use crate::test_support::mail::{self, Behaviour, FakeRelay, ORIGIN, PASSWORD, SUPPORT};
    use axum_test::{TestResponse, TestServer};
    use chrono::{DateTime, Utc};
    use std::sync::Arc;
    use tokio_util::sync::CancellationToken;

    struct Env {
        app: PgPool,
        ingestion: PgPool,
        runtime: Arc<MailRuntime>,
        server: TestServer,
    }

    async fn env_on(pool: &PgPool, port: u16) -> Env {
        let app = test_support::db::app_pool_for(pool).await;
        let ingestion = test_support::db::ingestion_pool_for(pool).await;
        let runtime = mail::shared_runtime(port);
        let server = mail::server(&app, &ingestion, &runtime);
        Env {
            app,
            ingestion,
            runtime,
            server,
        }
    }

    async fn env(pool: &PgPool) -> Env {
        env_on(pool, 1).await
    }

    fn another(env: &Env) -> TestServer {
        mail::server(&env.app, &env.ingestion, &env.runtime)
    }

    fn context() -> Context {
        Context {
            origin: ORIGIN.to_owned(),
            support: SUPPORT.to_owned(),
        }
    }

    fn outbox(env: &Env) -> Outbox {
        Outbox::new(Some(&env.runtime), Some(&url::Url::parse(ORIGIN).unwrap()))
            .unwrap()
            .unwrap()
    }

    async fn account(env: &Env, name: &str, email: &str) -> Uuid {
        test_support::db::create_adult_with_password(&env.app, name, email, PASSWORD).await
    }

    async fn verified(env: &Env, pool: &PgPool, name: &str, email: &str) -> Uuid {
        let id = account(env, name, email).await;
        mail::verify_address(pool, id).await;
        id
    }

    async fn admin(env: &Env, pool: &PgPool) -> (Uuid, String) {
        let id = account(env, "Admin", "mail-admin@example.test").await;
        sqlx::query!(
            "UPDATE users SET role = 'admin'::user_role WHERE id = $1",
            id
        )
        .execute(pool)
        .await
        .unwrap();
        let csrf = mail::login(&env.server, "mail-admin@example.test", PASSWORD).await;
        (id, csrf)
    }

    async fn post(
        server: &TestServer,
        csrf: &str,
        path: &str,
        body: serde_json::Value,
    ) -> TestResponse {
        let (name, value) = mail::csrf(csrf);
        server.post(path).add_header(name, value).json(&body).await
    }

    async fn post_empty(server: &TestServer, csrf: &str, path: &str) -> TestResponse {
        let (name, value) = mail::csrf(csrf);
        server.post(path).add_header(name, value).await
    }

    async fn patch(
        server: &TestServer,
        csrf: &str,
        id: Uuid,
        body: serde_json::Value,
    ) -> TestResponse {
        let (name, value) = mail::csrf(csrf);
        server
            .patch(&format!("/api/v1/users/{id}"))
            .add_header(name, value)
            .json(&body)
            .await
    }

    fn instant(value: &serde_json::Value) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(value.as_str().unwrap())
            .unwrap()
            .with_timezone(&Utc)
    }

    struct Invitee {
        id: Uuid,
        token: String,
        expires_at: DateTime<Utc>,
    }

    async fn invite(env: &Env, csrf: &str, email: &str) -> Invitee {
        let response = post(
            &env.server,
            csrf,
            "/api/v1/users",
            json!({"email": email, "display_name": "Invitee", "role": "adult"}),
        )
        .await;
        assert_eq!(response.status_code(), StatusCode::CREATED);
        let body: serde_json::Value = response.json();
        Invitee {
            id: body["user"]["id"].as_str().unwrap().parse().unwrap(),
            token: body["link"]["token"].as_str().unwrap().to_owned(),
            expires_at: instant(&body["link"]["expires_at"]),
        }
    }

    async fn reissue(env: &Env, csrf: &str, id: Uuid) -> (String, DateTime<Utc>) {
        let response = post_empty(
            &env.server,
            csrf,
            &format!("/api/v1/users/{id}/credential-links"),
        )
        .await;
        assert_eq!(response.status_code(), StatusCode::CREATED);
        let body: serde_json::Value = response.json();
        (
            body["token"].as_str().unwrap().to_owned(),
            instant(&body["expires_at"]),
        )
    }

    async fn send(env: &Env, csrf: &str, id: Uuid, token: &str) -> TestResponse {
        post(
            &env.server,
            csrf,
            &format!("/api/v1/users/{id}/credential-links/emails"),
            json!({"token": token}),
        )
        .await
    }

    async fn deliver_next(env: &Env) -> bool {
        process_next(&env.app, &env.runtime, &CancellationToken::new())
            .await
            .unwrap()
    }

    fn assert_notice(message: &mail::Stored, event: Notice, email: &str, recipient: &str) {
        let candidates: Vec<_> = [0, -1, 1]
            .into_iter()
            .map(|offset| {
                render::notice(
                    &context(),
                    event,
                    email,
                    Utc::now() + chrono::TimeDelta::minutes(offset),
                )
            })
            .collect();
        assert!(
            candidates
                .iter()
                .any(|c| c.subject == message.subject && c.body == message.body),
            "unexpected notice: {message:?}"
        );
        assert_eq!(message.recipient, recipient);
    }

    async fn notice_messages(pool: &PgPool, id: Uuid) -> Vec<mail::Stored> {
        mail::messages(pool, id)
            .await
            .into_iter()
            .filter(|m| m.purpose.starts_with("notice_"))
            .collect()
    }

    // ----- administrator link delivery -----

    #[sqlx::test(migrations = "./migrations")]
    async fn smtp_admin_delivery_refuses_other_callers(pool: PgPool) {
        let e = env(&pool).await;
        let (_, csrf) = admin(&e, &pool).await;
        let target = invite(&e, &csrf, "target@example.test").await;
        let send_path = format!("/api/v1/users/{}/credential-links/emails", target.id);
        let status_path = format!(
            "/api/v1/users/{}/credential-links/emails/{}",
            target.id,
            Uuid::new_v4()
        );

        let (_, basic) = test_support::db::create_admin_and_basic_auth(&e.app).await;
        let decoded = base64ct::Base64::decode_vec(basic.strip_prefix("Basic ").unwrap()).unwrap();
        let bearer = format!(
            "Bearer {}",
            String::from_utf8(decoded).unwrap().replace(':', ".")
        );
        let bare = test_support::db::server_with_mail(
            &e.app,
            &e.ingestion,
            test_support::test_config(),
            Arc::clone(&e.runtime),
        );
        for header in [&basic, &bearer] {
            let sent = bare
                .post(&send_path)
                .add_header(auth(header).0, auth(header).1)
                .json(&json!({"token": target.token}))
                .await;
            assert_eq!(sent.status_code(), StatusCode::FORBIDDEN);
            let polled = bare
                .get(&status_path)
                .add_header(auth(header).0, auth(header).1)
                .await;
            assert_eq!(polled.status_code(), StatusCode::FORBIDDEN);
        }

        let adult = another(&e);
        account(&e, "Adult", "adult-session@example.test").await;
        let adult_csrf = mail::login(&adult, "adult-session@example.test", PASSWORD).await;
        let refused = post(
            &adult,
            &adult_csrf,
            &send_path,
            json!({"token": target.token}),
        )
        .await;
        assert_eq!(refused.status_code(), StatusCode::FORBIDDEN);
        assert_eq!(
            adult.get(&status_path).await.status_code(),
            StatusCode::FORBIDDEN
        );

        let anonymous = another(&e);
        let unauthenticated = anonymous
            .post(&send_path)
            .json(&json!({"token": target.token}))
            .await;
        assert_eq!(unauthenticated.status_code(), StatusCode::UNAUTHORIZED);

        let missing = e
            .server
            .post(&send_path)
            .json(&json!({"token": target.token}))
            .await;
        assert_eq!(missing.status_code(), StatusCode::PRECONDITION_REQUIRED);
        let (name, value) = mail::csrf("not-the-session-token");
        let wrong = e
            .server
            .post(&send_path)
            .add_header(name, value)
            .json(&json!({"token": target.token}))
            .await;
        assert_eq!(wrong.status_code(), StatusCode::FORBIDDEN);
        assert_eq!(mail::job_count(&pool, target.id, None).await, 0);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn smtp_admin_delivery_queues_the_live_link_for_the_stored_address(pool: PgPool) {
        let e = env(&pool).await;
        let (_, csrf) = admin(&e, &pool).await;
        let invitee = invite(&e, &csrf, "invitee@example.test").await;

        let response = post(
            &e.server,
            &csrf,
            &format!("/api/v1/users/{}/credential-links/emails", invitee.id),
            json!({
                "token": invitee.token,
                "email": "attacker@example.test",
                "recipient": "attacker@example.test",
                "origin": "https://attacker.example.test",
                "url": "https://attacker.example.test/set-password",
            }),
        )
        .await;
        assert_eq!(response.status_code(), StatusCode::ACCEPTED);
        assert_eq!(response.headers()["cache-control"], "no-store");
        let body: serde_json::Value = response.json();
        assert_eq!(body["status"], "queued");
        assert!(body["failure_category"].is_null());
        let job = body["job_id"].as_str().unwrap().to_owned();

        let stored = mail::messages(&pool, invitee.id).await;
        let [message] = stored.as_slice() else {
            panic!("expected one message: {stored:?}");
        };
        assert_eq!(message.id.to_string(), job);
        assert_eq!(message.purpose, "admin_link");
        assert_eq!(message.recipient, "invitee@example.test");
        let expected = render::admin_link(
            &context(),
            LinkKind::Set,
            "invitee@example.test",
            &invitee.token,
            invitee.expires_at,
        );
        assert_eq!(message.subject, expected.subject);
        assert_eq!(message.body, expected.body);
        assert!(
            message
                .body
                .contains(&format!("{ORIGIN}/set-password#{}", invitee.token))
        );
        assert!(!message.body.contains("attacker"));

        let holder = account(&e, "Holder", "holder@example.test").await;
        let (token, expires_at) = reissue(&e, &csrf, holder).await;
        let sent = send(&e, &csrf, holder, &token).await;
        assert_eq!(sent.status_code(), StatusCode::ACCEPTED);
        let stored = mail::messages(&pool, holder).await;
        let expected = render::admin_link(
            &context(),
            LinkKind::Reset,
            "holder@example.test",
            &token,
            expires_at,
        );
        assert_eq!(stored[0].subject, expected.subject);
        assert_eq!(stored[0].body, expected.body);
        assert!(
            stored[0]
                .body
                .contains(&format!("{ORIGIN}/reset-password#{token}"))
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn smtp_admin_delivery_refuses_forged_stale_and_unusable_links(pool: PgPool) {
        let e = env(&pool).await;
        let (_, csrf) = admin(&e, &pool).await;
        let target = invite(&e, &csrf, "target@example.test").await;
        let other = invite(&e, &csrf, "other@example.test").await;

        for forged in ["A".repeat(43), "short".to_owned(), other.token.clone()] {
            let response = send(&e, &csrf, target.id, &forged).await;
            assert_eq!(
                response.status_code(),
                StatusCode::UNPROCESSABLE_ENTITY,
                "{forged}"
            );
        }
        let (fresh, _) = reissue(&e, &csrf, target.id).await;
        let stale = send(&e, &csrf, target.id, &target.token).await;
        assert_eq!(stale.status_code(), StatusCode::UNPROCESSABLE_ENTITY);

        sqlx::query!(
            "UPDATE credential_links SET expires_at = now() WHERE user_id = $1",
            target.id
        )
        .execute(&pool)
        .await
        .unwrap();
        let expired = send(&e, &csrf, target.id, &fresh).await;
        assert_eq!(expired.status_code(), StatusCode::UNPROCESSABLE_ENTITY);

        let (live, _) = reissue(&e, &csrf, target.id).await;
        sqlx::query!("UPDATE users SET email = NULL WHERE id = $1", target.id)
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(
            send(&e, &csrf, target.id, &live).await.status_code(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        sqlx::query!(
            "UPDATE users SET email = 'target@example.test', disabled_at = now() WHERE id = $1",
            target.id
        )
        .execute(&pool)
        .await
        .unwrap();
        assert_eq!(
            send(&e, &csrf, target.id, &live).await.status_code(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(
            send(&e, &csrf, Uuid::new_v4(), &live).await.status_code(),
            StatusCode::NOT_FOUND
        );
        assert_eq!(mail::job_count(&pool, target.id, None).await, 0);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn smtp_admin_delivery_address_changes_revoke_delivered_links(pool: PgPool) {
        let relay = FakeRelay::start(Behaviour::Accept).await;
        let e = env_on(&pool, relay.port).await;
        let (_, csrf) = admin(&e, &pool).await;
        for (index, email) in [Some("new@example.test"), None].into_iter().enumerate() {
            let invitee = invite(&e, &csrf, &format!("old-{index}@example.test")).await;
            send(&e, &csrf, invitee.id, &invitee.token)
                .await
                .assert_status(StatusCode::ACCEPTED);
            assert!(deliver_next(&e).await);
            patch(
                &e.server,
                &csrf,
                invitee.id,
                json!({"email": format!("OLD-{index}@example.test")}),
            )
            .await
            .assert_status(StatusCode::OK);
            assert!(
                crate::models::credential_link::lookup(&e.app, &invitee.token)
                    .await
                    .unwrap()
                    .is_some()
            );
            patch(&e.server, &csrf, invitee.id, json!({"email": email}))
                .await
                .assert_status(StatusCode::OK);
            if email.is_none() {
                patch(
                    &e.server,
                    &csrf,
                    invitee.id,
                    json!({"email": "restored@example.test"}),
                )
                .await
                .assert_status(StatusCode::OK);
            }
            let response = another(&e)
                .post("/auth/credential-links/redemptions")
                .json(&json!({"token": invitee.token, "password": PASSWORD}))
                .await;
            assert_eq!(response.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
            assert!(
                crate::models::local_credentials::find_by_user_id(&e.app, invitee.id)
                    .await
                    .unwrap()
                    .is_none()
            );
        }
    }

    // ----- mailbox verification -----

    #[sqlx::test(migrations = "./migrations")]
    async fn smtp_verification_endpoints_are_session_only(pool: PgPool) {
        let e = env(&pool).await;
        account(&e, "Holder", "holder@example.test").await;
        mail::login(&e.server, "holder@example.test", PASSWORD).await;
        let (_, basic) = test_support::db::create_admin_and_basic_auth(&e.app).await;
        let decoded = base64ct::Base64::decode_vec(basic.strip_prefix("Basic ").unwrap()).unwrap();
        let bearer = format!(
            "Bearer {}",
            String::from_utf8(decoded).unwrap().replace(':', ".")
        );
        let bare = test_support::db::server_with_mail(
            &e.app,
            &e.ingestion,
            test_support::test_config(),
            Arc::clone(&e.runtime),
        );
        let base = "/api/v1/account/email-verifications";
        let token = json!({"token": "A".repeat(43)});
        for header in [&basic, &bearer] {
            let (name, value) = auth(header);
            assert_eq!(
                bare.get(base)
                    .add_header(name.clone(), value.clone())
                    .await
                    .status_code(),
                StatusCode::FORBIDDEN
            );
            assert_eq!(
                bare.post(base)
                    .add_header(name.clone(), value.clone())
                    .await
                    .status_code(),
                StatusCode::FORBIDDEN
            );
            assert_eq!(
                bare.post(&format!("{base}/confirmations"))
                    .add_header(name, value)
                    .json(&token)
                    .await
                    .status_code(),
                StatusCode::FORBIDDEN
            );
        }
        let anonymous = another(&e);
        assert_eq!(
            anonymous.get(base).await.status_code(),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            anonymous.post(base).await.status_code(),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            anonymous
                .post(&format!("{base}/confirmations"))
                .json(&token)
                .await
                .status_code(),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            e.server.post(base).await.status_code(),
            StatusCode::PRECONDITION_REQUIRED
        );
        assert_eq!(
            e.server
                .post(&format!("{base}/confirmations"))
                .json(&token)
                .await
                .status_code(),
            StatusCode::PRECONDITION_REQUIRED
        );
        let (name, value) = mail::csrf("wrong");
        assert_eq!(
            e.server
                .post(base)
                .add_header(name, value)
                .await
                .status_code(),
            StatusCode::FORBIDDEN
        );
        assert_eq!(e.server.get(base).await.status_code(), StatusCode::OK);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn smtp_verification_request_rechecks_disablement_after_the_account_lock(pool: PgPool) {
        let e = Arc::new(env(&pool).await);
        let id = account(&e, "Holder", "holder@example.test").await;
        let csrf = mail::login(&e.server, "holder@example.test", PASSWORD).await;
        let mut held = e.app.begin().await.unwrap();
        lock_account(&mut held, id).await.unwrap().unwrap();
        let runner = Arc::clone(&e);
        let request = tokio::spawn(async move {
            post_empty(&runner.server, &csrf, "/api/v1/account/email-verifications").await
        });
        test_support::wait_for_blocked_query(&pool, "%users%", 1).await;
        sqlx::query!("UPDATE users SET disabled_at = now() WHERE id = $1", id)
            .execute(&mut *held)
            .await
            .unwrap();
        held.commit().await.unwrap();
        assert_eq!(
            request.await.unwrap().status_code(),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(mail::job_count(&pool, id, None).await, 0);
        assert!(!mail::proof_live(&pool, id, ProofPurpose::Verification.as_str()).await);
    }

    async fn requested_token(server: &TestServer, pool: &PgPool, id: Uuid, csrf: &str) -> String {
        let response = post_empty(server, csrf, "/api/v1/account/email-verifications").await;
        assert_eq!(response.status_code(), StatusCode::ACCEPTED);
        let stored = mail::messages(pool, id).await;
        let message = stored.iter().find(|m| m.purpose == "verification").unwrap();
        mail::token_in(&message.body)
    }

    fn confirm_path() -> &'static str {
        "/api/v1/account/email-verifications/confirmations"
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn smtp_verification_confirmation_belongs_to_the_matching_account(pool: PgPool) {
        let e = env(&pool).await;
        let holder = account(&e, "Holder", "holder@example.test").await;
        account(&e, "Other", "other@example.test").await;
        let csrf = mail::login(&e.server, "holder@example.test", PASSWORD).await;
        let token = requested_token(&e.server, &pool, holder, &csrf).await;

        let other = another(&e);
        let other_csrf = mail::login(&other, "other@example.test", PASSWORD).await;
        let wrong = post(&other, &other_csrf, confirm_path(), json!({"token": token})).await;
        assert_eq!(wrong.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
        let forged = post(
            &e.server,
            &csrf,
            confirm_path(),
            json!({"token": "A".repeat(43)}),
        )
        .await;
        assert_eq!(forged.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(
            wrong.json::<serde_json::Value>()["detail"],
            forged.json::<serde_json::Value>()["detail"]
        );
        assert!(
            mail::proof_live(&pool, holder, "verification").await,
            "a foreign attempt consumes nothing"
        );

        let reset_path = another(&e)
            .post("/auth/credential-links/lookups")
            .json(&json!({"token": token}))
            .await;
        assert_eq!(reset_path.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn smtp_verification_confirmation_refuses_a_challenge_for_a_former_address(pool: PgPool) {
        let e = env(&pool).await;
        let (_, admin_csrf) = admin(&e, &pool).await;
        let holder = account(&e, "Holder", "holder@example.test").await;
        let session = another(&e);
        let csrf = mail::login(&session, "holder@example.test", PASSWORD).await;
        let response = post_empty(&session, &csrf, "/api/v1/account/email-verifications").await;
        assert_eq!(response.status_code(), StatusCode::ACCEPTED);
        let token = mail::token_in(&mail::messages(&pool, holder).await[0].body);

        let moved = patch(
            &e.server,
            &admin_csrf,
            holder,
            json!({"email": "moved@example.test"}),
        )
        .await;
        assert_eq!(moved.status_code(), StatusCode::OK);
        let late = post(&session, &csrf, confirm_path(), json!({"token": token})).await;
        assert_eq!(late.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
        assert!(
            sqlx::query_scalar!("SELECT email_verified_at FROM users WHERE id = $1", holder)
                .fetch_one(&pool)
                .await
                .unwrap()
                .is_none()
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn smtp_verification_confirmation_rechecks_expiry_after_the_account_lock(pool: PgPool) {
        let e = Arc::new(env(&pool).await);
        let id = account(&e, "Holder", "holder@example.test").await;
        let csrf = mail::login(&e.server, "holder@example.test", PASSWORD).await;
        let token = requested_token(&e.server, &pool, id, &csrf).await;
        let mut held = e.app.begin().await.unwrap();
        lock_account(&mut held, id).await.unwrap().unwrap();
        let runner = Arc::clone(&e);
        let (submitted, header) = (token.clone(), csrf.clone());
        let confirmation = tokio::spawn(async move {
            post(
                &runner.server,
                &header,
                confirm_path(),
                json!({"token": submitted}),
            )
            .await
        });
        test_support::wait_for_blocked_query(&pool, "%users%", 1).await;
        sqlx::query!(
            "UPDATE account_email_proofs SET expires_at = clock_timestamp() WHERE user_id = $1",
            id
        )
        .execute(&mut *held)
        .await
        .unwrap();
        held.commit().await.unwrap();
        assert_eq!(
            confirmation.await.unwrap().status_code(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert!(
            sqlx::query_scalar!("SELECT email_verified_at FROM users WHERE id = $1", id)
                .fetch_one(&pool)
                .await
                .unwrap()
                .is_none()
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn smtp_verification_initial_account_writers_leave_the_address_unverified(pool: PgPool) {
        let e = env(&pool).await;
        let setup = e
            .server
            .post("/auth/setup")
            .json(&json!({"email": "first@example.test", "display_name": "First", "password": PASSWORD}))
            .await;
        assert_eq!(setup.status_code(), StatusCode::CREATED);
        let csrf = mail::login(&e.server, "first@example.test", PASSWORD).await;
        let created = invite(&e, &csrf, "created@example.test").await;
        let oidc = user::upsert_from_oidc(
            &e.app,
            "https://issuer.example.test",
            "subject-1",
            "Oidc",
            Some("oidc@example.test"),
        )
        .await
        .unwrap();
        let mut config = test_support::test_config();
        config.self_registration_enabled = true;
        let open = test_support::db::server_with_config(&e.app, &e.ingestion, config);
        let registered = open
            .post("/auth/register")
            .json(&json!({"email": "registered@example.test", "display_name": "Registered", "password": PASSWORD}))
            .await;
        assert_eq!(registered.status_code(), StatusCode::CREATED);
        let unverified = sqlx::query_scalar!(
            r#"SELECT count(*) AS "n!" FROM users WHERE email_verified_at IS NOT NULL"#
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(unverified, 0);
        let total = sqlx::query_scalar!(r#"SELECT count(*) AS "n!" FROM users"#)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(total, 4, "{} {}", created.id, oidc.id);
    }

    // ----- notifications -----

    #[sqlx::test(migrations = "./migrations")]
    async fn smtp_notifications_own_password_change_notifies_and_keeps_replacement_proofs(
        pool: PgPool,
    ) {
        let e = env(&pool).await;
        let (_, admin_csrf) = admin(&e, &pool).await;
        let id = verified(&e, &pool, "Holder", "holder@example.test").await;
        let (link, link_expiry) = reissue(&e, &admin_csrf, id).await;
        let mut tx = e.app.begin().await.unwrap();
        lock_account(&mut tx, id).await.unwrap();
        let outbox = outbox(&e);
        let admin_job = outbox
            .admin_link(&mut tx, id, "holder@example.test", true, &link, link_expiry)
            .await
            .unwrap();
        outbox
            .request_proof(&mut tx, id, ProofPurpose::Recovery, "holder@example.test")
            .await
            .unwrap();
        outbox
            .request_proof(
                &mut tx,
                id,
                ProofPurpose::Verification,
                "holder@example.test",
            )
            .await
            .unwrap();
        tx.commit().await.unwrap();

        let session = another(&e);
        let csrf = mail::login(&session, "holder@example.test", PASSWORD).await;
        let response = post(
            &session,
            &csrf,
            "/api/v1/account/password",
            json!({"current_password": PASSWORD, "new_password": "amber thistle vault ridge"}),
        )
        .await;
        assert_eq!(response.status_code(), StatusCode::OK);
        assert_eq!(
            session.get("/auth/me").await.status_code(),
            StatusCode::UNAUTHORIZED
        );

        let notices = notice_messages(&pool, id).await;
        let [notice] = notices.as_slice() else {
            panic!("expected one notice: {notices:?}");
        };
        assert_notice(
            notice,
            Notice::PasswordChanged,
            "holder@example.test",
            "holder@example.test",
        );
        assert_eq!(
            sqlx::query_scalar!(
                r#"SELECT count(*) AS "n!" FROM credential_links WHERE user_id = $1"#,
                id
            )
            .fetch_one(&pool)
            .await
            .unwrap(),
            1,
            "own-password change keeps the administrator link"
        );
        for purpose in ["recovery", "verification"] {
            assert!(mail::proof_live(&pool, id, purpose).await, "{purpose}");
        }
        assert_eq!(mail::state_of(&pool, admin_job).await, "pending");
        let pending = mail::messages(&pool, id)
            .await
            .into_iter()
            .filter(|m| !m.purpose.starts_with("notice_") && m.state == "pending")
            .count();
        assert_eq!(pending, 3, "all three bearer deliveries stay queued");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn smtp_notifications_link_redemption_tells_first_password_from_change(pool: PgPool) {
        let e = env(&pool).await;
        let (_, csrf) = admin(&e, &pool).await;
        let invitee = invite(&e, &csrf, "invitee@example.test").await;
        mail::verify_address(&pool, invitee.id).await;
        let holder = verified(&e, &pool, "Holder", "holder@example.test").await;
        let (reset, _) = reissue(&e, &csrf, holder).await;
        let unverified = account(&e, "Plain", "plain@example.test").await;
        let (plain, _) = reissue(&e, &csrf, unverified).await;

        let public = another(&e);
        for (token, expected) in [
            (invitee.token.as_str(), 204),
            (reset.as_str(), 204),
            (plain.as_str(), 204),
        ] {
            let response = public
                .post("/auth/credential-links/redemptions")
                .json(&json!({"token": token, "password": "amber thistle vault ridge"}))
                .await;
            assert_eq!(response.status_code().as_u16(), expected);
        }
        let first = notice_messages(&pool, invitee.id).await;
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].purpose, "notice_password_set");
        assert_notice(
            &first[0],
            Notice::PasswordSet,
            "invitee@example.test",
            "invitee@example.test",
        );
        let changed = notice_messages(&pool, holder).await;
        assert_eq!(changed.len(), 1);
        assert_eq!(changed[0].purpose, "notice_password_changed");
        assert_notice(
            &changed[0],
            Notice::PasswordChanged,
            "holder@example.test",
            "holder@example.test",
        );
        assert_eq!(mail::job_count(&pool, unverified, None).await, 0);
    }

    async fn change_address_fixture(e: &Env, pool: &PgPool) -> (Uuid, String) {
        let (_, csrf) = admin(e, pool).await;
        let id = verified(e, pool, "Holder", "holder@example.test").await;
        (id, csrf)
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn smtp_notifications_address_change_notifies_the_old_verified_address(pool: PgPool) {
        let e = env(&pool).await;
        let (id, csrf) = change_address_fixture(&e, &pool).await;
        let outbox = outbox(&e);
        let mut tx = e.app.begin().await.unwrap();
        lock_account(&mut tx, id).await.unwrap();
        outbox
            .request_proof(&mut tx, id, ProofPurpose::Recovery, "holder@example.test")
            .await
            .unwrap();
        outbox
            .request_proof(
                &mut tx,
                id,
                ProofPurpose::Verification,
                "holder@example.test",
            )
            .await
            .unwrap();
        tx.commit().await.unwrap();

        let response = patch(
            &e.server,
            &csrf,
            id,
            json!({"email": "new-home@example.test"}),
        )
        .await;
        assert_eq!(response.status_code(), StatusCode::OK);
        let row = sqlx::query!(
            "SELECT email, email_verified_at FROM users WHERE id = $1",
            id
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(row.email.as_deref(), Some("new-home@example.test"));
        assert!(row.email_verified_at.is_none());
        for purpose in ["recovery", "verification"] {
            assert!(!mail::proof_live(&pool, id, purpose).await, "{purpose}");
        }
        let all = mail::messages(&pool, id).await;
        assert_eq!(
            all.len(),
            1,
            "bearer mail is withdrawn and only the notice remains: {all:?}"
        );
        assert_eq!(all[0].purpose, "notice_address_changed");
        assert_notice(
            &all[0],
            Notice::AddressChanged,
            "holder@example.test",
            "holder@example.test",
        );
        assert_eq!(
            sqlx::query_scalar!(
                "SELECT count(*) FROM mail_jobs WHERE user_id = $1 AND state = 'cancelled'",
                id
            )
            .fetch_one(&pool)
            .await
            .unwrap(),
            Some(2)
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn smtp_notifications_address_removal_notifies_the_old_verified_address(pool: PgPool) {
        let e = env(&pool).await;
        let (id, csrf) = change_address_fixture(&e, &pool).await;
        let response = patch(&e.server, &csrf, id, json!({"email": null})).await;
        assert_eq!(response.status_code(), StatusCode::OK);
        let notices = notice_messages(&pool, id).await;
        let [notice] = notices.as_slice() else {
            panic!("expected one notice: {notices:?}");
        };
        assert_eq!(notice.purpose, "notice_address_removed");
        assert_notice(
            notice,
            Notice::AddressRemoved,
            "holder@example.test",
            "holder@example.test",
        );
        assert_ne!(
            notice.subject,
            render::notice(
                &context(),
                Notice::AddressChanged,
                "holder@example.test",
                Utc::now()
            )
            .subject
        );
        let again = patch(&e.server, &csrf, id, json!({"email": null})).await;
        assert_eq!(again.status_code(), StatusCode::OK);
        assert_eq!(
            notice_messages(&pool, id).await.len(),
            1,
            "removing nothing notifies nobody"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn smtp_notifications_queued_notices_keep_their_recipient_through_changes(pool: PgPool) {
        let relay = FakeRelay::start(Behaviour::Accept).await;
        let e = env_on(&pool, relay.port).await;
        let (id, csrf) = change_address_fixture(&e, &pool).await;
        let session = another(&e);
        let session_csrf = mail::login(&session, "holder@example.test", PASSWORD).await;
        let changed = post(
            &session,
            &session_csrf,
            "/api/v1/account/password",
            json!({"current_password": PASSWORD, "new_password": "amber thistle vault ridge"}),
        )
        .await;
        assert_eq!(changed.status_code(), StatusCode::OK);

        let moved = patch(
            &e.server,
            &csrf,
            id,
            json!({"email": "new-home@example.test"}),
        )
        .await;
        assert_eq!(moved.status_code(), StatusCode::OK);
        let disabled = e
            .server
            .put(&format!("/api/v1/users/{id}/account-status"))
            .add_header(mail::csrf(&csrf).0, mail::csrf(&csrf).1)
            .json(&json!({"disabled": true}))
            .await;
        assert_eq!(disabled.status_code(), StatusCode::OK);

        let notices = notice_messages(&pool, id).await;
        assert_eq!(notices.len(), 2);
        assert!(
            notices
                .iter()
                .all(|n| n.state == "pending" && n.recipient == "holder@example.test")
        );
        while deliver_next(&e).await {}
        let recipients = relay.recipients();
        assert_eq!(recipients.len(), 2, "{recipients:?}");
        assert!(
            recipients.iter().all(|r| r.contains("holder@example.test")),
            "{recipients:?}"
        );
        assert!(!recipients.iter().any(|r| r.contains("new-home")));
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn smtp_notifications_roll_back_the_account_change_when_enqueue_fails(pool: PgPool) {
        let e = env(&pool).await;
        let (id, csrf) = change_address_fixture(&e, &pool).await;
        mail::break_enqueue(&pool).await;

        let moved = patch(
            &e.server,
            &csrf,
            id,
            json!({"email": "new-home@example.test"}),
        )
        .await;
        assert_eq!(moved.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
        let row = sqlx::query!(
            "SELECT email, email_verified_at FROM users WHERE id = $1",
            id
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(row.email.as_deref(), Some("holder@example.test"));
        assert!(row.email_verified_at.is_some());

        let session = another(&e);
        let session_csrf = mail::login(&session, "holder@example.test", PASSWORD).await;
        let changed = post(
            &session,
            &session_csrf,
            "/api/v1/account/password",
            json!({"current_password": PASSWORD, "new_password": "amber thistle vault ridge"}),
        )
        .await;
        assert_eq!(changed.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
        let credential = crate::models::local_credentials::find_by_user_id(&e.app, id)
            .await
            .unwrap()
            .unwrap();
        assert!(
            crate::auth::password::verify_password(PASSWORD.as_bytes(), &credential.password_hash)
                .is_ok()
        );
        assert_eq!(
            session.get("/auth/me").await.status_code(),
            StatusCode::OK,
            "the session survives a rolled-back change"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn smtp_notifications_raw_logs_carry_no_secrets(pool: PgPool) {
        let logs = test_support::CapturedLogs::default();
        let _guard = tracing::subscriber::set_default(logs.trace_subscriber());
        let e = env(&pool).await;
        let (_, csrf) = admin(&e, &pool).await;
        let invitee = invite(&e, &csrf, "private-invitee@example.test").await;
        send(&e, &csrf, invitee.id, &invitee.token).await;
        let holder = account(&e, "Holder", "private-holder@example.test").await;
        let session = another(&e);
        let session_csrf = mail::login(&session, "private-holder@example.test", PASSWORD).await;
        let token = requested_token(&session, &pool, holder, &session_csrf).await;
        let confirmed = post(
            &session,
            &session_csrf,
            confirm_path(),
            json!({"token": token}),
        )
        .await;
        assert_eq!(confirmed.status_code(), StatusCode::NO_CONTENT);
        patch(
            &e.server,
            &csrf,
            holder,
            json!({"email": "private-new@example.test"}),
        )
        .await;
        let captured = logs.text();
        assert!(
            captured.contains("credential_link_email_queued"),
            "the capture must see the request path"
        );
        for secret in [
            invitee.token.as_str(),
            token.as_str(),
            PASSWORD,
            "private-invitee@example.test",
            "private-holder@example.test",
            "private-new@example.test",
            "Set your Reverie password",
        ] {
            assert!(
                !captured.contains(secret),
                "a log line leaked a secret value"
            );
        }
    }
}
