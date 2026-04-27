use axum::extract::State;
use axum::response::{IntoResponse, Redirect};
use axum::routing::{get, patch, post};
use axum::{Json, Router};
use axum_extra::extract::cookie::CookieJar;
use openidconnect::core::CoreResponseType;
use openidconnect::{
    AuthenticationFlow, AuthorizationCode, CsrfToken, Nonce, PkceCodeChallenge, PkceCodeVerifier,
    Scope, TokenResponse,
};

use crate::auth::backend::OidcCredentials;
use crate::auth::middleware::{AuthCtx, CurrentUser};
use crate::auth::oidc;
use crate::auth::theme_cookie::set_theme_cookie;
use crate::models::theme_preference::ThemePreference;
use crate::error::AppError;
use crate::models::user;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/auth/login", get(login))
        .route("/auth/callback", get(callback))
        .route("/auth/logout", post(logout))
        .route("/auth/me", get(me))
        .route("/auth/me/theme", patch(update_theme))
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
    jar: CookieJar,
    axum::extract::Query(params): axum::extract::Query<CallbackParams>,
) -> Result<(CookieJar, Redirect), AppError> {
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

    // Clean up single-use OIDC flow state from session. A failure here
    // leaves residual OIDC material in the session store but must not abort
    // the login redirect — the user is already authenticated. Log instead.
    if let Err(e) = auth_session.session.remove::<String>("pkce_verifier").await {
        tracing::warn!(error = %e, "failed to remove pkce_verifier from session after OIDC callback");
    }
    if let Err(e) = auth_session.session.remove::<String>("csrf_token").await {
        tracing::warn!(error = %e, "failed to remove csrf_token from session after OIDC callback");
    }
    if let Err(e) = auth_session.session.remove::<String>("nonce").await {
        tracing::warn!(error = %e, "failed to remove nonce from session after OIDC callback");
    }

    // Seed reverie_theme cookie from the freshly-loaded user record so the
    // FOUC script reads the same value on next cold load.
    let jar = set_theme_cookie(
        jar,
        user.theme_preference,
        state.config.security.behind_https,
    );

    Ok((jar, Redirect::temporary("/")))
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
        "theme_preference": u.theme_preference,
    })))
}

#[derive(serde::Deserialize)]
struct UpdateThemeRequest {
    theme_preference: ThemePreference,
}

async fn update_theme(
    current_user: CurrentUser,
    State(state): State<AppState>,
    jar: CookieJar,
    Json(body): Json<UpdateThemeRequest>,
) -> Result<(CookieJar, Json<serde_json::Value>), AppError> {
    sqlx::query("UPDATE users SET theme_preference = $1, updated_at = now() WHERE id = $2")
        .bind(body.theme_preference)
        .bind(current_user.user_id)
        .execute(&state.pool)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    let jar = set_theme_cookie(
        jar,
        body.theme_preference,
        state.config.security.behind_https,
    );
    Ok((
        jar,
        Json(serde_json::json!({ "theme_preference": body.theme_preference })),
    ))
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;

    use crate::models::theme_preference::ThemePreference;
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
    async fn patch_theme_returns_401_without_auth() {
        let server = test_support::test_server();
        let response = server
            .patch("/auth/me/theme")
            .json(&serde_json::json!({"theme_preference": "dark"}))
            .await;
        assert_eq!(response.status_code(), StatusCode::UNAUTHORIZED);
        let theme_cookies: Vec<&str> = response
            .headers()
            .get_all("set-cookie")
            .iter()
            .filter_map(|v| v.to_str().ok())
            .filter(|c| c.starts_with("reverie_theme="))
            .collect();
        assert!(
            theme_cookies.is_empty(),
            "unauthenticated request must not emit a reverie_theme cookie; got: {theme_cookies:?}"
        );
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

    #[sqlx::test(migrations = "./migrations")]
    async fn me_returns_theme_preference_default(pool: sqlx::PgPool) {
        use axum::http::header::AUTHORIZATION;

        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_user_id, basic) =
            test_support::db::create_adult_and_basic_auth(&app_pool, "theme-me-default").await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

        let resp = server
            .get("/auth/me")
            .add_header(AUTHORIZATION, basic)
            .await;
        assert_eq!(resp.status_code(), StatusCode::OK);

        let body: serde_json::Value = resp.json();
        assert_eq!(
            body.get("theme_preference").and_then(|v| v.as_str()),
            Some("system"),
            "default theme_preference must be 'system' (matches migration default)"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn patch_theme_updates_user_row(pool: sqlx::PgPool) {
        use axum::http::header::AUTHORIZATION;

        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

        // Cover every allowed value: a typo or column-type bug that
        // accepted only a subset would otherwise pass undetected.
        for (label, wire, expected) in [
            ("light", "light", ThemePreference::Light),
            ("dark", "dark", ThemePreference::Dark),
            ("system", "system", ThemePreference::System),
        ] {
            let (user_id, basic) = test_support::db::create_adult_and_basic_auth(
                &app_pool,
                &format!("theme-patch-happy-{label}"),
            )
            .await;

            let resp = server
                .patch("/auth/me/theme")
                .add_header(AUTHORIZATION, basic)
                .json(&serde_json::json!({"theme_preference": wire}))
                .await;
            assert_eq!(
                resp.status_code(),
                StatusCode::OK,
                "expected 200 for theme_preference={wire}"
            );

            let set_cookie = resp
                .headers()
                .get("set-cookie")
                .unwrap_or_else(|| panic!("set-cookie header missing on PATCH success ({wire})"))
                .to_str()
                .expect("set-cookie header not ascii");
            assert!(
                set_cookie.starts_with(&format!("reverie_theme={wire}")),
                "expected reverie_theme={wire} prefix; got: {set_cookie}"
            );

            let stored: ThemePreference =
                sqlx::query_scalar("SELECT theme_preference FROM users WHERE id = $1")
                    .bind(user_id)
                    .fetch_one(&app_pool)
                    .await
                    .expect("read back theme_preference");
            assert_eq!(stored, expected, "theme_preference={wire}");
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn patch_theme_rejects_invalid_value(pool: sqlx::PgPool) {
        use axum::http::header::AUTHORIZATION;

        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (user_id, basic) =
            test_support::db::create_adult_and_basic_auth(&app_pool, "theme-patch-invalid").await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

        let resp = server
            .patch("/auth/me/theme")
            .add_header(AUTHORIZATION, basic)
            .json(&serde_json::json!({"theme_preference": "purple"}))
            .await;
        // AppError::Validation maps to 422 (NOT 400) — see backend/src/error.rs.
        assert_eq!(resp.status_code(), StatusCode::UNPROCESSABLE_ENTITY);

        let stored: ThemePreference =
            sqlx::query_scalar("SELECT theme_preference FROM users WHERE id = $1")
                .bind(user_id)
                .fetch_one(&app_pool)
                .await
                .expect("read back theme_preference");
        assert_eq!(
            stored,
            ThemePreference::System,
            "row must remain default after rejection"
        );
        // Filter to reverie_theme= specifically — session middleware may
        // emit its own Set-Cookie on authenticated routes, and that's
        // unrelated to the theme-rejection invariant we're testing.
        let theme_cookies: Vec<&str> = resp
            .headers()
            .get_all("set-cookie")
            .iter()
            .filter_map(|v| v.to_str().ok())
            .filter(|c| c.starts_with("reverie_theme="))
            .collect();
        assert!(
            theme_cookies.is_empty(),
            "rejected request must not emit a reverie_theme cookie; got: {theme_cookies:?}"
        );
    }
}
