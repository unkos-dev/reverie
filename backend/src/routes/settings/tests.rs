use axum::http::StatusCode;
use sqlx::PgPool;

use crate::test_support;

fn server(app_pool: &PgPool, ingestion_pool: &PgPool) -> axum_test::TestServer {
    test_support::db::server_with_real_pools(app_pool, ingestion_pool)
}

#[tokio::test]
async fn get_settings_unauthenticated_returns_401() {
    let server = test_support::test_server();
    let r = server.get("/api/v1/settings").await;
    assert_eq!(r.status_code(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn put_settings_unauthenticated_returns_401() {
    let server = test_support::test_server();
    let r = server
        .put("/api/v1/settings")
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
        .get("/api/v1/settings")
        .add_header(axum::http::header::AUTHORIZATION, admin_basic)
        .await;
    assert_eq!(r.status_code(), StatusCode::OK);

    let body: serde_json::Value = r.json();
    assert!(body["enrichment_enabled"].is_boolean());
    assert!(body["enrichment_concurrency"].is_number());
    assert!(body["format_priority"].is_array());
    assert!(body["restart_required_fields"].is_array());
    test_support::assert_rfc3339(&body, "updated_at");
    assert!(
        body.get("last_successful_reload_at").is_some(),
        "last_successful_reload_at must be present in response"
    );
    assert!(
        body["last_successful_reload_at"].is_null()
            || body["last_successful_reload_at"].is_string(),
        "last_successful_reload_at must be null or RFC 3339 string"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn get_settings_as_non_admin_returns_403(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_adult_id, adult_basic) =
        test_support::db::create_adult_and_basic_auth(&app_pool, "settings-adult").await;
    let server = server(&app_pool, &ingestion_pool);

    let r = server
        .get("/api/v1/settings")
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
        .put("/api/v1/settings")
        .add_header(axum::http::header::AUTHORIZATION, admin_basic.clone())
        .json(&serde_json::json!({"enrichment_concurrency": 7}))
        .await;
    assert_eq!(r.status_code(), StatusCode::OK);

    let body: serde_json::Value = r.json();
    assert_eq!(body["enrichment_concurrency"], 7);
    assert_eq!(body["restart_required"], false);

    let r2 = server
        .get("/api/v1/settings")
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
        .put("/api/v1/settings")
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
        .put("/api/v1/settings")
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
        .put("/api/v1/settings")
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
        .put("/api/v1/settings")
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
        .put("/api/v1/settings")
        .add_header(axum::http::header::AUTHORIZATION, admin_basic.clone())
        .json(&serde_json::json!({"format_priority": ["pdf", "epub"]}))
        .await;
    assert_eq!(r.status_code(), StatusCode::OK);

    let r2 = server
        .get("/api/v1/settings")
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
        .put("/api/v1/settings")
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
        .put("/api/v1/settings")
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
        .put("/api/v1/settings")
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
        .put("/api/v1/settings")
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
        .put("/api/v1/settings")
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
        .put("/api/v1/settings")
        .add_header(axum::http::header::AUTHORIZATION, admin_basic.clone())
        .json(&serde_json::json!({"openlibrary_base_url": "https://custom.example.com"}))
        .await;
    assert_eq!(r.status_code(), StatusCode::OK);

    let r2 = server
        .get("/api/v1/settings")
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
        .put("/api/v1/settings")
        .add_header(axum::http::header::AUTHORIZATION, admin_basic)
        .json(&serde_json::json!({"enrichment_concurency": 5}))
        .await;
    assert_eq!(r.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test(migrations = "./migrations")]
async fn put_settings_provider_visibility_persists_and_round_trips(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin_id, admin_basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let server = server(&app_pool, &ingestion_pool);

    let r = server
        .put("/api/v1/settings")
        .add_header(axum::http::header::AUTHORIZATION, admin_basic.clone())
        .json(&serde_json::json!({"provider_visibility": {"googlebooks": false, "asin": true}}))
        .await;
    assert_eq!(r.status_code(), StatusCode::OK, "body = {}", r.text());
    let body: serde_json::Value = r.json();
    assert_eq!(
        body["provider_visibility"],
        serde_json::json!({"asin": true, "googlebooks": false})
    );

    let r2 = server
        .get("/api/v1/settings")
        .add_header(axum::http::header::AUTHORIZATION, admin_basic)
        .await;
    let body2: serde_json::Value = r2.json();
    assert_eq!(
        body2["provider_visibility"],
        serde_json::json!({"asin": true, "googlebooks": false})
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn put_settings_provider_visibility_unknown_key_returns_422(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin_id, admin_basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let server = server(&app_pool, &ingestion_pool);

    // `manual` exists in metadata_sources but is neither an identifier
    // scheme nor a rating source, so it is not a valid visibility key.
    let r = server
        .put("/api/v1/settings")
        .add_header(axum::http::header::AUTHORIZATION, admin_basic)
        .json(&serde_json::json!({"provider_visibility": {"manual": false}}))
        .await;
    assert_eq!(r.status_code(), StatusCode::UNPROCESSABLE_ENTITY);

    let body: serde_json::Value = r.json();
    let detail = body["detail"].as_str().unwrap_or_default();
    assert!(
        detail.contains("manual"),
        "expected detail naming the unknown key, got {detail}"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn put_settings_provider_visibility_non_bool_value_returns_422(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin_id, admin_basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let server = server(&app_pool, &ingestion_pool);

    let r = server
        .put("/api/v1/settings")
        .add_header(axum::http::header::AUTHORIZATION, admin_basic)
        .json(&serde_json::json!({"provider_visibility": {"googlebooks": "hidden"}}))
        .await;
    assert_eq!(r.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test(migrations = "./migrations")]
async fn put_settings_provider_visibility_over_cap_returns_422(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin_id, admin_basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let server = server(&app_pool, &ingestion_pool);

    let visibility: serde_json::Map<String, serde_json::Value> = (0..65)
        .map(|i| (format!("provider{i}"), serde_json::Value::Bool(true)))
        .collect();
    let r = server
        .put("/api/v1/settings")
        .add_header(axum::http::header::AUTHORIZATION, admin_basic)
        .json(&serde_json::json!({"provider_visibility": visibility}))
        .await;
    assert_eq!(r.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
    let body: serde_json::Value = r.json();
    let detail = body["detail"].as_str().unwrap_or_default();
    assert!(
        detail.contains("exceeds 64 entries"),
        "expected the entry-count cap message, got {detail}"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn put_settings_provider_visibility_at_cap_passes_count_check(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin_id, admin_basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let server = server(&app_pool, &ingestion_pool);

    // The registry seeds only 11 real provider ids, so a request at the
    // 64-entry cap cannot also be all valid keys. Asserting the rejection
    // is the registry's "unknown key" error, not the count-cap message,
    // proves the count check let 64 entries through to the registry lookup.
    let visibility: serde_json::Map<String, serde_json::Value> = (0..64)
        .map(|i| (format!("provider{i}"), serde_json::Value::Bool(true)))
        .collect();
    let r = server
        .put("/api/v1/settings")
        .add_header(axum::http::header::AUTHORIZATION, admin_basic)
        .json(&serde_json::json!({"provider_visibility": visibility}))
        .await;
    assert_eq!(r.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
    let body: serde_json::Value = r.json();
    let detail = body["detail"].as_str().unwrap_or_default();
    assert!(
        detail.contains("unknown provider_visibility key"),
        "expected the registry rejection, not the count cap, got {detail}"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn put_settings_revision_increases_per_update(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin_id, admin_basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let server = server(&app_pool, &ingestion_pool);

    let r1 = server
        .put("/api/v1/settings")
        .add_header(axum::http::header::AUTHORIZATION, admin_basic.clone())
        .json(&serde_json::json!({"opds_page_size": 60}))
        .await;
    assert_eq!(r1.status_code(), StatusCode::OK);
    let rev1 = r1.json::<serde_json::Value>()["revision"]
        .as_i64()
        .expect("revision in response");

    let r2 = server
        .put("/api/v1/settings")
        .add_header(axum::http::header::AUTHORIZATION, admin_basic)
        .json(&serde_json::json!({"opds_page_size": 70}))
        .await;
    assert_eq!(r2.status_code(), StatusCode::OK);
    let rev2 = r2.json::<serde_json::Value>()["revision"]
        .as_i64()
        .expect("revision in response");
    assert!(
        rev2 > rev1,
        "revision must increase on every update (got {rev1} then {rev2})"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn put_settings_zero_cover_max_bytes_returns_422(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin_id, admin_basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let server = server(&app_pool, &ingestion_pool);

    let r = server
        .put("/api/v1/settings")
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
