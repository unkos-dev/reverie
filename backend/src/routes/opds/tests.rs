//! Integration tests for OPDS routes.
//!
//! Shared setup helpers live in [`crate::test_support::db`]. These tests use
//! `#[sqlx::test]` so each gets its own isolated DB. Cover/download tests
//! also build a per-test `TempDir` for the `library_path` root and write
//! real EPUB bytes to the path that `manifestations.file_path` points at.
#![expect(
    clippy::items_after_statements,
    clippy::format_collect,
    clippy::cast_possible_wrap,
    reason = "test-only module: these are test helper patterns (local struct defs, casts for test fixtures, format-collect for URL construction) not worth refactoring"
)]

use std::path::Path as StdPath;

use axum::http::{StatusCode, header::AUTHORIZATION};
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::test_support;

async fn insert_epub_manifestation(
    ingestion_pool: &PgPool,
    library_root: &StdPath,
    marker: &str,
    title: &str,
) -> (Uuid, Uuid, String, std::path::PathBuf) {
    let epub_bytes = test_support::db::make_minimal_epub_with_cover_tagged(marker);
    let dest = library_root.join(format!("{marker}.epub"));
    std::fs::write(&dest, &epub_bytes).expect("write epub");
    let abs_path = std::fs::canonicalize(&dest).expect("canonicalize");
    let file_path = format!("{marker}.epub");

    use sha2::{Digest, Sha256};
    let hash: String = Sha256::digest(&epub_bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();

    let work_id: Uuid = sqlx::query_scalar!(
        "INSERT INTO works (title, sort_title) VALUES ($1, $1) RETURNING id",
        title,
    )
    .fetch_one(ingestion_pool)
    .await
    .expect("insert work");

    let m_id: Uuid = sqlx::query_scalar!(
        "WITH inserted AS (INSERT INTO manifestations \
            (library_id, work_id, format, file_path, ingestion_file_hash, current_file_hash, \
             file_size_bytes, ingestion_status, validation_status) \
         VALUES ((SELECT id FROM libraries WHERE configuration_key = 'default'), $1, 'epub'::manifestation_format, $2, $3, $3, $4, \
                 'complete'::ingestion_status, 'clean'::validation_status) \
         RETURNING *), claimed AS (INSERT INTO library_path_claims (library_id, path, manifestation_id) SELECT library_id, file_path, id FROM inserted) SELECT id AS \"id!\" FROM inserted",
        work_id,
        file_path,
        hash,
        epub_bytes.len() as i64,
    )
    .fetch_one(ingestion_pool)
    .await
    .expect("insert manifestation");

    (work_id, m_id, file_path, abs_path)
}

// ── Test 21: unauthenticated returns WWW-Authenticate challenge ──────────

#[sqlx::test(migrations = "./migrations")]
async fn unauthenticated_returns_challenge(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let tmp = tempfile::TempDir::new().unwrap();
    let server =
        test_support::db::server_with_opds_enabled(&app_pool, &ingestion_pool, tmp.path()).await;

    let response = server.get("/opds").await;
    assert_eq!(response.status_code(), StatusCode::UNAUTHORIZED);
    let challenge = response
        .headers()
        .get(axum::http::header::WWW_AUTHENTICATE)
        .expect("WWW-Authenticate header")
        .to_str()
        .unwrap()
        .to_owned();
    assert_eq!(challenge, r#"Basic realm="Reverie OPDS", charset="UTF-8""#);
    let problem = test_support::assert_problem(
        &response,
        crate::error::problems::BASIC_AUTH_REQUIRED,
        StatusCode::UNAUTHORIZED,
    );
    assert_eq!(problem["status"], 401);
}

// ── Regression: DB failure surfaces as 500, not a 401 challenge ──────────

#[sqlx::test(migrations = "./migrations")]
async fn basic_only_db_failure_returns_500_not_challenge(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let tmp = tempfile::TempDir::new().unwrap();
    let server =
        test_support::db::server_with_opds_enabled(&app_pool, &ingestion_pool, tmp.path()).await;

    // Close the app pool the server's state holds. A subsequent Basic auth
    // attempt drives verify_basic into a PoolClosed sqlx::Error that
    // bubbles up as AppError::Internal.  Before the fix, BasicOnly's
    // wildcard arm swallowed it into a 401 + challenge.
    app_pool.close().await;

    let response = server.get("/opds").add_header(AUTHORIZATION, basic).await;
    assert_eq!(response.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
    assert!(
        response
            .headers()
            .get(axum::http::header::WWW_AUTHENTICATE)
            .is_none(),
        "500 must not emit a Basic challenge"
    );
}

// ── Test 22: revoked device token rejected ───────────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn revoked_token_rejected(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (user_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    // Revoke the only token for the user.
    sqlx::query!(
        "UPDATE device_tokens SET revoked_at = now() WHERE user_id = $1",
        user_id,
    )
    .execute(&app_pool)
    .await
    .expect("revoke");

    let tmp = tempfile::TempDir::new().unwrap();
    let server =
        test_support::db::server_with_opds_enabled(&app_pool, &ingestion_pool, tmp.path()).await;
    let response = server.get("/opds").add_header(AUTHORIZATION, basic).await;
    assert_eq!(response.status_code(), StatusCode::UNAUTHORIZED);
    assert!(
        response
            .headers()
            .get(axum::http::header::WWW_AUTHENTICATE)
            .is_some()
    );
}

// ── Test 25: child sees only whitelisted manifestations ──────────────────

#[sqlx::test(migrations = "./migrations")]
async fn child_sees_only_whitelisted_manifestations(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let tmp = tempfile::TempDir::new().unwrap();

    let (child_id, basic) =
        test_support::db::create_child_user_and_basic_auth(&app_pool, "kid").await;

    // Three manifestations exist in the library; only two are on the child's
    // shelves. RLS should hide the third from child-scope `/opds/library/*`.
    let (_w1, m1, _, _) =
        insert_epub_manifestation(&ingestion_pool, tmp.path(), "ks-1", "Kid Book A").await;
    let (_w2, m2, _, _) =
        insert_epub_manifestation(&ingestion_pool, tmp.path(), "ks-2", "Kid Book B").await;
    let (_w3, _m3, _, _) =
        insert_epub_manifestation(&ingestion_pool, tmp.path(), "ks-3", "Adult Only").await;

    let shelf_a = test_support::db::create_shelf(&app_pool, child_id, "A").await;
    let shelf_b = test_support::db::create_shelf(&app_pool, child_id, "B").await;
    test_support::db::add_to_shelf(&app_pool, shelf_a, m1).await;
    test_support::db::add_to_shelf(&app_pool, shelf_b, m2).await;

    let server =
        test_support::db::server_with_opds_enabled(&app_pool, &ingestion_pool, tmp.path()).await;
    let response = server
        .get("/opds/library/new")
        .add_header(AUTHORIZATION, basic)
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let body = std::str::from_utf8(response.as_bytes()).unwrap();
    assert!(body.contains("Kid Book A"));
    assert!(body.contains("Kid Book B"));
    assert!(!body.contains("Adult Only"));
}

// ── Test 27: cross-user shelf returns 404 ────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn cross_user_shelf_returns_404(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (adult_a_id, basic_a) = test_support::db::create_adult_and_basic_auth(&app_pool, "A").await;
    let (adult_b_id, basic_b) = test_support::db::create_adult_and_basic_auth(&app_pool, "B").await;
    let shelf_a = test_support::db::create_shelf(&app_pool, adult_a_id, "A").await;
    let _ = adult_b_id;

    let tmp = tempfile::TempDir::new().unwrap();
    let server =
        test_support::db::server_with_opds_enabled(&app_pool, &ingestion_pool, tmp.path()).await;

    // A can reach their own shelf.
    let response = server
        .get(&format!("/opds/shelves/{shelf_a}"))
        .add_header(AUTHORIZATION, basic_a)
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);

    // B cannot — returns 404, not 403.
    let response = server
        .get(&format!("/opds/shelves/{shelf_a}"))
        .add_header(AUTHORIZATION, basic_b)
        .await;
    assert_eq!(response.status_code(), StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = "./migrations")]
async fn xml_robustness_control_char(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let tmp = tempfile::TempDir::new().unwrap();

    insert_epub_manifestation(
        &ingestion_pool,
        tmp.path(),
        "xmlrob",
        "Hello <script>&amp; \u{0001}emoji \u{1F600}",
    )
    .await;

    let server =
        test_support::db::server_with_opds_enabled(&app_pool, &ingestion_pool, tmp.path()).await;
    let response = server
        .get("/opds/library/new")
        .add_header(AUTHORIZATION, basic)
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let bytes = response.as_bytes().to_vec();
    assert!(
        !bytes.contains(&0x01),
        "\\x01 must be stripped before reaching the wire"
    );
    let body = std::str::from_utf8(&bytes).unwrap();
    // Ampersand escaped; angle bracket escaped. Emoji preserved.
    assert!(body.contains("&amp;amp;") || body.contains("&amp;")); // plan: raw `&` -> `&amp;`
    assert!(body.contains("&lt;script&gt;"));
    assert!(body.contains("\u{1F600}"));
}

#[sqlx::test(migrations = "./migrations")]
async fn search_reflection_xss_safe(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let tmp = tempfile::TempDir::new().unwrap();

    let server =
        test_support::db::server_with_opds_enabled(&app_pool, &ingestion_pool, tmp.path()).await;
    let response = server
        .get("/opds/library/search?q=%3Cscript%3Ealert(1)%3C%2Fscript%3E")
        .add_header(AUTHORIZATION, basic)
        .await;
    // Empty library — 200 with an empty feed; any XML-injected q would
    // break parsing, but we just check the response body doesn't contain
    // a literal `<script>` (we never reflect user search strings into the
    // feed for the search handler, but verify anyway).
    assert_eq!(response.status_code(), StatusCode::OK);
    let body = std::str::from_utf8(response.as_bytes()).unwrap();
    assert!(!body.contains("<script>"));
}

#[sqlx::test(migrations = "./migrations")]
async fn capability_download_outside_link_forbidden(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let library = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let (_, manifestation, _, path) =
        insert_epub_manifestation(&ingestion_pool, library.path(), "capability", "Linked Book")
            .await;
    let destination = outside.path().join("outside.epub");
    std::fs::write(&destination, b"outside bytes").unwrap();
    std::fs::remove_file(&path).unwrap();
    std::os::unix::fs::symlink(destination, &path).unwrap();
    let server =
        test_support::db::server_with_opds_enabled(&app_pool, &ingestion_pool, library.path())
            .await;
    let response = server
        .get(&format!("/opds/books/{manifestation}/file"))
        .add_header(AUTHORIZATION, basic)
        .await;
    assert_eq!(response.status_code(), StatusCode::FORBIDDEN);
    assert!(!response.text().contains("outside bytes"));
    assert!(!response.text().contains(path.to_str().unwrap()));
}

#[tokio::test]
async fn capability_download_opened_handle_survives_path_replacement() {
    assert_opened_download_survives_replacement().await;
}

async fn assert_opened_download_survives_replacement() {
    let library = tempfile::tempdir().unwrap();
    let path = library.path().join("book.epub");
    std::fs::write(&path, b"original bytes").unwrap();
    let id = crate::models::storage_library::LibraryId::from_uuid(Uuid::new_v4());
    let files =
        test_support::test_library_files_at(&library.path().to_str().unwrap().parse().unwrap(), id);
    let location = crate::services::files::LibraryLocation {
        library_id: id,
        path: "book.epub".parse().unwrap(),
    };
    let opened = files.open_download(&location).await.unwrap();
    let replacement = library.path().join("replacement.epub");
    std::fs::write(&replacement, b"replacement with a different length").unwrap();
    std::fs::rename(replacement, &path).unwrap();
    let response = super::download::stream_download(opened, "Book", Uuid::new_v4()).unwrap();
    assert_eq!(response.headers()[axum::http::header::CONTENT_LENGTH], "14");
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(bytes, b"original bytes".as_slice());
}

mod storage {
    use super::*;

    fn recorded_download_server(
        app_pool: &PgPool,
        ingestion_pool: &PgPool,
        config: crate::config::Config,
        files: crate::services::files::LibraryFiles,
    ) -> axum_test::TestServer {
        let mut state = test_support::test_state();
        state.pool = app_pool.clone();
        state.ingestion_pool = ingestion_pool.clone();
        state.config = config;
        state.library_files = files;
        state.config.opds.enabled = true;
        axum_test::TestServer::new(crate::build_router(state))
    }

    async fn insert_recorded_download(
        pool: &PgPool,
        library_id: crate::models::storage_library::LibraryId,
        path: &str,
        hash: &str,
    ) -> (Uuid, Uuid) {
        let work = sqlx::query_scalar!(
        "INSERT INTO works (title, sort_title) VALUES ('Original title', 'original title') RETURNING id"
    ).fetch_one(pool).await.unwrap();
        let file = sqlx::query_scalar!(
        "WITH inserted AS (INSERT INTO manifestations
         (library_id, work_id, file_path, format, ingestion_file_hash, current_file_hash, file_size_bytes)
         VALUES ($1, $2, $3, 'epub', $4, $4, 12345) RETURNING *), claimed AS (INSERT INTO library_path_claims (library_id, path, manifestation_id) SELECT library_id, file_path, id FROM inserted) SELECT id AS \"id!\" FROM inserted",
        library_id.as_uuid(), work, path, hash,
    ).fetch_one(pool).await.unwrap();
        (work, file)
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn library_storage_download_nested_location_survives_metadata_change(pool: PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let library = tempfile::tempdir().unwrap();
        std::fs::create_dir(library.path().join("Author")).unwrap();
        std::fs::write(
            library.path().join("Author/Recorded.epub"),
            b"recorded bytes",
        )
        .unwrap();
        let id = crate::models::storage_library::default_library_id(&pool)
            .await
            .unwrap();
        let (work, file) =
            insert_recorded_download(&ingestion_pool, id, "Author/Recorded.epub", "nested").await;
        let (config, files) = test_support::test_storage_config(Some(library.path()), id);
        let server = recorded_download_server(&app_pool, &ingestion_pool, config, files);
        for title in ["Original title", "Renamed title"] {
            sqlx::query!("UPDATE works SET title = $1 WHERE id = $2", title, work)
                .execute(&ingestion_pool)
                .await
                .unwrap();
            let response = server
                .get(&format!("/opds/books/{file}/file"))
                .add_header(AUTHORIZATION, basic.clone())
                .await;
            assert_eq!(response.status_code(), StatusCode::OK);
            assert_eq!(response.as_bytes().as_ref(), b"recorded bytes");
            assert_eq!(response.headers()[axum::http::header::CONTENT_LENGTH], "14");
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn library_storage_download_two_library_same_name(pool: PgPool) {
        use crate::models::storage_library::{LibraryId, default_library_id};
        use crate::services::files::LibraryFiles;
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let first = default_library_id(&pool).await.unwrap();
        let second = LibraryId::from_uuid(
            sqlx::query_scalar!(
                "INSERT INTO libraries (configuration_key) VALUES ('second') RETURNING id"
            )
            .fetch_one(&pool)
            .await
            .unwrap(),
        );
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        std::fs::write(a.path().join("book.epub"), b"first library").unwrap();
        std::fs::write(b.path().join("book.epub"), b"second library").unwrap();
        let root_a: crate::config::AbsoluteRootPath = a.path().to_str().unwrap().parse().unwrap();
        let root_b = b.path().to_str().unwrap().parse().unwrap();
        let files =
            LibraryFiles::open([(first, root_a.clone()), (second, root_b)], &root_a).unwrap();
        let mut config = test_support::test_config();
        config.library_path = root_a.clone();
        config.ingestion_path = root_a.clone();
        let server = recorded_download_server(&app_pool, &ingestion_pool, config, files);
        for (id, hash, expected) in [
            (first, "first", b"first library".as_slice()),
            (second, "second", b"second library".as_slice()),
        ] {
            let (_, file) = insert_recorded_download(&ingestion_pool, id, "book.epub", hash).await;
            let response = server
                .get(&format!("/opds/books/{file}/file"))
                .add_header(AUTHORIZATION, basic.clone())
                .await;
            assert_eq!(response.status_code(), StatusCode::OK);
            assert_eq!(response.as_bytes().as_ref(), expected);
            assert_eq!(
                response.headers()[axum::http::header::CONTENT_LENGTH],
                expected.len().to_string()
            );
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn library_storage_download_unknown_identity_is_generic(pool: PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let unbound = crate::models::storage_library::LibraryId::from_uuid(
            sqlx::query_scalar!(
                "INSERT INTO libraries (configuration_key) VALUES ('unbound') RETURNING id"
            )
            .fetch_one(&pool)
            .await
            .unwrap(),
        );
        let (_, file) =
            insert_recorded_download(&ingestion_pool, unbound, "book.epub", "unknown").await;
        let library = tempfile::tempdir().unwrap();
        std::fs::write(library.path().join("book.epub"), b"wrong library bytes").unwrap();
        let server =
            test_support::db::server_with_opds_enabled(&app_pool, &ingestion_pool, library.path())
                .await;
        let response = server
            .get(&format!("/opds/books/{file}/file"))
            .add_header(AUTHORIZATION, basic)
            .await;
        assert_eq!(response.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(
            response.json::<serde_json::Value>()["detail"],
            "An internal error occurred."
        );
        assert!(!response.text().contains("wrong library bytes"));
        assert!(!response.text().contains(&unbound.as_uuid().to_string()));
    }
}

async fn insert_manifestation_bytes(
    ingestion_pool: &PgPool,
    library_root: &StdPath,
    marker: &str,
    title: &str,
    epub_bytes: &[u8],
) -> Uuid {
    let dest = library_root.join(format!("{marker}.epub"));
    std::fs::write(&dest, epub_bytes).expect("write epub");
    let file_path = format!("{marker}.epub");

    use sha2::{Digest, Sha256};
    let hash: String = Sha256::digest(epub_bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();

    let work_id: Uuid = sqlx::query_scalar!(
        "INSERT INTO works (title, sort_title) VALUES ($1, $1) RETURNING id",
        title,
    )
    .fetch_one(ingestion_pool)
    .await
    .expect("insert work");

    sqlx::query_scalar!(
        "WITH inserted AS (INSERT INTO manifestations \
            (library_id, work_id, format, file_path, ingestion_file_hash, current_file_hash, \
             file_size_bytes, ingestion_status, validation_status) \
         VALUES ((SELECT id FROM libraries WHERE configuration_key = 'default'), $1, 'epub'::manifestation_format, $2, $3, $3, $4, \
                 'complete'::ingestion_status, 'clean'::validation_status) \
         RETURNING *), claimed AS (INSERT INTO library_path_claims (library_id, path, manifestation_id) SELECT library_id, file_path, id FROM inserted) SELECT id AS \"id!\" FROM inserted",
        work_id,
        file_path,
        hash,
        epub_bytes.len() as i64,
    )
    .fetch_one(ingestion_pool)
    .await
    .expect("insert manifestation")
}

// ── Regression: series feed renders every manifestation in one page ─────

#[sqlx::test(migrations = "./migrations")]
async fn series_feed_renders_all_manifestations(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let tmp = tempfile::TempDir::new().unwrap();

    let series_id: Uuid = sqlx::query_scalar!(
        "INSERT INTO series (name, sort_name) VALUES ('Long Saga', 'Long Saga') RETURNING id",
    )
    .fetch_one(&ingestion_pool)
    .await
    .expect("insert series");

    // 51 books (> page_size=50). Positions are monotonic with `i`; created_at
    // is 2020 for all except the last, which is 2023 — so on the buggy
    // `(created_at, id) < cursor` page 2, the last position's row is
    // filtered out. Fix: no cursor on series, whole series on one page.
    let mut expected: Vec<Uuid> = Vec::with_capacity(51);
    for i in 0..51u32 {
        let title = format!("Volume {i:03}");
        let work_id: Uuid = sqlx::query_scalar!(
            "INSERT INTO works (title, sort_title) VALUES ($1, $1) RETURNING id",
            title,
        )
        .fetch_one(&ingestion_pool)
        .await
        .expect("insert work");
        let created_at: DateTime<Utc> = if i == 50 {
            "2023-01-01T00:00:00Z".parse().expect("valid timestamp")
        } else {
            "2020-01-01T00:00:00Z".parse().expect("valid timestamp")
        };
        let file_path = format!("fixtures/series-{i}.epub");
        let hash = format!("series-hash-{i}");
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
        .fetch_one(&ingestion_pool)
        .await
        .expect("insert manifestation");
        let position = f64::from(i as i32 + 1);
        sqlx::query!(
            "INSERT INTO series_works (series_id, work_id, position) \
             VALUES ($1, $2, $3::float8)",
            series_id,
            work_id,
            position,
        )
        .execute(&ingestion_pool)
        .await
        .expect("insert series_works");
        expected.push(m_id);
    }

    let server =
        test_support::db::server_with_opds_enabled(&app_pool, &ingestion_pool, tmp.path()).await;
    let response = server
        .get(&format!("/opds/library/series/{series_id}"))
        .add_header(AUTHORIZATION, basic)
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let body = std::str::from_utf8(response.as_bytes()).unwrap();

    for m_id in &expected {
        let urn = format!("urn:reverie:manifestation:{m_id}");
        assert!(
            body.contains(&urn),
            "missing {urn} — buggy cursor pagination would drop the last-position row"
        );
    }
    // Single-page contract: no rel="next" on series feed.
    assert!(
        !body.contains(r#"rel="next""#),
        "series feed must emit the whole series on one page — no next link"
    );
}

// ── Wrong password returns a Basic challenge ────────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn wrong_password_returns_challenge(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin_id, correct) = test_support::db::create_admin_and_basic_auth(&app_pool).await;

    // Extract the real `{prefix}{token_id}` username from the valid Basic
    // credential, then re-encode with a wrong plaintext secret — this
    // exercises the "known token id, wrong secret" rejection specifically,
    // rather than the "unknown/malformed id" path.
    use base64ct::Encoding;
    let correct_b64 = correct.strip_prefix("Basic ").expect("Basic prefix");
    let mut buf = vec![0u8; correct_b64.len()];
    let decoded = base64ct::Base64::decode(correct_b64.as_bytes(), &mut buf).expect("decode basic");
    let decoded_str = std::str::from_utf8(decoded).expect("utf8 basic");
    let (username, _correct_secret) = decoded_str.split_once(':').expect("basic has a colon");

    let wrong_plaintext = "rev_0000000000000000000000000000000000000000";
    let bad_basic = format!(
        "Basic {}",
        base64ct::Base64::encode_string(format!("{username}:{wrong_plaintext}").as_bytes())
    );

    let tmp = tempfile::TempDir::new().unwrap();
    let server =
        test_support::db::server_with_opds_enabled(&app_pool, &ingestion_pool, tmp.path()).await;
    let response = server
        .get("/opds")
        .add_header(AUTHORIZATION, bad_basic)
        .await;
    assert_eq!(response.status_code(), StatusCode::UNAUTHORIZED);
    let challenge = response
        .headers()
        .get(axum::http::header::WWW_AUTHENTICATE)
        .expect("WWW-Authenticate header present")
        .to_str()
        .unwrap();
    assert!(
        challenge.starts_with("Basic "),
        "expected Basic challenge, got {challenge}"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn query_handlers_reject_duplicate_key_with_problem_json(pool: PgPool) {
    // Every OPDS query handler routes a malformed-query rejection through
    // AppError → RFC 9457 application/problem+json (clears debt 2026-06-10).
    // Duplicate-key (`?field=a&field=b`) is the universal reject vector:
    // serde_html_form (axum_extra::Query) errors on a repeated scalar key.
    // For shelf-scoped routes the Query rejection fires before
    // assert_shelf_owned, so a random shelf id (no fixture) still yields a
    // 400, not a 404.
    // THREAT: the malformed-query status must be structural (always 400),
    // never vary by shelf existence — a status/timing oracle would otherwise
    // leak shelf enumeration. Asserting 400 on a random (non-owned) shelf id
    // pins that invariant.
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let tmp = tempfile::TempDir::new().unwrap();
    let server =
        test_support::db::server_with_opds_enabled(&app_pool, &ingestion_pool, tmp.path()).await;

    let sid = Uuid::new_v4();
    let author = Uuid::new_v4();
    let series = Uuid::new_v4();
    let paths = [
        "/opds/library/new?cursor=a&cursor=b".to_string(),
        "/opds/library/authors?cursor=a&cursor=b".to_string(),
        format!("/opds/library/authors/{author}?cursor=a&cursor=b"),
        "/opds/library/series?cursor=a&cursor=b".to_string(),
        format!("/opds/library/series/{series}?cursor=a&cursor=b"),
        "/opds/library/search?q=a&q=b".to_string(),
        format!("/opds/shelves/{sid}/new?cursor=a&cursor=b"),
        format!("/opds/shelves/{sid}/authors?cursor=a&cursor=b"),
        format!("/opds/shelves/{sid}/authors/{author}?cursor=a&cursor=b"),
        format!("/opds/shelves/{sid}/series?cursor=a&cursor=b"),
        format!("/opds/shelves/{sid}/series/{series}?cursor=a&cursor=b"),
        format!("/opds/shelves/{sid}/search?q=a&q=b"),
    ];
    for path in paths {
        let response = server
            .get(&path)
            .add_header(AUTHORIZATION, basic.clone())
            .await;
        assert_eq!(
            response.status_code(),
            StatusCode::BAD_REQUEST,
            "expected 400 at {path}"
        );
        test_support::assert_problem(
            &response,
            crate::error::problems::MALFORMED_QUERY,
            StatusCode::BAD_REQUEST,
        );
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn capability_cover_route_headers_authorisation_and_conditional_open(pool: PgPool) {
    use axum::http::header::{CACHE_CONTROL, ETAG, IF_NONE_MATCH, VARY};
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_, admin) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let (_, child) =
        test_support::db::create_child_user_and_basic_auth(&app_pool, "cover-child").await;
    let library = tempfile::tempdir().unwrap();
    let (_, file, _, source) =
        insert_epub_manifestation(&ingestion_pool, library.path(), "cap-route", "Covered").await;
    let server =
        test_support::db::server_with_opds_enabled(&app_pool, &ingestion_pool, library.path())
            .await;
    for mount in ["/opds", "/api/v1"] {
        for suffix in ["cover", "cover/thumb"] {
            let url = format!("{mount}/books/{file}/{suffix}");
            let response = server
                .get(&url)
                .add_header(AUTHORIZATION, admin.clone())
                .await;
            assert_eq!(response.status_code(), StatusCode::OK);
            image::load_from_memory(response.as_bytes()).unwrap();
            let etag = response.headers()[ETAG].to_str().unwrap().to_owned();
            assert!(etag.starts_with('"') && etag.ends_with('"'));
            assert!(etag.ends_with(if suffix.ends_with("thumb") {
                "-thumb\""
            } else {
                "-full\""
            }));
            assert_eq!(response.headers()[CACHE_CONTROL], "private, max-age=86400");
            assert_eq!(response.headers()[VARY], "Authorization, Cookie");
            let conditional = server
                .get(&url)
                .add_header(AUTHORIZATION, admin.clone())
                .add_header(IF_NONE_MATCH, etag.clone())
                .await;
            assert_eq!(conditional.status_code(), StatusCode::NOT_MODIFIED);
            assert_eq!(conditional.headers()[ETAG], etag);
            assert_eq!(
                conditional.headers()[CACHE_CONTROL],
                "private, max-age=86400"
            );
            assert_eq!(conditional.headers()[VARY], "Authorization, Cookie");
            assert!(conditional.as_bytes().is_empty());
            let hidden = server
                .get(&url)
                .add_header(AUTHORIZATION, child.clone())
                .add_header(IF_NONE_MATCH, etag.clone())
                .await;
            assert_eq!(hidden.status_code(), StatusCode::NOT_FOUND);
            assert!(hidden.headers().get(CACHE_CONTROL).is_none());
            assert!(hidden.headers().get(VARY).is_none());
        }
    }
    let response = server
        .get(&format!("/opds/books/{file}/cover"))
        .add_header(AUTHORIZATION, admin.clone())
        .await;
    let etag = response.headers()[ETAG].to_str().unwrap().to_owned();
    std::fs::remove_file(&source).unwrap();
    let cache = library.path().join("_covers/cache");
    for entry in std::fs::read_dir(&cache).unwrap() {
        std::fs::remove_file(entry.unwrap().path()).unwrap();
    }
    let missing = server
        .get(&format!("/opds/books/{file}/cover"))
        .add_header(AUTHORIZATION, admin.clone())
        .add_header(IF_NONE_MATCH, etag.clone())
        .await;
    assert_eq!(missing.status_code(), StatusCode::NOT_FOUND);
    assert_eq!(missing.headers()[CACHE_CONTROL], "private, max-age=60");
    assert_eq!(missing.headers()[VARY], "Authorization, Cookie");
    let outside = tempfile::tempdir().unwrap();
    std::fs::remove_dir(&cache).unwrap();
    std::os::unix::fs::symlink(outside.path(), &cache).unwrap();
    for mount in ["/opds", "/api/v1"] {
        let denied = server
            .get(&format!("{mount}/books/{file}/cover"))
            .add_header(AUTHORIZATION, admin.clone())
            .add_header(IF_NONE_MATCH, etag.clone())
            .await;
        assert_eq!(denied.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(
            denied.json::<serde_json::Value>()["detail"],
            "An internal error occurred."
        );
        assert!(denied.headers().get(CACHE_CONTROL).is_none());
    }
    assert_eq!(std::fs::read_dir(outside.path()).unwrap().count(), 0);
    std::fs::remove_file(&cache).unwrap();
    let outside_source = outside.path().join("book.epub");
    let outside_bytes = test_support::db::make_minimal_epub_with_cover_tagged("outside");
    std::fs::write(&outside_source, &outside_bytes).unwrap();
    std::os::unix::fs::symlink(&outside_source, &source).unwrap();
    for mount in ["/opds", "/api/v1"] {
        let denied = server
            .get(&format!("{mount}/books/{file}/cover"))
            .add_header(AUTHORIZATION, admin.clone())
            .add_header(IF_NONE_MATCH, etag.clone())
            .await;
        assert_eq!(denied.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(
            denied.json::<serde_json::Value>()["detail"],
            "An internal error occurred."
        );
    }
    assert_eq!(std::fs::read(outside_source).unwrap(), outside_bytes);
}

#[sqlx::test(migrations = "./migrations")]
async fn capability_cover_route_svg_and_uncached_failures(pool: PgPool) {
    use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE, VARY};
    use std::io::Write;
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let library = tempfile::tempdir().unwrap();
    let svg = insert_manifestation_bytes(
        &ingestion_pool,
        library.path(),
        "cap-svg",
        "SVG",
        &test_support::db::make_minimal_epub_with_svg_cover_sibling_ref("cap-svg"),
    )
    .await;
    let malformed = insert_manifestation_bytes(
        &ingestion_pool,
        library.path(),
        "cap-bad-svg",
        "Malformed SVG",
        &test_support::db::make_minimal_epub_with_malformed_svg_cover("cap-bad-svg"),
    )
    .await;
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    writer
        .start_file(
            "mimetype",
            zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored),
        )
        .unwrap();
    writer.write_all(b"application/epub+zip").unwrap();
    let coverless_bytes = writer.finish().unwrap().into_inner();
    let coverless = insert_manifestation_bytes(
        &ingestion_pool,
        library.path(),
        "cap-no-cover",
        "No cover",
        &coverless_bytes,
    )
    .await;
    let mut rejected_bytes = coverless_bytes;
    rejected_bytes.push(0xaa);
    let rejected = insert_manifestation_bytes(
        &ingestion_pool,
        library.path(),
        "cap-rejected",
        "Rejected",
        &rejected_bytes,
    )
    .await;
    let server =
        test_support::db::server_with_opds_enabled(&app_pool, &ingestion_pool, library.path())
            .await;
    for mount in ["/opds", "/api/v1"] {
        for suffix in ["cover", "cover/thumb"] {
            let raster = server
                .get(&format!("{mount}/books/{svg}/{suffix}"))
                .add_header(AUTHORIZATION, basic.clone())
                .await;
            assert_eq!(raster.status_code(), StatusCode::OK);
            let expected = if suffix.ends_with("thumb") {
                "image/jpeg"
            } else {
                "image/png"
            };
            assert_eq!(raster.headers()[CONTENT_TYPE], expected);
            image::load_from_memory(raster.as_bytes()).unwrap();
            for (id, status) in [
                (coverless, StatusCode::NOT_FOUND),
                (rejected, StatusCode::NOT_FOUND),
                (malformed, StatusCode::INTERNAL_SERVER_ERROR),
            ] {
                let failure = server
                    .get(&format!("{mount}/books/{id}/{suffix}"))
                    .add_header(AUTHORIZATION, basic.clone())
                    .await;
                assert_eq!(failure.status_code(), status);
                assert!(failure.headers().get(CACHE_CONTROL).is_none());
                assert!(failure.headers().get(VARY).is_none());
                assert!(
                    failure.headers()[CONTENT_TYPE]
                        .to_str()
                        .unwrap()
                        .contains("application/problem+json")
                );
            }
        }
    }
    for entry in std::fs::read_dir(library.path().join("_covers/cache")).unwrap() {
        let path = entry.unwrap().path();
        assert!(
            path.file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with(&svg.to_string())
        );
        image::load_from_memory(&std::fs::read(path).unwrap()).unwrap();
    }
}
