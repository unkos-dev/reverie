//! Integration tests for the admin-only `/api/v1/dashboard/*` endpoints
//! (Step 12 / UNK-81).
//!
//! Mirrors [`crate::routes::library::tests`] — `#[sqlx::test]` per case,
//! real-pool harness via [`crate::test_support::db::server_with_real_pools`].
//! Fixtures are seeded through the `reverie_ingestion` pool (bypasses the
//! `manifestations` RLS policies); the endpoints are exercised through the
//! `reverie_app` pool with the admin RLS context set by `acquire_with_rls`.
#![allow(
    clippy::cast_possible_wrap,
    reason = "test-only casts on small fixture sizes"
)]

use axum::http::{StatusCode, header::AUTHORIZATION};
use serde_json::Value;
use sqlx::PgPool;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::error::problems;
use crate::test_support;

/// Insert a bare work via the ingestion pool, returning its id.
async fn seed_work(ingestion_pool: &PgPool, title: &str) -> Uuid {
    sqlx::query_scalar!(
        "INSERT INTO works (title, sort_title) VALUES ($1, $1) RETURNING id",
        title,
    )
    .fetch_one(ingestion_pool)
    .await
    .expect("insert work")
}

/// Insert a manifestation under `work_id`. `format` / `validation_status`
/// are bound as text and cast to their enum types (see backend/CLAUDE.md
/// enum-binding tactics). `marker` keeps `file_path` + hashes unique so the
/// `manifestations_file_hash_unique` constraint is not tripped.
async fn seed_manifestation(
    ingestion_pool: &PgPool,
    work_id: Uuid,
    marker: &str,
    format: &str,
    validation_status: &str,
    file_size_bytes: i64,
) -> Uuid {
    let file_path = format!("/tmp/dash-{marker}.bin");
    let hash = format!("dash-hash-{marker}");
    sqlx::query_scalar!(
        "INSERT INTO manifestations \
            (work_id, format, file_path, ingestion_file_hash, current_file_hash, \
             file_size_bytes, ingestion_status, validation_status) \
         VALUES ($1, ($2::text)::manifestation_format, $3, $4, $4, $5, \
                 'complete'::ingestion_status, ($6::text)::validation_status) \
         RETURNING id",
        work_id,
        format,
        file_path,
        hash,
        file_size_bytes,
        validation_status,
    )
    .fetch_one(ingestion_pool)
    .await
    .expect("insert manifestation")
}

/// Insert one ingestion job into `batch_id` with the given `status`.
async fn seed_job(
    ingestion_pool: &PgPool,
    batch_id: Uuid,
    marker: &str,
    status: &str,
    completed_at: Option<OffsetDateTime>,
) {
    let source_path = format!("/tmp/scan-{marker}");
    sqlx::query!(
        "INSERT INTO ingestion_jobs (batch_id, source_path, status, completed_at) \
         VALUES ($1, $2, ($3::text)::job_status, $4)",
        batch_id,
        source_path,
        status,
        completed_at,
    )
    .execute(ingestion_pool)
    .await
    .expect("insert ingestion job");
}

/// Find the `count` for `status` inside a breakdown array (`validation_breakdown`
/// / `enrichment_breakdown`). Panics if the bucket is absent — the handler emits
/// every enum variant unconditionally, so a missing bucket is a real failure.
fn bucket_count(body: &Value, array_key: &str, status: &str) -> i64 {
    body[array_key]
        .as_array()
        .unwrap_or_else(|| panic!("{array_key} is an array"))
        .iter()
        .find(|b| b["status"] == status)
        .unwrap_or_else(|| panic!("{array_key} missing bucket {status}"))["count"]
        .as_i64()
        .expect("count is an integer")
}

#[sqlx::test(migrations = "./migrations")]
async fn stats_distinct_works_vs_manifestations(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, admin_auth) = test_support::db::create_admin_and_basic_auth(&app_pool).await;

    // Two manifestations sharing ONE work: a clean epub + a clean pdf.
    let work_id = seed_work(&ingestion_pool, "Distinct Works Probe").await;
    seed_manifestation(&ingestion_pool, work_id, "dw-epub", "epub", "clean", 1000).await;
    seed_manifestation(&ingestion_pool, work_id, "dw-pdf", "pdf", "clean", 500).await;

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let response = server
        .get("/api/v1/dashboard/stats")
        .add_header(AUTHORIZATION, admin_auth)
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let body: Value = response.json();

    assert_eq!(body["total_manifestations"], 2, "two files");
    assert_eq!(body["total_works"], 1, "both share one work_id");
    assert_eq!(body["storage_total_bytes"], 1500);
    assert_eq!(
        body["clean_non_epub_count"], 1,
        "the pdf is the non-epub clean row"
    );
    assert_eq!(bucket_count(&body, "validation_breakdown", "clean"), 2);
    assert_eq!(bucket_count(&body, "validation_breakdown", "degraded"), 0);
    // Default enrichment_status is `pending` for freshly-seeded rows.
    assert_eq!(bucket_count(&body, "enrichment_breakdown", "pending"), 2);
    assert_eq!(body["metadata_coverage"]["total"], 2);
}

#[sqlx::test(migrations = "./migrations")]
async fn stats_empty_library_returns_zeros(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, admin_auth) = test_support::db::create_admin_and_basic_auth(&app_pool).await;

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let response = server
        .get("/api/v1/dashboard/stats")
        .add_header(AUTHORIZATION, admin_auth)
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let body: Value = response.json();

    assert_eq!(body["total_manifestations"], 0);
    assert_eq!(body["total_works"], 0);
    assert_eq!(body["storage_total_bytes"], 0, "COALESCE(SUM,0) not NULL");
    assert_eq!(body["storage_cover_bytes"], 0);
    assert_eq!(body["clean_non_epub_count"], 0);
    assert_eq!(body["metadata_coverage"]["total"], 0);
    // GROUP BY format over an empty table yields no rows → empty array.
    assert_eq!(
        body["storage_by_format"].as_array().expect("array").len(),
        0,
    );
    // Breakdown arrays still carry every variant (count 0).
    assert_eq!(bucket_count(&body, "validation_breakdown", "clean"), 0);
    assert_eq!(bucket_count(&body, "enrichment_breakdown", "complete"), 0);
}

#[sqlx::test(migrations = "./migrations")]
async fn stats_endpoint_rejects_non_admin(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_uid, adult_auth) =
        test_support::db::create_adult_and_basic_auth(&app_pool, "dash-adult").await;

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let response = server
        .get("/api/v1/dashboard/stats")
        .add_header(AUTHORIZATION, adult_auth)
        .await;
    test_support::assert_problem(&response, problems::FORBIDDEN, StatusCode::FORBIDDEN);
}

#[sqlx::test(migrations = "./migrations")]
async fn stats_endpoint_requires_auth(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let response = server.get("/api/v1/dashboard/stats").await;
    assert_eq!(response.status_code(), StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "./migrations")]
async fn activity_endpoint_admin_lists_batches(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, admin_auth) = test_support::db::create_admin_and_basic_auth(&app_pool).await;

    let batch = Uuid::new_v4();
    let now = OffsetDateTime::now_utc();
    seed_job(&ingestion_pool, batch, "a1", "complete", Some(now)).await;
    seed_job(&ingestion_pool, batch, "a2", "complete", Some(now)).await;

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let response = server
        .get("/api/v1/dashboard/activity")
        .add_header(AUTHORIZATION, admin_auth)
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let body: Value = response.json();
    let batches = body["batches"].as_array().expect("batches array");
    assert_eq!(batches.len(), 1);
    assert_eq!(batches[0]["total"], 2);
    assert_eq!(batches[0]["completed"], 2);
    assert_eq!(batches[0]["batch_id"], batch.to_string());
}

#[sqlx::test(migrations = "./migrations")]
async fn activity_in_progress_sums_to_total(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, admin_auth) = test_support::db::create_admin_and_basic_auth(&app_pool).await;

    // In-flight batch: a running job (no completed_at) plus terminal jobs.
    let batch = Uuid::new_v4();
    let now = OffsetDateTime::now_utc();
    seed_job(&ingestion_pool, batch, "b-run", "running", None).await;
    seed_job(&ingestion_pool, batch, "b-done", "complete", Some(now)).await;
    seed_job(&ingestion_pool, batch, "b-fail", "failed", Some(now)).await;
    seed_job(&ingestion_pool, batch, "b-skip", "skipped", Some(now)).await;

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let response = server
        .get("/api/v1/dashboard/activity")
        .add_header(AUTHORIZATION, admin_auth)
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);
    let body: Value = response.json();
    let b = &body["batches"][0];

    let total = b["total"].as_i64().unwrap();
    let sum = b["completed"].as_i64().unwrap()
        + b["failed"].as_i64().unwrap()
        + b["skipped"].as_i64().unwrap()
        + b["in_progress"].as_i64().unwrap();
    assert_eq!(sum, total, "outcome columns reconcile to total");
    assert_eq!(total, 4);
    assert_eq!(b["in_progress"], 1);
    assert!(
        b["ended_at"].is_null(),
        "in-flight batch has no end time, got {b}"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn activity_limit_is_clamped(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, admin_auth) = test_support::db::create_admin_and_basic_auth(&app_pool).await;

    let batch = Uuid::new_v4();
    seed_job(
        &ingestion_pool,
        batch,
        "c1",
        "complete",
        Some(OffsetDateTime::now_utc()),
    )
    .await;

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

    // Huge limit clamps to 100 — no error.
    let huge = server
        .get("/api/v1/dashboard/activity?limit=99999")
        .add_header(AUTHORIZATION, admin_auth.clone())
        .await;
    assert_eq!(huge.status_code(), StatusCode::OK);
    assert_eq!(huge.json::<Value>()["batches"].as_array().unwrap().len(), 1);

    // Zero clamps to 1 — no Postgres negative/zero-LIMIT error, still returns rows.
    let zero = server
        .get("/api/v1/dashboard/activity?limit=0")
        .add_header(AUTHORIZATION, admin_auth)
        .await;
    assert_eq!(zero.status_code(), StatusCode::OK);
    assert_eq!(zero.json::<Value>()["batches"].as_array().unwrap().len(), 1);
}

#[sqlx::test(migrations = "./migrations")]
async fn activity_endpoint_rejects_non_admin(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_uid, adult_auth) =
        test_support::db::create_adult_and_basic_auth(&app_pool, "dash-activity-adult").await;

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let response = server
        .get("/api/v1/dashboard/activity")
        .add_header(AUTHORIZATION, adult_auth)
        .await;
    test_support::assert_problem(&response, problems::FORBIDDEN, StatusCode::FORBIDDEN);
}

#[sqlx::test(migrations = "./migrations")]
async fn activity_endpoint_requires_auth(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;

    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
    let response = server.get("/api/v1/dashboard/activity").await;
    assert_eq!(response.status_code(), StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "./migrations")]
async fn activity_malformed_query_returns_400(pool: PgPool) {
    // Both the bad-type vector (`?limit=abc`, an Option<i64> decode failure)
    // and the duplicate-key vector (`?limit=1&limit=2`) reject at the
    // axum_extra::Query extractor and must return RFC 9457 problem+json, not
    // axum's plaintext 400 (clears debt 2026-06-10). Admin creds so the
    // require_admin gate passes and execution reaches the query-rejection.
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, admin_auth) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

    for q in ["?limit=abc", "?limit=1&limit=2"] {
        let response = server
            .get(&format!("/api/v1/dashboard/activity{q}"))
            .add_header(AUTHORIZATION, admin_auth.clone())
            .await;
        assert_eq!(
            response.status_code(),
            StatusCode::BAD_REQUEST,
            "expected 400 at {q}"
        );
        test_support::assert_problem(
            &response,
            problems::MALFORMED_QUERY,
            StatusCode::BAD_REQUEST,
        );
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn activity_empty_limit_uses_default_not_400(pool: PgPool) {
    // Parser-swap parity (serde_urlencoded → serde_html_form): `?limit=`
    // (empty) errored under serde_urlencoded ("cannot parse integer from
    // empty string"); under serde_html_form it decodes to None and the
    // handler applies the default limit. Assert the success path holds — the
    // swap must not turn an empty optional param into a spurious 400.
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_admin, admin_auth) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

    let response = server
        .get("/api/v1/dashboard/activity?limit=")
        .add_header(AUTHORIZATION, admin_auth)
        .await;
    assert_eq!(response.status_code(), StatusCode::OK);
}
