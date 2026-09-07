//! Verify that `ApiPath` and `ApiJson` answer a request they cannot extract
//! with an RFC 9457 document rather than axum's plain text, keeping the status
//! axum assigns each rejection class.
//!
//! This test lives under `backend/tests/` so its router is test infrastructure
//! outside the shipped crate: the raw `.route(...)` calls never enter
//! `backend/src/`, where every registration must carry `#[utoipa::path]`.

use axum::Router;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum_test::TestServer;
use reverie_api::extract::{ApiJson, ApiPath};
use uuid::Uuid;

#[derive(Debug, serde::Deserialize)]
struct Probe {
    name: String,
}

async fn get_item(ApiPath(id): ApiPath<Uuid>) -> impl IntoResponse {
    id.to_string()
}

async fn create_item(ApiJson(probe): ApiJson<Probe>) -> impl IntoResponse {
    probe.name
}

fn test_server() -> TestServer {
    let router = Router::new()
        .route("/items/{id}", get(get_item))
        .route("/items", post(create_item));
    TestServer::new(router)
}

fn assert_problem(response: &axum_test::TestResponse, status: u16, slug: &str) {
    assert_eq!(response.status_code(), status);
    assert_eq!(response.content_type(), "application/problem+json");
    let body: serde_json::Value = response.json();
    let typ = body["type"].as_str().unwrap_or_default();
    assert!(
        typ.ends_with(&format!("/{slug}")),
        "expected type ending in /{slug}, got {body}"
    );
}

#[tokio::test]
async fn malformed_path_parameter_is_400_malformed_path() {
    let response = test_server().get("/items/not-a-uuid").await;
    assert_problem(&response, 400, "malformed-path");
}

#[tokio::test]
async fn invalid_json_syntax_is_400_invalid_request_body() {
    let response = test_server()
        .post("/items")
        .text("{")
        .content_type("application/json")
        .await;
    assert_problem(&response, 400, "invalid-request-body");
}

#[tokio::test]
async fn wrong_field_type_is_422_invalid_request_body() {
    let response = test_server()
        .post("/items")
        .text(r#"{"name": 5}"#)
        .content_type("application/json")
        .await;
    assert_problem(&response, 422, "invalid-request-body");
}

#[tokio::test]
async fn wrong_content_type_is_415_invalid_request_body() {
    let response = test_server().post("/items").text(r#"{"name": "x"}"#).await;
    assert_problem(&response, 415, "invalid-request-body");
}

#[tokio::test]
async fn oversized_body_is_413_invalid_request_body() {
    let response = test_server()
        .post("/items")
        .text(" ".repeat(3 * 1024 * 1024))
        .content_type("application/json")
        .await;
    assert_problem(&response, 413, "invalid-request-body");
}

#[tokio::test]
async fn valid_request_reaches_the_handler() {
    let response = test_server()
        .post("/items")
        .text(r#"{"name": "x"}"#)
        .content_type("application/json")
        .await;
    assert_eq!(response.status_code(), 200);
    assert_eq!(response.text(), "x");
}
