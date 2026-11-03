//! Integration tests for `/api/v1/shelves*`.

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

#[sqlx::test(migrations = "./migrations")]
async fn list_shelves_returns_only_callers_shelves(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (a_id, a_basic) =
        test_support::db::create_adult_and_basic_auth(&app_pool, "alpha-list").await;
    let (_b_id, _) = test_support::db::create_adult_and_basic_auth(&app_pool, "beta-list").await;
    test_support::db::create_shelf(&app_pool, a_id, "Alpha private").await;

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let r = server
        .get("/api/v1/shelves")
        .add_header(auth(&a_basic).0, auth(&a_basic).1)
        .await;
    assert_eq!(r.status_code(), StatusCode::OK);
    let body: serde_json::Value = r.json();
    let names: Vec<&str> = body["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"Alpha private"));
    assert!(!names.iter().any(|n| n.contains("beta")));
    assert!(body["next_cursor"].is_null());

    let shelf = body["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["name"] == "Alpha private")
        .expect("the caller's shelf is listed");
    test_support::assert_rfc3339(shelf, "created_at");
    test_support::assert_rfc3339(shelf, "updated_at");
}

#[sqlx::test(migrations = "./migrations")]
async fn rename_other_users_shelf_returns_404(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (a_id, _) = test_support::db::create_adult_and_basic_auth(&app_pool, "owner-A").await;
    let (_b_id, b_basic) =
        test_support::db::create_adult_and_basic_auth(&app_pool, "owner-B").await;
    let a_shelf = test_support::db::create_shelf(&app_pool, a_id, "A's shelf").await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

    let r = server
        .patch(&format!("/api/v1/shelves/{a_shelf}"))
        .add_header(auth(&b_basic).0, auth(&b_basic).1)
        .json(&json!({"name": "B tried"}))
        .await;
    test_support::assert_problem(&r, problems::NOT_FOUND, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = "./migrations")]
async fn delete_other_users_shelf_returns_404(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (a_id, _) = test_support::db::create_adult_and_basic_auth(&app_pool, "del-A").await;
    let (_b_id, b_basic) = test_support::db::create_adult_and_basic_auth(&app_pool, "del-B").await;
    let a_shelf = test_support::db::create_shelf(&app_pool, a_id, "A only").await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

    let r = server
        .delete(&format!("/api/v1/shelves/{a_shelf}"))
        .add_header(auth(&b_basic).0, auth(&b_basic).1)
        .await;
    test_support::assert_problem(&r, problems::NOT_FOUND, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = "./migrations")]
async fn child_cannot_create_shelf(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_c_id, c_basic) =
        test_support::db::create_child_user_and_basic_auth(&app_pool, "kid-create").await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let r = server
        .post("/api/v1/shelves")
        .add_header(auth(&c_basic).0, auth(&c_basic).1)
        .json(&json!({"name": "Forbidden"}))
        .await;
    test_support::assert_problem(&r, problems::FORBIDDEN, StatusCode::FORBIDDEN);
}

#[sqlx::test(migrations = "./migrations")]
async fn child_cannot_delete_shelf(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (c_id, c_basic) =
        test_support::db::create_child_user_and_basic_auth(&app_pool, "kid-delete").await;
    let shelf_id = test_support::db::create_shelf(&app_pool, c_id, "Kid's").await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let r = server
        .delete(&format!("/api/v1/shelves/{shelf_id}"))
        .add_header(auth(&c_basic).0, auth(&c_basic).1)
        .await;
    test_support::assert_problem(&r, problems::FORBIDDEN, StatusCode::FORBIDDEN);
}

// ------------------------------------------------------------------
// Items: add / remove
// ------------------------------------------------------------------

async fn make_owner_shelf_and_books(
    pool: &PgPool,
    app_pool: &PgPool,
    ingestion_pool: &PgPool,
    marker: &str,
    book_count: usize,
) -> (Uuid, String, Uuid, Vec<Uuid>) {
    let (user_id, basic) = test_support::db::create_adult_and_basic_auth(app_pool, marker).await;
    let shelf_id = test_support::db::create_shelf(app_pool, user_id, "Items").await;
    // Each manifestation needs to be visible to the user — add them
    // to the shelf so the manifestations RLS policy reads true.
    let mut manifestation_ids = Vec::with_capacity(book_count);
    for i in 0..book_count {
        let (_w, m) = test_support::db::insert_work_and_manifestation(
            ingestion_pool,
            &format!("{marker}-{i}"),
        )
        .await;
        manifestation_ids.push(m);
    }
    // Bypass POST /items so the shelf keeps its original updated_at.
    for m_id in &manifestation_ids {
        sqlx::query!(
            "INSERT INTO shelf_items (shelf_id, manifestation_id) VALUES ($1, $2)",
            shelf_id,
            m_id,
        )
        .execute(pool)
        .await
        .unwrap();
    }
    (user_id, basic, shelf_id, manifestation_ids)
}

#[sqlx::test(migrations = "./migrations")]
async fn add_shelf_item_404_when_child_cannot_see_manifestation(pool: PgPool) {
    // Adults have full manifestation visibility under the
    // `manifestations_select_adult` RLS policy (catalog is open by
    // design — shelves are curation, not access control). The
    // existence-leak threat only meaningfully applies to children,
    // whose RLS visibility is shelf-gated. This test exercises that
    // path: a child trying to add a not-on-their-shelf manifestation
    // gets 404 (existence-not-leaked), not 204.
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (c_id, c_basic) =
        test_support::db::create_child_user_and_basic_auth(&app_pool, "probe-kid").await;
    // The child owns a shelf (system-created via SQL) so they can
    // attempt the add — otherwise the shelf-ownership check would
    // 404 before the manifestation probe runs.
    let c_shelf: Uuid = sqlx::query_scalar!(
        "INSERT INTO shelves (user_id, name) VALUES ($1, 'Kids') RETURNING id",
        c_id,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let (_w, m_id) =
        test_support::db::insert_work_and_manifestation(&ingestion_pool, "probe-kid").await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let r = server
        .post(&format!("/api/v1/shelves/{c_shelf}/items"))
        .add_header(auth(&c_basic).0, auth(&c_basic).1)
        .json(&json!({"manifestation_id": m_id}))
        .await;
    test_support::assert_problem(&r, problems::NOT_FOUND, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = "./migrations")]
async fn delete_shelf_removes_its_items_and_keeps_the_manifestation(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_uid, basic, shelf_id, ids) =
        make_owner_shelf_and_books(&pool, &app_pool, &ingestion_pool, "cascade-shelf", 2).await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

    let r = server
        .delete(&format!("/api/v1/shelves/{shelf_id}"))
        .add_header(auth(&basic).0, auth(&basic).1)
        .await;
    assert_eq!(r.status_code(), StatusCode::NO_CONTENT);

    let items: i64 = sqlx::query_scalar!(
        r#"SELECT COUNT(*) AS "n!" FROM shelf_items WHERE shelf_id = $1"#,
        shelf_id,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(items, 0, "the shelf's items go with it");
    let manifestations: i64 = sqlx::query_scalar!(
        r#"SELECT COUNT(*) AS "n!" FROM manifestations WHERE id = ANY($1)"#,
        &ids,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(manifestations, 2, "deleting a shelf never deletes a book");
}

#[sqlx::test(migrations = "./migrations")]
async fn get_other_users_shelf_returns_404(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_a_id, _a_basic, a_shelf, _ids) =
        make_owner_shelf_and_books(&pool, &app_pool, &ingestion_pool, "get-owner", 1).await;
    let (_b_id, b_basic) =
        test_support::db::create_adult_and_basic_auth(&app_pool, "get-other").await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

    let r = server
        .get(&format!("/api/v1/shelves/{a_shelf}"))
        .add_header(auth(&b_basic).0, auth(&b_basic).1)
        .await;
    test_support::assert_problem(&r, problems::NOT_FOUND, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = "./migrations")]
async fn add_item_to_other_users_shelf_returns_404(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_a_id, _a_basic, a_shelf, _ids) =
        make_owner_shelf_and_books(&pool, &app_pool, &ingestion_pool, "add-owner", 1).await;
    let (_b_id, b_basic) =
        test_support::db::create_adult_and_basic_auth(&app_pool, "add-other").await;
    let (_w, b_visible) =
        test_support::db::insert_work_and_manifestation(&ingestion_pool, "add-other-book").await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

    let r = server
        .post(&format!("/api/v1/shelves/{a_shelf}/items"))
        .add_header(auth(&b_basic).0, auth(&b_basic).1)
        .json(&json!({"manifestation_id": b_visible}))
        .await;
    test_support::assert_problem(&r, problems::NOT_FOUND, StatusCode::NOT_FOUND);

    let count: i64 = sqlx::query_scalar!(
        "SELECT count(*) AS \"count!\" FROM shelf_items WHERE shelf_id = $1",
        a_shelf,
    )
    .fetch_one(&app_pool)
    .await
    .expect("fetch item count");
    assert_eq!(count, 1, "another account's add must not change the shelf");
}

#[sqlx::test(migrations = "./migrations")]
async fn remove_item_from_other_users_shelf_returns_404(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_a_id, _a_basic, a_shelf, ids) =
        make_owner_shelf_and_books(&pool, &app_pool, &ingestion_pool, "remove-owner", 1).await;
    let (_b_id, b_basic) =
        test_support::db::create_adult_and_basic_auth(&app_pool, "remove-other").await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

    let r = server
        .delete(&format!("/api/v1/shelves/{a_shelf}/items/{}", ids[0]))
        .add_header(auth(&b_basic).0, auth(&b_basic).1)
        .await;
    test_support::assert_problem(&r, problems::NOT_FOUND, StatusCode::NOT_FOUND);

    let count: i64 = sqlx::query_scalar!(
        "SELECT count(*) AS \"count!\" FROM shelf_items WHERE shelf_id = $1",
        a_shelf,
    )
    .fetch_one(&app_pool)
    .await
    .expect("fetch item count");
    assert_eq!(
        count, 1,
        "another account's remove must not change the shelf"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn child_cannot_rename_shelf(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (c_id, c_basic) =
        test_support::db::create_child_user_and_basic_auth(&app_pool, "kid-rename").await;
    let shelf_id = test_support::db::create_shelf(&app_pool, c_id, "Kid's").await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let r = server
        .patch(&format!("/api/v1/shelves/{shelf_id}"))
        .add_header(auth(&c_basic).0, auth(&c_basic).1)
        .json(&json!({"name": "New"}))
        .await;
    test_support::assert_problem(&r, problems::FORBIDDEN, StatusCode::FORBIDDEN);
}

#[sqlx::test(migrations = "./migrations")]
async fn child_can_view_own_shelves(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (c_id, c_basic) =
        test_support::db::create_child_user_and_basic_auth(&app_pool, "kid-view").await;
    test_support::db::create_shelf(&app_pool, c_id, "Kid's books").await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let r = server
        .get("/api/v1/shelves")
        .add_header(auth(&c_basic).0, auth(&c_basic).1)
        .await;
    assert_eq!(r.status_code(), StatusCode::OK);
    let arr: serde_json::Value = r.json();
    assert!(
        arr["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["name"].as_str() == Some("Kid's books"))
    );
}

// ── Keyset pagination on the shelves list + items page ─────────

#[sqlx::test(migrations = "./migrations")]
async fn shelf_items_rejects_cross_endpoint_cursor_replay(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (a_id, a_basic) =
        test_support::db::create_adult_and_basic_auth(&app_pool, "cross-tag").await;
    let shelf_id = test_support::db::create_shelf(&app_pool, a_id, "Replay shelf").await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

    // A structurally valid shelves-LIST cursor fed to the items
    // endpoint must surface the codec's UnknownTag as a 422, not a 500
    // or a silently wrong page.
    let foreign = crate::routes::cursor::ShelfCursor {
        is_system: false,
        name: "Replay shelf".into(),
        id: shelf_id,
    }
    .encode();
    let r = server
        .get(&format!("/api/v1/shelves/{shelf_id}?cursor={foreign}"))
        .add_header(auth(&a_basic).0, auth(&a_basic).1)
        .await;
    assert_eq!(r.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
}
