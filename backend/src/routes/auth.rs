//! Authentication routes — OIDC login / callback, session logout, and the
//! cookie-authenticated `/auth/me` profile + theme-preference endpoints.

use axum::extract::State;
use axum::response::{IntoResponse, Redirect};
use axum::routing::{get, patch, post};
use axum::{Json, Router};
use axum_extra::extract::cookie::CookieJar;
use base64ct::{Base64UrlUnpadded, Encoding};
use openidconnect::core::CoreResponseType;
use openidconnect::{
    AuthenticationFlow, AuthorizationCode, CsrfToken, Nonce, PkceCodeChallenge, PkceCodeVerifier,
    Scope, TokenResponse,
};

use crate::auth::backend::OidcCredentials;
use crate::auth::middleware::{AuthCtx, CurrentUser};
use crate::auth::oidc;
use crate::auth::theme_cookie::set_theme_cookie;
use crate::error::AppError;
use crate::models::theme_preference::ThemePreference;
use crate::models::user;
use crate::state::AppState;

/// Build the `/auth/*` router (login / callback / logout / me / theme).
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/auth/login", get(login))
        .route("/auth/callback", get(callback))
        .route("/auth/logout", post(logout))
        .route("/auth/me", get(me))
        .route("/auth/me/theme", patch(update_theme))
}

/// `/auth/callback` query-string parameters returned by the OIDC issuer
/// after the user authenticates.
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

    // Store OIDC flow state in the underlying session. The OIDC
    // transient anti-forgery state lives under `oidc_csrf_state` — a
    // dedicated key so it can never shadow or be confused with the
    // long-lived app-level `csrf_token` (synchronizer-token Phase 1)
    // that `/auth/callback` writes after a successful login. See
    // adr/2026-05-22-json-api-conventions.md §"CSRF defense".
    let session = &auth_session.session;
    session
        .insert("pkce_verifier", pkce_verifier.secret().clone())
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    session
        .insert("oidc_csrf_state", csrf_token.secret().clone())
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

    // Validate OIDC anti-forgery state (the `state` query param echoed
    // back by the IdP must match the value `/auth/login` stored under
    // `oidc_csrf_state`). This is the OIDC transient — distinct from
    // the long-lived `csrf_token` that the synchronizer-token defense
    // writes after `auth_session.login()` below.
    let stored_csrf: String = session
        .get("oidc_csrf_state")
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
        .map_or(subject, |n: &openidconnect::EndUserName| n.as_str());
    let email = claims
        .email()
        .map(|e: &openidconnect::EndUserEmail| e.as_str());

    // Authenticate via axum-login backend (upserts user + first-user promotion)
    let user = auth_session
        .authenticate(OidcCredentials {
            subject: subject.to_owned(),
            display_name: display_name.to_owned(),
            email: email.map(std::borrow::ToOwned::to_owned),
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
    if let Err(e) = auth_session
        .session
        .remove::<String>("oidc_csrf_state")
        .await
    {
        tracing::warn!(error = %e, "failed to remove oidc_csrf_state from session after OIDC callback");
    }
    if let Err(e) = auth_session.session.remove::<String>("nonce").await {
        tracing::warn!(error = %e, "failed to remove nonce from session after OIDC callback");
    }

    // OWASP CSRF synchronizer token (Phase 1: token issuance only;
    // Phase 2 enables the validating middleware — see
    // adr/2026-05-22-json-api-conventions.md §"CSRF defense" and the
    // order-of-operations note). 32 bytes from the OS CSPRNG, encoded
    // as 43-char base64url-unpadded; mirrors `auth::token::generate_device_token`.
    //
    // THREAT: `SameSite=Lax` cookies alone don't cover top-level GET CSRF
    // returning sensitive state and don't protect when a cookie is set
    // with `SameSite=None`. The synchronizer token bound to this session
    // (read via `/auth/me`, sent back as `X-CSRF-Token` on mutating verbs)
    // is the primary defense; SameSite + CSP API layer are defense-in-depth.
    //
    // Stored under session key `csrf_token`. Disjoint from the OIDC
    // anti-forgery state (`oidc_csrf_state`, removed above) so a
    // logged-in user re-hitting `/auth/login` cannot overwrite this
    // value with a transient OIDC parameter and confuse a future
    // reader. Re-running `/auth/callback` deliberately rotates this
    // token (each login overwrites the prior session's value).
    // Failure here aborts the login because an unguarded session
    // would leave the browser unable to mutate state once the Phase
    // 2 middleware turns on.
    let mut csrf_bytes = [0u8; 32];
    rand::fill(&mut csrf_bytes);
    let csrf_token = Base64UrlUnpadded::encode_string(&csrf_bytes);
    auth_session
        .session
        .insert("csrf_token", &csrf_token)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    // Seed reverie_theme cookie from the freshly-loaded user record so the
    // FOUC script reads the same value on next cold load.
    let jar = set_theme_cookie(jar, user.theme_preference);

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
    auth_session: AuthCtx,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, AppError> {
    let u = user::find_by_id(&state.pool, current_user.user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?
        .ok_or(AppError::Unauthorized)?;
    // THREAT: surfaces the session-bound CSRF synchronizer token to the
    // first-party SPA (see adr/2026-05-22-json-api-conventions.md). The
    // session key `csrf_token` is disjoint from the OIDC transient
    // `oidc_csrf_state` used by `/auth/login`, so this value is always
    // the long-lived app token (or absent for sessions that never went
    // through `/auth/callback`, e.g. Basic-auth OPDS callers). Treat
    // the missing case as `null` rather than 500: the response shape
    // stays stable, and the Phase 2 middleware (not this handler) is
    // what refuses unsafe verbs without a token.
    let csrf_token: Option<String> = auth_session
        .session
        .get("csrf_token")
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    Ok(Json(serde_json::json!({
        "id": u.id,
        "display_name": u.display_name,
        "email": u.email,
        "role": u.role,
        "is_child": u.is_child,
        "theme_preference": u.theme_preference,
        "csrf_token": csrf_token,
    })))
}

#[derive(serde::Deserialize)]
struct UpdateThemeRequest {
    theme_preference: ThemePreference,
}

// 422 contract: invalid `theme_preference` values are rejected by axum 0.8's
// default `Json` extractor (`JsonRejection::JsonDataError` → 422), so serde
// is the wire-boundary validation gate. If a future axum upgrade changes
// the default rejection status, the `patch_theme_rejects_invalid_value`
// test in this module will fail and surface the regression.
async fn update_theme(
    current_user: CurrentUser,
    State(state): State<AppState>,
    jar: CookieJar,
    Json(body): Json<UpdateThemeRequest>,
) -> Result<(CookieJar, Json<serde_json::Value>), AppError> {
    sqlx::query!(
        "UPDATE users SET theme_preference = $1, updated_at = now() WHERE id = $2",
        body.theme_preference as ThemePreference,
        current_user.user_id,
    )
    .execute(&state.pool)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    let jar = set_theme_cookie(jar, body.theme_preference);
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
        // Basic-auth sessions skip `/auth/callback`, so the CSRF
        // synchronizer token is never seeded. The field must still be
        // present (shape stability) but null. Phase 2's `csrf_required`
        // middleware exempts Basic-auth callers (OPDS clients) from
        // mutating-verb gating; this assertion locks that contract so
        // a future "always populate csrf_token" refactor cannot
        // accidentally start gating Basic-auth without a deliberate
        // policy change.
        assert!(
            body.get("csrf_token")
                .is_some_and(serde_json::Value::is_null),
            "Basic-auth /auth/me must include csrf_token: null; got: {body}"
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

            let stored = sqlx::query_scalar!(
                "SELECT theme_preference AS \"theme_preference: ThemePreference\" \
                 FROM users WHERE id = $1",
                user_id,
            )
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

        let stored = sqlx::query_scalar!(
            "SELECT theme_preference AS \"theme_preference: ThemePreference\" \
             FROM users WHERE id = $1",
            user_id,
        )
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

    /// End-to-end happy path through `/auth/login` → `/auth/callback`. Exercises:
    /// PKCE/CSRF/nonce session round-trip, mock token exchange against a
    /// signed ID token whose nonce matches what `/auth/login` stored,
    /// upsert-and-promote-to-admin (first user), session login (cookie cycled),
    /// and the FOUC theme cookie seeded from the freshly-loaded user record.
    #[sqlx::test(migrations = "./migrations")]
    async fn callback_succeeds_first_user_promoted_to_admin(pool: sqlx::PgPool) {
        use crate::auth::backend::AuthBackend;
        use crate::models::role::Role;
        use crate::state::AppState;
        use crate::test_support::oidc_mock::MockOidcProvider;
        use tower_sessions::session::Id as SessionId;
        use tower_sessions::{MemoryStore, SessionStore};

        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;

        // Mock IdP: spins up wiremock + signs a key + serves /jwks.
        // client_id matches `test_config().oidc_client_id` (empty string).
        let mock = MockOidcProvider::start("").await;
        let oidc_client = mock.client("http://localhost:3000/auth/callback");

        // Shared session store so the test can read what /auth/login wrote.
        let store = MemoryStore::default();
        let state = AppState {
            pool: app_pool.clone(),
            ingestion_pool,
            config: test_support::test_config(),
            oidc_client,
            settings: test_support::test_settings(),
            last_settings_reload: std::sync::Arc::new(tokio::sync::RwLock::new(None)),
        };
        let auth_backend = AuthBackend {
            pool: app_pool.clone(),
        };
        let app = crate::build_router_with_session_store(state, auth_backend, store.clone());
        let mut server = axum_test::TestServer::new(app);
        server.save_cookies();

        // Step 1: drive /auth/login. Server stores csrf_token, nonce,
        // pkce_verifier in the session and 307-redirects to the IdP.
        let login_resp = server.get("/auth/login").await;
        assert_eq!(login_resp.status_code(), StatusCode::TEMPORARY_REDIRECT);

        // Step 2: extract the session ID from the issued cookie and load
        // the stored OIDC flow state out of the shared MemoryStore.
        let session_cookie_value = login_resp.cookie("id").value().to_string();
        let session_id: SessionId = session_cookie_value.parse().expect("parse session id");
        let record = store
            .load(&session_id)
            .await
            .expect("load session record")
            .expect("session record present");
        let csrf: String = serde_json::from_value(
            record
                .data
                .get("oidc_csrf_state")
                .expect("oidc_csrf_state in session")
                .clone(),
        )
        .expect("oidc_csrf_state is string");
        let nonce: String =
            serde_json::from_value(record.data.get("nonce").expect("nonce in session").clone())
                .expect("nonce is string");

        // Step 3: install /token responder that returns an ID token
        // signed with the mock's key and bearing the matching nonce.
        mock.mount_token_endpoint(
            "test-subject-123",
            Some("alice@example.com"),
            Some("Alice Test"),
            &nonce,
        )
        .await;

        // Step 4: drive /auth/callback. Cookie jar carries the session id
        // from the login response; CSRF state is the value /auth/login stored.
        let cb_resp = server
            .get("/auth/callback")
            .add_query_param("code", "mock-auth-code")
            .add_query_param("state", &csrf)
            .await;
        assert_eq!(
            cb_resp.status_code(),
            StatusCode::TEMPORARY_REDIRECT,
            "expected 307 to /, got body: {}",
            cb_resp.text()
        );
        assert_eq!(
            cb_resp
                .header("location")
                .to_str()
                .expect("location is ascii"),
            "/",
        );

        // Session-fixation defence: axum-login's login() must rotate the
        // session id, so the cookie value after callback differs from the
        // one issued by /auth/login. A regression where login() stops
        // cycling would let a pre-auth attacker plant a session id that
        // becomes authenticated post-login.
        let new_session_value = cb_resp.cookie("id").value().to_string();
        assert_ne!(
            new_session_value, session_cookie_value,
            "session id must rotate across login() to prevent fixation"
        );

        // Step 5: callback emits a `reverie_theme` cookie reflecting the
        // upserted user's default ThemePreference::System.
        let theme_cookie = cb_resp
            .headers()
            .get_all("set-cookie")
            .iter()
            .filter_map(|v| v.to_str().ok())
            .find(|c| c.starts_with("reverie_theme="))
            .expect("reverie_theme cookie present after callback");
        assert!(
            theme_cookie.starts_with("reverie_theme=system"),
            "expected reverie_theme=system, got: {theme_cookie}"
        );

        // Step 6: user row exists and was promoted to admin (first user).
        let row = sqlx::query!(
            "SELECT role AS \"role: Role\", email \
             FROM users WHERE oidc_subject = $1",
            "test-subject-123",
        )
        .fetch_one(&app_pool)
        .await
        .expect("user row inserted by callback");
        assert_eq!(
            row.role,
            Role::Admin,
            "first user must be promoted to admin"
        );
        assert_eq!(row.email.as_deref(), Some("alice@example.com"));

        // Step 7: the cycled session cookie authenticates /auth/me.
        // axum-login's login() rotates the session id; axum-test's
        // save_cookies() picks up the new id from the callback response.
        let me_resp = server.get("/auth/me").await;
        assert_eq!(me_resp.status_code(), StatusCode::OK);
        let me_body: serde_json::Value = me_resp.json();
        assert_eq!(
            me_body.get("role").and_then(|v| v.as_str()),
            Some("admin"),
            "expected /auth/me role=admin, got body: {me_body}"
        );
        assert_eq!(
            me_body.get("email").and_then(|v| v.as_str()),
            Some("alice@example.com"),
        );

        // Step 8: /auth/me carries a session-stored CSRF synchronizer
        // token (43-char base64url-unpadded ≙ 32 random bytes). Phase 2
        // wires the middleware that validates `X-CSRF-Token`; Phase 1
        // ships token issuance + exposure here so the frontend can
        // start reading it before the middleware turns on. See
        // adr/2026-05-22-json-api-conventions.md §"CSRF defense".
        let token = me_body
            .get("csrf_token")
            .and_then(|v| v.as_str())
            .unwrap_or_else(|| panic!("csrf_token missing on /auth/me; body: {me_body}"));
        assert_eq!(
            token.len(),
            43,
            "csrf_token must be 43-char base64url-unpadded; got {} chars: {token}",
            token.len()
        );
        assert!(
            token
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
            "csrf_token must be base64url charset; got: {token}"
        );

        // Step 9: token is stable across reads within a session. Phase
        // 2 will rotate on role change; Phase 1 must NOT rotate per
        // request — otherwise the frontend's cached token races every
        // mutating request.
        let me_resp_2 = server.get("/auth/me").await;
        assert_eq!(me_resp_2.status_code(), StatusCode::OK);
        let me_body_2: serde_json::Value = me_resp_2.json();
        assert_eq!(
            me_body_2.get("csrf_token").and_then(|v| v.as_str()),
            Some(token),
            "csrf_token must be stable across /auth/me reads in same session"
        );
    }

    /// A logged-in user re-hitting `/auth/login` (e.g. mistaken click,
    /// stale tab) must NOT overwrite their long-lived synchronizer
    /// token. The OIDC transient state lives under `oidc_csrf_state`
    /// and the app token under `csrf_token` — disjoint keys. This
    /// test pins that disjointness so a future refactor that collapses
    /// the two keys breaks here rather than silently shipping a
    /// confused-deputy where `/auth/me` returns the OIDC transient
    /// value pretending to be the app token.
    ///
    /// Locks Pass-1 finding D1 from the PR #306 adversarial review.
    #[sqlx::test(migrations = "./migrations")]
    async fn re_login_preserves_app_csrf_token(pool: sqlx::PgPool) {
        use crate::auth::backend::AuthBackend;
        use crate::state::AppState;
        use crate::test_support::oidc_mock::MockOidcProvider;
        use tower_sessions::session::Id as SessionId;
        use tower_sessions::{MemoryStore, SessionStore};

        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;

        let mock = MockOidcProvider::start("").await;
        let oidc_client = mock.client("http://localhost:3000/auth/callback");
        let store = MemoryStore::default();
        let state = AppState {
            pool: app_pool.clone(),
            ingestion_pool,
            config: test_support::test_config(),
            oidc_client,
            settings: test_support::test_settings(),
            last_settings_reload: std::sync::Arc::new(tokio::sync::RwLock::new(None)),
        };
        let auth_backend = AuthBackend {
            pool: app_pool.clone(),
        };
        let app = crate::build_router_with_session_store(state, auth_backend, store.clone());
        let mut server = axum_test::TestServer::new(app);
        server.save_cookies();

        // Step 1: drive /login → /callback to mint the app csrf_token.
        let login_resp = server.get("/auth/login").await;
        assert_eq!(login_resp.status_code(), StatusCode::TEMPORARY_REDIRECT);
        let session_cookie_value = login_resp.cookie("id").value().to_string();
        let session_id: SessionId = session_cookie_value.parse().expect("parse session id");
        let record = store
            .load(&session_id)
            .await
            .expect("load session record")
            .expect("session record present");
        let oidc_state_1: String = serde_json::from_value(
            record
                .data
                .get("oidc_csrf_state")
                .expect("oidc_csrf_state present after /auth/login")
                .clone(),
        )
        .expect("oidc_csrf_state is string");
        assert!(
            !record.data.contains_key("csrf_token"),
            "/auth/login must NOT write the app-level csrf_token key — that \
             would clobber a returning user's existing app token",
        );
        let nonce: String =
            serde_json::from_value(record.data.get("nonce").expect("nonce in session").clone())
                .expect("nonce is string");
        mock.mount_token_endpoint(
            "test-subject-relogin",
            Some("alice@example.com"),
            Some("Alice Test"),
            &nonce,
        )
        .await;
        let cb_resp = server
            .get("/auth/callback")
            .add_query_param("code", "mock-auth-code")
            .add_query_param("state", &oidc_state_1)
            .await;
        assert_eq!(
            cb_resp.status_code(),
            StatusCode::TEMPORARY_REDIRECT,
            "first callback failed: {}",
            cb_resp.text()
        );
        let token_a = server
            .get("/auth/me")
            .await
            .json::<serde_json::Value>()
            .get("csrf_token")
            .and_then(|v| v.as_str())
            .expect("csrf_token after first callback")
            .to_owned();
        assert_eq!(token_a.len(), 43, "token A must be 43-char base64url");

        // Step 2: same authenticated cookie jar — drive /auth/login
        // AGAIN (no callback). This writes a fresh OIDC transient
        // under `oidc_csrf_state`. The app token under `csrf_token`
        // MUST be preserved; otherwise the Phase-2 frontend reader
        // would see /auth/me return the OIDC transient pretending
        // to be the app token.
        let login_resp_2 = server.get("/auth/login").await;
        assert_eq!(login_resp_2.status_code(), StatusCode::TEMPORARY_REDIRECT);
        let session_cookie_2 = login_resp_2.cookie("id").value().to_string();
        let session_id_2: SessionId = session_cookie_2.parse().expect("parse session id 2");
        let record_2 = store
            .load(&session_id_2)
            .await
            .expect("load session record after second /auth/login")
            .expect("session record present after second /auth/login");

        // The new OIDC transient must be present and must differ from
        // the first login's transient (otherwise CSRF state is being
        // reused across login attempts — a separate security smell).
        let oidc_state_2: String = serde_json::from_value(
            record_2
                .data
                .get("oidc_csrf_state")
                .expect("oidc_csrf_state present after re-login")
                .clone(),
        )
        .expect("oidc_csrf_state 2 is string");
        assert_ne!(
            oidc_state_1, oidc_state_2,
            "OIDC anti-forgery state must rotate per /auth/login call"
        );

        // The app token under `csrf_token` must survive the re-login
        // intact. If this fires, /auth/login is shadowing the app
        // token with OIDC transient state (D1 regression).
        let preserved_app_token: String = serde_json::from_value(
            record_2
                .data
                .get("csrf_token")
                .expect("app csrf_token must survive /auth/login re-entry")
                .clone(),
        )
        .expect("preserved csrf_token is string");
        assert_eq!(
            preserved_app_token, token_a,
            "app csrf_token must equal the value minted by the previous \
             /auth/callback; mismatch indicates /auth/login wrote to the \
             app-token key and clobbered the long-lived value",
        );

        // /auth/me on the same cookie must still return token_a (not
        // the OIDC transient) — final user-facing contract lock.
        let me_after = server.get("/auth/me").await;
        assert_eq!(me_after.status_code(), StatusCode::OK);
        let me_after_body: serde_json::Value = me_after.json();
        assert_eq!(
            me_after_body.get("csrf_token").and_then(|v| v.as_str()),
            Some(token_a.as_str()),
            "/auth/me must keep returning the app csrf_token (not the \
             OIDC transient) after a re-login attempt without callback",
        );
    }
}
