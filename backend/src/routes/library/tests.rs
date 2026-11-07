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

// Data loss boundary: a concurrent write cannot detach list or matched values from their validators.
#[sqlx::test(migrations = "./migrations")]
async fn matched_list_baselines_share_one_snapshot(pool: PgPool) {
    use axum::http::header::ETAG;

    let app = test_support::db::app_pool_for(&pool).await;
    let ing = test_support::db::ingestion_pool_for(&pool).await;
    let (user, basic) = test_support::db::create_admin_and_basic_auth(&app).await;
    let (work, edition) = insert_book(&ing, "snapshots", "Before title").await;
    let (_, unread) = insert_book(&ing, "snapshots-unread", "Unread book").await;
    let server = test_support::db::server_with_real_pools(&app, &ing);
    let metadata_url = format!("/api/v1/books/{edition}/metadata");
    let initial = server
        .get(&metadata_url)
        .add_header(AUTHORIZATION, basic.clone())
        .await;
    let patch = server
        .patch(&metadata_url)
        .add_header(AUTHORIZATION, basic.clone())
        .add_header(axum::http::header::IF_MATCH, initial.header(ETAG))
        .json(
            &serde_json::json!({"tags": ["Before tag"], "genres": ["Before genre"],
            "moods": ["Before mood"], "contributors": {"author": ["Before author"]},
            "work_identifiers": {"openlibrary": "OL123W"}}),
        )
        .await;
    assert_eq!(patch.status_code(), StatusCode::OK, "{}", patch.text());
    sqlx::query!("INSERT INTO reading_state (user_id, manifestation_id, status, rating, notes, progress_pct, started_at, finished_at, last_read_at) VALUES ($1, $2, 'reading', 3, 'Before notes', 25, now(), now(), now())", user, edition).execute(&pool).await.unwrap();
    let before_metadata = server
        .get(&metadata_url)
        .add_header(AUTHORIZATION, basic.clone())
        .await;
    let before_reading = server
        .get(&format!("/api/v1/books/{edition}/reading"))
        .add_header(AUTHORIZATION, basic.clone())
        .await;
    let unread_reading = server
        .get(&format!("/api/v1/books/{unread}/reading"))
        .add_header(AUTHORIZATION, basic.clone())
        .await;
    let before_list = server
        .get("/api/v1/books")
        .add_header(AUTHORIZATION, basic.clone())
        .await
        .json::<serde_json::Value>();
    let before_row = before_list["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == edition.to_string())
        .unwrap();
    assert_eq!(
        before_row["metadata_etag"],
        before_metadata.header(ETAG).to_str().unwrap()
    );
    assert_eq!(
        before_row["reading_etag"],
        before_reading.header(ETAG).to_str().unwrap()
    );
    let unread_row = before_list["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == unread.to_string())
        .unwrap();
    assert!(unread_row["reading_state"].is_null());
    assert_eq!(
        unread_row["reading_etag"],
        unread_reading.header(ETAG).to_str().unwrap()
    );

    let mut writer = pool.begin().await.unwrap();
    sqlx::query!("LOCK TABLE manifestation_tags IN ACCESS EXCLUSIVE MODE")
        .execute(&mut *writer)
        .await
        .unwrap();
    let list = async {
        server
            .get("/api/v1/books")
            .add_header(AUTHORIZATION, basic.clone())
            .await
    };
    let matched = async {
        server
            .get(&metadata_url)
            .add_header(AUTHORIZATION, basic.clone())
            .await
    };
    tokio::pin!(list, matched);
    tokio::select! {
        response = &mut list => panic!("list did not reach the relation barrier: {}", response.text()),
        response = &mut matched => panic!("metadata did not reach the relation barrier: {}", response.text()),
        () = test_support::wait_for_blocked_query(&pool, "%FROM manifestation_tags%", 2) => {}
    }
    sqlx::query!("UPDATE works SET title = 'After title' WHERE id = $1", work)
        .execute(&mut *writer)
        .await
        .unwrap();
    sqlx::query!("UPDATE authors SET name = 'After author' WHERE id IN (SELECT author_id FROM work_authors WHERE work_id = $1)", work).execute(&mut *writer).await.unwrap();
    sqlx::query!("UPDATE tags SET name = 'After tag' WHERE id IN (SELECT tag_id FROM manifestation_tags WHERE manifestation_id = $1)", edition).execute(&mut *writer).await.unwrap();
    sqlx::query!(
        "UPDATE work_external_identifiers SET external_id = 'OL456W' WHERE work_id = $1",
        work
    )
    .execute(&mut *writer)
    .await
    .unwrap();
    sqlx::query!("UPDATE reading_state SET rating = 5, notes = 'After notes', progress_pct = 75, last_read_at = now() WHERE user_id = $1 AND manifestation_id = $2", user, edition).execute(&mut *writer).await.unwrap();
    writer.commit().await.unwrap();
    let (list, matched) = tokio::join!(list, matched);
    assert_eq!(list.status_code(), StatusCode::OK, "{}", list.text());
    assert_eq!(matched.status_code(), StatusCode::OK, "{}", matched.text());
    let list = list.json::<serde_json::Value>();
    let row = list["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == edition.to_string())
        .unwrap();
    assert_eq!(row, before_row);
    assert_eq!(
        matched.json::<serde_json::Value>(),
        before_metadata.json::<serde_json::Value>()
    );
    assert_eq!(matched.header(ETAG), before_metadata.header(ETAG));

    let after_metadata = server
        .get(&metadata_url)
        .add_header(AUTHORIZATION, basic.clone())
        .await;
    let after_reading = server
        .get(&format!("/api/v1/books/{edition}/reading"))
        .add_header(AUTHORIZATION, basic.clone())
        .await;
    let after_list = server
        .get("/api/v1/books")
        .add_header(AUTHORIZATION, basic.clone())
        .await
        .json::<serde_json::Value>();
    let after_row = after_list["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == edition.to_string())
        .unwrap();
    assert_eq!(after_row["title"], "After title");
    assert_eq!(after_row["authors"], serde_json::json!(["After author"]));
    assert_eq!(after_row["tags"], serde_json::json!(["After tag"]));
    assert_eq!(
        after_row["metadata_etag"],
        after_metadata.header(ETAG).to_str().unwrap()
    );
    assert_eq!(
        after_row["reading_etag"],
        after_reading.header(ETAG).to_str().unwrap()
    );
    assert_ne!(after_row["metadata_etag"], before_row["metadata_etag"]);
    assert_ne!(after_row["reading_etag"], before_row["reading_etag"]);
}

/// Build a real-pool test server with a custom OPDS `page_size`.
/// Mirrors [`test_support::db::server_with_real_pools`] but overrides
/// the page-size knob so pagination-overflow tests can drive small
/// pages without inserting a hundred fixture rows.
fn server_with_page_size(app_pool: &PgPool, ingestion_pool: &PgPool, page_size: u32) -> TestServer {
    use crate::config::OpdsConfig;
    use crate::state::AppState;

    let (mut config, library_files) = test_support::test_storage_config(
        None,
        crate::models::storage_library::LibraryId::from_uuid(uuid::Uuid::new_v4()),
    );
    config.opds = OpdsConfig {
        enabled: false,
        page_size,
        realm: "Reverie OPDS".into(),
    };
    let state = AppState {
        pool: app_pool.clone(),
        ingestion_pool: ingestion_pool.clone(),
        ingestion: crate::services::ingestion::coordinator_channel().0,
        library_files,
        hash_limiter: crate::auth::password::HashLimiter::new(
            usize::try_from(config.auth_hash_concurrency).unwrap(),
        ),
        config,
        oidc: Some(std::sync::Arc::new(test_support::test_oidc_runtime())),
        login_limiter: test_support::test_login_limiter(),
        recovery_limiter: test_support::test_login_limiter(),
        oidc_limiter: test_support::test_login_limiter(),
        settings: test_support::test_settings(),
        last_settings_reload: std::sync::Arc::new(tokio::sync::RwLock::new(None)),
        mail: None,
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

    let file_path = format!("fixtures/library-test-{marker}.epub");
    let hash = format!("library-test-hash-{marker}");
    let m_id: Uuid = sqlx::query_scalar!(
        "WITH inserted AS (INSERT INTO manifestations \
            (library_id, work_id, format, file_path, ingestion_file_hash, current_file_hash, \
             file_size_bytes, ingestion_status, validation_status) \
         VALUES ((SELECT id FROM libraries WHERE configuration_key = 'default'), $1, 'epub'::manifestation_format, $2, $3, $3, 1000, \
                 'complete'::ingestion_status, 'clean'::validation_status) \
         RETURNING *), claimed AS (INSERT INTO library_path_claims (library_id, path, manifestation_id) SELECT library_id, file_path, id FROM inserted) SELECT id AS \"id!\" FROM inserted",
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

    let file_path = format!("fixtures/library-test-{marker}.epub");
    let hash = format!("library-test-hash-{marker}");
    let m_id: Uuid = sqlx::query_scalar!(
        "WITH inserted AS (INSERT INTO manifestations \
            (library_id, work_id, format, file_path, ingestion_file_hash, current_file_hash, \
             file_size_bytes, ingestion_status, validation_status, created_at) \
         VALUES ((SELECT id FROM libraries WHERE configuration_key = 'default'), $1, 'epub'::manifestation_format, $2, $3, $3, 1000, \
                 'complete'::ingestion_status, 'clean'::validation_status, $4) \
         RETURNING *), claimed AS (INSERT INTO library_path_claims (library_id, path, manifestation_id) SELECT library_id, file_path, id FROM inserted) SELECT id AS \"id!\" FROM inserted",
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

// ─── multi-column sort stack: boundary walks + error paths ──────────────

#[sqlx::test(migrations = "./migrations")]
async fn list_filter_duplicate_sort_column_returns_422(pool: PgPool) {
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
        problems::VALIDATION,
        StatusCode::UNPROCESSABLE_ENTITY,
    );
    let detail = body["detail"].as_str().expect("detail string");
    assert!(detail.contains("invalid sort"), "got detail: {detail}");
}

#[sqlx::test(migrations = "./migrations")]
async fn list_filter_too_many_sort_levels_returns_422(pool: PgPool) {
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
        problems::VALIDATION,
        StatusCode::UNPROCESSABLE_ENTITY,
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
async fn detail_endpoint_omits_pending_versions_for_child_caller(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, admin_basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let (child_id, child_basic) =
        test_support::db::create_child_user_and_basic_auth(&app_pool, "kid").await;

    let m_id =
        insert_book_with_author(&ingestion_pool, "kid-draft", "Shelved Tome", "Doe, Jane").await;
    let shelf = test_support::db::create_shelf(&app_pool, child_id, "kid-shelf").await;
    test_support::db::add_to_shelf(&app_pool, shelf, m_id).await;

    sqlx::query!(
        "INSERT INTO metadata_versions \
            (manifestation_id, source, field_name, new_value, value_hash, match_type, \
             status, confidence_score) \
         VALUES ($1, 'opf', 'description', to_jsonb('a proposal'::text), \
                 decode('00000001', 'hex'), 'title', 'pending'::metadata_review_status, 0.5)",
        m_id,
    )
    .execute(&ingestion_pool)
    .await
    .expect("seed one pending version");

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

    let response = server
        .get(&format!("/api/v1/books/{m_id}"))
        .add_header(AUTHORIZATION, admin_basic)
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(
        body["metadata_version_summary"]["pending"], 1,
        "an adult sees the pending proposal, got {body}"
    );

    let response = server
        .get(&format!("/api/v1/books/{m_id}"))
        .add_header(AUTHORIZATION, child_basic)
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let body: serde_json::Value = response.json();
    assert_eq!(
        body["metadata_version_summary"]["pending"], 0,
        "a child sees no pending proposal, got {body}"
    );
    assert!(
        body["metadata_versions"].as_array().unwrap().is_empty(),
        "a child receives no pending versions, got {body}"
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

// ---------------------------------------------------------------------------
// work_endpoint — GET /api/v1/works/{id}
// ---------------------------------------------------------------------------

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

#[sqlx::test(migrations = "./migrations")]
async fn list_filter_by_shelf_scoped_to_caller(pool: PgPool) {
    // REV-REQ-0059: scalar and multi-shelf filters cannot reveal foreign memberships.
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let (other_id, _other_basic) =
        test_support::db::create_adult_and_basic_auth(&app_pool, "other-adult").await;

    let (_w1, m1) = insert_book(&ingestion_pool, "a", "Caller Book").await;
    let (_w2, m2) = insert_book(&ingestion_pool, "b", "Other Book").await;

    let my_shelf = test_support::db::create_shelf(&app_pool, admin_id, "mine").await;
    let second_shelf = test_support::db::create_shelf(&app_pool, admin_id, "second").await;
    let other_shelf = test_support::db::create_shelf(&app_pool, other_id, "theirs").await;
    test_support::db::add_to_shelf(&app_pool, my_shelf, m1).await;
    test_support::db::add_to_shelf(&app_pool, second_shelf, m2).await;
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
        .add_header(AUTHORIZATION, basic.clone())
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

    let owned = server
        .get(&format!(
            "/api/v1/books?shelf_any={my_shelf}&shelf_any={second_shelf}"
        ))
        .add_header(AUTHORIZATION, basic.clone())
        .await;
    assert_eq!(owned.status_code(), StatusCode::OK);
    let mut ids: Vec<Uuid> = owned.json::<serde_json::Value>()["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| serde_json::from_value(item["id"].clone()).unwrap())
        .collect();
    ids.sort_unstable();
    let mut expected = vec![m1, m2];
    expected.sort_unstable();
    assert_eq!(ids, expected);

    for unavailable in [other_shelf, Uuid::new_v4()] {
        let mixed = server
            .get(&format!(
                "/api/v1/books?shelf_any={my_shelf}&shelf_any={unavailable}"
            ))
            .add_header(AUTHORIZATION, basic.clone())
            .await;
        assert_eq!(mixed.status_code(), StatusCode::OK);
        assert_eq!(
            mixed.json::<serde_json::Value>()["items"],
            serde_json::json!([])
        );
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn list_filter_unknown_sort_field_returns_422(pool: PgPool) {
    // `?sort=` is validated in-handler against the `SortColumn`
    // whitelist (`SortSpec::parse`) after the query string decodes: a
    // field outside the whitelist must surface as RFC 9457 422, not a
    // silent default-sort fallthrough.
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
        problems::VALIDATION,
        StatusCode::UNPROCESSABLE_ENTITY,
    );
    let detail = body["detail"].as_str().expect("detail string");
    assert!(detail.contains("invalid sort"), "got detail: {detail}");
}

// ─── 11b — search endpoint ───────────────────────────────────────────────

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

// ---------------------------------------------------------------------------
// Typed per-column filters + quick search.
// ---------------------------------------------------------------------------

use crate::models::reading_status::ReadingStatus;

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
async fn isbn_partial_filter_accepts_separators(pool: PgPool) {
    // Incident: formatted partial searches missed compact stored ISBN-13 values.
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let (_, target_id) = insert_book(&ingestion_pool, "isbn-partial", "ISBN target").await;
    sqlx::query!(
        "UPDATE manifestations SET isbn_13 = $1 WHERE id = $2",
        "9781639739134",
        target_id,
    )
    .execute(&ingestion_pool)
    .await
    .expect("set ISBN-13");
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let response = server
        .get("/api/v1/books?isbn_13_contains=978-%20163")
        .add_header(AUTHORIZATION, basic)
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let body: serde_json::Value = response.json();
    let ids: Vec<Uuid> = body["items"]
        .as_array()
        .expect("items array")
        .iter()
        .map(|item| item["id"].as_str().expect("item id").parse().expect("UUID"))
        .collect();
    assert_eq!(ids, vec![target_id]);
}

#[sqlx::test(migrations = "./migrations")]
async fn filter_created_at_inclusive_lower_exclusive_upper_and_cursor_identity(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    for (marker, title, instant) in [
        ("before", "Before", "2026-06-29T15:59:59.999999Z"),
        ("lower", "Lower", "2026-06-29T16:00:00Z"),
        ("inside", "Inside", "2026-06-30T15:59:59.999999Z"),
        ("upper", "Upper", "2026-06-30T16:00:00Z"),
    ] {
        insert_book_at(
            &ingestion_pool,
            marker,
            title,
            chrono::DateTime::parse_from_rfc3339(instant)
                .unwrap()
                .with_timezone(&chrono::Utc),
        )
        .await;
    }
    let server = server_with_page_size(&app_pool, &ingestion_pool, 50);
    for (query, expected) in [
        (
            "created_at_gte=2026-06-29T16:00:00Z&created_at_lt=2026-06-30T16:00:00Z",
            vec!["Inside", "Lower"],
        ),
        (
            "created_at_gte=2026-06-29T16:00:00Z",
            vec!["Inside", "Lower", "Upper"],
        ),
        (
            "created_at_lt=2026-06-30T16:00:00Z",
            vec!["Before", "Inside", "Lower"],
        ),
        (
            "created_at_gte=2026-06-30T16:00:00Z&created_at_lt=2026-06-30T16:00:00Z",
            vec![],
        ),
        (
            "created_at_gte=2026-07-01T00:00:00Z&created_at_lt=2026-06-30T16:00:00Z",
            vec![],
        ),
    ] {
        let mut titles = filtered_titles(&server, &basic, &format!("/api/v1/books?{query}")).await;
        titles.sort();
        assert_eq!(titles, expected, "{query}");
    }
    let server = server_with_page_size(&app_pool, &ingestion_pool, 1);
    let first = server
        .get("/api/v1/books?created_at_gte=2026-06-29T16:00:00Z&created_at_lt=2026-06-30T16:00:00Z")
        .add_header(AUTHORIZATION, basic.clone())
        .await;
    assert_eq!(first.status_code(), StatusCode::OK);
    let body: serde_json::Value = first.json();
    let cursor = body["next_cursor"]
        .as_str()
        .expect("two matches mint a cursor");
    let same = server.get(&format!("/api/v1/books?created_at_gte=2026-06-30T00:00:00%2B08:00&created_at_lt=2026-07-01T00:00:00%2B08:00&cursor={cursor}"))
        .add_header(AUTHORIZATION, basic.clone()).await;
    assert_eq!(same.status_code(), StatusCode::OK, "{}", same.text());
    let next: serde_json::Value = same.json();
    assert!(next["next_cursor"].is_null());
    let mut titles = vec![
        body["items"][0]["title"].as_str().unwrap(),
        next["items"][0]["title"].as_str().unwrap(),
    ];
    titles.sort_unstable();
    assert_eq!(titles, vec!["Inside", "Lower"]);
    let changed = server.get(&format!("/api/v1/books?created_at_gte=2026-06-29T16:00:01Z&created_at_lt=2026-06-30T16:00:00Z&cursor={cursor}"))
        .add_header(AUTHORIZATION, basic).await;
    assert_eq!(changed.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test(migrations = "./migrations")]
async fn filter_numeric_equal_bounds_match_and_inverted_bounds_are_empty(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let (_, id) = insert_book(&ingestion_pool, "exact", "Exact").await;
    set_pages(&ingestion_pool, id, 500).await;
    set_reading(&app_pool, admin, id, None, Some(4)).await;
    let server = server_with_page_size(&app_pool, &ingestion_pool, 50);
    for query in ["pages_gte=500&pages_lte=500", "rating_gte=4&rating_lte=4"] {
        assert_eq!(
            filtered_titles(&server, &basic, &format!("/api/v1/books?{query}")).await,
            vec!["Exact"]
        );
    }
    for query in ["pages_gte=501&pages_lte=500", "rating_gte=5&rating_lte=4"] {
        assert_eq!(
            filtered_titles(&server, &basic, &format!("/api/v1/books?{query}")).await,
            Vec::<String>::new()
        );
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn filter_cursor_rejects_changed_filter_and_ignores_value_order(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
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
    let series = sqlx::query_scalar!(
        "INSERT INTO series (name, sort_name) VALUES ($1, $1) RETURNING id",
        "Shared series",
    )
    .fetch_one(&ingestion_pool)
    .await
    .unwrap();
    sqlx::query!(
        "INSERT INTO series_works (series_id, work_id) VALUES ($1, $2), ($3, $4)",
        series,
        w1,
        series,
        w2,
    )
    .execute(&ingestion_pool)
    .await
    .unwrap();
    let shelf = test_support::db::create_shelf(&app_pool, admin, "Both books").await;
    let empty_shelf = test_support::db::create_shelf(&app_pool, admin, "Empty").await;
    test_support::db::add_to_shelf(&app_pool, shelf, thin).await;
    test_support::db::add_to_shelf(&app_pool, shelf, thick).await;
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
    let arrays =
        format!("shelf_any={shelf}&shelf_any={empty_shelf}&series_any={series}&series_any={other}");
    let minted = server
        .get(&format!(
            "/api/v1/books?author_any={shared}&author_any={other}&pages_gte=50&{arrays}"
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
            "/api/v1/books?author_any={other}&author_any={shared}&pages_gte=50&shelf_any={empty_shelf}&shelf_any={shelf}&series_any={other}&series_any={series}&cursor={nc}"
        ))
        .add_header(AUTHORIZATION, basic.clone())
        .await;
    assert_eq!(
        reordered.status_code(),
        StatusCode::OK,
        "reordered values must keep the fingerprint: {}",
        reordered.text()
    );
    // REV-REQ-0057: each changed predicate rejects the cursor without returning items.
    for changed_arrays in [
        format!("shelf_any={shelf}&series_any={series}&series_any={other}"),
        format!("shelf_any={shelf}&shelf_any={empty_shelf}&series_any={series}"),
    ] {
        let changed = server
            .get(&format!(
                "/api/v1/books?author_any={shared}&author_any={other}&pages_gte=50&{changed_arrays}&cursor={nc}"
            ))
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
                .contains("cursor filter mismatch")
        );
        assert!(problem.get("items").is_none());
    }
}

// ── external identifiers + ratings projection ────────────────────────────
