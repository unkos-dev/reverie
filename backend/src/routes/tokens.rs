use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use uuid::Uuid;

use crate::auth::middleware::CurrentUser;
use crate::auth::token::generate_device_token;
use crate::error::AppError;
use crate::models::device_token;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/tokens", post(create_token))
        .route("/api/tokens", get(list_tokens))
        .route("/api/tokens/{id}", delete(revoke_token))
}

#[derive(serde::Deserialize)]
struct CreateTokenRequest {
    name: String,
}

#[derive(serde::Serialize)]
struct CreateTokenResponse {
    id: Uuid,
    name: String,
    token: String,
}

async fn create_token(
    current_user: CurrentUser,
    State(state): State<AppState>,
    Json(body): Json<CreateTokenRequest>,
) -> Result<impl IntoResponse, AppError> {
    if body.name.trim().is_empty() || body.name.len() > 255 {
        return Err(AppError::Validation("name must be 1-255 characters".into()));
    }

    let (plaintext, hash) = generate_device_token();
    let dt = device_token::create_with_limit(&state.pool, current_user.user_id, &body.name, &hash)
        .await
        .map_err(|e| match e {
            device_token::CreateError::LimitExceeded => {
                AppError::Validation("maximum of 10 active device tokens per user".into())
            }
            device_token::CreateError::Db(e) => AppError::Internal(e.into()),
        })?;

    Ok((
        StatusCode::CREATED,
        Json(CreateTokenResponse {
            id: dt.id,
            name: dt.name,
            token: plaintext,
        }),
    ))
}

async fn list_tokens(
    current_user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, AppError> {
    let tokens = device_token::list_for_user(&state.pool, current_user.user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let items: Vec<serde_json::Value> = tokens
        .iter()
        .map(|t| {
            serde_json::json!({
                "id": t.id,
                "name": t.name,
                "last_used_at": t.last_used_at,
                "created_at": t.created_at,
            })
        })
        .collect();

    Ok(Json(serde_json::json!(items)))
}

async fn revoke_token(
    current_user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    let revoked = device_token::revoke(&state.pool, id, current_user.user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    if revoked {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::NotFound)
    }
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;

    use crate::test_support;

    #[tokio::test]
    async fn create_token_returns_401_without_auth() {
        let server = test_support::test_server();
        let response = server
            .post("/api/tokens")
            .json(&serde_json::json!({"name": "My Kindle"}))
            .await;
        assert_eq!(response.status_code(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn list_tokens_returns_401_without_auth() {
        let server = test_support::test_server();
        let response = server.get("/api/tokens").await;
        assert_eq!(response.status_code(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn revoke_token_returns_401_without_auth() {
        let server = test_support::test_server();
        let response = server
            .delete(&format!("/api/tokens/{}", uuid::Uuid::new_v4()))
            .await;
        assert_eq!(response.status_code(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    #[ignore] // Requires running postgres
    async fn create_token_validates_name() {
        let url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://tome_app:tome_app@localhost:5433/tome_dev".into());
        let pool = sqlx::PgPool::connect(&url).await.expect("connect");

        // Create a test user and device token for Basic auth
        let subject = format!("token-route-test-{}", uuid::Uuid::new_v4());
        let user = crate::models::user::upsert_from_oidc_and_maybe_promote(
            &pool,
            &subject,
            "Token Test User",
            None,
        )
        .await
        .expect("create user");
        let (plaintext, hash) = crate::auth::token::generate_device_token();
        crate::models::device_token::create(&pool, user.id, "auth-token", &hash)
            .await
            .expect("create token");

        let state = crate::state::AppState {
            pool: pool.clone(),
            config: test_support::test_config(),
            oidc_client: test_support::test_oidc_client(),
        };
        let auth_backend = crate::auth::backend::AuthBackend { pool: pool.clone() };
        let app = crate::build_router(state, auth_backend);
        let server = axum_test::TestServer::new(app);

        use base64ct::Encoding;
        let basic =
            base64ct::Base64::encode_string(format!("{}:{}", user.id, plaintext).as_bytes());

        // Empty name
        let response = server
            .post("/api/tokens")
            .add_header(axum::http::header::AUTHORIZATION, format!("Basic {basic}"))
            .json(&serde_json::json!({"name": ""}))
            .await;
        assert_eq!(response.status_code(), StatusCode::UNPROCESSABLE_ENTITY);

        // Name too long
        let long_name = "x".repeat(256);
        let response = server
            .post("/api/tokens")
            .add_header(axum::http::header::AUTHORIZATION, format!("Basic {basic}"))
            .json(&serde_json::json!({"name": long_name}))
            .await;
        assert_eq!(response.status_code(), StatusCode::UNPROCESSABLE_ENTITY);

        // Cleanup
        sqlx::query("DELETE FROM device_tokens WHERE user_id = $1")
            .bind(user.id)
            .execute(&pool)
            .await
            .expect("cleanup tokens");
        sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(user.id)
            .execute(&pool)
            .await
            .expect("cleanup user");
    }
}
