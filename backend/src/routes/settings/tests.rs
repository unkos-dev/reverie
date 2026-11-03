use axum::http::StatusCode;
use sqlx::PgPool;

use crate::error::problems;
use crate::test_support;

fn server(app_pool: &PgPool, ingestion_pool: &PgPool) -> axum_test::TestServer {
    test_support::db::server_with_real_pools(app_pool, ingestion_pool)
}

async fn current_etag(server: &axum_test::TestServer, basic: &str) -> String {
    let r = server
        .get("/api/v1/settings")
        .add_header(axum::http::header::AUTHORIZATION, basic.to_owned())
        .await;
    assert_eq!(r.status_code(), StatusCode::OK);
    r.header(axum::http::header::ETAG)
        .to_str()
        .expect("ETag is ASCII")
        .to_owned()
}

async fn put_settings(
    server: &axum_test::TestServer,
    basic: &str,
    body: &serde_json::Value,
) -> axum_test::TestResponse {
    put_settings_with_etag(server, basic, &current_etag(server, basic).await, body).await
}

async fn put_settings_with_etag(
    server: &axum_test::TestServer,
    basic: &str,
    etag: &str,
    body: &serde_json::Value,
) -> axum_test::TestResponse {
    server
        .put("/api/v1/settings")
        .add_header(axum::http::header::AUTHORIZATION, basic.to_owned())
        .add_header(axum::http::header::IF_MATCH, etag.to_owned())
        .json(body)
        .await
}

async fn get_body(server: &axum_test::TestServer, basic: &str) -> serde_json::Value {
    server
        .get("/api/v1/settings")
        .add_header(axum::http::header::AUTHORIZATION, basic.to_owned())
        .await
        .json()
}

#[sqlx::test(migrations = "./migrations")]
async fn put_settings_with_matching_etag_persists_and_returns_the_new_etag(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let server = server(&app_pool, &ingestion_pool);

    let etag = current_etag(&server, &basic).await;
    let r = put_settings_with_etag(
        &server,
        &basic,
        &etag,
        &serde_json::json!({"opds_page_size": 62}),
    )
    .await;
    assert_eq!(r.status_code(), StatusCode::OK, "body = {}", r.text());
    let put_etag = r
        .header(axum::http::header::ETAG)
        .to_str()
        .unwrap()
        .to_owned();
    assert_ne!(put_etag, etag);
    assert_eq!(put_etag, current_etag(&server, &basic).await);
    assert_eq!(get_body(&server, &basic).await["opds_page_size"], 62);

    let chained = put_settings_with_etag(
        &server,
        &basic,
        &put_etag,
        &serde_json::json!({"opds_page_size": 63}),
    )
    .await;
    assert_eq!(chained.status_code(), StatusCode::OK);
}

#[sqlx::test(migrations = "./migrations")]
async fn put_settings_with_stale_etag_returns_412_and_leaves_settings_unchanged(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let server = server(&app_pool, &ingestion_pool);

    let stale = current_etag(&server, &basic).await;
    let winner = put_settings(&server, &basic, &serde_json::json!({"opds_page_size": 70})).await;
    assert_eq!(winner.status_code(), StatusCode::OK);
    let before = get_body(&server, &basic).await;

    let r = put_settings_with_etag(
        &server,
        &basic,
        &stale,
        &serde_json::json!({"opds_page_size": 80, "enrichment_concurrency": 9}),
    )
    .await;
    test_support::assert_problem(
        &r,
        problems::IF_MATCH_MISMATCH,
        StatusCode::PRECONDITION_FAILED,
    );
    assert_eq!(
        r.header(axum::http::header::ETAG).to_str().unwrap(),
        current_etag(&server, &basic).await,
        "the 412 must carry the current ETag"
    );

    let after = get_body(&server, &basic).await;
    assert_eq!(before, after, "a rejected write must not change the row");
    assert_eq!(after["opds_page_size"], 70);
    assert_eq!(after["revision"], before["revision"]);
}

#[sqlx::test(migrations = "./migrations")]
async fn put_settings_without_if_match_returns_428_and_leaves_settings_unchanged(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let server = server(&app_pool, &ingestion_pool);

    let before = get_body(&server, &basic).await;
    let r = server
        .put("/api/v1/settings")
        .add_header(axum::http::header::AUTHORIZATION, basic.clone())
        .json(&serde_json::json!({"opds_page_size": 90}))
        .await;
    test_support::assert_problem(
        &r,
        problems::IF_MATCH_REQUIRED,
        StatusCode::PRECONDITION_REQUIRED,
    );
    assert_eq!(before, get_body(&server, &basic).await);
}

#[sqlx::test(migrations = "./migrations")]
async fn put_settings_refuses_wildcard_weak_and_listed_if_match(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let server = server(&app_pool, &ingestion_pool);

    let current = current_etag(&server, &basic).await;
    let before = get_body(&server, &basic).await;
    for value in [
        "*".to_owned(),
        format!("W/{current}"),
        format!("{current}, \"other\""),
    ] {
        let r = put_settings_with_etag(
            &server,
            &basic,
            &value,
            &serde_json::json!({"opds_page_size": 91}),
        )
        .await;
        assert_eq!(
            r.status_code(),
            StatusCode::BAD_REQUEST,
            "If-Match: {value}"
        );
    }
    assert_eq!(before, get_body(&server, &basic).await);
}

#[sqlx::test(migrations = "./migrations")]
async fn put_settings_concurrent_writers_with_one_etag_yield_one_winner(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let server = server(&app_pool, &ingestion_pool);

    let etag = current_etag(&server, &basic).await;
    let first = serde_json::json!({"opds_page_size": 101});
    let second = serde_json::json!({"opds_page_size": 102});
    let (a, b) = tokio::join!(
        put_settings_with_etag(&server, &basic, &etag, &first),
        put_settings_with_etag(&server, &basic, &etag, &second),
    );

    let mut statuses = [a.status_code(), b.status_code()];
    statuses.sort();
    assert_eq!(
        statuses,
        [StatusCode::OK, StatusCode::PRECONDITION_FAILED],
        "exactly one writer may win"
    );
    let winning_size = if a.status_code() == StatusCode::OK {
        101
    } else {
        102
    };
    assert_eq!(
        get_body(&server, &basic).await["opds_page_size"],
        winning_size
    );
}
