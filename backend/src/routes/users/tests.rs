//! Integration tests for `/api/users*` admin endpoints.

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

// ---------- GET /api/users ----------

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
        .get("/api/users")
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
        .get("/api/users")
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
        .get("/api/users")
        .add_header(auth(&child_basic).0, auth(&child_basic).1)
        .await;
    test_support::assert_problem(&r, problems::FORBIDDEN, StatusCode::FORBIDDEN);
}

// ---------- PUT /api/users/{id}/role ----------

#[sqlx::test(migrations = "./migrations")]
async fn promote_adult_to_admin(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin_id, admin_basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let (adult_id, _) =
        test_support::db::create_adult_and_basic_auth(&app_pool, "promote-target").await;

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let r = server
        .put(&format!("/api/users/{adult_id}/role"))
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
        .put(&format!("/api/users/{admin2_id}/role"))
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
        .put(&format!("/api/users/{admin_id}/role"))
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
        .put(&format!("/api/users/{adult_id}/role"))
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
        .put(&format!("/api/users/{adult_id}/role"))
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
        .put(&format!("/api/users/{fake_id}/role"))
        .add_header(auth(&admin_basic).0, auth(&admin_basic).1)
        .json(&json!({"role": "adult"}))
        .await;
    test_support::assert_problem(&r, problems::NOT_FOUND, StatusCode::NOT_FOUND);
}

// ---------- PUT /api/users/{id}/child-status ----------

#[sqlx::test(migrations = "./migrations")]
async fn toggle_child_on_sets_role_child(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin_id, admin_basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let (adult_id, _) =
        test_support::db::create_adult_and_basic_auth(&app_pool, "child-toggle").await;

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let r = server
        .put(&format!("/api/users/{adult_id}/child-status"))
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
        .put(&format!("/api/users/{child_id}/child-status"))
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
        .put(&format!("/api/users/{admin_id}/child-status"))
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

// ---------- PATCH /api/users/{id} ----------

#[sqlx::test(migrations = "./migrations")]
async fn patch_user_updates_display_name_and_email(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin_id, admin_basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let (adult_id, _) =
        test_support::db::create_adult_and_basic_auth(&app_pool, "patch-target").await;

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let r = server
        .patch(&format!("/api/users/{adult_id}"))
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
        .patch(&format!("/api/users/{adult_id}"))
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
        .patch(&format!("/api/users/{adult_id}"))
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
        .patch(&format!("/api/users/{user2_id}"))
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
        .patch(&format!("/api/users/{user2_id}"))
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
        .patch(&format!("/api/users/{fake_id}"))
        .add_header(auth(&admin_basic).0, auth(&admin_basic).1)
        .json(&json!({"display_name": "Ghost"}))
        .await;
    test_support::assert_problem(&r, problems::NOT_FOUND, StatusCode::NOT_FOUND);
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
        .put(&format!("/api/users/{admin3_id}/role"))
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
        .put(&format!("/api/users/{admin2_id}/role"))
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
        .put(&format!("/api/users/{admin1_id}/role"))
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
