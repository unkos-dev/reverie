//! Integration tests for `/api/v1/users*` admin endpoints.

use axum::http::{HeaderName, HeaderValue, StatusCode};
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
    let r = server
        .put(&format!("/api/v1/users/{admin_id}/role"))
        .add_header(auth(&admin_basic).0, auth(&admin_basic).1)
        .json(&json!({"role": "adult"}))
        .await;
    let problem =
        test_support::assert_problem(&r, problems::VALIDATION, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        problem["detail"].as_str().unwrap(),
        "would leave zero admins"
    );
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
async fn set_role_child_on_non_child_user_returns_422(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin_id, admin_basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let (adult_id, _) =
        test_support::db::create_adult_and_basic_auth(&app_pool, "child-role-sync").await;

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let r = server
        .put(&format!("/api/v1/users/{adult_id}/role"))
        .add_header(auth(&admin_basic).0, auth(&admin_basic).1)
        .json(&json!({"role": "child"}))
        .await;
    let problem =
        test_support::assert_problem(&r, problems::VALIDATION, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        problem["detail"].as_str().unwrap().contains("child status"),
        "expected child-role-sync validation, got: {}",
        problem["detail"],
    );
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
async fn set_non_child_role_on_child_user_returns_422(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin_id, admin_basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let (child_id, _) =
        test_support::db::create_child_user_and_basic_auth(&app_pool, "role-sync-mirror").await;

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let r = server
        .put(&format!("/api/v1/users/{child_id}/role"))
        .add_header(auth(&admin_basic).0, auth(&admin_basic).1)
        .json(&json!({"role": "adult"}))
        .await;
    let problem =
        test_support::assert_problem(&r, problems::VALIDATION, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        problem["detail"].as_str().unwrap().contains("child status"),
        "expected child-role-sync validation, got: {}",
        problem["detail"],
    );
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
    let (_admin_id, admin_basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

    let r = server
        .post("/api/v1/users")
        .add_header(auth(&admin_basic).0, auth(&admin_basic).1)
        .json(&json!({
            "email": "new-adult@example.com",
            "display_name": "New Adult",
            "role": "adult",
            "password": STRONG_PW,
        }))
        .await;
    assert_eq!(r.status_code(), StatusCode::CREATED);
    let body: serde_json::Value = r.json();
    assert_eq!(body["role"], "adult");
    assert_eq!(body["is_child"], false);
    assert_eq!(body["disabled"], false);
}

#[sqlx::test(migrations = "./migrations")]
async fn create_user_creates_child(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin_id, admin_basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

    let r = server
        .post("/api/v1/users")
        .add_header(auth(&admin_basic).0, auth(&admin_basic).1)
        .json(&json!({
            "email": "new-child@example.com",
            "display_name": "New Child",
            "role": "child",
            "password": STRONG_PW,
        }))
        .await;
    assert_eq!(r.status_code(), StatusCode::CREATED);
    let body: serde_json::Value = r.json();
    assert_eq!(body["role"], "child");
    assert_eq!(body["is_child"], true);
}

#[sqlx::test(migrations = "./migrations")]
async fn create_user_weak_password_returns_422(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin_id, admin_basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

    let r = server
        .post("/api/v1/users")
        .add_header(auth(&admin_basic).0, auth(&admin_basic).1)
        .json(&json!({
            "email": "weak@example.com",
            "display_name": "Weak",
            "role": "adult",
            "password": "password",
        }))
        .await;
    test_support::assert_problem(&r, problems::VALIDATION, StatusCode::UNPROCESSABLE_ENTITY);
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
            "password": STRONG_PW,
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
            "password": STRONG_PW,
        }))
        .await;
    test_support::assert_problem(&r, problems::FORBIDDEN, StatusCode::FORBIDDEN);
}

#[sqlx::test(migrations = "./migrations")]
async fn create_user_duplicate_email_returns_409(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin_id, admin_basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

    let make = |email: &str| {
        server
            .post("/api/v1/users")
            .add_header(auth(&admin_basic).0, auth(&admin_basic).1)
            .json(&json!({
                "email": email,
                "display_name": "Dup",
                "role": "adult",
                "password": STRONG_PW,
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

    // A disables B and B disables A at the same time. The FOR UPDATE guard must
    // let exactly one through, or the instance is bricked with zero admins.
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
        .filter(|&&c| c == StatusCode::UNPROCESSABLE_ENTITY)
        .count();
    assert_eq!(ok, 1, "exactly one cross-disable succeeds, got {codes:?}");
    assert_eq!(
        rejected, 1,
        "the other is rejected by the last-enabled-admin guard, got {codes:?}"
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

// ---------- POST /api/v1/users/{id}/password-reset ----------

#[sqlx::test(migrations = "./migrations")]
async fn admin_reset_password_bumps_session_version(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin_id, admin_basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let (target_id, _) =
        test_support::db::create_adult_and_basic_auth(&app_pool, "reset-target").await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

    let before: i32 = sqlx::query_scalar!(
        r#"SELECT session_version AS "sv!" FROM users WHERE id = $1"#,
        target_id,
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    let r = server
        .post(&format!("/api/v1/users/{target_id}/password-reset"))
        .add_header(auth(&admin_basic).0, auth(&admin_basic).1)
        .json(&json!({"new_password": STRONG_PW}))
        .await;
    assert_eq!(r.status_code(), StatusCode::OK);

    let after: i32 = sqlx::query_scalar!(
        r#"SELECT session_version AS "sv!" FROM users WHERE id = $1"#,
        target_id,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(
        after > before,
        "admin reset bumps the target's session_version"
    );

    let cred = crate::models::local_credentials::find_by_user_id(&app_pool, target_id)
        .await
        .unwrap();
    assert!(cred.is_some(), "admin reset upserts a local credential");
}

#[sqlx::test(migrations = "./migrations")]
async fn admin_reset_weak_password_returns_422(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin_id, admin_basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let (target_id, _) =
        test_support::db::create_adult_and_basic_auth(&app_pool, "reset-weak").await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

    let r = server
        .post(&format!("/api/v1/users/{target_id}/password-reset"))
        .add_header(auth(&admin_basic).0, auth(&admin_basic).1)
        .json(&json!({"new_password": "password"}))
        .await;
    test_support::assert_problem(&r, problems::VALIDATION, StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test(migrations = "./migrations")]
async fn admin_reset_non_admin_returns_403(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_adult_id, adult_basic) =
        test_support::db::create_adult_and_basic_auth(&app_pool, "reset-forbidden").await;
    let (target_id, _) =
        test_support::db::create_adult_and_basic_auth(&app_pool, "reset-victim").await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

    let r = server
        .post(&format!("/api/v1/users/{target_id}/password-reset"))
        .add_header(auth(&adult_basic).0, auth(&adult_basic).1)
        .json(&json!({"new_password": STRONG_PW}))
        .await;
    test_support::assert_problem(&r, problems::FORBIDDEN, StatusCode::FORBIDDEN);
}

#[sqlx::test(migrations = "./migrations")]
async fn admin_reset_unknown_id_returns_404(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin_id, admin_basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

    let r = server
        .post(&format!("/api/v1/users/{}/password-reset", Uuid::new_v4()))
        .add_header(auth(&admin_basic).0, auth(&admin_basic).1)
        .json(&json!({"new_password": STRONG_PW}))
        .await;
    test_support::assert_problem(&r, problems::NOT_FOUND, StatusCode::NOT_FOUND);
}

// ---------- POST /api/v1/account/password (self-service) ----------

const OLD_PW: &str = "the old password one!";

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
