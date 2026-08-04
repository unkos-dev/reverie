//! Integration tests for `/api/v1/shelves*`.

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
    let r = server.get("/api/v1/shelves").await;
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
async fn create_shelf_round_trips(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_a_id, a_basic) =
        test_support::db::create_adult_and_basic_auth(&app_pool, "create-rt").await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

    let r = server
        .post("/api/v1/shelves")
        .add_header(auth(&a_basic).0.clone(), auth(&a_basic).1.clone())
        .json(&json!({"name": "  Currently reading  "}))
        .await;
    assert_eq!(r.status_code(), StatusCode::CREATED);
    let body: serde_json::Value = r.json();
    assert_eq!(body["name"], "Currently reading");
    assert!(!body["is_system"].as_bool().unwrap());
    assert_eq!(body["item_count"].as_i64().unwrap(), 0);
    test_support::assert_rfc3339(&body, "created_at");
    test_support::assert_rfc3339(&body, "updated_at");
    let etag = r
        .headers()
        .get(header::ETAG)
        .expect("ETag on create")
        .to_str()
        .unwrap();
    assert!(etag.starts_with('"'), "ETag must be quoted: {etag}");
    assert_eq!(
        etag.trim_matches('"'),
        body["updated_at"].as_str().unwrap(),
        "the ETag is the body's updated_at, so the two must agree textually",
    );

    // List now includes the new shelf.
    let listed = server
        .get("/api/v1/shelves")
        .add_header(auth(&a_basic).0.clone(), auth(&a_basic).1.clone())
        .await;
    assert_eq!(listed.status_code(), StatusCode::OK);
    let arr: serde_json::Value = listed.json();
    assert!(
        arr["items"]
            .as_array()
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
        .post("/api/v1/shelves")
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
        .patch(&format!("/api/v1/shelves/{shelf_id}"))
        .add_header(auth(&a_basic).0, auth(&a_basic).1)
        .json(&json!({"name": "New name"}))
        .await;
    assert_eq!(r.status_code(), StatusCode::OK, "body: {}", r.text());
    let body: serde_json::Value = r.json();
    assert_eq!(body["name"], "New name");
    test_support::assert_rfc3339(&body, "created_at");
    test_support::assert_rfc3339(&body, "updated_at");
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
        .patch(&format!("/api/v1/shelves/{sys_id}"))
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
        .delete(&format!("/api/v1/shelves/{sys_id}"))
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
        .delete(&format!("/api/v1/shelves/{shelf_id}"))
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
        .get(&format!("/api/v1/shelves/{shelf_id}"))
        .add_header(auth(&a_basic).0.clone(), auth(&a_basic).1.clone())
        .await;
    let initial_etag = etag_value(initial.headers());

    let r = server
        .post(&format!("/api/v1/shelves/{shelf_id}/items"))
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
        .post(&format!("/api/v1/shelves/{c_shelf}/items"))
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
        .get(&format!("/api/v1/shelves/{shelf_id}"))
        .add_header(auth(&basic).0.clone(), auth(&basic).1.clone())
        .await;
    let initial_etag = etag_value(initial.headers());

    let r = server
        .delete(&format!("/api/v1/shelves/{shelf_id}/items/{}", ids[0]))
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
        .put(&format!("/api/v1/shelves/{shelf_id}/items"))
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
        .get(&format!("/api/v1/shelves/{shelf_id}"))
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
        .put(&format!("/api/v1/shelves/{shelf_id}/items"))
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
        .get(&format!("/api/v1/shelves/{shelf_id}"))
        .add_header(auth(&basic).0.clone(), auth(&basic).1.clone())
        .await;
    let etag = etag_value(initial.headers());

    let new_order = vec![ids[2], ids[0], ids[1]];
    let r = server
        .put(&format!("/api/v1/shelves/{shelf_id}/items"))
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
        .get(&format!("/api/v1/shelves/{shelf_id}"))
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
async fn shelf_detail_timestamps_round_trip_as_if_match(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_uid, basic, shelf_id, ids) =
        make_owner_shelf_and_books(&pool, &app_pool, &ingestion_pool, "wire-detail", 2).await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

    let r = server
        .get(&format!("/api/v1/shelves/{shelf_id}"))
        .add_header(auth(&basic).0.clone(), auth(&basic).1.clone())
        .await;
    assert_eq!(r.status_code(), StatusCode::OK, "body: {}", r.text());
    let etag = etag_value(r.headers());
    let body: serde_json::Value = r.json();

    // The detail envelope is route-local and re-declares both
    // timestamps, so the model DTO being correct proves nothing here.
    test_support::assert_rfc3339(&body, "created_at");
    test_support::assert_rfc3339(&body, "updated_at");
    for item in body["items"].as_array().expect("items array") {
        test_support::assert_rfc3339(item, "added_at");
    }

    let read_updated_at = body["updated_at"].as_str().unwrap();
    assert_eq!(
        etag.trim_matches('"'),
        read_updated_at,
        "the ETag header and the body's updated_at must be the same text",
    );

    // The property clients depend on: the timestamp read from the body,
    // quoted, is accepted as If-Match. A body shape the client cannot
    // reproduce as a header value breaks the reorder contract even when
    // the header alone is well-formed.
    let reorder = server
        .put(&format!("/api/v1/shelves/{shelf_id}/items"))
        .add_header(auth(&basic).0.clone(), auth(&basic).1.clone())
        .add_header(
            header::IF_MATCH,
            HeaderValue::from_str(&format!("\"{read_updated_at}\"")).unwrap(),
        )
        .json(&json!({"items": [ids[1], ids[0]]}))
        .await;
    assert_eq!(
        reorder.status_code(),
        StatusCode::NO_CONTENT,
        "body: {}",
        reorder.text()
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn shelf_timestamps_keep_sub_second_precision(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (a_id, a_basic) =
        test_support::db::create_adult_and_basic_auth(&app_pool, "sub-second").await;
    // Written at INSERT, not UPDATE: the `shelves_set_updated_at`
    // trigger fires BEFORE UPDATE and would replace any chosen value
    // with `now()`.
    let shelf_id: Uuid = sqlx::query_scalar!(
        "INSERT INTO shelves (user_id, name, created_at, updated_at) \
         VALUES ($1, 'Precise', '2026-05-24T01:00:00.123456Z', \
         '2026-05-24T01:00:00.123456Z') RETURNING id",
        a_id,
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let r = server
        .get(&format!("/api/v1/shelves/{shelf_id}"))
        .add_header(auth(&a_basic).0.clone(), auth(&a_basic).1.clone())
        .await;
    assert_eq!(r.status_code(), StatusCode::OK, "body: {}", r.text());
    let body: serde_json::Value = r.json();

    for field in ["created_at", "updated_at"] {
        let parsed = test_support::assert_rfc3339(&body, field);
        assert_eq!(
            parsed.nanosecond(),
            123_456_000,
            "sub-second precision must survive serialisation: {}",
            body[field],
        );
        // Sub-second digits are emitted only when non-zero, so consumers
        // must accept variable precision rather than a fixed width.
        assert!(
            body[field].as_str().unwrap().contains('.'),
            "a fractional second must be emitted when present: {}",
            body[field],
        );
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn reorder_rejects_partial_list(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_uid, basic, shelf_id, ids) =
        make_owner_shelf_and_books(&pool, &app_pool, &ingestion_pool, "partial", 3).await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let initial = server
        .get(&format!("/api/v1/shelves/{shelf_id}"))
        .add_header(auth(&basic).0.clone(), auth(&basic).1.clone())
        .await;
    let etag = etag_value(initial.headers());

    let r = server
        .put(&format!("/api/v1/shelves/{shelf_id}/items"))
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
        .put(&format!("/api/v1/shelves/{shelf_id}/items"))
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
        .put(&format!("/api/v1/shelves/{shelf_id}/items"))
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
        .put(&format!("/api/v1/shelves/{shelf_id}/items"))
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
        .get(&format!("/api/v1/shelves/{shelf_id}"))
        .add_header(auth(&basic).0.clone(), auth(&basic).1.clone())
        .await;
    let etag = etag_value(initial.headers());
    let foreign = Uuid::new_v4();
    let r = server
        .put(&format!("/api/v1/shelves/{shelf_id}/items"))
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
            .post(&format!("/api/v1/shelves/{shelf_id}/items"))
            .add_header(auth(&a_basic).0.clone(), auth(&a_basic).1.clone())
            .json(&json!({"manifestation_id": m_id}))
            .await;
        assert_eq!(r.status_code(), StatusCode::NO_CONTENT);
    }
    // ON CONFLICT DO NOTHING — exactly one row.
    let after = server
        .get(&format!("/api/v1/shelves/{shelf_id}"))
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
        .delete(&format!("/api/v1/shelves/{shelf_id}/items/{phantom}"))
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
        .patch(&format!("/api/v1/shelves/{shelf_id}"))
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
        .patch(&format!("/api/v1/shelves/{shelf_id}"))
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
        .get(&format!("/api/v1/shelves/{shelf_id}"))
        .add_header(auth(&basic).0.clone(), auth(&basic).1.clone())
        .await;
    let etag = etag_value(initial.headers());

    let path = format!("/api/v1/shelves/{shelf_id}/items");
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
async fn list_shelves_pagination_walks_across_system_boundary(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (a_id, a_basic) =
        test_support::db::create_adult_and_basic_auth(&app_pool, "page-walk").await;
    // One system shelf + two user shelves; sort order is
    // (is_system DESC, name ASC, id ASC) → [system, Alpha, Bravo].
    sqlx::query!(
        "INSERT INTO shelves (user_id, name, is_system) VALUES ($1, 'Zys', TRUE)",
        a_id,
    )
    .execute(&app_pool)
    .await
    .expect("insert system shelf");
    test_support::db::create_shelf(&app_pool, a_id, "Alpha").await;
    test_support::db::create_shelf(&app_pool, a_id, "Bravo").await;

    // page_size = 1 forces a page boundary exactly at the
    // is_system DESC → name ASC transition (the mixed-direction OR arm).
    let server = test_support::db::server_with_real_pools_page_size(&app_pool, &ingestion_pool, 1);

    let mut names: Vec<String> = Vec::new();
    let mut url = "/api/v1/shelves".to_string();
    let mut pages = 0u32;
    loop {
        let r = server
            .get(&url)
            .add_header(auth(&a_basic).0.clone(), auth(&a_basic).1.clone())
            .await;
        assert_eq!(r.status_code(), StatusCode::OK);
        let body: serde_json::Value = r.json();
        for v in body["items"].as_array().unwrap() {
            names.push(v["name"].as_str().unwrap().to_owned());
        }
        pages += 1;
        assert!(pages < 10, "runaway pagination");
        if let Some(nc) = body["next_cursor"].as_str() {
            // The Link header mirrors the body cursor.
            let link = r
                .headers()
                .get(header::LINK)
                .expect("Link header on overflow page")
                .to_str()
                .unwrap();
            assert!(link.contains(r#"rel="next""#), "Link header: {link}");
            url = format!("/api/v1/shelves?cursor={nc}");
        } else {
            assert!(
                r.headers().get(header::LINK).is_none(),
                "final page must not carry a Link rel=next header"
            );
            break;
        }
    }
    assert_eq!(names, ["Zys", "Alpha", "Bravo"], "walked {pages} pages");
}

#[sqlx::test(migrations = "./migrations")]
async fn list_shelves_rejects_malformed_cursor(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_a_id, a_basic) =
        test_support::db::create_adult_and_basic_auth(&app_pool, "bad-cursor").await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let r = server
        .get("/api/v1/shelves?cursor=!!!not-base64url!!!")
        .add_header(auth(&a_basic).0, auth(&a_basic).1)
        .await;
    assert_eq!(r.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test(migrations = "./migrations")]
async fn shelf_items_pagination_total_under_identical_sort_keys(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (a_id, a_basic) =
        test_support::db::create_adult_and_basic_auth(&app_pool, "items-walk").await;
    let shelf_id = test_support::db::create_shelf(&app_pool, a_id, "Walk shelf").await;

    // Three items sharing position AND added_at — only the
    // manifestation_id tiebreaker keeps the walk total. A page boundary
    // between two of them must not drop or duplicate a row.
    let mut expected: Vec<Uuid> = Vec::new();
    for marker in ["walk-zebra", "walk-quill", "walk-marsh"] {
        let (_w, m_id) =
            test_support::db::insert_work_and_manifestation(&ingestion_pool, marker).await;
        sqlx::query!(
            "INSERT INTO shelf_items (shelf_id, manifestation_id, position, added_at) \
             VALUES ($1, $2, 0, '2026-01-01T00:00:00Z')",
            shelf_id,
            m_id,
        )
        .execute(&app_pool)
        .await
        .expect("insert shelf item");
        expected.push(m_id);
    }
    expected.sort();

    let server = test_support::db::server_with_real_pools_page_size(&app_pool, &ingestion_pool, 2);

    let mut seen: Vec<Uuid> = Vec::new();
    let mut url = format!("/api/v1/shelves/{shelf_id}");
    let mut pages = 0u32;
    loop {
        let r = server
            .get(&url)
            .add_header(auth(&a_basic).0.clone(), auth(&a_basic).1.clone())
            .await;
        assert_eq!(r.status_code(), StatusCode::OK);
        // ETag rides on every items page, not just the first.
        assert!(
            r.headers().get(header::ETAG).is_some(),
            "ETag on page {pages}"
        );
        let body: serde_json::Value = r.json();
        for v in body["items"].as_array().unwrap() {
            let id: Uuid = v["manifestation_id"].as_str().unwrap().parse().unwrap();
            assert!(!seen.contains(&id), "duplicate item {id} on page {pages}");
            // Every page serialises items, not just the first.
            test_support::assert_rfc3339(v, "added_at");
            seen.push(id);
        }
        pages += 1;
        assert!(pages < 10, "runaway pagination");
        match body["next_cursor"].as_str() {
            Some(nc) => url = format!("/api/v1/shelves/{shelf_id}?cursor={nc}"),
            None => break,
        }
    }
    assert_eq!(pages, 2, "3 items at page_size=2 must take exactly 2 pages");
    seen.sort();
    assert_eq!(seen, expected, "every item exactly once");
}

#[sqlx::test(migrations = "./migrations")]
async fn list_shelves_id_tiebreaker_under_identical_names(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (a_id, a_basic) =
        test_support::db::create_adult_and_basic_auth(&app_pool, "dup-names").await;
    // Two shelves with the SAME name: only the id column in the keyset
    // predicate separates them. page_size = 1 forces the boundary
    // between the duplicates.
    let s1 = test_support::db::create_shelf(&app_pool, a_id, "Duplicate").await;
    let s2 = test_support::db::create_shelf(&app_pool, a_id, "Duplicate").await;
    let mut expected = [s1, s2];
    expected.sort();

    let server = test_support::db::server_with_real_pools_page_size(&app_pool, &ingestion_pool, 1);

    let mut seen: Vec<Uuid> = Vec::new();
    let mut url = "/api/v1/shelves".to_string();
    let mut pages = 0u32;
    loop {
        let r = server
            .get(&url)
            .add_header(auth(&a_basic).0.clone(), auth(&a_basic).1.clone())
            .await;
        assert_eq!(r.status_code(), StatusCode::OK);
        let body: serde_json::Value = r.json();
        for v in body["items"].as_array().unwrap() {
            let id: Uuid = v["id"].as_str().unwrap().parse().unwrap();
            assert!(!seen.contains(&id), "duplicate shelf {id} on page {pages}");
            seen.push(id);
        }
        pages += 1;
        assert!(pages < 10, "runaway pagination");
        match body["next_cursor"].as_str() {
            Some(nc) => url = format!("/api/v1/shelves?cursor={nc}"),
            None => break,
        }
    }
    seen.sort();
    assert_eq!(
        seen, expected,
        "both same-name shelves exactly once over {pages} pages"
    );
}

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

#[sqlx::test(migrations = "./migrations")]
async fn shelf_items_rejects_malformed_cursor(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (a_id, a_basic) =
        test_support::db::create_adult_and_basic_auth(&app_pool, "items-bad-cursor").await;
    let shelf_id = test_support::db::create_shelf(&app_pool, a_id, "Cursor shelf").await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let r = server
        .get(&format!("/api/v1/shelves/{shelf_id}?cursor=!!!garbage!!!"))
        .add_header(auth(&a_basic).0, auth(&a_basic).1)
        .await;
    assert_eq!(r.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test(migrations = "./migrations")]
async fn list_shelves_duplicate_query_key_returns_400(pool: PgPool) {
    // `?cursor=a&cursor=b` rejects at the axum_extra::Query extractor
    // (serde_html_form errors on a repeated scalar key) and must surface as
    // RFC 9457 problem+json, not axum's plaintext 400 (clears debt 2026-06-10).
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_a_id, a_basic) =
        test_support::db::create_adult_and_basic_auth(&app_pool, "dup-list").await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let r = server
        .get("/api/v1/shelves?cursor=a&cursor=b")
        .add_header(auth(&a_basic).0, auth(&a_basic).1)
        .await;
    test_support::assert_problem(
        &r,
        crate::error::problems::MALFORMED_QUERY,
        StatusCode::BAD_REQUEST,
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn shelf_items_duplicate_query_key_returns_400(pool: PgPool) {
    // The Query rejection fires as the first line of the handler body, before
    // the shelf lookup, so a duplicate `?cursor=a&cursor=b` yields 400
    // problem+json (clears debt 2026-06-10).
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (a_id, a_basic) =
        test_support::db::create_adult_and_basic_auth(&app_pool, "dup-items").await;
    let shelf_id = test_support::db::create_shelf(&app_pool, a_id, "Dup shelf").await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let r = server
        .get(&format!("/api/v1/shelves/{shelf_id}?cursor=a&cursor=b"))
        .add_header(auth(&a_basic).0, auth(&a_basic).1)
        .await;
    test_support::assert_problem(
        &r,
        crate::error::problems::MALFORMED_QUERY,
        StatusCode::BAD_REQUEST,
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn list_shelves_empty_cursor_returns_first_page_not_422(pool: PgPool) {
    // Parser-swap parity (serde_urlencoded → serde_html_form): `?cursor=`
    // (empty) decoded to Some("") under serde_urlencoded → 422 malformed
    // cursor; under serde_html_form it decodes to None, so the handler returns
    // the first page. Assert the success path post-swap.
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_a_id, a_basic) =
        test_support::db::create_adult_and_basic_auth(&app_pool, "empty-cursor").await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let r = server
        .get("/api/v1/shelves?cursor=")
        .add_header(auth(&a_basic).0, auth(&a_basic).1)
        .await;
    assert_eq!(r.status_code(), StatusCode::OK);
}

#[sqlx::test(migrations = "./migrations")]
async fn shelf_items_empty_cursor_returns_first_page_not_422(pool: PgPool) {
    // Same parser-swap parity as list_shelves, for the sibling
    // get_shelf_with_items handler (ShelfItemsParams.cursor is the same
    // Option<String>). `?cursor=` decodes to None under serde_html_form → first
    // page. Needs a real shelf: the cursor decode runs after the shelf lookup,
    // not before (unlike the duplicate-key rejection, which fires at the
    // extractor).
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (a_id, a_basic) =
        test_support::db::create_adult_and_basic_auth(&app_pool, "empty-items-cursor").await;
    let shelf_id = test_support::db::create_shelf(&app_pool, a_id, "Empty items shelf").await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let r = server
        .get(&format!("/api/v1/shelves/{shelf_id}?cursor="))
        .add_header(auth(&a_basic).0, auth(&a_basic).1)
        .await;
    assert_eq!(r.status_code(), StatusCode::OK);
}
