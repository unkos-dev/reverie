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
    let dt = device_token::create(&state.pool, current_user.user_id, &body.name, &hash)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

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
