//! Integration tests for the `/api/books*` and `/api/works/{id}`
//! endpoints (11a Tasks 3, 5, 7).
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
        settings: test_support::test_settings(),
        last_settings_reload: std::sync::Arc::new(tokio::sync::RwLock::new(None)),
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

// ---------------------------------------------------------------------------
// detail_endpoint — GET /api/books/{id}
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
        .get(&format!("/api/books/{m_id}"))
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
        format!("/api/books/{m_id}/cover/thumb"),
    );
    assert!(body["ingestion_status"].is_string());
    assert!(body["enrichment_status"].is_string());
    assert!(body["validation_status"].is_string());
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
        .get(&format!("/api/books/{m_id}"))
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
async fn detail_endpoint_hidden_id_returns_404(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_child, basic) =
        test_support::db::create_child_user_and_basic_auth(&app_pool, "hidden").await;

    // Insert a book the child cannot see (not on any shelf of theirs).
    let (_w, m_id) = insert_book(&ingestion_pool, "hidden", "Forbidden Tome").await;

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let response = server
        .get(&format!("/api/books/{m_id}"))
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
        .get("/api/books/not-a-uuid")
        .add_header(AUTHORIZATION, basic)
        .await;
    // axum 0.8 default `Path<Uuid>` rejection: 400 plain-text body.
    assert_eq!(response.status_code(), StatusCode::BAD_REQUEST);
}

// ---------------------------------------------------------------------------
// work_endpoint — GET /api/works/{id}
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
                     'complete'::ingestion_status, 'valid'::validation_status) \
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
        .get(&format!("/api/works/{work_id}"))
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
            format!("/api/books/{mid}/cover/thumb"),
        );
        assert!(m["ingestion_status"].is_string());
    }
    assert!(
        body["series"].is_null(),
        "no series seeded → series must surface null, got {body}",
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn work_endpoint_malformed_uuid_returns_400(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

    let response = server
        .get("/api/works/not-a-uuid")
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
        .get(&format!("/api/works/{}", Uuid::new_v4()))
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
        .get(&format!("/api/works/{work_id}"))
        .add_header(AUTHORIZATION, basic)
        .await;
    test_support::assert_problem(&response, problems::NOT_FOUND, StatusCode::NOT_FOUND);
}

// ─── 11b — list filters ──────────────────────────────────────────────────

/// Insert a tag and link it to a manifestation via `manifestation_tags`.
async fn tag_book(ingestion_pool: &PgPool, manifestation_id: Uuid, tag_name: &str) {
    let tag_id: Uuid = sqlx::query_scalar!(
        "INSERT INTO tags (name, tag_type) VALUES ($1, 'genre'::tag_type) \
         ON CONFLICT (name, tag_type) DO UPDATE SET name = EXCLUDED.name \
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
        .get(&format!("/api/books?author={author_id}"))
        .add_header(AUTHORIZATION, basic)
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let body: serde_json::Value = response.json();
    let items = body["items"].as_array().expect("items");
    assert_eq!(items.len(), 1, "only Gibson's book should match");
    assert_eq!(items[0]["title"], "Neuromancer");
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
        .get(&format!("/api/books?series={series_id}"))
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
        .get(&format!("/api/books?shelf={my_shelf}"))
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
        .get(&format!("/api/books?shelf={other_shelf}"))
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
        .get(&format!("/api/books?{qs}"))
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
        .get("/api/books?tag=scifi&tag=hugo")
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
        .get("/api/search?q=galaxy")
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
        .get("/api/search?q=Hemingwy")
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
async fn search_websearch_phrase_hits_phrase(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;

    insert_book(&ingestion_pool, "wp", "War and Peace").await;
    insert_book(&ingestion_pool, "ak", "Anna Karenina").await;
    insert_book(&ingestion_pool, "rd", "Resurrection Detail").await;

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let r = server
        .get("/api/search?q=%22war+and+peace%22")
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
        .get("/api/search?q=tolstoy+-anna")
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
        .get("/api/search?q=")
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
        .get("/api/search")
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
        .get(&format!("/api/search?q={long_q}"))
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
        .get("/api/search?q=%27%29%3B+DROP+TABLE+works%3B--")
        .add_header(AUTHORIZATION, basic)
        .await;

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
    let r = server.get("/api/search?q=anything").await;
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
        .get("/api/search?q=story")
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
                     'complete'::ingestion_status, 'valid'::validation_status)",
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
            .get(&format!("/api/search?q={q}"))
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
