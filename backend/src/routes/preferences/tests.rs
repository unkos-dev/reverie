use axum::http::{HeaderName, HeaderValue, StatusCode};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::problems;
use crate::test_support;

const PATH: &str = "/auth/me/preferences";

fn auth(header: &str) -> (HeaderName, HeaderValue) {
    (
        axum::http::header::AUTHORIZATION,
        HeaderValue::from_str(header).expect("ascii auth header"),
    )
}

/// Bind an app-role connection to `user_id` for the duration of a
/// transaction, mirroring what `db::acquire_with_rls` does for a request.
/// Isolation tests must run through a `reverie_app` pool: the pool
/// `#[sqlx::test]` injects connects as the table owner, and no migration
/// here uses `FORCE ROW LEVEL SECURITY`, so an owner session bypasses the
/// policy and would pass whether or not it exists.
async fn rls_tx(app_pool: &PgPool, user_id: Uuid) -> sqlx::Transaction<'_, sqlx::Postgres> {
    crate::db::acquire_with_rls(app_pool, user_id)
        .await
        .expect("rls transaction")
}

// --- authentication ---

#[sqlx::test(migrations = "./migrations")]
async fn get_preferences_requires_auth(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

    let r = server.get(PATH).await;
    assert_eq!(r.status_code(), StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "./migrations")]
async fn patch_preferences_requires_auth(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

    let r = server
        .patch(PATH)
        .json(&json!({"density": "compact"}))
        .await;
    assert_eq!(r.status_code(), StatusCode::UNAUTHORIZED);
}

// --- read: lazy row, tier-0 defaults resolved at read time ---

#[sqlx::test(migrations = "./migrations")]
async fn get_preferences_without_a_row_returns_null_overrides_and_defaults(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_user_id, basic) = test_support::db::create_adult_and_basic_auth(&app_pool, "get").await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

    let r = server
        .get(PATH)
        .add_header(auth(&basic).0, auth(&basic).1)
        .await;
    assert_eq!(r.status_code(), StatusCode::OK, "body: {}", r.text());

    let body: serde_json::Value = r.json();
    for group in ["hidden_columns", "density", "view", "sort_stack"] {
        assert!(
            body[group].is_null(),
            "{group} should be an unset override: {body}"
        );
    }
    assert_eq!(body["defaults"]["hidden_columns"], json!([]));
    assert_eq!(body["defaults"]["density"], "comfortable");
    assert_eq!(body["defaults"]["view"], "grid");
    assert_eq!(body["defaults"]["sort_stack"], "-created_at");
}

#[sqlx::test(migrations = "./migrations")]
async fn get_preferences_creates_no_row(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (user_id, basic) = test_support::db::create_adult_and_basic_auth(&app_pool, "norow").await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

    server
        .get(PATH)
        .add_header(auth(&basic).0, auth(&basic).1)
        .await;

    let rows = sqlx::query_scalar!(
        r#"SELECT count(*) AS "count!" FROM user_preferences WHERE user_id = $1"#,
        user_id,
    )
    .fetch_one(&pool)
    .await
    .expect("count");
    assert_eq!(rows, 0, "a read must not materialise a preferences row");
}

// --- write: lazy upsert, partial semantics ---

#[sqlx::test(migrations = "./migrations")]
async fn first_patch_creates_the_row(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (user_id, basic) = test_support::db::create_adult_and_basic_auth(&app_pool, "first").await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

    let r = server
        .patch(PATH)
        .add_header(auth(&basic).0, auth(&basic).1)
        .json(&json!({"density": "compact"}))
        .await;
    assert_eq!(r.status_code(), StatusCode::OK, "body: {}", r.text());
    assert_eq!(r.json::<serde_json::Value>()["density"], "compact");

    let stored = sqlx::query_scalar!(
        "SELECT density::text FROM user_preferences WHERE user_id = $1",
        user_id,
    )
    .fetch_one(&pool)
    .await
    .expect("row exists after first patch");
    assert_eq!(stored.as_deref(), Some("compact"));
}

#[sqlx::test(migrations = "./migrations")]
async fn patch_leaves_omitted_groups_unchanged(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_user_id, basic) = test_support::db::create_adult_and_basic_auth(&app_pool, "omit").await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

    server
        .patch(PATH)
        .add_header(auth(&basic).0, auth(&basic).1)
        .json(&json!({
            "density": "compact",
            "view": "table",
            "hidden_columns": ["pages"],
            "sort_stack": "title",
        }))
        .await;

    let r = server
        .patch(PATH)
        .add_header(auth(&basic).0, auth(&basic).1)
        .json(&json!({"view": "grid"}))
        .await;
    assert_eq!(r.status_code(), StatusCode::OK, "body: {}", r.text());

    let body: serde_json::Value = r.json();
    assert_eq!(body["view"], "grid");
    assert_eq!(body["density"], "compact");
    assert_eq!(body["hidden_columns"], json!(["pages"]));
    assert_eq!(body["sort_stack"], "title");
}

// One reset test per group: collapsing absent and null in serde is the
// single failure that would silently disable the whole reset affordance,
// and it can be introduced for one field at a time.

#[sqlx::test(migrations = "./migrations")]
async fn null_resets_density_only(pool: PgPool) {
    assert_reset_isolates_group(&pool, "density", json!(null)).await;
}

#[sqlx::test(migrations = "./migrations")]
async fn null_resets_view_only(pool: PgPool) {
    assert_reset_isolates_group(&pool, "view", json!(null)).await;
}

#[sqlx::test(migrations = "./migrations")]
async fn null_resets_hidden_columns_only(pool: PgPool) {
    assert_reset_isolates_group(&pool, "hidden_columns", json!(null)).await;
}

#[sqlx::test(migrations = "./migrations")]
async fn null_resets_sort_stack_only(pool: PgPool) {
    assert_reset_isolates_group(&pool, "sort_stack", json!(null)).await;
}

/// Set all four groups, reset exactly one with an explicit `null`, and
/// assert that group inherits again while the other three survive.
async fn assert_reset_isolates_group(pool: &PgPool, group: &str, reset: serde_json::Value) {
    let app_pool = test_support::db::app_pool_for(pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(pool).await;
    let (_user_id, basic) = test_support::db::create_adult_and_basic_auth(&app_pool, group).await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

    let all_set = json!({
        "density": "compact",
        "view": "table",
        "hidden_columns": ["pages"],
        "sort_stack": "title",
    });
    server
        .patch(PATH)
        .add_header(auth(&basic).0, auth(&basic).1)
        .json(&all_set)
        .await;

    let r = server
        .patch(PATH)
        .add_header(auth(&basic).0, auth(&basic).1)
        .json(&json!({ group: reset }))
        .await;
    assert_eq!(r.status_code(), StatusCode::OK, "body: {}", r.text());

    let body: serde_json::Value = r.json();
    assert!(
        body[group].is_null(),
        "{group} should inherit after an explicit null: {body}"
    );
    for other in ["density", "view", "hidden_columns", "sort_stack"] {
        if other == group {
            continue;
        }
        assert_eq!(
            body[other], all_set[other],
            "resetting {group} must leave {other} untouched: {body}"
        );
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn preferences_survive_a_fresh_read(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_user_id, basic) =
        test_support::db::create_adult_and_basic_auth(&app_pool, "persist").await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

    server
        .patch(PATH)
        .add_header(auth(&basic).0, auth(&basic).1)
        .json(&json!({"view": "table", "sort_stack": "-pages,title"}))
        .await;

    let body: serde_json::Value = server
        .get(PATH)
        .add_header(auth(&basic).0, auth(&basic).1)
        .await
        .json();
    assert_eq!(body["view"], "table");
    assert_eq!(body["sort_stack"], "-pages,title");
}

// --- validation ---

#[sqlx::test(migrations = "./migrations")]
async fn patch_rejects_unknown_density(pool: PgPool) {
    assert_patch_rejected(&pool, "bad-density", json!({"density": "roomy"})).await;
}

#[sqlx::test(migrations = "./migrations")]
async fn patch_rejects_unknown_view(pool: PgPool) {
    assert_patch_rejected(&pool, "bad-view", json!({"view": "list"})).await;
}

#[sqlx::test(migrations = "./migrations")]
async fn patch_rejects_unknown_sort_field(pool: PgPool) {
    assert_patch_rejected(&pool, "bad-sort", json!({"sort_stack": "shoe_size"})).await;
}

#[sqlx::test(migrations = "./migrations")]
async fn patch_rejects_sort_stack_over_the_level_cap(pool: PgPool) {
    assert_patch_rejected(
        &pool,
        "deep-sort",
        json!({"sort_stack": "title,author,pages,-created_at"}),
    )
    .await;
}

#[sqlx::test(migrations = "./migrations")]
async fn patch_rejects_empty_sort_stack(pool: PgPool) {
    assert_patch_rejected(&pool, "empty-sort", json!({"sort_stack": ""})).await;
}

#[sqlx::test(migrations = "./migrations")]
async fn patch_rejects_malformed_column_key(pool: PgPool) {
    assert_patch_rejected(
        &pool,
        "bad-key",
        json!({"hidden_columns": ["pages", "DROP TABLE"]}),
    )
    .await;
}

#[sqlx::test(migrations = "./migrations")]
async fn patch_rejects_oversized_hidden_columns(pool: PgPool) {
    let keys: Vec<String> = (0..65).map(|i| format!("col_{i}")).collect();
    assert_patch_rejected(&pool, "many-keys", json!({"hidden_columns": keys})).await;
}

#[sqlx::test(migrations = "./migrations")]
async fn patch_rejects_a_body_with_no_groups(pool: PgPool) {
    assert_patch_rejected(&pool, "empty-body", json!({})).await;
}

async fn assert_patch_rejected(pool: &PgPool, name: &str, body: serde_json::Value) {
    let app_pool = test_support::db::app_pool_for(pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(pool).await;
    let (user_id, basic) = test_support::db::create_adult_and_basic_auth(&app_pool, name).await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

    let r = server
        .patch(PATH)
        .add_header(auth(&basic).0, auth(&basic).1)
        .json(&body)
        .await;
    test_support::assert_problem(&r, problems::VALIDATION, StatusCode::UNPROCESSABLE_ENTITY);

    let rows = sqlx::query_scalar!(
        r#"SELECT count(*) AS "count!" FROM user_preferences WHERE user_id = $1"#,
        user_id,
    )
    .fetch_one(pool)
    .await
    .expect("count");
    assert_eq!(rows, 0, "a rejected patch must persist nothing");
}

// --- concurrency (D2: last write wins, nothing partial) ---

#[sqlx::test(migrations = "./migrations")]
async fn concurrent_patches_resolve_to_one_value_without_error(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (user_id, basic) =
        test_support::db::create_adult_and_basic_auth(&app_pool, "concurrent").await;
    let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

    let (a, b) = tokio::join!(
        server
            .patch(PATH)
            .add_header(auth(&basic).0, auth(&basic).1)
            .json(&json!({"density": "compact"})),
        server
            .patch(PATH)
            .add_header(auth(&basic).0, auth(&basic).1)
            .json(&json!({"density": "comfortable"})),
    );
    assert_eq!(a.status_code(), StatusCode::OK, "body: {}", a.text());
    assert_eq!(b.status_code(), StatusCode::OK, "body: {}", b.text());

    let stored = sqlx::query_scalar!(
        "SELECT density::text FROM user_preferences WHERE user_id = $1",
        user_id,
    )
    .fetch_one(&pool)
    .await
    .expect("exactly one row");
    assert!(
        matches!(stored.as_deref(), Some("compact" | "comfortable")),
        "one of the two writes must win outright, got {stored:?}"
    );
}

// --- row-level security, proven against the real policy ---

#[sqlx::test(migrations = "./migrations")]
async fn rls_hides_another_users_preferences(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let (owner, _) = test_support::db::create_adult_and_basic_auth(&app_pool, "rls-owner").await;
    let (other, _) = test_support::db::create_adult_and_basic_auth(&app_pool, "rls-other").await;

    sqlx::query!(
        "INSERT INTO user_preferences (user_id, density) VALUES ($1, 'compact')",
        owner,
    )
    .execute(&pool)
    .await
    .expect("seed owner row as table owner");

    let mut tx = rls_tx(&app_pool, other).await;
    let visible = sqlx::query_scalar!(
        r#"SELECT count(*) AS "count!" FROM user_preferences WHERE user_id = $1"#,
        owner,
    )
    .fetch_one(&mut *tx)
    .await
    .expect("count under RLS");
    assert_eq!(visible, 0, "a non-owner must not see another user's row");
}

#[sqlx::test(migrations = "./migrations")]
async fn rls_rejects_writing_another_users_preferences(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let (owner, _) = test_support::db::create_adult_and_basic_auth(&app_pool, "rls-w-own").await;
    let (other, _) = test_support::db::create_adult_and_basic_auth(&app_pool, "rls-w-oth").await;

    let mut tx = rls_tx(&app_pool, other).await;
    let result = sqlx::query!(
        "INSERT INTO user_preferences (user_id, density) VALUES ($1, 'compact')",
        owner,
    )
    .execute(&mut *tx)
    .await;
    assert!(
        result.is_err(),
        "the WITH CHECK clause must refuse a row owned by someone else"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn rls_rejects_updating_another_users_preferences(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let (owner, _) = test_support::db::create_adult_and_basic_auth(&app_pool, "rls-u-own").await;
    let (other, _) = test_support::db::create_adult_and_basic_auth(&app_pool, "rls-u-oth").await;

    sqlx::query!(
        "INSERT INTO user_preferences (user_id, density) VALUES ($1, 'compact')",
        owner,
    )
    .execute(&pool)
    .await
    .expect("seed owner row as table owner");

    let mut tx = rls_tx(&app_pool, other).await;
    let affected = sqlx::query!(
        "UPDATE user_preferences SET density = 'comfortable' WHERE user_id = $1",
        owner,
    )
    .execute(&mut *tx)
    .await
    .expect("update runs but matches nothing")
    .rows_affected();
    assert_eq!(
        affected, 0,
        "USING must hide the row from a non-owner UPDATE"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn a_users_own_row_stays_reachable_under_rls(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let (owner, _) = test_support::db::create_adult_and_basic_auth(&app_pool, "rls-self").await;

    let mut tx = rls_tx(&app_pool, owner).await;
    sqlx::query!(
        "INSERT INTO user_preferences (user_id, density) VALUES ($1, 'compact')",
        owner,
    )
    .execute(&mut *tx)
    .await
    .expect("owner may insert its own row");
    let visible = sqlx::query_scalar!(r#"SELECT count(*) AS "count!" FROM user_preferences"#)
        .fetch_one(&mut *tx)
        .await
        .expect("count under RLS");
    assert_eq!(visible, 1, "positive control: the policy is not deny-all");
}

// --- schema constraints (backstop for the boundary parser) ---

#[sqlx::test(migrations = "./migrations")]
async fn check_constraint_rejects_a_malformed_sort_stack(pool: PgPool) {
    let (owner, _) = test_support::db::create_adult_and_basic_auth(
        &test_support::db::app_pool_for(&pool).await,
        "chk-sort",
    )
    .await;

    // Positive control first: a negative-only assertion cannot tell "the
    // CHECK rejected this" from "the insert could not run at all".
    sqlx::query!(
        "INSERT INTO user_preferences (user_id, sort_stack) VALUES ($1, $2)",
        owner,
        "-created_at",
    )
    .execute(&pool)
    .await
    .expect("a well-formed sort stack must be storable");

    let result = sqlx::query!(
        "UPDATE user_preferences SET sort_stack = $2 WHERE user_id = $1",
        owner,
        "title;DROP TABLE users",
    )
    .execute(&pool)
    .await;
    assert!(
        result.is_err(),
        "the sort_stack CHECK must reject a value outside the wire grammar"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn check_constraint_rejects_an_oversized_hidden_column_set(pool: PgPool) {
    let (owner, _) = test_support::db::create_adult_and_basic_auth(
        &test_support::db::app_pool_for(&pool).await,
        "chk-cols",
    )
    .await;
    let within_cap: Vec<String> = (0..64).map(|i| format!("col_{i}")).collect();
    let over_cap: Vec<String> = (0..65).map(|i| format!("col_{i}")).collect();

    // Positive control at exactly the cap, so the assertion below is about
    // the boundary and not about the insert failing for some other reason.
    sqlx::query!(
        "INSERT INTO user_preferences (user_id, hidden_columns) VALUES ($1, $2)",
        owner,
        &within_cap[..],
    )
    .execute(&pool)
    .await
    .expect("a set at the cap must be storable");

    let result = sqlx::query!(
        "UPDATE user_preferences SET hidden_columns = $2 WHERE user_id = $1",
        owner,
        &over_cap[..],
    )
    .execute(&pool)
    .await;
    assert!(
        result.is_err(),
        "the hidden_columns CHECK must bound how much a single row can store"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn deleting_a_user_removes_their_preferences(pool: PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let (owner, _) = test_support::db::create_adult_and_basic_auth(&app_pool, "cascade").await;

    sqlx::query!(
        "INSERT INTO user_preferences (user_id, density) VALUES ($1, 'compact')",
        owner,
    )
    .execute(&pool)
    .await
    .expect("seed");
    sqlx::query!("DELETE FROM users WHERE id = $1", owner)
        .execute(&pool)
        .await
        .expect("delete user");

    let rows = sqlx::query_scalar!(r#"SELECT count(*) AS "count!" FROM user_preferences"#)
        .fetch_one(&pool)
        .await
        .expect("count");
    assert_eq!(rows, 0, "preferences must not outlive their account");
}
