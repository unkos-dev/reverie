//! Integration tests for the `/api/books` list endpoint (11a Task 3).
//!
//! Mirrors [`crate::routes::opds::tests`] — `#[sqlx::test]` per case,
//! real-pool harness via [`crate::test_support::db::server_with_real_pools`].
#![allow(
    clippy::cast_possible_wrap,
    reason = "test-only casts on small fixture sizes"
)]

use axum::http::{StatusCode, header::AUTHORIZATION};
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::problems;
use crate::test_support;

/// Insert a `(work, manifestation)` pair via the ingestion pool and
/// return `(work_id, manifestation_id)`.
async fn insert_book(ingestion_pool: &PgPool, marker: &str, title: &str) -> (Uuid, Uuid) {
    let work_id: Uuid = sqlx::query_scalar!(
        "INSERT INTO works (title, sort_title) VALUES ($1, $1) RETURNING id",
        title,
    )
    .fetch_one(ingestion_pool)
    .await
    .expect("insert work");

    let file_path = format!("/tmp/library-test-{marker}.epub");
    let hash = format!("library-test-hash-{marker}");
    let m_id: Uuid = sqlx::query_scalar!(
        "INSERT INTO manifestations \
            (work_id, format, file_path, ingestion_file_hash, current_file_hash, \
             file_size_bytes, ingestion_status, validation_status) \
         VALUES ($1, 'epub'::manifestation_format, $2, $3, $3, 1000, \
                 'complete'::ingestion_status, 'valid'::validation_status) \
         RETURNING id",
        work_id,
        file_path,
        hash,
    )
    .fetch_one(ingestion_pool)
    .await
    .expect("insert manifestation");
    (work_id, m_id)
}

#[sqlx::test(migrations = "./migrations")]
async fn list_endpoint_admin_sees_all_books(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;

    for (m, t) in [("a", "Alpha"), ("b", "Beta"), ("c", "Gamma")] {
        insert_book(&ingestion_pool, m, t).await;
    }

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let response = server
        .get("/api/books")
        .add_header(AUTHORIZATION, basic)
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let body: serde_json::Value = response.json();
    let items = body["items"].as_array().expect("items array");
    assert_eq!(items.len(), 3, "admin sees all 3 manifestations");
    assert!(
        body["next_cursor"].is_null(),
        "3 rows under page_size=50 → no next cursor, got {body}"
    );
    // Cover URL synthesised by the handler.
    let first_id = items[0]["id"].as_str().expect("item id");
    assert_eq!(
        items[0]["cover_url"].as_str().unwrap(),
        format!("/api/books/{first_id}/cover/thumb"),
    );
    assert!(items[0]["validation_status"].is_string());
    assert!(items[0]["ingestion_status"].is_string());
    assert!(items[0]["enrichment_status"].is_string());
    // created_at must be elided per the wire-format invariant.
    assert!(
        items[0].get("created_at").is_none(),
        "created_at must be #[serde(skip)]'d, got {}",
        items[0]
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn list_endpoint_adult_sees_only_rls_visible(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_adult, basic) = test_support::db::create_adult_and_basic_auth(&app_pool, "adult").await;

    // Adults see all manifestations under the current RLS policy set —
    // child-shelf gating is the only role-based restriction. Confirm
    // the endpoint at least surfaces the rows the policy permits.
    for (m, t) in [("v1", "Vol 1"), ("v2", "Vol 2")] {
        insert_book(&ingestion_pool, m, t).await;
    }

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let response = server
        .get("/api/books")
        .add_header(AUTHORIZATION, basic)
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(body["items"].as_array().unwrap().len(), 2);
}

#[sqlx::test(migrations = "./migrations")]
async fn list_endpoint_child_sees_only_shelved(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (child_id, basic) =
        test_support::db::create_child_user_and_basic_auth(&app_pool, "kid").await;

    let (_w1, m1) = insert_book(&ingestion_pool, "kid-a", "Kid Friendly A").await;
    let (_w2, m2) = insert_book(&ingestion_pool, "kid-b", "Kid Friendly B").await;
    let (_w3, _m3) = insert_book(&ingestion_pool, "adult-only", "Adult Only Material").await;

    let shelf = test_support::db::create_shelf(&app_pool, child_id, "kid-shelf").await;
    test_support::db::add_to_shelf(&app_pool, shelf, m1).await;
    test_support::db::add_to_shelf(&app_pool, shelf, m2).await;

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let response = server
        .get("/api/books")
        .add_header(AUTHORIZATION, basic)
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let body: serde_json::Value = response.json();
    let items = body["items"].as_array().unwrap();
    assert_eq!(
        items.len(),
        2,
        "child should only see shelved manifestations"
    );
    let titles: Vec<&str> = items
        .iter()
        .map(|it| it["title"].as_str().unwrap())
        .collect();
    assert!(titles.contains(&"Kid Friendly A"));
    assert!(titles.contains(&"Kid Friendly B"));
    assert!(!titles.contains(&"Adult Only Material"));
}

#[sqlx::test(migrations = "./migrations")]
async fn list_endpoint_unauthenticated_returns_401(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

    let response = server.get("/api/books").await;
    test_support::assert_problem(&response, problems::UNAUTHORIZED, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "./migrations")]
async fn list_endpoint_malformed_cursor_returns_422(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

    let response = server
        .get("/api/books?cursor=!!!not-base64url!!!")
        .add_header(AUTHORIZATION, basic)
        .await;
    let body = test_support::assert_problem(
        &response,
        problems::VALIDATION,
        StatusCode::UNPROCESSABLE_ENTITY,
    );
    let detail = body["detail"].as_str().unwrap();
    assert!(detail.contains("invalid cursor"), "got detail: {detail}");
}

#[sqlx::test(migrations = "./migrations")]
async fn list_endpoint_sort_title_orders_alphabetically(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;

    // Distinct vocabulary keeps the pg_trgm find_or_create dedup
    // threshold from collapsing these into a single work.
    for (m, t) in [
        ("zebra", "Zebra Crossing"),
        ("apple", "Apple Orchards"),
        ("middle", "Middle Distance"),
    ] {
        insert_book(&ingestion_pool, m, t).await;
    }

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let response = server
        .get("/api/books?sort=title")
        .add_header(AUTHORIZATION, basic)
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let body: serde_json::Value = response.json();
    let titles: Vec<&str> = body["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|it| it["title"].as_str().unwrap())
        .collect();
    assert_eq!(
        titles,
        vec!["Apple Orchards", "Middle Distance", "Zebra Crossing"]
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn list_endpoint_multi_series_work_does_not_duplicate(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;

    let (work_id, _m_id) = insert_book(&ingestion_pool, "multi", "Multi-Series Vol 1").await;
    let s1: Uuid = sqlx::query_scalar!(
        "INSERT INTO series (name, sort_name) VALUES ('Alpha Saga', 'Alpha Saga') RETURNING id",
    )
    .fetch_one(&ingestion_pool)
    .await
    .unwrap();
    let s2: Uuid = sqlx::query_scalar!(
        "INSERT INTO series (name, sort_name) VALUES ('Bravo Saga', 'Bravo Saga') RETURNING id",
    )
    .fetch_one(&ingestion_pool)
    .await
    .unwrap();
    sqlx::query!(
        "INSERT INTO series_works (series_id, work_id, position) \
         VALUES ($1, $2, 1::float8), ($3, $2, 2::float8)",
        s1,
        work_id,
        s2,
    )
    .execute(&ingestion_pool)
    .await
    .unwrap();

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let response = server
        .get("/api/books")
        .add_header(AUTHORIZATION, basic)
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let body: serde_json::Value = response.json();
    let items = body["items"].as_array().unwrap();
    assert_eq!(
        items.len(),
        1,
        "work in 2 series must surface as a single row, got: {body}"
    );
    assert_eq!(items[0]["series"]["name"], "Alpha Saga");
}

#[sqlx::test(migrations = "./migrations")]
async fn list_endpoint_cross_sort_cursor_rejected(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

    // Mint a recent-tagged cursor, then try to replay it under sort=title.
    let recent = crate::routes::cursor::CursorKey::Recent {
        created_at: time::OffsetDateTime::now_utc(),
        id: Uuid::new_v4(),
    }
    .encode();

    let response = server
        .get(&format!("/api/books?sort=title&cursor={recent}"))
        .add_header(AUTHORIZATION, basic)
        .await;
    let body = test_support::assert_problem(
        &response,
        problems::VALIDATION,
        StatusCode::UNPROCESSABLE_ENTITY,
    );
    assert!(
        body["detail"]
            .as_str()
            .unwrap()
            .contains("cursor sort mismatch")
    );
}
