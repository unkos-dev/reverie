use axum::http::{HeaderName, HeaderValue, StatusCode};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

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
