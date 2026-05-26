use axum::http::StatusCode;
use sqlx::PgPool;

use crate::test_support;

fn server(app_pool: &PgPool, ingestion_pool: &PgPool) -> axum_test::TestServer {
    test_support::db::server_with_real_pools(app_pool, ingestion_pool)
}

#[tokio::test]
async fn get_settings_unauthenticated_returns_401() {
    let server = test_support::test_server();
    let r = server.get("/api/settings").await;
    assert_eq!(r.status_code(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn put_settings_unauthenticated_returns_401() {
    let server = test_support::test_server();
    let r = server
        .put("/api/settings")
        .json(&serde_json::json!({"enrichment_concurrency": 5}))
        .await;
    assert_eq!(r.status_code(), StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "./migrations")]
async fn get_settings_as_admin_returns_200(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin_id, admin_basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let server = server(&app_pool, &ingestion_pool);

    let r = server
        .get("/api/settings")
        .add_header(axum::http::header::AUTHORIZATION, admin_basic)
        .await;
    assert_eq!(r.status_code(), StatusCode::OK);

    let body: serde_json::Value = r.json();
    assert!(body["enrichment_enabled"].is_boolean());
    assert!(body["enrichment_concurrency"].is_number());
    assert!(body["format_priority"].is_array());
    assert!(body["restart_required_fields"].is_array());
    assert!(!body["updated_at"].is_null(), "updated_at must be present");
}

#[sqlx::test(migrations = "./migrations")]
async fn get_settings_as_non_admin_returns_403(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_adult_id, adult_basic) =
        test_support::db::create_adult_and_basic_auth(&app_pool, "settings-adult").await;
    let server = server(&app_pool, &ingestion_pool);

    let r = server
        .get("/api/settings")
        .add_header(axum::http::header::AUTHORIZATION, adult_basic)
        .await;
    assert_eq!(r.status_code(), StatusCode::FORBIDDEN);

    let body: serde_json::Value = r.json();
    let prob_type = body["type"].as_str().unwrap_or_default();
    assert!(
        prob_type.ends_with("/forbidden"),
        "expected problem type ending in /forbidden, got {prob_type}"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn put_settings_updates_enrichment_concurrency(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin_id, admin_basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let server = server(&app_pool, &ingestion_pool);

    let r = server
        .put("/api/settings")
        .add_header(axum::http::header::AUTHORIZATION, admin_basic.clone())
        .json(&serde_json::json!({"enrichment_concurrency": 7}))
        .await;
    assert_eq!(r.status_code(), StatusCode::OK);

    let body: serde_json::Value = r.json();
    assert_eq!(body["enrichment_concurrency"], 7);
    assert_eq!(body["restart_required"], false);

    let r2 = server
        .get("/api/settings")
        .add_header(axum::http::header::AUTHORIZATION, admin_basic)
        .await;
    let body2: serde_json::Value = r2.json();
    assert_eq!(body2["enrichment_concurrency"], 7);
}

#[sqlx::test(migrations = "./migrations")]
async fn put_settings_as_non_admin_returns_403(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_adult_id, adult_basic) =
        test_support::db::create_adult_and_basic_auth(&app_pool, "settings-put-adult").await;
    let server = server(&app_pool, &ingestion_pool);

    let r = server
        .put("/api/settings")
        .add_header(axum::http::header::AUTHORIZATION, adult_basic)
        .json(&serde_json::json!({"enrichment_concurrency": 5}))
        .await;
    assert_eq!(r.status_code(), StatusCode::FORBIDDEN);
}

#[sqlx::test(migrations = "./migrations")]
async fn put_settings_invalid_concurrency_returns_422(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin_id, admin_basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let server = server(&app_pool, &ingestion_pool);

    let r = server
        .put("/api/settings")
        .add_header(axum::http::header::AUTHORIZATION, admin_basic)
        .json(&serde_json::json!({"enrichment_concurrency": -1}))
        .await;
    assert_eq!(r.status_code(), StatusCode::UNPROCESSABLE_ENTITY);

    let body: serde_json::Value = r.json();
    let prob_type = body["type"].as_str().unwrap_or_default();
    assert!(
        prob_type.ends_with("/validation"),
        "expected validation problem type, got {prob_type}"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn put_settings_empty_body_returns_422(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin_id, admin_basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let server = server(&app_pool, &ingestion_pool);

    let r = server
        .put("/api/settings")
        .add_header(axum::http::header::AUTHORIZATION, admin_basic)
        .json(&serde_json::json!({}))
        .await;
    assert_eq!(r.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test(migrations = "./migrations")]
async fn put_settings_invalid_format_priority_returns_422(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin_id, admin_basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let server = server(&app_pool, &ingestion_pool);

    let r = server
        .put("/api/settings")
        .add_header(axum::http::header::AUTHORIZATION, admin_basic)
        .json(&serde_json::json!({"format_priority": ["epub", "banana"]}))
        .await;
    assert_eq!(r.status_code(), StatusCode::UNPROCESSABLE_ENTITY);

    let body: serde_json::Value = r.json();
    let detail = body["detail"].as_str().unwrap_or_default();
    assert!(
        detail.contains("banana"),
        "expected detail mentioning 'banana', got {detail}"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn put_settings_valid_format_priority_persists(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin_id, admin_basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let server = server(&app_pool, &ingestion_pool);

    let r = server
        .put("/api/settings")
        .add_header(axum::http::header::AUTHORIZATION, admin_basic.clone())
        .json(&serde_json::json!({"format_priority": ["pdf", "epub"]}))
        .await;
    assert_eq!(r.status_code(), StatusCode::OK);

    let r2 = server
        .get("/api/settings")
        .add_header(axum::http::header::AUTHORIZATION, admin_basic)
        .await;
    let body: serde_json::Value = r2.json();
    let fp = body["format_priority"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    assert_eq!(fp, vec!["pdf", "epub"]);
}

#[sqlx::test(migrations = "./migrations")]
async fn put_settings_invalid_cleanup_mode_returns_422(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin_id, admin_basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let server = server(&app_pool, &ingestion_pool);

    let r = server
        .put("/api/settings")
        .add_header(axum::http::header::AUTHORIZATION, admin_basic)
        .json(&serde_json::json!({"cleanup_mode": "yeet"}))
        .await;
    assert_eq!(r.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test(migrations = "./migrations")]
async fn put_settings_multiple_fields_at_once(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin_id, admin_basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let server = server(&app_pool, &ingestion_pool);

    let r = server
        .put("/api/settings")
        .add_header(axum::http::header::AUTHORIZATION, admin_basic.clone())
        .json(&serde_json::json!({
            "enrichment_concurrency": 5,
            "writeback_enabled": false,
            "opds_page_size": 100
        }))
        .await;
    assert_eq!(r.status_code(), StatusCode::OK);

    let body: serde_json::Value = r.json();
    assert_eq!(body["enrichment_concurrency"], 5);
    assert_eq!(body["writeback_enabled"], false);
    assert_eq!(body["opds_page_size"], 100);
}

#[sqlx::test(migrations = "./migrations")]
async fn put_settings_duplicate_format_priority_returns_422(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin_id, admin_basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let server = server(&app_pool, &ingestion_pool);

    let r = server
        .put("/api/settings")
        .add_header(axum::http::header::AUTHORIZATION, admin_basic)
        .json(&serde_json::json!({"format_priority": ["epub", "epub", "pdf"]}))
        .await;
    assert_eq!(r.status_code(), StatusCode::UNPROCESSABLE_ENTITY);

    let body: serde_json::Value = r.json();
    let detail = body["detail"].as_str().unwrap_or_default();
    assert!(
        detail.contains("duplicate"),
        "expected 'duplicate' in detail, got {detail}"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn put_settings_empty_format_priority_returns_422(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin_id, admin_basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let server = server(&app_pool, &ingestion_pool);

    let r = server
        .put("/api/settings")
        .add_header(axum::http::header::AUTHORIZATION, admin_basic)
        .json(&serde_json::json!({"format_priority": []}))
        .await;
    assert_eq!(r.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test(migrations = "./migrations")]
async fn put_settings_invalid_url_scheme_returns_422(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin_id, admin_basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let server = server(&app_pool, &ingestion_pool);

    let r = server
        .put("/api/settings")
        .add_header(axum::http::header::AUTHORIZATION, admin_basic)
        .json(&serde_json::json!({"openlibrary_base_url": "file:///etc/passwd"}))
        .await;
    assert_eq!(r.status_code(), StatusCode::UNPROCESSABLE_ENTITY);

    let body: serde_json::Value = r.json();
    let detail = body["detail"].as_str().unwrap_or_default();
    assert!(
        detail.contains("http"),
        "expected scheme error, got {detail}"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn put_settings_valid_url_persists(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin_id, admin_basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let server = server(&app_pool, &ingestion_pool);

    let r = server
        .put("/api/settings")
        .add_header(axum::http::header::AUTHORIZATION, admin_basic.clone())
        .json(&serde_json::json!({"openlibrary_base_url": "https://custom.example.com"}))
        .await;
    assert_eq!(r.status_code(), StatusCode::OK);

    let r2 = server
        .get("/api/settings")
        .add_header(axum::http::header::AUTHORIZATION, admin_basic)
        .await;
    let body: serde_json::Value = r2.json();
    assert_eq!(body["openlibrary_base_url"], "https://custom.example.com");
}

#[sqlx::test(migrations = "./migrations")]
async fn put_settings_unknown_field_returns_422(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin_id, admin_basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let server = server(&app_pool, &ingestion_pool);

    let r = server
        .put("/api/settings")
        .add_header(axum::http::header::AUTHORIZATION, admin_basic)
        .json(&serde_json::json!({"enrichment_concurency": 5}))
        .await;
    assert_eq!(r.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test(migrations = "./migrations")]
async fn put_settings_zero_cover_max_bytes_returns_422(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin_id, admin_basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let server = server(&app_pool, &ingestion_pool);

    let r = server
        .put("/api/settings")
        .add_header(axum::http::header::AUTHORIZATION, admin_basic)
        .json(&serde_json::json!({"cover_max_bytes": 0}))
        .await;
    assert_eq!(r.status_code(), StatusCode::UNPROCESSABLE_ENTITY);

    let body: serde_json::Value = r.json();
    let detail = body["detail"].as_str().unwrap_or_default();
    assert!(
        detail.contains("positive"),
        "expected 'positive' in detail, got {detail}"
    );
}
