use axum::http::header::AUTHORIZATION;
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use crate::models::ingestion_input::{
    self, AttemptOutcome, Fingerprint, Input, InputPath, InputStatus, RejectionReason,
};
use crate::test_support::db::{AdminEnv as Env, admin_env as env};

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
        None,
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
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
async fn bulk_input_retry_caps_batches_and_preserves_attempt_history(pool: PgPool) {
    use ingestion_input::RetryReason;
    let env = env(&pool).await;
    for index in 0..501 {
        let input = observe(&env, format!("{index}.epub").as_bytes(), index + 1000).await;
        needs_change(&env, &input).await;
    }
    let (first, remaining) = ingestion_input::reset_class(&env.ing, RetryReason::NeedsChange)
        .await
        .unwrap();
    assert_eq!((first.len(), remaining), (500, 1));
    let (last, remaining) = ingestion_input::reset_class(&env.ing, RetryReason::NeedsChange)
        .await
        .unwrap();
    assert_eq!((last.len(), remaining), (1, 0));
    assert!(first.iter().all(|input| input.id != last[0].id));
    let jobs = sqlx::query_scalar!("SELECT COUNT(*) AS \"count!\" FROM ingestion_jobs")
        .fetch_one(&env.ing)
        .await
        .unwrap();
    assert_eq!(jobs, 501);
    assert!(
        ingestion_input::reset_class(&env.ing, RetryReason::NeedsChange)
            .await
            .unwrap()
            .0
            .is_empty()
    );
}
