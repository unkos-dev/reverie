//! Application error type and HTTP response mapping.
//!
//! [`AppError`] is the single error returned by Axum handlers via
//! `Result<impl IntoResponse, AppError>`. Its
//! [`axum::response::IntoResponse`] impl emits RFC 7807 Problem
//! Details (`application/problem+json`) for every variant except
//! [`AppError::BasicAuthRequired`], which keeps its pre-7807
//! empty-body + `WWW-Authenticate: Basic` shape per RFC 7617 because
//! it is consumed by OPDS clients (Atom XML feeds, e-readers) that
//! expect that exact shape — RFC 7807 applies to the JSON API
//! surface only.
//!
//! See `adr/2026-05-22-json-api-conventions.md` for the full
//! convention and migration rationale.
//!
//! # Information disclosure
//!
//! Internal errors (anything wrapped in [`AppError::Internal`])
//! deliberately do **not** leak the inner cause's message to clients
//! — the cause is `tracing::error!`-logged with full context and the
//! response `detail` is a fixed `"An internal error occurred."`
//! string. Handlers may `?`-propagate errors whose `Display`
//! includes connection strings, file paths, or other sensitive
//! operational detail; the centralised mapping here ensures that
//! detail never reaches the network.
//!
//! # `instance` field
//!
//! The `instance` field (RFC 7807 §3.1) carries the request path.
//! It is captured by the [`instance::problem_instance_layer`] tower
//! middleware mounted on the API router group, stored in a tokio
//! task-local, and read back by [`AppError::into_response`]. When
//! called outside an HTTP request (unit tests invoking
//! `.into_response()` directly), the task-local is unset and the
//! `instance` field is simply omitted from the body — RFC 7807 §3.1
//! permits omission.

pub mod instance;
pub mod problems;

use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};

/// Errors returned from Axum handlers; converted to RFC 7807 Problem
/// Details JSON responses by the [`IntoResponse`] impl on this type.
///
/// Handlers convert library errors via `?` (using `#[from]` on
/// [`Self::Internal`] for `anyhow::Error` and any
/// `Into<anyhow::Error>` type — `sqlx::Error` and friends).
/// Domain-specific failures use the dedicated variants
/// ([`Self::NotFound`], [`Self::Validation`]) so the HTTP mapping is
/// explicit at the call site rather than buried inside an `anyhow`
/// chain.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum AppError {
    /// Resource not found. RFC 7807 `type` slug
    /// [`problems::NOT_FOUND`]. HTTP 404. Also the mapping RLS-hidden
    /// rows resolve to — see the json-api-conventions ADR's
    /// "existence-not-leaked" decision.
    #[error("not found")]
    NotFound,
    /// Caller unauthenticated. RFC 7807 `type`
    /// [`problems::UNAUTHORIZED`]. HTTP 401, no `WWW-Authenticate`
    /// challenge — use [`Self::BasicAuthRequired`] for the OPDS
    /// Basic-auth challenge variant.
    #[error("unauthorized")]
    Unauthorized,
    /// 401 with `WWW-Authenticate: Basic` challenge (RFC 7617).
    /// **Not RFC 7807** — emits an EMPTY body for compatibility with
    /// OPDS Basic-auth clients (`KOReader`, `Foliate`). `realm` is
    /// operator-configured and validated at startup (no embedded
    /// `"` allowed).
    #[error("basic auth required")]
    BasicAuthRequired {
        /// The `realm` value emitted in the `WWW-Authenticate: Basic`
        /// challenge. Pre-validated at config load.
        realm: String,
    },
    /// Caller authenticated but lacks the role / policy to perform
    /// the action. RFC 7807 `type` [`problems::FORBIDDEN`]. HTTP 403.
    #[error("forbidden")]
    Forbidden,
    /// Request validation failed (malformed input, business-rule
    /// violation). RFC 7807 `type` [`problems::VALIDATION`]. HTTP
    /// 422. The inner string is emitted as the `detail` field, so
    /// callers should keep it free of sensitive context.
    #[error("validation error: {0}")]
    Validation(String),
    /// `X-CSRF-Token` header missing on a mutating-verb request
    /// under `/api/*`. RFC 7807 `type` [`problems::CSRF_MISSING`].
    /// HTTP 428 Precondition Required.
    #[error("CSRF token required")]
    CsrfMissing,
    /// `X-CSRF-Token` header present but does not match the
    /// session-stored token. RFC 7807 `type`
    /// [`problems::CSRF_MISMATCH`]. HTTP 403 Forbidden.
    #[error("CSRF token invalid")]
    CsrfMismatch,
    /// `If-Match` header missing on a precondition-protected
    /// endpoint. RFC 7807 `type` [`problems::IF_MATCH_REQUIRED`].
    /// HTTP 428 Precondition Required.
    #[error("If-Match header required")]
    IfMatchRequired,
    /// `If-Match` header present but `ETag` does not match current
    /// resource state. RFC 7807 `type`
    /// [`problems::IF_MATCH_MISMATCH`]. HTTP 412 Precondition Failed.
    #[error("If-Match precondition failed")]
    IfMatchMismatch,
    /// Anything else — unhandled `sqlx::Error`, IO failure, etc. RFC
    /// 7807 `type` [`problems::INTERNAL`]. HTTP 500 with a fixed
    /// non-leaking `detail`; the inner cause is
    /// `tracing::error!`-logged with full context.
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        // BasicAuthRequired keeps its pre-7807 shape: empty body +
        // WWW-Authenticate per RFC 7617. OPDS clients depend on this
        // exact contract; the JSON API surface uses 7807 for every
        // other variant.
        if let Self::BasicAuthRequired { realm } = &self {
            let challenge = format!("Basic realm=\"{realm}\", charset=\"UTF-8\"");
            let mut response = Response::new(axum::body::Body::empty());
            *response.status_mut() = StatusCode::UNAUTHORIZED;
            if let Ok(value) = HeaderValue::from_str(&challenge) {
                response
                    .headers_mut()
                    .insert(header::WWW_AUTHENTICATE, value);
            }
            return response;
        }

        let (status, slug, title, detail) = match self {
            Self::NotFound => (
                StatusCode::NOT_FOUND,
                problems::NOT_FOUND,
                "Not Found",
                "Resource not found.".to_owned(),
            ),
            Self::Unauthorized => (
                StatusCode::UNAUTHORIZED,
                problems::UNAUTHORIZED,
                "Unauthorized",
                "Authentication required.".to_owned(),
            ),
            Self::BasicAuthRequired { .. } => unreachable!("handled above"),
            Self::Forbidden => (
                StatusCode::FORBIDDEN,
                problems::FORBIDDEN,
                "Forbidden",
                "Access denied.".to_owned(),
            ),
            Self::Validation(msg) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                problems::VALIDATION,
                "Unprocessable Entity",
                msg,
            ),
            Self::CsrfMissing => (
                StatusCode::PRECONDITION_REQUIRED,
                problems::CSRF_MISSING,
                "Precondition Required",
                "X-CSRF-Token header required.".to_owned(),
            ),
            Self::CsrfMismatch => (
                StatusCode::FORBIDDEN,
                problems::CSRF_MISMATCH,
                "Forbidden",
                "CSRF token invalid.".to_owned(),
            ),
            Self::IfMatchRequired => (
                StatusCode::PRECONDITION_REQUIRED,
                problems::IF_MATCH_REQUIRED,
                "Precondition Required",
                "If-Match header required.".to_owned(),
            ),
            Self::IfMatchMismatch => (
                StatusCode::PRECONDITION_FAILED,
                problems::IF_MATCH_MISMATCH,
                "Precondition Failed",
                "Resource changed since last read.".to_owned(),
            ),
            Self::Internal(err) => {
                tracing::error!(error = %err, "internal server error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    problems::INTERNAL,
                    "Internal Server Error",
                    "An internal error occurred.".to_owned(),
                )
            }
        };

        let mut body = serde_json::json!({
            "type": problems::problem_type(slug),
            "title": title,
            "status": status.as_u16(),
            "detail": detail,
        });
        if let Some(instance_uri) = instance::current_request_uri()
            && let Some(obj) = body.as_object_mut()
        {
            obj.insert("instance".into(), serde_json::Value::String(instance_uri));
        }

        let mut response = (status, axum::Json(body)).into_response();
        response.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/problem+json"),
        );
        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;

    async fn parse_problem(err: AppError) -> (StatusCode, String, serde_json::Value) {
        let response = err.into_response();
        let status = response.status();
        let content_type = response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_owned();
        let body = to_bytes(response.into_body(), 4096).await.unwrap();
        let json: serde_json::Value =
            serde_json::from_slice(&body).expect("response body parses as JSON");
        (status, content_type, json)
    }

    fn assert_problem_shape(
        json: &serde_json::Value,
        expected_slug: &str,
        expected_status: u16,
        expected_title: &str,
    ) {
        let typ = json["type"].as_str().expect("type field present");
        assert!(
            typ.ends_with(&format!("/{expected_slug}")),
            "expected type ending in /{expected_slug}, got {typ}"
        );
        assert_eq!(
            json["status"].as_u64().expect("status field present"),
            u64::from(expected_status),
        );
        assert_eq!(
            json["title"].as_str().expect("title field present"),
            expected_title,
        );
        assert!(
            json["detail"].as_str().is_some(),
            "detail field present, got {json}",
        );
    }

    #[tokio::test]
    async fn not_found_returns_404_problem() {
        let (status, ct, json) = parse_problem(AppError::NotFound).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert!(
            ct.contains("application/problem+json"),
            "wrong content-type: {ct}"
        );
        assert_problem_shape(&json, problems::NOT_FOUND, 404, "Not Found");
    }

    #[tokio::test]
    async fn unauthorized_returns_401_problem() {
        let (status, _, json) = parse_problem(AppError::Unauthorized).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_problem_shape(&json, problems::UNAUTHORIZED, 401, "Unauthorized");
    }

    #[tokio::test]
    async fn forbidden_returns_403_problem() {
        let (status, _, json) = parse_problem(AppError::Forbidden).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_problem_shape(&json, problems::FORBIDDEN, 403, "Forbidden");
    }

    #[tokio::test]
    async fn validation_returns_422_with_message_in_detail() {
        let (status, _, json) = parse_problem(AppError::Validation("bad input".into())).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_problem_shape(&json, problems::VALIDATION, 422, "Unprocessable Entity");
        assert_eq!(json["detail"].as_str().unwrap(), "bad input");
    }

    #[tokio::test]
    async fn csrf_missing_returns_428_problem() {
        let (status, _, json) = parse_problem(AppError::CsrfMissing).await;
        assert_eq!(status, StatusCode::PRECONDITION_REQUIRED);
        assert_problem_shape(&json, problems::CSRF_MISSING, 428, "Precondition Required");
    }

    #[tokio::test]
    async fn csrf_mismatch_returns_403_problem() {
        let (status, _, json) = parse_problem(AppError::CsrfMismatch).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_problem_shape(&json, problems::CSRF_MISMATCH, 403, "Forbidden");
    }

    #[tokio::test]
    async fn if_match_required_returns_428_problem() {
        let (status, _, json) = parse_problem(AppError::IfMatchRequired).await;
        assert_eq!(status, StatusCode::PRECONDITION_REQUIRED);
        assert_problem_shape(
            &json,
            problems::IF_MATCH_REQUIRED,
            428,
            "Precondition Required",
        );
    }

    #[tokio::test]
    async fn if_match_mismatch_returns_412_problem() {
        let (status, _, json) = parse_problem(AppError::IfMatchMismatch).await;
        assert_eq!(status, StatusCode::PRECONDITION_FAILED);
        assert_problem_shape(
            &json,
            problems::IF_MATCH_MISMATCH,
            412,
            "Precondition Failed",
        );
    }

    #[tokio::test]
    async fn internal_returns_500_without_leaking_details() {
        let inner = anyhow::anyhow!("secret database connection string leaked");
        let (status, _, json) = parse_problem(AppError::Internal(inner)).await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_problem_shape(&json, problems::INTERNAL, 500, "Internal Server Error");
        let detail = json["detail"].as_str().unwrap();
        assert!(!detail.contains("secret"));
        assert!(!detail.contains("database"));
        assert_eq!(detail, "An internal error occurred.");
    }

    #[tokio::test]
    async fn basic_auth_required_keeps_pre_7807_shape() {
        let response = AppError::BasicAuthRequired {
            realm: "Reverie OPDS".into(),
        }
        .into_response();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let challenge = response
            .headers()
            .get(header::WWW_AUTHENTICATE)
            .expect("WWW-Authenticate header present")
            .to_str()
            .unwrap()
            .to_owned();
        assert_eq!(challenge, r#"Basic realm="Reverie OPDS", charset="UTF-8""#);
        let body = to_bytes(response.into_body(), 1024).await.unwrap();
        assert!(body.is_empty(), "BasicAuthRequired body must be empty");
    }

    #[tokio::test]
    async fn instance_omitted_outside_request() {
        let (_, _, json) = parse_problem(AppError::NotFound).await;
        assert!(
            json.get("instance").is_none(),
            "instance must be omitted when task-local is unset, got: {json}"
        );
    }
}
