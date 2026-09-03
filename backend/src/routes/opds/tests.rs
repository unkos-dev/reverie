//! Integration tests for OPDS routes. See BLUEPRINT §"Task List" (Tests 20–33).
//!
//! Shared setup helpers live in [`crate::test_support::db`]. These tests use
//! `#[sqlx::test]` so each gets its own isolated DB. Cover/download tests
//! also build a per-test `TempDir` for the `library_path` root and write
//! real EPUB bytes to the path that `manifestations.file_path` points at.
#![expect(
    clippy::items_after_statements,
    clippy::format_collect,
    clippy::cast_possible_wrap,
    clippy::option_if_let_else,
    reason = "test-only module: these are test helper patterns (local struct defs, casts for test fixtures, format-collect for URL construction) not worth refactoring"
)]

use std::collections::HashSet;
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
    let file_path = abs_path.to_string_lossy().into_owned();

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
        "INSERT INTO manifestations \
            (work_id, format, file_path, ingestion_file_hash, current_file_hash, \
             file_size_bytes, ingestion_status, validation_status) \
         VALUES ($1, 'epub'::manifestation_format, $2, $3, $3, $4, \
                 'complete'::ingestion_status, 'clean'::validation_status) \
         RETURNING id",
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
    let server = test_support::db::server_with_opds_enabled(&app_pool, &ingestion_pool, tmp.path());

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

// ── Test 33: search folds accents under unaccent_english ─────────────────

#[sqlx::test(migrations = "./migrations")]
async fn search_folds_accents(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let tmp = tempfile::TempDir::new().unwrap();

    insert_epub_manifestation(&ingestion_pool, tmp.path(), "acc", "\u{c9}mile Zola").await;
    insert_epub_manifestation(&ingestion_pool, tmp.path(), "dec", "Unrelated Title").await;

    let server = test_support::db::server_with_opds_enabled(&app_pool, &ingestion_pool, tmp.path());

    // OPDS assembles its own query, so the unaccent_english coupling between
    // works.search_vector and this endpoint's plainto_tsquery is pinned here,
    // independent of the JSON search surface's tests. Unaccented input finds
    // the accented row.
    let response = server
        .get("/opds/library/search?q=emile%20zola")
        .add_header(AUTHORIZATION, basic.clone())
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let body = std::str::from_utf8(response.as_bytes()).unwrap();
    assert!(body.contains("\u{c9}mile Zola"));
    assert!(!body.contains("Unrelated Title"));

    // Accented input keeps matching (no regression from folding).
    let response = server
        .get("/opds/library/search?q=%C3%89mile")
        .add_header(AUTHORIZATION, basic)
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let body = std::str::from_utf8(response.as_bytes()).unwrap();
    assert!(body.contains("\u{c9}mile Zola"));
    assert!(!body.contains("Unrelated Title"));
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

    let work_id: Uuid = sqlx::query_scalar!(
        "INSERT INTO works (title, sort_title) VALUES ('Outside', 'Outside') RETURNING id",
    )
    .fetch_one(&ingestion_pool)
    .await
    .unwrap();
    let outside_path = outside_abs.to_string_lossy().into_owned();
    let outside_m: Uuid = sqlx::query_scalar!(
        "INSERT INTO manifestations \
            (work_id, format, file_path, ingestion_file_hash, current_file_hash, \
             file_size_bytes, ingestion_status, validation_status) \
         VALUES ($1, 'epub'::manifestation_format, $2, 'outside-hash', 'outside-hash', 13, \
                 'complete'::ingestion_status, 'clean'::validation_status) \
         RETURNING id",
        work_id,
        outside_path,
    )
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
    assert_eq!(cache_ctrl, "private, max-age=86400");
    let etag = response
        .headers()
        .get(axum::http::header::ETAG)
        .expect("cover response carries a strong ETag")
        .to_str()
        .unwrap()
        .to_owned();
    assert!(
        etag.starts_with('"') && etag.ends_with('"'),
        "ETag is a quoted strong validator: {etag}"
    );
    // Vary partitions the per-user private cache so a shared browser cannot
    // replay an RLS-scoped cover across an account switch.
    let vary = response
        .headers()
        .get(axum::http::header::VARY)
        .expect("cover 200 carries Vary")
        .to_str()
        .unwrap();
    assert!(
        vary.contains("Authorization") && vary.contains("Cookie"),
        "Vary covers both credential channels: {vary}"
    );
    let first_bytes = response.as_bytes().to_vec();
    assert!(!first_bytes.is_empty());

    // Cache directory exists with the cover.
    let cache_dir = library_root.join("_covers").join("cache");
    assert!(cache_dir.exists(), "cache dir should be created");
    let entries: Vec<_> = std::fs::read_dir(&cache_dir)
        .unwrap()
        .filter_map(std::result::Result::ok)
        .collect();
    assert!(!entries.is_empty(), "expected at least one cached cover");

    // Second request returns same bytes.
    let response = server
        .get(&format!("/opds/books/{m}/cover"))
        .add_header(AUTHORIZATION, basic.clone())
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);
    assert_eq!(response.as_bytes().to_vec(), first_bytes);

    // Conditional request with the matching ETag → 304 Not Modified, no body.
    let response = server
        .get(&format!("/opds/books/{m}/cover"))
        .add_header(AUTHORIZATION, basic.clone())
        .add_header(axum::http::header::IF_NONE_MATCH, etag.clone())
        .await;
    assert_eq!(response.status_code(), StatusCode::NOT_MODIFIED);
    assert!(response.as_bytes().is_empty(), "304 carries no body");
    assert!(
        response
            .headers()
            .get(axum::http::header::VARY)
            .is_some_and(|v| v.to_str().is_ok_and(|s| s.contains("Cookie"))),
        "304 also carries Vary"
    );

    // Stale/mismatched If-None-Match → 200 with the full body (the
    // revalidate-after-content-change path, e.g. a Step 8 writeback).
    let response = server
        .get(&format!("/opds/books/{m}/cover"))
        .add_header(AUTHORIZATION, basic.clone())
        .add_header(axum::http::header::IF_NONE_MATCH, "\"stale-hash-full\"")
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);
    assert_eq!(response.as_bytes().to_vec(), first_bytes);

    // Thumb variant: served as JPEG, same caching contract (ETag present).
    let response = server
        .get(&format!("/opds/books/{m}/cover/thumb"))
        .add_header(AUTHORIZATION, basic.clone())
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(axum::http::header::CONTENT_TYPE)
            .unwrap()
            .to_str()
            .unwrap(),
        "image/jpeg"
    );
    assert!(
        response.headers().get(axum::http::header::ETAG).is_some(),
        "thumb response carries an ETag"
    );
}

// ── Missing / unreadable cover file status mapping ───────────────────────

async fn insert_manifestation_with_path(
    ingestion_pool: &PgPool,
    file_path: &str,
    title: &str,
) -> Uuid {
    let work_id: Uuid = sqlx::query_scalar!(
        "INSERT INTO works (title, sort_title) VALUES ($1, $1) RETURNING id",
        title,
    )
    .fetch_one(ingestion_pool)
    .await
    .expect("insert work");

    sqlx::query_scalar!(
        "INSERT INTO manifestations \
            (work_id, format, file_path, ingestion_file_hash, current_file_hash, \
             file_size_bytes, ingestion_status, validation_status) \
         VALUES ($1, 'epub'::manifestation_format, $2, $3, $3, $4, \
                 'complete'::ingestion_status, 'clean'::validation_status) \
         RETURNING id",
        work_id,
        file_path,
        "missinghash0000missinghash0000ab",
        1000_i64,
    )
    .fetch_one(ingestion_pool)
    .await
    .expect("insert manifestation")
}

#[sqlx::test(migrations = "./migrations")]
async fn cover_missing_file_returns_404_problem_json(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let tmp = tempfile::TempDir::new().unwrap();
    let library_root = std::fs::canonicalize(tmp.path()).unwrap();

    // The manifestation is RLS-visible, but its EPUB is absent on disk, so the
    // cover cannot be generated or served.
    let missing = library_root.join("gone.epub");
    let missing_path = missing.to_string_lossy().into_owned();
    let m = insert_manifestation_with_path(&ingestion_pool, &missing_path, "Ghost").await;

    let server =
        test_support::db::server_with_opds_enabled(&app_pool, &ingestion_pool, &library_root);

    for suffix in ["cover/thumb", "cover"] {
        let response = server
            .get(&format!("/api/v1/books/{m}/{suffix}"))
            .add_header(AUTHORIZATION, basic.clone())
            .await;
        assert_eq!(
            response.status_code(),
            StatusCode::NOT_FOUND,
            "missing {suffix} file should map to 404, not 500"
        );

        let ct = response
            .headers()
            .get(axum::http::header::CONTENT_TYPE)
            .expect("content-type present")
            .to_str()
            .unwrap();
        assert!(
            ct.contains("application/problem+json"),
            "404 uses the problem+json error shape, got {ct}"
        );

        let json: serde_json::Value = serde_json::from_slice(response.as_bytes()).unwrap();
        assert_eq!(json["status"].as_u64(), Some(404));

        // The negative response is per-credential cacheable so a grid of
        // missing covers is not re-hit on every navigation.
        let cache_ctrl = response
            .headers()
            .get(axum::http::header::CACHE_CONTROL)
            .expect("404 carries Cache-Control")
            .to_str()
            .unwrap();
        assert!(
            cache_ctrl.contains("private"),
            "missing-cover 404 is privately cacheable, got {cache_ctrl}"
        );
        assert!(
            response
                .headers()
                .get(axum::http::header::VARY)
                .is_some_and(|v| v.to_str().is_ok_and(|s| s.contains("Cookie"))),
            "missing-cover 404 partitions its cache by credential"
        );
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn cover_unreadable_file_returns_500(pool: PgPool) {
    use std::os::unix::fs::PermissionsExt;

    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let tmp = tempfile::TempDir::new().unwrap();
    let library_root = std::fs::canonicalize(tmp.path()).unwrap();

    let (_w, m, _, abs_path) =
        insert_epub_manifestation(&ingestion_pool, &library_root, "locked", "Locked").await;

    std::fs::set_permissions(&abs_path, std::fs::Permissions::from_mode(0o000)).unwrap();

    // A root test runner bypasses permission bits, leaving the file readable;
    // that environment cannot exercise the permission-denied path, so skip the
    // assertion rather than fail spuriously.
    if std::fs::read(&abs_path).is_ok() {
        std::fs::set_permissions(&abs_path, std::fs::Permissions::from_mode(0o644)).ok();
        return;
    }

    let server =
        test_support::db::server_with_opds_enabled(&app_pool, &ingestion_pool, &library_root);

    let response = server
        .get(&format!("/api/v1/books/{m}/cover/thumb"))
        .add_header(AUTHORIZATION, basic.clone())
        .await;
    // A permission error is a genuine server-side failure, distinct from a
    // missing file: it must stay a 500, not be masked as a 404.
    assert_eq!(response.status_code(), StatusCode::INTERNAL_SERVER_ERROR);

    // Restore permissions so TempDir cleanup can remove the file.
    std::fs::set_permissions(&abs_path, std::fs::Permissions::from_mode(0o644)).ok();
}

// ── SVG covers rasterize to PNG and serve ────────────────────────────────

async fn insert_manifestation_bytes(
    ingestion_pool: &PgPool,
    library_root: &StdPath,
    marker: &str,
    title: &str,
    epub_bytes: &[u8],
) -> Uuid {
    let dest = library_root.join(format!("{marker}.epub"));
    std::fs::write(&dest, epub_bytes).expect("write epub");
    let abs_path = std::fs::canonicalize(&dest).expect("canonicalize");
    let file_path = abs_path.to_string_lossy().into_owned();

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
        "INSERT INTO manifestations \
            (work_id, format, file_path, ingestion_file_hash, current_file_hash, \
             file_size_bytes, ingestion_status, validation_status) \
         VALUES ($1, 'epub'::manifestation_format, $2, $3, $3, $4, \
                 'complete'::ingestion_status, 'clean'::validation_status) \
         RETURNING id",
        work_id,
        file_path,
        hash,
        epub_bytes.len() as i64,
    )
    .fetch_one(ingestion_pool)
    .await
    .expect("insert manifestation")
}

#[sqlx::test(migrations = "./migrations")]
async fn svg_cover_rasterizes_and_serves(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let tmp = tempfile::TempDir::new().unwrap();
    let library_root = std::fs::canonicalize(tmp.path()).unwrap();

    // SE-realistic: cover.svg references a sibling cover.jpg inside the ZIP.
    let epub = test_support::db::make_minimal_epub_with_svg_cover_sibling_ref("svg-cov");
    let m = insert_manifestation_bytes(
        &ingestion_pool,
        &library_root,
        "svg-cov",
        "SVG Covered",
        &epub,
    )
    .await;

    let server =
        test_support::db::server_with_opds_enabled(&app_pool, &ingestion_pool, &library_root);

    // Thumbnail: the SVG rasterizes to PNG internally, then re-encodes to
    // JPEG for the grid (still no SVG bytes ever served).
    let response = server
        .get(&format!("/api/v1/books/{m}/cover/thumb"))
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

    let body = response.as_bytes().to_vec();
    assert_eq!(
        image::guess_format(&body).unwrap(),
        image::ImageFormat::Jpeg
    );
    let img = image::load_from_memory(&body).expect("decode served jpeg");
    assert!(
        img.width().max(img.height()) <= 300,
        "thumb long edge ≤ 300"
    );

    // Full-size preserves the rasterized PNG for reader-view fidelity.
    let full = server
        .get(&format!("/api/v1/books/{m}/cover"))
        .add_header(AUTHORIZATION, basic.clone())
        .await;
    assert_eq!(full.status_code(), StatusCode::OK);
    assert_eq!(
        full.headers()
            .get(axum::http::header::CONTENT_TYPE)
            .unwrap()
            .to_str()
            .unwrap(),
        "image/png"
    );
    let full_body = full.as_bytes().to_vec();
    assert_eq!(
        image::guess_format(&full_body).unwrap(),
        image::ImageFormat::Png
    );

    let cache_dir = library_root.join("_covers").join("cache");
    let exts: Vec<String> = std::fs::read_dir(&cache_dir)
        .unwrap()
        .filter_map(std::result::Result::ok)
        .filter_map(|e| {
            e.path()
                .extension()
                .and_then(|x| x.to_str())
                .map(str::to_owned)
        })
        .collect();
    assert!(
        exts.iter().any(|x| x == "jpg"),
        "expected a cached .jpg thumbnail, got {exts:?}"
    );
    assert!(
        exts.iter().any(|x| x == "png"),
        "expected a cached .png full cover, got {exts:?}"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn malformed_svg_cover_does_not_serve(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let tmp = tempfile::TempDir::new().unwrap();
    let library_root = std::fs::canonicalize(tmp.path()).unwrap();

    let epub = test_support::db::make_minimal_epub_with_malformed_svg_cover("bad-svg");
    let m = insert_manifestation_bytes(&ingestion_pool, &library_root, "bad-svg", "Bad SVG", &epub)
        .await;

    let server =
        test_support::db::server_with_opds_enabled(&app_pool, &ingestion_pool, &library_root);

    let response = server
        .get(&format!("/api/v1/books/{m}/cover/thumb"))
        .add_header(AUTHORIZATION, basic.clone())
        .await;
    // Malformed SVG → CoverError::Decode → 500 (same as a corrupt raster cover).
    assert_eq!(response.status_code(), StatusCode::INTERNAL_SERVER_ERROR);

    let cache_dir = library_root.join("_covers").join("cache");
    let count = std::fs::read_dir(&cache_dir).map_or(0, Iterator::count);
    assert_eq!(count, 0, "no cover should be cached for a malformed SVG");
}

// ── Regression: cover dual-mount asymmetry ────────────────────────────────

/// The cover handler is dual-mounted and the mount is *asymmetric* — the API
/// side lives at `/api/v1`, the OPDS side stays at `/opds`. Both must keep
/// serving the shared handler. A router-wiring slip (API mount left at the
/// old prefix, or the OPDS mount moved too) would slip past the single-route
/// `/api/v1/books` move test.
#[sqlx::test(migrations = "./migrations")]
async fn cover_dual_mount_serves_api_v1_and_opds(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let tmp = tempfile::TempDir::new().unwrap();
    let library_root = std::fs::canonicalize(tmp.path()).unwrap();

    let (_w, m, _, _) =
        insert_epub_manifestation(&ingestion_pool, &library_root, "dual", "Dual Mounted").await;

    let server =
        test_support::db::server_with_opds_enabled(&app_pool, &ingestion_pool, &library_root);

    // API mount moved to /api/v1 (CurrentUser accepts Basic credentials).
    let api = server
        .get(&format!("/api/v1/books/{m}/cover"))
        .add_header(AUTHORIZATION, basic.clone())
        .await;
    assert_eq!(api.status_code(), StatusCode::OK);

    // OPDS mount stayed put and still serves the same handler alongside it.
    let opds = server
        .get(&format!("/opds/books/{m}/cover"))
        .add_header(AUTHORIZATION, basic.clone())
        .await;
    assert_eq!(opds.status_code(), StatusCode::OK);

    // Old unversioned API path is gone (reserved-prefix fallback → 404).
    let gone = server
        .get(&format!("/api/books/{m}/cover"))
        .add_header(AUTHORIZATION, basic)
        .await;
    assert_eq!(gone.status_code(), StatusCode::NOT_FOUND);
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
        let file_path = format!("/tmp/series-{i}.epub");
        let hash = format!("series-hash-{i}");
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

    let server = test_support::db::server_with_opds_enabled(&app_pool, &ingestion_pool, tmp.path());
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

// ── Test 28: pagination walk of 125 manifestations ───────────────────────

fn extract_manifestation_ids(body: &str) -> Vec<Uuid> {
    let mut out = Vec::new();
    let marker = "<id>urn:reverie:manifestation:";
    let mut rest = body;
    while let Some(start) = rest.find(marker) {
        let tail = &rest[start + marker.len()..];
        if let Some(end) = tail.find("</id>") {
            if let Ok(id) = Uuid::parse_str(&tail[..end]) {
                out.push(id);
            }
            rest = &tail[end..];
        } else {
            break;
        }
    }
    out
}

fn extract_next_href(body: &str) -> Option<String> {
    // Search for a <link rel="next" ... href="..." /> element. The rel and
    // href attribute order is writer-dependent; grep for `rel="next"` then
    // walk back/forward to find the nearest `href="..."` within the same
    // `<link .../>` tag.
    let rel_idx = body.find(r#"rel="next""#)?;
    let link_start = body[..rel_idx].rfind("<link").unwrap_or(0);
    let link_end = body[rel_idx..].find("/>").map(|i| rel_idx + i)?;
    let tag = &body[link_start..link_end];
    let href_start = tag.find(r#"href=""#).map(|i| i + 6)?;
    let href_end = tag[href_start..].find('"')?;
    Some(tag[href_start..href_start + href_end].to_string())
}

#[sqlx::test(migrations = "./migrations")]
async fn pagination_walk_125(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let tmp = tempfile::TempDir::new().unwrap();

    for i in 0..125u32 {
        let marker = format!("p-{i:03}");
        let title = format!("Book {i:03}");
        insert_epub_manifestation(&ingestion_pool, tmp.path(), &marker, &title).await;
    }

    let server = test_support::db::server_with_opds_enabled(&app_pool, &ingestion_pool, tmp.path());

    let mut seen: HashSet<Uuid> = HashSet::new();
    let mut url = "/opds/library/new".to_string();
    let mut walked_pages = 0u32;
    loop {
        // `url` may be an absolute http://host.example/... or a path;
        // TestServer.get takes a path, so strip the origin if present.
        let path = if let Some(idx) = url.find("/opds") {
            &url[idx..]
        } else {
            &url
        };
        let response = server
            .get(path)
            .add_header(AUTHORIZATION, basic.clone())
            .await;
        assert_eq!(response.status_code(), StatusCode::OK);
        let body = std::str::from_utf8(response.as_bytes()).unwrap();

        for id in extract_manifestation_ids(body) {
            assert!(
                seen.insert(id),
                "duplicate manifestation id {id} on page {walked_pages} at {path}"
            );
        }
        walked_pages += 1;
        assert!(walked_pages < 20, "runaway pagination");

        match extract_next_href(body) {
            Some(next) => url = next,
            None => break,
        }
    }

    assert_eq!(
        seen.len(),
        125,
        "expected every manifestation visited exactly once, saw {} over {walked_pages} pages",
        seen.len()
    );
}

// ── Invalid cursor: 422 on every cursor-accepting handler ───────────────

#[sqlx::test(migrations = "./migrations")]
async fn invalid_cursor_returns_422(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (admin_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    // Need an owned shelf so the shelf-scoped cursor path reaches the
    // cursor parser (otherwise we'd 404 on shelf ownership).
    let shelf_id = test_support::db::create_shelf(&app_pool, admin_id, "test-shelf").await;
    // Any author UUID — the query parses the cursor before it runs.
    let author_id = Uuid::new_v4();
    let tmp = tempfile::TempDir::new().unwrap();
    let server = test_support::db::server_with_opds_enabled(&app_pool, &ingestion_pool, tmp.path());

    let bad_cursor = "!!!not-base64url!!!";
    let paths = [
        format!("/opds/library/new?cursor={bad_cursor}"),
        format!("/opds/library/authors/{author_id}?cursor={bad_cursor}"),
        format!("/opds/shelves/{shelf_id}/new?cursor={bad_cursor}"),
    ];
    for path in paths {
        let response = server
            .get(&path)
            .add_header(AUTHORIZATION, basic.clone())
            .await;
        assert_eq!(
            response.status_code(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "expected 422 for invalid cursor at {path}, got {}",
            response.status_code()
        );
    }
}

// ── Empty library: no rel="next" ────────────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn empty_library_has_no_next_link(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let tmp = tempfile::TempDir::new().unwrap();
    let server = test_support::db::server_with_opds_enabled(&app_pool, &ingestion_pool, tmp.path());

    let response = server
        .get("/opds/library/new")
        .add_header(AUTHORIZATION, basic)
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let body = std::str::from_utf8(response.as_bytes()).unwrap();
    assert_eq!(body.matches("<entry>").count(), 0);
    assert!(
        !body.contains(r#"rel="next""#),
        "empty library must not emit a next link"
    );
}

// ── Exactly page_size rows: no rel="next" ───────────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn exact_page_size_has_no_next_link(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let tmp = tempfile::TempDir::new().unwrap();

    // Sentinel for `has_more = rows.len() > page_size`. page_size=50 in
    // server_with_opds_enabled, so insert exactly 50 rows.
    for i in 0..50u32 {
        let title = format!("Book {i:03}");
        let work_id: Uuid = sqlx::query_scalar!(
            "INSERT INTO works (title, sort_title) VALUES ($1, $1) RETURNING id",
            title,
        )
        .fetch_one(&ingestion_pool)
        .await
        .expect("insert work");
        let file_path = format!("/tmp/exact-{i}.epub");
        let hash = format!("exact-hash-{i}");
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

    let server = test_support::db::server_with_opds_enabled(&app_pool, &ingestion_pool, tmp.path());
    let response = server
        .get("/opds/library/new")
        .add_header(AUTHORIZATION, basic)
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let body = std::str::from_utf8(response.as_bytes()).unwrap();
    assert_eq!(body.matches("<entry>").count(), 50);
    assert!(
        !body.contains(r#"rel="next""#),
        "exact page_size must not emit a next link (off-by-one sentinel)"
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
    let server = test_support::db::server_with_opds_enabled(&app_pool, &ingestion_pool, tmp.path());
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

// ── Disabled OPDS: /opds returns 404 ────────────────────────────────────

#[tokio::test]
async fn opds_disabled_returns_404() {
    // test_server() uses test_config() with opds.enabled = false — the
    // router_enabled() gate returns None so /opds isn't mounted.
    let server = test_support::test_server();
    let response = server.get("/opds").await;
    assert_eq!(response.status_code(), StatusCode::NOT_FOUND);
}

// ── Navigation feeds (authors / series) are keyset-paginated ─────────────

/// Seed an author with one visible manifestation so the navigation
/// feeds surface it.
async fn insert_author_with_book(
    ingestion_pool: &PgPool,
    library_root: &StdPath,
    marker: &str,
    author_name: &str,
) {
    let (work_id, _m, _, _) =
        insert_epub_manifestation(ingestion_pool, library_root, marker, marker).await;
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
}

/// Walk a navigation feed via `rel="next"`, asserting each of `names`
/// appears exactly once across the walk; returns the page count.
async fn walk_navigation_feed(
    server: &axum_test::TestServer,
    basic: &str,
    start: &str,
    names: &[&str],
) -> u32 {
    let basic = basic.to_owned();
    let mut seen: Vec<String> = Vec::new();
    let mut url = start.to_string();
    let mut pages = 0u32;
    loop {
        let path = if let Some(idx) = url.find("/opds") {
            url[idx..].to_string()
        } else {
            url.clone()
        };
        let response = server
            .get(&path)
            .add_header(AUTHORIZATION, basic.clone())
            .await;
        assert_eq!(response.status_code(), StatusCode::OK);
        let body = std::str::from_utf8(response.as_bytes()).unwrap().to_owned();
        for name in names {
            if body.contains(name) {
                assert!(
                    !seen.contains(&(*name).to_string()),
                    "{name} duplicated on page {pages}"
                );
                seen.push((*name).to_string());
            }
        }
        pages += 1;
        assert!(pages < 10, "runaway pagination");
        match extract_next_href(&body) {
            Some(next) => url = next,
            None => break,
        }
    }
    assert_eq!(
        seen.len(),
        names.len(),
        "every entry exactly once: {seen:?}"
    );
    pages
}

#[sqlx::test(migrations = "./migrations")]
async fn authors_navigation_feed_paginates(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let tmp = tempfile::TempDir::new().unwrap();

    let authors = ["Alder, Quinn", "Birch, Rowan", "Cedar, Sage"];
    for (i, name) in authors.iter().enumerate() {
        insert_author_with_book(&ingestion_pool, tmp.path(), &format!("nav-a{i}"), name).await;
    }

    let server =
        test_support::db::server_with_opds_page_size(&app_pool, &ingestion_pool, tmp.path(), 2);

    let pages = walk_navigation_feed(&server, &basic, "/opds/library/authors", &authors).await;
    assert_eq!(
        pages, 2,
        "3 authors at page_size=2 must take exactly 2 pages"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn series_navigation_feed_paginates(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let tmp = tempfile::TempDir::new().unwrap();

    let series_names = ["Amber Cycle", "Borealis Saga", "Cinder Sequence"];
    for (i, name) in series_names.iter().enumerate() {
        let (work_id, _m, _, _) = insert_epub_manifestation(
            &ingestion_pool,
            tmp.path(),
            &format!("nav-s{i}"),
            &format!("nav-s{i}"),
        )
        .await;
        let series_id: Uuid = sqlx::query_scalar!(
            "INSERT INTO series (name, sort_name) VALUES ($1, $1) RETURNING id",
            name,
        )
        .fetch_one(&ingestion_pool)
        .await
        .expect("insert series");
        sqlx::query!(
            "INSERT INTO series_works (series_id, work_id, position) VALUES ($1, $2, 1)",
            series_id,
            work_id,
        )
        .execute(&ingestion_pool)
        .await
        .expect("insert series_works");
    }

    let server =
        test_support::db::server_with_opds_page_size(&app_pool, &ingestion_pool, tmp.path(), 2);

    let pages = walk_navigation_feed(&server, &basic, "/opds/library/series", &series_names).await;
    assert_eq!(
        pages, 2,
        "3 series at page_size=2 must take exactly 2 pages"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn navigation_feeds_reject_malformed_cursor(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let tmp = tempfile::TempDir::new().unwrap();
    let server = test_support::db::server_with_opds_enabled(&app_pool, &ingestion_pool, tmp.path());

    for path in [
        "/opds/library/authors?cursor=!!!garbage!!!",
        "/opds/library/series?cursor=!!!garbage!!!",
    ] {
        let response = server
            .get(path)
            .add_header(AUTHORIZATION, basic.clone())
            .await;
        assert_eq!(
            response.status_code(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "expected 422 at {path}"
        );
    }
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
    let server = test_support::db::server_with_opds_enabled(&app_pool, &ingestion_pool, tmp.path());

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
async fn empty_cursor_returns_first_page_not_422(pool: PgPool) {
    // Parser-swap parity (serde_urlencoded → serde_html_form): `?cursor=`
    // (empty) decoded to Some("") → 422 malformed cursor under
    // serde_urlencoded; under serde_html_form it decodes to None, so the feed
    // returns its first page. Assert the success path post-swap.
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let tmp = tempfile::TempDir::new().unwrap();
    let server = test_support::db::server_with_opds_enabled(&app_pool, &ingestion_pool, tmp.path());

    let response = server
        .get("/opds/library/new?cursor=")
        .add_header(AUTHORIZATION, basic)
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);
}
