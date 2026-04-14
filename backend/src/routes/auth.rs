use axum::extract::State;
use axum::response::{IntoResponse, Redirect};
use axum::routing::{get, post};
use axum::{Json, Router};
use openidconnect::core::CoreResponseType;
use openidconnect::{
    AuthenticationFlow, AuthorizationCode, CsrfToken, Nonce, PkceCodeChallenge, PkceCodeVerifier,
    Scope, TokenResponse,
};

use crate::auth::backend::OidcCredentials;
use crate::auth::middleware::{AuthCtx, CurrentUser};
use crate::auth::oidc;
use crate::error::AppError;
use crate::models::user;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/auth/login", get(login))
        .route("/auth/callback", get(callback))
        .route("/auth/logout", post(logout))
        .route("/auth/me", get(me))
}

#[derive(serde::Deserialize)]
pub struct CallbackParams {
    code: String,
    state: String,
}

async fn login(
    State(state): State<AppState>,
    auth_session: AuthCtx,
) -> Result<impl IntoResponse, AppError> {
    let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();

    let (auth_url, csrf_token, nonce) = state
        .oidc_client
        .authorize_url(
            AuthenticationFlow::<CoreResponseType>::AuthorizationCode,
            CsrfToken::new_random,
            Nonce::new_random,
        )
        .add_scope(Scope::new("email".to_string()))
        .add_scope(Scope::new("profile".to_string()))
        .set_pkce_challenge(pkce_challenge)
        .url();

    // Store OIDC flow state in the underlying session
    let session = &auth_session.session;
    session
        .insert("pkce_verifier", pkce_verifier.secret().clone())
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    session
        .insert("csrf_token", csrf_token.secret().clone())
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    session
        .insert("nonce", nonce.secret().clone())
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    Ok(Redirect::temporary(auth_url.as_str()))
}

async fn callback(
    State(state): State<AppState>,
    mut auth_session: AuthCtx,
    axum::extract::Query(params): axum::extract::Query<CallbackParams>,
) -> Result<impl IntoResponse, AppError> {
    let session = &auth_session.session;

    // Validate CSRF token
    let stored_csrf: String = session
        .get("csrf_token")
        .await
        .map_err(|e| AppError::Internal(e.into()))?
        .ok_or(AppError::Unauthorized)?;
    if stored_csrf != params.state {
        return Err(AppError::Unauthorized);
    }

    // Retrieve stored PKCE verifier and nonce
    let stored_verifier: String = session
        .get("pkce_verifier")
        .await
        .map_err(|e| AppError::Internal(e.into()))?
        .ok_or(AppError::Unauthorized)?;
    let stored_nonce: String = session
        .get("nonce")
        .await
        .map_err(|e| AppError::Internal(e.into()))?
        .ok_or(AppError::Unauthorized)?;

    // Exchange code for tokens
    let http_client = oidc::exchange_http_client().map_err(AppError::Internal)?;
    let token_response = state
        .oidc_client
        .exchange_code(AuthorizationCode::new(params.code))
        .map_err(|e| AppError::Internal(anyhow::anyhow!("exchange_code config error: {e}")))?
        .set_pkce_verifier(PkceCodeVerifier::new(stored_verifier))
        .request_async(&http_client)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("token exchange failed: {e}")))?;

    // Validate ID token and extract claims
    let id_token = token_response
        .id_token()
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("missing ID token")))?;
    let claims = id_token
        .claims(
            &state.oidc_client.id_token_verifier(),
            &Nonce::new(stored_nonce),
        )
        .map_err(|e| AppError::Internal(anyhow::anyhow!("ID token validation failed: {e}")))?;

    let subject = claims.subject().as_str();
    let display_name = claims
        .name()
        .and_then(|n: &openidconnect::LocalizedClaim<openidconnect::EndUserName>| n.get(None))
        .map(|n: &openidconnect::EndUserName| n.as_str())
        .unwrap_or(subject);
    let email = claims
        .email()
        .map(|e: &openidconnect::EndUserEmail| e.as_str());

    // Authenticate via axum-login backend (upserts user + first-user promotion)
    let user = auth_session
        .authenticate(OidcCredentials {
            subject: subject.to_owned(),
            display_name: display_name.to_owned(),
            email: email.map(|e| e.to_owned()),
        })
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("auth backend error: {e}")))?
        .ok_or(AppError::Unauthorized)?;

    // Log the user in — cycles session ID (fixation prevention) and stores auth hash
    auth_session
        .login(&user)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("login failed: {e}")))?;

    // Clean up single-use OIDC flow state from session
    let _ = auth_session.session.remove::<String>("pkce_verifier").await;
    let _ = auth_session.session.remove::<String>("csrf_token").await;
    let _ = auth_session.session.remove::<String>("nonce").await;

    Ok(Redirect::temporary("/"))
}

async fn logout(mut auth_session: AuthCtx) -> Result<impl IntoResponse, AppError> {
    auth_session
        .logout()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("logout failed: {e}")))?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

async fn me(
    current_user: CurrentUser,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, AppError> {
    let u = user::find_by_id(&state.pool, current_user.user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?
        .ok_or(AppError::Unauthorized)?;
    Ok(Json(serde_json::json!({
        "id": u.id,
        "display_name": u.display_name,
        "email": u.email,
        "role": u.role,
        "is_child": u.is_child,
    })))
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;

    use crate::test_support;

    #[tokio::test]
    async fn login_redirects_to_oidc_provider() {
        let server = test_support::test_server();
        let response = server.get("/auth/login").await;
        // Should redirect to the fake OIDC provider's auth URL
        assert_eq!(response.status_code(), StatusCode::TEMPORARY_REDIRECT);
        let location = response.header("location").to_str().unwrap().to_owned();
        assert!(
            location.starts_with("https://fake-issuer.example.com/auth"),
            "expected redirect to OIDC provider, got: {location}"
        );
        // Verify PKCE and required OAuth params are present
        assert!(
            location.contains("code_challenge="),
            "missing PKCE code_challenge"
        );
        assert!(
            location.contains("code_challenge_method=S256"),
            "missing PKCE method"
        );
        assert!(
            location.contains("response_type=code"),
            "missing response_type"
        );
        assert!(location.contains("scope="), "missing scope");
    }

    #[tokio::test]
    async fn me_returns_401_without_auth() {
        let server = test_support::test_server();
        let response = server.get("/auth/me").await;
        assert_eq!(response.status_code(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn logout_returns_204_without_session() {
        let server = test_support::test_server();
        let response = server.post("/auth/logout").await;
        // logout on a non-authenticated session still succeeds (no-op)
        assert_eq!(response.status_code(), StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn callback_returns_401_without_session_state() {
        let server = test_support::test_server();
        // Callback without a prior login flow (no CSRF/PKCE in session) should fail
        let response = server
            .get("/auth/callback")
            .add_query_param("code", "fake-code")
            .add_query_param("state", "fake-state")
            .await;
        assert_eq!(response.status_code(), StatusCode::UNAUTHORIZED);
    }
}
