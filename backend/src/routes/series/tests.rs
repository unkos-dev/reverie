//! Integration tests for `/api/v1/series/{id}`.
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
        .get(&format!("/api/v1/series/{series_id}"))
        .add_header(auth(&basic).0, auth(&basic).1)
        .await;
    test_support::assert_problem(&r, problems::NOT_FOUND, StatusCode::NOT_FOUND);
}

async fn attach_work(pool: &PgPool, series_id: Uuid, work_id: Uuid, position: Option<f64>) {
    sqlx::query!(
        "INSERT INTO series_works (series_id, work_id, position) VALUES ($1, $2, $3::float8)",
        series_id,
        work_id,
        position,
    )
    .execute(pool)
    .await
    .unwrap();
}

async fn set_title(pool: &PgPool, work_id: Uuid, title: &str) {
    sqlx::query!("UPDATE works SET title = $1 WHERE id = $2", title, work_id)
        .execute(pool)
        .await
        .unwrap();
}

async fn child_shelf(pool: &PgPool, app_pool: &PgPool, admin_id: Uuid, child_id: Uuid) -> Uuid {
    let shelf_id = test_support::db::create_shelf(app_pool, admin_id, "Shared").await;
    sqlx::query!(
        "UPDATE shelves SET user_id = $1 WHERE id = $2",
        child_id,
        shelf_id,
    )
    .execute(pool)
    .await
    .unwrap();
    shelf_id
}

#[sqlx::test(migrations = "./migrations")]
async fn detail_child_omits_inaccessible_works_and_preserves_metadata(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (admin_id, _) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let (child_id, basic) =
        test_support::db::create_child_user_and_basic_auth(&app_pool, "kid-partial").await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let (series_id, first, first_copy) =
        insert_series_with_one_work(&pool, &ingestion_pool, "Recorded Series", 2.0, "first").await;
    let (last, last_copy) =
        test_support::db::insert_work_and_manifestation(&ingestion_pool, "last").await;
    let (hidden, hidden_copy) =
        test_support::db::insert_work_and_manifestation(&ingestion_pool, "hidden").await;
    attach_work(&pool, series_id, last, Some(4.0)).await;
    attach_work(&pool, series_id, hidden, Some(3.0)).await;
    set_title(&pool, first, "Second Book").await;
    set_title(&pool, last, "Fourth Book").await;
    set_title(&pool, hidden, "Excluded Title Sentinel").await;
    test_support::db::insert_contributor(&pool, first, "Second Author", "author", 1).await;
    test_support::db::insert_contributor(&pool, first, "First Author", "author", 0).await;
    test_support::db::insert_contributor(&pool, last, "Fourth Author", "author", 0).await;
    let hidden_author = test_support::db::insert_contributor(
        &pool,
        hidden,
        "Excluded Author Sentinel",
        "author",
        0,
    )
    .await;
    let shelf = child_shelf(&pool, &app_pool, admin_id, child_id).await;
    test_support::db::add_to_shelf(&app_pool, shelf, first_copy).await;
    test_support::db::add_to_shelf(&app_pool, shelf, last_copy).await;

    let response = server
        .get(&format!("/api/v1/series/{series_id}"))
        .add_header(auth(&basic).0, auth(&basic).1)
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(body["id"], series_id.to_string());
    assert_eq!(body["name"], "Recorded Series");
    assert_eq!(body["sort_name"], "Recorded Series");
    let works = body["works"].as_array().unwrap();
    assert_eq!(works.len(), 2);
    for (work, id, title, authors, position, copy) in [
        (
            &works[0],
            first,
            "Second Book",
            vec!["First Author", "Second Author"],
            2.0,
            first_copy,
        ),
        (
            &works[1],
            last,
            "Fourth Book",
            vec!["Fourth Author"],
            4.0,
            last_copy,
        ),
    ] {
        assert_eq!(work["id"], id.to_string());
        assert_eq!(work["title"], title);
        assert_eq!(work["authors"], serde_json::json!(authors));
        assert_eq!(work["position"], position);
        assert_eq!(work["manifestations"].as_array().unwrap().len(), 1);
        assert_eq!(work["manifestations"][0]["id"], copy.to_string());
    }
    for sentinel in [
        hidden.to_string(),
        hidden_copy.to_string(),
        hidden_author.to_string(),
        "Excluded Title Sentinel".to_owned(),
        "Excluded Author Sentinel".to_owned(),
    ] {
        assert!(
            !response.text().contains(&sentinel),
            "excluded metadata: {sentinel}"
        );
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn detail_child_keeps_only_shared_manifestations_of_a_visible_work(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (admin_id, _) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let (child_id, basic) =
        test_support::db::create_child_user_and_basic_auth(&app_pool, "mixed-copies").await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let (series_id, work, shared_copy) =
        insert_series_with_one_work(&pool, &ingestion_pool, "Mixed Copies", 4.5, "shared-copy")
            .await;
    let (_, hidden_copy) =
        test_support::db::insert_work_and_manifestation(&ingestion_pool, "hidden-copy").await;
    sqlx::query!(
        "UPDATE manifestations SET work_id = $1 WHERE id = $2",
        work,
        hidden_copy
    )
    .execute(&ingestion_pool)
    .await
    .unwrap();
    set_title(&pool, work, "Recorded Work").await;
    test_support::db::insert_contributor(&pool, work, "Later Author", "author", 1).await;
    test_support::db::insert_contributor(&pool, work, "Earlier Author", "author", 0).await;
    let shelf = child_shelf(&pool, &app_pool, admin_id, child_id).await;
    test_support::db::add_to_shelf(&app_pool, shelf, shared_copy).await;
    let response = server
        .get(&format!("/api/v1/series/{series_id}"))
        .add_header(auth(&basic).0, auth(&basic).1)
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let body: serde_json::Value = response.json();
    let works = body["works"].as_array().unwrap();
    assert_eq!(works.len(), 1);
    assert_eq!(works[0]["id"], work.to_string());
    assert_eq!(works[0]["title"], "Recorded Work");
    assert_eq!(works[0]["position"], 4.5);
    assert_eq!(
        works[0]["authors"],
        serde_json::json!(["Earlier Author", "Later Author"])
    );
    assert_eq!(works[0]["manifestations"].as_array().unwrap().len(), 1);
    assert_eq!(works[0]["manifestations"][0]["id"], shared_copy.to_string());
    assert!(!response.text().contains(&hidden_copy.to_string()));
}
