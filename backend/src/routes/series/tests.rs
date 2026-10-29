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
async fn detail_requires_auth(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

    let r = server
        .get(&format!("/api/v1/series/{}", Uuid::new_v4()))
        .await;
    assert_eq!(r.status_code(), StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "./migrations")]
async fn detail_missing_series_returns_404_problem(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

    let r = server
        .get(&format!("/api/v1/series/{}", Uuid::new_v4()))
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
    let (w_b, m_b) = test_support::db::insert_work_and_manifestation(&ingestion_pool, "b").await;
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
    // First-positioned work carries a page count; the other stays NULL, so
    // the response must surface both states.
    sqlx::query!(
        "UPDATE manifestations SET pages = $1 WHERE id = $2",
        320,
        m_b,
    )
    .execute(&ingestion_pool)
    .await
    .unwrap();

    let r = server
        .get(&format!("/api/v1/series/{series_id}"))
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
    assert_eq!(works[0]["manifestations"][0]["pages"].as_i64(), Some(320));
    assert!(works[1]["manifestations"][0]["pages"].is_null());
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
async fn detail_child_matches_adult_catalogue_before_unshared_books_are_added(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (admin_id, _) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let (_, adult_auth) =
        test_support::db::create_adult_and_basic_auth(&app_pool, "adult-catalogue").await;
    let (child_id, child_auth) =
        test_support::db::create_child_user_and_basic_auth(&app_pool, "child-catalogue").await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let (series_id, first, first_copy) = insert_series_with_one_work(
        &pool,
        &ingestion_pool,
        "Equivalent Catalogue",
        2.5,
        "eq-first",
    )
    .await;
    let (last, last_copy) =
        test_support::db::insert_work_and_manifestation(&ingestion_pool, "eq-last").await;
    attach_work(&pool, series_id, last, None).await;
    set_title(&pool, first, "Fractional Book").await;
    set_title(&pool, last, "Unnumbered Book").await;
    test_support::db::insert_contributor(&pool, first, "Catalogue Author", "author", 0).await;
    let shelf = child_shelf(&pool, &app_pool, admin_id, child_id).await;
    test_support::db::add_to_shelf(&app_pool, shelf, first_copy).await;
    test_support::db::add_to_shelf(&app_pool, shelf, last_copy).await;
    let path = format!("/api/v1/series/{series_id}");
    let adult_before = server
        .get(&path)
        .add_header(auth(&adult_auth).0, auth(&adult_auth).1)
        .await;
    let child_before = server
        .get(&path)
        .add_header(auth(&child_auth).0, auth(&child_auth).1)
        .await;
    assert_eq!(adult_before.status_code(), StatusCode::OK);
    assert_eq!(child_before.status_code(), StatusCode::OK);
    let original: serde_json::Value = adult_before.json();
    assert_eq!(child_before.json::<serde_json::Value>(), original);
    assert_eq!(original["works"][0]["id"], first.to_string());
    assert_eq!(original["works"][0]["title"], "Fractional Book");
    assert_eq!(
        original["works"][0]["authors"],
        serde_json::json!(["Catalogue Author"])
    );
    assert_eq!(original["works"][0]["position"], 2.5);
    assert_eq!(original["works"][1]["id"], last.to_string());
    assert_eq!(original["works"][1]["title"], "Unnumbered Book");
    assert!(original["works"][1]["position"].is_null());

    let (added, _) =
        test_support::db::insert_work_and_manifestation(&ingestion_pool, "eq-added").await;
    attach_work(&pool, series_id, added, Some(3.0)).await;
    set_title(&pool, added, "Unshared Book").await;
    let child_after = server
        .get(&path)
        .add_header(auth(&child_auth).0, auth(&child_auth).1)
        .await;
    assert_eq!(child_after.status_code(), StatusCode::OK);
    assert_eq!(child_after.json::<serde_json::Value>(), original);
    let adult_after = server
        .get(&path)
        .add_header(auth(&adult_auth).0, auth(&adult_auth).1)
        .await;
    assert_eq!(adult_after.status_code(), StatusCode::OK);
    let updated: serde_json::Value = adult_after.json();
    assert_eq!(updated["works"].as_array().unwrap().len(), 3);
    assert_eq!(updated["works"][1]["id"], added.to_string());
    assert_eq!(updated["works"][2], original["works"][1]);
    assert_eq!(updated["works"][0], original["works"][0]);
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
        .get(&format!("/api/v1/series/{series_id}"))
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
