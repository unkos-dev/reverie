//! Integration tests for `/api/shelves*`.

use axum::http::{HeaderName, HeaderValue, StatusCode, header};
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
async fn list_shelves_requires_auth(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let r = server.get("/api/shelves").await;
    assert_eq!(r.status_code(), StatusCode::UNAUTHORIZED);
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
        .get("/api/shelves")
        .add_header(auth(&a_basic).0, auth(&a_basic).1)
        .await;
    assert_eq!(r.status_code(), StatusCode::OK);
    let body: serde_json::Value = r.json();
    let names: Vec<&str> = body
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"Alpha private"));
    assert!(!names.iter().any(|n| n.contains("beta")));
}

#[sqlx::test(migrations = "./migrations")]
async fn create_shelf_round_trips(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_a_id, a_basic) =
        test_support::db::create_adult_and_basic_auth(&app_pool, "create-rt").await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

    let r = server
        .post("/api/shelves")
        .add_header(auth(&a_basic).0.clone(), auth(&a_basic).1.clone())
        .json(&json!({"name": "  Currently reading  "}))
        .await;
    assert_eq!(r.status_code(), StatusCode::CREATED);
    let body: serde_json::Value = r.json();
    assert_eq!(body["name"], "Currently reading");
    assert!(!body["is_system"].as_bool().unwrap());
    assert_eq!(body["item_count"].as_i64().unwrap(), 0);
    let etag = r
        .headers()
        .get(header::ETAG)
        .expect("ETag on create")
        .to_str()
        .unwrap();
    assert!(etag.starts_with('"'), "ETag must be quoted: {etag}");

    // List now includes the new shelf.
    let listed = server
        .get("/api/shelves")
        .add_header(auth(&a_basic).0.clone(), auth(&a_basic).1.clone())
        .await;
    assert_eq!(listed.status_code(), StatusCode::OK);
    let arr: serde_json::Value = listed.json();
    assert!(
        arr.as_array()
            .unwrap()
            .iter()
            .any(|v| v["name"].as_str() == Some("Currently reading"))
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn create_shelf_rejects_empty_name(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_a_id, a_basic) =
        test_support::db::create_adult_and_basic_auth(&app_pool, "empty-name").await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let r = server
        .post("/api/shelves")
        .add_header(auth(&a_basic).0, auth(&a_basic).1)
        .json(&json!({"name": "  "}))
        .await;
    test_support::assert_problem(&r, problems::VALIDATION, StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test(migrations = "./migrations")]
async fn rename_shelf_updates_name(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (a_id, a_basic) =
        test_support::db::create_adult_and_basic_auth(&app_pool, "rename-ok").await;
    let shelf_id = test_support::db::create_shelf(&app_pool, a_id, "Old name").await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

    let r = server
        .patch(&format!("/api/shelves/{shelf_id}"))
        .add_header(auth(&a_basic).0, auth(&a_basic).1)
        .json(&json!({"name": "New name"}))
        .await;
    assert_eq!(r.status_code(), StatusCode::OK, "body: {}", r.text());
    let body: serde_json::Value = r.json();
    assert_eq!(body["name"], "New name");
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
        .patch(&format!("/api/shelves/{a_shelf}"))
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
        .delete(&format!("/api/shelves/{a_shelf}"))
        .add_header(auth(&b_basic).0, auth(&b_basic).1)
        .await;
    test_support::assert_problem(&r, problems::NOT_FOUND, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = "./migrations")]
async fn rename_system_shelf_returns_409_system_shelf_immutable(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (a_id, a_basic) =
        test_support::db::create_adult_and_basic_auth(&app_pool, "sys-rename").await;
    let sys_id: Uuid = sqlx::query_scalar!(
        "INSERT INTO shelves (user_id, name, is_system) VALUES ($1, 'Read', TRUE) RETURNING id",
        a_id,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

    let r = server
        .patch(&format!("/api/shelves/{sys_id}"))
        .add_header(auth(&a_basic).0, auth(&a_basic).1)
        .json(&json!({"name": "Mine now"}))
        .await;
    test_support::assert_problem(&r, problems::SYSTEM_SHELF_IMMUTABLE, StatusCode::CONFLICT);
}

#[sqlx::test(migrations = "./migrations")]
async fn delete_system_shelf_returns_409_system_shelf_immutable(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (a_id, a_basic) =
        test_support::db::create_adult_and_basic_auth(&app_pool, "sys-delete").await;
    let sys_id: Uuid = sqlx::query_scalar!(
        "INSERT INTO shelves (user_id, name, is_system) VALUES ($1, 'Read', TRUE) RETURNING id",
        a_id,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let r = server
        .delete(&format!("/api/shelves/{sys_id}"))
        .add_header(auth(&a_basic).0, auth(&a_basic).1)
        .await;
    test_support::assert_problem(&r, problems::SYSTEM_SHELF_IMMUTABLE, StatusCode::CONFLICT);
}

#[sqlx::test(migrations = "./migrations")]
async fn delete_shelf_removes_row(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (a_id, a_basic) =
        test_support::db::create_adult_and_basic_auth(&app_pool, "del-happy").await;
    let shelf_id = test_support::db::create_shelf(&app_pool, a_id, "Toss me").await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

    let r = server
        .delete(&format!("/api/shelves/{shelf_id}"))
        .add_header(auth(&a_basic).0, auth(&a_basic).1)
        .await;
    assert_eq!(r.status_code(), StatusCode::NO_CONTENT);
    let remaining: i64 = sqlx::query_scalar!(
        r#"SELECT COUNT(*) AS "n!" FROM shelves WHERE id = $1"#,
        shelf_id,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(remaining, 0);
}

#[sqlx::test(migrations = "./migrations")]
async fn child_cannot_create_shelf(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_c_id, c_basic) =
        test_support::db::create_child_user_and_basic_auth(&app_pool, "kid-create").await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let r = server
        .post("/api/shelves")
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
        .delete(&format!("/api/shelves/{shelf_id}"))
        .add_header(auth(&c_basic).0, auth(&c_basic).1)
        .await;
    test_support::assert_problem(&r, problems::FORBIDDEN, StatusCode::FORBIDDEN);
}

// ------------------------------------------------------------------
// Items: add / remove / reorder (with If-Match precondition)
// ------------------------------------------------------------------

/// Strip the surrounding double quotes from a quoted `ETag` header
/// value (the way the handler emits the timestamp). Tests echo the
/// value verbatim on `If-Match`, so this is just a sanity-check.
fn etag_value(headers: &axum::http::HeaderMap) -> String {
    headers
        .get(header::ETAG)
        .expect("ETag header present")
        .to_str()
        .expect("ETag ascii")
        .to_owned()
}

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
    // Bypass POST /items so the shelf still has the original ETag.
    for (idx, m_id) in manifestation_ids.iter().enumerate() {
        sqlx::query!(
            "INSERT INTO shelf_items (shelf_id, manifestation_id, position) \
             VALUES ($1, $2, $3)",
            shelf_id,
            m_id,
            i32::try_from(idx).unwrap(),
        )
        .execute(pool)
        .await
        .unwrap();
    }
    (user_id, basic, shelf_id, manifestation_ids)
}

#[sqlx::test(migrations = "./migrations")]
async fn add_shelf_item_appends_and_bumps_etag(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (a_id, a_basic) =
        test_support::db::create_adult_and_basic_auth(&app_pool, "add-item").await;
    let shelf_id = test_support::db::create_shelf(&app_pool, a_id, "Stuff").await;
    let (_w, m_id) =
        test_support::db::insert_work_and_manifestation(&ingestion_pool, "add-item").await;
    // ensure manifestation visibility — assign via shelf membership
    // already done by the POST (item insert itself grants visibility
    // through the existing RLS policy), so the RLS-probe needs the
    // book to already be reachable. Pre-attach to a separate shelf
    // owned by the same user.
    let other = test_support::db::create_shelf(&app_pool, a_id, "Visible").await;
    test_support::db::add_to_shelf(&app_pool, other, m_id).await;

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    // Snapshot the initial ETag (pre-POST).
    let initial = server
        .get(&format!("/api/shelves/{shelf_id}"))
        .add_header(auth(&a_basic).0.clone(), auth(&a_basic).1.clone())
        .await;
    let initial_etag = etag_value(initial.headers());

    let r = server
        .post(&format!("/api/shelves/{shelf_id}/items"))
        .add_header(auth(&a_basic).0.clone(), auth(&a_basic).1.clone())
        .json(&json!({"manifestation_id": m_id}))
        .await;
    assert_eq!(
        r.status_code(),
        StatusCode::NO_CONTENT,
        "body: {}",
        r.text()
    );
    let new_etag = etag_value(r.headers());
    assert_ne!(
        initial_etag, new_etag,
        "POST /items must bump the ETag (initial: {initial_etag}, new: {new_etag})",
    );
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
        .post(&format!("/api/shelves/{c_shelf}/items"))
        .add_header(auth(&c_basic).0, auth(&c_basic).1)
        .json(&json!({"manifestation_id": m_id}))
        .await;
    test_support::assert_problem(&r, problems::NOT_FOUND, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = "./migrations")]
async fn remove_shelf_item_bumps_etag(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_uid, basic, shelf_id, ids) =
        make_owner_shelf_and_books(&pool, &app_pool, &ingestion_pool, "rm-bump", 2).await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let initial = server
        .get(&format!("/api/shelves/{shelf_id}"))
        .add_header(auth(&basic).0.clone(), auth(&basic).1.clone())
        .await;
    let initial_etag = etag_value(initial.headers());

    let r = server
        .delete(&format!("/api/shelves/{shelf_id}/items/{}", ids[0]))
        .add_header(auth(&basic).0.clone(), auth(&basic).1.clone())
        .await;
    assert_eq!(r.status_code(), StatusCode::NO_CONTENT);
    assert_ne!(initial_etag, etag_value(r.headers()));
}

#[sqlx::test(migrations = "./migrations")]
async fn reorder_without_if_match_returns_428(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_uid, basic, shelf_id, ids) =
        make_owner_shelf_and_books(&pool, &app_pool, &ingestion_pool, "no-ifmatch", 2).await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let r = server
        .put(&format!("/api/shelves/{shelf_id}/items"))
        .add_header(auth(&basic).0, auth(&basic).1)
        .json(&json!({"items": [ids[1], ids[0]]}))
        .await;
    test_support::assert_problem(
        &r,
        problems::IF_MATCH_REQUIRED,
        StatusCode::PRECONDITION_REQUIRED,
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn reorder_with_stale_if_match_returns_412(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_uid, basic, shelf_id, ids) =
        make_owner_shelf_and_books(&pool, &app_pool, &ingestion_pool, "stale", 2).await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let initial = server
        .get(&format!("/api/shelves/{shelf_id}"))
        .add_header(auth(&basic).0.clone(), auth(&basic).1.clone())
        .await;
    let stale_etag = etag_value(initial.headers());

    // Bump updated_at out-of-band so the captured ETag is stale.
    sqlx::query!(
        "UPDATE shelves SET updated_at = now() + interval '1 second' WHERE id = $1",
        shelf_id,
    )
    .execute(&pool)
    .await
    .unwrap();

    let r = server
        .put(&format!("/api/shelves/{shelf_id}/items"))
        .add_header(auth(&basic).0.clone(), auth(&basic).1.clone())
        .add_header(
            header::IF_MATCH,
            HeaderValue::from_str(&stale_etag).unwrap(),
        )
        .json(&json!({"items": [ids[1], ids[0]]}))
        .await;
    test_support::assert_problem(
        &r,
        problems::IF_MATCH_MISMATCH,
        StatusCode::PRECONDITION_FAILED,
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn reorder_happy_path_persists_new_order(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_uid, basic, shelf_id, ids) =
        make_owner_shelf_and_books(&pool, &app_pool, &ingestion_pool, "reorder-ok", 3).await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let initial = server
        .get(&format!("/api/shelves/{shelf_id}"))
        .add_header(auth(&basic).0.clone(), auth(&basic).1.clone())
        .await;
    let etag = etag_value(initial.headers());

    let new_order = vec![ids[2], ids[0], ids[1]];
    let r = server
        .put(&format!("/api/shelves/{shelf_id}/items"))
        .add_header(auth(&basic).0.clone(), auth(&basic).1.clone())
        .add_header(header::IF_MATCH, HeaderValue::from_str(&etag).unwrap())
        .json(&json!({"items": new_order}))
        .await;
    assert_eq!(
        r.status_code(),
        StatusCode::NO_CONTENT,
        "body: {}",
        r.text()
    );

    let after = server
        .get(&format!("/api/shelves/{shelf_id}"))
        .add_header(auth(&basic).0.clone(), auth(&basic).1.clone())
        .await;
    let body: serde_json::Value = after.json();
    let surfaced: Vec<String> = body["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["manifestation_id"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(
        surfaced,
        vec![ids[2].to_string(), ids[0].to_string(), ids[1].to_string(),],
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn reorder_rejects_partial_list(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_uid, basic, shelf_id, ids) =
        make_owner_shelf_and_books(&pool, &app_pool, &ingestion_pool, "partial", 3).await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let initial = server
        .get(&format!("/api/shelves/{shelf_id}"))
        .add_header(auth(&basic).0.clone(), auth(&basic).1.clone())
        .await;
    let etag = etag_value(initial.headers());

    let r = server
        .put(&format!("/api/shelves/{shelf_id}/items"))
        .add_header(auth(&basic).0.clone(), auth(&basic).1.clone())
        .add_header(header::IF_MATCH, HeaderValue::from_str(&etag).unwrap())
        .json(&json!({"items": [ids[0]]}))
        .await;
    test_support::assert_problem(&r, problems::VALIDATION, StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test(migrations = "./migrations")]
async fn reorder_with_non_quoted_if_match_returns_422(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_uid, basic, shelf_id, ids) =
        make_owner_shelf_and_books(&pool, &app_pool, &ingestion_pool, "unquoted", 2).await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let r = server
        .put(&format!("/api/shelves/{shelf_id}/items"))
        .add_header(auth(&basic).0, auth(&basic).1)
        .add_header(
            header::IF_MATCH,
            HeaderValue::from_static("2026-05-24T03:00:00Z"),
        )
        .json(&json!({"items": [ids[1], ids[0]]}))
        .await;
    test_support::assert_problem(&r, problems::VALIDATION, StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test(migrations = "./migrations")]
async fn reorder_with_weak_etag_if_match_returns_422(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_uid, basic, shelf_id, ids) =
        make_owner_shelf_and_books(&pool, &app_pool, &ingestion_pool, "weak", 2).await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let r = server
        .put(&format!("/api/shelves/{shelf_id}/items"))
        .add_header(auth(&basic).0, auth(&basic).1)
        .add_header(
            header::IF_MATCH,
            HeaderValue::from_static("W/\"2026-05-24T03:00:00Z\""),
        )
        .json(&json!({"items": [ids[1], ids[0]]}))
        .await;
    test_support::assert_problem(&r, problems::VALIDATION, StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test(migrations = "./migrations")]
async fn reorder_with_malformed_timestamp_if_match_returns_422(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_uid, basic, shelf_id, ids) =
        make_owner_shelf_and_books(&pool, &app_pool, &ingestion_pool, "malformed", 2).await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let r = server
        .put(&format!("/api/shelves/{shelf_id}/items"))
        .add_header(auth(&basic).0, auth(&basic).1)
        .add_header(header::IF_MATCH, HeaderValue::from_static("\"garbage\""))
        .json(&json!({"items": [ids[1], ids[0]]}))
        .await;
    test_support::assert_problem(&r, problems::VALIDATION, StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test(migrations = "./migrations")]
async fn reorder_rejects_foreign_manifestation_id(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_uid, basic, shelf_id, ids) =
        make_owner_shelf_and_books(&pool, &app_pool, &ingestion_pool, "foreign", 2).await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let initial = server
        .get(&format!("/api/shelves/{shelf_id}"))
        .add_header(auth(&basic).0.clone(), auth(&basic).1.clone())
        .await;
    let etag = etag_value(initial.headers());
    let foreign = Uuid::new_v4();
    let r = server
        .put(&format!("/api/shelves/{shelf_id}/items"))
        .add_header(auth(&basic).0.clone(), auth(&basic).1.clone())
        .add_header(header::IF_MATCH, HeaderValue::from_str(&etag).unwrap())
        .json(&json!({"items": [ids[0], foreign]}))
        .await;
    test_support::assert_problem(&r, problems::VALIDATION, StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test(migrations = "./migrations")]
async fn duplicate_add_item_is_idempotent_no_double_position(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (a_id, a_basic) = test_support::db::create_adult_and_basic_auth(&app_pool, "dup-add").await;
    let shelf_id = test_support::db::create_shelf(&app_pool, a_id, "Stuff").await;
    let (_w, m_id) =
        test_support::db::insert_work_and_manifestation(&ingestion_pool, "dup-add").await;
    let other = test_support::db::create_shelf(&app_pool, a_id, "Visible").await;
    test_support::db::add_to_shelf(&app_pool, other, m_id).await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

    for _ in 0..2 {
        let r = server
            .post(&format!("/api/shelves/{shelf_id}/items"))
            .add_header(auth(&a_basic).0.clone(), auth(&a_basic).1.clone())
            .json(&json!({"manifestation_id": m_id}))
            .await;
        assert_eq!(r.status_code(), StatusCode::NO_CONTENT);
    }
    // ON CONFLICT DO NOTHING — exactly one row.
    let after = server
        .get(&format!("/api/shelves/{shelf_id}"))
        .add_header(auth(&a_basic).0, auth(&a_basic).1)
        .await;
    let body: serde_json::Value = after.json();
    assert_eq!(body["items"].as_array().unwrap().len(), 1);
}

#[sqlx::test(migrations = "./migrations")]
async fn remove_shelf_item_404_when_item_not_on_shelf(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (a_id, a_basic) =
        test_support::db::create_adult_and_basic_auth(&app_pool, "rm-phantom").await;
    let shelf_id = test_support::db::create_shelf(&app_pool, a_id, "Empty").await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let phantom = Uuid::new_v4();
    let r = server
        .delete(&format!("/api/shelves/{shelf_id}/items/{phantom}"))
        .add_header(auth(&a_basic).0, auth(&a_basic).1)
        .await;
    test_support::assert_problem(&r, problems::NOT_FOUND, StatusCode::NOT_FOUND);
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
        .patch(&format!("/api/shelves/{shelf_id}"))
        .add_header(auth(&c_basic).0, auth(&c_basic).1)
        .json(&json!({"name": "New"}))
        .await;
    test_support::assert_problem(&r, problems::FORBIDDEN, StatusCode::FORBIDDEN);
}

#[sqlx::test(migrations = "./migrations")]
async fn rename_shelf_rejects_empty_name(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (a_id, a_basic) =
        test_support::db::create_adult_and_basic_auth(&app_pool, "ren-empty").await;
    let shelf_id = test_support::db::create_shelf(&app_pool, a_id, "Old").await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let r = server
        .patch(&format!("/api/shelves/{shelf_id}"))
        .add_header(auth(&a_basic).0, auth(&a_basic).1)
        .json(&json!({"name": "   "}))
        .await;
    test_support::assert_problem(&r, problems::VALIDATION, StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test(migrations = "./migrations")]
async fn parallel_reorders_with_same_if_match_serialize(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_uid, basic, shelf_id, ids) =
        make_owner_shelf_and_books(&pool, &app_pool, &ingestion_pool, "concurrent", 2).await;
    let server = std::sync::Arc::new(test_support::db::server_with_real_pools(
        &app_pool,
        &ingestion_pool,
    ));
    let initial = server
        .get(&format!("/api/shelves/{shelf_id}"))
        .add_header(auth(&basic).0.clone(), auth(&basic).1.clone())
        .await;
    let etag = etag_value(initial.headers());

    let path = format!("/api/shelves/{shelf_id}/items");
    let basic1 = basic.clone();
    let basic2 = basic.clone();
    let etag1 = etag.clone();
    let etag2 = etag.clone();
    let ids1 = ids.clone();
    let ids2 = ids.clone();
    let s1 = std::sync::Arc::clone(&server);
    let s2 = std::sync::Arc::clone(&server);
    let path1 = path.clone();
    let path2 = path.clone();

    let (r1, r2) = tokio::join!(
        async move {
            s1.put(&path1)
                .add_header(auth(&basic1).0.clone(), auth(&basic1).1.clone())
                .add_header(header::IF_MATCH, HeaderValue::from_str(&etag1).unwrap())
                .json(&json!({"items": [ids1[1], ids1[0]]}))
                .await
        },
        async move {
            s2.put(&path2)
                .add_header(auth(&basic2).0.clone(), auth(&basic2).1.clone())
                .add_header(header::IF_MATCH, HeaderValue::from_str(&etag2).unwrap())
                .json(&json!({"items": [ids2[0], ids2[1]]}))
                .await
        },
    );

    let mut codes = [r1.status_code(), r2.status_code()];
    codes.sort_by_key(StatusCode::as_u16);
    assert_eq!(
        codes,
        [StatusCode::NO_CONTENT, StatusCode::PRECONDITION_FAILED],
        "exactly one parallel reorder must win, the other must 412 — got {codes:?}",
    );
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
        .get("/api/shelves")
        .add_header(auth(&c_basic).0, auth(&c_basic).1)
        .await;
    assert_eq!(r.status_code(), StatusCode::OK);
    let arr: serde_json::Value = r.json();
    assert!(
        arr.as_array()
            .unwrap()
            .iter()
            .any(|v| v["name"].as_str() == Some("Kid's books"))
    );
}
