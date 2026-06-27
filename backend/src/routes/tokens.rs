//! Per-user device-token issue / list / revoke endpoints.
//!
//! Token plaintext is returned exactly once on `POST /api/v1/tokens` — it
//! is never persisted, only its SHA-256 hash. Subsequent `GET` and
//! `DELETE` operate on the row id; the plaintext cannot be recovered.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use time::OffsetDateTime;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::auth::middleware::CurrentUser;
use crate::auth::token::generate_device_token;
use crate::error::AppError;
use crate::models::device_token;
use crate::state::AppState;

/// Build the device-token management router as an [`OpenApiRouter`] so each
/// handler's `#[utoipa::path]` contributes to the generated spec (a missing
/// annotation fails to compile). Merged into `crate::openapi::pilot_router`
/// and split into its runtime and spec halves there.
pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(create_token, list_tokens))
        .routes(routes!(revoke_token))
}

#[derive(serde::Deserialize, utoipa::ToSchema)]
struct CreateTokenRequest {
    /// User-supplied label for the device (1-255 characters after trim).
    #[schema(example = "My Kindle")]
    name: String,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
struct CreateTokenResponse {
    /// Token row id — the handle for later revocation.
    id: Uuid,
    /// The label the token was created with (trimmed).
    name: String,
    /// Token plaintext. Returned exactly once, here; only its SHA-256 hash
    /// is persisted, so the plaintext cannot be recovered later.
    token: String,
}

/// One active device token in the list view. The token plaintext and its
/// stored hash are never part of this shape.
#[derive(serde::Serialize, utoipa::ToSchema)]
struct TokenListItem {
    /// Token row id.
    id: Uuid,
    /// User-supplied label.
    name: String,
    /// Timestamp of the last successful auth with this token; `null` if
    /// the token has never been used.
    last_used_at: Option<OffsetDateTime>,
    /// Token creation timestamp.
    created_at: OffsetDateTime,
}

/// `POST /api/v1/tokens` — issue a new device token for the caller.
///
/// # Errors
/// - [`AppError::Validation`] when the trimmed name is empty or longer than
///   255 characters, or the caller already has 10 active tokens.
/// - [`AppError::Internal`] on database errors.
#[utoipa::path(
    post,
    path = "/api/v1/tokens",
    tag = "tokens",
    request_body = CreateTokenRequest,
    responses(
        (status = 201, description = "Token created. The `token` field is the plaintext, returned exactly once — only its SHA-256 hash is persisted.", body = CreateTokenResponse),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails),
        (status = 422, description = "Name empty / longer than 255 characters after trim, or the per-user cap of 10 active tokens is reached", body = crate::openapi::ProblemDetails)
    )
)]
async fn create_token(
    current_user: CurrentUser,
    State(state): State<AppState>,
    Json(body): Json<CreateTokenRequest>,
) -> Result<impl IntoResponse, AppError> {
    let name = body.name.trim();
    if name.is_empty() || name.chars().count() > 255 {
        return Err(AppError::Validation("name must be 1-255 characters".into()));
    }

    let (plaintext, hash) = generate_device_token();
    let dt = device_token::create_with_limit(&state.pool, current_user.user_id, name, &hash)
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

/// `GET /api/v1/tokens` — list the caller's active (non-revoked) tokens.
///
/// # Errors
/// - [`AppError::Internal`] on database errors.
#[utoipa::path(
    get,
    path = "/api/v1/tokens",
    tag = "tokens",
    responses(
        (status = 200, description = "The caller's active device tokens, hash and plaintext elided", body = [TokenListItem]),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails)
    )
)]
async fn list_tokens(
    current_user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<Vec<TokenListItem>>, AppError> {
    let tokens = device_token::list_for_user(&state.pool, current_user.user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let items: Vec<TokenListItem> = tokens
        .into_iter()
        .map(|t| TokenListItem {
            id: t.id,
            name: t.name,
            last_used_at: t.last_used_at,
            created_at: t.created_at,
        })
        .collect();

    Ok(Json(items))
}

/// `DELETE /api/v1/tokens/{id}` — revoke one of the caller's tokens.
///
/// # Errors
/// - [`AppError::NotFound`] when the id does not exist, belongs to another
///   user, or is already revoked (ownership is not leaked).
/// - [`AppError::Internal`] on database errors.
#[utoipa::path(
    delete,
    path = "/api/v1/tokens/{id}",
    tag = "tokens",
    params(("id" = Uuid, Path, description = "Token row id")),
    responses(
        (status = 204, description = "Token revoked"),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails),
        (status = 404, description = "Token does not exist, belongs to another user, or is already revoked", body = crate::openapi::ProblemDetails)
    )
)]
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
#[allow(
    clippy::items_after_statements,
    reason = "test code: `use base64ct::Encoding` placed inline where needed in test function body"
)]
mod tests {
    use axum::http::StatusCode;

    use crate::test_support;

    #[tokio::test]
    async fn create_token_returns_401_without_auth() {
        let server = test_support::test_server();
        let response = server
            .post("/api/v1/tokens")
            .json(&serde_json::json!({"name": "My Kindle"}))
            .await;
        assert_eq!(response.status_code(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn list_tokens_returns_401_without_auth() {
        let server = test_support::test_server();
        let response = server.get("/api/v1/tokens").await;
        assert_eq!(response.status_code(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn revoke_token_returns_401_without_auth() {
        let server = test_support::test_server();
        let response = server
            .delete(&format!("/api/v1/tokens/{}", uuid::Uuid::new_v4()))
            .await;
        assert_eq!(response.status_code(), StatusCode::UNAUTHORIZED);
    }

    /// Provision a user + device token in the per-test database and return a
    /// `TestServer` plus the `Basic` credential string authenticating as that
    /// user. Centralises the auth/state/router wiring shared by the
    /// create-token tests so a change to `AppState` or the auth flow lands in
    /// one place.
    async fn authenticated_test_server(pool: sqlx::PgPool) -> (axum_test::TestServer, String) {
        let pool = test_support::db::app_pool_for(&pool).await;

        let subject = format!("token-route-test-{}", uuid::Uuid::new_v4());
        let user = crate::models::user::upsert_from_oidc(
            &pool,
            "https://token-test-issuer.example.com",
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
            ingestion_pool: pool.clone(),
            config: test_support::test_config(),
            oidc_client: Some(test_support::test_oidc_client()),
            login_limiter: test_support::test_login_limiter(),
            settings: test_support::test_settings(),
            last_settings_reload: std::sync::Arc::new(tokio::sync::RwLock::new(None)),
        };
        let app = crate::build_router(state);
        let server = axum_test::TestServer::new(app);

        use base64ct::Encoding;
        let basic =
            base64ct::Base64::encode_string(format!("{}:{}", user.id, plaintext).as_bytes());

        (server, basic)
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn list_tokens_pins_wire_shape(pool: sqlx::PgPool) {
        let (server, basic) = authenticated_test_server(pool).await;

        // A token created but never used for auth — last_used_at stays null.
        let created = server
            .post("/api/v1/tokens")
            .add_header(axum::http::header::AUTHORIZATION, format!("Basic {basic}"))
            .json(&serde_json::json!({"name": "reader"}))
            .await;
        assert_eq!(created.status_code(), StatusCode::CREATED);
        let created_id = created.json::<serde_json::Value>()["id"].clone();

        let response = server
            .get("/api/v1/tokens")
            .add_header(axum::http::header::AUTHORIZATION, format!("Basic {basic}"))
            .await;
        assert_eq!(response.status_code(), StatusCode::OK);
        let body = response.json::<serde_json::Value>();
        let items = body.as_array().expect("list response is a JSON array");
        // The helper's auth token plus the one created above.
        assert_eq!(items.len(), 2);

        let reader = items
            .iter()
            .find(|t| t["id"] == created_id)
            .expect("created token present in list");
        assert_eq!(reader["name"], "reader");
        assert!(reader["last_used_at"].is_null());
        assert!(!reader["created_at"].is_null());

        // The helper's auth token is also listed. Its last_used_at is NOT
        // asserted non-null here: verify_basic schedules the
        // update_last_used write on a detached tokio::spawn, so the value
        // is racy from the response's point of view. The debounced-touch
        // behaviour has its own tests in models::device_token.
        assert!(items.iter().any(|t| t["name"] == "auth-token"));

        // Exactly the documented TokenListItem fields — neither the token
        // plaintext nor its stored hash may appear in the list shape.
        for item in items {
            let obj = item.as_object().expect("list item is a JSON object");
            assert_eq!(obj.len(), 4, "unexpected fields in {obj:?}");
            for key in ["id", "name", "last_used_at", "created_at"] {
                assert!(obj.contains_key(key), "missing {key} in {obj:?}");
            }
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn create_token_validates_name(pool: sqlx::PgPool) {
        let (server, basic) = authenticated_test_server(pool).await;

        // Empty name
        let response = server
            .post("/api/v1/tokens")
            .add_header(axum::http::header::AUTHORIZATION, format!("Basic {basic}"))
            .json(&serde_json::json!({"name": ""}))
            .await;
        assert_eq!(response.status_code(), StatusCode::UNPROCESSABLE_ENTITY);

        // Name too long
        let long_name = "x".repeat(256);
        let response = server
            .post("/api/v1/tokens")
            .add_header(axum::http::header::AUTHORIZATION, format!("Basic {basic}"))
            .json(&serde_json::json!({"name": long_name}))
            .await;
        assert_eq!(response.status_code(), StatusCode::UNPROCESSABLE_ENTITY);

        // Exactly 255 characters is the accepted upper boundary — pins the
        // `> 255` comparison against a `>= 255` off-by-one regression.
        let max_name = "x".repeat(255);
        let response = server
            .post("/api/v1/tokens")
            .add_header(axum::http::header::AUTHORIZATION, format!("Basic {basic}"))
            .json(&serde_json::json!({"name": max_name}))
            .await;
        assert_eq!(response.status_code(), StatusCode::CREATED);

        // The limit counts characters, not bytes: 255 multibyte chars (510
        // bytes) is accepted, matching the "1-255 characters" message.
        let multibyte = "é".repeat(255);
        let response = server
            .post("/api/v1/tokens")
            .add_header(axum::http::header::AUTHORIZATION, format!("Basic {basic}"))
            .json(&serde_json::json!({"name": multibyte}))
            .await;
        assert_eq!(response.status_code(), StatusCode::CREATED);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn create_token_trims_name(pool: sqlx::PgPool) {
        let (server, basic) = authenticated_test_server(pool).await;

        // Whitespace-only rejects as empty-after-trim, not as a length pass.
        let response = server
            .post("/api/v1/tokens")
            .add_header(axum::http::header::AUTHORIZATION, format!("Basic {basic}"))
            .json(&serde_json::json!({"name": "   "}))
            .await;
        assert_eq!(response.status_code(), StatusCode::UNPROCESSABLE_ENTITY);

        // 256 spaces is empty after trim — must reject as empty, not run the
        // pre-trim length branch.
        let response = server
            .post("/api/v1/tokens")
            .add_header(axum::http::header::AUTHORIZATION, format!("Basic {basic}"))
            .json(&serde_json::json!({"name": " ".repeat(256)}))
            .await;
        assert_eq!(response.status_code(), StatusCode::UNPROCESSABLE_ENTITY);

        // Raw byte count (258) exceeds 255 but the trimmed name (254) is valid:
        // the old pre-trim length check rejected this, the fix accepts it. This
        // is the case that distinguishes the length-half of the bug.
        let padded = format!("  {}  ", "a".repeat(254));
        let response = server
            .post("/api/v1/tokens")
            .add_header(axum::http::header::AUTHORIZATION, format!("Basic {basic}"))
            .json(&serde_json::json!({"name": padded}))
            .await;
        assert_eq!(response.status_code(), StatusCode::CREATED);
        assert_eq!(
            response.json::<serde_json::Value>()["name"],
            "a".repeat(254)
        );

        // Surrounding whitespace is accepted and stripped before persistence.
        let response = server
            .post("/api/v1/tokens")
            .add_header(axum::http::header::AUTHORIZATION, format!("Basic {basic}"))
            .json(&serde_json::json!({"name": "   abc   "}))
            .await;
        assert_eq!(response.status_code(), StatusCode::CREATED);
        assert_eq!(response.json::<serde_json::Value>()["name"], "abc");
    }
}
