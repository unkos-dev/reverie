use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

#[allow(dead_code)] // Used by route handlers in subsequent steps
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("not found")]
    NotFound,
    #[error("unauthorized")]
    Unauthorized,
    #[error("validation error: {0}")]
    Validation(String),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            Self::NotFound => (StatusCode::NOT_FOUND, "not found".to_owned()),
            Self::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized".to_owned()),
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
    async fn internal_returns_500_without_leaking_details() {
        let inner = anyhow::anyhow!("secret database connection string leaked");
        let (status, body) = status_of(AppError::Internal(inner)).await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert!(!body.contains("secret"));
        assert!(!body.contains("database"));
        assert!(body.contains("internal server error"));
    }
}
