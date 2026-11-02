use axum::http::{StatusCode, header::AUTHORIZATION};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

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

fn entry(source: &str, class: &str) -> Value {
    json!({"source": source, "class": class})
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
    let response = get(&env, LIST).await;
    let text = response.text();
    for leaked in ["password", "database", "transient source"] {
        assert!(!text.contains(leaked), "response leaked {leaked}: {text}");
    }
    let body: Value = response.json();
    assert_eq!(items(&body).len(), 1);
    assert_eq!(
        body["items"][0]["primary"],
        json!({"source": null, "class": "unspecified"})
    );
    assert_eq!(body["items"][0]["also"], json!([]));
    assert_eq!(
        titles(&list(&env, "?class=unspecified").await),
        vec!["legacy"]
    );
    let counts: Value = get(&env, COUNTS).await.json();
    assert_eq!(counts["total"], 1);
    assert_eq!(
        counts["by_failure"],
        json!([{"source": null, "class": "unspecified", "count": 1}])
    );
}

async fn bulk_snapshot(env: &Env, id: Uuid) -> Value {
    sqlx::query_scalar!(
        r#"SELECT to_jsonb(m) AS "state!" FROM manifestations m WHERE id = $1"#,
        id
    )
    .fetch_one(&env.ing)
    .await
    .unwrap()
}

#[sqlx::test(migrations = "./migrations")]
async fn bulk_enrichment_retry_preserves_provider_timing_and_reset_columns(pool: PgPool) {
    let env = env(&pool).await;
    for class in crate::models::enrichment_failure::FailureClass::ALL {
        let mut fixtures = Vec::new();
        for (index, delay) in [None, Some(-60_i64), Some(3600_i64)]
            .into_iter()
            .enumerate()
        {
            let id = seed(
                &env,
                "deadline",
                if index == 2 {
                    EnrichmentStatus::Skipped
                } else {
                    EnrichmentStatus::Failed
                },
                Some("failure"),
                json!([entry("openlibrary", class.as_str())]),
            )
            .await;
            sqlx::query!("UPDATE manifestations SET enrichment_retry_not_before = now() + ($2::bigint * INTERVAL '1 second') WHERE id = $1", id, delay)
                .execute(&env.ing).await.unwrap();
            fixtures.push((id, bulk_snapshot(&env, id).await));
        }
        let response = env
            .server
            .post(&format!("{LIST}/retries"))
            .add_header(AUTHORIZATION, env.auth.clone())
            .json(&json!({"source":"openlibrary","class":class.as_str()}))
            .await;
        assert_eq!(response.status_code(), StatusCode::ACCEPTED);
        assert_eq!(response.json::<Value>(), json!({"queued":3,"remaining":0}));
        for (id, mut expected) in fixtures {
            expected["enrichment_status"] = json!("pending");
            expected["enrichment_attempt_count"] = json!(0);
            expected["enrichment_attempted_at"] = Value::Null;
            expected["enrichment_error"] = Value::Null;
            expected["enrichment_failures"] = json!([]);
            expected["enrichment_rerun_requested"] = json!(false);
            let actual = bulk_snapshot(&env, id).await;
            assert!(
                actual["updated_at"].as_str().unwrap() >= expected["updated_at"].as_str().unwrap()
            );
            expected["updated_at"] = actual["updated_at"].clone();
            assert_eq!(actual, expected);
        }
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn bulk_enrichment_retry_checks_scope_and_current_role(pool: PgPool) {
    use crate::auth::{scope::Scope, token};
    let env = env(&pool).await;
    let app = test_support::db::app_pool_for(&pool).await;
    let (id, _) = test_support::db::create_admin_and_basic_auth(&app).await;
    let row = failing(&env, "untouched", json!([entry("openlibrary", "timeout")])).await;
    let before = bulk_snapshot(&env, row).await;
    for scopes in [vec![Scope::Write], vec![Scope::Admin]] {
        let (plain, hash) = token::generate_device_token();
        let device =
            crate::models::device_token::create_with_limit(&app, id, "bulk", &hash, &scopes, None)
                .await
                .unwrap();
        if scopes == vec![Scope::Admin] {
            sqlx::query!(
                "UPDATE users SET role = 'adult'::user_role WHERE id = $1",
                id
            )
            .execute(&pool)
            .await
            .unwrap();
        }
        let bearer = format!("Bearer {}{}.{}", token::TOKEN_PREFIX, device.id, plain);
        let response = env
            .server
            .post(&format!("{LIST}/retries"))
            .add_header(AUTHORIZATION, bearer)
            .json(&json!({"source":"openlibrary", "class":"timeout"}))
            .await;
        assert_eq!(response.status_code(), StatusCode::FORBIDDEN);
    }
    assert_eq!(bulk_snapshot(&env, row).await, before);
}
