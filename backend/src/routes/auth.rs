//! Authentication routes — OIDC login / callback, session logout, and the
//! cookie-authenticated `/auth/me` profile + theme-preference endpoints.

use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Redirect};
use axum_extra::extract::cookie::CookieJar;
use axum_extra::extract::{Query, QueryRejection};
use base64ct::{Base64UrlUnpadded, Encoding};
use openidconnect::core::CoreResponseType;
use openidconnect::{
    AuthenticationFlow, AuthorizationCode, CsrfToken, Nonce, PkceCodeChallenge, PkceCodeVerifier,
    Scope, TokenResponse,
};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use tower_sessions::Session;

use crate::auth::middleware::CurrentUser;
use crate::auth::oidc;
use crate::auth::theme_cookie::set_theme_cookie;
use crate::error::AppError;
use crate::models::theme_preference::ThemePreference;
use crate::models::user;
use crate::state::AppState;

/// Build the `/auth/*` router (login / callback / logout / me / theme) as
/// an [`OpenApiRouter`] so each handler's `#[utoipa::path]` contributes to
/// the generated spec (a missing annotation fails to compile). Merged into
/// `crate::openapi::pilot_router` and split into its runtime and spec
/// halves there.
pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(oidc_login))
        .routes(routes!(callback))
        .routes(routes!(local_login))
        .routes(routes!(setup_status))
        .routes(routes!(setup))
        .routes(routes!(forgot_password))
        .routes(routes!(reset_password))
        .routes(routes!(logout))
        .routes(routes!(me))
        .routes(routes!(update_theme))
}

/// `/auth/callback` query-string parameters returned by the OIDC issuer
/// after the user authenticates.
#[derive(serde::Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct CallbackParams {
    /// Authorization code minted by the `IdP`.
    code: String,
    /// OIDC anti-forgery state echoed back by the `IdP`; must match the
    /// value `/auth/oidc/login` stored in the session.
    state: String,
}

/// `GET /auth/oidc/login` — start the OIDC authorization-code + PKCE flow.
///
/// Renamed from `/auth/login` so the user-facing login page (a SPA route served
/// at `/login`) does not collide with this auth-protocol endpoint; `/auth/*` is
/// the auth API namespace. The OIDC callback path deliberately stays
/// `/auth/callback` (NOT `/auth/oidc/callback`) so operators need not reconfigure
/// `OIDC_REDIRECT_URI` at their `IdP`.
///
/// # Errors
/// - [`AppError::NotFound`] when OIDC is not configured.
/// - [`AppError::Internal`] when session storage fails.
#[utoipa::path(
    get,
    path = "/auth/oidc/login",
    tag = "auth",
    security(()),
    responses(
        (status = 307, description = "Redirect to the OIDC issuer's authorization endpoint; PKCE verifier, anti-forgery state, and nonce are stored in the (anonymous) session"),
        (status = 404, description = "OIDC is not configured on this instance", body = crate::openapi::ProblemDetails)
    )
)]
async fn oidc_login(
    State(state): State<AppState>,
    session: Session,
) -> Result<impl IntoResponse, AppError> {
    // OIDC is optional. When unconfigured this handler is
    // unreachable in practice (the SPA hides the OIDC action), but guard so a
    // direct hit 404s cleanly rather than acting on an absent client.
    let oidc_client = state.oidc_client.as_ref().ok_or(AppError::NotFound)?;

    let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();

    let (auth_url, csrf_token, nonce) = oidc_client
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
    // long-lived app-level `csrf_token` (the synchronizer token)
    // that `/auth/callback` writes after a successful login. See
    // adr/2026-05-22-json-api-conventions.md §"CSRF defense".
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

/// `GET /auth/callback` — OIDC redirect target: validate state, exchange
/// the code, establish the session, and issue the CSRF synchronizer token.
///
/// # Errors
/// - [`AppError::Unauthorized`] when the anti-forgery state is missing or
///   mismatched, or the ID token fails validation.
/// - [`AppError::Internal`] on token-exchange or session-storage failure.
#[utoipa::path(
    get,
    path = "/auth/callback",
    tag = "auth",
    security(()),
    params(CallbackParams),
    responses(
        (status = 307, description = "Login complete; session established, theme cookie seeded, redirect to /"),
        (status = 400, description = "Malformed query parameter", body = crate::openapi::ProblemDetails),
        (status = 401, description = "Anti-forgery state missing/mismatched or ID-token validation failed", body = crate::openapi::ProblemDetails)
    )
)]
async fn callback(
    State(state): State<AppState>,
    session: Session,
    jar: CookieJar,
    params: Result<Query<CallbackParams>, QueryRejection>,
) -> Result<(CookieJar, Redirect), AppError> {
    let Query(params) = params?;
    // OIDC optional: a callback without a configured client 404s.
    let oidc_client = state.oidc_client.as_ref().ok_or(AppError::NotFound)?;
    // Validate OIDC anti-forgery state (the `state` query param echoed
    // back by the IdP must match the value `/auth/oidc/login` stored under
    // `oidc_csrf_state`). This is the OIDC transient — distinct from
    // the long-lived `csrf_token` that the synchronizer-token defense
    // writes after `auth::session::login()` below.
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
    let token_response = oidc_client
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
        .claims(&oidc_client.id_token_verifier(), &Nonce::new(stored_nonce))
        .map_err(|e| AppError::Internal(anyhow::anyhow!("ID token validation failed: {e}")))?;

    let subject = claims.subject().as_str();
    // Verified `iss` from the ID-token claims namespaces the subject. For a
    // single configured issuer this matches config; using the verified claim is
    // the spec-correct source and keeps the schema multi-issuer-correct.
    let issuer = claims.issuer().as_str();
    let display_name = claims
        .name()
        .and_then(|n: &openidconnect::LocalizedClaim<openidconnect::EndUserName>| n.get(None))
        .map_or(subject, |n: &openidconnect::EndUserName| n.as_str());
    let email = claims
        .email()
        .map(|e: &openidconnect::EndUserEmail| e.as_str());

    // Provision/resolve the user through user_identities keyed on
    // (issuer, subject). No auto-promotion: the first administrator is granted
    // only via bootstrap. The OIDC claims are signature-verified above.
    let user = user::upsert_from_oidc(&state.pool, issuer, subject, display_name, email)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("user upsert failed: {e}")))?;

    // Log the user in — cycles session ID (fixation prevention) and persists
    // user_id + session_version for per-request rehydration.
    crate::auth::session::login(&session, &user)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("login failed: {e}")))?;

    // Clean up single-use OIDC flow state from session. A failure here
    // leaves residual OIDC material in the session store but must not abort
    // the login redirect — the user is already authenticated. Log instead.
    if let Err(e) = session.remove::<String>("pkce_verifier").await {
        tracing::warn!(error = %e, "failed to remove pkce_verifier from session after OIDC callback");
    }
    if let Err(e) = session.remove::<String>("oidc_csrf_state").await {
        tracing::warn!(error = %e, "failed to remove oidc_csrf_state from session after OIDC callback");
    }
    if let Err(e) = session.remove::<String>("nonce").await {
        tracing::warn!(error = %e, "failed to remove nonce from session after OIDC callback");
    }

    // OWASP CSRF synchronizer token. This mints and exposes the token;
    // a separate validating middleware enforces it on mutating requests
    // (see adr/2026-05-22-json-api-conventions.md §"CSRF defense" and the
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
    // logged-in user re-hitting `/auth/oidc/login` cannot overwrite this
    // value with a transient OIDC parameter and confuse a future
    // reader. Re-running `/auth/callback` deliberately rotates this
    // token (each login overwrites the prior session's value).
    // Failure here aborts the login because an unguarded session
    // would leave the browser unable to mutate state once the Phase
    // 2 middleware turns on.
    let mut csrf_bytes = [0u8; 32];
    rand::fill(&mut csrf_bytes);
    let csrf_token = Base64UrlUnpadded::encode_string(&csrf_bytes);
    session
        .insert("csrf_token", &csrf_token)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    // Seed reverie_theme cookie from the freshly-loaded user record so the
    // FOUC script reads the same value on next cold load.
    let jar = set_theme_cookie(jar, user.theme_preference);

    Ok((jar, Redirect::temporary("/")))
}

/// Request body for `POST /auth/local/login`. Explicit allow-list (no
/// mass-assignment): only the two credential fields are accepted.
#[derive(serde::Deserialize, utoipa::ToSchema)]
struct LocalLoginRequest {
    /// Account email; matched case-insensitively against `users.email`.
    email: String,
    /// Plaintext password, verified against the stored Argon2id hash.
    password: String,
}

/// `POST /auth/local/login` — email + password sign-in, establishing the same
/// session contract as the OIDC callback.
///
/// THREAT (enumeration): unknown email and wrong password return the identical
/// generic 422, and both spend equivalent Argon2 work, so neither
/// the response nor the timing distinguishes a non-existent account.
///
/// # Errors
/// - [`AppError::NotFound`] when local authentication is disabled.
/// - [`AppError::RateLimited`] (429) when the per-source limit is exceeded, or a
///   failed attempt arrives during an active per-account backoff.
/// - [`AppError::Validation`] (422) on bad credentials (generic; no enumeration).
/// - [`AppError::Internal`] on session-store or database failure.
#[utoipa::path(
    post,
    path = "/auth/local/login",
    tag = "auth",
    security(()),
    request_body = LocalLoginRequest,
    responses(
        (status = 204, description = "Login succeeded; session established (id rotated, CSRF token minted, theme cookie seeded)"),
        (status = 404, description = "Local authentication is disabled on this instance", body = crate::openapi::ProblemDetails),
        (status = 422, description = "Invalid credentials (generic; identical for unknown email and wrong password)", body = crate::openapi::ProblemDetails),
        (status = 429, description = "Too many login attempts", body = crate::openapi::ProblemDetails)
    )
)]
async fn local_login(
    State(state): State<AppState>,
    session: Session,
    jar: CookieJar,
    headers: HeaderMap,
    peer: crate::auth::rate_limit::PeerAddr,
    Json(body): Json<LocalLoginRequest>,
) -> Result<(CookieJar, StatusCode), AppError> {
    if !state.config.local_auth_enabled {
        return Err(AppError::NotFound);
    }

    // Per-source hard block (governor). Tolerates a missing peer (the test
    // harness supplies none); the per-account backoff is the IP-independent
    // backstop.
    let peer = peer.0.map(|addr| addr.ip());
    if let Some(ip) = crate::auth::rate_limit::client_ip(
        &headers,
        peer,
        state.config.trusted_client_ip_header.as_deref(),
    ) && state.login_limiter.check_key(&ip).is_err()
    {
        return Err(AppError::RateLimited);
    }

    // Resolve the account, then verify. On an unknown email or an account with
    // no local credential, spend equivalent Argon2 work against a dummy hash so
    // login latency does not leak account existence.
    let account = user::find_by_email(&state.pool, &body.email)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    let credential = match &account {
        Some(u) => crate::models::local_credentials::find_by_user_id(&state.pool, u.id)
            .await
            .map_err(|e| AppError::Internal(e.into()))?,
        None => None,
    };
    let verified = if let Some(cred) = &credential {
        crate::auth::password::verify_password(body.password.as_bytes(), &cred.password_hash)
            .is_ok()
    } else {
        // Spend equivalent Argon2 work and discard the (always false) result so
        // an unknown account is timing-indistinguishable from a wrong password.
        crate::auth::password::verify_against_dummy(body.password.as_bytes());
        false
    };

    let account_exists = account.is_some();
    // Decision 6: a correct password succeeds even during an active backoff
    // (which it then clears). Verify-first means backoff never blocks a
    // legitimate login; it only rejects continued *wrong* attempts.
    let session_user = if verified { account } else { None };
    if let Some(user) = session_user {
        crate::models::login_throttle::reset(&state.pool, &body.email)
            .await
            .map_err(|e| AppError::Internal(e.into()))?;

        // Session contract: rotate id + persist identity, mint the
        // CSRF synchronizer token, seed the theme cookie. Identical to the OIDC
        // callback, returning 204 rather than a redirect.
        crate::auth::session::login(&session, &user)
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("login failed: {e}")))?;
        let mut csrf_bytes = [0u8; 32];
        rand::fill(&mut csrf_bytes);
        let csrf_token = Base64UrlUnpadded::encode_string(&csrf_bytes);
        session
            .insert("csrf_token", &csrf_token)
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
        let jar = set_theme_cookie(jar, user.theme_preference);
        return Ok((jar, StatusCode::NO_CONTENT));
    }

    // Failed attempt. Escalate the per-account backoff, but only for a real
    // account, so an unknown email cannot grow the throttle table unbounded (the
    // per-source limiter covers unknown-email spray). A wrong attempt during an
    // active backoff is rejected hard without further growing the counter.
    //
    // Keyed on account existence ONLY, deliberately not on whether a local
    // credential is set. Gating on credential presence would let an attacker
    // distinguish "has a local password" from "OIDC-only / no password" by
    // observing whether a backoff appears, reintroducing the enumeration oracle
    // the constant-work verify paths close. An OIDC-only account never completes
    // a local login, so the backoff is harmless and self-clears on a later
    // success.
    if account_exists {
        if crate::models::login_throttle::backoff_until(&state.pool, &body.email)
            .await
            .map_err(|e| AppError::Internal(e.into()))?
            .is_some()
        {
            return Err(AppError::RateLimited);
        }
        crate::models::login_throttle::record_failure(
            &state.pool,
            &body.email,
            state.config.login_throttle_base_secs,
            state.config.login_throttle_cap_secs,
        )
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    }
    Err(AppError::Validation("invalid email or password".to_owned()))
}

/// Public first-run status for the SPA: whether setup is required and which
/// providers are usable.
#[derive(serde::Serialize, utoipa::ToSchema)]
struct SetupStatusResponse {
    /// `true` when no administrator exists yet, so the SPA shows the setup form.
    setup_required: bool,
    /// Whether local email+password login is enabled.
    local_auth_enabled: bool,
    /// Whether OIDC is configured (computed from the issuer). Drives
    /// the SPA's provider-aware redirect and the "Sign in with OIDC" action.
    oidc_enabled: bool,
}

/// `GET /auth/setup/status` — public provider/bootstrap state for the SPA.
///
/// # Errors
/// - [`AppError::Internal`] on database failure.
#[utoipa::path(
    get,
    path = "/auth/setup/status",
    tag = "auth",
    security(()),
    responses((status = 200, description = "Setup and provider state", body = SetupStatusResponse))
)]
async fn setup_status(
    State(state): State<AppState>,
) -> Result<Json<SetupStatusResponse>, AppError> {
    let admin = user::admin_exists(&state.pool)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    Ok(Json(SetupStatusResponse {
        setup_required: !admin,
        local_auth_enabled: state.config.local_auth_enabled,
        oidc_enabled: state.config.oidc_configured(),
    }))
}

/// Request body for `POST /auth/setup`. Explicit allow-list (no mass-assignment).
#[derive(serde::Deserialize, utoipa::ToSchema)]
struct SetupRequest {
    /// First administrator's email (RFC 5322 addr-spec).
    email: String,
    /// Display name.
    display_name: String,
    /// Plaintext password; enforced against `password_min_length`, then hashed.
    password: String,
}

/// `POST /auth/setup` — first-run bootstrap: mint the first administrator.
///
/// THREAT: bootstrap is the ONLY first-admin path. The race
/// guarantee is the DB `instance_bootstrap` singleton insert inside
/// [`user::create_first_admin`], NOT this `admin_exists` pre-check (which is only
/// a cheap fast-reject; a `SELECT EXISTS ... INSERT` does not serialize under
/// READ COMMITTED). A second concurrent setup loses the marker-row race and maps
/// to 409. Setup does NOT auto-login (parity with recovery; the session contract
/// lives only at `/auth/local/login`).
///
/// # Errors
/// - [`AppError::SetupAlreadyComplete`] (409) when an administrator already
///   exists (fast-reject or the marker-row race).
/// - [`AppError::Validation`] (422) on a malformed email or too-short password.
/// - [`AppError::Internal`] on hashing or database failure.
#[utoipa::path(
    post,
    path = "/auth/setup",
    tag = "auth",
    security(()),
    request_body = SetupRequest,
    responses(
        (status = 201, description = "First administrator created (no auto-login)"),
        (status = 409, description = "An administrator already exists", body = crate::openapi::ProblemDetails),
        (status = 422, description = "Validation failed", body = crate::openapi::ProblemDetails)
    )
)]
async fn setup(
    State(state): State<AppState>,
    Json(body): Json<SetupRequest>,
) -> Result<StatusCode, AppError> {
    // Cheap fast-reject; the authoritative guard is the marker insert below.
    if user::admin_exists(&state.pool)
        .await
        .map_err(|e| AppError::Internal(e.into()))?
    {
        return Err(AppError::SetupAlreadyComplete);
    }
    if !user::is_addr_spec(&body.email) {
        return Err(AppError::Validation("invalid email address".to_owned()));
    }
    if body.password.chars().count() < state.config.password_min_length {
        return Err(AppError::Validation(format!(
            "password must be at least {} characters",
            state.config.password_min_length
        )));
    }
    let phc = crate::auth::password::hash_password(body.password.as_bytes())
        .map_err(|e| AppError::Internal(anyhow::anyhow!("password hash failed: {e}")))?;

    match user::create_first_admin(&state.pool, &body.email, &body.display_name, &phc).await {
        Ok(_admin) => Ok(StatusCode::CREATED),
        Err(user::BootstrapError::AlreadyBootstrapped) => Err(AppError::SetupAlreadyComplete),
        Err(user::BootstrapError::EmailTaken) => {
            Err(AppError::Validation("email already in use".to_owned()))
        }
        Err(user::BootstrapError::Db(e)) => Err(AppError::Internal(e.into())),
    }
}

/// Resolve the client IP and enforce the per-source login/recovery rate limit.
/// Tolerates a missing peer (test harness); shared by the login and recovery
/// handlers.
fn enforce_source_rate_limit(
    state: &AppState,
    headers: &HeaderMap,
    peer: &crate::auth::rate_limit::PeerAddr,
) -> Result<(), AppError> {
    let ip = peer.0.map(|addr| addr.ip());
    if let Some(ip) = crate::auth::rate_limit::client_ip(
        headers,
        ip,
        state.config.trusted_client_ip_header.as_deref(),
    ) && state.login_limiter.check_key(&ip).is_err()
    {
        return Err(AppError::RateLimited);
    }
    Ok(())
}

/// Request body for `POST /auth/forgot-password`.
#[derive(serde::Deserialize, utoipa::ToSchema)]
struct ForgotPasswordRequest {
    /// Account email to start recovery for.
    email: String,
}

/// `POST /auth/forgot-password` — start email-less PIN recovery.
///
/// THREAT (codeguard #2): always returns a generic 200 — the response never
/// reveals whether the email exists. When it does, a fresh CSPRNG PIN is
/// generated, prior active PINs for the user are superseded (at most one active
/// PIN), the Argon2id hash row is persisted FIRST, then the clear PIN is written
/// to a per-user operator-readable file (mode 0600). A failed file write leaves an
/// unconsumed-but-unusable row that simply expires; it is never a cleartext PIN
/// with no consuming row. On an unknown email, equivalent Argon2 work is spent so
/// timing does not leak existence (a small DB/file residual is
/// accepted).
///
/// # Errors
/// - [`AppError::NotFound`] when local authentication is disabled.
/// - [`AppError::RateLimited`] (429) when the per-source limit is exceeded.
/// - [`AppError::Internal`] on hashing or database failure.
#[utoipa::path(
    post,
    path = "/auth/forgot-password",
    tag = "auth",
    security(()),
    request_body = ForgotPasswordRequest,
    responses(
        (status = 200, description = "Recovery started if the account exists (generic; no enumeration)"),
        (status = 404, description = "Local authentication is disabled", body = crate::openapi::ProblemDetails),
        (status = 429, description = "Too many requests", body = crate::openapi::ProblemDetails)
    )
)]
async fn forgot_password(
    State(state): State<AppState>,
    headers: HeaderMap,
    peer: crate::auth::rate_limit::PeerAddr,
    Json(body): Json<ForgotPasswordRequest>,
) -> Result<StatusCode, AppError> {
    if !state.config.local_auth_enabled {
        return Err(AppError::NotFound);
    }
    enforce_source_rate_limit(&state, &headers, &peer)?;

    let user = user::find_by_email(&state.pool, &body.email)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    if let Some(user) = user {
        crate::models::password_reset_pin::supersede_active(&state.pool, user.id)
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
        let pin = crate::auth::recovery::generate_pin();
        let pin_hash = crate::auth::password::hash_password(pin.as_bytes())
            .map_err(|e| AppError::Internal(anyhow::anyhow!("pin hash failed: {e}")))?;
        let expires_at = time::OffsetDateTime::now_utc()
            + time::Duration::seconds(state.config.recovery_pin_ttl_secs);
        // DB row first: a later file-write failure leaves only an unusable row.
        crate::models::password_reset_pin::insert(&state.pool, user.id, &pin_hash, expires_at)
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
        let email = user.email.as_deref().unwrap_or(&body.email);
        if let Err(e) = crate::auth::recovery::write_pin_file(
            std::path::Path::new(&state.config.recovery_pin_dir),
            user.id,
            email,
            &pin,
            expires_at,
        ) {
            // Never surface to the client; the row expires harmlessly.
            tracing::error!(error = %e, "failed to write recovery PIN file");
        }
    } else {
        // Unknown email: spend comparable Argon2 work so response timing does not
        // reveal account existence.
        crate::auth::password::verify_against_dummy(body.email.as_bytes());
    }

    Ok(StatusCode::OK)
}

/// Request body for `POST /auth/reset-password`.
#[derive(serde::Deserialize, utoipa::ToSchema)]
struct ResetPasswordRequest {
    /// Account email.
    email: String,
    /// The recovery PIN from the host file.
    pin: String,
    /// New plaintext password; enforced against `password_min_length`.
    new_password: String,
}

/// `POST /auth/reset-password` — complete recovery with a PIN.
///
/// THREAT (codeguard #2): the PIN is single-use (race-safe consume), short-lived,
/// and verified constant-time via Argon2. Any failure (unknown email, no active
/// PIN, wrong PIN, expired, too-short password) returns one generic 422, so the
/// response never distinguishes which part failed; the no-PIN path spends
/// equivalent work. Reset does NOT establish a session (no auto-login). Consuming
/// the PIN, writing the new credential, and bumping `session_version` (which
/// invalidates every session predating the reset) run in one transaction, so a
/// partial apply cannot leave the account locked out or a stale session live.
/// The PIN file is removed after the transaction commits.
///
/// # Errors
/// - [`AppError::NotFound`] when local authentication is disabled.
/// - [`AppError::RateLimited`] (429) when the per-source limit is exceeded.
/// - [`AppError::Validation`] (422) generic failure.
/// - [`AppError::Internal`] on hashing or database failure.
#[utoipa::path(
    post,
    path = "/auth/reset-password",
    tag = "auth",
    security(()),
    request_body = ResetPasswordRequest,
    responses(
        (status = 200, description = "Password reset; no session established (re-authentication required)"),
        (status = 404, description = "Local authentication is disabled", body = crate::openapi::ProblemDetails),
        (status = 422, description = "Invalid or expired reset request (generic)", body = crate::openapi::ProblemDetails),
        (status = 429, description = "Too many requests", body = crate::openapi::ProblemDetails)
    )
)]
async fn reset_password(
    State(state): State<AppState>,
    headers: HeaderMap,
    peer: crate::auth::rate_limit::PeerAddr,
    Json(body): Json<ResetPasswordRequest>,
) -> Result<StatusCode, AppError> {
    if !state.config.local_auth_enabled {
        return Err(AppError::NotFound);
    }
    enforce_source_rate_limit(&state, &headers, &peer)?;

    let generic = || AppError::Validation("invalid or expired reset request".to_owned());

    if body.new_password.chars().count() < state.config.password_min_length {
        return Err(generic());
    }

    let user = user::find_by_email(&state.pool, &body.email)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    let active = match &user {
        Some(u) => crate::models::password_reset_pin::find_active_by_user(&state.pool, u.id)
            .await
            .map_err(|e| AppError::Internal(e.into()))?,
        None => None,
    };
    let verified = if let Some(pin) = &active {
        crate::auth::password::verify_password(body.pin.as_bytes(), &pin.pin_hash).is_ok()
    } else {
        // Spend equivalent work on the no-PIN path (timing parity).
        crate::auth::password::verify_against_dummy(body.pin.as_bytes());
        false
    };

    if verified && let (Some(user), Some(pin)) = (&user, &active) {
        // Hash before opening the transaction: Argon2 is CPU-bound (~100 ms) and
        // must not be held across an open DB connection/locks.
        let phc = crate::auth::password::hash_password(body.new_password.as_bytes())
            .map_err(|e| AppError::Internal(anyhow::anyhow!("password hash failed: {e}")))?;

        // One transaction: consume the PIN, write the new credential, and
        // invalidate existing sessions atomically. A partial apply (PIN spent
        // but password unchanged) would lock the user out with no usable PIN.
        let mut tx = state
            .pool
            .begin()
            .await
            .map_err(|e| AppError::Internal(e.into()))?;

        // Single-use: consume first (race-safe). A losing concurrent reset gets
        // `false`; rolling back keeps that branch a no-op.
        let consumed = crate::models::password_reset_pin::consume(&mut *tx, pin.id)
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
        if consumed {
            crate::models::local_credentials::set_password(&mut *tx, user.id, &phc)
                .await
                .map_err(|e| AppError::Internal(e.into()))?;
            // THREAT (CWE-613): a password reset must terminate sessions that
            // predate it, or a session stolen before recovery survives the reset.
            crate::models::user::increment_session_version(&mut *tx, user.id)
                .await
                .map_err(|e| AppError::Internal(e.into()))?;
            tx.commit()
                .await
                .map_err(|e| AppError::Internal(e.into()))?;

            if let Err(e) = crate::auth::recovery::remove_pin_file(
                std::path::Path::new(&state.config.recovery_pin_dir),
                user.id,
            ) {
                tracing::warn!(error = %e, "failed to remove recovery PIN file after reset");
            }
            // No session established: the user must sign in with the new password.
            return Ok(StatusCode::OK);
        }
    }

    Err(generic())
}

/// `POST /auth/logout` — destroy the current session (idempotent).
///
/// # Errors
/// - [`AppError::Internal`] when session deletion fails at the store.
#[utoipa::path(
    post,
    path = "/auth/logout",
    tag = "auth",
    security(()),
    responses(
        (status = 204, description = "Session destroyed (no-op without one)")
    )
)]
async fn logout(session: Session) -> Result<impl IntoResponse, AppError> {
    crate::auth::session::logout(&session)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("logout failed: {e}")))?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

/// Profile payload for `GET /auth/me`.
#[derive(serde::Serialize, utoipa::ToSchema)]
struct MeResponse {
    /// User id.
    id: Uuid,
    /// Human-readable display name.
    display_name: String,
    /// Email address; `null` when none on file.
    email: Option<String>,
    /// Access-control role.
    role: crate::models::role::Role,
    /// Whether child content-visibility rules apply.
    is_child: bool,
    /// Persisted UI theme preference.
    theme_preference: ThemePreference,
    /// Session-bound CSRF synchronizer token to echo as `X-CSRF-Token` on
    /// unsafe verbs; `null` for sessions that never completed
    /// `/auth/callback` (e.g. Basic-auth OPDS callers).
    csrf_token: Option<String>,
}

/// `GET /auth/me` — the authenticated caller's profile + CSRF token.
///
/// # Errors
/// - [`AppError::Unauthorized`] when the session user no longer exists.
/// - [`AppError::Internal`] on database or session-store errors.
#[utoipa::path(
    get,
    path = "/auth/me",
    tag = "auth",
    responses(
        (status = 200, description = "Caller profile; `csrf_token` is the synchronizer token for unsafe verbs (null for Basic-auth sessions)", body = MeResponse),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails)
    )
)]
async fn me(
    current_user: CurrentUser,
    session: Session,
    State(state): State<AppState>,
) -> Result<Json<MeResponse>, AppError> {
    let u = user::find_by_id(&state.pool, current_user.user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?
        .ok_or(AppError::Unauthorized)?;
    // THREAT: surfaces the session-bound CSRF synchronizer token to the
    // first-party SPA (see adr/2026-05-22-json-api-conventions.md). The
    // session key `csrf_token` is disjoint from the OIDC transient
    // `oidc_csrf_state` used by `/auth/oidc/login`, so this value is always
    // the long-lived app token (or absent for sessions that never went
    // through `/auth/callback`, e.g. Basic-auth OPDS callers). Treat
    // the missing case as `null` rather than 500: the response shape
    // stays stable, and the validating middleware (not this handler) is
    // what refuses unsafe verbs without a token.
    let csrf_token: Option<String> = session
        .get("csrf_token")
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    Ok(Json(MeResponse {
        id: u.id,
        display_name: u.display_name,
        email: u.email,
        role: u.role,
        is_child: u.is_child,
        theme_preference: u.theme_preference,
        csrf_token,
    }))
}

/// Body for `PATCH /auth/me/theme`.
#[derive(serde::Deserialize, utoipa::ToSchema)]
struct UpdateThemeRequest {
    /// New theme preference; invalid values are rejected at
    /// deserialization (422).
    theme_preference: ThemePreference,
}

/// Echo payload for `PATCH /auth/me/theme`.
#[derive(serde::Serialize, utoipa::ToSchema)]
struct ThemeResponse {
    /// The persisted theme preference.
    theme_preference: ThemePreference,
}

// 422 contract: invalid `theme_preference` values are rejected by axum 0.8's
// default `Json` extractor (`JsonRejection::JsonDataError` → 422), so serde
// is the wire-boundary validation gate. If a future axum upgrade changes
// the default rejection status, the `patch_theme_rejects_invalid_value`
// test in this module will fail and surface the regression.
/// `PATCH /auth/me/theme` — persist the caller's theme preference and
/// refresh the FOUC theme cookie.
///
/// # Errors
/// - [`AppError::Internal`] on database errors.
#[utoipa::path(
    patch,
    path = "/auth/me/theme",
    tag = "auth",
    request_body = UpdateThemeRequest,
    responses(
        (status = 200, description = "Preference persisted; `reverie_theme` cookie refreshed", body = ThemeResponse),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails),
        (status = 422, description = "Unknown theme_preference value", body = crate::openapi::ProblemDetails)
    )
)]
async fn update_theme(
    current_user: CurrentUser,
    State(state): State<AppState>,
    jar: CookieJar,
    Json(body): Json<UpdateThemeRequest>,
) -> Result<(CookieJar, Json<ThemeResponse>), AppError> {
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
        Json(ThemeResponse {
            theme_preference: body.theme_preference,
        }),
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
        let response = server.get("/auth/oidc/login").await;
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
    async fn callback_malformed_query_returns_400_problem_json_no_secret_leak() {
        // A duplicate `?code=a&code=b` rejects at the axum_extra::Query
        // extractor before any session/OIDC work, so this returns RFC 9457
        // problem+json (not axum's plaintext 400) without a live DB (clears
        // debt 2026-06-10). THREAT (Hard Rule 6): the rejection `detail` must
        // name only the offending field, never the `code`/`state` value.
        let server = test_support::test_server();
        let response = server
            .get("/auth/callback?code=secret-a&code=secret-b")
            .await;

        let body = test_support::assert_problem(
            &response,
            crate::error::problems::MALFORMED_QUERY,
            StatusCode::BAD_REQUEST,
        );
        let detail = body["detail"].as_str().unwrap();
        assert!(
            !detail.contains("secret-a") && !detail.contains("secret-b"),
            "rejection detail must not echo the code value, got: {detail}"
        );
    }

    fn session_set_cookie(response: &axum_test::TestResponse) -> String {
        response
            .headers()
            .get_all("set-cookie")
            .iter()
            .filter_map(|v| v.to_str().ok())
            .find(|c| c.starts_with("id="))
            .expect("session `id` cookie emitted by /auth/oidc/login")
            .to_owned()
    }

    #[tokio::test]
    async fn session_cookie_carries_httponly_lax_and_no_secure_by_default() {
        let server = test_support::test_server();
        let cookie = session_set_cookie(&server.get("/auth/oidc/login").await);
        assert!(
            cookie.contains("HttpOnly"),
            "session cookie must be HttpOnly; got: {cookie}"
        );
        assert!(
            cookie.contains("SameSite=Lax"),
            "session cookie must be SameSite=Lax; got: {cookie}"
        );
        assert!(
            !cookie.contains("Secure"),
            "session cookie must not be Secure when behind_https=false; got: {cookie}"
        );
    }

    #[tokio::test]
    async fn session_cookie_secure_tracks_behind_https() {
        let mut state = test_support::test_state();
        state.config.security.behind_https = true;
        let app =
            crate::build_router_with_session_store(state, tower_sessions::MemoryStore::default());
        let server = axum_test::TestServer::new(app);
        let cookie = session_set_cookie(&server.get("/auth/oidc/login").await);
        assert!(
            cookie.contains("Secure"),
            "session cookie must be Secure when behind_https=true; got: {cookie}"
        );
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
        // present (shape stability) but null. The `csrf_required`
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

    /// End-to-end happy path through `/auth/oidc/login` → `/auth/callback`. Exercises:
    /// PKCE/CSRF/nonce session round-trip, mock token exchange against a
    /// signed ID token whose nonce matches what `/auth/oidc/login` stored,
    /// identity provisioning without auto-promotion (first user stays a
    /// non-administrator), session login (cookie cycled), and the FOUC theme
    /// cookie seeded from the freshly-loaded user record.
    #[sqlx::test(migrations = "./migrations")]
    async fn callback_succeeds_first_user_not_promoted(pool: sqlx::PgPool) {
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
        let oidc_client = Some(mock.client("http://localhost:3000/auth/callback"));

        // Shared session store so the test can read what /auth/oidc/login wrote.
        let store = MemoryStore::default();
        let state = AppState {
            pool: app_pool.clone(),
            ingestion_pool,
            config: test_support::test_config(),
            oidc_client,
            login_limiter: test_support::test_login_limiter(),
            settings: test_support::test_settings(),
            last_settings_reload: std::sync::Arc::new(tokio::sync::RwLock::new(None)),
        };
        let app = crate::build_router_with_session_store(state, store.clone());
        let mut server = axum_test::TestServer::new(app);
        server.save_cookies();

        // Step 1: drive /auth/oidc/login. Server stores csrf_token, nonce,
        // pkce_verifier in the session and 307-redirects to the IdP.
        let login_resp = server.get("/auth/oidc/login").await;
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
        // from the login response; CSRF state is the value /auth/oidc/login stored.
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

        // Session-fixation defence: our login() (auth::session::login →
        // cycle_id) must rotate the session id, so the cookie value after
        // callback differs from the one issued by /auth/oidc/login. A regression
        // where login() stops cycling would let a pre-auth attacker plant a
        // session id that becomes authenticated post-login.
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

        // Step 6: user row exists and is NOT auto-promoted (S1 retires
        // first-user promotion). Identity resolves through user_identities;
        // users.oidc_subject is left NULL, so match on the full identity key
        // (issuer, subject) that the schema's UNIQUE index uses.
        let row = sqlx::query!(
            "SELECT u.role AS \"role: Role\", u.email \
             FROM users u \
             JOIN user_identities ui ON ui.user_id = u.id \
             WHERE ui.issuer = $1 AND ui.subject = $2",
            mock.issuer(),
            "test-subject-123",
        )
        .fetch_one(&app_pool)
        .await
        .expect("user row inserted by callback");
        assert_eq!(
            row.role,
            Role::Adult,
            "first OIDC login must be a non-administrator"
        );
        assert_eq!(row.email.as_deref(), Some("alice@example.com"));

        // Step 7: the cycled session cookie authenticates /auth/me.
        // login() rotates the session id; axum-test's save_cookies()
        // picks up the new id from the callback response.
        let me_resp = server.get("/auth/me").await;
        assert_eq!(me_resp.status_code(), StatusCode::OK);
        let me_body: serde_json::Value = me_resp.json();
        assert_eq!(
            me_body.get("role").and_then(|v| v.as_str()),
            Some("adult"),
            "expected /auth/me role=adult, got body: {me_body}"
        );
        assert_eq!(
            me_body.get("email").and_then(|v| v.as_str()),
            Some("alice@example.com"),
        );

        // Step 8: /auth/me carries a session-stored CSRF synchronizer
        // token (43-char base64url-unpadded ≙ 32 random bytes). The
        // validating middleware checks `X-CSRF-Token`; token issuance +
        // exposure ship here so the frontend can start reading it before
        // the middleware turns on. See
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

        // Step 9: token is stable across reads within a session. A
        // future change may rotate it on role change, but issuance must
        // NOT rotate per request — otherwise the frontend's cached token
        // races every mutating request.
        let me_resp_2 = server.get("/auth/me").await;
        assert_eq!(me_resp_2.status_code(), StatusCode::OK);
        let me_body_2: serde_json::Value = me_resp_2.json();
        assert_eq!(
            me_body_2.get("csrf_token").and_then(|v| v.as_str()),
            Some(token),
            "csrf_token must be stable across /auth/me reads in same session"
        );
    }

    /// A logged-in user re-hitting `/auth/oidc/login` (e.g. mistaken click,
    /// stale tab) must NOT overwrite their long-lived synchronizer
    /// token. The OIDC transient state lives under `oidc_csrf_state`
    /// and the app token under `csrf_token` — disjoint keys. This
    /// test pins that disjointness so a future refactor that collapses
    /// the two keys breaks here rather than silently shipping a
    /// confused-deputy where `/auth/me` returns the OIDC transient
    /// value pretending to be the app token.
    #[sqlx::test(migrations = "./migrations")]
    async fn re_login_preserves_app_csrf_token(pool: sqlx::PgPool) {
        use crate::state::AppState;
        use crate::test_support::oidc_mock::MockOidcProvider;
        use tower_sessions::session::Id as SessionId;
        use tower_sessions::{MemoryStore, SessionStore};

        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;

        let mock = MockOidcProvider::start("").await;
        let oidc_client = Some(mock.client("http://localhost:3000/auth/callback"));
        let store = MemoryStore::default();
        let state = AppState {
            pool: app_pool.clone(),
            ingestion_pool,
            config: test_support::test_config(),
            oidc_client,
            login_limiter: test_support::test_login_limiter(),
            settings: test_support::test_settings(),
            last_settings_reload: std::sync::Arc::new(tokio::sync::RwLock::new(None)),
        };
        let app = crate::build_router_with_session_store(state, store.clone());
        let mut server = axum_test::TestServer::new(app);
        server.save_cookies();

        // Step 1: drive /login → /callback to mint the app csrf_token.
        let login_resp = server.get("/auth/oidc/login").await;
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
                .expect("oidc_csrf_state present after /auth/oidc/login")
                .clone(),
        )
        .expect("oidc_csrf_state is string");
        assert!(
            !record.data.contains_key("csrf_token"),
            "/auth/oidc/login must NOT write the app-level csrf_token key — that \
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

        // Step 2: same authenticated cookie jar — drive /auth/oidc/login
        // AGAIN (no callback). This writes a fresh OIDC transient
        // under `oidc_csrf_state`. The app token under `csrf_token`
        // MUST be preserved; otherwise the Phase-2 frontend reader
        // would see /auth/me return the OIDC transient pretending
        // to be the app token.
        let login_resp_2 = server.get("/auth/oidc/login").await;
        assert_eq!(login_resp_2.status_code(), StatusCode::TEMPORARY_REDIRECT);
        let session_cookie_2 = login_resp_2.cookie("id").value().to_string();
        let session_id_2: SessionId = session_cookie_2.parse().expect("parse session id 2");
        let record_2 = store
            .load(&session_id_2)
            .await
            .expect("load session record after second /auth/oidc/login")
            .expect("session record present after second /auth/oidc/login");

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
            "OIDC anti-forgery state must rotate per /auth/oidc/login call"
        );

        // The app token under `csrf_token` must survive the re-login
        // intact. If this fires, /auth/oidc/login is shadowing the app
        // token with OIDC transient state.
        let preserved_app_token: String = serde_json::from_value(
            record_2
                .data
                .get("csrf_token")
                .expect("app csrf_token must survive /auth/oidc/login re-entry")
                .clone(),
        )
        .expect("preserved csrf_token is string");
        assert_eq!(
            preserved_app_token, token_a,
            "app csrf_token must equal the value minted by the previous \
             /auth/callback; mismatch indicates /auth/oidc/login wrote to the \
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

    // Force-logout enforcement: bumping `users.session_version` must reject
    // the next request on an already-authenticated session. This moved out of
    // axum-login's `from_session` auth-hash check into the first-party
    // `CurrentUser` extractor (ADR 2026-06-04); previously only the DB-column
    // bump was tested (`routes/users/tests.rs`), never end-to-end enforcement.
    #[sqlx::test(migrations = "./migrations")]
    async fn session_version_bump_forces_logout(pool: sqlx::PgPool) {
        use crate::state::AppState;
        use crate::test_support::oidc_mock::MockOidcProvider;
        use tower_sessions::session::Id as SessionId;
        use tower_sessions::{MemoryStore, SessionStore};

        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;

        let mock = MockOidcProvider::start("").await;
        let oidc_client = Some(mock.client("http://localhost:3000/auth/callback"));
        let store = MemoryStore::default();
        let state = AppState {
            pool: app_pool.clone(),
            ingestion_pool,
            config: test_support::test_config(),
            oidc_client,
            login_limiter: test_support::test_login_limiter(),
            settings: test_support::test_settings(),
            last_settings_reload: std::sync::Arc::new(tokio::sync::RwLock::new(None)),
        };
        let app = crate::build_router_with_session_store(state, store.clone());
        let mut server = axum_test::TestServer::new(app);
        server.save_cookies();

        // Drive /auth/oidc/login → /auth/callback to establish an authenticated session.
        let login_resp = server.get("/auth/oidc/login").await;
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
        mock.mount_token_endpoint(
            "force-logout-subject",
            Some("fl@example.com"),
            Some("FL Test"),
            &nonce,
        )
        .await;
        let cb_resp = server
            .get("/auth/callback")
            .add_query_param("code", "mock-auth-code")
            .add_query_param("state", &csrf)
            .await;
        assert_eq!(
            cb_resp.status_code(),
            StatusCode::TEMPORARY_REDIRECT,
            "callback should succeed: {}",
            cb_resp.text()
        );
        // login() rotates the id via cycle_id, so the authenticated session
        // lives under a NEW id (the pre-login `session_id` row is already gone).
        // The flush assertion below must target this rotated id, not the stale one.
        let auth_session_id: SessionId = cb_resp
            .cookie("id")
            .value()
            .parse()
            .expect("parse rotated session id");

        // The session authenticates BEFORE the bump — guards against the test
        // passing for the wrong reason (e.g. session never established).
        let me_before = server.get("/auth/me").await;
        assert_eq!(
            me_before.status_code(),
            StatusCode::OK,
            "session must authenticate before session_version is bumped"
        );

        // Bump session_version in the DB (admin role change / security event).
        // Resolve the user via its full identity key (oidc_subject is now NULL).
        sqlx::query!(
            "UPDATE users SET session_version = session_version + 1 \
             WHERE id = (SELECT user_id FROM user_identities \
                         WHERE issuer = $1 AND subject = $2)",
            mock.issuer(),
            "force-logout-subject",
        )
        .execute(&app_pool)
        .await
        .expect("bump session_version");

        // The stored session_version is now stale → next request is rejected.
        let me_after = server.get("/auth/me").await;
        assert_eq!(
            me_after.status_code(),
            StatusCode::UNAUTHORIZED,
            "bumped session_version must force-logout the session on the next request"
        );

        // Force-logout must also flush the row server-side, not just 401 — a
        // lingering row would keep re-loading on every request until idle expiry.
        let flushed = store
            .load(&auth_session_id)
            .await
            .expect("load session after force-logout");
        assert!(
            flushed.is_none(),
            "force-logout must flush the stale session row server-side"
        );
    }

    // A session whose user row has been deleted must 401 on the next request
    // AND flush the orphaned session row — otherwise the row re-loads (one
    // `find_by_id` per request) until 24h idle expiry reaps it.
    #[sqlx::test(migrations = "./migrations")]
    async fn deleted_user_session_is_flushed_on_next_request(pool: sqlx::PgPool) {
        use crate::state::AppState;
        use crate::test_support::oidc_mock::MockOidcProvider;
        use tower_sessions::session::Id as SessionId;
        use tower_sessions::{MemoryStore, SessionStore};

        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;

        let mock = MockOidcProvider::start("").await;
        let oidc_client = Some(mock.client("http://localhost:3000/auth/callback"));
        let store = MemoryStore::default();
        let state = AppState {
            pool: app_pool.clone(),
            ingestion_pool,
            config: test_support::test_config(),
            oidc_client,
            login_limiter: test_support::test_login_limiter(),
            settings: test_support::test_settings(),
            last_settings_reload: std::sync::Arc::new(tokio::sync::RwLock::new(None)),
        };
        let app = crate::build_router_with_session_store(state, store.clone());
        let mut server = axum_test::TestServer::new(app);
        server.save_cookies();

        // Establish an authenticated session via /auth/oidc/login → /auth/callback.
        let login_resp = server.get("/auth/oidc/login").await;
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
        mock.mount_token_endpoint(
            "deleted-user-subject",
            Some("del@example.com"),
            Some("Del Test"),
            &nonce,
        )
        .await;
        let cb_resp = server
            .get("/auth/callback")
            .add_query_param("code", "mock-auth-code")
            .add_query_param("state", &csrf)
            .await;
        assert_eq!(
            cb_resp.status_code(),
            StatusCode::TEMPORARY_REDIRECT,
            "callback should succeed: {}",
            cb_resp.text()
        );
        // login() rotates the id; the authenticated session lives under the
        // post-cycle_id id, which is what the flush assertion must target.
        let auth_session_id: SessionId = cb_resp
            .cookie("id")
            .value()
            .parse()
            .expect("parse rotated session id");

        // Session authenticates before deletion — guards against a false pass.
        let me_before = server.get("/auth/me").await;
        assert_eq!(
            me_before.status_code(),
            StatusCode::OK,
            "session must authenticate before the user is deleted"
        );

        // Delete the user row out from under the live session. Resolve via the
        // identity subject (oidc_subject is now NULL); the identity link
        // cascades on the users delete.
        sqlx::query!(
            "DELETE FROM users \
             WHERE id = (SELECT user_id FROM user_identities \
                         WHERE issuer = $1 AND subject = $2)",
            mock.issuer(),
            "deleted-user-subject"
        )
        .execute(&app_pool)
        .await
        .expect("delete user");

        let me_after = server.get("/auth/me").await;
        assert_eq!(
            me_after.status_code(),
            StatusCode::UNAUTHORIZED,
            "a session for a deleted user must be rejected on the next request"
        );

        // The orphaned row must be flushed, not left to linger until expiry.
        let flushed = store
            .load(&auth_session_id)
            .await
            .expect("load session after user deletion");
        assert!(
            flushed.is_none(),
            "a deleted-user session row must be flushed server-side"
        );
    }

    // with_always_save(true): an authenticated read must slide the OnInactivity
    // expiry. Without it, OnInactivity only refreshes on a save (login), so an
    // active user would be logged out 24h after login regardless of activity.
    #[sqlx::test(migrations = "./migrations")]
    async fn authenticated_read_slides_inactivity_expiry(pool: sqlx::PgPool) {
        use crate::state::AppState;
        use crate::test_support::oidc_mock::MockOidcProvider;
        use tower_sessions::session::Id as SessionId;
        use tower_sessions::{MemoryStore, SessionStore};

        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;

        let mock = MockOidcProvider::start("").await;
        let oidc_client = Some(mock.client("http://localhost:3000/auth/callback"));
        let store = MemoryStore::default();
        let state = AppState {
            pool: app_pool.clone(),
            ingestion_pool,
            config: test_support::test_config(),
            oidc_client,
            login_limiter: test_support::test_login_limiter(),
            settings: test_support::test_settings(),
            last_settings_reload: std::sync::Arc::new(tokio::sync::RwLock::new(None)),
        };
        let app = crate::build_router_with_session_store(state, store.clone());
        let mut server = axum_test::TestServer::new(app);
        server.save_cookies();

        let login_resp = server.get("/auth/oidc/login").await;
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
        mock.mount_token_endpoint(
            "slide-subject",
            Some("slide@example.com"),
            Some("Slide"),
            &nonce,
        )
        .await;
        let cb_resp = server
            .get("/auth/callback")
            .add_query_param("code", "mock-auth-code")
            .add_query_param("state", &csrf)
            .await;
        assert_eq!(cb_resp.status_code(), StatusCode::TEMPORARY_REDIRECT);
        let auth_session_id: SessionId = cb_resp
            .cookie("id")
            .value()
            .parse()
            .expect("parse rotated session id");

        let expiry_before = store
            .load(&auth_session_id)
            .await
            .expect("load")
            .expect("record present")
            .expiry_date;

        // Sleep so the post-read save lands at a strictly later instant than the
        // login save; the assertion is `>` so any positive delta proves sliding.
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;

        let me = server.get("/auth/me").await;
        assert_eq!(me.status_code(), StatusCode::OK);

        let expiry_after = store
            .load(&auth_session_id)
            .await
            .expect("load")
            .expect("record present")
            .expiry_date;

        assert!(
            expiry_after > expiry_before,
            "an authenticated read must slide the inactivity expiry \
             (before={expiry_before}, after={expiry_after})"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn local_login_succeeds_and_establishes_session(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
        test_support::db::create_adult_with_password(
            &app_pool,
            "local-happy",
            "happy@example.com",
            "correct horse battery staple",
        )
        .await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

        let resp = server
            .post("/auth/local/login")
            .json(&serde_json::json!({
                "email": "happy@example.com",
                "password": "correct horse battery staple",
            }))
            .await;
        assert_eq!(
            resp.status_code(),
            StatusCode::NO_CONTENT,
            "correct credentials establish a session"
        );

        // Invariant 2: the local-login session carries through and a non-empty
        // CSRF synchronizer token was minted (same contract as the OIDC callback).
        let me = server.get("/auth/me").await;
        assert_eq!(me.status_code(), StatusCode::OK, "session carries through");
        let body: serde_json::Value = me.json();
        let csrf = body.get("csrf_token").and_then(|v| v.as_str());
        assert!(
            csrf.is_some_and(|t| !t.is_empty()),
            "local login mints a CSRF token; got: {body}"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn local_login_wrong_password_is_generic_422(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
        test_support::db::create_adult_with_password(
            &app_pool,
            "local-wrong",
            "wrong@example.com",
            "the right one",
        )
        .await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

        let resp = server
            .post("/auth/local/login")
            .json(&serde_json::json!({"email": "wrong@example.com", "password": "the WRONG one"}))
            .await;
        assert_eq!(resp.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn local_login_unknown_email_matches_wrong_password(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

        // No such account exists: the response must be the identical generic 422
        // a wrong password yields (no enumeration).
        let resp = server
            .post("/auth/local/login")
            .json(&serde_json::json!({"email": "ghost@example.com", "password": "anything"}))
            .await;
        assert_eq!(
            resp.status_code(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "an unknown email returns the same generic 422 as a wrong password"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn local_login_session_is_csrf_enforced(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
        test_support::db::create_adult_with_password(
            &app_pool,
            "csrf",
            "csrf@example.com",
            "a good password",
        )
        .await;
        let mut server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
        // Persist the session cookie set by login so the later mutations are
        // session-authenticated; without it the CSRF layer correctly exempts an
        // anonymous request and the assertions below would see 401, not 428.
        server.save_cookies();

        server
            .post("/auth/local/login")
            .json(&serde_json::json!({"email": "csrf@example.com", "password": "a good password"}))
            .await;

        // A session-authenticated mutation with no token is blocked (428).
        let no_token = server
            .post("/api/v1/shelves")
            .json(&serde_json::json!({"name": "Shelf"}))
            .await;
        assert_eq!(
            no_token.status_code(),
            StatusCode::PRECONDITION_REQUIRED,
            "CSRF layer blocks a session mutation with no X-CSRF-Token"
        );

        // A wrong token is rejected (403).
        let wrong = server
            .post("/api/v1/shelves")
            .add_header(
                axum::http::HeaderName::from_static("x-csrf-token"),
                axum::http::HeaderValue::from_static("not-the-token"),
            )
            .json(&serde_json::json!({"name": "Shelf"}))
            .await;
        assert_eq!(
            wrong.status_code(),
            StatusCode::FORBIDDEN,
            "a mismatched CSRF token is rejected"
        );

        // The minted token passes the CSRF layer: the request
        // reaches the handler rather than being blocked at 428/403.
        let me: serde_json::Value = server.get("/auth/me").await.json();
        let token = me
            .get("csrf_token")
            .and_then(|v| v.as_str())
            .expect("csrf token minted")
            .to_owned();
        let with_token = server
            .post("/api/v1/shelves")
            .add_header(
                axum::http::HeaderName::from_static("x-csrf-token"),
                axum::http::HeaderValue::from_str(&token).expect("valid header"),
            )
            .json(&serde_json::json!({"name": "Shelf"}))
            .await;
        assert_ne!(
            with_token.status_code(),
            StatusCode::PRECONDITION_REQUIRED,
            "a valid token must not be blocked by the CSRF layer"
        );
        assert_ne!(
            with_token.status_code(),
            StatusCode::FORBIDDEN,
            "a valid token must not be rejected by the CSRF layer"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn setup_creates_admin_then_rejects_second_attempt(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

        // Uninitialised instance: setup is required.
        let status: serde_json::Value = server.get("/auth/setup/status").await.json();
        assert_eq!(
            status
                .get("setup_required")
                .and_then(serde_json::Value::as_bool),
            Some(true),
            "a fresh instance requires setup"
        );

        let first = server
            .post("/auth/setup")
            .json(&serde_json::json!({
                "email": "admin@example.com",
                "display_name": "Admin",
                "password": "a strong password",
            }))
            .await;
        assert_eq!(
            first.status_code(),
            StatusCode::CREATED,
            "first setup mints the administrator"
        );

        // setup_required flips false after bootstrap.
        let status2: serde_json::Value = server.get("/auth/setup/status").await.json();
        assert_eq!(
            status2
                .get("setup_required")
                .and_then(serde_json::Value::as_bool),
            Some(false),
            "setup_required is false once an admin exists"
        );

        // Invariant 1: a second setup is rejected (409) once an admin exists.
        let second = server
            .post("/auth/setup")
            .json(&serde_json::json!({
                "email": "other@example.com",
                "display_name": "Other",
                "password": "another strong one",
            }))
            .await;
        assert_eq!(
            second.status_code(),
            StatusCode::CONFLICT,
            "first-run setup is closed once an administrator exists"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn setup_enforces_password_min_length(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

        let resp = server
            .post("/auth/setup")
            .json(&serde_json::json!({
                "email": "admin@example.com",
                "display_name": "Admin",
                "password": "short",
            }))
            .await;
        assert_eq!(
            resp.status_code(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "a password below the minimum length is rejected"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn forgot_password_is_generic_for_unknown_email(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

        // No such account: still a generic 200 (no enumeration). This path writes
        // no PIN file, so it cannot race the happy-path test below.
        let resp = server
            .post("/auth/forgot-password")
            .json(&serde_json::json!({"email": "ghost@example.com"}))
            .await;
        assert_eq!(resp.status_code(), StatusCode::OK);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn reset_password_with_invalid_pin_is_generic_422(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
        test_support::db::create_adult_with_password(
            &app_pool,
            "reset-bad",
            "reset-bad@example.com",
            "original password",
        )
        .await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

        // No PIN was ever issued: any reset attempt is the generic 422.
        let resp = server
            .post("/auth/reset-password")
            .json(&serde_json::json!({
                "email": "reset-bad@example.com",
                "pin": "0000000000",
                "new_password": "a brand new password",
            }))
            .await;
        assert_eq!(resp.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn forgot_then_reset_changes_the_password(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
        let user_id = test_support::db::create_adult_with_password(
            &app_pool,
            "recover",
            "recover@example.com",
            "the old password",
        )
        .await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

        let forgot = server
            .post("/auth/forgot-password")
            .json(&serde_json::json!({"email": "recover@example.com"}))
            .await;
        assert_eq!(forgot.status_code(), StatusCode::OK);

        // Read the clear PIN from the user's operator file (the model stores only
        // its hash).
        let pin_path = std::path::Path::new(&test_support::test_config().recovery_pin_dir)
            .join(format!("{user_id}.pin"));
        let contents = std::fs::read_to_string(&pin_path).expect("recovery PIN file written");
        let pin = contents
            .lines()
            .find_map(|l| l.strip_prefix("pin: "))
            .expect("PIN line in file")
            .to_owned();

        let reset = server
            .post("/auth/reset-password")
            .json(&serde_json::json!({
                "email": "recover@example.com",
                "pin": pin,
                "new_password": "a brand new password",
            }))
            .await;
        assert_eq!(
            reset.status_code(),
            StatusCode::OK,
            "a valid PIN resets the password"
        );

        // No auto-login: the new password works, the old one does not.
        let with_new = server
            .post("/auth/local/login")
            .json(&serde_json::json!({
                "email": "recover@example.com",
                "password": "a brand new password",
            }))
            .await;
        assert_eq!(
            with_new.status_code(),
            StatusCode::NO_CONTENT,
            "the new password authenticates"
        );

        // A consumed PIN cannot be reused.
        let reuse = server
            .post("/auth/reset-password")
            .json(&serde_json::json!({
                "email": "recover@example.com",
                "pin": pin,
                "new_password": "yet another password",
            }))
            .await;
        assert_eq!(
            reuse.status_code(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "a consumed PIN is single-use"
        );
        // The successful reset already removed the PIN file; nothing to clean up.
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn reset_password_invalidates_existing_sessions(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
        let user_id = test_support::db::create_adult_with_password(
            &app_pool,
            "stale",
            "stale@example.com",
            "the old password",
        )
        .await;
        let mut server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
        server.save_cookies();

        // Establish a session and confirm it is live before the reset.
        let login = server
            .post("/auth/local/login")
            .json(
                &serde_json::json!({"email": "stale@example.com", "password": "the old password"}),
            )
            .await;
        assert_eq!(login.status_code(), StatusCode::NO_CONTENT);
        assert_eq!(
            server.get("/auth/me").await.status_code(),
            StatusCode::OK,
            "the session authenticates before the reset"
        );

        // Recover and reset the password. A session that predates the reset (e.g.
        // an attacker's stolen cookie) must not survive it.
        let forgot = server
            .post("/auth/forgot-password")
            .json(&serde_json::json!({"email": "stale@example.com"}))
            .await;
        assert_eq!(forgot.status_code(), StatusCode::OK);
        let pin_path = std::path::Path::new(&test_support::test_config().recovery_pin_dir)
            .join(format!("{user_id}.pin"));
        let contents = std::fs::read_to_string(&pin_path).expect("recovery PIN file written");
        let pin = contents
            .lines()
            .find_map(|l| l.strip_prefix("pin: "))
            .expect("PIN line in file")
            .to_owned();
        let reset = server
            .post("/auth/reset-password")
            .json(&serde_json::json!({
                "email": "stale@example.com",
                "pin": pin,
                "new_password": "a brand new password",
            }))
            .await;
        assert_eq!(reset.status_code(), StatusCode::OK);

        // The pre-reset session is now rejected: the reset bumped session_version.
        assert_eq!(
            server.get("/auth/me").await.status_code(),
            StatusCode::UNAUTHORIZED,
            "a session established before the reset is invalidated by it"
        );
    }
}
