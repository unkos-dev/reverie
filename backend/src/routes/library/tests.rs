//! Integration tests for the `/api/v1/books*` and `/api/v1/works/{id}`
//! endpoints (11a Tasks 3, 5, 7).
//!
//! Mirrors [`crate::routes::opds::tests`] — `#[sqlx::test]` per case,
//! real-pool harness via [`crate::test_support::db::server_with_real_pools`].

use axum::http::{StatusCode, header::AUTHORIZATION};
use axum_test::TestServer;
use base64ct::{Base64UrlUnpadded, Encoding};
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::problems;
use crate::test_support;

/// Build a real-pool test server with a custom OPDS `page_size`.
/// Mirrors [`test_support::db::server_with_real_pools`] but overrides
/// the page-size knob so pagination-overflow tests can drive small
/// pages without inserting a hundred fixture rows.
fn server_with_page_size(app_pool: &PgPool, ingestion_pool: &PgPool, page_size: u32) -> TestServer {
    use crate::config::OpdsConfig;
    use crate::state::AppState;

    let mut config = test_support::test_config();
    config.opds = OpdsConfig {
        enabled: false,
        page_size,
        realm: "Reverie OPDS".into(),
        public_url: Some(url::Url::parse("http://localhost:3000").unwrap()),
    };
    let state = AppState {
        pool: app_pool.clone(),
        ingestion_pool: ingestion_pool.clone(),
        config,
        oidc: Some(std::sync::Arc::new(test_support::test_oidc_runtime())),
        jwt_validator: None,
        login_limiter: test_support::test_login_limiter(),
        settings: test_support::test_settings(),
        last_settings_reload: std::sync::Arc::new(tokio::sync::RwLock::new(None)),
    };
    TestServer::new(crate::build_router(state))
}

/// Insert a work + matching author (returns the manifestation id) so
/// `sort=author` tests have a non-NULL first-author boundary to
/// pivot on. Authors get `sort_name = name` for predictable ordering.
async fn insert_book_with_author(
    ingestion_pool: &PgPool,
    marker: &str,
    title: &str,
    author_name: &str,
) -> Uuid {
    let (work_id, m_id) = insert_book(ingestion_pool, marker, title).await;
    let author_id: Uuid = sqlx::query_scalar!(
        "INSERT INTO authors (name, sort_name) VALUES ($1, $1) RETURNING id",
        author_name,
    )
    .fetch_one(ingestion_pool)
    .await
    .expect("insert author");
    sqlx::query!(
        "INSERT INTO work_authors (work_id, author_id, position) VALUES ($1, $2, 0)",
        work_id,
        author_id,
    )
    .execute(ingestion_pool)
    .await
    .expect("insert work_authors");
    // Real writers (upgrade_stub, PATCH contributors) refresh this
    // denormalized column as part of the same transaction; this
    // hand-rolled fixture must do the same or sort=author reads NULL.
    let mut conn = ingestion_pool.acquire().await.expect("acquire conn");
    crate::models::work::refresh_first_author_sort(&mut conn, work_id)
        .await
        .expect("refresh first_author_sort_name");
    m_id
}

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
                 'complete'::ingestion_status, 'clean'::validation_status) \
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

/// Insert a `(work, manifestation)` pair with an explicit `created_at`,
/// for sort-walk fixtures that need deterministic timestamp ordering
/// (the column default is `now()`, which cannot express intentional
/// ties or cross-row ordering within one test). Mirrors
/// [`insert_book`]'s query shape with `created_at` added as a bound
/// column.
async fn insert_book_at(
    ingestion_pool: &PgPool,
    marker: &str,
    title: &str,
    created_at: chrono::DateTime<chrono::Utc>,
) -> (Uuid, Uuid) {
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
             file_size_bytes, ingestion_status, validation_status, created_at) \
         VALUES ($1, 'epub'::manifestation_format, $2, $3, $3, 1000, \
                 'complete'::ingestion_status, 'clean'::validation_status, $4) \
         RETURNING id",
        work_id,
        file_path,
        hash,
        created_at,
    )
    .fetch_one(ingestion_pool)
    .await
    .expect("insert manifestation");
    (work_id, m_id)
}

/// Like [`insert_book_with_author`], but with an explicit `sort_name`
/// (distinct from the author's display `name`, so two different
/// authors can collide on `first_author_sort_name` without violating
/// `authors_name_unique`) and an explicit `created_at` for
/// deterministic multi-level ordering.
async fn insert_book_with_author_sort_name(
    ingestion_pool: &PgPool,
    marker: &str,
    title: &str,
    author_name: &str,
    sort_name: &str,
    created_at: chrono::DateTime<chrono::Utc>,
) -> Uuid {
    let (work_id, m_id) = insert_book_at(ingestion_pool, marker, title, created_at).await;
    let author_id: Uuid = sqlx::query_scalar!(
        "INSERT INTO authors (name, sort_name) VALUES ($1, $2) RETURNING id",
        author_name,
        sort_name,
    )
    .fetch_one(ingestion_pool)
    .await
    .expect("insert author");
    sqlx::query!(
        "INSERT INTO work_authors (work_id, author_id, position) VALUES ($1, $2, 0)",
        work_id,
        author_id,
    )
    .execute(ingestion_pool)
    .await
    .expect("insert work_authors");
    let mut conn = ingestion_pool.acquire().await.expect("acquire conn");
    crate::models::work::refresh_first_author_sort(&mut conn, work_id)
        .await
        .expect("refresh first_author_sort_name");
    m_id
}

/// Set a manifestation's `pages` for sort-walk fixtures needing a
/// non-NULL boundary value.
async fn set_pages(ingestion_pool: &PgPool, m_id: Uuid, pages: i32) {
    sqlx::query!(
        "UPDATE manifestations SET pages = $1 WHERE id = $2",
        pages,
        m_id,
    )
    .execute(ingestion_pool)
    .await
    .expect("set pages");
}

/// Deterministic `created_at` boundary value for walk tests: seconds
/// past a fixed epoch, so relative ordering across fixture rows is
/// exact rather than depending on wall-clock insert timing.
fn ts(offset_seconds: i64) -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::from_timestamp(1_700_000_000 + offset_seconds, 0)
        .expect("valid unix timestamp")
}

/// Walk every page from `first_url` via the `next_cursor` chain,
/// collecting each item's manifestation id in return order. Shared by
/// the multi-level sort-walk tests below; caps at 20 pages as a
/// runaway-pagination guard.
async fn walk_all_ids(server: &TestServer, basic: &str, first_url: &str) -> Vec<Uuid> {
    let sep = if first_url.contains('?') { "&" } else { "?" };
    let mut seen = Vec::new();
    let mut url = first_url.to_owned();
    let mut walked = 0u32;
    loop {
        let response = server
            .get(&url)
            .add_header(AUTHORIZATION, basic.to_owned())
            .await;
        assert_eq!(
            response.status_code(),
            StatusCode::OK,
            "page {walked} at {url}"
        );
        let body: serde_json::Value = response.json();
        for item in body["items"].as_array().unwrap() {
            let id: Uuid = item["id"].as_str().unwrap().parse().expect("uuid id");
            seen.push(id);
        }
        walked += 1;
        assert!(walked < 20, "runaway pagination: seen = {seen:?}");
        match body["next_cursor"].as_str() {
            Some(nc) => url = format!("{first_url}{sep}cursor={nc}"),
            None => break,
        }
    }
    seen
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
        .get("/api/v1/books")
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
        format!("/api/v1/books/{first_id}/cover/thumb"),
    );
    // Value-assert (not just is_string): the list path is a runtime QueryBuilder
    // that decodes validation_status via PgRow::get into the ValidationStatus
    // enum — not macro-checked, so this is the only guard on that decode path.
    assert_eq!(items[0]["validation_status"].as_str().unwrap(), "clean");
    assert!(items[0]["ingestion_status"].is_string());
    assert!(items[0]["enrichment_status"].is_string());
    // created_at is RFC 3339 on the wire; it sources the "Added" sort column
    // and the frontend BookListItemSchema expects z.string().
    let created_at = items[0]["created_at"]
        .as_str()
        .unwrap_or_else(|| panic!("created_at must be a JSON string, got {}", items[0]));
    chrono::DateTime::parse_from_rfc3339(created_at)
        .unwrap_or_else(|e| panic!("created_at must parse as RFC 3339 ({e}): {created_at}"));
}

#[sqlx::test(migrations = "./migrations")]
async fn list_endpoint_decodes_pending_validation_status(pool: PgPool) {
    // `pending` is the column default — a row ingested but not yet validated.
    // Every other test seeds `clean`, so this is the only guard that the
    // QueryBuilder PgRow::get decode handles the default variant too.
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;

    let work_id: Uuid = sqlx::query_scalar!(
        "INSERT INTO works (title, sort_title) VALUES ($1, $1) RETURNING id",
        "Unvalidated Tome"
    )
    .fetch_one(&ingestion_pool)
    .await
    .expect("insert work");
    sqlx::query!(
        "INSERT INTO manifestations \
            (work_id, format, file_path, ingestion_file_hash, current_file_hash, \
             file_size_bytes, ingestion_status, validation_status) \
         VALUES ($1, 'epub'::manifestation_format, '/tmp/pending.epub', 'pending-hash', \
                 'pending-hash', 1000, 'complete'::ingestion_status, 'pending'::validation_status)",
        work_id,
    )
    .execute(&ingestion_pool)
    .await
    .expect("insert pending manifestation");

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let response = server
        .get("/api/v1/books")
        .add_header(AUTHORIZATION, basic)
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let body: serde_json::Value = response.json();
    let items = body["items"].as_array().expect("items array");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["validation_status"].as_str().unwrap(), "pending");
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
        .get("/api/v1/books")
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
        .get("/api/v1/books")
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

    let response = server.get("/api/v1/books").await;
    test_support::assert_problem(&response, problems::UNAUTHORIZED, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "./migrations")]
async fn list_endpoint_malformed_cursor_returns_422(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

    let response = server
        .get("/api/v1/books?cursor=!!!not-base64url!!!")
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
        .get("/api/v1/books?sort=title")
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
        .get("/api/v1/books")
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
async fn list_endpoint_series_position_matches_stored_value(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;

    let (work_id, _m_id) = insert_book(&ingestion_pool, "list-series-pos", "Volume Seven").await;
    let series_id: Uuid = sqlx::query_scalar!(
        "INSERT INTO series (name, sort_name) VALUES ('Long Saga', 'Long Saga') RETURNING id",
    )
    .fetch_one(&ingestion_pool)
    .await
    .expect("insert series");
    sqlx::query!(
        "INSERT INTO series_works (series_id, work_id, position) VALUES ($1, $2, $3::float8)",
        series_id,
        work_id,
        7.0_f64,
    )
    .execute(&ingestion_pool)
    .await
    .expect("insert series_works");

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let response = server
        .get("/api/v1/books")
        .add_header(AUTHORIZATION, basic)
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let body: serde_json::Value = response.json();
    let items = body["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    // The list path fails differently from the detail endpoint: its dynamic
    // QueryBuilder rows go through series_ref_from_row, where a column type
    // mismatch surfaces as a decode error rather than garbage bytes. That
    // error was once discarded into a silent null; this pins the ordinal
    // actually reaching the API payload.
    let position = items[0]["series"]["position"].as_f64().unwrap();
    assert!(
        (position - 7.0).abs() < 1e-9,
        "series.position must equal the stored ordinal, got {body}",
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn list_endpoint_cross_sort_cursor_rejected(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

    // Mint a cursor under the default (-created_at) stack, then try to
    // replay it under sort=title.
    let default_spec = crate::routes::sort_spec::SortSpec::default();
    let recent = crate::routes::cursor::SortCursor {
        spec: default_spec.canonical(),
        keys: vec![crate::routes::cursor::CursorValue::Ts(chrono::Utc::now())],
        id: Uuid::new_v4(),
        // Inert here: the sort mismatch is detected before the fingerprint.
        filter_fp: String::new(),
    }
    .encode()
    .expect("encode");

    let response = server
        .get(&format!("/api/v1/books?sort=title&cursor={recent}"))
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

#[sqlx::test(migrations = "./migrations")]
async fn list_endpoint_sort_author_orders_by_first_author(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;

    // Mix of authored works + one stub (no author) — verifies the
    // NULLS LAST projection and the non-NULL ordering on the same
    // page.
    insert_book_with_author(&ingestion_pool, "g", "Neuromancer", "Gibson, William").await;
    insert_book_with_author(&ingestion_pool, "a", "Anathem", "Stephenson, Neal").await;
    insert_book_with_author(&ingestion_pool, "p", "Persuasion", "Austen, Jane").await;
    insert_book(&ingestion_pool, "stub", "Unattributed Stub").await;

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let response = server
        .get("/api/v1/books?sort=author")
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
        vec![
            "Persuasion",        // Austen, Jane
            "Neuromancer",       // Gibson, William
            "Anathem",           // Stephenson, Neal
            "Unattributed Stub"  // NULLS LAST
        ]
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn list_endpoint_sort_author_pagination_walk_across_null_boundary(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;

    // Two non-NULL + four NULL-author works. With page_size = 2 the
    // page boundary lands inside the NULL bucket. Naive `(NULL, _) >
    // ('', _)` predicate would drop NULL-bucket rows from page 2;
    // regression guard for adversarial-review finding C1.
    insert_book_with_author(&ingestion_pool, "g", "Neuromancer", "Gibson, William").await;
    insert_book_with_author(&ingestion_pool, "a", "Anathem", "Stephenson, Neal").await;
    for (m, t) in [
        ("stub-1", "Stub Alpha"),
        ("stub-2", "Stub Bravo"),
        ("stub-3", "Stub Charlie"),
        ("stub-4", "Stub Delta"),
    ] {
        insert_book(&ingestion_pool, m, t).await;
    }

    let server = server_with_page_size(&app_pool, &ingestion_pool, 2);

    let mut seen: Vec<String> = Vec::new();
    let mut url = "/api/v1/books?sort=author".to_string();
    let mut walked = 0u32;
    loop {
        let response = server
            .get(&url)
            .add_header(AUTHORIZATION, basic.clone())
            .await;
        assert_eq!(
            response.status_code(),
            StatusCode::OK,
            "page {walked} at {url}"
        );
        let body: serde_json::Value = response.json();
        for item in body["items"].as_array().unwrap() {
            seen.push(item["title"].as_str().unwrap().to_owned());
        }
        walked += 1;
        assert!(walked < 10, "runaway pagination: seen = {seen:?}");
        match body["next_cursor"].as_str() {
            Some(nc) => {
                url = format!("/api/v1/books?sort=author&cursor={nc}");
            }
            None => break,
        }
    }

    // First two: non-NULL authors ordered by sort_name ASC.
    assert_eq!(&seen[..2], &["Neuromancer", "Anathem"]);
    // Tail (4 NULL-author stubs) ordered by w.id ASC (gen_random_uuid
    // produces unpredictable order — assert by set, not sequence).
    let tail: std::collections::HashSet<&str> = seen[2..].iter().map(String::as_str).collect();
    let expected_tail: std::collections::HashSet<&str> =
        ["Stub Alpha", "Stub Bravo", "Stub Charlie", "Stub Delta"]
            .into_iter()
            .collect();
    assert_eq!(
        tail, expected_tail,
        "every NULL-bucket row must surface across the cursor walk — regression guard against C1 \
         (NULL boundary truncation under naive tuple comparison)"
    );
    assert_eq!(seen.len(), 6, "no duplicates across the walk");
}

#[sqlx::test(migrations = "./migrations")]
async fn list_endpoint_sort_author_translator_only_work_sorts_into_null_bucket(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;

    // first_author_sort_name is populated from role = 'author' only, so
    // a work with a translator but no author must land in the NULLS
    // LAST bucket alongside genuinely authorless stubs, not sort by
    // the translator's (alphabetically-earlier) name.
    insert_book_with_author(&ingestion_pool, "g", "Neuromancer", "Gibson, William").await;
    let (translator_work_id, _translator_m_id) =
        insert_book(&ingestion_pool, "t", "Translated Only").await;
    test_support::db::insert_contributor(
        &ingestion_pool,
        translator_work_id,
        "Aardvark, Ann",
        "translator",
        0,
    )
    .await;

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let response = server
        .get("/api/v1/books?sort=author")
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
        vec!["Neuromancer", "Translated Only"],
        "translator-only work has no role='author' row, so first_author_sort_name is NULL and \
         it must sort after Gibson despite 'Aardvark' being alphabetically first"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn list_endpoint_pagination_walk_emits_link_and_flips_to_null(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;

    // page_size = 2, three rows → page 1 carries Link rel=next +
    // body next_cursor; page 2 carries the third row alone, no Link
    // header, next_cursor flipped to null.
    insert_book(&ingestion_pool, "p1", "Page One Alpha").await;
    insert_book(&ingestion_pool, "p2", "Page One Bravo").await;
    insert_book(&ingestion_pool, "p3", "Page Two Charlie").await;

    let server = server_with_page_size(&app_pool, &ingestion_pool, 2);

    let response = server
        .get("/api/v1/books")
        .add_header(AUTHORIZATION, basic.clone())
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let link = response
        .headers()
        .get(axum::http::header::LINK)
        .expect("RFC 8288 Link header on overflow page")
        .to_str()
        .unwrap()
        .to_owned();
    assert!(link.contains(r#"rel="next""#), "Link header: {link}");
    let body: serde_json::Value = response.json();
    assert_eq!(body["items"].as_array().unwrap().len(), 2);
    let nc = body["next_cursor"]
        .as_str()
        .expect("next_cursor present on overflow")
        .to_owned();
    let next_url = format!("/api/v1/books?cursor={nc}");

    let response = server.get(&next_url).add_header(AUTHORIZATION, basic).await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(body["items"].as_array().unwrap().len(), 1);
    assert!(
        body["next_cursor"].is_null(),
        "final page next_cursor must be null, got {body}"
    );
    assert!(
        response.headers().get(axum::http::header::LINK).is_none(),
        "final page must not carry a Link rel=next header"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn list_endpoint_sort_title_pagination_walk(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;

    // Distinct vocabulary keeps the pg_trgm find_or_create dedup
    // threshold from collapsing these into a single work.
    for (m, t) in [
        ("alpha", "Alpha Centauri"),
        ("bravo", "Bravo Charlie"),
        ("delta", "Delta Echo"),
        ("foxtrot", "Foxtrot Golf"),
        ("hotel", "Hotel India"),
    ] {
        insert_book(&ingestion_pool, m, t).await;
    }

    let server = server_with_page_size(&app_pool, &ingestion_pool, 2);

    let mut seen: Vec<String> = Vec::new();
    let mut url = "/api/v1/books?sort=title".to_string();
    let mut walked = 0u32;
    loop {
        let response = server
            .get(&url)
            .add_header(AUTHORIZATION, basic.clone())
            .await;
        assert_eq!(
            response.status_code(),
            StatusCode::OK,
            "page {walked} at {url}"
        );
        let body: serde_json::Value = response.json();
        for item in body["items"].as_array().unwrap() {
            seen.push(item["title"].as_str().unwrap().to_owned());
        }
        walked += 1;
        assert!(walked < 10, "runaway pagination: seen = {seen:?}");
        match body["next_cursor"].as_str() {
            Some(nc) => url = format!("/api/v1/books?sort=title&cursor={nc}"),
            None => break,
        }
    }

    assert_eq!(
        seen,
        vec![
            "Alpha Centauri",
            "Bravo Charlie",
            "Delta Echo",
            "Foxtrot Golf",
            "Hotel India",
        ],
        "title-sort cursor must walk all rows in lexicographic order without duplicates or drops"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn list_endpoint_sort_title_multi_manifestation_per_work_not_dropped(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;

    // Two works, each with two manifestations sharing the same
    // `(sort_title, work_id)` sort key. Without an `m.id` tiebreaker
    // in ORDER BY + cursor, the page boundary between the two
    // siblings would silently drop the second from page N+1.
    // Regression guard for Greptile finding P1 on PR #308.
    for (work_title, marker_pdf, marker_epub) in [
        ("Lima Mike", "lima-pdf", "lima-epub"),
        ("November Oscar", "november-pdf", "november-epub"),
    ] {
        // Two distinct manifestations sharing the same work-id via
        // shared title — but we want them to share the SAME work
        // row, not two stub-deduped works. Easiest path: insert the
        // work once, then insert two manifestations referencing it.
        let work_id: Uuid = sqlx::query_scalar!(
            "INSERT INTO works (title, sort_title) VALUES ($1, $1) RETURNING id",
            work_title,
        )
        .fetch_one(&ingestion_pool)
        .await
        .expect("insert work");
        for marker in [marker_pdf, marker_epub] {
            let file_path = format!("/tmp/multimani-{marker}.epub");
            let hash = format!("multimani-hash-{marker}");
            sqlx::query!(
                "INSERT INTO manifestations \
                    (work_id, format, file_path, ingestion_file_hash, current_file_hash, \
                     file_size_bytes, ingestion_status, validation_status) \
                 VALUES ($1, 'epub'::manifestation_format, $2, $3, $3, 1000, \
                         'complete'::ingestion_status, 'clean'::validation_status)",
                work_id,
                file_path,
                hash,
            )
            .execute(&ingestion_pool)
            .await
            .expect("insert manifestation");
        }
    }

    // page_size = 1 forces a boundary between every sibling pair.
    let server = server_with_page_size(&app_pool, &ingestion_pool, 1);
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut url = "/api/v1/books?sort=title".to_string();
    let mut walked = 0u32;
    loop {
        let response = server
            .get(&url)
            .add_header(AUTHORIZATION, basic.clone())
            .await;
        assert_eq!(response.status_code(), StatusCode::OK, "page {walked}");
        let body: serde_json::Value = response.json();
        for item in body["items"].as_array().unwrap() {
            let id = item["id"].as_str().unwrap().to_owned();
            assert!(
                seen.insert(id.clone()),
                "duplicate manifestation id {id} across cursor walk"
            );
        }
        walked += 1;
        assert!(
            walked < 10,
            "runaway pagination on multi-manifestation walk"
        );
        match body["next_cursor"].as_str() {
            Some(nc) => url = format!("/api/v1/books?sort=title&cursor={nc}"),
            None => break,
        }
    }
    assert_eq!(
        seen.len(),
        4,
        "all 4 manifestations across 2 works must surface exactly once — regression guard \
         against `(sort_title, work_id)` tuple comparison dropping sibling manifestations"
    );
}

// ─── multi-column sort stack: boundary walks + error paths ──────────────

#[sqlx::test(migrations = "./migrations")]
async fn default_sort_unchanged(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;

    let mut ids = Vec::new();
    for (marker, title, offset) in [
        ("orbit", "Orbital Decay", 300),
        ("comet", "Comet Tail", 200),
        ("nebula", "Nebula Drift", 100),
    ] {
        let (_w, m_id) = insert_book_at(&ingestion_pool, marker, title, ts(offset)).await;
        ids.push(m_id);
    }

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let default_seen = walk_all_ids(&server, &basic, "/api/v1/books").await;
    let explicit_seen = walk_all_ids(&server, &basic, "/api/v1/books?sort=-created_at").await;

    assert_eq!(
        default_seen, explicit_seen,
        "an omitted ?sort= must walk identically to the explicit default stack"
    );
    assert_eq!(
        default_seen, ids,
        "default stack is -created_at: newest manifestation first"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn two_level_mixed_direction_walk(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;

    // Three works share the title "Mistwood Tandem"; page_size=2 lands
    // a page boundary inside that tie group, so the walk must fall
    // through to the created_at DESC tiebreak rather than dropping or
    // duplicating a sibling.
    let mut expected_order = Vec::new();
    for (marker, title) in [("amber", "Amber Grove"), ("brindle", "Brindle Hollow")] {
        let (_w, m_id) = insert_book(&ingestion_pool, marker, title).await;
        expected_order.push(m_id);
    }
    for (marker, offset) in [("mist-1", 300), ("mist-2", 200), ("mist-3", 100)] {
        let (_w, m_id) =
            insert_book_at(&ingestion_pool, marker, "Mistwood Tandem", ts(offset)).await;
        expected_order.push(m_id);
    }
    for (marker, title) in [("willow", "Willow Path"), ("zenith", "Zenith Point")] {
        let (_w, m_id) = insert_book(&ingestion_pool, marker, title).await;
        expected_order.push(m_id);
    }

    let server = server_with_page_size(&app_pool, &ingestion_pool, 2);
    let seen = walk_all_ids(&server, &basic, "/api/v1/books?sort=title,-created_at").await;

    assert_eq!(
        seen, expected_order,
        "title ASC, then created_at DESC, must walk in this exact order with no drops or dupes"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn three_level_walk(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;

    // Five works share the author sort name "Bramblewood, Ida" (via
    // distinct display names, since `authors_name_unique` forbids
    // reusing a display name); two of them additionally tie on
    // created_at, so the walk must fall through all three levels
    // (author, then created_at DESC, then title ASC) before the id
    // tiebreaker is ever needed.
    let mut expected_order = Vec::new();
    for (marker, title, author_name, offset) in [
        ("falcon", "Falcon Point", "Bramblewood One", 500),
        ("ember", "Ember Falls", "Bramblewood Two", 400),
        ("copper", "Copper Vale", "Bramblewood Three", 300),
        ("amber3", "Amber Isle", "Bramblewood Four", 200),
        ("birch", "Birch Hollow", "Bramblewood Five", 200),
    ] {
        let m_id = insert_book_with_author_sort_name(
            &ingestion_pool,
            marker,
            title,
            author_name,
            "Bramblewood, Ida",
            ts(offset),
        )
        .await;
        expected_order.push(m_id);
    }
    let harbor_id = insert_book_with_author_sort_name(
        &ingestion_pool,
        "harbor",
        "Harbor Light",
        "Thistledown Original",
        "Thistledown, Wren",
        ts(100),
    )
    .await;
    expected_order.push(harbor_id);

    let server = server_with_page_size(&app_pool, &ingestion_pool, 2);
    let seen = walk_all_ids(
        &server,
        &basic,
        "/api/v1/books?sort=author,-created_at,title",
    )
    .await;

    assert_eq!(
        seen, expected_order,
        "author ASC, then created_at DESC, then title ASC must resolve every tie with no \
         drops or dupes"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn null_bucket_author_boundary(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;

    // Descending primary sort with a NULL bucket: exercises the DESC
    // side of the nullable-column advance clause, which the existing
    // ASC-only author walk test does not cover.
    let voss_id =
        insert_book_with_author(&ingestion_pool, "voss", "Voss Chronicle", "Voss, Elena").await;
    let larkin_id =
        insert_book_with_author(&ingestion_pool, "larkin", "Larkin Verses", "Larkin, Theo").await;
    let mut stub_ids: std::collections::HashSet<Uuid> = std::collections::HashSet::new();
    for (marker, title) in [
        ("stub-x", "Stub Xenon"),
        ("stub-y", "Stub Yttrium"),
        ("stub-z", "Stub Zirconium"),
    ] {
        let (_w, m_id) = insert_book(&ingestion_pool, marker, title).await;
        stub_ids.insert(m_id);
    }

    let server = server_with_page_size(&app_pool, &ingestion_pool, 2);
    let seen = walk_all_ids(&server, &basic, "/api/v1/books?sort=-author").await;

    assert_eq!(seen.len(), 5, "no duplicates or drops across the walk");
    assert_eq!(
        &seen[..2],
        &[voss_id, larkin_id],
        "non-NULL authors sort DESC by sort_name before the NULL bucket"
    );
    let tail: std::collections::HashSet<Uuid> = seen[2..].iter().copied().collect();
    assert_eq!(
        tail, stub_ids,
        "every NULL-bucket row must surface exactly once after a DESC primary sort"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn null_bucket_pages_desc(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;

    // DESC + nullable: the direction where a naive advance clause
    // (`pages < boundary` with no `OR pages IS NULL`) would silently
    // drop the entire NULL bucket the first time a non-NULL boundary
    // is crossed. NULLS LAST puts NULL rows at the tail in either
    // direction, so this must behave the same as the ASC case.
    let (_w1, heavy_id) = insert_book(&ingestion_pool, "heavy", "Page Heavy Tome").await;
    set_pages(&ingestion_pool, heavy_id, 800).await;
    let (_w2, medium_id) = insert_book(&ingestion_pool, "medium", "Page Medium Tome").await;
    set_pages(&ingestion_pool, medium_id, 400).await;
    let mut slim_ids: std::collections::HashSet<Uuid> = std::collections::HashSet::new();
    for (marker, title) in [
        ("slim-a", "Slim Alpha"),
        ("slim-b", "Slim Bravo"),
        ("slim-c", "Slim Charlie"),
    ] {
        let (_w, m_id) = insert_book(&ingestion_pool, marker, title).await;
        slim_ids.insert(m_id);
    }

    let server = server_with_page_size(&app_pool, &ingestion_pool, 2);
    let seen = walk_all_ids(&server, &basic, "/api/v1/books?sort=-pages").await;

    assert_eq!(seen.len(), 5, "no duplicates or drops across the walk");
    assert_eq!(
        &seen[..2],
        &[heavy_id, medium_id],
        "non-NULL pages sort DESC before the NULL bucket"
    );
    let tail: std::collections::HashSet<Uuid> = seen[2..].iter().copied().collect();
    assert_eq!(
        tail, slim_ids,
        "every NULL-pages row must surface exactly once after a DESC primary sort"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn null_bucket_author_at_non_primary_level(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;

    // sort=created_at,author,title with every row tying on created_at, so the
    // walk always falls through to `author` at level 1: the nullable, non-primary
    // level. Some rows carry a NULL author, so a page boundary landing inside the
    // NULLS LAST bucket mints a cursor whose author boundary is NULL at a
    // non-primary level. push_cursor_predicate then skips that level's advance
    // branch and every later branch must render the author boundary as
    // `first_author_sort_name IS NULL` equality. The single-level null-bucket
    // tests never reach that equality arm, because their NULL level is primary
    // with no prior levels to equate and no later level to continue the walk.
    let tie = ts(100);
    let mut expected = Vec::new();
    for (marker, title, author_name, sort_name) in [
        ("named-ash", "Named Ash", "Vera Ash", "Ash, Vera"),
        ("named-cole", "Named Cole", "Ida Cole", "Cole, Ida"),
        ("named-frost", "Named Frost", "Nils Frost", "Frost, Nils"),
    ] {
        expected.push(
            insert_book_with_author_sort_name(
                &ingestion_pool,
                marker,
                title,
                author_name,
                sort_name,
                tie,
            )
            .await,
        );
    }
    // NULL-author rows (no work_authors row leaves first_author_sort_name NULL),
    // tie-broken by title ASC within the NULLS LAST bucket.
    for (marker, title) in [
        ("null-alpha", "Null Alpha"),
        ("null-bravo", "Null Bravo"),
        ("null-charlie", "Null Charlie"),
    ] {
        let (_w, m_id) = insert_book_at(&ingestion_pool, marker, title, tie).await;
        expected.push(m_id);
    }

    let server = server_with_page_size(&app_pool, &ingestion_pool, 2);
    let seen = walk_all_ids(
        &server,
        &basic,
        "/api/v1/books?sort=created_at,author,title",
    )
    .await;

    assert_eq!(
        seen, expected,
        "created_at ASC (all tied), then author ASC NULLS LAST, then title ASC: the three named \
         authors sort ahead of the NULL bucket, the NULL rows tie-break by title, and the page \
         boundary that lands inside the NULL bucket drops or duplicates nothing"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn pages_sort_walk(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;

    let mut tied_ids: std::collections::HashSet<Uuid> = std::collections::HashSet::new();
    for (marker, title) in [
        ("novella-a", "Novella Aurora"),
        ("novella-b", "Novella Borealis"),
        ("novella-c", "Novella Cascade"),
    ] {
        let (_w, m_id) = insert_book(&ingestion_pool, marker, title).await;
        set_pages(&ingestion_pool, m_id, 100).await;
        tied_ids.insert(m_id);
    }
    let (_w, longer_id) = insert_book(&ingestion_pool, "epic", "Epic Longform").await;
    set_pages(&ingestion_pool, longer_id, 200).await;

    let server = server_with_page_size(&app_pool, &ingestion_pool, 2);
    let seen = walk_all_ids(&server, &basic, "/api/v1/books?sort=pages").await;

    assert_eq!(seen.len(), 4, "no duplicates or drops across the walk");
    let head: std::collections::HashSet<Uuid> = seen[..3].iter().copied().collect();
    assert_eq!(
        head, tied_ids,
        "every 100-page row must surface exactly once; m.id tiebreak resolves the tie"
    );
    assert_eq!(
        seen[3], longer_id,
        "the untied 200-page row sorts after the tied 100-page group"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn list_filter_duplicate_sort_column_returns_400(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

    let response = server
        .get("/api/v1/books?sort=title,-title")
        .add_header(AUTHORIZATION, basic)
        .await;
    let body = test_support::assert_problem(
        &response,
        problems::MALFORMED_QUERY,
        StatusCode::BAD_REQUEST,
    );
    let detail = body["detail"].as_str().expect("detail string");
    assert!(detail.contains("invalid sort"), "got detail: {detail}");
}

#[sqlx::test(migrations = "./migrations")]
async fn list_filter_too_many_sort_levels_returns_400(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

    let response = server
        .get("/api/v1/books?sort=title,author,created_at,pages")
        .add_header(AUTHORIZATION, basic)
        .await;
    let body = test_support::assert_problem(
        &response,
        problems::MALFORMED_QUERY,
        StatusCode::BAD_REQUEST,
    );
    let detail = body["detail"].as_str().expect("detail string");
    assert!(detail.contains("invalid sort"), "got detail: {detail}");
}

#[sqlx::test(migrations = "./migrations")]
async fn list_endpoint_cursor_direction_mismatch_returns_422(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

    // Mint a cursor under `title` (ascending), then replay it under
    // `-title` (descending): same column, different direction, so the
    // canonical spec strings differ and the request must reject.
    let minted_spec = crate::routes::sort_spec::SortSpec::parse("title").expect("valid spec");
    let cursor = crate::routes::cursor::SortCursor {
        spec: minted_spec.canonical(),
        keys: vec![crate::routes::cursor::CursorValue::Text(Some(
            "neuromancer".to_owned(),
        ))],
        id: Uuid::new_v4(),
        // Sort mismatch is checked before the filter fingerprint, so this
        // value is inert for this test; empty is the no-filter fingerprint's
        // preimage regardless.
        filter_fp: String::new(),
    }
    .encode()
    .expect("encode");

    let response = server
        .get(&format!("/api/v1/books?sort=-title&cursor={cursor}"))
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

#[sqlx::test(migrations = "./migrations")]
async fn list_endpoint_legacy_cursor_tag_returns_422(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

    // Hand-encode a pre-v2 `r`-tagged (pipe-delimited Recent) payload.
    // `SortCursor::parse_for` only recognizes tag `m`, so a stale
    // bookmark carrying the old cursor shape must reject as an unknown
    // tag rather than attempt to decode it.
    let legacy = Base64UrlUnpadded::encode_string(
        format!("r|2026-05-22T09:30:00Z|{}", Uuid::new_v4().as_hyphenated()).as_bytes(),
    );

    let response = server
        .get(&format!("/api/v1/books?cursor={legacy}"))
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

// ---------------------------------------------------------------------------
// detail_endpoint — GET /api/v1/books/{id}
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "./migrations")]
async fn detail_endpoint_returns_book_with_version_summary(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;

    let m_id =
        insert_book_with_author(&ingestion_pool, "detail", "Detailed Tome", "Doe, Jane").await;

    // Seed a pending version + a rejected version + a canonical pointer
    // (title_version_id on the work) so metadata_version_summary has
    // non-trivial counts on both sides.
    let work_id: Uuid =
        sqlx::query_scalar!("SELECT work_id FROM manifestations WHERE id = $1", m_id)
            .fetch_one(&ingestion_pool)
            .await
            .unwrap();
    let canonical_v: Uuid = sqlx::query_scalar!(
        "INSERT INTO metadata_versions \
            (manifestation_id, source, field_name, new_value, value_hash, match_type, status, \
             confidence_score) \
         VALUES ($1, 'opf', 'title', '\"Detailed Tome\"'::jsonb, '\\x01'::bytea, 'title', \
                 'pending'::metadata_review_status, 0.9) \
         RETURNING id",
        m_id,
    )
    .fetch_one(&ingestion_pool)
    .await
    .expect("insert canonical version");
    sqlx::query!(
        "INSERT INTO metadata_versions \
            (manifestation_id, source, field_name, new_value, value_hash, match_type, status, \
             confidence_score) \
         VALUES ($1, 'opf', 'description', '\"draft prose\"'::jsonb, '\\x02'::bytea, 'title', \
                 'pending'::metadata_review_status, 0.8)",
        m_id,
    )
    .execute(&ingestion_pool)
    .await
    .expect("insert pending version");
    sqlx::query!(
        "INSERT INTO metadata_versions \
            (manifestation_id, source, field_name, new_value, value_hash, match_type, status, \
             confidence_score) \
         VALUES ($1, 'opf', 'publisher', '\"Bad Pub\"'::jsonb, '\\x03'::bytea, 'title', \
                 'rejected'::metadata_review_status, 0.4)",
        m_id,
    )
    .execute(&ingestion_pool)
    .await
    .expect("insert rejected version");
    sqlx::query!(
        "UPDATE works SET title_version_id = $1 WHERE id = $2",
        canonical_v,
        work_id,
    )
    .execute(&ingestion_pool)
    .await
    .expect("link canonical");

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let response = server
        .get(&format!("/api/v1/books/{m_id}"))
        .add_header(AUTHORIZATION, basic)
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(body["id"].as_str().unwrap(), m_id.to_string());
    assert_eq!(body["title"].as_str().unwrap(), "Detailed Tome");
    assert_eq!(
        body["authors"].as_array().unwrap()[0].as_str().unwrap(),
        "Doe, Jane",
    );
    assert_eq!(
        body["cover_url"].as_str().unwrap(),
        format!("/api/v1/books/{m_id}/cover/thumb"),
    );
    assert!(body["ingestion_status"].is_string());
    assert!(body["enrichment_status"].is_string());
    assert_eq!(body["validation_status"].as_str().unwrap(), "clean");
    let summary = &body["metadata_version_summary"];
    assert_eq!(
        summary["pending"].as_u64().unwrap(),
        1,
        "canonical title version excluded from pending (only description draft remains) — got {body}",
    );
    assert_eq!(
        summary["accepted"].as_u64().unwrap(),
        1,
        "one canonical pointer set (works.title_version_id) — got {body}",
    );
    // Wire-shape regression guard: optional fields must still surface
    // (null or empty array) so an accidental `skip_serializing_if`
    // breaking the frontend `BookDetail` contract trips the test.
    assert!(
        body.get("tags").is_some_and(serde_json::Value::is_array),
        "tags must surface as an array (empty when no tags), got {body}",
    );
    assert!(
        body.get("genres").is_some_and(serde_json::Value::is_array),
        "genres must surface as an array (empty when no genres), got {body}",
    );
    assert!(
        body.get("moods").is_some_and(serde_json::Value::is_array),
        "moods must surface as an array (empty when no moods), got {body}",
    );
    assert!(
        body.get("content_rating")
            .is_some_and(serde_json::Value::is_null),
        "content_rating must surface as null when unrated, got {body}",
    );
    assert!(
        body.get("series").is_some_and(serde_json::Value::is_null),
        "series must surface as null when the work isn't on a series, got {body}",
    );
    assert!(
        body.get("isbn_10").is_some_and(serde_json::Value::is_null),
        "isbn_10 must surface as null when unset, got {body}",
    );
    assert!(
        body.get("description").is_some(),
        "description must always be present on the wire (null or string), got {body}",
    );
    assert!(
        body.get("language").is_some(),
        "language must always be present on the wire (null or string), got {body}",
    );
    // 11d carry-over from 11c: `publisher` + `pub_date` surface on
    // `BookDetail` so the frontend `EditMetadataDialog` can confirm
    // clears for both fields (`canonicalEditableFields`). Null is the
    // contractual shape for an unset value.
    assert!(
        body.get("publisher").is_some(),
        "publisher must surface on BookDetail (null when unset), got {body}",
    );
    assert!(
        body.get("pub_date").is_some(),
        "pub_date must surface on BookDetail (null when unset), got {body}",
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn detail_and_list_endpoints_exclude_editor_from_authors(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;

    let m_id = insert_book_with_author(&ingestion_pool, "ed", "Edited Tome", "Doe, Jane").await;
    let work_id: Uuid =
        sqlx::query_scalar!("SELECT work_id FROM manifestations WHERE id = $1", m_id)
            .fetch_one(&ingestion_pool)
            .await
            .unwrap();
    test_support::db::insert_contributor(&ingestion_pool, work_id, "Roe, Pat", "editor", 1).await;

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

    let list_response = server
        .get("/api/v1/books")
        .add_header(AUTHORIZATION, basic.clone())
        .await;
    assert_eq!(list_response.status_code(), StatusCode::OK);
    let list_body: serde_json::Value = list_response.json();
    let list_authors: Vec<&str> = list_body["items"][0]["authors"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a.as_str().unwrap())
        .collect();
    assert_eq!(
        list_authors,
        vec!["Doe, Jane"],
        "editor role must not surface in BookListRow.authors — got {list_body}"
    );

    let detail_response = server
        .get(&format!("/api/v1/books/{m_id}"))
        .add_header(AUTHORIZATION, basic)
        .await;
    assert_eq!(detail_response.status_code(), StatusCode::OK);
    let detail_body: serde_json::Value = detail_response.json();
    let detail_authors: Vec<&str> = detail_body["authors"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a.as_str().unwrap())
        .collect();
    assert_eq!(
        detail_authors,
        vec!["Doe, Jane"],
        "editor role must not surface in BookDetail.authors — got {detail_body}"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn detail_and_list_endpoints_surface_contributors_with_roles(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;

    let m_id =
        insert_book_with_author(&ingestion_pool, "ctr", "Contributed Tome", "Doe, Jane").await;
    let work_id: Uuid =
        sqlx::query_scalar!("SELECT work_id FROM manifestations WHERE id = $1", m_id)
            .fetch_one(&ingestion_pool)
            .await
            .unwrap();
    test_support::db::insert_contributor(&ingestion_pool, work_id, "Voz, Kim", "narrator", 1).await;
    test_support::db::insert_contributor(&ingestion_pool, work_id, "Roe, Pat", "editor", 1).await;
    test_support::db::insert_contributor(&ingestion_pool, work_id, "Tran, Sam", "translator", 1)
        .await;

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

    // Role order follows the author_role enum declaration (editor,
    // translator, narrator), position within a role — insertion order
    // above is deliberately scrambled to prove the ordering is server-side.
    let expected = serde_json::json!([
        {"name": "Roe, Pat", "role": "editor"},
        {"name": "Tran, Sam", "role": "translator"},
        {"name": "Voz, Kim", "role": "narrator"},
    ]);

    let list_response = server
        .get("/api/v1/books")
        .add_header(AUTHORIZATION, basic.clone())
        .await;
    assert_eq!(list_response.status_code(), StatusCode::OK);
    let list_body: serde_json::Value = list_response.json();
    assert_eq!(
        list_body["items"][0]["contributors"], expected,
        "non-author roles must surface as contributors on list rows — got {list_body}"
    );
    assert_eq!(
        list_body["items"][0]["authors"],
        serde_json::json!(["Doe, Jane"]),
        "authors display array must stay author-role only — got {list_body}"
    );

    let detail_response = server
        .get(&format!("/api/v1/books/{m_id}"))
        .add_header(AUTHORIZATION, basic)
        .await;
    assert_eq!(detail_response.status_code(), StatusCode::OK);
    let detail_body: serde_json::Value = detail_response.json();
    assert_eq!(
        detail_body["contributors"], expected,
        "non-author roles must surface as contributors on detail — got {detail_body}"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn detail_and_list_endpoints_surface_subtitle_and_pages(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;

    let m_id = insert_book_with_author(&ingestion_pool, "sp", "Subtitled Tome", "Doe, Jane").await;
    let work_id: Uuid =
        sqlx::query_scalar!("SELECT work_id FROM manifestations WHERE id = $1", m_id)
            .fetch_one(&ingestion_pool)
            .await
            .unwrap();
    sqlx::query!(
        "UPDATE works SET subtitle = $1 WHERE id = $2",
        "A Tale of Two Fixtures",
        work_id,
    )
    .execute(&ingestion_pool)
    .await
    .expect("set subtitle");
    sqlx::query!(
        "UPDATE manifestations SET pages = $1 WHERE id = $2",
        321,
        m_id,
    )
    .execute(&ingestion_pool)
    .await
    .expect("set pages");

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

    let list_response = server
        .get("/api/v1/books")
        .add_header(AUTHORIZATION, basic.clone())
        .await;
    assert_eq!(list_response.status_code(), StatusCode::OK);
    let list_body: serde_json::Value = list_response.json();
    assert_eq!(
        list_body["items"][0]["subtitle"].as_str().unwrap(),
        "A Tale of Two Fixtures",
        "got {list_body}"
    );
    assert_eq!(
        list_body["items"][0]["pages"].as_i64().unwrap(),
        321,
        "got {list_body}"
    );

    let detail_response = server
        .get(&format!("/api/v1/books/{m_id}"))
        .add_header(AUTHORIZATION, basic)
        .await;
    assert_eq!(detail_response.status_code(), StatusCode::OK);
    let detail_body: serde_json::Value = detail_response.json();
    assert_eq!(
        detail_body["subtitle"].as_str().unwrap(),
        "A Tale of Two Fixtures",
        "got {detail_body}"
    );
    assert_eq!(
        detail_body["pages"].as_i64().unwrap(),
        321,
        "got {detail_body}"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn detail_endpoint_caps_pending_versions_at_200(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;

    let m_id =
        insert_book_with_author(&ingestion_pool, "capped", "Crowded Tome", "Doe, Jane").await;

    // Seed 250 distinct pending versions for one manifestation — the
    // bulk-enrichment / repeated-edit scenario that produces an
    // unbounded result set (debt/2026-05-26-load-pending-versions-unbounded).
    // Distinct value_hash satisfies metadata_versions_mfs_hash_unique;
    // descending last_seen_at makes the freshest rows deterministic.
    sqlx::query!(
        "INSERT INTO metadata_versions \
            (manifestation_id, source, field_name, new_value, value_hash, match_type, \
             status, confidence_score, last_seen_at) \
         SELECT $1, 'opf', 'description', \
                to_jsonb('draft ' || g.n), \
                decode(lpad(to_hex(g.n), 8, '0'), 'hex'), \
                'title', 'pending'::metadata_review_status, 0.5, \
                now() - (g.n || ' seconds')::interval \
         FROM generate_series(1, 250) AS g(n)",
        m_id,
    )
    .execute(&ingestion_pool)
    .await
    .expect("seed 250 pending versions");

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let response = server
        .get(&format!("/api/v1/books/{m_id}"))
        .add_header(AUTHORIZATION, basic)
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let body: serde_json::Value = response.json();
    let cap = u64::try_from(super::MAX_PENDING_VERSIONS).unwrap();
    assert_eq!(
        body["metadata_version_summary"]["pending"]
            .as_u64()
            .unwrap(),
        cap,
        "pending versions must be capped at {cap} to bound the result set, got {body}",
    );

    // Ordering contract: `last_seen_at DESC LIMIT` keeps the freshest
    // `cap` drafts. Seed n=1 is the freshest (now()-1s), n=250 the
    // stalest (now()-250s), so drafts 1..=200 survive and 201..=250 are
    // dropped. Without this, deleting `ORDER BY last_seen_at DESC` from
    // the query leaves the count at 200 and the test green — the cut
    // direction would be untested.
    let versions = body["metadata_versions"].as_array().unwrap();
    assert_eq!(versions.len(), usize::try_from(cap).unwrap());
    let drafts: Vec<&str> = versions
        .iter()
        .filter_map(|v| v["new_value"].as_str())
        .collect();
    assert!(
        drafts.contains(&"draft 1"),
        "freshest draft (n=1) must survive the cut, got {drafts:?}",
    );
    assert!(
        !drafts.contains(&"draft 250"),
        "stalest draft (n=250) must be dropped by the cut, got {drafts:?}",
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn detail_endpoint_surfaces_publisher_and_pub_date(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;

    let m_id =
        insert_book_with_author(&ingestion_pool, "pubmeta", "With Publisher", "Doe, Jane").await;
    sqlx::query!(
        "UPDATE manifestations SET publisher = 'Tor', pub_date = DATE '2024-01-15' WHERE id = $1",
        m_id,
    )
    .execute(&ingestion_pool)
    .await
    .expect("populate publisher + pub_date");

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let response = server
        .get(&format!("/api/v1/books/{m_id}"))
        .add_header(AUTHORIZATION, basic)
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(body["publisher"].as_str().unwrap(), "Tor");
    // ISO 8601 calendar date (YYYY-MM-DD) — matches the frontend
    // `BookDetailSchema.pub_date` shape.
    assert_eq!(body["pub_date"].as_str().unwrap(), "2024-01-15");
}

#[sqlx::test(migrations = "./migrations")]
async fn detail_endpoint_timestamps_are_rfc3339_strings(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let m_id = insert_book_with_author(&ingestion_pool, "ts", "Timestamped", "Doe, Jane").await;

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let response = server
        .get(&format!("/api/v1/books/{m_id}"))
        .add_header(AUTHORIZATION, basic)
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let body: serde_json::Value = response.json();
    // Both must reach the wire as RFC 3339 strings; the frontend
    // BookDetailSchema rejects any other shape.
    for field in ["created_at", "updated_at"] {
        let raw = body[field]
            .as_str()
            .unwrap_or_else(|| panic!("{field} must be a JSON string, got {}", body[field]));
        chrono::DateTime::parse_from_rfc3339(raw)
            .unwrap_or_else(|e| panic!("{field} must parse as RFC 3339 ({e}): {raw}"));
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn detail_endpoint_series_position_matches_stored_value(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;

    let (work_id, m_id) = insert_book(&ingestion_pool, "series-pos", "Volume Eighty-Five").await;
    let series_id: Uuid = sqlx::query_scalar!(
        "INSERT INTO series (name, sort_name) VALUES ('Long Saga', 'Long Saga') RETURNING id",
    )
    .fetch_one(&ingestion_pool)
    .await
    .expect("insert series");
    // Bind through float8 on write, matching the real enrichment writer
    // (models::work::upgrade_stub) so this test stores the ordinal the way
    // production data arrives.
    sqlx::query!(
        "INSERT INTO series_works (series_id, work_id, position) VALUES ($1, $2, $3::float8)",
        series_id,
        work_id,
        85.0_f64,
    )
    .execute(&ingestion_pool)
    .await
    .expect("insert series_works");

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let response = server
        .get(&format!("/api/v1/books/{m_id}"))
        .add_header(AUTHORIZATION, basic)
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let body: serde_json::Value = response.json();
    // Regression guard: when this column was NUMERIC, a macro read without
    // a SQL-level `::float8` cast decoded the NUMERIC wire bytes as if they
    // were an IEEE-754 float8, producing a denormal garbage value. The
    // column is double precision now; this stays as the end-to-end pin that
    // the stored ordinal survives to the API.
    let position = body["series"]["position"].as_f64().unwrap();
    assert!(
        (position - 85.0).abs() < 1e-9,
        "series.position must equal the stored ordinal, got {body}",
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn detail_endpoint_hidden_id_returns_404(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_child, basic) =
        test_support::db::create_child_user_and_basic_auth(&app_pool, "hidden").await;

    // Insert a book the child cannot see (not on any shelf of theirs).
    let (_w, m_id) = insert_book(&ingestion_pool, "hidden", "Forbidden Tome").await;

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let response = server
        .get(&format!("/api/v1/books/{m_id}"))
        .add_header(AUTHORIZATION, basic)
        .await;
    // RLS hides the row → 404, NOT 403 (existence-not-leaked).
    test_support::assert_problem(&response, problems::NOT_FOUND, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = "./migrations")]
async fn detail_endpoint_malformed_uuid_returns_400(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

    let response = server
        .get("/api/v1/books/not-a-uuid")
        .add_header(AUTHORIZATION, basic)
        .await;
    // axum 0.8 default `Path<Uuid>` rejection: 400 plain-text body.
    assert_eq!(response.status_code(), StatusCode::BAD_REQUEST);
}

// ---------------------------------------------------------------------------
// work_endpoint — GET /api/v1/works/{id}
// ---------------------------------------------------------------------------

#[sqlx::test(migrations = "./migrations")]
async fn work_endpoint_returns_work_with_manifestations(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;

    // One work with two manifestations (epub + pdf-shape sibling). Author
    // attached to verify the join surfaces in the response.
    let work_id: Uuid = sqlx::query_scalar!(
        "INSERT INTO works (title, sort_title, description, language) \
         VALUES ('Compendium', 'Compendium', 'A long prose summary.', 'en') \
         RETURNING id",
    )
    .fetch_one(&ingestion_pool)
    .await
    .expect("insert work");
    let author_id: Uuid = sqlx::query_scalar!(
        "INSERT INTO authors (name, sort_name) VALUES ('Roe, Avery', 'Roe, Avery') RETURNING id",
    )
    .fetch_one(&ingestion_pool)
    .await
    .expect("insert author");
    sqlx::query!(
        "INSERT INTO work_authors (work_id, author_id, position) VALUES ($1, $2, 0)",
        work_id,
        author_id,
    )
    .execute(&ingestion_pool)
    .await
    .unwrap();
    let mut insertion_order: Vec<Uuid> = Vec::new();
    for marker in ["epub-vol", "pdf-vol"] {
        let file_path = format!("/tmp/work-test-{marker}.epub");
        let hash = format!("work-test-hash-{marker}");
        let mid: Uuid = sqlx::query_scalar!(
            "INSERT INTO manifestations \
                (work_id, format, file_path, ingestion_file_hash, current_file_hash, \
                 file_size_bytes, ingestion_status, validation_status) \
             VALUES ($1, 'epub'::manifestation_format, $2, $3, $3, 1000, \
                     'complete'::ingestion_status, 'clean'::validation_status) \
             RETURNING id",
            work_id,
            file_path,
            hash,
        )
        .fetch_one(&ingestion_pool)
        .await
        .unwrap();
        insertion_order.push(mid);
    }

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let response = server
        .get(&format!("/api/v1/works/{work_id}"))
        .add_header(AUTHORIZATION, basic)
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(body["id"].as_str().unwrap(), work_id.to_string());
    assert_eq!(body["title"].as_str().unwrap(), "Compendium");
    assert_eq!(
        body["description"].as_str().unwrap(),
        "A long prose summary.",
    );
    assert_eq!(body["language"].as_str().unwrap(), "en");
    assert_eq!(
        body["authors"].as_array().unwrap()[0].as_str().unwrap(),
        "Roe, Avery",
    );
    let manifestations = body["manifestations"].as_array().unwrap();
    assert_eq!(manifestations.len(), 2);
    // Handler emits ORDER BY created_at ASC, id ASC — must match insertion
    // order so a regression to an unordered query is caught.
    let returned_ids: Vec<String> = manifestations
        .iter()
        .map(|m| m["id"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(
        returned_ids,
        insertion_order
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        "manifestations must surface in (created_at ASC, id ASC) order",
    );
    for m in manifestations {
        let mid = m["id"].as_str().unwrap();
        assert_eq!(
            m["cover_url"].as_str().unwrap(),
            format!("/api/v1/books/{mid}/cover/thumb"),
        );
        assert!(m["ingestion_status"].is_string());
        // Must reach the wire as an RFC 3339 string; the frontend
        // WorkManifestationSchema rejects any other shape.
        let raw = m["created_at"]
            .as_str()
            .unwrap_or_else(|| panic!("created_at must be a JSON string, got {}", m["created_at"]));
        chrono::DateTime::parse_from_rfc3339(raw)
            .unwrap_or_else(|e| panic!("created_at must parse as RFC 3339 ({e}): {raw}"));
    }
    assert!(
        body["series"].is_null(),
        "no series seeded → series must surface null, got {body}",
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn series_ref_from_row_fails_loud_on_decode_mismatch(pool: PgPool) {
    // Pins the loud-but-handled contract directly at the row boundary: a
    // column type drift must surface as an error, not degrade to a silent
    // None. Swallowing this Err is what hid the original NUMERIC mismatch.
    let mistyped_series_row_sql = "SELECT 'not-a-uuid'::text AS series_id, \
         'Long Saga'::text AS series_name, \
         'seven'::text AS series_position";
    let row = sqlx::query(mistyped_series_row_sql)
        .fetch_one(&pool)
        .await
        .expect("fetch mistyped row");
    assert!(
        super::series_ref_from_row(&row).is_err(),
        "a series column decode mismatch must propagate as an error",
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn work_endpoint_series_position_matches_stored_value(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;

    let (work_id, _m_id) = insert_book(&ingestion_pool, "work-series-pos", "Volume One").await;
    let series_id: Uuid = sqlx::query_scalar!(
        "INSERT INTO series (name, sort_name) VALUES ('Long Saga', 'Long Saga') RETURNING id",
    )
    .fetch_one(&ingestion_pool)
    .await
    .expect("insert series");
    sqlx::query!(
        "INSERT INTO series_works (series_id, work_id, position) VALUES ($1, $2, $3::float8)",
        series_id,
        work_id,
        1.0_f64,
    )
    .execute(&ingestion_pool)
    .await
    .expect("insert series_works");

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let response = server
        .get(&format!("/api/v1/works/{work_id}"))
        .add_header(AUTHORIZATION, basic)
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let body: serde_json::Value = response.json();
    let position = body["series"]["position"].as_f64().unwrap();
    assert!(
        (position - 1.0).abs() < 1e-9,
        "series.position must equal the stored ordinal, got {body}",
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn work_endpoint_malformed_uuid_returns_400(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

    let response = server
        .get("/api/v1/works/not-a-uuid")
        .add_header(AUTHORIZATION, basic)
        .await;
    // axum 0.8 default `Path<Uuid>` rejection: 400 plain-text body.
    assert_eq!(response.status_code(), StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = "./migrations")]
async fn work_endpoint_hidden_work_returns_404(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

    // Nonexistent UUID → 404.
    let response = server
        .get(&format!("/api/v1/works/{}", Uuid::new_v4()))
        .add_header(AUTHORIZATION, basic)
        .await;
    test_support::assert_problem(&response, problems::NOT_FOUND, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = "./migrations")]
async fn work_endpoint_child_without_shelf_returns_404_not_empty_array(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_child, basic) =
        test_support::db::create_child_user_and_basic_auth(&app_pool, "shelved").await;

    // Work exists, manifestation exists, but the child has not shelved
    // it — the manifestation is RLS-hidden, so the work must surface as
    // 404 (existence-not-leaked) rather than 200 with `manifestations: []`.
    let (work_id, _m_id) = insert_book(&ingestion_pool, "child-hidden", "Adult Material").await;

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let response = server
        .get(&format!("/api/v1/works/{work_id}"))
        .add_header(AUTHORIZATION, basic)
        .await;
    test_support::assert_problem(&response, problems::NOT_FOUND, StatusCode::NOT_FOUND);
}

// ─── 11b — list filters ──────────────────────────────────────────────────

/// Insert a tag and link it to a manifestation via `manifestation_tags`.
async fn tag_book(ingestion_pool: &PgPool, manifestation_id: Uuid, tag_name: &str) {
    let tag_id: Uuid = sqlx::query_scalar!(
        "INSERT INTO tags (name) VALUES ($1) \
         ON CONFLICT ((lower(name))) DO UPDATE SET name = EXCLUDED.name \
         RETURNING id",
        tag_name,
    )
    .fetch_one(ingestion_pool)
    .await
    .expect("insert tag");
    sqlx::query!(
        "INSERT INTO manifestation_tags (manifestation_id, tag_id) \
         VALUES ($1, $2) ON CONFLICT DO NOTHING",
        manifestation_id,
        tag_id,
    )
    .execute(ingestion_pool)
    .await
    .expect("insert manifestation_tags");
}

/// Insert a genre and link it to a manifestation via `manifestation_genres`.
async fn genre_book(ingestion_pool: &PgPool, manifestation_id: Uuid, genre_name: &str) {
    let genre_id: Uuid = sqlx::query_scalar!(
        "INSERT INTO genres (name) VALUES ($1) \
         ON CONFLICT ((lower(name))) DO UPDATE SET name = EXCLUDED.name \
         RETURNING id",
        genre_name,
    )
    .fetch_one(ingestion_pool)
    .await
    .expect("insert genre");
    sqlx::query!(
        "INSERT INTO manifestation_genres (manifestation_id, genre_id) \
         VALUES ($1, $2) ON CONFLICT DO NOTHING",
        manifestation_id,
        genre_id,
    )
    .execute(ingestion_pool)
    .await
    .expect("insert manifestation_genres");
}

/// Insert a mood and link it to a manifestation via `manifestation_moods`.
async fn mood_book(ingestion_pool: &PgPool, manifestation_id: Uuid, mood_name: &str) {
    let mood_id: Uuid = sqlx::query_scalar!(
        "INSERT INTO moods (name) VALUES ($1) \
         ON CONFLICT ((lower(name))) DO UPDATE SET name = EXCLUDED.name \
         RETURNING id",
        mood_name,
    )
    .fetch_one(ingestion_pool)
    .await
    .expect("insert mood");
    sqlx::query!(
        "INSERT INTO manifestation_moods (manifestation_id, mood_id) \
         VALUES ($1, $2) ON CONFLICT DO NOTHING",
        manifestation_id,
        mood_id,
    )
    .execute(ingestion_pool)
    .await
    .expect("insert manifestation_moods");
}

#[sqlx::test(migrations = "./migrations")]
async fn list_filter_by_author(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;

    insert_book_with_author(&ingestion_pool, "g", "Neuromancer", "Gibson, William").await;
    insert_book_with_author(&ingestion_pool, "a", "Anathem", "Stephenson, Neal").await;

    let author_id: Uuid =
        sqlx::query_scalar!("SELECT id AS \"id!\" FROM authors WHERE name = 'Gibson, William'")
            .fetch_one(&ingestion_pool)
            .await
            .expect("author id");

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let response = server
        .get(&format!("/api/v1/books?author={author_id}"))
        .add_header(AUTHORIZATION, basic)
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let body: serde_json::Value = response.json();
    let items = body["items"].as_array().expect("items");
    assert_eq!(items.len(), 1, "only Gibson's book should match");
    assert_eq!(items[0]["title"], "Neuromancer");
}

#[sqlx::test(migrations = "./migrations")]
async fn list_filter_by_author_excludes_non_author_roles(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;

    let (work_id, _m_id) = insert_book(&ingestion_pool, "ea", "Edited Anthology").await;
    let editor_id = test_support::db::insert_contributor(
        &ingestion_pool,
        work_id,
        "Compiler Quill Osgood",
        "editor",
        0,
    )
    .await;

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let response = server
        .get(&format!("/api/v1/books?author={editor_id}"))
        .add_header(AUTHORIZATION, basic)
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let body: serde_json::Value = response.json();
    let items = body["items"].as_array().expect("items");
    assert!(
        items.is_empty(),
        "an editor-role link must not satisfy the ?author= filter"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn list_filter_by_series(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;

    let (work_a, _m_a) = insert_book(&ingestion_pool, "a", "Vol 1").await;
    let (work_b, _m_b) = insert_book(&ingestion_pool, "b", "Vol 2").await;
    insert_book(&ingestion_pool, "c", "Standalone").await;

    let series_id: Uuid = sqlx::query_scalar!(
        "INSERT INTO series (name, sort_name) VALUES ('Test Series', 'Test Series') RETURNING id"
    )
    .fetch_one(&ingestion_pool)
    .await
    .expect("series");
    for (w, pos) in [(work_a, 1.0_f64), (work_b, 2.0)] {
        sqlx::query!(
            "INSERT INTO series_works (series_id, work_id, position) VALUES ($1, $2, $3::float8)",
            series_id,
            w,
            pos,
        )
        .execute(&ingestion_pool)
        .await
        .expect("series_works");
    }

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let response = server
        .get(&format!("/api/v1/books?series={series_id}"))
        .add_header(AUTHORIZATION, basic)
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let body: serde_json::Value = response.json();
    let items = body["items"].as_array().expect("items");
    assert_eq!(items.len(), 2, "only series volumes — not the standalone");
}

#[sqlx::test(migrations = "./migrations")]
async fn list_filter_by_shelf_scoped_to_caller(pool: PgPool) {
    // Cross-user shelf probe: caller queries another user's shelf id —
    // the filter must yield zero rows, not leak shelf contents.
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let (other_id, _other_basic) =
        test_support::db::create_adult_and_basic_auth(&app_pool, "other-adult").await;

    let (_w1, m1) = insert_book(&ingestion_pool, "a", "Caller Book").await;
    let (_w2, m2) = insert_book(&ingestion_pool, "b", "Other Book").await;

    let my_shelf = test_support::db::create_shelf(&app_pool, admin_id, "mine").await;
    let other_shelf = test_support::db::create_shelf(&app_pool, other_id, "theirs").await;
    test_support::db::add_to_shelf(&app_pool, my_shelf, m1).await;
    test_support::db::add_to_shelf(&app_pool, other_shelf, m2).await;

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

    let r = server
        .get(&format!("/api/v1/books?shelf={my_shelf}"))
        .add_header(AUTHORIZATION, basic.clone())
        .await;
    assert_eq!(r.status_code(), StatusCode::OK);
    let items = r.json::<serde_json::Value>()["items"]
        .as_array()
        .cloned()
        .expect("items");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["title"], "Caller Book");

    let r = server
        .get(&format!("/api/v1/books?shelf={other_shelf}"))
        .add_header(AUTHORIZATION, basic)
        .await;
    assert_eq!(r.status_code(), StatusCode::OK);
    let items = r.json::<serde_json::Value>()["items"]
        .as_array()
        .cloned()
        .expect("items");
    assert!(
        items.is_empty(),
        "must not expose another user's shelf members"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn list_filter_malformed_author_uuid_returns_400(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

    let response = server
        .get("/api/v1/books?author=garbage")
        .add_header(AUTHORIZATION, basic)
        .await;
    let body = test_support::assert_problem(
        &response,
        problems::MALFORMED_QUERY,
        StatusCode::BAD_REQUEST,
    );
    let detail = body["detail"].as_str().expect("detail string");
    assert!(
        detail.contains("malformed query parameter"),
        "got detail: {detail}"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn list_filter_malformed_series_uuid_returns_400(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

    let response = server
        .get("/api/v1/books?series=garbage")
        .add_header(AUTHORIZATION, basic)
        .await;
    let body = test_support::assert_problem(
        &response,
        problems::MALFORMED_QUERY,
        StatusCode::BAD_REQUEST,
    );
    let detail = body["detail"].as_str().expect("detail string");
    assert!(
        detail.contains("malformed query parameter"),
        "got detail: {detail}"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn list_filter_malformed_shelf_uuid_returns_400(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

    let response = server
        .get("/api/v1/books?shelf=garbage")
        .add_header(AUTHORIZATION, basic)
        .await;
    let body = test_support::assert_problem(
        &response,
        problems::MALFORMED_QUERY,
        StatusCode::BAD_REQUEST,
    );
    let detail = body["detail"].as_str().expect("detail string");
    assert!(
        detail.contains("malformed query parameter"),
        "got detail: {detail}"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn list_filter_malformed_sort_returns_400(pool: PgPool) {
    // `?sort=` is validated in-handler against the `SortColumn`
    // whitelist (`SortSpec::parse`), a structurally distinct decode
    // path from the `Option<Uuid>` filter params: a field outside the
    // whitelist must still surface as RFC 9457 400, not a silent
    // default-sort fallthrough.
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

    let response = server
        .get("/api/v1/books?sort=isbn_13")
        .add_header(AUTHORIZATION, basic)
        .await;
    let body = test_support::assert_problem(
        &response,
        problems::MALFORMED_QUERY,
        StatusCode::BAD_REQUEST,
    );
    let detail = body["detail"].as_str().expect("detail string");
    assert!(detail.contains("invalid sort"), "got detail: {detail}");
}

#[sqlx::test(migrations = "./migrations")]
async fn list_filter_too_many_tags_returns_422(pool: PgPool) {
    // Cap is MAX_TAG_FILTERS=20; 21 tag params must surface as a
    // validation problem rather than executing a pathologically large
    // COUNT subquery.
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let qs: String = (0..21)
        .map(|i| format!("tag=t{i}"))
        .collect::<Vec<_>>()
        .join("&");
    let r = server
        .get(&format!("/api/v1/books?{qs}"))
        .add_header(AUTHORIZATION, basic)
        .await;
    test_support::assert_problem(&r, problems::VALIDATION, StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test(migrations = "./migrations")]
async fn list_filter_multi_tag_and_match(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;

    let (_w1, m1) = insert_book(&ingestion_pool, "a", "Both Tags").await;
    let (_w2, m2) = insert_book(&ingestion_pool, "b", "Scifi Only").await;
    let (_w3, _m3) = insert_book(&ingestion_pool, "c", "Untagged").await;

    tag_book(&ingestion_pool, m1, "scifi").await;
    tag_book(&ingestion_pool, m1, "hugo").await;
    tag_book(&ingestion_pool, m2, "scifi").await;

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let r = server
        .get("/api/v1/books?tag=scifi&tag=hugo")
        .add_header(AUTHORIZATION, basic)
        .await;
    assert_eq!(r.status_code(), StatusCode::OK);
    let items = r.json::<serde_json::Value>()["items"]
        .as_array()
        .cloned()
        .expect("items");
    assert_eq!(items.len(), 1, "AND-match — only the book with BOTH tags");
    assert_eq!(items[0]["title"], "Both Tags");
}

#[sqlx::test(migrations = "./migrations")]
async fn list_filter_tag_any_or_match(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;

    let (_w1, m1) = insert_book(&ingestion_pool, "ta", "Orchid Files").await;
    let (_w2, m2) = insert_book(&ingestion_pool, "tb", "Corduroy Notes").await;
    insert_book(&ingestion_pool, "tc", "Silent Reams").await;

    tag_book(&ingestion_pool, m1, "spearmint").await;
    tag_book(&ingestion_pool, m2, "juniper").await;

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let r = server
        .get("/api/v1/books?tag_any=spearmint&tag_any=juniper")
        .add_header(AUTHORIZATION, basic)
        .await;
    assert_eq!(r.status_code(), StatusCode::OK);
    let items = r.json::<serde_json::Value>()["items"]
        .as_array()
        .cloned()
        .expect("items");
    assert_eq!(items.len(), 2, "any-of matches either tag, got {items:?}");
}

#[sqlx::test(migrations = "./migrations")]
async fn list_filter_tag_none_excludes_match(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;

    let (_w1, m1) = insert_book(&ingestion_pool, "na", "Harbor Sketch").await;
    let (_w2, m2) = insert_book(&ingestion_pool, "nb", "Ember Recital").await;
    insert_book(&ingestion_pool, "nc", "Fresh Quire").await;

    tag_book(&ingestion_pool, m1, "walrus").await;
    tag_book(&ingestion_pool, m2, "obsidian").await;

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let r = server
        .get("/api/v1/books?tag_none=walrus")
        .add_header(AUTHORIZATION, basic)
        .await;
    assert_eq!(r.status_code(), StatusCode::OK);
    let items = r.json::<serde_json::Value>()["items"]
        .as_array()
        .cloned()
        .expect("items");
    let titles: Vec<&str> = items.iter().filter_map(|i| i["title"].as_str()).collect();
    assert_eq!(
        items.len(),
        2,
        "none-of keeps untagged rows, got {titles:?}"
    );
    assert!(
        !titles.contains(&"Harbor Sketch"),
        "the excluded tag's book must not surface, got {titles:?}"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn list_filter_multi_genre_and_match(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;

    let (_w1, m1) = insert_book(&ingestion_pool, "ga", "Quasar Handbook").await;
    let (_w2, m2) = insert_book(&ingestion_pool, "gb", "Lathe Primer").await;
    insert_book(&ingestion_pool, "gc", "Plain Volume").await;

    genre_book(&ingestion_pool, m1, "Astrophysics").await;
    genre_book(&ingestion_pool, m1, "Woodworking").await;
    genre_book(&ingestion_pool, m2, "Astrophysics").await;

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let r = server
        .get("/api/v1/books?genre=Astrophysics&genre=Woodworking")
        .add_header(AUTHORIZATION, basic)
        .await;
    assert_eq!(r.status_code(), StatusCode::OK);
    let items = r.json::<serde_json::Value>()["items"]
        .as_array()
        .cloned()
        .expect("items");
    assert_eq!(items.len(), 1, "AND-match — only the book with BOTH genres");
    assert_eq!(items[0]["title"], "Quasar Handbook");
}

#[sqlx::test(migrations = "./migrations")]
async fn list_filter_genre_any_or_match(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;

    let (_w1, m1) = insert_book(&ingestion_pool, "ha", "Sourdough Diary").await;
    let (_w2, m2) = insert_book(&ingestion_pool, "hb", "Atlas Fragments").await;
    insert_book(&ingestion_pool, "hc", "Empty Codex").await;

    genre_book(&ingestion_pool, m1, "Baking").await;
    genre_book(&ingestion_pool, m2, "Cartography").await;

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let r = server
        .get("/api/v1/books?genre_any=Baking&genre_any=Cartography")
        .add_header(AUTHORIZATION, basic)
        .await;
    assert_eq!(r.status_code(), StatusCode::OK);
    let items = r.json::<serde_json::Value>()["items"]
        .as_array()
        .cloned()
        .expect("items");
    assert_eq!(items.len(), 2, "any-of matches either genre, got {items:?}");
}

#[sqlx::test(migrations = "./migrations")]
async fn list_filter_genre_matches_case_insensitively(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;

    let (_w1, m1) = insert_book(&ingestion_pool, "ci", "Lantern Almanac").await;
    genre_book(&ingestion_pool, m1, "Horology").await;

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    // Vocabulary identity is lower(name): a lowercase query value must match
    // the stored display casing, same as suggest does.
    let r = server
        .get("/api/v1/books?genre=horology")
        .add_header(AUTHORIZATION, basic)
        .await;
    assert_eq!(r.status_code(), StatusCode::OK);
    let items = r.json::<serde_json::Value>()["items"]
        .as_array()
        .cloned()
        .expect("items");
    assert_eq!(items.len(), 1, "case-insensitive match, got {items:?}");
}

#[sqlx::test(migrations = "./migrations")]
async fn list_filter_genre_none_excludes_match(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;

    let (_w1, m1) = insert_book(&ingestion_pool, "ia", "Midnight Static").await;
    let (_w2, m2) = insert_book(&ingestion_pool, "ib", "Trellis Guide").await;
    insert_book(&ingestion_pool, "ic", "Blank Folio").await;

    genre_book(&ingestion_pool, m1, "Horror").await;
    genre_book(&ingestion_pool, m2, "Gardening").await;

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let r = server
        .get("/api/v1/books?genre_none=Horror")
        .add_header(AUTHORIZATION, basic)
        .await;
    assert_eq!(r.status_code(), StatusCode::OK);
    let items = r.json::<serde_json::Value>()["items"]
        .as_array()
        .cloned()
        .expect("items");
    let titles: Vec<&str> = items.iter().filter_map(|i| i["title"].as_str()).collect();
    assert_eq!(
        items.len(),
        2,
        "none-of keeps ungenred rows, got {titles:?}"
    );
    assert!(
        !titles.contains(&"Midnight Static"),
        "the excluded genre's book must not surface, got {titles:?}"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn list_filter_multi_mood_and_match(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;

    let (_w1, m1) = insert_book(&ingestion_pool, "ja", "Confetti Album").await;
    let (_w2, m2) = insert_book(&ingestion_pool, "jb", "Granite Elegy").await;
    insert_book(&ingestion_pool, "jc", "Bare Booklet").await;

    mood_book(&ingestion_pool, m1, "Whimsical").await;
    mood_book(&ingestion_pool, m1, "Somber").await;
    mood_book(&ingestion_pool, m2, "Whimsical").await;

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let r = server
        .get("/api/v1/books?mood=Whimsical&mood=Somber")
        .add_header(AUTHORIZATION, basic)
        .await;
    assert_eq!(r.status_code(), StatusCode::OK);
    let items = r.json::<serde_json::Value>()["items"]
        .as_array()
        .cloned()
        .expect("items");
    assert_eq!(items.len(), 1, "AND-match — only the book with BOTH moods");
    assert_eq!(items[0]["title"], "Confetti Album");
}

#[sqlx::test(migrations = "./migrations")]
async fn list_filter_mood_any_or_match(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;

    let (_w1, m1) = insert_book(&ingestion_pool, "ka", "Hearth Sketches").await;
    let (_w2, m2) = insert_book(&ingestion_pool, "kb", "Ticking Vault").await;
    insert_book(&ingestion_pool, "kc", "Vacant Tome").await;

    mood_book(&ingestion_pool, m1, "Cozy").await;
    mood_book(&ingestion_pool, m2, "Tense").await;

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let r = server
        .get("/api/v1/books?mood_any=Cozy&mood_any=Tense")
        .add_header(AUTHORIZATION, basic)
        .await;
    assert_eq!(r.status_code(), StatusCode::OK);
    let items = r.json::<serde_json::Value>()["items"]
        .as_array()
        .cloned()
        .expect("items");
    assert_eq!(items.len(), 2, "any-of matches either mood, got {items:?}");
}

#[sqlx::test(migrations = "./migrations")]
async fn list_filter_mood_none_excludes_match(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;

    let (_w1, m1) = insert_book(&ingestion_pool, "la", "Ashen Spiral").await;
    let (_w2, m2) = insert_book(&ingestion_pool, "lb", "Meadow Romp").await;
    insert_book(&ingestion_pool, "lc", "Untouched Quarto").await;

    mood_book(&ingestion_pool, m1, "Bleak").await;
    mood_book(&ingestion_pool, m2, "Playful").await;

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let r = server
        .get("/api/v1/books?mood_none=Bleak")
        .add_header(AUTHORIZATION, basic)
        .await;
    assert_eq!(r.status_code(), StatusCode::OK);
    let items = r.json::<serde_json::Value>()["items"]
        .as_array()
        .cloned()
        .expect("items");
    let titles: Vec<&str> = items.iter().filter_map(|i| i["title"].as_str()).collect();
    assert_eq!(
        items.len(),
        2,
        "none-of keeps unmooded rows, got {titles:?}"
    );
    assert!(
        !titles.contains(&"Ashen Spiral"),
        "the excluded mood's book must not surface, got {titles:?}"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn list_filter_combined_genre_any_mood_all_tag_none(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;

    let (_w1, m1) = insert_book(&ingestion_pool, "ca", "Cipher Meadowlark").await;
    let (_w2, m2) = insert_book(&ingestion_pool, "cb", "Smoke Register").await;
    let (_w3, m3) = insert_book(&ingestion_pool, "cc", "Quiet Jetty").await;
    let (_w4, m4) = insert_book(&ingestion_pool, "cd", "Rapid Descent").await;

    genre_book(&ingestion_pool, m1, "Espionage").await;
    mood_book(&ingestion_pool, m1, "Frantic").await;
    genre_book(&ingestion_pool, m2, "Espionage").await;
    mood_book(&ingestion_pool, m2, "Frantic").await;
    tag_book(&ingestion_pool, m2, "houndstooth").await;
    genre_book(&ingestion_pool, m3, "Espionage").await;
    mood_book(&ingestion_pool, m4, "Frantic").await;

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let r = server
        .get("/api/v1/books?genre_any=Espionage&mood=Frantic&tag_none=houndstooth")
        .add_header(AUTHORIZATION, basic)
        .await;
    assert_eq!(r.status_code(), StatusCode::OK);
    let items = r.json::<serde_json::Value>()["items"]
        .as_array()
        .cloned()
        .expect("items");
    assert_eq!(
        items.len(),
        1,
        "combined filters intersect — only the genre+mood book without the tag, got {items:?}"
    );
    assert_eq!(items[0]["title"], "Cipher Meadowlark");
}

#[sqlx::test(migrations = "./migrations")]
async fn list_filter_too_many_genre_any_returns_422(pool: PgPool) {
    // Same MAX_TAG_FILTERS=20 cap as ?tag=, enforced per param: 21
    // genre_any values must surface as a validation problem.
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let qs: String = (0..21)
        .map(|i| format!("genre_any=g{i}"))
        .collect::<Vec<_>>()
        .join("&");
    let r = server
        .get(&format!("/api/v1/books?{qs}"))
        .add_header(AUTHORIZATION, basic)
        .await;
    test_support::assert_problem(&r, problems::VALIDATION, StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test(migrations = "./migrations")]
async fn detail_endpoint_returns_genres_moods_and_content_rating(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;

    let (_work_id, m_id) = insert_book(&ingestion_pool, "vocab", "Meridian Ledger").await;
    genre_book(&ingestion_pool, m_id, "Astrophysics").await;
    genre_book(&ingestion_pool, m_id, "Woodworking").await;
    mood_book(&ingestion_pool, m_id, "Somber").await;
    sqlx::query!(
        "UPDATE manifestations SET content_rating = 'mature'::content_rating WHERE id = $1",
        m_id,
    )
    .execute(&ingestion_pool)
    .await
    .expect("set content_rating");

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let response = server
        .get(&format!("/api/v1/books/{m_id}"))
        .add_header(AUTHORIZATION, basic)
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(
        body["genres"],
        serde_json::json!(["Astrophysics", "Woodworking"]),
        "genres ordered by name, got {body}"
    );
    assert_eq!(body["moods"], serde_json::json!(["Somber"]), "got {body}");
    assert_eq!(body["content_rating"], "mature", "got {body}");
}

#[sqlx::test(migrations = "./migrations")]
async fn list_endpoint_surfaces_vocabularies_and_content_rating(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;

    let (_work_id, m_id) = insert_book(&ingestion_pool, "lv", "Aardvark Almanac").await;
    tag_book(&ingestion_pool, m_id, "signed").await;
    genre_book(&ingestion_pool, m_id, "Woodworking").await;
    genre_book(&ingestion_pool, m_id, "Astrophysics").await;
    mood_book(&ingestion_pool, m_id, "Somber").await;
    sqlx::query!(
        "UPDATE manifestations SET content_rating = 'mature'::content_rating WHERE id = $1",
        m_id,
    )
    .execute(&ingestion_pool)
    .await
    .expect("set content_rating");
    let (_bare_work, _bare_id) = insert_book(&ingestion_pool, "lv2", "Bare Binder").await;

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let response = server
        .get("/api/v1/books?sort=title")
        .add_header(AUTHORIZATION, basic)
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let body: serde_json::Value = response.json();

    let decorated = &body["items"][0];
    assert_eq!(decorated["title"], "Aardvark Almanac", "got {body}");
    assert_eq!(
        decorated["tags"],
        serde_json::json!(["signed"]),
        "got {body}"
    );
    assert_eq!(
        decorated["genres"],
        serde_json::json!(["Astrophysics", "Woodworking"]),
        "genres ordered by name, got {body}"
    );
    assert_eq!(
        decorated["moods"],
        serde_json::json!(["Somber"]),
        "got {body}"
    );
    assert_eq!(decorated["content_rating"], "mature", "got {body}");

    let bare = &body["items"][1];
    assert_eq!(bare["title"], "Bare Binder", "got {body}");
    assert_eq!(bare["tags"], serde_json::json!([]), "got {body}");
    assert_eq!(bare["genres"], serde_json::json!([]), "got {body}");
    assert_eq!(bare["moods"], serde_json::json!([]), "got {body}");
    assert!(
        bare["content_rating"].is_null()
            && bare
                .as_object()
                .is_some_and(|o| o.contains_key("content_rating")),
        "unrated book must carry an explicit null content_rating, got {body}"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn manual_vocab_patch_does_not_inflate_pending_count(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;

    let (_work_id, m_id) = insert_book(&ingestion_pool, "vocab-pending", "Basalt Compendium").await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

    let initial = server
        .get(&format!("/api/v1/books/{m_id}/metadata"))
        .add_header(AUTHORIZATION, basic.clone())
        .await;
    let etag = initial
        .headers()
        .get(axum::http::header::ETAG)
        .expect("ETag header present")
        .clone();

    let patch = server
        .patch(&format!("/api/v1/books/{m_id}/metadata"))
        .add_header(AUTHORIZATION, basic.clone())
        .add_header(axum::http::header::IF_MATCH, etag)
        .json(&serde_json::json!({
            "genres": ["Falconry"], "moods": ["Wistful"], "tags": ["Heirloom"]
        }))
        .await;
    assert_eq!(patch.status_code(), StatusCode::OK);

    // The three manual journal rows are applied (their pointers live on the
    // junction rows), so none of them may resurface as a pending draft.
    let response = server
        .get(&format!("/api/v1/books/{m_id}"))
        .add_header(AUTHORIZATION, basic)
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(body["metadata_version_summary"]["pending"], 0, "got {body}");
    // The three junction-held manual versions count as accepted: they are
    // applied metadata, just pointed at from junction rows instead of
    // scalar pointer columns.
    assert_eq!(
        body["metadata_version_summary"]["accepted"], 3,
        "got {body}"
    );
    assert_eq!(
        body["genres"],
        serde_json::json!(["Falconry"]),
        "got {body}"
    );
}

/// Walk an `EXPLAIN (FORMAT JSON)` plan tree and fail on any Seq Scan
/// node whose Relation Name is `relation`.
fn assert_no_seq_scan_on(plan: &serde_json::Value, relation: &str, label: &str) {
    match plan {
        serde_json::Value::Object(obj) => {
            if obj.get("Node Type").and_then(serde_json::Value::as_str) == Some("Seq Scan") {
                assert_ne!(
                    obj.get("Relation Name").and_then(serde_json::Value::as_str),
                    Some(relation),
                    "{label}: planner chose a Seq Scan over {relation}"
                );
            }
            for child in obj.values() {
                assert_no_seq_scan_on(child, relation, label);
            }
        }
        serde_json::Value::Array(items) => {
            for child in items {
                assert_no_seq_scan_on(child, relation, label);
            }
        }
        _ => {}
    }
}

/// Walk an `EXPLAIN (FORMAT JSON)` plan tree and report whether any node reads
/// the named index (an Index Scan / Bitmap Index Scan / Index Only Scan whose
/// `Index Name` matches). Stronger than [`assert_no_seq_scan_on`]: a
/// leading-wildcard `ILIKE` that fell back to a full index-order scan with the
/// predicate applied as a per-row Filter still avoids a Seq Scan node, so
/// proving the trigram index is the actual access path needs this.
fn plan_uses_index(plan: &serde_json::Value, index: &str) -> bool {
    match plan {
        serde_json::Value::Object(obj) => {
            obj.get("Index Name").and_then(serde_json::Value::as_str) == Some(index)
                || obj.values().any(|child| plan_uses_index(child, index))
        }
        serde_json::Value::Array(items) => items.iter().any(|child| plan_uses_index(child, index)),
        _ => false,
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn subtitle_contains_rides_trgm_index_at_scale(pool: PgPool) {
    use sqlx::Row as _;

    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;

    // 50k works, each with a distinct subtitle, so a selective substring needle
    // matches a single row: the case the subtitle trigram index must serve.
    sqlx::query!(
        "INSERT INTO works (title, sort_title, subtitle) \
         SELECT 'Bulk Opus ' || g.n, 'Bulk Opus ' || g.n, 'Bulk Subtitle ' || g.n \
         FROM generate_series(1, 50000) AS g(n)"
    )
    .execute(&ingestion_pool)
    .await
    .expect("seed works");
    sqlx::query!(
        "INSERT INTO manifestations \
            (work_id, format, file_path, ingestion_file_hash, current_file_hash, \
             file_size_bytes, ingestion_status, validation_status) \
         SELECT w.id, 'epub'::manifestation_format, '/tmp/bulk-' || w.id, \
                'bulk-hash-' || w.id, 'bulk-hash-' || w.id, 1000, \
                'complete'::ingestion_status, 'clean'::validation_status \
         FROM works w"
    )
    .execute(&ingestion_pool)
    .await
    .expect("seed manifestations");

    // CARVE-OUT (runtime-sqlx allowlist): ANALYZE is maintenance DDL the macros
    // cannot validate; fresh stats keep the planner from choosing empty-table
    // plans for the bulk seed. Runs on the owner pool (ANALYZE needs ownership).
    sqlx::query("ANALYZE works, manifestations")
        .execute(&pool)
        .await
        .expect("analyze seeded tables");

    // Literal twin of push_ilike_contains' output for subtitle_contains on the
    // recent-sort list query (default page size + 1). A selective needle so the
    // planner must reach for the trigram index over a full-scan Filter.
    let explain_subtitle_sql = "EXPLAIN (FORMAT JSON) \
        SELECT m.id FROM manifestations m \
        JOIN works w ON w.id = m.work_id \
        WHERE TRUE AND immutable_unaccent(w.subtitle) \
            ILIKE '%' || nullif(immutable_unaccent_like($1), '') || '%' \
        ORDER BY m.created_at DESC, m.id DESC LIMIT 61";

    // Discourage seq scans and nested loops for this probe inside a throwaway
    // transaction. The planner can otherwise satisfy the LIMIT by walking the
    // recent-keyset index, probing works_pkey, and applying subtitle as a
    // per-row Filter. That plan depends on a correlation estimate and does not
    // prove the trigram index can serve the joined query. The GUCs penalize
    // rather than forbid those paths, so a missing or unusable index still
    // forces a non-trigram plan and fails the guard. SET LOCAL is
    // transaction-scoped, so it cannot leak to a pooled connection reused by
    // another test.
    let mut tx = ingestion_pool.begin().await.expect("begin explain txn");
    // CARVE-OUT (runtime-sqlx allowlist): SET LOCAL is a transaction-scoped GUC
    // mutation the compile-time macros cannot validate.
    sqlx::query("SET LOCAL enable_seqscan = off")
        .execute(&mut *tx)
        .await
        .expect("disable seqscan for the probe");
    // CARVE-OUT (runtime-sqlx allowlist): SET LOCAL is a transaction-scoped GUC
    // mutation the compile-time macros cannot validate.
    sqlx::query("SET LOCAL enable_nestloop = off")
        .execute(&mut *tx)
        .await
        .expect("disable nested loops for the probe");
    // CARVE-OUT (runtime-sqlx allowlist): EXPLAIN is planner introspection over
    // the dynamic filter SQL the compile-time macros cannot prepare.
    let row = sqlx::query(explain_subtitle_sql)
        .bind("Subtitle 12345")
        .fetch_one(&mut *tx)
        .await
        .expect("explain subtitle_contains");
    let plan: serde_json::Value = row.get("QUERY PLAN");
    tx.rollback().await.expect("rollback explain txn");
    assert_no_seq_scan_on(&plan, "works", "subtitle_contains");
    assert!(
        plan_uses_index(&plan, "idx_works_subtitle_trgm"),
        "subtitle_contains must ride the trigram index, not a full-scan Filter: {plan}"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn title_contains_diacritic_folding_rides_trgm_index_at_scale(pool: PgPool) {
    use sqlx::Row as _;

    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;

    // 50k works with distinct titles, plus one accented needle so a folded,
    // unaccented query still selects a single row: the case the folded
    // trigram index (`immutable_unaccent(title)`) must serve.
    sqlx::query!(
        "INSERT INTO works (title, sort_title) \
         SELECT 'Bulk Opus ' || g.n, 'Bulk Opus ' || g.n \
         FROM generate_series(1, 50000) AS g(n)"
    )
    .execute(&ingestion_pool)
    .await
    .expect("seed works");
    sqlx::query!(
        "INSERT INTO works (title, sort_title) VALUES ('\u{c9}mile Zola', '\u{c9}mile Zola')"
    )
    .execute(&ingestion_pool)
    .await
    .expect("seed accented work");
    sqlx::query!(
        "INSERT INTO manifestations \
            (work_id, format, file_path, ingestion_file_hash, current_file_hash, \
             file_size_bytes, ingestion_status, validation_status) \
         SELECT w.id, 'epub'::manifestation_format, '/tmp/bulk-' || w.id, \
                'bulk-hash-' || w.id, 'bulk-hash-' || w.id, 1000, \
                'complete'::ingestion_status, 'clean'::validation_status \
         FROM works w"
    )
    .execute(&ingestion_pool)
    .await
    .expect("seed manifestations");

    // CARVE-OUT (runtime-sqlx allowlist): ANALYZE is maintenance DDL the macros
    // cannot validate; fresh stats keep the planner from choosing empty-table
    // plans for the bulk seed. Runs on the owner pool (ANALYZE needs ownership).
    sqlx::query("ANALYZE works, manifestations")
        .execute(&pool)
        .await
        .expect("analyze seeded tables");

    // Literal twin of push_ilike_contains' folded output for title_contains on
    // the recent-sort list query (default page size + 1).
    let explain_title_sql = "EXPLAIN (FORMAT JSON) \
        SELECT m.id FROM manifestations m \
        JOIN works w ON w.id = m.work_id \
        WHERE TRUE AND immutable_unaccent(w.title) \
            ILIKE '%' || nullif(immutable_unaccent_like($1), '') || '%' \
        ORDER BY m.created_at DESC, m.id DESC LIMIT 61";

    let mut tx = ingestion_pool.begin().await.expect("begin explain txn");
    // CARVE-OUT (runtime-sqlx allowlist): SET LOCAL is a transaction-scoped GUC
    // mutation the compile-time macros cannot validate.
    sqlx::query("SET LOCAL enable_seqscan = off")
        .execute(&mut *tx)
        .await
        .expect("disable seqscan for the probe");
    // CARVE-OUT (runtime-sqlx allowlist): SET LOCAL is a transaction-scoped GUC
    // mutation the compile-time macros cannot validate.
    sqlx::query("SET LOCAL enable_nestloop = off")
        .execute(&mut *tx)
        .await
        .expect("disable nested loops for the probe");
    // CARVE-OUT (runtime-sqlx allowlist): EXPLAIN is planner introspection over
    // the dynamic filter SQL the compile-time macros cannot prepare.
    let row = sqlx::query(explain_title_sql)
        // Unaccented needle: the folded index, not the dropped raw one, must
        // serve this query.
        .bind("emile zola")
        .fetch_one(&mut *tx)
        .await
        .expect("explain title_contains diacritic folding");
    let plan: serde_json::Value = row.get("QUERY PLAN");
    tx.rollback().await.expect("rollback explain txn");
    assert_no_seq_scan_on(&plan, "works", "title_contains diacritic folding");
    assert!(
        plan_uses_index(&plan, "idx_works_title_trgm"),
        "title_contains must ride the folded trigram index, not a full-scan Filter: {plan}"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn list_filter_genre_predicates_use_indexes_at_scale(pool: PgPool) {
    use sqlx::Row as _;

    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;

    // Set-based bulk seed: 50k works + manifestations, 200 genres,
    // 3 genres per manifestation (150k junction rows).
    sqlx::query!(
        "INSERT INTO works (title, sort_title) \
         SELECT 'Bulk Opus ' || g.n, 'Bulk Opus ' || g.n \
         FROM generate_series(1, 50000) AS g(n)"
    )
    .execute(&ingestion_pool)
    .await
    .expect("seed works");
    sqlx::query!(
        "INSERT INTO manifestations \
            (work_id, format, file_path, ingestion_file_hash, current_file_hash, \
             file_size_bytes, ingestion_status, validation_status) \
         SELECT w.id, 'epub'::manifestation_format, '/tmp/bulk-' || w.id, \
                'bulk-hash-' || w.id, 'bulk-hash-' || w.id, 1000, \
                'complete'::ingestion_status, 'clean'::validation_status \
         FROM works w"
    )
    .execute(&ingestion_pool)
    .await
    .expect("seed manifestations");
    sqlx::query!(
        "INSERT INTO genres (name) \
         SELECT 'bulkgenre-' || g.n FROM generate_series(1, 200) AS g(n)"
    )
    .execute(&ingestion_pool)
    .await
    .expect("seed genres");
    // Offsets 0/67/134 are pairwise distinct mod 200, so the composite
    // PK never conflicts.
    sqlx::query!(
        "INSERT INTO manifestation_genres (manifestation_id, genre_id) \
         SELECT m.id, g.id \
         FROM (SELECT id, row_number() OVER (ORDER BY id) AS rn FROM manifestations) m \
         CROSS JOIN generate_series(0, 2) AS k(k) \
         JOIN (SELECT id, row_number() OVER (ORDER BY id) AS rn FROM genres) g \
           ON g.rn = ((m.rn + k.k * 67) % 200) + 1"
    )
    .execute(&ingestion_pool)
    .await
    .expect("seed manifestation_genres");

    // CARVE-OUT (runtime-sqlx allowlist): ANALYZE is maintenance DDL the
    // macros cannot validate; fresh stats keep the planner from choosing
    // empty-table plans for the bulk seed. Runs on the owner pool
    // (ANALYZE requires table ownership).
    sqlx::query("ANALYZE manifestation_genres, genres, manifestations, works")
        .execute(&pool)
        .await
        .expect("analyze seeded tables");

    let genre_names: Vec<String> = vec!["bulkgenre-7".into(), "bulkgenre-93".into()];

    // Literal SQL twins of the QueryBuilder output assembled by
    // `push_vocab_predicates` for the genre any-of and all-of legs of
    // GET /api/v1/books (recent sort, default page size + 1).
    let explain_any_of_sql = "EXPLAIN (FORMAT JSON) \
        SELECT m.id FROM manifestations m \
        JOIN works w ON w.id = m.work_id \
        WHERE TRUE AND EXISTS (SELECT 1 FROM manifestation_genres mg \
          JOIN genres g ON g.id = mg.genre_id \
          WHERE mg.manifestation_id = m.id AND lower(g.name) = ANY($1)) \
        ORDER BY m.created_at DESC, m.id DESC LIMIT 61";
    let explain_all_of_sql = "EXPLAIN (FORMAT JSON) \
        SELECT m.id FROM manifestations m \
        JOIN works w ON w.id = m.work_id \
        WHERE TRUE AND (SELECT COUNT(DISTINCT g.name) FROM manifestation_genres mg \
          JOIN genres g ON g.id = mg.genre_id \
          WHERE mg.manifestation_id = m.id AND lower(g.name) = ANY($1)) = $2 \
        ORDER BY m.created_at DESC, m.id DESC LIMIT 61";

    // CARVE-OUT (runtime-sqlx allowlist): EXPLAIN is planner
    // introspection over dynamic filter SQL; the compile-time macros
    // cannot prepare it.
    let row = sqlx::query(explain_any_of_sql)
        .bind(&genre_names)
        .fetch_one(&ingestion_pool)
        .await
        .expect("explain any-of genre filter");
    let any_plan: serde_json::Value = row.get("QUERY PLAN");
    assert_no_seq_scan_on(&any_plan, "manifestation_genres", "any-of");

    let row = sqlx::query(explain_all_of_sql)
        .bind(&genre_names)
        .bind(2_i64)
        .fetch_one(&ingestion_pool)
        .await
        .expect("explain all-of genre filter");
    let all_plan: serde_json::Value = row.get("QUERY PLAN");
    assert_no_seq_scan_on(&all_plan, "manifestation_genres", "all-of");
}

#[sqlx::test(migrations = "./migrations")]
async fn pages_sort_rides_desc_nulls_last_index_at_scale(pool: PgPool) {
    use sqlx::Row as _;

    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;

    // Set-based bulk seed: 50k works + manifestations. `pages` is left NULL, so
    // the whole corpus is the NULLS LAST bucket: the exact shape a fresh library
    // has before page counts are enriched, and the case the ADR calls out as
    // load-bearing for the DESC NULLS LAST index.
    sqlx::query!(
        "INSERT INTO works (title, sort_title) \
         SELECT 'Bulk Opus ' || g.n, 'Bulk Opus ' || g.n \
         FROM generate_series(1, 50000) AS g(n)"
    )
    .execute(&ingestion_pool)
    .await
    .expect("seed works");
    sqlx::query!(
        "INSERT INTO manifestations \
            (work_id, format, file_path, ingestion_file_hash, current_file_hash, \
             file_size_bytes, ingestion_status, validation_status) \
         SELECT w.id, 'epub'::manifestation_format, '/tmp/bulk-' || w.id, \
                'bulk-hash-' || w.id, 'bulk-hash-' || w.id, 1000, \
                'complete'::ingestion_status, 'clean'::validation_status \
         FROM works w"
    )
    .execute(&ingestion_pool)
    .await
    .expect("seed manifestations");

    // CARVE-OUT (runtime-sqlx allowlist): ANALYZE is maintenance DDL the
    // macros cannot validate; fresh stats keep the planner from choosing an
    // empty-table plan for the bulk seed. Runs on the owner pool (ANALYZE
    // requires table ownership). The genre tables are empty here (harmless).
    sqlx::query("ANALYZE manifestation_genres, genres, manifestations, works")
        .execute(&pool)
        .await
        .expect("analyze seeded tables");

    // Literal twin of push_order_by's output for `?sort=-pages` (default page
    // size + 1). The gate: even an all-NULL corpus must ride the DESC NULLS
    // LAST pages index added in the sort-whitelist migration, never a
    // full-table Seq Scan + Sort (the degradation the ADR's revisit trigger
    // names).
    let explain_pages_sql = "EXPLAIN (FORMAT JSON) \
        SELECT m.id FROM manifestations m \
        JOIN works w ON w.id = m.work_id \
        ORDER BY m.pages DESC NULLS LAST, m.id DESC LIMIT 61";

    // CARVE-OUT (runtime-sqlx allowlist): EXPLAIN is planner introspection
    // over the dynamic sort SQL the QueryBuilder assembles; the compile-time
    // macros cannot prepare it.
    let row = sqlx::query(explain_pages_sql)
        .fetch_one(&ingestion_pool)
        .await
        .expect("explain -pages sort");
    let pages_plan: serde_json::Value = row.get("QUERY PLAN");
    assert_no_seq_scan_on(&pages_plan, "manifestations", "-pages");
}

// ─── 11b — search endpoint ───────────────────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn search_returns_ranked_results(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;

    insert_book(&ingestion_pool, "1", "The Hitchhiker Guide to the Galaxy").await;
    insert_book(&ingestion_pool, "2", "Pride and Prejudice").await;

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let r = server
        .get("/api/v1/search?q=galaxy")
        .add_header(AUTHORIZATION, basic)
        .await;
    assert_eq!(r.status_code(), StatusCode::OK);
    let body: serde_json::Value = r.json();
    let items = body["items"].as_array().expect("items");
    assert_eq!(items.len(), 1, "only the matching title surfaces");
    assert_eq!(items[0]["kind"], "book");
    assert!(
        items[0]["snippet"]
            .as_str()
            .is_some_and(|s| s.contains('\u{0002}')),
        "snippet must carry STX highlight marker, got {:?}",
        items[0]["snippet"]
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn search_typo_tolerant_via_trigram(pool: PgPool) {
    // "Hemingwy" (typo) should still find "Hemingway" via the trigram leg.
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;

    insert_book(&ingestion_pool, "h", "Hemingway").await;

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let r = server
        .get("/api/v1/search?q=Hemingwy")
        .add_header(AUTHORIZATION, basic)
        .await;
    assert_eq!(r.status_code(), StatusCode::OK);
    let items = r.json::<serde_json::Value>()["items"]
        .as_array()
        .cloned()
        .expect("items");
    assert_eq!(items.len(), 1, "trigram leg recovers the typo");
}

#[sqlx::test(migrations = "./migrations")]
async fn search_diacritic_folding_matches_unaccented_query(pool: PgPool) {
    // "emile zola" (no diacritics) must find "\u{c9}mile Zola" via the
    // hybrid search, folding accents on both the tsvector and trigram legs.
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;

    insert_book(&ingestion_pool, "emile", "\u{c9}mile Zola").await;
    insert_book(&ingestion_pool, "other", "Unrelated Title").await;

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let r = server
        .get("/api/v1/search?q=emile+zola")
        .add_header(AUTHORIZATION, basic.clone())
        .await;
    assert_eq!(r.status_code(), StatusCode::OK);
    let items = r.json::<serde_json::Value>()["items"]
        .as_array()
        .cloned()
        .expect("items");
    assert!(
        items.iter().any(|i| i["title"] == "\u{c9}mile Zola"),
        "unaccented query must find the accented title, got {items:?}"
    );

    // Exact accented input still matches (no regression from folding).
    let r = server
        .get("/api/v1/search?q=%C3%89mile+Zola")
        .add_header(AUTHORIZATION, basic)
        .await;
    assert_eq!(r.status_code(), StatusCode::OK);
    let items = r.json::<serde_json::Value>()["items"]
        .as_array()
        .cloned()
        .expect("items");
    assert!(
        items.iter().any(|i| i["title"] == "\u{c9}mile Zola"),
        "accented query must still match the accented title, got {items:?}"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn search_websearch_phrase_hits_phrase(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;

    insert_book(&ingestion_pool, "wp", "War and Peace").await;
    insert_book(&ingestion_pool, "ak", "Anna Karenina").await;
    insert_book(&ingestion_pool, "rd", "Resurrection Detail").await;

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let r = server
        .get("/api/v1/search?q=%22war+and+peace%22")
        .add_header(AUTHORIZATION, basic.clone())
        .await;
    assert_eq!(r.status_code(), StatusCode::OK);
    let items = r.json::<serde_json::Value>()["items"]
        .as_array()
        .cloned()
        .expect("items");
    assert!(
        items.iter().any(|i| i["title"] == "War and Peace"),
        "phrase search hits War and Peace, got {items:?}"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn search_websearch_exclude_operator(pool: PgPool) {
    // `tolstoy -anna` — the tsquery leg honours `-anna`. The trigram
    // leg compares raw similarity against the whole query string, so
    // it does not honour token negation — the excluded row may still
    // re-surface via trigram. The composite gate is therefore "the
    // non-excluded match is present in the result set"; tightening to
    // "the excluded match is absent" would require either dropping
    // the trigram leg for queries containing `-tokens` (planner
    // complexity) or post-filtering in Rust (defeats trigram
    // robustness). Documented limitation; revisit in 11b follow-up if
    // user research shows exclude is heavily used.
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;

    insert_book(&ingestion_pool, "wp", "Tolstoy War and Peace").await;
    insert_book(&ingestion_pool, "ak", "Tolstoy Anna Karenina").await;

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let r = server
        .get("/api/v1/search?q=tolstoy+-anna")
        .add_header(AUTHORIZATION, basic)
        .await;
    assert_eq!(r.status_code(), StatusCode::OK);
    let items = r.json::<serde_json::Value>()["items"]
        .as_array()
        .cloned()
        .expect("items");
    let titles: Vec<String> = items
        .iter()
        .map(|i| i["title"].as_str().unwrap_or_default().to_owned())
        .collect();
    assert!(
        titles.iter().any(|t| t.contains("War and Peace")),
        "non-Anna Tolstoy must be in results: {titles:?}"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn search_empty_query_returns_422(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let r = server
        .get("/api/v1/search?q=")
        .add_header(AUTHORIZATION, basic)
        .await;
    test_support::assert_problem(&r, problems::VALIDATION, StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test(migrations = "./migrations")]
async fn search_missing_query_returns_422(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let r = server
        .get("/api/v1/search")
        .add_header(AUTHORIZATION, basic)
        .await;
    test_support::assert_problem(&r, problems::VALIDATION, StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test(migrations = "./migrations")]
async fn search_oversized_query_returns_422(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let long_q = "a".repeat(201);
    let r = server
        .get(&format!("/api/v1/search?q={long_q}"))
        .add_header(AUTHORIZATION, basic)
        .await;
    test_support::assert_problem(&r, problems::VALIDATION, StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test(migrations = "./migrations")]
async fn search_sql_injection_probe_is_safe(pool: PgPool) {
    // Bound parameter path absorbs a malformed query; schema survives.
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;

    insert_book(&ingestion_pool, "x", "Harmless Title").await;

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let _ = server
        .get("/api/v1/search?q=%27%29%3B+DROP+TABLE+works%3B--")
        .add_header(AUTHORIZATION, basic)
        .await;

    let still_there: i64 = sqlx::query_scalar!("SELECT COUNT(*) AS \"c!\" FROM works")
        .fetch_one(&ingestion_pool)
        .await
        .expect("works survives");
    assert!(still_there > 0, "works table must survive the SQL probe");
}

#[sqlx::test(migrations = "./migrations")]
async fn books_filter_sql_injection_probe_is_safe(pool: PgPool) {
    // Every typed filter value reaches SQL through `push_bind`; a hostile needle
    // in a text filter or the quick search is bound, not interpolated, so the
    // schema survives. Guards the larger new user-controlled surface on
    // `/api/v1/books` the way the sibling `/search` probe guards its own.
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;

    insert_book(&ingestion_pool, "x", "Harmless Title").await;

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    // `');DROP TABLE works;--` delivered through both a text column filter and
    // the full-text quick search. Both are valid (if hostile) strings, so the
    // request is a normal 200 with zero matches, not an error.
    let response = server
        .get("/api/v1/books?title_contains=%27%29%3B+DROP+TABLE+works%3B--&q=%27%3B+DROP+TABLE+works%3B--")
        .add_header(AUTHORIZATION, basic)
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);
    // The hostile payload is a valid string that matches no title, so the list
    // must come back empty. Asserting zero matches (not just 200) rules out the
    // injection silently widening the result set instead of narrowing it.
    let body: serde_json::Value = response.json();
    assert!(
        body["items"].as_array().expect("items array").is_empty(),
        "hostile filter must match nothing, got {body}"
    );

    let still_there: i64 = sqlx::query_scalar!("SELECT COUNT(*) AS \"c!\" FROM works")
        .fetch_one(&ingestion_pool)
        .await
        .expect("works survives");
    assert!(still_there > 0, "works table must survive the SQL probe");
}

#[sqlx::test(migrations = "./migrations")]
async fn search_unauthenticated_returns_401(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let r = server.get("/api/v1/search?q=anything").await;
    assert_eq!(r.status_code(), StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "./migrations")]
async fn search_child_only_sees_shelved_titles(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (child_id, basic) =
        test_support::db::create_child_user_and_basic_auth(&app_pool, "kiddo").await;

    let (_w1, m1) = insert_book(&ingestion_pool, "k", "Kid Favourite Story").await;
    insert_book(&ingestion_pool, "g", "Grown-Up Story").await;

    let shelf = test_support::db::create_shelf(&app_pool, child_id, "kid-shelf").await;
    test_support::db::add_to_shelf(&app_pool, shelf, m1).await;

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let r = server
        .get("/api/v1/search?q=story")
        .add_header(AUTHORIZATION, basic)
        .await;
    assert_eq!(r.status_code(), StatusCode::OK);
    let items = r.json::<serde_json::Value>()["items"]
        .as_array()
        .cloned()
        .expect("items");
    assert_eq!(items.len(), 1, "child sees only their shelved row");
    assert_eq!(items[0]["title"], "Kid Favourite Story");
}

// ─── 11b — perf gate ─────────────────────────────────────────────────────

// Performance gate: seeds 10K rows and asserts p50 of 11 trials
// stays under 200 ms. `#[ignore]`d so default per-PR runs skip it;
// the nightly CI lane runs `cargo test -- --ignored`. Calibrated
// against the dev DB — treat as a regression alarm (planner change,
// missing index), not a production SLO.
#[sqlx::test(migrations = "./migrations")]
#[ignore = "perf gate — run via `cargo test -- --ignored` (nightly CI)"]
async fn perf_search_p50_under_200ms_at_10k_rows(pool: PgPool) {
    use std::time::Instant;

    const SEED_ROWS: usize = 10_000;
    const P50_LIMIT_MS: u128 = 200;
    const TRIALS: usize = 11;

    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;

    for i in 0..SEED_ROWS {
        let title = format!("Title{i} Book");
        let work_id: Uuid = sqlx::query_scalar!(
            "INSERT INTO works (title, sort_title) VALUES ($1, $1) RETURNING id",
            title,
        )
        .fetch_one(&ingestion_pool)
        .await
        .expect("seed work");
        let hash = format!("perf-search-hash-{i}");
        let file_path = format!("/tmp/perf-search-{i}.epub");
        sqlx::query!(
            "INSERT INTO manifestations \
                (work_id, format, file_path, ingestion_file_hash, current_file_hash, \
                 file_size_bytes, ingestion_status, validation_status) \
             VALUES ($1, 'epub'::manifestation_format, $2, $3, $3, 1000, \
                     'complete'::ingestion_status, 'clean'::validation_status)",
            work_id,
            file_path,
            hash,
        )
        .execute(&ingestion_pool)
        .await
        .expect("seed manifestation");
    }

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let mut times_ms: Vec<u128> = Vec::with_capacity(TRIALS);
    for i in 0..TRIALS {
        let start = Instant::now();
        let q = format!("title{i}");
        let r = server
            .get(&format!("/api/v1/search?q={q}"))
            .add_header(AUTHORIZATION, basic.clone())
            .await;
        times_ms.push(start.elapsed().as_millis());
        assert_eq!(r.status_code(), StatusCode::OK);
    }
    times_ms.sort_unstable();
    let p50 = times_ms[TRIALS / 2];
    assert!(
        p50 < P50_LIMIT_MS,
        "p50 {p50} ms over {P50_LIMIT_MS} ms gate (full set {times_ms:?})"
    );
}

/// PR1 (`/api`→`/api/v1`): the prefix change is a *move*, not an
/// additive alias. The new versioned path serves; the old unversioned path is
/// gone and — critically — falls through to the reserved-prefix JSON `404`
/// Problem, never the SPA `index.html`, so stale API clients receive a
/// machine-readable error instead of an HTML `200`.
#[sqlx::test(migrations = "./migrations")]
async fn api_v1_move_old_path_returns_problem_not_spa(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

    // New versioned path serves (empty library → 200, empty list).
    let new = server
        .get("/api/v1/books")
        .add_header(AUTHORIZATION, basic.clone())
        .await;
    assert_eq!(new.status_code(), StatusCode::OK);

    // Old path is gone: reserved-prefix fallback emits a JSON Problem 404
    // (assert_problem checks the application/problem+json Content-Type, so a
    // regression that served SPA HTML here would fail).
    let old = server
        .get("/api/books")
        .add_header(AUTHORIZATION, basic)
        .await;
    test_support::assert_problem(&old, problems::NOT_FOUND, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = "./migrations")]
async fn search_endpoint_duplicate_query_key_returns_400(pool: PgPool) {
    // A duplicate `?q=a&q=b` rejects at the axum_extra::Query extractor
    // (serde_html_form errors on a repeated scalar key) and must surface as
    // RFC 9457 problem+json, not axum's plaintext 400 (clears debt 2026-06-10).
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

    let response = server
        .get("/api/v1/search?q=a&q=b")
        .add_header(AUTHORIZATION, basic)
        .await;
    test_support::assert_problem(
        &response,
        problems::MALFORMED_QUERY,
        StatusCode::BAD_REQUEST,
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn list_endpoint_reading_state_is_caller_scoped(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_a_id, a_basic) =
        test_support::db::create_adult_and_basic_auth(&app_pool, "reading-list-a").await;
    let (_b_id, b_basic) =
        test_support::db::create_adult_and_basic_auth(&app_pool, "reading-list-b").await;
    let (_work, m_id) = insert_book(&ingestion_pool, "reading-list", "Reading List Book").await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

    let a_read = server
        .get(&format!("/api/v1/books/{m_id}/reading"))
        .add_header(AUTHORIZATION, a_basic.clone())
        .await;
    let a_etag = a_read
        .headers()
        .get(axum::http::header::ETAG)
        .expect("ETag header present")
        .clone();
    server
        .patch(&format!("/api/v1/books/{m_id}/reading"))
        .add_header(AUTHORIZATION, a_basic.clone())
        .add_header(axum::http::header::IF_MATCH, a_etag)
        .json(&serde_json::json!({"status": "reading", "rating": 4}))
        .await;

    let a_list = server
        .get("/api/v1/books")
        .add_header(AUTHORIZATION, a_basic)
        .await;
    assert_eq!(a_list.status_code(), StatusCode::OK);
    let a_body: serde_json::Value = a_list.json();
    let a_item = a_body["items"]
        .as_array()
        .expect("items array")
        .iter()
        .find(|i| i["id"].as_str() == Some(m_id.to_string().as_str()))
        .expect("book present in A's list");
    assert_eq!(a_item["reading_state"]["status"], "reading");
    assert_eq!(a_item["reading_state"]["rating"], 4);

    let b_list = server
        .get("/api/v1/books")
        .add_header(AUTHORIZATION, b_basic)
        .await;
    assert_eq!(b_list.status_code(), StatusCode::OK);
    let b_body: serde_json::Value = b_list.json();
    let b_item = b_body["items"]
        .as_array()
        .expect("items array")
        .iter()
        .find(|i| i["id"].as_str() == Some(m_id.to_string().as_str()))
        .expect("book present in B's list");
    assert!(
        b_item["reading_state"].is_null(),
        "user B has no reading_state row for this book: {b_item}"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn list_endpoint_reading_summary_carries_reading_dates(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_a_id, a_basic) =
        test_support::db::create_adult_and_basic_auth(&app_pool, "reading-notes-a").await;
    let (_b_id, b_basic) =
        test_support::db::create_adult_and_basic_auth(&app_pool, "reading-notes-b").await;
    let (_work, m_id) = insert_book(&ingestion_pool, "reading-notes", "Annotated Book").await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

    // Real write path: entering `reading` stamps started_at, entering
    // `finished` stamps finished_at — the summary must carry both stamps.
    let a_read = server
        .get(&format!("/api/v1/books/{m_id}/reading"))
        .add_header(AUTHORIZATION, a_basic.clone())
        .await;
    let a_etag = a_read
        .headers()
        .get(axum::http::header::ETAG)
        .expect("ETag header present")
        .clone();
    let first = server
        .patch(&format!("/api/v1/books/{m_id}/reading"))
        .add_header(AUTHORIZATION, a_basic.clone())
        .add_header(axum::http::header::IF_MATCH, a_etag)
        .json(&serde_json::json!({"status": "reading", "notes": "loved the first act"}))
        .await;
    let a_etag = first
        .headers()
        .get(axum::http::header::ETAG)
        .expect("ETag header present")
        .clone();
    server
        .patch(&format!("/api/v1/books/{m_id}/reading"))
        .add_header(AUTHORIZATION, a_basic.clone())
        .add_header(axum::http::header::IF_MATCH, a_etag)
        .json(&serde_json::json!({"status": "finished"}))
        .await;

    let a_list = server
        .get("/api/v1/books")
        .add_header(AUTHORIZATION, a_basic)
        .await;
    assert_eq!(a_list.status_code(), StatusCode::OK);
    let a_body: serde_json::Value = a_list.json();
    let a_state = &a_body["items"][0]["reading_state"];
    assert_eq!(a_state["status"], "finished", "got {a_body}");
    // Notes are capped at 10k characters per row, so a page of summaries
    // must not carry them; the single-book reading endpoint serves them.
    assert!(
        a_state
            .as_object()
            .is_some_and(|o| !o.contains_key("notes")),
        "notes must stay off the list summary, got {a_body}"
    );
    for stamp in ["started_at", "finished_at"] {
        let raw = a_state[stamp].as_str().unwrap_or_else(|| {
            panic!("{stamp} must be an RFC 3339 string on the list summary, got {a_body}")
        });
        chrono::DateTime::parse_from_rfc3339(raw)
            .unwrap_or_else(|e| panic!("{stamp} must parse as RFC 3339 ({e}), got {a_body}"));
    }

    // A rating-only row keeps the new slots as explicit nulls.
    let b_read = server
        .get(&format!("/api/v1/books/{m_id}/reading"))
        .add_header(AUTHORIZATION, b_basic.clone())
        .await;
    let b_etag = b_read
        .headers()
        .get(axum::http::header::ETAG)
        .expect("ETag header present")
        .clone();
    server
        .patch(&format!("/api/v1/books/{m_id}/reading"))
        .add_header(AUTHORIZATION, b_basic.clone())
        .add_header(axum::http::header::IF_MATCH, b_etag)
        .json(&serde_json::json!({"rating": 3}))
        .await;
    let b_list = server
        .get("/api/v1/books")
        .add_header(AUTHORIZATION, b_basic)
        .await;
    assert_eq!(b_list.status_code(), StatusCode::OK);
    let b_body: serde_json::Value = b_list.json();
    let b_state = &b_body["items"][0]["reading_state"];
    assert_eq!(b_state["rating"], 3, "got {b_body}");
    for slot in ["started_at", "finished_at"] {
        assert!(
            b_state[slot].is_null() && b_state.as_object().is_some_and(|o| o.contains_key(slot)),
            "rating-only summary must carry explicit null {slot}, got {b_body}"
        );
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn seed_library_50k_script_seeds_and_pages(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;

    // psql meta-commands (`\set ...`) aren't valid SQL; sqlx executes the
    // rest of the script verbatim as one multi-statement batch, same as
    // `reverie-dev psql-rw -f ...` would send it.
    let seed_sql: String = include_str!("../../../scripts/seed_library_50k.sql")
        .lines()
        .filter(|line| !line.trim_start().starts_with('\\'))
        .collect::<Vec<_>>()
        .join("\n");

    // CARVE-OUT (runtime-sqlx allowlist): the seed script is a static,
    // operator-run file executed exactly as written (BEGIN/DO
    // guard/bulk INSERT/ANALYZE/COMMIT), not a `query!` candidate. Runs
    // on `pool`, the schema-owner connection under `#[sqlx::test]`,
    // mirroring `reverie_migrator` (which owns every app table in a real
    // deploy and so bypasses `manifestations` RLS the same way).
    sqlx::raw_sql(sqlx::AssertSqlSafe(seed_sql.as_str()))
        .execute(&pool)
        .await
        .expect("seed script executes cleanly against an empty DB");

    let work_count = sqlx::query_scalar!("SELECT count(*) AS \"count!\" FROM works")
        .fetch_one(&pool)
        .await
        .expect("count works");
    assert_eq!(work_count, 50_000, "expected 50k seeded works");

    let manifestation_count =
        sqlx::query_scalar!("SELECT count(*) AS \"count!\" FROM manifestations")
            .fetch_one(&pool)
            .await
            .expect("count manifestations");
    assert_eq!(
        manifestation_count, 50_000,
        "expected 50k seeded manifestations"
    );

    let work_author_count = sqlx::query_scalar!("SELECT count(*) AS \"count!\" FROM work_authors")
        .fetch_one(&pool)
        .await
        .expect("count work_authors");
    assert_eq!(
        work_author_count, 50_000,
        "seed links exactly one author per work; a different count means the sort_name join matched wrong"
    );

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

    let first = server
        .get("/api/v1/books")
        .add_header(AUTHORIZATION, basic.clone())
        .await;
    assert_eq!(first.status_code(), StatusCode::OK);
    let first_body: serde_json::Value = first.json();
    let first_items = first_body["items"].as_array().expect("items array");
    assert_eq!(
        first_items.len(),
        50,
        "first page is a full page over 50k seeded rows"
    );
    let first_ids: std::collections::HashSet<&str> = first_items
        .iter()
        .map(|it| it["id"].as_str().expect("item id"))
        .collect();

    let next_cursor = first_body["next_cursor"]
        .as_str()
        .expect("50k rows over page_size=50 must carry a next_cursor")
        .to_owned();

    let second = server
        .get(&format!("/api/v1/books?cursor={next_cursor}"))
        .add_header(AUTHORIZATION, basic)
        .await;
    assert_eq!(second.status_code(), StatusCode::OK);
    let second_body: serde_json::Value = second.json();
    let second_items = second_body["items"].as_array().expect("items array");
    assert_eq!(
        second_items.len(),
        50,
        "second page is also a full page this far from the tail"
    );
    let second_ids: std::collections::HashSet<&str> = second_items
        .iter()
        .map(|it| it["id"].as_str().expect("item id"))
        .collect();

    assert!(
        first_ids.is_disjoint(&second_ids),
        "cursor paging must not repeat ids across pages"
    );

    // Guard: re-running the full script against the now-50k-seeded DB
    // must abort inside the DO block instead of doubling the corpus.
    let rerun = sqlx::raw_sql(sqlx::AssertSqlSafe(seed_sql.as_str()))
        .execute(&pool)
        .await;
    assert!(
        rerun.is_err(),
        "re-running the seed script against a 50k-seeded DB must error (idempotence guard)"
    );

    let work_count_after_rerun = sqlx::query_scalar!("SELECT count(*) AS \"count!\" FROM works")
        .fetch_one(&pool)
        .await
        .expect("count works after guarded rerun attempt");
    assert_eq!(
        work_count_after_rerun, 50_000,
        "guard must roll back the whole rerun transaction; works count must not double"
    );
}

// ---------------------------------------------------------------------------
// Typed per-column filters + quick search.
// ---------------------------------------------------------------------------

use crate::models::reading_status::ReadingStatus;

/// Set a manifestation's `subtitle` (the column lives on `works`).
async fn set_subtitle(ingestion_pool: &PgPool, work_id: Uuid, subtitle: &str) {
    sqlx::query!(
        "UPDATE works SET subtitle = $1 WHERE id = $2",
        subtitle,
        work_id,
    )
    .execute(ingestion_pool)
    .await
    .expect("set subtitle");
}

/// Set a work's `description`; the works trigger refolds `search_vector`.
async fn set_description(ingestion_pool: &PgPool, work_id: Uuid, description: &str) {
    sqlx::query!(
        "UPDATE works SET description = $1 WHERE id = $2",
        description,
        work_id,
    )
    .execute(ingestion_pool)
    .await
    .expect("set description");
}

/// Set a manifestation's `isbn_13`.
async fn set_isbn(ingestion_pool: &PgPool, m_id: Uuid, isbn: &str) {
    sqlx::query!(
        "UPDATE manifestations SET isbn_13 = $1 WHERE id = $2",
        isbn,
        m_id,
    )
    .execute(ingestion_pool)
    .await
    .expect("set isbn_13");
}

/// Insert a caller-scoped `reading_state` row (status and/or rating) through
/// an RLS transaction, matching the write path the reading-domain endpoint
/// uses. A `None` status leaves the column NULL, which is what the `unread`
/// pseudo-value keys on.
async fn set_reading(
    app_pool: &PgPool,
    user_id: Uuid,
    m_id: Uuid,
    status: Option<ReadingStatus>,
    rating: Option<i16>,
) {
    let mut tx = crate::db::acquire_with_rls(app_pool, user_id)
        .await
        .expect("rls tx");
    // Bind the status through a text cast: sqlx has no built-in Rust mapping
    // for the `reading_status` enum param, so `$3::text::reading_status` lets
    // it describe the bind as text and the wire name casts to the enum.
    let status_wire = status.map(ReadingStatus::as_str);
    sqlx::query!(
        "INSERT INTO reading_state (user_id, manifestation_id, status, rating) \
         VALUES ($1, $2, $3::text::reading_status, $4)",
        user_id,
        m_id,
        status_wire,
        rating,
    )
    .execute(&mut *tx)
    .await
    .expect("insert reading_state");
    tx.commit().await.expect("commit reading_state");
}

/// Insert a work + manifestation with one contributor in `role`, returning
/// `(work_id, manifestation_id, author_id)` so author-filter tests can add
/// co-contributors and pass the id.
async fn insert_book_with_role(
    ingestion_pool: &PgPool,
    marker: &str,
    title: &str,
    author_name: &str,
    role: &str,
) -> (Uuid, Uuid, Uuid) {
    let (work_id, m_id) = insert_book(ingestion_pool, marker, title).await;
    let author_id =
        test_support::db::insert_contributor(ingestion_pool, work_id, author_name, role, 0).await;
    (work_id, m_id, author_id)
}

/// GET a filtered list as admin and return the item titles in response order.
async fn filtered_titles(server: &TestServer, basic: &str, url: &str) -> Vec<String> {
    let response = server
        .get(url)
        .add_header(AUTHORIZATION, basic.to_owned())
        .await;
    assert_eq!(
        response.status_code(),
        StatusCode::OK,
        "GET {url}: {}",
        response.text()
    );
    let body: serde_json::Value = response.json();
    body["items"]
        .as_array()
        .expect("items array")
        .iter()
        .map(|i| i["title"].as_str().expect("title").to_owned())
        .collect()
}

#[sqlx::test(migrations = "./migrations")]
async fn filter_title_contains_is_case_insensitive_and_escapes_wildcards(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    insert_book(&ingestion_pool, "dune", "Dune Messiah").await;
    insert_book(&ingestion_pool, "pct", "50% Discount Manual").await;
    insert_book(&ingestion_pool, "other", "Foundation").await;
    let server = server_with_page_size(&app_pool, &ingestion_pool, 50);

    // Case-insensitive substring.
    let hits = filtered_titles(&server, &basic, "/api/v1/books?title_contains=dune").await;
    assert_eq!(hits, vec!["Dune Messiah"]);

    // A literal `%` in the needle must not act as a wildcard: it matches the
    // title that literally contains `%`, and nothing else.
    let hits = filtered_titles(&server, &basic, "/api/v1/books?title_contains=50%25").await;
    assert_eq!(hits, vec!["50% Discount Manual"]);
}

#[sqlx::test(migrations = "./migrations")]
async fn filter_title_contains_folds_accents(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    insert_book(&ingestion_pool, "emile", "\u{c9}mile Zola").await;
    insert_book(&ingestion_pool, "other", "Unrelated Title").await;
    let server = server_with_page_size(&app_pool, &ingestion_pool, 50);

    // Unaccented needle finds the accented row.
    let hits = filtered_titles(&server, &basic, "/api/v1/books?title_contains=emile%20zola").await;
    assert_eq!(hits, vec!["\u{c9}mile Zola"]);

    // Accented needle still matches (no regression from folding).
    let hits = filtered_titles(&server, &basic, "/api/v1/books?title_contains=%C3%89mile").await;
    assert_eq!(hits, vec!["\u{c9}mile Zola"]);
}

#[sqlx::test(migrations = "./migrations")]
async fn filter_title_contains_escapes_folded_metacharacters(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    insert_book(&ingestion_pool, "pct", "50% Discount Manual").await;
    insert_book(&ingestion_pool, "bslash", r"Back\slash Path").await;
    insert_book(&ingestion_pool, "other", "Unrelated Title").await;
    let server = server_with_page_size(&app_pool, &ingestion_pool, 50);

    // unaccent folds fullwidth ％ (U+FF05) to ASCII %. Folding must not
    // re-materialize a live wildcard out of the needle: the folded % matches
    // the title carrying a literal %, and nothing else.
    let hits = filtered_titles(&server, &basic, "/api/v1/books?title_contains=%EF%BC%85").await;
    assert_eq!(hits, vec!["50% Discount Manual"]);

    // Fullwidth ＿ (U+FF3F) folds to _. As a literal it matches no title; as
    // a smuggled single-character wildcard it would match every title.
    let hits = filtered_titles(&server, &basic, "/api/v1/books?title_contains=%EF%BC%BF").await;
    assert_eq!(hits, Vec::<String>::new());

    // Fullwidth ＼ (U+FF3C) folds to \ and must itself be escaped: it matches
    // the literal backslash in the stored title.
    let hits = filtered_titles(&server, &basic, "/api/v1/books?title_contains=%EF%BC%BC").await;
    assert_eq!(hits, vec![r"Back\slash Path"]);

    // A folded ＼ directly before an ASCII % must not neutralize that %'s
    // escape (which would leave a live wildcard matching any title with a
    // backslash). No title contains a literal `\%`, so: no hits.
    let hits = filtered_titles(&server, &basic, "/api/v1/books?title_contains=%EF%BC%BC%25").await;
    assert_eq!(hits, Vec::<String>::new());

    // A needle that folds to nothing (combining marks map to the empty
    // string in unaccent.rules) must match nothing, not everything.
    let hits = filtered_titles(&server, &basic, "/api/v1/books?title_contains=%CC%81").await;
    assert_eq!(hits, Vec::<String>::new());
}

#[sqlx::test(migrations = "./migrations")]
async fn filter_title_eq_and_ne(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    insert_book(&ingestion_pool, "exact", "Exact Title").await;
    insert_book(&ingestion_pool, "exactly", "Exact Title Longer").await;
    let server = server_with_page_size(&app_pool, &ingestion_pool, 50);

    // `_eq` is a whole-value case-insensitive match, not a substring.
    let hits = filtered_titles(&server, &basic, "/api/v1/books?title_eq=exact%20title").await;
    assert_eq!(hits, vec!["Exact Title"]);

    // `_ne` excludes the exact match, keeping the rest.
    let mut hits = filtered_titles(&server, &basic, "/api/v1/books?title_ne=Exact%20Title").await;
    hits.sort();
    assert_eq!(hits, vec!["Exact Title Longer"]);
}

#[sqlx::test(migrations = "./migrations")]
async fn filter_subtitle_empty_both_values(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let (with_sub_work, _m) = insert_book(&ingestion_pool, "subbed", "Has Subtitle").await;
    set_subtitle(&ingestion_pool, with_sub_work, "A Reckoning").await;
    insert_book(&ingestion_pool, "plain", "No Subtitle").await;
    let server = server_with_page_size(&app_pool, &ingestion_pool, 50);

    let empty = filtered_titles(&server, &basic, "/api/v1/books?subtitle_empty=true").await;
    assert_eq!(empty, vec!["No Subtitle"]);
    let present = filtered_titles(&server, &basic, "/api/v1/books?subtitle_empty=false").await;
    assert_eq!(present, vec!["Has Subtitle"]);
    // Contains matches only the row whose subtitle carries the needle.
    let present = filtered_titles(&server, &basic, "/api/v1/books?subtitle_contains=reckon").await;
    assert_eq!(present, vec!["Has Subtitle"]);
}

#[sqlx::test(migrations = "./migrations")]
async fn filter_isbn_13_eq_is_case_insensitive_and_empty(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let (_w, m_isbn) = insert_book(&ingestion_pool, "isbn", "Coded").await;
    set_isbn(&ingestion_pool, m_isbn, "978X000111222").await;
    insert_book(&ingestion_pool, "noisbn", "Uncoded").await;
    let server = server_with_page_size(&app_pool, &ingestion_pool, 50);

    // Exact match, folding case on the `X` check digit.
    let hits = filtered_titles(&server, &basic, "/api/v1/books?isbn_13_eq=978x000111222").await;
    assert_eq!(hits, vec!["Coded"]);
    let missing = filtered_titles(&server, &basic, "/api/v1/books?isbn_13_empty=true").await;
    assert_eq!(missing, vec!["Uncoded"]);
}

#[sqlx::test(migrations = "./migrations")]
async fn filter_pages_band_and_empty(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let (_w1, thin) = insert_book(&ingestion_pool, "thin", "Thin").await;
    set_pages(&ingestion_pool, thin, 120).await;
    let (_w2, mid) = insert_book(&ingestion_pool, "mid", "Middle").await;
    set_pages(&ingestion_pool, mid, 400).await;
    let (_w3, fat) = insert_book(&ingestion_pool, "fat", "Doorstop").await;
    set_pages(&ingestion_pool, fat, 900).await;
    insert_book(&ingestion_pool, "unpaged", "Unpaged").await;
    let server = server_with_page_size(&app_pool, &ingestion_pool, 50);

    let mut band =
        filtered_titles(&server, &basic, "/api/v1/books?pages_gte=200&pages_lte=500").await;
    band.sort();
    assert_eq!(band, vec!["Middle"]);
    // A NULL page count is excluded from a bounded range but caught by `_empty`.
    let empty = filtered_titles(&server, &basic, "/api/v1/books?pages_empty=true").await;
    assert_eq!(empty, vec!["Unpaged"]);
}

#[sqlx::test(migrations = "./migrations")]
async fn filter_created_at_bounds_are_day_inclusive(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    // Three books on three distinct calendar days (UTC).
    let day = |iso: &str| {
        chrono::DateTime::parse_from_rfc3339(iso)
            .unwrap()
            .with_timezone(&chrono::Utc)
    };
    insert_book_at(
        &ingestion_pool,
        "d1",
        "June 29",
        day("2026-06-29T23:59:00Z"),
    )
    .await;
    insert_book_at(
        &ingestion_pool,
        "d2",
        "June 30",
        day("2026-06-30T12:00:00Z"),
    )
    .await;
    insert_book_at(&ingestion_pool, "d3", "July 1", day("2026-07-01T00:01:00Z")).await;
    let server = server_with_page_size(&app_pool, &ingestion_pool, 50);

    // gte is inclusive of the whole start day; lte is inclusive of the whole
    // end day (a same-day upper bound keeps that day's late rows).
    let mut band = filtered_titles(
        &server,
        &basic,
        "/api/v1/books?created_at_gte=2026-06-30&created_at_lte=2026-06-30",
    )
    .await;
    band.sort();
    assert_eq!(band, vec!["June 30"]);

    let mut through_july =
        filtered_titles(&server, &basic, "/api/v1/books?created_at_gte=2026-06-30").await;
    through_july.sort();
    assert_eq!(through_july, vec!["July 1", "June 30"]);

    // A malformed date is a 400 (type-level), not a 422.
    let bad = server
        .get("/api/v1/books?created_at_gte=2026-13-40")
        .add_header(AUTHORIZATION, basic.clone())
        .await;
    assert_eq!(bad.status_code(), StatusCode::BAD_REQUEST, "{}", bad.text());
}

#[sqlx::test(migrations = "./migrations")]
async fn filter_status_unread_pseudo_value_semantics(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    // "No Row" gets no reading_state at all; the other two do.
    insert_book(&ingestion_pool, "norow", "No Row").await;
    let (_w2, rating_only) = insert_book(&ingestion_pool, "ratingonly", "Rating Only").await;
    let (_w3, reading) = insert_book(&ingestion_pool, "reading", "Now Reading").await;
    set_reading(&app_pool, admin, rating_only, None, Some(4)).await;
    set_reading(
        &app_pool,
        admin,
        reading,
        Some(ReadingStatus::Reading),
        None,
    )
    .await;
    let server = server_with_page_size(&app_pool, &ingestion_pool, 50);

    // Unread = no status set: the row with no reading_state AND the row that
    // carries only a rating both qualify; the row with a real status does not.
    let mut unread = filtered_titles(&server, &basic, "/api/v1/books?status_any=unread").await;
    unread.sort();
    assert_eq!(unread, vec!["No Row", "Rating Only"]);

    // A real status matches only its row.
    let now = filtered_titles(&server, &basic, "/api/v1/books?status_any=reading").await;
    assert_eq!(now, vec!["Now Reading"]);
}

#[sqlx::test(migrations = "./migrations")]
async fn filter_status_any_unions_and_none_excludes(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let (_w1, reading) = insert_book(&ingestion_pool, "reading", "Reading Now").await;
    let (_w2, done) = insert_book(&ingestion_pool, "done", "All Done").await;
    let (_w3, quit) = insert_book(&ingestion_pool, "quit", "Gave Up").await;
    set_reading(
        &app_pool,
        admin,
        reading,
        Some(ReadingStatus::Reading),
        None,
    )
    .await;
    set_reading(&app_pool, admin, done, Some(ReadingStatus::Finished), None).await;
    set_reading(&app_pool, admin, quit, Some(ReadingStatus::Abandoned), None).await;
    let server = server_with_page_size(&app_pool, &ingestion_pool, 50);

    let mut union = filtered_titles(
        &server,
        &basic,
        "/api/v1/books?status_any=reading&status_any=finished",
    )
    .await;
    union.sort();
    assert_eq!(union, vec!["All Done", "Reading Now"]);

    let mut kept = filtered_titles(&server, &basic, "/api/v1/books?status_none=abandoned").await;
    kept.sort();
    assert_eq!(kept, vec!["All Done", "Reading Now"]);
}

#[sqlx::test(migrations = "./migrations")]
async fn filter_rating_bounds_and_empty(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let (_w1, low) = insert_book(&ingestion_pool, "low", "Two Stars").await;
    let (_w2, high) = insert_book(&ingestion_pool, "high", "Five Stars").await;
    insert_book(&ingestion_pool, "unrated", "Unrated").await;
    set_reading(&app_pool, admin, low, None, Some(2)).await;
    set_reading(&app_pool, admin, high, None, Some(5)).await;
    let server = server_with_page_size(&app_pool, &ingestion_pool, 50);

    let good = filtered_titles(&server, &basic, "/api/v1/books?rating_gte=4").await;
    assert_eq!(good, vec!["Five Stars"]);
    let unrated = filtered_titles(&server, &basic, "/api/v1/books?rating_empty=true").await;
    assert_eq!(unrated, vec!["Unrated"]);
}

#[sqlx::test(migrations = "./migrations")]
async fn filter_author_triple_is_role_scoped(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    // One book authored by two people; another edited (not authored) by a third.
    let (co_work, _co_m, ursula) =
        insert_book_with_role(&ingestion_pool, "coauth", "Co-Authored", "Ursula", "author").await;
    let gene =
        test_support::db::insert_contributor(&ingestion_pool, co_work, "Gene", "author", 1).await;
    let (_ed_work, _ed_m, editrix) = insert_book_with_role(
        &ingestion_pool,
        "edited",
        "Only Edited",
        "Editrix",
        "editor",
    )
    .await;
    let server = server_with_page_size(&app_pool, &ingestion_pool, 50);

    // all-of: both authors must be attached (only the co-authored book).
    let all_of = filtered_titles(
        &server,
        &basic,
        &format!("/api/v1/books?author={ursula}&author={gene}"),
    )
    .await;
    assert_eq!(all_of, vec!["Co-Authored"]);

    // any-of one author matches; none-of excludes.
    let any_of = filtered_titles(
        &server,
        &basic,
        &format!("/api/v1/books?author_any={ursula}"),
    )
    .await;
    assert_eq!(any_of, vec!["Co-Authored"]);
    let none_of = filtered_titles(
        &server,
        &basic,
        &format!("/api/v1/books?author_none={ursula}"),
    )
    .await;
    assert_eq!(none_of, vec!["Only Edited"]);

    // Role scoping: filtering by an editor-only credit's author id matches no
    // book (the `authors[]` surface and the filter both narrow to `author`).
    let editor_hits =
        filtered_titles(&server, &basic, &format!("/api/v1/books?author={editrix}")).await;
    assert!(
        editor_hits.is_empty(),
        "editor-only credit must not match ?author="
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn filter_q_matches_tsvector_and_ilike_legs(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    insert_book(&ingestion_pool, "leguin", "The Left Hand of Darkness").await;
    insert_book(&ingestion_pool, "gibson", "Neuromancer").await;
    let (accented_work, _m) = insert_book(&ingestion_pool, "zola", "Plain Title").await;
    set_description(&ingestion_pool, accented_work, "M\u{e9}moires d'\u{c9}mile").await;
    let (numword_work, _m2) = insert_book(&ingestion_pool, "edition", "Second Plain Title").await;
    set_description(&ingestion_pool, numword_work, "1\u{e8}re \u{e9}dition").await;
    let server = server_with_page_size(&app_pool, &ingestion_pool, 50);

    // Full-text leg: a two-word websearch hits the tsvector even though the
    // words are not a contiguous substring of the title.
    let ts_hit = filtered_titles(&server, &basic, "/api/v1/books?q=darkness%20left").await;
    assert_eq!(ts_hit, vec!["The Left Hand of Darkness"]);

    // Substring leg: a mid-word fragment the stemmed tsvector never carries
    // still matches via ILIKE.
    let ilike_hit = filtered_titles(&server, &basic, "/api/v1/books?q=euroman").await;
    assert_eq!(ilike_hit, vec!["Neuromancer"]);

    // Folding leg: accented content lives only in the description, so the
    // ILIKE title leg cannot mask a broken tsvector configuration; the
    // unaccented query must match through the folded search_vector alone.
    let fold_hit = filtered_titles(&server, &basic, "/api/v1/books?q=memoires").await;
    assert_eq!(fold_hit, vec!["Plain Title"]);

    // numword tokens fold too: '1ère' (letters plus a digit) classifies as
    // numword, outside the word/hword/hword_part classes, and must still be
    // reachable by its unaccented form.
    let numword_hit = filtered_titles(&server, &basic, "/api/v1/books?q=1ere").await;
    assert_eq!(numword_hit, vec!["Second Plain Title"]);
}

#[sqlx::test(migrations = "./migrations")]
async fn filter_combination_conjunction_and_contradiction_is_empty(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let (target_work, target) = insert_book(&ingestion_pool, "target", "Fantasy Doorstop").await;
    set_subtitle(&ingestion_pool, target_work, "Book One").await;
    set_pages(&ingestion_pool, target, 800).await;
    let (_w2, decoy) = insert_book(&ingestion_pool, "decoy", "Fantasy Novella").await;
    set_pages(&ingestion_pool, decoy, 120).await;
    let server = server_with_page_size(&app_pool, &ingestion_pool, 50);

    // Three families combined with AND narrow to the one row satisfying all.
    let both = filtered_titles(
        &server,
        &basic,
        "/api/v1/books?title_contains=fantasy&pages_gte=500&subtitle_empty=false",
    )
    .await;
    assert_eq!(both, vec!["Fantasy Doorstop"]);

    // Contradictory conditions AND to unsatisfiable: 200 with an empty page.
    let response = server
        .get("/api/v1/books?subtitle_empty=true&subtitle_contains=book")
        .add_header(AUTHORIZATION, basic.clone())
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert!(
        body["items"].as_array().unwrap().is_empty(),
        "contradiction must be an empty page"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn filter_validation_errors_400_and_422(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let server = server_with_page_size(&app_pool, &ingestion_pool, 50);

    let status_of = |url: String| {
        let basic = basic.clone();
        let server = &server;
        async move {
            server
                .get(&url)
                .add_header(AUTHORIZATION, basic)
                .await
                .status_code()
        }
    };

    // 21 author_any ids -> 422 (over the value cap).
    let many = (0..=20)
        .map(|_| format!("author_any={}", Uuid::new_v4()))
        .collect::<Vec<_>>()
        .join("&");
    assert_eq!(
        status_of(format!("/api/v1/books?{many}")).await,
        StatusCode::UNPROCESSABLE_ENTITY
    );

    // 201-char title_contains -> 422 (over the text cap).
    let long = "a".repeat(201);
    assert_eq!(
        status_of(format!("/api/v1/books?title_contains={long}")).await,
        StatusCode::UNPROCESSABLE_ENTITY
    );

    // Unknown status token -> 422.
    assert_eq!(
        status_of("/api/v1/books?status_any=currently_reading".to_owned()).await,
        StatusCode::UNPROCESSABLE_ENTITY
    );

    // Non-integer pages -> 400 (type-level serde rejection).
    assert_eq!(
        status_of("/api/v1/books?pages_gte=lots".to_owned()).await,
        StatusCode::BAD_REQUEST
    );

    // Malformed author uuid -> 400.
    assert_eq!(
        status_of("/api/v1/books?author=not-a-uuid".to_owned()).await,
        StatusCode::BAD_REQUEST
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn filter_cursor_rejects_changed_filter_and_ignores_value_order(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let (w1, thin) = insert_book_at(&ingestion_pool, "thin", "Thin", ts(1)).await;
    set_pages(&ingestion_pool, thin, 100).await;
    let (w2, thick) = insert_book_at(&ingestion_pool, "thick", "Thick", ts(2)).await;
    set_pages(&ingestion_pool, thick, 200).await;
    // Share one author across both books so the `author_any` walk below actually
    // matches and mints a cursor; an unmatched author filter would leave
    // next_cursor null and let the value-order replay silently skip.
    let shared =
        test_support::db::insert_contributor(&ingestion_pool, w1, "Shared", "author", 0).await;
    sqlx::query!(
        "INSERT INTO work_authors (work_id, author_id, role, position) \
         VALUES ($1, $2, ($3::text)::author_role, $4)",
        w2,
        shared,
        "author",
        1,
    )
    .execute(&ingestion_pool)
    .await
    .expect("link shared author to second work");
    // page_size 1 so page 1 yields a cursor mid-walk.
    let server = server_with_page_size(&app_pool, &ingestion_pool, 1);

    let page1 = server
        .get("/api/v1/books?pages_gte=50")
        .add_header(AUTHORIZATION, basic.clone())
        .await;
    assert_eq!(page1.status_code(), StatusCode::OK);
    let body: serde_json::Value = page1.json();
    let nc = body["next_cursor"]
        .as_str()
        .expect("cursor on page 1")
        .to_owned();

    // Replayed under a changed filter: 422 (fingerprint mismatch).
    let changed = server
        .get(&format!("/api/v1/books?pages_gte=150&cursor={nc}"))
        .add_header(AUTHORIZATION, basic.clone())
        .await;
    let problem = test_support::assert_problem(
        &changed,
        problems::VALIDATION,
        StatusCode::UNPROCESSABLE_ENTITY,
    );
    assert!(
        problem["detail"]
            .as_str()
            .unwrap()
            .contains("cursor filter mismatch"),
        "got {problem}"
    );

    // Replayed under the same filter: 200, continuing the walk.
    let same = server
        .get(&format!("/api/v1/books?pages_gte=50&cursor={nc}"))
        .add_header(AUTHORIZATION, basic.clone())
        .await;
    assert_eq!(same.status_code(), StatusCode::OK, "{}", same.text());

    // Value order within a multi-value param does not change the fingerprint:
    // a cursor minted under one ordering validates under the reverse. `shared`
    // matches both books, so at page_size 1 the walk spans two pages and mints
    // a cursor; `other` is an unrelated id that any-of tolerates.
    let other = Uuid::new_v4();
    let minted = server
        .get(&format!(
            "/api/v1/books?author_any={shared}&author_any={other}&pages_gte=50"
        ))
        .add_header(AUTHORIZATION, basic.clone())
        .await;
    assert_eq!(minted.status_code(), StatusCode::OK);
    let body: serde_json::Value = minted.json();
    let nc = body["next_cursor"]
        .as_str()
        .expect("author_any spanning both books must mint a cursor")
        .to_owned();
    let reordered = server
        .get(&format!(
            "/api/v1/books?author_any={other}&author_any={shared}&pages_gte=50&cursor={nc}"
        ))
        .add_header(AUTHORIZATION, basic)
        .await;
    assert_eq!(
        reordered.status_code(),
        StatusCode::OK,
        "reordered values must keep the fingerprint: {}",
        reordered.text()
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn filter_composes_with_pagination_exactly_once(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    // Five matching books plus one that fails the filter; page_size 2 forces a
    // multi-page walk across a boundary.
    for (i, (m, t)) in [
        ("f1", "Filtered Alpha"),
        ("f2", "Filtered Bravo"),
        ("f3", "Filtered Charlie"),
        ("f4", "Filtered Delta"),
        ("f5", "Filtered Echo"),
    ]
    .into_iter()
    .enumerate()
    {
        let seconds = i64::try_from(i).expect("small fixture index");
        let (_w, id) = insert_book_at(&ingestion_pool, m, t, ts(seconds)).await;
        set_pages(&ingestion_pool, id, 300).await;
    }
    let (_w, excluded) = insert_book(&ingestion_pool, "short", "Excluded Short").await;
    set_pages(&ingestion_pool, excluded, 50).await;
    let server = server_with_page_size(&app_pool, &ingestion_pool, 2);

    let walked = walk_all_ids(&server, &basic, "/api/v1/books?pages_gte=100").await;
    let mut walked_sorted = walked.clone();
    walked_sorted.sort();
    walked_sorted.dedup();
    assert_eq!(
        walked.len(),
        walked_sorted.len(),
        "no row seen twice across the filtered walk"
    );
    assert_eq!(
        walked.len(),
        5,
        "every matching row seen once; the short book excluded"
    );
    assert!(
        !walked.contains(&excluded),
        "the sub-100-page book must not appear"
    );
}

// ── external identifiers + ratings projection ────────────────────────────

/// Seed one book with identifiers on both levels plus two provider
/// ratings, via the ingestion pool (enrichment's write path).
async fn seed_external_refs(ingestion_pool: &PgPool, work_id: Uuid, m_id: Uuid) {
    use crate::models::external_identifier::{
        upsert_manifestation_identifier, upsert_work_identifier,
    };
    use crate::models::external_rating::{RatingObservation, upsert_rating};

    upsert_work_identifier(ingestion_pool, work_id, "openlibrary", "OL45804W", None)
        .await
        .expect("seed work id");
    upsert_manifestation_identifier(ingestion_pool, m_id, "googlebooks", "zyTZAAAAYAAJ", None)
        .await
        .expect("seed googlebooks id");
    upsert_manifestation_identifier(ingestion_pool, m_id, "asin", "B004GXAX8C", None)
        .await
        .expect("seed asin id");
    upsert_rating(
        ingestion_pool,
        m_id,
        "googlebooks",
        &RatingObservation::new(4.5, 5.0, 100).expect("valid test rating"),
    )
    .await
    .expect("seed googlebooks rating");
    upsert_rating(
        ingestion_pool,
        m_id,
        "amazon",
        &RatingObservation::new(4.1, 5.0, 2000).expect("valid test rating"),
    )
    .await
    .expect("seed amazon rating");
}

fn id_entry<'a>(items: &'a [serde_json::Value], scheme: &str) -> Option<&'a serde_json::Value> {
    items.iter().find(|e| e["scheme"] == scheme)
}

#[sqlx::test(migrations = "./migrations")]
async fn list_and_detail_surface_external_ids_and_ratings(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let (work_id, m_id) = insert_book(&ingestion_pool, "extref", "Dune").await;
    seed_external_refs(&ingestion_pool, work_id, m_id).await;

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let response = server
        .get("/api/v1/books")
        .add_header(AUTHORIZATION, basic.clone())
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let body: serde_json::Value = response.json();
    let item = &body["items"].as_array().expect("items")[0];

    let ids = item["external_ids"].as_array().expect("external_ids");
    assert_eq!(ids.len(), 3, "both levels surfaced: {ids:?}");
    let ol = id_entry(ids, "openlibrary").expect("openlibrary id");
    assert_eq!(ol["level"], "work");
    assert_eq!(ol["external_id"], "OL45804W");
    let gb = id_entry(ids, "googlebooks").expect("googlebooks id");
    assert_eq!(gb["level"], "manifestation");
    assert_eq!(gb["external_id"], "zyTZAAAAYAAJ");
    assert!(id_entry(ids, "asin").is_some());

    let ratings = item["external_ratings"]
        .as_array()
        .expect("external_ratings");
    assert_eq!(ratings.len(), 2, "one row per provider: {ratings:?}");
    let gb_rating = ratings
        .iter()
        .find(|r| r["source"] == "googlebooks")
        .expect("googlebooks rating");
    assert!((gb_rating["rating"].as_f64().unwrap() - 4.5).abs() < 1e-6);
    assert!((gb_rating["rating_scale"].as_f64().unwrap() - 5.0).abs() < 1e-6);
    assert_eq!(gb_rating["review_count"], 100);
    assert!(gb_rating["fetched_at"].is_string(), "RFC 3339 timestamp");

    // Detail carries the same projection.
    let detail = server
        .get(&format!("/api/v1/books/{m_id}"))
        .add_header(AUTHORIZATION, basic)
        .await;
    assert_eq!(detail.status_code(), StatusCode::OK);
    let detail: serde_json::Value = detail.json();
    assert_eq!(detail["external_ids"].as_array().unwrap().len(), 3);
    assert_eq!(detail["external_ratings"].as_array().unwrap().len(), 2);
}

#[sqlx::test(migrations = "./migrations")]
async fn hidden_provider_absent_from_projection_and_hot_reloads(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let (work_id, m_id) = insert_book(&ingestion_pool, "extvis", "Dune").await;
    seed_external_refs(&ingestion_pool, work_id, m_id).await;

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

    // Hide googlebooks (ids + rating share the key) and asin (ids only;
    // Amazon's rating key is 'amazon' and stays visible).
    let put = server
        .put("/api/v1/settings")
        .add_header(AUTHORIZATION, basic.clone())
        .json(&serde_json::json!({"provider_visibility": {"googlebooks": false, "asin": false}}))
        .await;
    assert_eq!(put.status_code(), StatusCode::OK, "body = {}", put.text());

    let body: serde_json::Value = server
        .get(&format!("/api/v1/books/{m_id}"))
        .add_header(AUTHORIZATION, basic.clone())
        .await
        .json();
    let ids = body["external_ids"].as_array().unwrap();
    assert!(
        id_entry(ids, "googlebooks").is_none(),
        "hidden id leaked: {ids:?}"
    );
    assert!(id_entry(ids, "asin").is_none(), "hidden id leaked: {ids:?}");
    assert!(id_entry(ids, "openlibrary").is_some(), "visible id dropped");
    let ratings = body["external_ratings"].as_array().unwrap();
    assert!(
        ratings.iter().all(|r| r["source"] != "googlebooks"),
        "hiding googlebooks must hide its rating too: {ratings:?}"
    );
    assert!(
        ratings.iter().any(|r| r["source"] == "amazon"),
        "hiding asin must not hide the amazon rating: {ratings:?}"
    );

    // Unhide everything: the next read reflects it without a restart —
    // the projection reads the hot-reloaded settings cache per request.
    let put = server
        .put("/api/v1/settings")
        .add_header(AUTHORIZATION, basic.clone())
        .json(&serde_json::json!({"provider_visibility": {}}))
        .await;
    assert_eq!(put.status_code(), StatusCode::OK);

    let body: serde_json::Value = server
        .get(&format!("/api/v1/books/{m_id}"))
        .add_header(AUTHORIZATION, basic)
        .await
        .json();
    assert_eq!(
        body["external_ids"].as_array().unwrap().len(),
        3,
        "toggling visibility back must restore the projection immediately"
    );
    assert_eq!(body["external_ratings"].as_array().unwrap().len(), 2);
}

#[sqlx::test(migrations = "./migrations")]
async fn list_pagination_unaffected_by_multiple_ids_and_ratings(pool: PgPool) {
    use crate::models::external_identifier::{
        upsert_manifestation_identifier, upsert_work_identifier,
    };
    use crate::models::external_rating::{RatingObservation, upsert_rating};

    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;

    // Three books, each carrying several identifier + rating rows: with the
    // one-to-many tables kept out of the paginated base query, page math
    // must still count distinct books, not joined rows.
    let mut all_ids = Vec::new();
    for (n, marker) in ["pg-a", "pg-b", "pg-c"].iter().enumerate() {
        let (work_id, m_id) = insert_book_at(
            &ingestion_pool,
            marker,
            &format!("Paged {marker}"),
            ts(i64::try_from(n).unwrap()),
        )
        .await;
        upsert_work_identifier(
            &ingestion_pool,
            work_id,
            "openlibrary",
            &format!("OL{n}1W"),
            None,
        )
        .await
        .expect("seed work id");
        upsert_manifestation_identifier(
            &ingestion_pool,
            m_id,
            "googlebooks",
            &format!("vol{n}A"),
            None,
        )
        .await
        .expect("seed gb id");
        upsert_manifestation_identifier(&ingestion_pool, m_id, "goodreads", "5907", None)
            .await
            .expect("seed goodreads id");
        upsert_rating(
            &ingestion_pool,
            m_id,
            "googlebooks",
            &RatingObservation::new(4.0, 5.0, 10).expect("valid test rating"),
        )
        .await
        .expect("seed rating");
        upsert_rating(
            &ingestion_pool,
            m_id,
            "openlibrary",
            &RatingObservation::new(3.5, 5.0, 7).expect("valid test rating"),
        )
        .await
        .expect("seed rating");
        all_ids.push(m_id);
    }

    let server = server_with_page_size(&app_pool, &ingestion_pool, 2);
    let first = server
        .get("/api/v1/books")
        .add_header(AUTHORIZATION, basic.clone())
        .await;
    assert_eq!(first.status_code(), StatusCode::OK);
    let body: serde_json::Value = first.json();
    let items = body["items"].as_array().unwrap();
    assert_eq!(items.len(), 2, "exactly `limit` distinct books per page");
    let distinct: std::collections::HashSet<&str> =
        items.iter().map(|i| i["id"].as_str().unwrap()).collect();
    assert_eq!(distinct.len(), 2, "no duplicated rows from the 1:N tables");
    assert!(body["next_cursor"].is_string(), "a third book remains");

    let walked = walk_all_ids(&server, &basic, "/api/v1/books").await;
    assert_eq!(
        walked.len(),
        3,
        "cursor walk covers every book exactly once"
    );
    let unique: std::collections::HashSet<Uuid> = walked.iter().copied().collect();
    assert_eq!(unique.len(), 3);
    for id in all_ids {
        assert!(unique.contains(&id));
    }
}
