//! Application error type and HTTP response mapping.
//!
//! [`AppError`] is the single error returned by Axum handlers via
//! `Result<impl IntoResponse, AppError>`. Its
//! [`axum::response::IntoResponse`] impl emits RFC 9457 (obsoletes
//! RFC 7807) Problem Details (`application/problem+json`) for every
//! variant, including [`AppError::BasicAuthRequired`], which additionally
//! carries the RFC 7617 `WWW-Authenticate: Basic` challenge OPDS clients
//! (Atom XML feeds, e-readers) rely on to prompt for credentials; those
//! clients ignore the JSON body.
//!
//! See `docs/adr/0011-json-api-conventions-for-the-browser-facing-rest-surface.md` for the full
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
//! The `instance` field (RFC 9457 §3.1) carries the request path.
//! It is captured by the [`instance::problem_instance_layer`] tower
//! middleware mounted on the API router group, stored in a tokio
//! task-local, and read back when the
//! [`crate::openapi::ProblemDetails`] body is serialized. When called
//! outside an HTTP request (unit tests invoking `.into_response()`
//! directly), the task-local is unset and the `instance` field is
//! simply omitted from the body — RFC 9457 §3.1 permits omission.

pub mod instance;
pub mod problems;

use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};

/// Errors returned from Axum handlers; converted to RFC 9457 Problem
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
    /// Resource not found. RFC 9457 `type` slug
    /// [`problems::NOT_FOUND`]. HTTP 404. Also the mapping RLS-hidden
    /// rows resolve to — see the json-api-conventions ADR's
    /// "existence-not-leaked" decision.
    #[error("not found")]
    NotFound,
    /// Caller unauthenticated: no credential was presented at all. RFC 9457
    /// `type` [`problems::UNAUTHORIZED`]. HTTP 401 with a bare
    /// `WWW-Authenticate: Bearer` challenge (RFC 6750 §3) — use
    /// [`Self::BasicAuthRequired`] for the OPDS Basic-auth challenge variant,
    /// or [`Self::InvalidCredential`] when a credential WAS presented and
    /// rejected.
    #[error("unauthorized")]
    Unauthorized,
    /// Caller presented a credential (device token, JWT, or a malformed
    /// `Authorization` header) and it was rejected. Identical RFC 9457 body
    /// to [`Self::Unauthorized`] (same slug, title, detail, HTTP 401) but
    /// with `WWW-Authenticate: Bearer error="invalid_token"` (RFC 6750 §3.1)
    /// instead of a bare challenge, so a client can distinguish "no
    /// credential yet" from "that credential doesn't work" without parsing
    /// the body. Deliberately carries no detail about *why* the credential
    /// was rejected (expired vs. forged vs. unknown identity are all the
    /// same response) to avoid handing an attacker an oracle.
    #[error("invalid credential")]
    InvalidCredential,
    /// 401 with `WWW-Authenticate: Basic` challenge (RFC 7617) for OPDS
    /// Basic-auth clients (`KOReader`, `Foliate`), carrying the same RFC
    /// 9457 body as every other variant — e-reader clients ignore the body
    /// and act on the status and challenge header alone. RFC 9457 `type`
    /// [`problems::BASIC_AUTH_REQUIRED`]. `realm` is operator-configured
    /// and validated at startup (no embedded `"` allowed).
    #[error("basic auth required")]
    BasicAuthRequired {
        /// The `realm` value emitted in the `WWW-Authenticate: Basic`
        /// challenge. Pre-validated at config load.
        realm: String,
    },
    /// Caller authenticated but lacks the role / policy to perform
    /// the action. RFC 9457 `type` [`problems::FORBIDDEN`]. HTTP 403.
    #[error("forbidden")]
    Forbidden,
    /// Request validation failed (malformed input, business-rule
    /// violation). RFC 9457 `type` [`problems::VALIDATION`]. HTTP
    /// 422. The inner string is emitted as the `detail` field, so
    /// callers should keep it free of sensitive context.
    #[error("validation error: {0}")]
    Validation(String),
    /// `X-CSRF-Token` header missing on a mutating-verb request
    /// under `/api/v1/*`. RFC 9457 `type` [`problems::CSRF_MISSING`].
    /// HTTP 428 Precondition Required.
    #[error("CSRF token required")]
    CsrfMissing,
    /// `X-CSRF-Token` header present but does not match the
    /// session-stored token. RFC 9457 `type`
    /// [`problems::CSRF_MISMATCH`]. HTTP 403 Forbidden.
    #[error("CSRF token invalid")]
    CsrfMismatch,
    /// `If-Match` header missing on a precondition-protected
    /// endpoint. RFC 9457 `type` [`problems::IF_MATCH_REQUIRED`].
    /// HTTP 428 Precondition Required.
    #[error("If-Match header required")]
    IfMatchRequired,
    /// `If-Match` header present but `ETag` does not match current
    /// resource state. RFC 9457 `type`
    /// [`problems::IF_MATCH_MISMATCH`]. HTTP 412 Precondition Failed.
    #[error("If-Match precondition failed")]
    IfMatchMismatch,
    /// Mutation attempt on a system-managed shelf (`is_system =
    /// TRUE`). RFC 9457 `type` [`problems::SYSTEM_SHELF_IMMUTABLE`].
    /// HTTP 409 Conflict.
    #[error("system shelf cannot be modified")]
    SystemShelfImmutable,
    /// A query-string parameter failed to deserialize at the extractor
    /// boundary (e.g. a malformed UUID in `?author=` / `?series=` /
    /// `?shelf=`, or an unknown `?sort=` variant). RFC 9457 `type`
    /// [`problems::MALFORMED_QUERY`]. HTTP 400 Bad Request. Distinct
    /// from [`Self::Validation`] (422): this is a syntactic decode
    /// failure, not a business-rule rejection. The inner string is
    /// emitted verbatim as the `detail` field (the `#[error]` Display
    /// is not used on the wire). The sole production constructor is the
    /// [`From`] impl for [`axum_extra::extract::QueryRejection`], which
    /// synthesises the string from the rejection's `Display` — the
    /// caller's own query bytes plus the failing field name, never
    /// server-side state. A future caller constructing this variant
    /// directly must likewise keep the string free of sensitive
    /// context.
    #[error("{0}")]
    MalformedQuery(String),
    /// A request header failed to satisfy its grammar or usage contract
    /// (e.g. a malformed, weak, or list-form `If-Match` entity-tag). RFC
    /// 9457 `type` [`problems::MALFORMED_HEADER`]. HTTP 400 Bad Request.
    /// RFC 9110 scopes 422 to request *content* (the body), so a rejected
    /// header value is a syntactic decode failure at the request-line
    /// level, not a business-rule rejection, and belongs with
    /// [`Self::MalformedQuery`] rather than [`Self::Validation`]. The
    /// inner string is emitted verbatim as the `detail` field (the
    /// `#[error]` Display is not used on the wire).
    #[error("{0}")]
    MalformedHeader(String),
    /// Per-source login rate limit exceeded (governor, per client IP). RFC 9457
    /// `type` [`problems::RATE_LIMITED`]. HTTP 429 Too Many Requests. Raised by
    /// the login / recovery handlers; the per-account backoff is separate.
    #[error("too many requests")]
    RateLimited,
    /// First-run setup (bootstrap) attempted when an administrator already
    /// exists. RFC 9457 `type` [`problems::SETUP_ALREADY_COMPLETE`]. HTTP 409
    /// Conflict. The authoritative guard is the `instance_bootstrap` singleton
    /// insert losing the race, mapped here; a separate `SystemShelfImmutable`
    /// (also 409) is deliberately NOT reused.
    #[error("setup already complete")]
    SetupAlreadyComplete,
    /// Account creation or self-registration rejected because the email is
    /// already in use (`idx_users_email_lower`). RFC 9457 `type`
    /// [`problems::EMAIL_CONFLICT`]. HTTP 409 Conflict.
    #[error("email already in use")]
    EmailConflict,
    /// Method not supported on an otherwise-matched route. RFC 9457
    /// `type` [`problems::METHOD_NOT_ALLOWED`]. HTTP 405. Must not set
    /// its own `Allow` header — axum appends the registered methods
    /// after this response is produced.
    #[error("method not allowed")]
    MethodNotAllowed,
    /// Anything else — unhandled `sqlx::Error`, IO failure, etc. RFC
    /// 9457 `type` [`problems::INTERNAL`]. HTTP 500 with a fixed
    /// non-leaking `detail`; the inner cause is
    /// `tracing::error!`-logged with full context.
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

impl IntoResponse for AppError {
    // A flat, exhaustive variant -> RFC 9457 dispatcher: one trivial arm per
    // error variant. Splitting it would scatter the status/slug/title mapping
    // without reducing complexity, so the length lint is allowed here.
    #[expect(
        clippy::too_many_lines,
        reason = "flat exhaustive variant-to-RFC-9457 dispatcher; splitting scatters the status/slug/title mapping without reducing complexity"
    )]
    fn into_response(self) -> Response {
        // Computed before the variant is consumed below: the three
        // WWW-Authenticate variants carry a Problem Details body and
        // differ only in this header, so the header is applied to the
        // finished response rather than threaded through the body match.
        let challenge = match &self {
            Self::Unauthorized => Some(HeaderValue::from_static("Bearer")),
            Self::InvalidCredential => {
                Some(HeaderValue::from_static(r#"Bearer error="invalid_token""#))
            }
            Self::BasicAuthRequired { realm } => {
                let challenge = format!("Basic realm=\"{realm}\", charset=\"UTF-8\"");
                HeaderValue::from_str(&challenge).ok()
            }
            _ => None,
        };

        let (status, slug, title, detail) = match self {
            Self::NotFound => (
                StatusCode::NOT_FOUND,
                problems::NOT_FOUND,
                "Not Found",
                "Resource not found.".to_owned(),
            ),
            // Unauthorized and InvalidCredential share this body — see the
            // challenge computation above for the header that distinguishes
            // them on the wire.
            Self::Unauthorized | Self::InvalidCredential => (
                StatusCode::UNAUTHORIZED,
                problems::UNAUTHORIZED,
                "Unauthorized",
                "Authentication required.".to_owned(),
            ),
            Self::BasicAuthRequired { .. } => (
                StatusCode::UNAUTHORIZED,
                problems::BASIC_AUTH_REQUIRED,
                "Unauthorized",
                "Basic authentication required.".to_owned(),
            ),
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
            Self::SystemShelfImmutable => (
                StatusCode::CONFLICT,
                problems::SYSTEM_SHELF_IMMUTABLE,
                "Conflict",
                "System shelves cannot be renamed or deleted.".to_owned(),
            ),
            Self::MalformedQuery(msg) => (
                StatusCode::BAD_REQUEST,
                problems::MALFORMED_QUERY,
                "Bad Request",
                msg,
            ),
            Self::MalformedHeader(msg) => (
                StatusCode::BAD_REQUEST,
                problems::MALFORMED_HEADER,
                "Bad Request",
                msg,
            ),
            Self::RateLimited => (
                StatusCode::TOO_MANY_REQUESTS,
                problems::RATE_LIMITED,
                "Too Many Requests",
                "Too many login attempts; please try again later.".to_owned(),
            ),
            Self::SetupAlreadyComplete => (
                StatusCode::CONFLICT,
                problems::SETUP_ALREADY_COMPLETE,
                "Conflict",
                "An administrator already exists; first-run setup is closed.".to_owned(),
            ),
            Self::EmailConflict => (
                StatusCode::CONFLICT,
                problems::EMAIL_CONFLICT,
                "Conflict",
                "An account with that email already exists.".to_owned(),
            ),
            Self::MethodNotAllowed => (
                StatusCode::METHOD_NOT_ALLOWED,
                problems::METHOD_NOT_ALLOWED,
                "Method Not Allowed",
                "The request method is not supported for this resource.".to_owned(),
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

        // The shared runtime DTO owns serialization, the problem+json
        // content type, and the instance capture from the request
        // task-local — one wire shape for every emitter.
        let mut response = crate::openapi::ProblemDetails {
            r#type: problems::problem_type(slug),
            title: title.to_owned(),
            status: status.as_u16(),
            detail,
            instance: None,
        }
        .into_response();

        if let Some(value) = challenge {
            response
                .headers_mut()
                .insert(header::WWW_AUTHENTICATE, value);
        }

        response
    }
}

/// Route the `axum_extra::extract::Query` rejection through the RFC
/// 9457 envelope (HTTP 400) instead of the framework default (a
/// non-RFC-9457 JSON 400 of the form `{"error": "..."}`). Handlers opt
/// in by extracting `Result<Query<T>, QueryRejection>` and
/// `?`-propagating the error.
impl From<axum_extra::extract::QueryRejection> for AppError {
    fn from(rejection: axum_extra::extract::QueryRejection) -> Self {
        Self::MalformedQuery(format!("malformed query parameter: {rejection}"))
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
    async fn unauthorized_carries_bare_bearer_challenge() {
        let response = AppError::Unauthorized.into_response();
        let challenge = response
            .headers()
            .get(header::WWW_AUTHENTICATE)
            .expect("WWW-Authenticate header present")
            .to_str()
            .unwrap();
        assert_eq!(
            challenge, "Bearer",
            "no credential was presented; the challenge must not claim invalid_token"
        );
    }

    #[tokio::test]
    async fn invalid_credential_returns_same_body_as_unauthorized() {
        let (status, _, json) = parse_problem(AppError::InvalidCredential).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_problem_shape(&json, problems::UNAUTHORIZED, 401, "Unauthorized");
    }

    #[tokio::test]
    async fn invalid_credential_carries_invalid_token_challenge() {
        let response = AppError::InvalidCredential.into_response();
        let challenge = response
            .headers()
            .get(header::WWW_AUTHENTICATE)
            .expect("WWW-Authenticate header present")
            .to_str()
            .unwrap();
        assert_eq!(
            challenge, r#"Bearer error="invalid_token""#,
            "a rejected credential must be distinguishable from no credential at all"
        );
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
    async fn malformed_query_returns_400_with_message_in_detail() {
        let (status, _, json) = parse_problem(AppError::MalformedQuery(
            "malformed query parameter: x".into(),
        ))
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_problem_shape(&json, problems::MALFORMED_QUERY, 400, "Bad Request");
        assert_eq!(
            json["detail"].as_str().unwrap(),
            "malformed query parameter: x"
        );
    }

    #[tokio::test]
    async fn malformed_header_returns_400_with_message_in_detail() {
        let (status, _, json) = parse_problem(AppError::MalformedHeader(
            "If-Match header must be ASCII".into(),
        ))
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_problem_shape(&json, problems::MALFORMED_HEADER, 400, "Bad Request");
        assert_eq!(
            json["detail"].as_str().unwrap(),
            "If-Match header must be ASCII"
        );
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
    async fn system_shelf_immutable_returns_409_problem() {
        let (status, _, json) = parse_problem(AppError::SystemShelfImmutable).await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_problem_shape(&json, problems::SYSTEM_SHELF_IMMUTABLE, 409, "Conflict");
    }

    #[tokio::test]
    async fn method_not_allowed_returns_405_problem() {
        let (status, _, json) = parse_problem(AppError::MethodNotAllowed).await;
        assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED);
        assert_problem_shape(
            &json,
            problems::METHOD_NOT_ALLOWED,
            405,
            "Method Not Allowed",
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
    async fn basic_auth_required_carries_challenge_and_problem_body() {
        let challenge_response = AppError::BasicAuthRequired {
            realm: "Reverie OPDS".into(),
        }
        .into_response();
        let challenge = challenge_response
            .headers()
            .get(header::WWW_AUTHENTICATE)
            .expect("WWW-Authenticate header present")
            .to_str()
            .unwrap()
            .to_owned();
        assert_eq!(challenge, r#"Basic realm="Reverie OPDS", charset="UTF-8""#);

        let (status, ct, json) = parse_problem(AppError::BasicAuthRequired {
            realm: "Reverie OPDS".into(),
        })
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert!(
            ct.contains("application/problem+json"),
            "wrong content-type: {ct}"
        );
        assert_problem_shape(&json, problems::BASIC_AUTH_REQUIRED, 401, "Unauthorized");
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
