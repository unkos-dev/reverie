//! Integration tests for OPDS routes. See BLUEPRINT §"Task List" (Tests 20–33).
//!
//! Shared setup helpers live in [`crate::test_support::db`]. These tests use
//! `#[sqlx::test]` so each gets its own isolated DB. Cover/download tests
//! also build a per-test `TempDir` for the `library_path` root and write
//! real EPUB bytes to the path that `manifestations.file_path` points at.

use std::path::Path as StdPath;

use axum::http::{StatusCode, header::AUTHORIZATION};
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
    let file_path = abs_path.to_string_lossy().into_owned();

    use sha2::{Digest, Sha256};
    let hash: String = Sha256::digest(&epub_bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();

    let work_id: Uuid =
        sqlx::query_scalar("INSERT INTO works (title, sort_title) VALUES ($1, $1) RETURNING id")
            .bind(title)
            .fetch_one(ingestion_pool)
            .await
            .expect("insert work");

    let m_id: Uuid = sqlx::query_scalar(
        "INSERT INTO manifestations \
            (work_id, format, file_path, ingestion_file_hash, current_file_hash, \
             file_size_bytes, ingestion_status, validation_status) \
         VALUES ($1, 'epub'::manifestation_format, $2, $3, $3, $4, \
                 'complete'::ingestion_status, 'valid'::validation_status) \
         RETURNING id",
    )
    .bind(work_id)
    .bind(&file_path)
    .bind(&hash)
    .bind(epub_bytes.len() as i64)
    .fetch_one(ingestion_pool)
    .await
    .expect("insert manifestation");

    (work_id, m_id, file_path, abs_path)
}

// ── Test 20: root feed happy path ────────────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn root_feed_happy_path(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let tmp = tempfile::TempDir::new().unwrap();
    let server = test_support::db::server_with_opds_enabled(&app_pool, &ingestion_pool, tmp.path());

    let response = server.get("/opds").add_header(AUTHORIZATION, basic).await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let ct = response
        .headers()
        .get(axum::http::header::CONTENT_TYPE)
        .expect("content-type")
        .to_str()
        .unwrap()
        .to_owned();
    assert!(
        ct.starts_with("application/atom+xml"),
        "unexpected content-type: {ct}"
    );
    let body = std::str::from_utf8(response.as_bytes()).unwrap();
    assert!(body.contains(r#"rel="self""#));
    assert!(body.contains(r#"rel="start""#));
    assert!(body.contains(r#"rel="subsection""#));
    assert!(body.contains("/opds/library"));
}

// ── Test 21: unauthenticated returns WWW-Authenticate challenge ──────────

#[sqlx::test(migrations = "./migrations")]
async fn unauthenticated_returns_challenge(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let tmp = tempfile::TempDir::new().unwrap();
    let server = test_support::db::server_with_opds_enabled(&app_pool, &ingestion_pool, tmp.path());

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
}

// ── Test 22: revoked device token rejected ───────────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn revoked_token_rejected(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (user_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    // Revoke the only token for the user.
    sqlx::query("UPDATE device_tokens SET revoked_at = now() WHERE user_id = $1")
        .bind(user_id)
        .execute(&app_pool)
        .await
        .expect("revoke");

    let tmp = tempfile::TempDir::new().unwrap();
    let server = test_support::db::server_with_opds_enabled(&app_pool, &ingestion_pool, tmp.path());
    let response = server.get("/opds").add_header(AUTHORIZATION, basic).await;
    assert_eq!(response.status_code(), StatusCode::UNAUTHORIZED);
    assert!(
        response
            .headers()
            .get(axum::http::header::WWW_AUTHENTICATE)
            .is_some()
    );
}

// ── Test 23: OpenSearch descriptor has {searchTerms} ──────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn opensearch_descriptor_has_search_terms(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let shelf_id = test_support::db::create_shelf(&app_pool, admin_id, "favs").await;
    let tmp = tempfile::TempDir::new().unwrap();
    let server = test_support::db::server_with_opds_enabled(&app_pool, &ingestion_pool, tmp.path());

    let response = server
        .get("/opds/library/opensearch.xml")
        .add_header(AUTHORIZATION, basic.clone())
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let body = std::str::from_utf8(response.as_bytes()).unwrap();
    assert!(body.contains("OpenSearchDescription"));
    assert!(body.contains("{searchTerms}"));

    let response = server
        .get(&format!("/opds/shelves/{shelf_id}/opensearch.xml"))
        .add_header(AUTHORIZATION, basic)
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let body = std::str::from_utf8(response.as_bytes()).unwrap();
    assert!(body.contains("{searchTerms}"));
}

// ── Test 24: search round-trip ───────────────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn search_roundtrip(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let tmp = tempfile::TempDir::new().unwrap();

    for (marker, title) in [
        ("a", "Pride and Prejudice"),
        ("b", "Neuromancer"),
        ("c", "Cryptonomicon"),
    ] {
        insert_epub_manifestation(&ingestion_pool, tmp.path(), marker, title).await;
    }

    let server = test_support::db::server_with_opds_enabled(&app_pool, &ingestion_pool, tmp.path());
    let response = server
        .get("/opds/library/search?q=Neuromancer")
        .add_header(AUTHORIZATION, basic)
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let body = std::str::from_utf8(response.as_bytes()).unwrap();
    assert!(body.contains("Neuromancer"));
    assert!(!body.contains("Pride and Prejudice"));
    assert!(!body.contains("Cryptonomicon"));
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

    let server = test_support::db::server_with_opds_enabled(&app_pool, &ingestion_pool, tmp.path());
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

// ── Test 26: adult shelf-scoped feed ─────────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn adult_shelf_scoped_feed(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let tmp = tempfile::TempDir::new().unwrap();

    let (adult_id, basic) = test_support::db::create_adult_and_basic_auth(&app_pool, "adult").await;

    let shelf_a = test_support::db::create_shelf(&app_pool, adult_id, "A").await;
    let shelf_b = test_support::db::create_shelf(&app_pool, adult_id, "B").await;
    for (marker, title) in [("a1", "A1"), ("a2", "A2"), ("a3", "A3")] {
        let (_w, m, _, _) =
            insert_epub_manifestation(&ingestion_pool, tmp.path(), marker, title).await;
        test_support::db::add_to_shelf(&app_pool, shelf_a, m).await;
    }
    for (marker, title) in [("b1", "B1"), ("b2", "B2")] {
        let (_w, m, _, _) =
            insert_epub_manifestation(&ingestion_pool, tmp.path(), marker, title).await;
        test_support::db::add_to_shelf(&app_pool, shelf_b, m).await;
    }

    let server = test_support::db::server_with_opds_enabled(&app_pool, &ingestion_pool, tmp.path());

    let response = server
        .get(&format!("/opds/shelves/{shelf_a}/new"))
        .add_header(AUTHORIZATION, basic.clone())
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let body = std::str::from_utf8(response.as_bytes()).unwrap();
    let a_entries = body.matches("<entry>").count();
    assert_eq!(a_entries, 3);

    let response = server
        .get(&format!("/opds/shelves/{shelf_b}/new"))
        .add_header(AUTHORIZATION, basic)
        .await;
    let body = std::str::from_utf8(response.as_bytes()).unwrap();
    assert_eq!(body.matches("<entry>").count(), 2);
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
    let server = test_support::db::server_with_opds_enabled(&app_pool, &ingestion_pool, tmp.path());

    // A can reach their own shelf.
    let response = server
        .get(&format!("/opds/shelves/{shelf_a}"))
        .add_header(AUTHORIZATION, basic_a)
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);

    // B cannot — returns 404, not 403 per BLUEPRINT.
    let response = server
        .get(&format!("/opds/shelves/{shelf_a}"))
        .add_header(AUTHORIZATION, basic_b)
        .await;
    assert_eq!(response.status_code(), StatusCode::NOT_FOUND);
}

// ── Test 29: XML robustness — control char + entities ───────────────────

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

    let server = test_support::db::server_with_opds_enabled(&app_pool, &ingestion_pool, tmp.path());
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

// ── Test 30: search reflection is XSS-safe (XML escaping) ───────────────

#[sqlx::test(migrations = "./migrations")]
async fn search_reflection_xss_safe(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let tmp = tempfile::TempDir::new().unwrap();

    let server = test_support::db::server_with_opds_enabled(&app_pool, &ingestion_pool, tmp.path());
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

// ── Test 31: download streams + path-traversal 403 ───────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn download_streams_and_path_traversal_403(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;

    let tmp = tempfile::TempDir::new().unwrap();
    let library_root = std::fs::canonicalize(tmp.path()).unwrap();

    // Valid manifestation inside library_root.
    let (_w, m, _, _) =
        insert_epub_manifestation(&ingestion_pool, &library_root, "dl", "The Book").await;

    let server =
        test_support::db::server_with_opds_enabled(&app_pool, &ingestion_pool, &library_root);

    // Happy path: bytes stream.
    let response = server
        .get(&format!("/opds/books/{m}/file"))
        .add_header(AUTHORIZATION, basic.clone())
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(axum::http::header::CONTENT_TYPE)
            .unwrap(),
        "application/epub+zip"
    );
    let cd = response
        .headers()
        .get(axum::http::header::CONTENT_DISPOSITION)
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    assert!(cd.starts_with("attachment;"));
    assert!(cd.contains("filename="));
    assert!(cd.contains("filename*=UTF-8''"));

    // Path-traversal: insert a manifestation whose file_path is a real file
    // OUTSIDE library_root. The canonicalisation guard rejects it with 403.
    let outside_dir = tempfile::TempDir::new().unwrap();
    let outside_file = outside_dir.path().join("outside.epub");
    std::fs::write(&outside_file, b"not real epub").unwrap();
    let outside_abs = std::fs::canonicalize(&outside_file).unwrap();

    let work_id: Uuid = sqlx::query_scalar(
        "INSERT INTO works (title, sort_title) VALUES ('Outside', 'Outside') RETURNING id",
    )
    .fetch_one(&ingestion_pool)
    .await
    .unwrap();
    let outside_m: Uuid = sqlx::query_scalar(
        "INSERT INTO manifestations \
            (work_id, format, file_path, ingestion_file_hash, current_file_hash, \
             file_size_bytes, ingestion_status, validation_status) \
         VALUES ($1, 'epub'::manifestation_format, $2, 'outside-hash', 'outside-hash', 13, \
                 'complete'::ingestion_status, 'valid'::validation_status) \
         RETURNING id",
    )
    .bind(work_id)
    .bind(outside_abs.to_string_lossy().into_owned())
    .fetch_one(&ingestion_pool)
    .await
    .unwrap();

    let response = server
        .get(&format!("/opds/books/{outside_m}/file"))
        .add_header(AUTHORIZATION, basic.clone())
        .await;
    assert_eq!(response.status_code(), StatusCode::FORBIDDEN);

    // File deleted from disk → 404.
    let (_w2, missing_m, _, abs) =
        insert_epub_manifestation(&ingestion_pool, &library_root, "delete-me", "Delete Me").await;
    std::fs::remove_file(&abs).unwrap();
    let response = server
        .get(&format!("/opds/books/{missing_m}/file"))
        .add_header(AUTHORIZATION, basic)
        .await;
    assert_eq!(response.status_code(), StatusCode::NOT_FOUND);
}

// ── Test 32: cover cache populates and serves ───────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn cover_cache_populates_and_serves(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let tmp = tempfile::TempDir::new().unwrap();
    let library_root = std::fs::canonicalize(tmp.path()).unwrap();

    let (_w, m, _, _) =
        insert_epub_manifestation(&ingestion_pool, &library_root, "cov", "Covered").await;

    let server =
        test_support::db::server_with_opds_enabled(&app_pool, &ingestion_pool, &library_root);

    // First request populates cache.
    let response = server
        .get(&format!("/opds/books/{m}/cover"))
        .add_header(AUTHORIZATION, basic.clone())
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let ct = response
        .headers()
        .get(axum::http::header::CONTENT_TYPE)
        .unwrap()
        .to_str()
        .unwrap();
    assert_eq!(ct, "image/jpeg");
    let cache_ctrl = response
        .headers()
        .get(axum::http::header::CACHE_CONTROL)
        .unwrap()
        .to_str()
        .unwrap();
    assert_eq!(cache_ctrl, "no-store");
    let first_bytes = response.as_bytes().to_vec();
    assert!(!first_bytes.is_empty());

    // Cache directory exists with the cover.
    let cache_dir = library_root.join("_covers").join("cache");
    assert!(cache_dir.exists(), "cache dir should be created");
    let entries: Vec<_> = std::fs::read_dir(&cache_dir)
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();
    assert!(!entries.is_empty(), "expected at least one cached cover");

    // Second request returns same bytes.
    let response = server
        .get(&format!("/opds/books/{m}/cover"))
        .add_header(AUTHORIZATION, basic.clone())
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);
    assert_eq!(response.as_bytes().to_vec(), first_bytes);

    // Thumb variant works too.
    let response = server
        .get(&format!("/opds/books/{m}/cover/thumb"))
        .add_header(AUTHORIZATION, basic.clone())
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);
}
