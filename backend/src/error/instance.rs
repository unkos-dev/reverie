//! Request-path capture for the RFC 9457 (obsoletes RFC 7807)
//! `instance` field.
//!
//! The `IntoResponse` impl on [`crate::error::AppError`] is a
//! value-level conversion with no access to the originating HTTP
//! request. RFC 9457 §3.1 recommends a problem document carry an
//! `instance` field identifying the specific occurrence — for our
//! API, the request path is the natural choice. We bridge the gap
//! with a tokio task-local: the [`problem_instance_layer`]
//! middleware stores the request path into the task-local before
//! `next.run()`, and [`current_request_uri`] reads it back from
//! inside the `IntoResponse` impl.
//!
//! Outside an HTTP request (unit-tests calling `.into_response()`
//! directly, background tasks) the task-local is unset and
//! [`current_request_uri`] returns `None`. The problem body simply
//! omits the `instance` field in that case — RFC 9457 §3.1 permits
//! omission.

use axum::extract::Request;
use axum::middleware::Next;
use axum::response::Response;

tokio::task_local! {
    static CURRENT_URI: String;
}

/// Tower middleware that captures the request path into a task-local
/// for [`current_request_uri`] to read.
///
/// Mount on the outermost API-bearing router so every response that
/// flows through the [`crate::error::AppError`] `IntoResponse` impl
/// can carry an `instance` field. Cost: one `String` clone per
/// request.
pub async fn problem_instance_layer(req: Request, next: Next) -> Response {
    let path = req.uri().path().to_owned();
    CURRENT_URI.scope(path, next.run(req)).await
}

/// Read the request path captured by [`problem_instance_layer`].
///
/// Returns `None` when called outside an HTTP request (e.g. a unit
/// test invoking `AppError::Validation(...).into_response()`
/// directly). The Problem Details body omits the `instance` field in
/// that case, which RFC 9457 §3.1 permits.
#[must_use]
pub fn current_request_uri() -> Option<String> {
    CURRENT_URI.try_with(String::clone).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::Router;
    use axum::middleware::from_fn;
    use axum::routing::get;
    use axum_test::TestServer;

    async fn handler() -> String {
        current_request_uri().unwrap_or_else(|| "<unset>".into())
    }

    #[tokio::test]
    async fn middleware_captures_request_path() {
        let app = Router::new()
            .route("/api/v1/books", get(handler))
            .layer(from_fn(problem_instance_layer));
        let server = TestServer::new(app);
        let r = server.get("/api/v1/books?cursor=foo").await;
        r.assert_status_ok();
        assert_eq!(r.text(), "/api/v1/books");
    }

    #[tokio::test]
    async fn outside_request_returns_none() {
        assert!(current_request_uri().is_none());
    }
}
