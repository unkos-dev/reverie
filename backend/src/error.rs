use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("not found")]
    NotFound,
    #[error("unauthorized")]
    Unauthorized,
    /// 401 that emits a `WWW-Authenticate: Basic` challenge (RFC 7617). Used by
    /// the `BasicOnly` extractor to signal OPDS clients to prompt for
    /// credentials. `realm` is operator-configured and validated at startup
    /// (no embedded `"` allowed).
    #[error("basic auth required")]
    BasicAuthRequired { realm: String },
    #[error("forbidden")]
    Forbidden,
    #[error("validation error: {0}")]
    Validation(String),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
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

        let (status, message) = match self {
            Self::NotFound => (StatusCode::NOT_FOUND, "not found".to_owned()),
            Self::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized".to_owned()),
            Self::BasicAuthRequired { .. } => unreachable!("handled above"),
            Self::Forbidden => (StatusCode::FORBIDDEN, "forbidden".to_owned()),
            Self::Validation(msg) => (StatusCode::UNPROCESSABLE_ENTITY, msg),
            Self::Internal(err) => {
                tracing::error!(error = %err, "internal server error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal server error".to_owned(),
                )
            }
        };

        let body = serde_json::json!({ "error": message });
        (status, axum::Json(body)).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;

    async fn status_of(err: AppError) -> (StatusCode, String) {
        let response = err.into_response();
        let status = response.status();
        let body = to_bytes(response.into_body(), 1024).await.unwrap();
        (status, String::from_utf8(body.to_vec()).unwrap())
    }

    #[tokio::test]
    async fn not_found_returns_404() {
        let (status, _) = status_of(AppError::NotFound).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn unauthorized_returns_401() {
        let (status, _) = status_of(AppError::Unauthorized).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn validation_returns_422_with_message() {
        let (status, body) = status_of(AppError::Validation("bad input".into())).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(body.contains("bad input"));
    }

    #[tokio::test]
    async fn forbidden_returns_403() {
        let (status, _) = status_of(AppError::Forbidden).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn basic_auth_required_emits_challenge() {
        let response = AppError::BasicAuthRequired {
            realm: "Reverie OPDS".into(),
        }
        .into_response();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let challenge = response
            .headers()
            .get(axum::http::header::WWW_AUTHENTICATE)
            .expect("WWW-Authenticate header present")
            .to_str()
            .unwrap()
            .to_owned();
        assert_eq!(challenge, r#"Basic realm="Reverie OPDS", charset="UTF-8""#);
        let body = to_bytes(response.into_body(), 1024).await.unwrap();
        assert!(body.is_empty(), "BasicAuthRequired body must be empty");
    }

    #[tokio::test]
    async fn internal_returns_500_without_leaking_details() {
        let inner = anyhow::anyhow!("secret database connection string leaked");
        let (status, body) = status_of(AppError::Internal(inner)).await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert!(!body.contains("secret"));
        assert!(!body.contains("database"));
        assert!(body.contains("internal server error"));
    }
}
