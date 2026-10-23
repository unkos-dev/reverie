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

#[sqlx::test(migrations = "./migrations")]
async fn list_users_as_admin_returns_all(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin_id, admin_basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let (_adult_id, _) =
        test_support::db::create_adult_and_basic_auth(&app_pool, "list-adult").await;
    let (_child_id, _) =
        test_support::db::create_child_user_and_basic_auth(&app_pool, "list-child").await;

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let r = server
        .get("/api/v1/users")
        .add_header(auth(&admin_basic).0, auth(&admin_basic).1)
        .await;
    assert_eq!(r.status_code(), StatusCode::OK);
    let body: Vec<serde_json::Value> = r.json();
    assert!(
        body.len() >= 3,
        "expected at least 3 users, got {}",
        body.len()
    );
    let roles: Vec<&str> = body.iter().map(|u| u["role"].as_str().unwrap()).collect();
    assert!(roles.contains(&"admin"));
    assert!(roles.contains(&"adult"));
    assert!(roles.contains(&"child"));
}

#[sqlx::test(migrations = "./migrations")]
async fn list_users_serializes_timestamps_as_rfc3339_strings(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin_id, admin_basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let r = server
        .get("/api/v1/users")
        .add_header(auth(&admin_basic).0, auth(&admin_basic).1)
        .await;
    assert_eq!(r.status_code(), StatusCode::OK);

    // The frontend user Zod schema parses created_at/updated_at as strings, so
    // both must reach the wire as RFC 3339 strings rather than any structured
    // form.
    let body: Vec<serde_json::Value> = r.json();
    let created = body[0]["created_at"]
        .as_str()
        .expect("created_at is a JSON string");
    let updated = body[0]["updated_at"]
        .as_str()
        .expect("updated_at is a JSON string");
    assert!(chrono::DateTime::parse_from_rfc3339(created).is_ok());
    assert!(chrono::DateTime::parse_from_rfc3339(updated).is_ok());
}

#[sqlx::test(migrations = "./migrations")]
async fn list_users_as_adult_returns_403(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_adult_id, adult_basic) =
        test_support::db::create_adult_and_basic_auth(&app_pool, "forbidden-adult").await;

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let r = server
        .get("/api/v1/users")
        .add_header(auth(&adult_basic).0, auth(&adult_basic).1)
        .await;
    test_support::assert_problem(&r, problems::FORBIDDEN, StatusCode::FORBIDDEN);
}

#[sqlx::test(migrations = "./migrations")]
async fn list_users_as_child_returns_403(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_child_id, child_basic) =
        test_support::db::create_child_user_and_basic_auth(&app_pool, "forbidden-child").await;

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let r = server
        .get("/api/v1/users")
        .add_header(auth(&child_basic).0, auth(&child_basic).1)
        .await;
    test_support::assert_problem(&r, problems::FORBIDDEN, StatusCode::FORBIDDEN);
}

// ---------- PUT /api/v1/users/{id}/role ----------

#[sqlx::test(migrations = "./migrations")]
async fn promote_adult_to_admin(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin_id, admin_basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let (adult_id, _) =
        test_support::db::create_adult_and_basic_auth(&app_pool, "promote-target").await;

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let r = server
        .put(&format!("/api/v1/users/{adult_id}/role"))
        .add_header(auth(&admin_basic).0, auth(&admin_basic).1)
        .json(&json!({"role": "admin"}))
        .await;
    assert_eq!(r.status_code(), StatusCode::OK);
    let body: serde_json::Value = r.json();
    assert_eq!(body["role"], "admin");

    // session_version bumped
    let sv: i32 = sqlx::query_scalar!(
        r#"SELECT session_version AS "sv!" FROM users WHERE id = $1"#,
        adult_id,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(sv > 0, "session_version should be bumped after role change");
}

#[sqlx::test(migrations = "./migrations")]
async fn demote_admin_to_adult_with_multiple_admins(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin1_id, admin1_basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    // Create a second admin.
    let (admin2_id, _) =
        test_support::db::create_adult_and_basic_auth(&app_pool, "admin2-demote").await;
    sqlx::query!(
        "UPDATE users SET role = 'admin'::user_role WHERE id = $1",
        admin2_id
    )
    .execute(&app_pool)
    .await
    .unwrap();

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let r = server
        .put(&format!("/api/v1/users/{admin2_id}/role"))
        .add_header(auth(&admin1_basic).0, auth(&admin1_basic).1)
        .json(&json!({"role": "adult"}))
        .await;
    assert_eq!(r.status_code(), StatusCode::OK);
    let body: serde_json::Value = r.json();
    assert_eq!(body["role"], "adult");
}

#[sqlx::test(migrations = "./migrations")]
async fn demote_sole_admin_returns_422(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (admin_id, admin_basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    for role in ["adult", "child"] {
        let r = server
            .put(&format!("/api/v1/users/{admin_id}/role"))
            .add_header(auth(&admin_basic).0, auth(&admin_basic).1)
            .json(&json!({"role": role}))
            .await;
        let problem = test_support::assert_problem(
            &r,
            problems::VALIDATION,
            StatusCode::UNPROCESSABLE_ENTITY,
        );
        assert_eq!(problem["detail"], "would leave zero admins");
        let stored = crate::models::user::find_by_id(&app_pool, admin_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stored.role.as_str(), "admin");
        assert!(!stored.is_child);
        assert_eq!(stored.session_version, 0);
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn non_admin_put_role_returns_403(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (adult_id, adult_basic) =
        test_support::db::create_adult_and_basic_auth(&app_pool, "no-role-change").await;

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let r = server
        .put(&format!("/api/v1/users/{adult_id}/role"))
        .add_header(auth(&adult_basic).0, auth(&adult_basic).1)
        .json(&json!({"role": "admin"}))
        .await;
    test_support::assert_problem(&r, problems::FORBIDDEN, StatusCode::FORBIDDEN);
}

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

#[sqlx::test(migrations = "./migrations")]
async fn put_role_nonexistent_user_returns_404(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin_id, admin_basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;

    let fake_id = Uuid::new_v4();
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let r = server
        .put(&format!("/api/v1/users/{fake_id}/role"))
        .add_header(auth(&admin_basic).0, auth(&admin_basic).1)
        .json(&json!({"role": "adult"}))
        .await;
    test_support::assert_problem(&r, problems::NOT_FOUND, StatusCode::NOT_FOUND);
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

#[sqlx::test(migrations = "./migrations")]
async fn toggle_child_off_reverts_role_to_adult(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin_id, admin_basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let (child_id, _) =
        test_support::db::create_child_user_and_basic_auth(&app_pool, "unchild").await;

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let r = server
        .put(&format!("/api/v1/users/{child_id}/child-status"))
        .add_header(auth(&admin_basic).0, auth(&admin_basic).1)
        .json(&json!({"is_child": false}))
        .await;
    assert_eq!(r.status_code(), StatusCode::OK);
    let body: serde_json::Value = r.json();
    assert_eq!(body["is_child"], false);
    assert_eq!(body["role"], "adult");
}

#[sqlx::test(migrations = "./migrations")]
async fn toggle_child_on_sole_admin_returns_422(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (admin_id, admin_basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let r = server
        .put(&format!("/api/v1/users/{admin_id}/child-status"))
        .add_header(auth(&admin_basic).0, auth(&admin_basic).1)
        .json(&json!({"is_child": true}))
        .await;
    let problem =
        test_support::assert_problem(&r, problems::VALIDATION, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        problem["detail"].as_str().unwrap(),
        "would leave zero admins"
    );
}

// ---------- PATCH /api/v1/users/{id} ----------

#[sqlx::test(migrations = "./migrations")]
async fn patch_user_updates_display_name_and_email(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin_id, admin_basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let (adult_id, _) =
        test_support::db::create_adult_and_basic_auth(&app_pool, "patch-target").await;

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let r = server
        .patch(&format!("/api/v1/users/{adult_id}"))
        .add_header(auth(&admin_basic).0, auth(&admin_basic).1)
        .json(&json!({"display_name": "Updated Name", "email": "new@example.com"}))
        .await;
    assert_eq!(r.status_code(), StatusCode::OK);
    let body: serde_json::Value = r.json();
    assert_eq!(body["display_name"], "Updated Name");
    assert_eq!(body["email"], "new@example.com");
}

#[sqlx::test(migrations = "./migrations")]
async fn patch_user_null_email_clears(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin_id, admin_basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let (adult_id, _) =
        test_support::db::create_adult_and_basic_auth(&app_pool, "clear-email").await;
    // Set an email first.
    sqlx::query!(
        "UPDATE users SET email = 'old@example.com' WHERE id = $1",
        adult_id
    )
    .execute(&pool)
    .await
    .unwrap();

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let r = server
        .patch(&format!("/api/v1/users/{adult_id}"))
        .add_header(auth(&admin_basic).0, auth(&admin_basic).1)
        .json(&json!({"email": null}))
        .await;
    assert_eq!(r.status_code(), StatusCode::OK);
    let body: serde_json::Value = r.json();
    assert!(body["email"].is_null(), "email should be cleared");
}

#[sqlx::test(migrations = "./migrations")]
async fn patch_user_null_display_name_returns_422(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin_id, admin_basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let (adult_id, _) = test_support::db::create_adult_and_basic_auth(&app_pool, "null-name").await;

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let r = server
        .patch(&format!("/api/v1/users/{adult_id}"))
        .add_header(auth(&admin_basic).0, auth(&admin_basic).1)
        .json(&json!({"display_name": null}))
        .await;
    let problem =
        test_support::assert_problem(&r, problems::VALIDATION, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(problem["detail"].as_str().unwrap().contains("display_name"),);
}

#[sqlx::test(migrations = "./migrations")]
async fn patch_user_duplicate_email_returns_422(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin_id, admin_basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let (user1_id, _) =
        test_support::db::create_adult_and_basic_auth(&app_pool, "dup-email-1").await;
    let (user2_id, _) =
        test_support::db::create_adult_and_basic_auth(&app_pool, "dup-email-2").await;
    sqlx::query!(
        "UPDATE users SET email = 'shared@example.com' WHERE id = $1",
        user1_id
    )
    .execute(&pool)
    .await
    .unwrap();

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let r = server
        .patch(&format!("/api/v1/users/{user2_id}"))
        .add_header(auth(&admin_basic).0, auth(&admin_basic).1)
        .json(&json!({"email": "shared@example.com"}))
        .await;
    let problem =
        test_support::assert_problem(&r, problems::VALIDATION, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(problem["detail"].as_str().unwrap(), "email already in use");
}

#[sqlx::test(migrations = "./migrations")]
async fn patch_user_case_insensitive_email_uniqueness(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin_id, admin_basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let (user1_id, _) =
        test_support::db::create_adult_and_basic_auth(&app_pool, "ci-email-1").await;
    let (user2_id, _) =
        test_support::db::create_adult_and_basic_auth(&app_pool, "ci-email-2").await;
    sqlx::query!(
        "UPDATE users SET email = 'Alice@Example.com' WHERE id = $1",
        user1_id
    )
    .execute(&pool)
    .await
    .unwrap();

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let r = server
        .patch(&format!("/api/v1/users/{user2_id}"))
        .add_header(auth(&admin_basic).0, auth(&admin_basic).1)
        .json(&json!({"email": "alice@example.com"}))
        .await;
    let problem =
        test_support::assert_problem(&r, problems::VALIDATION, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(problem["detail"].as_str().unwrap(), "email already in use");
}

#[sqlx::test(migrations = "./migrations")]
async fn patch_nonexistent_user_returns_404(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin_id, admin_basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;

    let fake_id = Uuid::new_v4();
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let r = server
        .patch(&format!("/api/v1/users/{fake_id}"))
        .add_header(auth(&admin_basic).0, auth(&admin_basic).1)
        .json(&json!({"display_name": "Ghost"}))
        .await;
    test_support::assert_problem(&r, problems::NOT_FOUND, StatusCode::NOT_FOUND);
}

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

#[sqlx::test(migrations = "./migrations")]
async fn concurrent_demote_last_two_admins_one_succeeds_one_fails(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (admin1_id, admin1_basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    // Second admin.
    let (admin2_id, _) =
        test_support::db::create_adult_and_basic_auth(&app_pool, "admin2-concurrent").await;
    sqlx::query!(
        "UPDATE users SET role = 'admin'::user_role WHERE id = $1",
        admin2_id
    )
    .execute(&app_pool)
    .await
    .unwrap();
    // Third admin so we can demote one and still have two left for
    // the real race test.
    let (admin3_id, _) =
        test_support::db::create_adult_and_basic_auth(&app_pool, "admin3-credential").await;
    sqlx::query!(
        "UPDATE users SET role = 'admin'::user_role WHERE id = $1",
        admin3_id
    )
    .execute(&app_pool)
    .await
    .unwrap();

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

    // admin1 demotes admin2, admin3 demotes admin1 concurrently.
    // With 3 admins, both initial demotions should succeed (each
    // leaves ≥2 admins). Let's test the real last-admin scenario:
    // first demote admin3 down to 2 admins, then race the last two.
    let r = server
        .put(&format!("/api/v1/users/{admin3_id}/role"))
        .add_header(auth(&admin1_basic).0, auth(&admin1_basic).1)
        .json(&json!({"role": "adult"}))
        .await;
    assert_eq!(
        r.status_code(),
        StatusCode::OK,
        "pre-demote admin3 should succeed"
    );

    // Now only admin1 and admin2 remain. Race demotions.
    // We can't truly race with axum_test (single-threaded), but we
    // can verify the constraint holds sequentially: first one succeeds,
    // second fails.
    let r1 = server
        .put(&format!("/api/v1/users/{admin2_id}/role"))
        .add_header(auth(&admin1_basic).0, auth(&admin1_basic).1)
        .json(&json!({"role": "adult"}))
        .await;
    assert_eq!(
        r1.status_code(),
        StatusCode::OK,
        "first demotion (admin2→adult) should succeed with 2 admins"
    );

    // Now only admin1 remains. Self-demotion must fail.
    let r2 = server
        .put(&format!("/api/v1/users/{admin1_id}/role"))
        .add_header(auth(&admin1_basic).0, auth(&admin1_basic).1)
        .json(&json!({"role": "adult"}))
        .await;
    let problem =
        test_support::assert_problem(&r2, problems::VALIDATION, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        problem["detail"].as_str().unwrap(),
        "would leave zero admins"
    );
}

// ---------- 403 guards: child-status PUT and PATCH ----------

#[sqlx::test(migrations = "./migrations")]
async fn non_admin_put_child_status_returns_403(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (adult_id, adult_basic) =
        test_support::db::create_adult_and_basic_auth(&app_pool, "no-child-toggle").await;

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let r = server
        .put(&format!("/api/v1/users/{adult_id}/child-status"))
        .add_header(auth(&adult_basic).0, auth(&adult_basic).1)
        .json(&json!({"is_child": true}))
        .await;
    test_support::assert_problem(&r, problems::FORBIDDEN, StatusCode::FORBIDDEN);
}

#[sqlx::test(migrations = "./migrations")]
async fn non_admin_patch_user_returns_403(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (adult_id, adult_basic) =
        test_support::db::create_adult_and_basic_auth(&app_pool, "no-patch").await;

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let r = server
        .patch(&format!("/api/v1/users/{adult_id}"))
        .add_header(auth(&adult_basic).0, auth(&adult_basic).1)
        .json(&json!({"display_name": "Hacker"}))
        .await;
    test_support::assert_problem(&r, problems::FORBIDDEN, StatusCode::FORBIDDEN);
}

// ---------- PATCH whitespace validation ----------

#[sqlx::test(migrations = "./migrations")]
async fn patch_user_whitespace_display_name_returns_422(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin_id, admin_basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let (adult_id, _) = test_support::db::create_adult_and_basic_auth(&app_pool, "ws-name").await;

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let r = server
        .patch(&format!("/api/v1/users/{adult_id}"))
        .add_header(auth(&admin_basic).0, auth(&admin_basic).1)
        .json(&json!({"display_name": "   "}))
        .await;
    let problem =
        test_support::assert_problem(&r, problems::VALIDATION, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        problem["detail"].as_str().unwrap().contains("display_name"),
        "expected display_name in error, got: {}",
        problem["detail"],
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn patch_user_whitespace_email_returns_422(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin_id, admin_basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let (adult_id, _) = test_support::db::create_adult_and_basic_auth(&app_pool, "ws-email").await;

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let r = server
        .patch(&format!("/api/v1/users/{adult_id}"))
        .add_header(auth(&admin_basic).0, auth(&admin_basic).1)
        .json(&json!({"email": "   "}))
        .await;
    let problem =
        test_support::assert_problem(&r, problems::VALIDATION, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        problem["detail"].as_str().unwrap().contains("email"),
        "expected email in error, got: {}",
        problem["detail"],
    );
}

// ---------- child/role sync: mirror branch ----------

#[sqlx::test(migrations = "./migrations")]
async fn set_admin_role_on_child_user_clears_child_status(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin_id, admin_basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let (child_id, _) =
        test_support::db::create_child_user_and_basic_auth(&app_pool, "role-sync-mirror").await;

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let r = server
        .put(&format!("/api/v1/users/{child_id}/role"))
        .add_header(auth(&admin_basic).0, auth(&admin_basic).1)
        .json(&json!({"role": "admin"}))
        .await;
    assert_eq!(r.status_code(), StatusCode::OK);
    let body: serde_json::Value = r.json();
    assert_eq!(body["role"], "admin");
    assert_eq!(body["is_child"], false);
}

// ---------- PATCH session_version bump policy ----------

#[sqlx::test(migrations = "./migrations")]
async fn patch_email_does_not_bump_session_version(pool: PgPool) {
    // Email gates nothing in the authz model — login identity is the OIDC `sub`,
    // RLS keys on user id/role/is_child, and the session auth hash is
    // session_version only. Changing email must NOT force a logout-everywhere.
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin_id, admin_basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let (adult_id, _) = test_support::db::create_adult_and_basic_auth(&app_pool, "sv-email").await;

    let sv_before: i32 = sqlx::query_scalar!(
        r#"SELECT session_version AS "sv!" FROM users WHERE id = $1"#,
        adult_id,
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let r = server
        .patch(&format!("/api/v1/users/{adult_id}"))
        .add_header(auth(&admin_basic).0, auth(&admin_basic).1)
        .json(&json!({"email": "sv-test@example.com"}))
        .await;
    assert_eq!(r.status_code(), StatusCode::OK);

    let sv_after: i32 = sqlx::query_scalar!(
        r#"SELECT session_version AS "sv!" FROM users WHERE id = $1"#,
        adult_id,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        sv_after, sv_before,
        "session_version should NOT bump on email change"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn patch_email_null_does_not_bump_session_version(pool: PgPool) {
    // Clearing email is the same non-security-gating mutation as setting it.
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin_id, admin_basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let (adult_id, _) =
        test_support::db::create_adult_and_basic_auth(&app_pool, "sv-null-email").await;
    sqlx::query!(
        "UPDATE users SET email = 'old@example.com' WHERE id = $1",
        adult_id,
    )
    .execute(&pool)
    .await
    .unwrap();

    let sv_before: i32 = sqlx::query_scalar!(
        r#"SELECT session_version AS "sv!" FROM users WHERE id = $1"#,
        adult_id,
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let r = server
        .patch(&format!("/api/v1/users/{adult_id}"))
        .add_header(auth(&admin_basic).0, auth(&admin_basic).1)
        .json(&json!({"email": null}))
        .await;
    assert_eq!(r.status_code(), StatusCode::OK);

    let sv_after: i32 = sqlx::query_scalar!(
        r#"SELECT session_version AS "sv!" FROM users WHERE id = $1"#,
        adult_id,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        sv_after, sv_before,
        "session_version should NOT bump on email clear"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn patch_display_name_only_does_not_bump_session_version(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin_id, admin_basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let (adult_id, _) =
        test_support::db::create_adult_and_basic_auth(&app_pool, "sv-name-only").await;

    let sv_before: i32 = sqlx::query_scalar!(
        r#"SELECT session_version AS "sv!" FROM users WHERE id = $1"#,
        adult_id,
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let r = server
        .patch(&format!("/api/v1/users/{adult_id}"))
        .add_header(auth(&admin_basic).0, auth(&admin_basic).1)
        .json(&json!({"display_name": "New Name Only"}))
        .await;
    assert_eq!(r.status_code(), StatusCode::OK);

    let sv_after: i32 = sqlx::query_scalar!(
        r#"SELECT session_version AS "sv!" FROM users WHERE id = $1"#,
        adult_id,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        sv_after, sv_before,
        "session_version should NOT bump on display_name-only change"
    );
}

// ---------- POST /api/v1/users (create / invite) ----------

/// A passphrase that clears the zxcvbn floor. The breach check is disabled in
/// the test config, so this never reaches HIBP.
const STRONG_PW: &str = "correct-horse-battery-staple-7!";

#[sqlx::test(migrations = "./migrations")]
async fn create_user_as_admin_creates_adult(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (server, csrf, _) = credential_link_admin(&app_pool, &ingestion_pool).await;

    let r = server
        .post("/api/v1/users")
        .add_header(csrf_header(&csrf).0, csrf_header(&csrf).1)
        .json(&json!({
            "email": "new-adult@example.com",
            "display_name": "New Adult",
            "role": "adult",
        }))
        .await;
    assert_eq!(r.status_code(), StatusCode::CREATED);
    let body = r.json::<serde_json::Value>()["user"].clone();
    assert_eq!(body["role"], "adult");
    assert_eq!(body["is_child"], false);
    assert_eq!(body["disabled"], false);
}

#[sqlx::test(migrations = "./migrations")]
async fn create_user_creates_child(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (server, csrf, _) = credential_link_admin(&app_pool, &ingestion_pool).await;

    let r = server
        .post("/api/v1/users")
        .add_header(csrf_header(&csrf).0, csrf_header(&csrf).1)
        .json(&json!({
            "email": "new-child@example.com",
            "display_name": "New Child",
            "role": "child",
        }))
        .await;
    assert_eq!(r.status_code(), StatusCode::CREATED);
    let body = r.json::<serde_json::Value>()["user"].clone();
    assert_eq!(body["role"], "child");
    assert_eq!(body["is_child"], true);
}

#[sqlx::test(migrations = "./migrations")]
async fn create_user_invalid_email_returns_422(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (server, csrf, _) = credential_link_admin(&app_pool, &ingestion_pool).await;

    for email in [
        "invalid",
        "\"john doe\"@example.com",
        "\"john\"@example.com",
        "jane@homelab",
        "jane!books@example.com",
        "jane#books@example.com",
        "jane%books@example.com",
        "jane/books@example.com",
        "josé@example.com",
    ] {
        let r = server
            .post("/api/v1/users")
            .add_header(csrf_header(&csrf).0, csrf_header(&csrf).1)
            .json(&json!({
                "email": email,
                "display_name": "Weak",
                "role": "adult",
            }))
            .await;
        test_support::assert_problem(&r, problems::VALIDATION, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(
            crate::models::user::find_by_email(&app_pool, email)
                .await
                .unwrap()
                .is_none()
        );
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn create_user_non_admin_returns_403(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_adult_id, adult_basic) =
        test_support::db::create_adult_and_basic_auth(&app_pool, "create-forbidden").await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

    let r = server
        .post("/api/v1/users")
        .add_header(auth(&adult_basic).0, auth(&adult_basic).1)
        .json(&json!({
            "email": "x@example.com",
            "display_name": "X",
            "role": "adult",
        }))
        .await;
    test_support::assert_problem(&r, problems::FORBIDDEN, StatusCode::FORBIDDEN);
}

#[sqlx::test(migrations = "./migrations")]
async fn create_user_child_caller_returns_403(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_child_id, child_basic) =
        test_support::db::create_child_user_and_basic_auth(&app_pool, "create-child-caller").await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

    let r = server
        .post("/api/v1/users")
        .add_header(auth(&child_basic).0, auth(&child_basic).1)
        .json(&json!({
            "email": "x@example.com",
            "display_name": "X",
            "role": "adult",
        }))
        .await;
    test_support::assert_problem(&r, problems::FORBIDDEN, StatusCode::FORBIDDEN);
}

#[sqlx::test(migrations = "./migrations")]
async fn create_user_duplicate_email_returns_409(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (server, csrf, _) = credential_link_admin(&app_pool, &ingestion_pool).await;

    let make = |email: &str| {
        server
            .post("/api/v1/users")
            .add_header(csrf_header(&csrf).0, csrf_header(&csrf).1)
            .json(&json!({
                "email": email,
                "display_name": "Dup",
                "role": "adult",
            }))
    };
    assert_eq!(
        make("dup@example.com").await.status_code(),
        StatusCode::CREATED
    );
    // Case-insensitive collision.
    let r = make("DUP@example.com").await;
    test_support::assert_problem(&r, problems::EMAIL_CONFLICT, StatusCode::CONFLICT);
}

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

#[sqlx::test(migrations = "./migrations")]
async fn repeat_disable_is_idempotent(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin_id, admin_basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let (target_id, _) =
        test_support::db::create_adult_and_basic_auth(&app_pool, "idem-target").await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

    let first = server
        .put(&format!("/api/v1/users/{target_id}/account-status"))
        .add_header(auth(&admin_basic).0, auth(&admin_basic).1)
        .json(&json!({"disabled": true}))
        .await;
    assert_eq!(first.status_code(), StatusCode::OK);

    let sv_after_first: i32 = sqlx::query_scalar!(
        r#"SELECT session_version AS "sv!" FROM users WHERE id = $1"#,
        target_id
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    // Disabling an already-disabled account stays 200 but must not bump
    // session_version again; a retry should not evict freshly-created sessions.
    let second = server
        .put(&format!("/api/v1/users/{target_id}/account-status"))
        .add_header(auth(&admin_basic).0, auth(&admin_basic).1)
        .json(&json!({"disabled": true}))
        .await;
    assert_eq!(second.status_code(), StatusCode::OK);
    assert_eq!(second.json::<serde_json::Value>()["disabled"], true);

    let sv_after_second: i32 = sqlx::query_scalar!(
        r#"SELECT session_version AS "sv!" FROM users WHERE id = $1"#,
        target_id
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        sv_after_first, sv_after_second,
        "a repeat disable must not bump session_version"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn cannot_disable_own_account(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (admin_id, admin_basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

    let r = server
        .put(&format!("/api/v1/users/{admin_id}/account-status"))
        .add_header(auth(&admin_basic).0, auth(&admin_basic).1)
        .json(&json!({"disabled": true}))
        .await;
    test_support::assert_problem(&r, problems::VALIDATION, StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test(migrations = "./migrations")]
async fn account_status_non_admin_returns_403(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_adult_id, adult_basic) =
        test_support::db::create_adult_and_basic_auth(&app_pool, "status-forbidden").await;
    let (target_id, _) =
        test_support::db::create_adult_and_basic_auth(&app_pool, "status-victim").await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

    let r = server
        .put(&format!("/api/v1/users/{target_id}/account-status"))
        .add_header(auth(&adult_basic).0, auth(&adult_basic).1)
        .json(&json!({"disabled": true}))
        .await;
    test_support::assert_problem(&r, problems::FORBIDDEN, StatusCode::FORBIDDEN);
}

#[sqlx::test(migrations = "./migrations")]
async fn concurrent_cross_disable_keeps_one_enabled_admin(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (admin_a, a_basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let (admin_b, b_basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let server_a = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let server_b = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

    // A disables B and B disables A at the same time. Exactly one must win, or
    // the instance is bricked with zero admins. The loser is rejected one of two
    // ways depending on scheduling, and both are correct:
    //   - 422: both requests authenticate while still enabled, then serialize on
    //     the `ORDER BY id FOR UPDATE` lock; the loser re-reads the now-smaller
    //     enabled-admin set and the last-enabled-admin guard rejects it.
    //   - 401: the winner commits first, disabling the loser's acting admin; the
    //     loser's `CurrentUser` extractor then rejects at the auth boundary
    //     before the handler (and its guard) ever runs.
    // A 401 can only arise after a successful disable committed, so it still
    // implies exactly one winner. The deterministic backstop is the
    // enabled-admin recount below.
    let fut_a = server_a
        .put(&format!("/api/v1/users/{admin_b}/account-status"))
        .add_header(auth(&a_basic).0, auth(&a_basic).1)
        .json(&json!({"disabled": true}));
    let fut_b = server_b
        .put(&format!("/api/v1/users/{admin_a}/account-status"))
        .add_header(auth(&b_basic).0, auth(&b_basic).1)
        .json(&json!({"disabled": true}));
    let (ra, rb) = tokio::join!(fut_a, fut_b);

    let codes = [ra.status_code(), rb.status_code()];
    let ok = codes.iter().filter(|&&c| c == StatusCode::OK).count();
    let rejected = codes
        .iter()
        .filter(|&&c| c == StatusCode::UNPROCESSABLE_ENTITY || c == StatusCode::UNAUTHORIZED)
        .count();
    assert_eq!(ok, 1, "exactly one cross-disable succeeds, got {codes:?}");
    assert_eq!(
        rejected, 1,
        "the other is rejected, either by the last-enabled-admin guard (422) or, when \
         the acting admin was concurrently disabled by the winner, at the auth boundary \
         (401); got {codes:?}"
    );

    let enabled_admins: i64 = sqlx::query_scalar!(
        r#"SELECT COUNT(*) AS "c!" FROM users WHERE role = 'admin' AND disabled_at IS NULL"#
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        enabled_admins, 1,
        "at least one enabled admin must always remain"
    );
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

#[sqlx::test(migrations = "./migrations")]
async fn change_own_password_accepts_only_one_concurrent_change(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (id, basic) = test_support::db::create_adult_and_basic_auth(&app_pool, "change-pair").await;
    let old_hash = crate::auth::password::hash_password(OLD_PW.as_bytes()).unwrap();
    crate::models::local_credentials::set_password(&app_pool, id, &old_hash)
        .await
        .unwrap();
    let mut blocker = app_pool.begin().await.unwrap();
    sqlx::query_scalar!("SELECT id FROM users WHERE id = $1 FOR UPDATE", id)
        .fetch_one(&mut *blocker)
        .await
        .unwrap();
    let requests: Vec<_> = (0..2)
        .map(|_| {
            let basic = basic.clone();
            let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
            tokio::spawn(async move {
                server
                    .post("/api/v1/account/password")
                    .add_header(auth(&basic).0, auth(&basic).1)
                    .json(&json!({"current_password": OLD_PW, "new_password": STRONG_PW}))
                    .await
            })
        })
        .collect();
    test_support::wait_for_blocked_query(
        &pool,
        "SELECT id FROM users WHERE id = $1 FOR NO KEY UPDATE%",
        2,
    )
    .await;
    blocker.commit().await.unwrap();
    let mut statuses = Vec::new();
    for request in requests {
        statuses.push(request.await.unwrap().status_code());
    }
    statuses.sort();
    assert_eq!(statuses, [StatusCode::OK, StatusCode::UNPROCESSABLE_ENTITY]);
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
async fn change_own_password_weak_new_returns_422(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    test_support::db::create_adult_with_password(
        &app_pool,
        "change-weak",
        "change-weak@example.com",
        OLD_PW,
    )
    .await;
    let mut server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    server.save_cookies();
    server
        .post("/auth/local/login")
        .json(&json!({"email": "change-weak@example.com", "password": OLD_PW}))
        .await;
    let me: serde_json::Value = server.get("/auth/me").await.json();
    let token = me["csrf_token"].as_str().unwrap().to_owned();

    let r = server
        .post("/api/v1/account/password")
        .add_header(csrf_header(&token).0, csrf_header(&token).1)
        .json(&json!({"current_password": OLD_PW, "new_password": "password"}))
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

#[sqlx::test(migrations = "./migrations")]
async fn auth_hash_census_users_endpoints(pool: PgPool) {
    let mut state = test_support::test_state();
    state.pool = test_support::db::app_pool_for(&pool).await;
    state.ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (admin, basic) = test_support::db::create_admin_and_basic_auth(&state.pool).await;
    let phc = crate::auth::password::hash_password(STRONG_PW.as_bytes()).unwrap();
    crate::models::local_credentials::set_password(&state.pool, admin, &phc)
        .await
        .unwrap();
    let count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let hook_count = count.clone();
    state.hash_limiter = crate::auth::password::HashLimiter::new(1).with_before_compute(
        std::sync::Arc::new(move || {
            hook_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }),
    );
    let server = axum_test::TestServer::new(crate::build_router(state.clone()));
    let changed = server
        .post("/api/v1/account/password")
        .add_header(auth(&basic).0, auth(&basic).1)
        .json(&json!({"current_password":STRONG_PW,"new_password":"lithe cobalt meadow drift"}))
        .await;
    assert_eq!(changed.status_code(), StatusCode::OK);
    assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 2);
    let id = admin;
    let credential = crate::models::local_credentials::find_by_user_id(&state.pool, id)
        .await
        .unwrap()
        .unwrap();
    crate::auth::password::verify_password(b"lithe cobalt meadow drift", &credential.password_hash)
        .unwrap();
}

#[sqlx::test(migrations = "./migrations")]
async fn auth_hash_busy_users_endpoints(pool: PgPool) {
    let mut state = test_support::test_state();
    state.pool = test_support::db::app_pool_for(&pool).await;
    state.ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (admin, basic) = test_support::db::create_admin_and_basic_auth(&state.pool).await;
    let phc = crate::auth::password::hash_password(STRONG_PW.as_bytes()).unwrap();
    crate::models::local_credentials::set_password(&state.pool, admin, &phc)
        .await
        .unwrap();
    let count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let hook_count = count.clone();
    state.hash_limiter = crate::auth::password::HashLimiter::new(1)
        .with_wait_limit(std::time::Duration::from_millis(10))
        .with_before_compute(std::sync::Arc::new(move || {
            hook_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }));
    let _held = state.hash_limiter.acquire().await.unwrap();
    let server = axum_test::TestServer::new(crate::build_router(state.clone()));
    let (path, body) = (
        "/api/v1/account/password".to_owned(),
        json!({"current_password":STRONG_PW,"new_password":"lithe cobalt meadow drift"}),
    );
    let response = server
        .post(&path)
        .add_header(auth(&basic).0, auth(&basic).1)
        .json(&body)
        .await;
    assert_eq!(response.status_code(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        response.headers()["content-type"],
        "application/problem+json"
    );
    assert!(!response.headers().contains_key("retry-after"));
    assert_eq!(
        response.json::<serde_json::Value>(),
        json!({"type":"https://reverie.example/probs/auth-busy","title":"Service Unavailable","status":503,"detail":"The server is busy. Try again in a moment.","instance":path})
    );
    assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 0);
    assert!(
        crate::models::user::find_by_email(&state.pool, "busy@example.com")
            .await
            .unwrap()
            .is_none()
    );
    let (id, expected) = (admin, phc);
    assert_eq!(
        crate::models::local_credentials::find_by_user_id(&state.pool, id)
            .await
            .unwrap()
            .unwrap()
            .password_hash,
        expected
    );
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

    async fn status(env: &Env, id: Uuid, job: &str) -> serde_json::Value {
        let response = env
            .server
            .get(&format!("/api/v1/users/{id}/credential-links/emails/{job}"))
            .await;
        assert_eq!(response.status_code(), StatusCode::OK);
        response.json()
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

    async fn live_job(pool: &PgPool, user: Uuid) -> Uuid {
        sqlx::query_scalar!(
            "SELECT id FROM mail_jobs WHERE user_id = $1 AND state IN ('pending', 'in_progress')",
            user
        )
        .fetch_one(pool)
        .await
        .unwrap()
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
    async fn smtp_admin_delivery_reports_disabled_local_auth_and_unavailable_mail(pool: PgPool) {
        let e = env(&pool).await;
        let (_, csrf) = admin(&e, &pool).await;
        let target = invite(&e, &csrf, "target@example.test").await;
        let send_path = format!("/api/v1/users/{}/credential-links/emails", target.id);

        let mut config = test_support::test_config();
        config.local_auth_enabled = false;
        let (_, basic) = test_support::db::create_admin_and_basic_auth(&e.app).await;
        let off = test_support::db::server_with_mail(
            &e.app,
            &e.ingestion,
            config,
            Arc::clone(&e.runtime),
        );
        for response in [
            off.post(&send_path)
                .add_header(auth(&basic).0, auth(&basic).1)
                .json(&json!({"token": target.token}))
                .await,
            off.get(&format!("{send_path}/{}", Uuid::new_v4()))
                .add_header(auth(&basic).0, auth(&basic).1)
                .await,
        ] {
            assert_eq!(response.status_code(), StatusCode::NOT_FOUND);
        }

        let no_mail = test_support::db::server_with_real_pools(&e.app, &e.ingestion);
        let mut no_mail = no_mail;
        no_mail.save_cookies();
        let csrf = mail::login(&no_mail, "mail-admin@example.test", PASSWORD).await;
        let response = post(&no_mail, &csrf, &send_path, json!({"token": target.token})).await;
        assert_eq!(response.status_code(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(
            response.json::<serde_json::Value>(),
            json!({
                "type": "https://reverie.example/probs/mail-unavailable",
                "title": "Service Unavailable",
                "status": 503,
                "detail": "Email is not available on this server.",
                "instance": send_path,
            })
        );
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
    async fn smtp_admin_delivery_status_follows_the_worker(pool: PgPool) {
        let relay = FakeRelay::start(Behaviour::Accept).await;
        let e = env_on(&pool, relay.port).await;
        let (_, csrf) = admin(&e, &pool).await;
        let invitee = invite(&e, &csrf, "invitee@example.test").await;

        let job = send(&e, &csrf, invitee.id, &invitee.token)
            .await
            .json::<serde_json::Value>()["job_id"]
            .as_str()
            .unwrap()
            .to_owned();
        let queued = status(&e, invitee.id, &job).await;
        assert_eq!(
            queued,
            json!({"job_id": job, "status": "queued", "failure_category": null})
        );
        assert!(deliver_next(&e).await);
        assert_eq!(
            status(&e, invitee.id, &job).await,
            json!({"job_id": job, "status": "accepted", "failure_category": null})
        );
        assert_eq!(relay.message_count(), 1);
        assert!(
            mail::messages(&pool, invitee.id).await.is_empty(),
            "the bearer payload is cleared once accepted"
        );

        relay.set(Behaviour::Permanent);
        let retry = send(&e, &csrf, invitee.id, &invitee.token)
            .await
            .json::<serde_json::Value>();
        let second = retry["job_id"].as_str().unwrap().to_owned();
        assert_ne!(second, job, "an explicit retry is a new delivery");
        assert!(deliver_next(&e).await);
        assert_eq!(
            status(&e, invitee.id, &second).await,
            json!({"job_id": second, "status": "failed", "failure_category": "permanent_refusal"})
        );

        relay.set(Behaviour::Accept);
        let third = send(&e, &csrf, invitee.id, &invitee.token)
            .await
            .json::<serde_json::Value>()["job_id"]
            .as_str()
            .unwrap()
            .to_owned();
        let removed = e
            .server
            .delete(&format!("/api/v1/users/{}/credential-links", invitee.id))
            .add_header(mail::csrf(&csrf).0, mail::csrf(&csrf).1)
            .await;
        assert_eq!(removed.status_code(), StatusCode::NO_CONTENT);
        for previous in [&job, &second] {
            assert_eq!(
                status(&e, invitee.id, previous).await,
                json!({"job_id": previous, "status": "inactive", "failure_category": null})
            );
        }
        assert_eq!(
            status(&e, invitee.id, &third).await,
            json!({"job_id": third, "status": "inactive", "failure_category": null})
        );

        let other = invite(&e, &csrf, "someone@example.test").await;
        let foreign = e
            .server
            .get(&format!(
                "/api/v1/users/{}/credential-links/emails/{job}",
                other.id
            ))
            .await;
        assert_eq!(foreign.status_code(), StatusCode::NOT_FOUND);
        let unknown = e
            .server
            .get(&format!(
                "/api/v1/users/{}/credential-links/emails/{}",
                invitee.id,
                Uuid::new_v4()
            ))
            .await;
        assert_eq!(unknown.status_code(), StatusCode::NOT_FOUND);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn smtp_admin_delivery_accepted_status_tracks_the_live_link(pool: PgPool) {
        let relay = FakeRelay::start(Behaviour::Accept).await;
        let e = env_on(&pool, relay.port).await;
        let (_, csrf) = admin(&e, &pool).await;
        for action in ["replace", "redeem", "expire"] {
            let invitee = invite(&e, &csrf, &format!("{action}@example.test")).await;
            let job = send(&e, &csrf, invitee.id, &invitee.token)
                .await
                .json::<serde_json::Value>()["job_id"]
                .as_str()
                .unwrap()
                .to_owned();
            assert!(deliver_next(&e).await);
            assert_eq!(status(&e, invitee.id, &job).await["status"], "accepted");
            match action {
                "replace" => {
                    reissue(&e, &csrf, invitee.id).await;
                }
                "redeem" => {
                    let response = another(&e)
                        .post("/auth/credential-links/redemptions")
                        .json(&json!({"token": invitee.token, "password": PASSWORD}))
                        .await;
                    assert_eq!(response.status_code(), StatusCode::NO_CONTENT);
                }
                "expire" => {
                    sqlx::query!("UPDATE credential_links SET expires_at = now() - interval '1 second' WHERE user_id = $1", invitee.id)
                        .execute(&pool).await.unwrap();
                }
                _ => unreachable!(),
            }
            assert_eq!(
                status(&e, invitee.id, &job).await,
                json!({"job_id": job, "status": "inactive", "failure_category": null}),
                "{action}"
            );
        }
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

    #[sqlx::test(migrations = "./migrations")]
    async fn smtp_admin_delivery_coalesces_a_pending_send(pool: PgPool) {
        let e = env(&pool).await;
        let (_, csrf) = admin(&e, &pool).await;
        let invitee = invite(&e, &csrf, "invitee@example.test").await;
        let first = send(&e, &csrf, invitee.id, &invitee.token)
            .await
            .json::<serde_json::Value>();
        let second = send(&e, &csrf, invitee.id, &invitee.token)
            .await
            .json::<serde_json::Value>();
        assert_eq!(first["job_id"], second["job_id"]);
        assert_eq!(mail::job_count(&pool, invitee.id, None).await, 1);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn smtp_admin_delivery_link_changes_withdraw_pending_mail(pool: PgPool) {
        let e = env(&pool).await;
        let (_, csrf) = admin(&e, &pool).await;
        let target = account(&e, "Target", "target@example.test").await;

        let (token, _) = reissue(&e, &csrf, target).await;
        send(&e, &csrf, target, &token).await;
        let job = live_job(&pool, target).await;
        let (replacement, _) = reissue(&e, &csrf, target).await;
        assert_eq!(mail::state_of(&pool, job).await, "cancelled");
        assert!(!mail::payload_present(&pool, job).await);

        send(&e, &csrf, target, &replacement).await;
        let job = live_job(&pool, target).await;
        let removed = e
            .server
            .delete(&format!("/api/v1/users/{target}/credential-links"))
            .add_header(mail::csrf(&csrf).0, mail::csrf(&csrf).1)
            .await;
        assert_eq!(removed.status_code(), StatusCode::NO_CONTENT);
        assert_eq!(mail::state_of(&pool, job).await, "cancelled");
        assert!(!mail::payload_present(&pool, job).await);

        let (token, _) = reissue(&e, &csrf, target).await;
        send(&e, &csrf, target, &token).await;
        let job = live_job(&pool, target).await;
        let redeemed = another(&e)
            .post("/auth/credential-links/redemptions")
            .json(&json!({"token": token, "password": "amber thistle vault ridge"}))
            .await;
        assert_eq!(redeemed.status_code(), StatusCode::NO_CONTENT);
        assert_eq!(mail::state_of(&pool, job).await, "cancelled");
        assert!(!mail::payload_present(&pool, job).await);

        let (token, _) = reissue(&e, &csrf, target).await;
        send(&e, &csrf, target, &token).await;
        let job = live_job(&pool, target).await;
        let moved = patch(
            &e.server,
            &csrf,
            target,
            json!({"email": "moved@example.test"}),
        )
        .await;
        assert_eq!(moved.status_code(), StatusCode::OK);
        assert_eq!(mail::state_of(&pool, job).await, "cancelled");
        assert!(!mail::payload_present(&pool, job).await);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn smtp_admin_delivery_rolls_back_a_failed_enqueue(pool: PgPool) {
        let e = env(&pool).await;
        let (_, csrf) = admin(&e, &pool).await;
        let invitee = invite(&e, &csrf, "invitee@example.test").await;
        mail::break_enqueue(&pool).await;
        let response = send(&e, &csrf, invitee.id, &invitee.token).await;
        assert_eq!(response.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(mail::job_count(&pool, invitee.id, None).await, 0);
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
    async fn smtp_verification_status_reports_address_and_mail(pool: PgPool) {
        let e = env(&pool).await;
        let id = account(&e, "Holder", "holder@example.test").await;
        mail::login(&e.server, "holder@example.test", PASSWORD).await;
        let base = "/api/v1/account/email-verifications";
        let response = e.server.get(base).await;
        assert_eq!(response.headers()["cache-control"], "no-store");
        assert_eq!(
            response.json::<serde_json::Value>(),
            json!({"email": "holder@example.test", "email_verified": false, "mail_enabled": true})
        );
        mail::verify_address(&pool, id).await;
        assert_eq!(
            e.server.get(base).await.json::<serde_json::Value>()["email_verified"],
            true
        );

        let plain = {
            let mut server = test_support::db::server_with_real_pools(&e.app, &e.ingestion);
            server.save_cookies();
            server
        };
        mail::login(&plain, "holder@example.test", PASSWORD).await;
        sqlx::query!(
            "UPDATE users SET email = NULL, email_verified_at = NULL WHERE id = $1",
            id
        )
        .execute(&pool)
        .await
        .unwrap();
        assert_eq!(
            plain.get(base).await.json::<serde_json::Value>(),
            json!({"email": null, "email_verified": false, "mail_enabled": false})
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn smtp_verification_request_queues_a_challenge_and_reuses_it(pool: PgPool) {
        let e = env(&pool).await;
        let id = account(&e, "Holder", "holder@example.test").await;
        let csrf = mail::login(&e.server, "holder@example.test", PASSWORD).await;
        let base = "/api/v1/account/email-verifications";

        let first = post_empty(&e.server, &csrf, base).await;
        assert_eq!(first.status_code(), StatusCode::ACCEPTED);
        assert_eq!(first.headers()["cache-control"], "no-store");
        let stored = mail::messages(&pool, id).await;
        let [message] = stored.as_slice() else {
            panic!("expected one message: {stored:?}");
        };
        assert_eq!(message.purpose, "verification");
        assert_eq!(message.recipient, "holder@example.test");
        let token = mail::token_in(&message.body);
        let expires_at = sqlx::query_scalar!(
            "SELECT expires_at FROM account_email_proofs WHERE user_id = $1 AND purpose = 'verification'",
            id
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let expected = render::verification(&context(), "holder@example.test", &token, expires_at);
        assert_eq!(message.subject, expected.subject);
        assert_eq!(message.body, expected.body);
        assert!(
            message
                .body
                .contains(&format!("{ORIGIN}/verify-email#{token}"))
        );
        let lifetime = expires_at - Utc::now();
        assert!(
            lifetime > chrono::TimeDelta::hours(23) && lifetime <= chrono::TimeDelta::hours(24)
        );

        let again = post_empty(&e.server, &csrf, base).await;
        assert_eq!(again.status_code(), StatusCode::ACCEPTED);
        assert_eq!(
            mail::job_count(&pool, id, None).await,
            1,
            "a pending delivery absorbs the request"
        );

        mail::settle_delivery(&pool, message.id).await;
        post_empty(&e.server, &csrf, base).await;
        let tokens: Vec<_> = mail::messages(&pool, id)
            .await
            .iter()
            .map(|m| mail::token_in(&m.body))
            .collect();
        assert_eq!(
            tokens,
            [token.clone(), token],
            "a repeat after the interval resends the live challenge"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn smtp_verification_request_refuses_missing_address_verified_address_and_no_mail(
        pool: PgPool,
    ) {
        let e = env(&pool).await;
        let id = account(&e, "Holder", "holder@example.test").await;
        let csrf = mail::login(&e.server, "holder@example.test", PASSWORD).await;
        let base = "/api/v1/account/email-verifications";

        mail::verify_address(&pool, id).await;
        let verified = post_empty(&e.server, &csrf, base).await;
        assert_eq!(verified.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
        sqlx::query!(
            "UPDATE users SET email = NULL, email_verified_at = NULL WHERE id = $1",
            id
        )
        .execute(&pool)
        .await
        .unwrap();
        let none = post_empty(&e.server, &csrf, base).await;
        assert_eq!(none.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(mail::job_count(&pool, id, None).await, 0);

        let mut plain = test_support::db::server_with_real_pools(&e.app, &e.ingestion);
        plain.save_cookies();
        sqlx::query!(
            "UPDATE users SET email = 'holder@example.test' WHERE id = $1",
            id
        )
        .execute(&pool)
        .await
        .unwrap();
        let csrf = mail::login(&plain, "holder@example.test", PASSWORD).await;
        let unavailable = post_empty(&plain, &csrf, base).await;
        assert_eq!(unavailable.status_code(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(mail::job_count(&pool, id, None).await, 0);
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
    async fn smtp_verification_confirmation_verifies_the_current_address(pool: PgPool) {
        let e = env(&pool).await;
        let id = account(&e, "Holder", "holder@example.test").await;
        let csrf = mail::login(&e.server, "holder@example.test", PASSWORD).await;
        let token = requested_token(&e.server, &pool, id, &csrf).await;
        let before = user::find_by_id(&e.app, id).await.unwrap().unwrap();

        let response = post(&e.server, &csrf, confirm_path(), json!({"token": token})).await;
        assert_eq!(response.status_code(), StatusCode::NO_CONTENT);
        assert!(
            sqlx::query_scalar!("SELECT email_verified_at FROM users WHERE id = $1", id)
                .fetch_one(&pool)
                .await
                .unwrap()
                .is_some()
        );
        assert!(!mail::proof_live(&pool, id, ProofPurpose::Verification.as_str()).await);
        let notices = notice_messages(&pool, id).await;
        let [notice] = notices.as_slice() else {
            panic!("expected one notice: {notices:?}");
        };
        assert_eq!(notice.purpose, "notice_address_verified");
        assert_notice(
            notice,
            Notice::AddressVerified,
            "holder@example.test",
            "holder@example.test",
        );
        let after = user::find_by_id(&e.app, id).await.unwrap().unwrap();
        assert_eq!(after.session_version, before.session_version);
        assert_eq!(
            e.server.get("/auth/me").await.status_code(),
            StatusCode::OK,
            "no sign-out and no new session needed"
        );
        assert_eq!(
            post(&e.server, &csrf, confirm_path(), json!({"token": token}))
                .await
                .status_code(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "a challenge works once"
        );
        assert_eq!(
            e.server
                .get("/api/v1/account/email-verifications")
                .await
                .json::<serde_json::Value>()["email_verified"],
            true
        );
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
    async fn smtp_verification_confirmation_serves_oidc_only_accounts_without_mail(pool: PgPool) {
        let e = env(&pool).await;
        let id = account(&e, "Holder", "holder@example.test").await;
        let csrf = mail::login(&e.server, "holder@example.test", PASSWORD).await;
        let token = requested_token(&e.server, &pool, id, &csrf).await;
        let mut bare = test_support::db::server_with_real_pools(&e.app, &e.ingestion);
        bare.save_cookies();
        let bare_csrf = mail::login(&bare, "holder@example.test", PASSWORD).await;
        sqlx::query!("DELETE FROM local_credentials WHERE user_id = $1", id)
            .execute(&pool)
            .await
            .unwrap();
        let response = post(&bare, &bare_csrf, confirm_path(), json!({"token": token})).await;
        assert_eq!(
            response.status_code(),
            StatusCode::NO_CONTENT,
            "a password-less account verifies its mailbox and no mail runtime is needed"
        );
        assert_eq!(
            mail::job_count(&pool, id, Some("notice_address_verified")).await,
            0
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn smtp_verification_confirmation_rolls_back_when_the_notice_cannot_be_queued(
        pool: PgPool,
    ) {
        let e = env(&pool).await;
        let id = account(&e, "Holder", "holder@example.test").await;
        let csrf = mail::login(&e.server, "holder@example.test", PASSWORD).await;
        let token = requested_token(&e.server, &pool, id, &csrf).await;
        mail::break_enqueue(&pool).await;
        let response = post(&e.server, &csrf, confirm_path(), json!({"token": token})).await;
        assert_eq!(response.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
        assert!(mail::proof_live(&pool, id, "verification").await);
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
    async fn smtp_notifications_need_a_verified_address_and_enabled_mail(pool: PgPool) {
        let e = env(&pool).await;
        let id = account(&e, "Holder", "holder@example.test").await;
        let session = another(&e);
        let csrf = mail::login(&session, "holder@example.test", PASSWORD).await;
        let change = |from: &str, to: &str| json!({"current_password": from, "new_password": to});
        let response = post(
            &session,
            &csrf,
            "/api/v1/account/password",
            change(PASSWORD, "amber thistle vault ridge"),
        )
        .await;
        assert_eq!(response.status_code(), StatusCode::OK);
        assert_eq!(
            mail::job_count(&pool, id, None).await,
            0,
            "an unverified address gets no notice"
        );

        mail::verify_address(&pool, id).await;
        let mut bare = test_support::db::server_with_real_pools(&e.app, &e.ingestion);
        bare.save_cookies();
        let csrf = mail::login(&bare, "holder@example.test", "amber thistle vault ridge").await;
        let response = post(
            &bare,
            &csrf,
            "/api/v1/account/password",
            change("amber thistle vault ridge", PASSWORD),
        )
        .await;
        assert_eq!(response.status_code(), StatusCode::OK);
        assert_eq!(
            mail::job_count(&pool, id, None).await,
            0,
            "mail disabled queues nothing"
        );
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
    async fn smtp_notifications_only_an_actual_change_clears_verification(pool: PgPool) {
        let e = env(&pool).await;
        let (id, csrf) = change_address_fixture(&e, &pool).await;
        let outbox = outbox(&e);
        let mut tx = e.app.begin().await.unwrap();
        lock_account(&mut tx, id).await.unwrap();
        outbox
            .request_proof(&mut tx, id, ProofPurpose::Recovery, "holder@example.test")
            .await
            .unwrap();
        tx.commit().await.unwrap();

        for body in [
            json!({"display_name": "Renamed"}),
            json!({"email": "holder@example.test"}),
            json!({"email": "HOLDER@example.test"}),
            json!({"email": " holder@example.test "}),
        ] {
            let response = patch(&e.server, &csrf, id, body.clone()).await;
            assert_eq!(response.status_code(), StatusCode::OK, "{body}");
            assert!(
                sqlx::query_scalar!("SELECT email_verified_at FROM users WHERE id = $1", id)
                    .fetch_one(&pool)
                    .await
                    .unwrap()
                    .is_some(),
                "{body}"
            );
            assert!(mail::proof_live(&pool, id, "recovery").await, "{body}");
        }
        assert!(notice_messages(&pool, id).await.is_empty());
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn smtp_notifications_address_edit_needs_a_verified_old_address_or_mail(pool: PgPool) {
        let e = env(&pool).await;
        let (_, csrf) = admin(&e, &pool).await;
        let unverified = account(&e, "Plain", "plain@example.test").await;
        let response = patch(
            &e.server,
            &csrf,
            unverified,
            json!({"email": "elsewhere@example.test"}),
        )
        .await;
        assert_eq!(response.status_code(), StatusCode::OK);
        assert_eq!(mail::job_count(&pool, unverified, None).await, 0);

        let disabled = verified(&e, &pool, "Off", "off@example.test").await;
        user::disable_account(&e.app, disabled).await.unwrap();
        let response = patch(
            &e.server,
            &csrf,
            disabled,
            json!({"email": "off-new@example.test"}),
        )
        .await;
        assert_eq!(response.status_code(), StatusCode::OK);
        assert_eq!(
            mail::job_count(&pool, disabled, Some("notice_address_changed")).await,
            1,
            "a disabled account still hears about a change to its verified address"
        );

        let bare_admin = {
            let mut server = test_support::db::server_with_real_pools(&e.app, &e.ingestion);
            server.save_cookies();
            server
        };
        let bare_csrf = mail::login(&bare_admin, "mail-admin@example.test", PASSWORD).await;
        let quiet = verified(&e, &pool, "Quiet", "quiet@example.test").await;
        let response = patch(
            &bare_admin,
            &bare_csrf,
            quiet,
            json!({"email": "quiet-new@example.test"}),
        )
        .await;
        assert_eq!(response.status_code(), StatusCode::OK);
        assert_eq!(mail::job_count(&pool, quiet, None).await, 0);
        assert!(
            sqlx::query_scalar!("SELECT email_verified_at FROM users WHERE id = $1", quiet)
                .fetch_one(&pool)
                .await
                .unwrap()
                .is_none(),
            "verification is still cleared when mail is off"
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
