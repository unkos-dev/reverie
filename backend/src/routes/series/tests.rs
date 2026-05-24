//! Integration tests for `/api/series/{id}`.
//!
//! Existence gate, RLS isolation, child visibility, and the
//! ordering invariant from the route's docstring.

use axum::http::{HeaderName, HeaderValue, StatusCode};
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

/// Insert a series row + (work, manifestation) pair attached at the
/// given position. Returns `(series_id, work_id, manifestation_id)`.
async fn insert_series_with_one_work(
    schema_owner: &PgPool,
    ingestion_pool: &PgPool,
    series_name: &str,
    work_position: f64,
    marker: &str,
) -> (Uuid, Uuid, Uuid) {
    let series_id: Uuid = sqlx::query_scalar!(
        "INSERT INTO series (name, sort_name) VALUES ($1, $1) RETURNING id",
        series_name,
    )
    .fetch_one(schema_owner)
    .await
    .expect("insert series");
    let (work_id, manifestation_id) =
        test_support::db::insert_work_and_manifestation(ingestion_pool, marker).await;
    sqlx::query!(
        "INSERT INTO series_works (series_id, work_id, position) VALUES ($1, $2, $3::float8)",
        series_id,
        work_id,
        work_position,
    )
    .execute(schema_owner)
    .await
    .expect("attach work to series");
    (series_id, work_id, manifestation_id)
}

#[sqlx::test(migrations = "./migrations")]
async fn detail_requires_auth(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

    let r = server.get(&format!("/api/series/{}", Uuid::new_v4())).await;
    assert_eq!(r.status_code(), StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "./migrations")]
async fn detail_missing_series_returns_404_problem(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

    let r = server
        .get(&format!("/api/series/{}", Uuid::new_v4()))
        .add_header(auth(&basic).0, auth(&basic).1)
        .await;
    test_support::assert_problem(&r, problems::NOT_FOUND, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = "./migrations")]
async fn detail_happy_path_returns_ordered_works(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

    // Two works at positions 2.0 and 1.0; the response must surface
    // them in position-ascending order.
    let series_id: Uuid = sqlx::query_scalar!(
        "INSERT INTO series (name, sort_name) VALUES ('Cosmere', 'cosmere') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let (w_a, _m_a) = test_support::db::insert_work_and_manifestation(&ingestion_pool, "a").await;
    let (w_b, _m_b) = test_support::db::insert_work_and_manifestation(&ingestion_pool, "b").await;
    sqlx::query!(
        "INSERT INTO series_works (series_id, work_id, position) \
         VALUES ($1, $2, 2.0::float8), ($1, $3, 1.0::float8)",
        series_id,
        w_a,
        w_b,
    )
    .execute(&pool)
    .await
    .unwrap();

    let r = server
        .get(&format!("/api/series/{series_id}"))
        .add_header(auth(&basic).0, auth(&basic).1)
        .await;
    assert_eq!(r.status_code(), StatusCode::OK);
    let body: serde_json::Value = r.json();
    assert_eq!(body["id"].as_str().unwrap(), series_id.to_string());
    assert_eq!(body["name"], "Cosmere");
    let works = body["works"].as_array().unwrap();
    assert_eq!(works.len(), 2);
    assert_eq!(works[0]["id"].as_str().unwrap(), w_b.to_string());
    assert!((works[0]["position"].as_f64().unwrap() - 1.0).abs() < 1e-9);
    assert_eq!(works[1]["id"].as_str().unwrap(), w_a.to_string());
    assert!((works[1]["position"].as_f64().unwrap() - 2.0).abs() < 1e-9);
    assert_eq!(works[0]["manifestations"].as_array().unwrap().len(), 1);
}

#[sqlx::test(migrations = "./migrations")]
async fn detail_404_when_no_manifestation_in_series_visible(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    // Caller is a child user with NO shelves assigned — RLS hides
    // every manifestation in the series.
    let (_child, basic) =
        test_support::db::create_child_user_and_basic_auth(&app_pool, "kid-no-access").await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

    let (series_id, _w, _m) =
        insert_series_with_one_work(&pool, &ingestion_pool, "Wheel of Time", 1.0, "wot").await;
    let r = server
        .get(&format!("/api/series/{series_id}"))
        .add_header(auth(&basic).0, auth(&basic).1)
        .await;
    test_support::assert_problem(&r, problems::NOT_FOUND, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = "./migrations")]
async fn detail_child_sees_visible_works_with_gap_for_hidden_ones(pool: PgPool) {
    // Series has two works; child has access only to work A's
    // manifestation. The response must surface BOTH works (the work
    // catalog is not user-scoped), but work B's `manifestations`
    // array must be empty so the frontend can render a "missing" gap.
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (admin_id, _) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let (child_id, basic) =
        test_support::db::create_child_user_and_basic_auth(&app_pool, "kid-partial").await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

    let series_id: Uuid = sqlx::query_scalar!(
        "INSERT INTO series (name, sort_name) VALUES ('Partial', 'partial') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let (w_a, m_a) = test_support::db::insert_work_and_manifestation(&ingestion_pool, "p-a").await;
    let (w_b, _m_b) = test_support::db::insert_work_and_manifestation(&ingestion_pool, "p-b").await;
    sqlx::query!(
        "INSERT INTO series_works (series_id, work_id, position) \
         VALUES ($1, $2, 1.0::float8), ($1, $3, 2.0::float8)",
        series_id,
        w_a,
        w_b,
    )
    .execute(&pool)
    .await
    .unwrap();
    // Only m_a is on a shelf the child can see.
    let shelf_id = test_support::db::create_shelf(&app_pool, admin_id, "Shared").await;
    sqlx::query!(
        "UPDATE shelves SET user_id = $1 WHERE id = $2",
        child_id,
        shelf_id,
    )
    .execute(&pool)
    .await
    .unwrap();
    test_support::db::add_to_shelf(&app_pool, shelf_id, m_a).await;

    let r = server
        .get(&format!("/api/series/{series_id}"))
        .add_header(auth(&basic).0, auth(&basic).1)
        .await;
    assert_eq!(r.status_code(), StatusCode::OK, "body: {}", r.text());
    let body: serde_json::Value = r.json();
    let works = body["works"].as_array().unwrap();
    assert_eq!(works.len(), 2, "both works surface in catalog order");
    assert_eq!(works[0]["id"].as_str().unwrap(), w_a.to_string());
    assert_eq!(works[0]["manifestations"].as_array().unwrap().len(), 1);
    assert_eq!(works[1]["id"].as_str().unwrap(), w_b.to_string());
    assert_eq!(
        works[1]["manifestations"].as_array().unwrap().len(),
        0,
        "hidden work surfaces with empty manifestations array (gap)",
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn detail_child_sees_series_when_at_least_one_shelf_assignment(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (admin_id, _) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let (child_id, basic) =
        test_support::db::create_child_user_and_basic_auth(&app_pool, "kid-can-read").await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

    let (series_id, _w, manifestation_id) =
        insert_series_with_one_work(&pool, &ingestion_pool, "Tales of Earthsea", 1.0, "earthsea")
            .await;

    // Admin gives the child read access via shelf assignment (the
    // child-shared-shelf RLS policy on `manifestations`).
    let shelf_id = test_support::db::create_shelf(&app_pool, admin_id, "Shared with Kid").await;
    sqlx::query!(
        "UPDATE shelves SET user_id = $1 WHERE id = $2",
        child_id,
        shelf_id,
    )
    .execute(&pool)
    .await
    .unwrap();
    test_support::db::add_to_shelf(&app_pool, shelf_id, manifestation_id).await;

    let r = server
        .get(&format!("/api/series/{series_id}"))
        .add_header(auth(&basic).0, auth(&basic).1)
        .await;
    assert_eq!(r.status_code(), StatusCode::OK, "body: {}", r.text());
    let body: serde_json::Value = r.json();
    assert_eq!(body["works"].as_array().unwrap().len(), 1);
    assert_eq!(
        body["works"][0]["manifestations"].as_array().unwrap().len(),
        1,
    );
}
