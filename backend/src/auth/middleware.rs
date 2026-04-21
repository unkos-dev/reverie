use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum_login::AuthSession;
use uuid::Uuid;

use crate::auth::backend::AuthBackend;
use crate::error::AppError;
use crate::models::{device_token, user};
use crate::state::AppState;

pub type AuthCtx = AuthSession<AuthBackend>;

#[derive(Debug, Clone)]
pub struct CurrentUser {
    pub user_id: Uuid,
    pub role: String,
    pub is_child: bool,
}

impl CurrentUser {
    /// Return `Err(Forbidden)` unless the user is an admin.
    pub fn require_admin(&self) -> Result<(), AppError> {
        if self.role == "admin" {
            Ok(())
        } else {
            Err(AppError::Forbidden)
        }
    }

    /// Return `Err(Forbidden)` for child accounts. Adult and admin pass.
    /// Used to gate metadata/enrichment endpoints that should not be visible
    /// to children.
    #[allow(dead_code)] // wired up by Step 7 tasks 25/26 (metadata + enrichment routes)
    pub fn require_not_child(&self) -> Result<(), AppError> {
        if self.is_child {
            Err(AppError::Forbidden)
        } else {
            Ok(())
        }
    }
}

/// Verify a `Authorization: Basic <b64>` header against the device-token
/// registry. Shared by [`CurrentUser`] (cookie-or-Basic) and
/// [`crate::auth::basic_only::BasicOnly`] (Basic-only).
///
/// Returns `Ok(Some(user))` when Basic credentials validate, `Ok(None)` when
/// no `Authorization: Basic ...` is present, and `Err(Unauthorized)` when a
/// Basic header is present but credentials are malformed or don't match any
/// active token. Side-effect: schedules an async `update_last_used` write
/// (SQL-side debounced to at most one UPDATE per token per 5 minutes).
pub(crate) async fn verify_basic(
    state: &AppState,
    parts: &Parts,
) -> Result<Option<CurrentUser>, AppError> {
    let Some(auth) = parts.headers.get(axum::http::header::AUTHORIZATION) else {
        return Ok(None);
    };
    let Ok(auth_str) = auth.to_str() else {
        return Ok(None);
    };
    let Some(credentials) = auth_str.strip_prefix("Basic ") else {
        return Ok(None);
    };

    use base64ct::Encoding;
    let mut buf = vec![0u8; credentials.len()];
    let decoded = base64ct::Base64::decode(credentials.as_bytes(), &mut buf)
        .map_err(|_| AppError::Unauthorized)?;
    let decoded_str = std::str::from_utf8(decoded).map_err(|_| AppError::Unauthorized)?;
    let (username, password) = decoded_str.split_once(':').ok_or(AppError::Unauthorized)?;

    let user_id: Uuid = username.parse().map_err(|_| AppError::Unauthorized)?;
    let u = user::find_by_id(&state.pool, user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?
        .ok_or(AppError::Unauthorized)?;
    let tokens = device_token::list_for_user(&state.pool, user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    // Iterate all tokens to avoid timing side-channel that leaks token position.
    let mut matched_token_id = None;
    for token in &tokens {
        if crate::auth::token::verify_device_token(password, &token.token_hash) {
            matched_token_id = Some(token.id);
        }
    }

    let token_id = matched_token_id.ok_or(AppError::Unauthorized)?;
    let pool = state.pool.clone();
    tokio::spawn(async move {
        let _ = device_token::update_last_used(&pool, token_id).await;
    });

    Ok(Some(CurrentUser {
        user_id: u.id,
        role: u.role,
        is_child: u.is_child,
    }))
}

impl FromRequestParts<AppState> for CurrentUser {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        // Try session cookie via axum-login (populated by AuthManagerLayer)
        if let Ok(auth_session) =
            <AuthCtx as FromRequestParts<AppState>>::from_request_parts(parts, state).await
            && let Some(u) = auth_session.user
        {
            return Ok(CurrentUser {
                user_id: u.id,
                role: u.role,
                is_child: u.is_child,
            });
        }

        // Fall back to Basic auth
        if let Some(user) = verify_basic(state, parts).await? {
            return Ok(user);
        }

        Err(AppError::Unauthorized)
    }
}
