use axum::http::{StatusCode, header::AUTHORIZATION};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::problems;
use crate::models::ingestion_input::{
    self, AttemptOutcome, Fingerprint, Input, InputPath, InputStatus, RejectionReason,
    TRANSIENT_ATTEMPT_LIMIT,
};
use crate::test_support;

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

fn fingerprint(inode: u64) -> Fingerprint {
    Fingerprint {
        device: 1,
        inode,
        size: 10,
        mtime_seconds: 1,
        mtime_nanoseconds: 0,
        ctime_seconds: 1,
        ctime_nanoseconds: 0,
    }
}

async fn observe(env: &Env, path: &[u8], inode: u64) -> Input {
    let path = InputPath::from_bytes(path.to_vec()).unwrap();
    ingestion_input::observe(&env.ing, &[path], &[fingerprint(inode)])
        .await
        .unwrap()
        .remove(0)
}

async fn attempt(
    env: &Env,
    input: &Input,
    outcome: AttemptOutcome,
    status: InputStatus,
    reason: Option<&str>,
    rejection_reasons: &[RejectionReason],
) {
    let job = ingestion_input::begin_attempt(&env.ing, input, Uuid::new_v4())
        .await
        .unwrap();
    let mut tx = env.ing.begin().await.unwrap();
    ingestion_input::finish(
        &mut tx,
        input,
        job,
        outcome,
        status,
        reason,
        rejection_reasons,
        None,
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
}

async fn rejected(env: &Env, path: &str, inode: u64, reasons: &[RejectionReason]) -> Input {
    let input = observe(env, path.as_bytes(), inode).await;
    attempt(
        env,
        &input,
        AttemptOutcome::Rejected,
        InputStatus::Rejected,
        Some("EPUB rejected: raw detail"),
        reasons,
    )
    .await;
    input
}

async fn transient(env: &Env, input: &Input) {
    attempt(
        env,
        input,
        AttemptOutcome::TransientInput,
        InputStatus::OperationalFailure,
        Some("I/O error: Other"),
        &[],
    )
    .await;
}

async fn needs_change(env: &Env, input: &Input) {
    attempt(
        env,
        input,
        AttemptOutcome::NeedsChange,
        InputStatus::OperationalFailure,
        Some("I/O error: PermissionDenied"),
        &[],
    )
    .await;
}

async fn get(env: &Env, url: &str) -> axum_test::TestResponse {
    env.server
        .get(url)
        .add_header(AUTHORIZATION, env.auth.clone())
        .await
}

async fn list(env: &Env, query: &str) -> Value {
    let response = get(env, &format!("/api/v1/ingestion/inputs{query}")).await;
    assert_eq!(response.status_code(), StatusCode::OK, "{query}");
    response.json()
}

fn item_count(body: &Value) -> usize {
    body["items"].as_array().unwrap().len()
}

fn paths(body: &Value) -> Vec<String> {
    body["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["path"].as_str().unwrap().to_owned())
        .collect()
}

async fn walk(env: &Env, query: &str, limit: usize) -> Vec<Value> {
    let mut items = Vec::new();
    let mut cursor: Option<String> = None;
    loop {
        let separator = if query.is_empty() { "?" } else { "&" };
        let after = cursor
            .as_deref()
            .map_or_else(String::new, |cursor| format!("&cursor={cursor}"));
        let url = format!("{query}{separator}limit={limit}{after}");
        let body = list(env, &url).await;
        items.extend(body["items"].as_array().unwrap().iter().cloned());
        match body["next_cursor"].as_str() {
            Some(next) => cursor = Some(next.to_owned()),
            None => return items,
        }
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn lists_a_rejected_input_with_its_classes_and_timestamps(pool: PgPool) {
    let env = env(&pool).await;
    rejected(
        &env,
        "shelf/bad.epub",
        1,
        &[RejectionReason::UnsafeContents, RejectionReason::Damaged],
    )
    .await;

    let body = list(&env, "").await;
    let item = &body["items"][0];
    assert_eq!(body["items"].as_array().unwrap().len(), 1);
    assert!(body["next_cursor"].is_null());
    assert_eq!(item["path"], "shelf/bad.epub");
    assert_eq!(item["status"], "rejected");
    assert_eq!(item["outcome"], "rejected");
    assert_eq!(item["primary_reason"], "unsafe_contents");
    assert_eq!(
        item["reasons"],
        serde_json::json!(["unsafe_contents", "damaged"])
    );
    test_support::assert_rfc3339(item, "observed_at");
    test_support::assert_rfc3339(item, "completed_at");
    assert!(item.get("reason").is_none());
}

#[sqlx::test(migrations = "./migrations")]
async fn empty_table_returns_empty_list_and_zero_counts(pool: PgPool) {
    let env = env(&pool).await;

    let body = list(&env, "").await;
    assert_eq!(item_count(&body), 0);
    assert!(body["next_cursor"].is_null());

    let counts: Value = get(&env, "/api/v1/ingestion/inputs/counts").await.json();
    assert_eq!(counts["by_reason"], serde_json::json!([]));
    assert_eq!(counts["attention_total"], 0);
}

#[sqlx::test(migrations = "./migrations")]
async fn default_listing_is_the_attention_set_with_one_class_per_state(pool: PgPool) {
    let env = env(&pool).await;
    rejected(&env, "rejected.epub", 1, &[RejectionReason::OverLimits]).await;

    let changed = observe(&env, b"needs-change.epub", 2).await;
    needs_change(&env, &changed).await;

    let waiting = observe(&env, b"waiting.epub", 3).await;
    transient(&env, &waiting).await;

    let exhausted = observe(&env, b"exhausted.epub", 4).await;
    for _ in 0..TRANSIENT_ATTEMPT_LIMIT {
        transient(&env, &exhausted).await;
    }

    let paused = observe(&env, b"paused.epub", 5).await;
    attempt(
        &env,
        &paused,
        AttemptOutcome::SharedDependency,
        InputStatus::Pending,
        Some("database unavailable"),
        &[],
    )
    .await;

    let duplicate = observe(&env, b"duplicate.epub", 6).await;
    attempt(
        &env,
        &duplicate,
        AttemptOutcome::Duplicate,
        InputStatus::Duplicate,
        None,
        &[],
    )
    .await;

    let imported = observe(&env, b"imported.epub", 7).await;
    attempt(
        &env,
        &imported,
        AttemptOutcome::Imported,
        InputStatus::Imported,
        None,
        &[],
    )
    .await;

    let ignored = observe(&env, b"notes.txt", 8).await;
    ingestion_input::set_unaccepted_many(&env.ing, &[ignored.id], &[ignored.generation])
        .await
        .unwrap();

    let body = list(&env, "").await;
    let mut listed: Vec<(String, String, String)> = body["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| {
            (
                item["path"].as_str().unwrap().to_owned(),
                item["primary_reason"].as_str().unwrap().to_owned(),
                item["status"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    listed.sort();
    assert_eq!(
        listed,
        vec![
            (
                "exhausted.epub".into(),
                "retries_exhausted".into(),
                "operational_failure".into()
            ),
            (
                "needs-change.epub".into(),
                "needs_change".into(),
                "operational_failure".into()
            ),
            (
                "rejected.epub".into(),
                "over_limits".into(),
                "rejected".into()
            ),
        ]
    );

    let ignored_body = list(&env, "?reason=format_not_accepted").await;
    assert_eq!(paths(&ignored_body), vec!["notes.txt"]);
    assert_eq!(ignored_body["items"][0]["status"], "not_accepted");
    assert!(ignored_body["items"][0]["outcome"].is_null());
}

#[sqlx::test(migrations = "./migrations")]
async fn failures_below_the_limit_are_still_waiting_and_the_limit_exhausts(pool: PgPool) {
    let env = env(&pool).await;
    let input = observe(&env, b"flaky.epub", 1).await;
    for _ in 1..TRANSIENT_ATTEMPT_LIMIT {
        transient(&env, &input).await;
    }
    assert_eq!(item_count(&list(&env, "").await), 0);

    transient(&env, &input).await;
    let body = list(&env, "").await;
    assert_eq!(body["items"][0]["primary_reason"], "retries_exhausted");
    assert_eq!(body["items"][0]["outcome"], "transient_input");
}

#[sqlx::test(migrations = "./migrations")]
async fn latest_attempt_outcome_wins_and_no_attempt_does_not_error(pool: PgPool) {
    let env = env(&pool).await;
    let input = observe(&env, b"changed.epub", 1).await;
    transient(&env, &input).await;
    needs_change(&env, &input).await;

    let untried = observe(&env, b"untried.epub", 2).await;
    sqlx::query!(
        "UPDATE ingestion_inputs SET status = 'rejected' WHERE id = $1",
        untried.id
    )
    .execute(&env.ing)
    .await
    .unwrap();

    let body = list(&env, "").await;
    let by_path = |path: &str| {
        body["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["path"] == path)
            .cloned()
            .unwrap()
    };
    let changed = by_path("changed.epub");
    assert_eq!(changed["outcome"], "needs_change");
    assert_eq!(changed["primary_reason"], "needs_change");
    let untried = by_path("untried.epub");
    assert!(untried["outcome"].is_null());
    assert_eq!(untried["primary_reason"], "unspecified");
}

#[sqlx::test(migrations = "./migrations")]
async fn an_older_generations_attempts_do_not_describe_the_current_one(pool: PgPool) {
    let env = env(&pool).await;
    let first = observe(&env, b"edited.epub", 1).await;
    needs_change(&env, &first).await;
    assert_eq!(list(&env, "").await["items"].as_array().unwrap().len(), 1);

    let second = observe(&env, b"edited.epub", 2).await;
    assert_eq!(second.generation, 2);
    assert_eq!(item_count(&list(&env, "").await), 0);

    transient(&env, &second).await;
    assert_eq!(item_count(&list(&env, "").await), 0);
}

#[sqlx::test(migrations = "./migrations")]
async fn removed_inputs_are_not_listed_or_counted(pool: PgPool) {
    let env = env(&pool).await;
    let input = rejected(&env, "gone.epub", 1, &[RejectionReason::Damaged]).await;
    rejected(&env, "kept.epub", 2, &[RejectionReason::Damaged]).await;
    ingestion_input::remove(&env.ing, &input, "external_disappearance")
        .await
        .unwrap();

    assert_eq!(paths(&list(&env, "").await), vec!["kept.epub"]);
    let counts: Value = get(&env, "/api/v1/ingestion/inputs/counts").await.json();
    assert_eq!(counts["attention_total"], 1);
}

#[sqlx::test(migrations = "./migrations")]
async fn stored_failure_text_never_reaches_the_response(pool: PgPool) {
    let env = env(&pool).await;
    let legacy = observe(&env, b"legacy.epub", 1).await;
    attempt(
        &env,
        &legacy,
        AttemptOutcome::Rejected,
        InputStatus::Rejected,
        Some("EPUB rejected: PathTraversal { entry_name: \"../../etc/passwd\" }"),
        &[],
    )
    .await;
    let internal = observe(&env, b"internal.epub", 2).await;
    attempt(
        &env,
        &internal,
        AttemptOutcome::NeedsChange,
        InputStatus::OperationalFailure,
        Some("library: sqlx: error returned from database: password authentication failed"),
        &[],
    )
    .await;

    let response = get(&env, "/api/v1/ingestion/inputs").await;
    let text = response.text();
    for leaked in [
        "passwd",
        "PathTraversal",
        "sqlx",
        "password",
        "EPUB rejected",
    ] {
        assert!(!text.contains(leaked), "response leaked {leaked}: {text}");
    }
    let body: Value = response.json();
    let legacy_item = body["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["path"] == "legacy.epub")
        .unwrap();
    assert_eq!(legacy_item["primary_reason"], "unspecified");
    assert_eq!(legacy_item["reasons"], serde_json::json!(["unspecified"]));
}

#[sqlx::test(migrations = "./migrations")]
async fn paths_are_relative_and_non_utf8_names_are_escaped(pool: PgPool) {
    let env = env(&pool).await;
    rejected(&env, "author/title.epub", 1, &[RejectionReason::Damaged]).await;
    let raw = observe(&env, b"odd/\xffname.epub", 2).await;
    attempt(
        &env,
        &raw,
        AttemptOutcome::Rejected,
        InputStatus::Rejected,
        None,
        &[RejectionReason::Damaged],
    )
    .await;

    let body = list(&env, "").await;
    let listed = paths(&body);
    assert_eq!(listed, vec!["author/title.epub", "odd/\\xffname.epub"]);
    assert!(listed.iter().all(|path| !path.starts_with('/')));
    let ids: Vec<&str> = body["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["id"].as_str().unwrap())
        .collect();
    assert_ne!(ids[0], ids[1]);
}

#[sqlx::test(migrations = "./migrations")]
async fn a_two_reason_file_is_listed_and_counted_under_its_primary_only(pool: PgPool) {
    let env = env(&pool).await;
    rejected(
        &env,
        "both.epub",
        1,
        &[RejectionReason::Damaged, RejectionReason::OverLimits],
    )
    .await;

    assert_eq!(
        paths(&list(&env, "?reason=damaged").await),
        vec!["both.epub"]
    );
    assert_eq!(item_count(&list(&env, "?reason=over_limits").await), 0);
    let item = &list(&env, "?reason=damaged").await["items"][0];
    assert_eq!(
        item["reasons"],
        serde_json::json!(["damaged", "over_limits"])
    );

    let counts: Value = get(&env, "/api/v1/ingestion/inputs/counts").await.json();
    assert_eq!(
        counts["by_reason"],
        serde_json::json!([{ "reason": "damaged", "count": 1 }])
    );
    assert_eq!(counts["attention_total"], 1);
}

#[sqlx::test(migrations = "./migrations")]
async fn counts_equal_the_rows_listed_per_class_across_pages(pool: PgPool) {
    let env = env(&pool).await;
    for index in 0..5u64 {
        rejected(
            &env,
            &format!("damaged-{index}.epub"),
            index + 1,
            &[RejectionReason::Damaged],
        )
        .await;
    }
    for index in 0..3u64 {
        rejected(
            &env,
            &format!("unsafe-{index}.epub"),
            index + 100,
            &[RejectionReason::UnsafeContents, RejectionReason::Damaged],
        )
        .await;
    }
    let changed = observe(&env, b"needs-change.epub", 200).await;
    needs_change(&env, &changed).await;
    let ignored = observe(&env, b"notes.txt", 201).await;
    ingestion_input::set_unaccepted_many(&env.ing, &[ignored.id], &[ignored.generation])
        .await
        .unwrap();

    let counts: Value = get(&env, "/api/v1/ingestion/inputs/counts").await.json();
    let by_reason = counts["by_reason"].as_array().unwrap();
    let mut attention = 0;
    for entry in by_reason {
        let reason = entry["reason"].as_str().unwrap();
        let count = entry["count"].as_i64().unwrap();
        let listed = walk(&env, &format!("?reason={reason}"), 2).await;
        assert_eq!(i64::try_from(listed.len()).unwrap(), count, "{reason}");
        assert!(listed.iter().all(|item| item["primary_reason"] == reason));
        if reason != "format_not_accepted" {
            attention += count;
        }
    }
    assert_eq!(counts["attention_total"], attention);
    assert_eq!(attention, 9);
    let order: Vec<&str> = by_reason
        .iter()
        .map(|entry| entry["reason"].as_str().unwrap())
        .collect();
    assert_eq!(
        order,
        vec![
            "unsafe_contents",
            "damaged",
            "needs_change",
            "format_not_accepted"
        ]
    );
    assert_eq!(
        i64::try_from(walk(&env, "", 4).await.len()).unwrap(),
        attention
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn counts_ignore_query_parameters(pool: PgPool) {
    let env = env(&pool).await;
    rejected(&env, "a.epub", 1, &[RejectionReason::Damaged]).await;
    rejected(&env, "b.epub", 2, &[RejectionReason::OverLimits]).await;

    let plain: Value = get(&env, "/api/v1/ingestion/inputs/counts").await.json();
    let filtered: Value = get(
        &env,
        "/api/v1/ingestion/inputs/counts?reason=damaged&limit=1",
    )
    .await
    .json();
    assert_eq!(plain, filtered);
    assert_eq!(plain["attention_total"], 2);
}

#[sqlx::test(migrations = "./migrations")]
async fn cursor_walk_has_no_gaps_or_duplicates_when_a_row_arrives_mid_scroll(pool: PgPool) {
    let env = env(&pool).await;
    let mut expected = Vec::new();
    for index in 0..5u64 {
        let name = format!("book-{index}.epub");
        rejected(&env, &name, index + 1, &[RejectionReason::Damaged]).await;
        expected.push(name);
    }

    let first = list(&env, "?limit=2").await;
    assert_eq!(paths(&first), expected[..2]);
    let cursor = first["next_cursor"].as_str().unwrap().to_owned();

    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    rejected(&env, "late.epub", 99, &[RejectionReason::Damaged]).await;
    expected.push("late.epub".into());

    let mut seen = paths(&first);
    let mut next = Some(cursor);
    while let Some(cursor) = next {
        let page = list(&env, &format!("?limit=2&cursor={cursor}")).await;
        seen.extend(paths(&page));
        next = page["next_cursor"].as_str().map(str::to_owned);
    }
    assert_eq!(seen, expected);
}

#[sqlx::test(migrations = "./migrations")]
async fn link_header_is_present_only_while_more_rows_remain(pool: PgPool) {
    let env = env(&pool).await;
    for index in 0..3u64 {
        rejected(
            &env,
            &format!("book-{index}.epub"),
            index + 1,
            &[RejectionReason::Damaged],
        )
        .await;
    }

    let page = get(&env, "/api/v1/ingestion/inputs?limit=2").await;
    let link = page.headers().get("link").unwrap().to_str().unwrap();
    assert!(link.contains("rel=\"next\""), "{link}");
    assert!(link.contains("limit=2"), "{link}");
    let body: Value = page.json();
    let cursor = body["next_cursor"].as_str().unwrap();
    assert!(link.contains(cursor), "{link}");

    let last = get(
        &env,
        &format!("/api/v1/ingestion/inputs?limit=2&cursor={cursor}"),
    )
    .await;
    assert!(last.headers().get("link").is_none());
    assert!(last.json::<Value>()["next_cursor"].is_null());
}

#[sqlx::test(migrations = "./migrations")]
async fn limit_is_clamped_and_malformed_parameters_are_rejected(pool: PgPool) {
    let env = env(&pool).await;
    for index in 0..3u64 {
        rejected(
            &env,
            &format!("book-{index}.epub"),
            index + 1,
            &[RejectionReason::Damaged],
        )
        .await;
    }

    for (query, expected) in [
        ("", 3),
        ("?limit=0", 1),
        ("?limit=-5", 1),
        ("?limit=1", 1),
        ("?limit=100", 3),
        ("?limit=101", 3),
        ("?limit=100000", 3),
    ] {
        let body = list(&env, query).await;
        assert_eq!(body["items"].as_array().unwrap().len(), expected, "{query}");
    }

    for query in [
        "?limit=abc",
        "?limit=1&limit=2",
        "?reason=bogus",
        "?reason=Damaged",
        "?reason=damaged&reason=over_limits",
    ] {
        let response = get(&env, &format!("/api/v1/ingestion/inputs{query}")).await;
        test_support::assert_problem(
            &response,
            problems::MALFORMED_QUERY,
            StatusCode::BAD_REQUEST,
        );
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn a_cursor_is_bound_to_its_reason_filter_and_endpoint(pool: PgPool) {
    let env = env(&pool).await;
    for index in 0..3u64 {
        rejected(
            &env,
            &format!("book-{index}.epub"),
            index + 1,
            &[RejectionReason::Damaged],
        )
        .await;
    }
    let page = list(&env, "?reason=damaged&limit=1").await;
    let cursor = page["next_cursor"].as_str().unwrap();

    for url in [
        format!("/api/v1/ingestion/inputs?reason=over_limits&cursor={cursor}"),
        format!("/api/v1/ingestion/inputs?cursor={cursor}"),
        "/api/v1/ingestion/inputs?cursor=not-a-cursor".to_owned(),
        "/api/v1/ingestion/inputs?cursor=!!!".to_owned(),
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
async fn inputs_endpoints_require_authentication_and_the_admin_role(pool: PgPool) {
    let app = test_support::db::app_pool_for(&pool).await;
    let ing = test_support::db::ingestion_pool_for(&pool).await;
    let (_, adult) = test_support::db::create_adult_and_basic_auth(&app, "inputs-adult").await;
    let (_, child) = test_support::db::create_child_user_and_basic_auth(&app, "inputs-child").await;
    let server = test_support::db::server_with_real_pools(&app, &ing);

    for url in [
        "/api/v1/ingestion/inputs",
        "/api/v1/ingestion/inputs/counts",
    ] {
        let response = server.get(url).await;
        assert_eq!(response.status_code(), StatusCode::UNAUTHORIZED, "{url}");
        for auth in [&adult, &child] {
            let response = server
                .get(url)
                .add_header(AUTHORIZATION, auth.clone())
                .await;
            test_support::assert_problem(&response, problems::FORBIDDEN, StatusCode::FORBIDDEN);
        }
    }
}

async fn classes(env: &Env) -> Vec<(String, String)> {
    let mut rows = sqlx::query!(
        r#"SELECT convert_from(source_path, 'UTF8') AS "path!", reason_class AS "class!"
           FROM ingestion_input_classes"#,
    )
    .fetch_all(&env.ing)
    .await
    .unwrap()
    .into_iter()
    .map(|row| (row.path, row.class))
    .collect::<Vec<_>>();
    rows.sort();
    rows
}

#[sqlx::test(migrations = "./migrations")]
async fn the_view_maps_each_state_to_its_bounded_class(pool: PgPool) {
    let env = env(&pool).await;
    rejected(&env, "unsafe.epub", 1, &[RejectionReason::UnsafeContents]).await;
    rejected(&env, "plain.epub", 2, &[]).await;

    let ignored = observe(&env, b"notes.txt", 3).await;
    ingestion_input::set_unaccepted_many(&env.ing, &[ignored.id], &[ignored.generation])
        .await
        .unwrap();

    let blocked = observe(&env, b"blocked.epub", 4).await;
    needs_change(&env, &blocked).await;

    let spent = observe(&env, b"spent.epub", 5).await;
    for _ in 0..TRANSIENT_ATTEMPT_LIMIT {
        transient(&env, &spent).await;
    }

    let waiting = observe(&env, b"waiting.epub", 6).await;
    for _ in 1..TRANSIENT_ATTEMPT_LIMIT {
        transient(&env, &waiting).await;
    }

    let pending = observe(&env, b"pending.epub", 7).await;
    assert_eq!(pending.status, InputStatus::Pending);

    let imported = observe(&env, b"imported.epub", 8).await;
    attempt(
        &env,
        &imported,
        AttemptOutcome::Imported,
        InputStatus::Imported,
        None,
        &[],
    )
    .await;

    let removed = observe(&env, b"removed.epub", 9).await;
    needs_change(&env, &removed).await;
    ingestion_input::remove(&env.ing, &removed, "admin_deletion")
        .await
        .unwrap();

    let class = |path: &str, class: &str| (path.to_owned(), class.to_owned());
    assert_eq!(
        classes(&env).await,
        vec![
            class("blocked.epub", "needs_change"),
            class("notes.txt", "format_not_accepted"),
            class("plain.epub", "unspecified"),
            class("spent.epub", "retries_exhausted"),
            class("unsafe.epub", "unsafe_contents"),
        ]
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn the_view_needs_the_recorded_exhaustion_and_a_transient_latest_outcome(pool: PgPool) {
    let env = env(&pool).await;
    let input = observe(&env, b"flaky.epub", 1).await;
    transient(&env, &input).await;
    assert!(classes(&env).await.is_empty());

    sqlx::query!(
        "UPDATE ingestion_inputs SET retries_exhausted_at = now() WHERE id = $1",
        input.id
    )
    .execute(&env.ing)
    .await
    .unwrap();
    assert_eq!(
        classes(&env).await,
        vec![("flaky.epub".into(), "retries_exhausted".into())]
    );

    needs_change(&env, &input).await;
    sqlx::query!(
        "UPDATE ingestion_inputs SET retries_exhausted_at = now() WHERE id = $1",
        input.id
    )
    .execute(&env.ing)
    .await
    .unwrap();
    assert_eq!(
        classes(&env).await,
        vec![("flaky.epub".into(), "needs_change".into())]
    );
}
