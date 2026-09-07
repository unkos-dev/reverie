//! Request extractors whose rejections are Problem Details.
//!
//! Axum's `Path` and `Json` answer a malformed request with a plain-text body
//! before the handler runs. These wrappers call the same extractors and map
//! each rejection into [`AppError`], so every failure the API raises on the
//! way into a handler is an `application/problem+json` document. They are
//! request-only: `axum::Json` remains the response type.

use axum::extract::{FromRequest, FromRequestParts};

use crate::error::AppError;

/// Path parameters; a value that fails to parse is a 400 `malformed-path`.
#[derive(Debug, FromRequestParts)]
#[from_request(via(axum::extract::Path), rejection(AppError))]
pub struct ApiPath<T>(pub T);

/// JSON request body; a rejection keeps axum's status under `invalid-request-body`.
#[derive(Debug, FromRequest)]
#[from_request(via(axum::Json), rejection(AppError))]
pub struct ApiJson<T>(pub T);

#[cfg(test)]
mod tests {
    use axum::Router;
    use axum::response::IntoResponse;
    use axum::routing::{get, post};
    use axum_test::TestServer;
    use uuid::Uuid;

    use super::{ApiJson, ApiPath};

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

    #[tokio::test]
    async fn malformed_path_parameter_is_400_malformed_path() {
        let server = test_server();
        let response = server.get("/items/not-a-uuid").await;
        assert_eq!(response.status_code(), 400);
        assert_eq!(response.content_type(), "application/problem+json");
        let body: serde_json::Value = response.json();
        assert!(
            body["type"].as_str().unwrap().ends_with("/malformed-path"),
            "unexpected type: {body}"
        );
    }

    #[tokio::test]
    async fn invalid_json_syntax_is_400_invalid_request_body() {
        let server = test_server();
        let response = server
            .post("/items")
            .text("{")
            .content_type("application/json")
            .await;
        assert_eq!(response.status_code(), 400);
        assert_eq!(response.content_type(), "application/problem+json");
        let body: serde_json::Value = response.json();
        assert!(
            body["type"]
                .as_str()
                .unwrap()
                .ends_with("/invalid-request-body"),
            "unexpected type: {body}"
        );
    }

    #[tokio::test]
    async fn wrong_field_type_is_422_invalid_request_body() {
        let server = test_server();
        let response = server
            .post("/items")
            .text(r#"{"name": 5}"#)
            .content_type("application/json")
            .await;
        assert_eq!(response.status_code(), 422);
        assert_eq!(response.content_type(), "application/problem+json");
        let body: serde_json::Value = response.json();
        assert!(
            body["type"]
                .as_str()
                .unwrap()
                .ends_with("/invalid-request-body"),
            "unexpected type: {body}"
        );
    }

    #[tokio::test]
    async fn wrong_content_type_is_415_invalid_request_body() {
        let server = test_server();
        let response = server.post("/items").text(r#"{"name": "x"}"#).await;
        assert_eq!(response.status_code(), 415);
        assert_eq!(response.content_type(), "application/problem+json");
        let body: serde_json::Value = response.json();
        assert!(
            body["type"]
                .as_str()
                .unwrap()
                .ends_with("/invalid-request-body"),
            "unexpected type: {body}"
        );
    }

    #[tokio::test]
    async fn oversized_body_is_413_invalid_request_body() {
        let server = test_server();
        let oversized_body = " ".repeat(3 * 1024 * 1024);
        let response = server
            .post("/items")
            .text(oversized_body)
            .content_type("application/json")
            .await;
        assert_eq!(response.status_code(), 413);
        assert_eq!(response.content_type(), "application/problem+json");
        let body: serde_json::Value = response.json();
        assert!(
            body["type"]
                .as_str()
                .unwrap()
                .ends_with("/invalid-request-body"),
            "unexpected type: {body}"
        );
    }

    #[tokio::test]
    async fn valid_request_reaches_the_handler() {
        let server = test_server();
        let response = server
            .post("/items")
            .text(r#"{"name": "x"}"#)
            .content_type("application/json")
            .await;
        assert_eq!(response.status_code(), 200);
        assert_eq!(response.text(), "x");
    }
}
