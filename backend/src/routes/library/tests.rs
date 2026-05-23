//! Integration tests for the `/api/books` list endpoint (11a Task 3).
//!
//! Mirrors [`crate::routes::opds::tests`] — `#[sqlx::test]` per case,
//! real-pool harness via [`crate::test_support::db::server_with_real_pools`].
#![allow(
    clippy::cast_possible_wrap,
    reason = "test-only casts on small fixture sizes"
)]

use axum::http::{StatusCode, header::AUTHORIZATION};
use axum_test::TestServer;
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::problems;
use crate::test_support;

/// Build a real-pool test server with a custom OPDS `page_size`.
/// Mirrors [`test_support::db::server_with_real_pools`] but overrides
/// the page-size knob so pagination-overflow tests can drive small
/// pages without inserting a hundred fixture rows.
fn server_with_page_size(app_pool: &PgPool, ingestion_pool: &PgPool, page_size: u32) -> TestServer {
    use crate::auth::backend::AuthBackend;
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
        oidc_client: test_support::test_oidc_client(),
    };
    let auth_backend = AuthBackend {
        pool: app_pool.clone(),
    };
    TestServer::new(crate::build_router(state, auth_backend))
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
                 'complete'::ingestion_status, 'valid'::validation_status) \
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
        .get("/api/books")
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
        format!("/api/books/{first_id}/cover/thumb"),
    );
    assert!(items[0]["validation_status"].is_string());
    assert!(items[0]["ingestion_status"].is_string());
    assert!(items[0]["enrichment_status"].is_string());
    // created_at must be elided per the wire-format invariant.
    assert!(
        items[0].get("created_at").is_none(),
        "created_at must be #[serde(skip)]'d, got {}",
        items[0]
    );
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
        .get("/api/books")
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
        .get("/api/books")
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

    let response = server.get("/api/books").await;
    test_support::assert_problem(&response, problems::UNAUTHORIZED, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "./migrations")]
async fn list_endpoint_malformed_cursor_returns_422(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

    let response = server
        .get("/api/books?cursor=!!!not-base64url!!!")
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
        .get("/api/books?sort=title")
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
        .get("/api/books")
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
async fn list_endpoint_cross_sort_cursor_rejected(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

    // Mint a recent-tagged cursor, then try to replay it under sort=title.
    let recent = crate::routes::cursor::CursorKey::Recent {
        created_at: time::OffsetDateTime::now_utc(),
        id: Uuid::new_v4(),
    }
    .encode();

    let response = server
        .get(&format!("/api/books?sort=title&cursor={recent}"))
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
        .get("/api/books?sort=author")
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
    let mut url = "/api/books?sort=author".to_string();
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
                url = format!("/api/books?sort=author&cursor={nc}");
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
        .get("/api/books")
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
    let next_url = format!("/api/books?cursor={nc}");

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
    let mut url = "/api/books?sort=title".to_string();
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
            Some(nc) => url = format!("/api/books?sort=title&cursor={nc}"),
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
                         'complete'::ingestion_status, 'valid'::validation_status)",
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
    let mut url = "/api/books?sort=title".to_string();
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
            Some(nc) => url = format!("/api/books?sort=title&cursor={nc}"),
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
