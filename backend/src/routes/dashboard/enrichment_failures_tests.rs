use axum::http::{StatusCode, header::AUTHORIZATION};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::problems;
use crate::models::enrichment_status::EnrichmentStatus;
use crate::test_support;

const LIST: &str = "/api/v1/dashboard/enrichment-failures";
const COUNTS: &str = "/api/v1/dashboard/enrichment-failures/counts";

struct Env {
    ing: PgPool,
    server: axum_test::TestServer,
    auth: String,
}

async fn env(pool: &PgPool) -> Env {
    let app = test_support::db::app_pool_for(pool).await;
    let ing = test_support::db::ingestion_pool_for(pool).await;
    let (_, auth) = test_support::db::create_admin_and_basic_auth(&app).await;
    let server = test_support::db::server_with_real_pools(&app, &ing);
    Env { ing, server, auth }
}

async fn seed(
    env: &Env,
    title: &str,
    status: EnrichmentStatus,
    error: Option<&str>,
    failures: Value,
) -> Uuid {
    let work = sqlx::query_scalar!(
        "INSERT INTO works (title, sort_title) VALUES ($1, $1) RETURNING id",
        title,
    )
    .fetch_one(&env.ing)
    .await
    .unwrap();
    let marker = Uuid::new_v4().simple().to_string();
    let path = format!("fixtures/failures-{marker}.epub");
    let hash = format!("failures-hash-{marker}");
    sqlx::query_scalar!(
        "WITH inserted AS (INSERT INTO manifestations \
            (library_id, work_id, format, file_path, ingestion_file_hash, current_file_hash, \
             file_size_bytes, ingestion_status, validation_status, enrichment_status, \
             enrichment_attempt_count, enrichment_attempted_at, enrichment_error, \
             enrichment_failures) \
         VALUES ((SELECT id FROM libraries WHERE configuration_key = 'default'), $1, \
                 'epub'::manifestation_format, $2, $3, $3, 1000, 'complete'::ingestion_status, \
                 'clean'::validation_status, $4, 3, now(), $5, $6) \
         RETURNING *), \
         claimed AS (INSERT INTO library_path_claims (library_id, path, manifestation_id) \
                     SELECT library_id, file_path, id FROM inserted) \
         SELECT id AS \"id!\" FROM inserted",
        work,
        path,
        hash,
        status as EnrichmentStatus,
        error,
        failures,
    )
    .fetch_one(&env.ing)
    .await
    .unwrap()
}

async fn failing(env: &Env, title: &str, failures: Value) -> Uuid {
    seed(
        env,
        title,
        EnrichmentStatus::Failed,
        Some("transient source failures"),
        failures,
    )
    .await
}

async fn get(env: &Env, url: &str) -> axum_test::TestResponse {
    env.server
        .get(url)
        .add_header(AUTHORIZATION, env.auth.clone())
        .await
}

async fn list(env: &Env, query: &str) -> Value {
    let response = get(env, &format!("{LIST}{query}")).await;
    assert_eq!(response.status_code(), StatusCode::OK, "{query}");
    response.json()
}

fn items(body: &Value) -> &Vec<Value> {
    body["items"].as_array().unwrap()
}

fn titles(body: &Value) -> Vec<String> {
    items(body)
        .iter()
        .map(|item| item["title"].as_str().unwrap().to_owned())
        .collect()
}

async fn walk(env: &Env, query: &str, limit: usize) -> Vec<Value> {
    let mut all = Vec::new();
    let mut cursor: Option<String> = None;
    loop {
        let separator = if query.is_empty() { "?" } else { "&" };
        let after = cursor
            .as_deref()
            .map_or_else(String::new, |cursor| format!("&cursor={cursor}"));
        let body = list(env, &format!("{query}{separator}limit={limit}{after}")).await;
        all.extend(items(&body).iter().cloned());
        match body["next_cursor"].as_str() {
            Some(next) => cursor = Some(next.to_owned()),
            None => return all,
        }
    }
}

fn entry(source: &str, class: &str) -> Value {
    json!({"source": source, "class": class})
}

#[sqlx::test(migrations = "./migrations")]
async fn lists_a_failing_book_with_its_primary_and_other_sources(pool: PgPool) {
    let env = env(&pool).await;
    let id = failing(
        &env,
        "Two Sources",
        json!([
            entry("hardcover", "timeout"),
            entry("openlibrary", "rate_limited")
        ]),
    )
    .await;

    let body = list(&env, "").await;
    assert_eq!(items(&body).len(), 1);
    assert!(body["next_cursor"].is_null());
    let item = &body["items"][0];
    assert_eq!(item["manifestation_id"], id.to_string());
    assert_eq!(item["title"], "Two Sources");
    assert_eq!(item["status"], "failed");
    assert_eq!(item["attempt_count"], 3);
    test_support::assert_rfc3339(item, "attempted_at");
    assert_eq!(item["primary"], entry("hardcover", "timeout"));
    assert_eq!(item["also"], json!([entry("openlibrary", "rate_limited")]));
    assert!(item["work_id"].is_string());
}

#[sqlx::test(migrations = "./migrations")]
async fn only_failed_and_skipped_books_with_an_error_are_listed(pool: PgPool) {
    let env = env(&pool).await;
    let failure = json!([entry("openlibrary", "timeout")]);
    for (title, status, error) in [
        ("complete", EnrichmentStatus::Complete, None),
        ("pending", EnrichmentStatus::Pending, None),
        ("in progress", EnrichmentStatus::InProgress, None),
        ("skipped without error", EnrichmentStatus::Skipped, None),
        ("failed without error", EnrichmentStatus::Failed, None),
        (
            "failed",
            EnrichmentStatus::Failed,
            Some("transient source failures"),
        ),
        (
            "skipped",
            EnrichmentStatus::Skipped,
            Some("transient source failures"),
        ),
    ] {
        seed(&env, title, status, error, failure.clone()).await;
    }

    let body = list(&env, "").await;
    let mut listed = titles(&body);
    listed.sort();
    assert_eq!(listed, vec!["failed", "skipped"]);
    let skipped = items(&body)
        .iter()
        .find(|item| item["title"] == "skipped")
        .unwrap();
    assert_eq!(skipped["status"], "skipped");
}

#[sqlx::test(migrations = "./migrations")]
async fn rows_without_classes_read_as_unspecified_and_never_expose_stored_text(pool: PgPool) {
    let env = env(&pool).await;
    seed(
        &env,
        "legacy",
        EnrichmentStatus::Failed,
        Some("error returned from database: password authentication failed"),
        json!([]),
    )
    .await;
    seed(
        &env,
        "unknown class",
        EnrichmentStatus::Failed,
        Some("transient source failures"),
        json!([{"source": "hardcover", "class": "secret-detail"}]),
    )
    .await;

    let response = get(&env, LIST).await;
    let text = response.text();
    for leaked in ["password", "database", "secret-detail", "transient source"] {
        assert!(!text.contains(leaked), "response leaked {leaked}: {text}");
    }
    let body: Value = response.json();
    for item in items(&body) {
        assert_eq!(
            item["primary"],
            json!({"source": null, "class": "unspecified"})
        );
        assert_eq!(item["also"], json!([]));
    }
    let counts: Value = get(&env, COUNTS).await.json();
    assert_eq!(counts["total"], 2);
    assert_eq!(
        counts["by_failure"],
        json!([
            {"source": "hardcover", "class": "unspecified", "count": 1},
            {"source": null, "class": "unspecified", "count": 1},
        ])
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn filters_match_the_primary_failure_only(pool: PgPool) {
    let env = env(&pool).await;
    failing(
        &env,
        "hardcover first",
        json!([
            entry("hardcover", "timeout"),
            entry("openlibrary", "rate_limited")
        ]),
    )
    .await;
    failing(
        &env,
        "openlibrary",
        json!([entry("openlibrary", "timeout")]),
    )
    .await;
    failing(
        &env,
        "internal",
        json!([{"source": null, "class": "internal"}]),
    )
    .await;

    assert_eq!(
        titles(&list(&env, "?source=hardcover").await),
        vec!["hardcover first"]
    );
    assert_eq!(
        titles(&list(&env, "?source=openlibrary").await),
        vec!["openlibrary"]
    );
    let mut timeouts = titles(&list(&env, "?class=timeout").await);
    timeouts.sort();
    assert_eq!(timeouts, vec!["hardcover first", "openlibrary"]);
    assert_eq!(
        titles(&list(&env, "?source=openlibrary&class=timeout").await),
        vec!["openlibrary"]
    );
    assert_eq!(
        items(&list(&env, "?source=hardcover&class=rate_limited").await).len(),
        0
    );
    assert_eq!(
        titles(&list(&env, "?class=internal").await),
        vec!["internal"]
    );
    assert_eq!(items(&list(&env, "?source=unknown").await).len(), 0);
}

#[sqlx::test(migrations = "./migrations")]
async fn counts_equal_the_rows_listed_per_group_across_pages(pool: PgPool) {
    let env = env(&pool).await;
    for index in 0..4 {
        failing(
            &env,
            &format!("hardcover timeout {index}"),
            json!([
                entry("hardcover", "timeout"),
                entry("openlibrary", "timeout")
            ]),
        )
        .await;
    }
    for index in 0..3 {
        failing(
            &env,
            &format!("openlibrary limited {index}"),
            json!([entry("openlibrary", "rate_limited")]),
        )
        .await;
    }
    failing(
        &env,
        "internal",
        json!([{"source": null, "class": "internal"}]),
    )
    .await;

    let counts: Value = get(&env, COUNTS).await.json();
    assert_eq!(
        counts["by_failure"],
        json!([
            {"source": "hardcover", "class": "timeout", "count": 4},
            {"source": "openlibrary", "class": "rate_limited", "count": 3},
            {"source": null, "class": "internal", "count": 1},
        ])
    );
    assert_eq!(counts["total"], 8);

    let mut seen = 0;
    for group in counts["by_failure"].as_array().unwrap() {
        let source = group["source"]
            .as_str()
            .map_or_else(String::new, |source| format!("&source={source}"));
        let query = format!("?class={}{source}", group["class"].as_str().unwrap());
        let listed = walk(&env, &query, 2).await;
        assert_eq!(
            i64::try_from(listed.len()).unwrap(),
            group["count"],
            "{query}"
        );
        seen += listed.len();
    }
    assert_eq!(seen, 8);
    assert_eq!(walk(&env, "", 3).await.len(), 8);
}

#[sqlx::test(migrations = "./migrations")]
async fn counts_ignore_query_parameters_and_an_empty_table_counts_zero(pool: PgPool) {
    let env = env(&pool).await;
    let empty: Value = get(&env, COUNTS).await.json();
    assert_eq!(empty, json!({"by_failure": [], "total": 0}));

    failing(&env, "a", json!([entry("hardcover", "timeout")])).await;
    failing(&env, "b", json!([entry("openlibrary", "timeout")])).await;
    let plain: Value = get(&env, COUNTS).await.json();
    let filtered: Value = get(&env, &format!("{COUNTS}?source=hardcover&limit=1"))
        .await
        .json();
    assert_eq!(plain, filtered);
    assert_eq!(plain["total"], 2);
}

#[sqlx::test(migrations = "./migrations")]
async fn cursor_walk_has_no_gaps_or_duplicates_when_a_row_arrives_mid_scroll(pool: PgPool) {
    let env = env(&pool).await;
    let mut expected = Vec::new();
    for index in 0..5 {
        let title = format!("book {index}");
        failing(&env, &title, json!([entry("hardcover", "timeout")])).await;
        expected.push(title);
    }

    let first = list(&env, "?limit=2").await;
    assert_eq!(titles(&first), expected[..2]);
    let mut cursor = first["next_cursor"].as_str().map(str::to_owned);
    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    failing(&env, "late", json!([entry("hardcover", "timeout")])).await;
    expected.push("late".into());

    let mut seen = titles(&first);
    while let Some(current) = cursor {
        let page = list(&env, &format!("?limit=2&cursor={current}")).await;
        seen.extend(titles(&page));
        cursor = page["next_cursor"].as_str().map(str::to_owned);
    }
    assert_eq!(seen, expected);
}

#[sqlx::test(migrations = "./migrations")]
async fn link_header_is_present_only_while_more_rows_remain(pool: PgPool) {
    let env = env(&pool).await;
    for index in 0..3 {
        failing(
            &env,
            &format!("book {index}"),
            json!([entry("hardcover", "timeout")]),
        )
        .await;
    }

    let page = get(&env, &format!("{LIST}?limit=2")).await;
    let link = page.headers().get("link").unwrap().to_str().unwrap();
    assert!(link.contains("rel=\"next\""), "{link}");
    let cursor = page.json::<Value>()["next_cursor"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(link.contains(&cursor), "{link}");

    let last = get(&env, &format!("{LIST}?limit=2&cursor={cursor}")).await;
    assert!(last.headers().get("link").is_none());
    assert!(last.json::<Value>()["next_cursor"].is_null());
}

#[sqlx::test(migrations = "./migrations")]
async fn limit_is_clamped_and_malformed_parameters_are_rejected(pool: PgPool) {
    let env = env(&pool).await;
    for index in 0..3 {
        failing(
            &env,
            &format!("book {index}"),
            json!([entry("hardcover", "timeout")]),
        )
        .await;
    }

    for (query, expected) in [
        ("", 3),
        ("?limit=0", 1),
        ("?limit=-5", 1),
        ("?limit=100", 3),
        ("?limit=101", 3),
    ] {
        assert_eq!(items(&list(&env, query).await).len(), expected, "{query}");
    }
    for query in [
        "?limit=abc",
        "?limit=1&limit=2",
        "?class=bogus",
        "?class=Timeout",
        "?source=Hard-Cover",
        "?source=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    ] {
        let response = get(&env, &format!("{LIST}{query}")).await;
        test_support::assert_problem(
            &response,
            problems::MALFORMED_QUERY,
            StatusCode::BAD_REQUEST,
        );
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn a_cursor_is_bound_to_its_filters_and_endpoint(pool: PgPool) {
    let env = env(&pool).await;
    for index in 0..3 {
        failing(
            &env,
            &format!("book {index}"),
            json!([entry("hardcover", "timeout")]),
        )
        .await;
    }
    let cursor = list(&env, "?source=hardcover&limit=1").await["next_cursor"]
        .as_str()
        .unwrap()
        .to_owned();

    for url in [
        format!("{LIST}?source=openlibrary&cursor={cursor}"),
        format!("{LIST}?cursor={cursor}"),
        format!("{LIST}?source=hardcover&class=timeout&cursor={cursor}"),
        format!("{LIST}?cursor=not-a-cursor"),
    ] {
        let response = get(&env, &url).await;
        test_support::assert_problem(
            &response,
            problems::VALIDATION,
            StatusCode::UNPROCESSABLE_ENTITY,
        );
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn enrichment_failure_reads_require_authentication_and_the_admin_role(pool: PgPool) {
    let app = test_support::db::app_pool_for(&pool).await;
    let ing = test_support::db::ingestion_pool_for(&pool).await;
    let (_, adult) = test_support::db::create_adult_and_basic_auth(&app, "failures-adult").await;
    let (_, child) =
        test_support::db::create_child_user_and_basic_auth(&app, "failures-child").await;
    let server = test_support::db::server_with_real_pools(&app, &ing);

    for url in [LIST, COUNTS] {
        assert_eq!(
            server.get(url).await.status_code(),
            StatusCode::UNAUTHORIZED,
            "{url}"
        );
        for auth in [&adult, &child] {
            let response = server
                .get(url)
                .add_header(AUTHORIZATION, auth.clone())
                .await;
            test_support::assert_problem(&response, problems::FORBIDDEN, StatusCode::FORBIDDEN);
        }
    }
}
