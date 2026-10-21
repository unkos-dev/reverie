//! Authentication routes — OIDC login / callback, session logout, and the
//! cookie-authenticated `/auth/me` profile + theme-preference endpoints.

use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header};
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
use crate::auth::theme_cookie::set_theme_cookie;
use crate::error::AppError;
use crate::extract::ApiJson;
use crate::models::theme_preference::ThemePreference;
use crate::models::{sign_in, user};
use crate::state::AppState;

/// Build the `/auth/*` router (login / callback / logout / me / theme).
///
/// It is an [`OpenApiRouter`] so each handler's `#[utoipa::path]` contributes
/// to the generated spec (a missing annotation fails to compile). Merged into
/// `crate::openapi::pilot_router` and split into its runtime and spec halves
/// there.
pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(oidc_login))
        .routes(routes!(callback))
        .routes(routes!(local_login))
        .routes(routes!(setup_status))
        .routes(routes!(setup))
        .routes(routes!(register))
        .routes(routes!(forgot_password))
        .routes(routes!(reset_password))
        .routes(routes!(lookup_credential_link))
        .routes(routes!(redeem_credential_link))
        .routes(routes!(logout))
        .routes(routes!(me))
        .routes(routes!(update_theme))
}

/// Success or provider-error parameters returned to `/auth/callback`.
#[derive(serde::Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct CallbackParams {
    /// Authorization code, present only on a successful provider response.
    code: Option<String>,
    /// Browser-bound state echoed by the provider.
    #[param(max_length = 22)]
    state: Option<String>,
    /// Provider error code, mutually exclusive with the authorization code.
    error: Option<String>,
    /// Provider description, discarded without logging or display.
    #[serde(rename = "error_description")]
    #[param(rename = "error_description")]
    _error_description: Option<String>,
}

impl CallbackParams {
    fn parse(self) -> Result<(String, AuthorizationResponse), SignInOutcome> {
        let state = self
            .state
            .filter(|state| !state.is_empty())
            .ok_or(SignInOutcome::Expired)?;
        let response = match (self.code, self.error) {
            (Some(code), None) if !code.is_empty() => AuthorizationResponse::Code(code),
            (None, Some(error)) if !error.is_empty() => AuthorizationResponse::Error(error),
            _ => return Err(SignInOutcome::Expired),
        };
        Ok((state, response))
    }
}

enum AuthorizationResponse {
    Code(String),
    Error(String),
}

#[derive(Clone, Copy)]
enum SignInOutcome {
    Denied,
    Expired,
    Unavailable,
    Keys,
    Account,
    Throttled,
}

impl SignInOutcome {
    fn unavailable(cause: &'static str) -> Self {
        tracing::warn!(cause, "OIDC sign-in internal failure");
        Self::Unavailable
    }

    fn redirect(self) -> Redirect {
        let location = match self {
            Self::Denied => "/login?outcome=denied",
            Self::Expired => "/login?outcome=expired",
            Self::Unavailable => "/login?outcome=unavailable",
            Self::Keys => "/login?outcome=keys",
            Self::Account => "/login?outcome=account",
            Self::Throttled => "/login?outcome=throttled",
        };
        if matches!(self, Self::Throttled) {
            tracing::debug!(outcome = location, "OIDC sign-in did not complete");
        } else {
            tracing::warn!(outcome = location, "OIDC sign-in did not complete");
        }
        Redirect::to(location)
    }
}

impl From<crate::auth::oidc::OidcVerificationError> for SignInOutcome {
    fn from(error: crate::auth::oidc::OidcVerificationError) -> Self {
        use crate::auth::oidc::{OidcRefreshError, OidcVerificationError};
        use openidconnect::{ClaimsVerificationError, SignatureVerificationError};
        match error {
            OidcVerificationError::Refresh(OidcRefreshError::Unavailable) => {
                Self::unavailable("oidc_runtime_unavailable")
            }
            OidcVerificationError::Refresh(_)
            | OidcVerificationError::Claims(ClaimsVerificationError::SignatureVerification(
                SignatureVerificationError::NoMatchingKey
                | SignatureVerificationError::AmbiguousKeyId(_),
            )) => Self::Keys,
            OidcVerificationError::Claims(_) => Self::unavailable("claims_verification_failed"),
        }
    }
}

/// `GET /auth/oidc/login`: start the OIDC authorization-code + PKCE flow.
///
/// Renamed from `/auth/login` so the user-facing login page (a SPA route served
/// at `/login`) does not collide with this auth-protocol endpoint; `/auth/*` is
/// the auth API namespace. The OIDC callback path deliberately stays
/// `/auth/callback` (NOT `/auth/oidc/callback`) so operators need not reconfigure
/// `OIDC_REDIRECT_URI` at their `IdP`.
///
/// # Errors
/// - [`AppError::NotFound`] when OIDC is not configured.
/// Signed-in browsers go to `/`; throttled, unavailable or inactive starts go to a fixed login outcome.
/// - [`AppError::Internal`] when session or attempt storage fails.
#[utoipa::path(
    get,
    path = "/auth/oidc/login",
    summary = "Start OIDC login",
    description = "Redirects to the configured OIDC provider to begin the authorization-code flow. No authentication is required to call it. Signed-in browsers redirect to /. A source limit redirects to /login?outcome=throttled and an inactive cohort to /login?outcome=expired. Pending discovery redirects to /login?outcome=unavailable. Returns 404 if OIDC is not configured on this instance.",
    tag = "auth",
    security(()),
    responses(
        (status = 303, description = "Already signed in: redirect to /; source limit: /login?outcome=throttled; inactive cohort: /login?outcome=expired; discovery pending: /login?outcome=unavailable"),
        (status = 307, description = "Redirect to the OIDC issuer's authorization endpoint; attempt material is persisted against the anonymous session"),
        (status = 404, description = "OIDC is not configured on this instance", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
    )
)]
async fn oidc_login(
    State(state): State<AppState>,
    session: Session,
    headers: HeaderMap,
    peer: crate::auth::rate_limit::PeerAddr,
) -> Result<impl IntoResponse, AppError> {
    // OIDC is optional. When unconfigured this handler is
    // unreachable in practice (the SPA hides the OIDC action), but guard so a
    // direct hit 404s cleanly rather than acting on an absent client.
    let oidc = state.oidc.as_ref().ok_or(AppError::NotFound)?;

    let had_session_id = session.id().is_some();
    if matches!(
        crate::auth::session::signed_in_user(&session, &state.pool).await?,
        crate::auth::session::SessionUser::SignedIn(_)
    ) {
        return Ok(Redirect::to("/"));
    }
    if enforce_source_rate_limit(&state, &state.oidc_limiter, &headers, &peer).is_err() {
        return Ok(SignInOutcome::Throttled.redirect());
    }
    let Some(snapshot) = oidc.snapshot().await else {
        return Ok(SignInOutcome::Unavailable.redirect());
    };
    let oidc_client = &snapshot.client;
    if let Some(id) = session.id()
        && sign_in::join_pending(&state.pool, &id.to_string())
            .await
            .map_err(|e| AppError::Internal(e.into()))?
            == sign_in::CohortState::Won
    {
        return Ok(SignInOutcome::Expired.redirect());
    }
    if had_session_id && session.id().is_none() {
        session
            .cycle_id()
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
    }
    session
        .insert("sign_in_started", true)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    session
        .save()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    let id = session
        .id()
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("saved session has no id")))?;

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

    sign_in::start_oidc_attempt(
        &state.pool,
        &id.to_string(),
        csrf_token.secret(),
        pkce_verifier.secret(),
        nonce.secret(),
    )
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    Ok(Redirect::temporary(auth_url.as_str()))
}

/// Complete a browser-bound OIDC attempt or redirect to a fixed login outcome.
///
/// # Errors
/// Returns [`AppError::NotFound`] when OIDC is not configured.
#[utoipa::path(
    get,
    path = "/auth/callback",
    summary = "Complete OIDC login",
    description = "Consumes a browser-bound authorization response once. Success establishes a session and redirects to /. Signed-in browsers redirect to / before query validation. Callback failures redirect to /login?outcome=denied, expired, unavailable, keys or account. Provider descriptions are never shown or logged. Returns 404 when OIDC is not configured. A failure of the session middleware's subsequent save can still return an empty 500.",
    tag = "auth",
    security(()),
    params(CallbackParams),
    responses(
        (status = 303, description = "Signed in: /; failure: fixed /login?outcome=denied, expired, unavailable, keys or account"),
        (status = 307, description = "Login complete; session established, theme cookie seeded, redirect to /"),
        (status = 404, description = "OIDC is not configured", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 500, description = "Session middleware subsequent save failed; empty response"),
    )
)]
async fn callback(
    State(state): State<AppState>,
    session: Session,
    jar: CookieJar,
    params: Result<Query<CallbackParams>, QueryRejection>,
) -> Result<(CookieJar, Redirect), AppError> {
    let oidc = state.oidc.as_ref().ok_or(AppError::NotFound)?;
    let result = async {
        if matches!(
            crate::auth::session::signed_in_user(&session, &state.pool)
                .await
                .map_err(|_| SignInOutcome::unavailable("signed_in_check_failed"))?,
            crate::auth::session::SessionUser::SignedIn(_)
        ) {
            return Ok(None);
        }
        let Query(params) = params.map_err(|_| SignInOutcome::Expired)?;
        let (callback_state, response) = params.parse()?;
        let id = session.id().ok_or(SignInOutcome::Expired)?.to_string();
        let attempt = sign_in::claim_oidc_attempt(&state.pool, &id, &callback_state)
            .await
            .map_err(|_| SignInOutcome::unavailable("attempt_claim_failed"))?
            .ok_or(SignInOutcome::Expired)?;
        let code = match response {
            AuthorizationResponse::Code(code) => code,
            AuthorizationResponse::Error(error) => {
                return Err(if error == "access_denied" {
                    SignInOutcome::Denied
                } else {
                    SignInOutcome::Unavailable
                });
            }
        };
        let snapshot = oidc
            .snapshot()
            .await
            .ok_or_else(|| SignInOutcome::unavailable("discovery_pending"))?;
        let token_response = snapshot
            .client
            .exchange_code(AuthorizationCode::new(code))
            .map_err(|_| SignInOutcome::unavailable("exchange_setup_failed"))?
            .set_pkce_verifier(PkceCodeVerifier::new(attempt.pkce_verifier))
            .request_async(&oidc.transport().oauth_client())
            .await
            .map_err(|error| match error {
                openidconnect::RequestTokenError::ServerResponse(error)
                    if error.error().as_ref() == "invalid_grant" =>
                {
                    SignInOutcome::Expired
                }
                _ => SignInOutcome::unavailable("token_request_failed"),
            })?;
        let id_token = token_response
            .id_token()
            .ok_or_else(|| SignInOutcome::unavailable("id_token_missing"))?;
        let claims = oidc
            .verify(id_token, &Nonce::new(attempt.nonce))
            .await
            .map_err(SignInOutcome::from)?;
        if !sign_in::claim_win(&state.pool, &id)
            .await
            .map_err(|_| SignInOutcome::unavailable("win_claim_failed"))?
        {
            return Err(SignInOutcome::Expired);
        }
        let completion = async {
            let user = provision_oidc_user(&state.pool, claims).await?;
            complete_oidc_session(&session, &user).await?;
            Ok(user)
        }
        .await;
        if completion.is_err() && session.flush().await.is_err() {
            tracing::warn!(
                cause = "session_flush_failed",
                "OIDC sign-in cleanup failed"
            );
        }
        completion.map(Some)
    }
    .await;
    match result {
        Ok(Some(user)) => Ok((
            set_theme_cookie(jar, user.theme_preference),
            Redirect::temporary("/"),
        )),
        Ok(None) => Ok((jar, Redirect::to("/"))),
        Err(outcome) => Ok((jar, outcome.redirect())),
    }
}

async fn provision_oidc_user(
    pool: &sqlx::PgPool,
    claims: &openidconnect::core::CoreIdTokenClaims,
) -> Result<user::User, SignInOutcome> {
    let subject = claims.subject().as_str();
    let issuer = claims.issuer().as_str();
    let display_name = claims
        .name()
        .and_then(|name| name.get(None))
        .map_or(subject, |name| name.as_str());
    let email = claims.email().map(|email| email.as_str());
    // THREAT: issuer/subject resolution never links an account by an unverified provider email.
    let user = user::upsert_from_oidc(pool, issuer, subject, display_name, email)
        .await
        .map_err(|error| match error {
            user::OidcProvisionError::EmailCollision => SignInOutcome::Account,
            user::OidcProvisionError::Db(_) => SignInOutcome::unavailable("provisioning_failed"),
        })?;
    if user.disabled_at.is_some() {
        return Err(SignInOutcome::Account);
    }
    Ok(user)
}

async fn complete_oidc_session(session: &Session, user: &user::User) -> Result<(), SignInOutcome> {
    crate::auth::session::login(session, user)
        .await
        .map_err(|error| {
            let cause = match error {
                tower_sessions::session::Error::Store(_) => "old_session_removal_failed",
                tower_sessions::session::Error::SerdeJson(_) => "identity_attachment_failed",
            };
            tracing::warn!(cause, "OIDC sign-in cleanup failed");
            SignInOutcome::Unavailable
        })?;
    let mut csrf_bytes = [0u8; 32];
    rand::fill(&mut csrf_bytes);
    let csrf_token = Base64UrlUnpadded::encode_string(&csrf_bytes);
    session
        .insert("csrf_token", &csrf_token)
        .await
        .map_err(|_| SignInOutcome::unavailable("csrf_insert_failed"))?;
    session
        .save()
        .await
        .map_err(|_| SignInOutcome::unavailable("session_save_failed"))?;
    Ok(())
}

/// Request body for `POST /auth/local/login`. Explicit allow-list (no
/// mass-assignment): only the two credential fields are accepted.
#[derive(serde::Deserialize, utoipa::ToSchema)]
struct LocalLoginRequest {
    /// Account email; matched case-insensitively against `users.email`.
    email: String,
    /// Plaintext password, verified against the stored Argon2id hash.
    #[schema(max_length = 256)]
    password: String,
}

/// `POST /auth/local/login`: email + password sign-in, establishing the same
/// session contract as the OIDC callback.
///
/// THREAT: equivalent admitted failures spend real or dummy verification; blocked requests spend neither.
///
/// # Errors
/// - [`AppError::NotFound`] when local authentication is disabled.
/// - [`AppError::RateLimited`] (429) when the per-source limit is exceeded, or a
///   request arrives during an active account block.
/// - [`AppError::Validation`] (422) on bad credentials (generic; no enumeration).
/// - [`AppError::SignInInactive`] (409) when signed in or arbitration is lost.
/// - [`AppError::Internal`] on session-store or database failure.
#[utoipa::path(
    post,
    path = "/auth/local/login",
    summary = "Sign in with email and password",
    description = "Admits the source and account before verifying `email` and `password`. Ten committed failures in a rolling 15-minute window start a 15-minute block; the tenth failure returns generic 422. Blocked requests, including correct passwords, return generic 429 with Retry-After without verification and do not extend the block. A winning enabled-account login clears accounting before session establishment. Returns 404 if local authentication is disabled. Returns 503 when authentication processing is busy.",
    tag = "auth",
    security(()),
    request_body = LocalLoginRequest,
    responses(
        (status = 503, description = "Authentication processing is busy", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 409, description = "Already signed in or sign-in no longer active", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 204, description = "Login succeeded; session established (id rotated, CSRF token minted, theme cookie seeded)"),
        (status = 404, description = "Local authentication is disabled on this instance", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 422, description = "Invalid credentials (generic; identical for unknown email and wrong password)", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 429, description = "Too many login attempts", body = crate::openapi::ProblemDetails, content_type = "application/problem+json", headers(("Retry-After" = u64, description = "Positive delay in seconds before retrying"))),
    )
)]
async fn local_login(
    State(state): State<AppState>,
    session: Session,
    jar: CookieJar,
    headers: HeaderMap,
    peer: crate::auth::rate_limit::PeerAddr,
    ApiJson(body): ApiJson<LocalLoginRequest>,
) -> Result<(CookieJar, StatusCode), AppError> {
    if !state.config.local_auth_enabled {
        return Err(AppError::NotFound);
    }

    enforce_source_rate_limit(&state, &state.login_limiter, &headers, &peer)?;

    if matches!(
        crate::auth::session::signed_in_user(&session, &state.pool).await?,
        crate::auth::session::SessionUser::SignedIn(_)
    ) {
        return Err(AppError::SignInInactive);
    }
    crate::auth::password_policy::enforce_max_length(
        &body.password,
        crate::auth::password_policy::MAX_PASSWORD_LENGTH,
    )
    .map_err(|_| AppError::Validation("invalid email or password".into()))?;
    let cohort_id = if let Some(id) = session.id() {
        let id = id.to_string();
        if sign_in::join_pending(&state.pool, &id)
            .await
            .map_err(|e| AppError::Internal(e.into()))?
            == sign_in::CohortState::Won
        {
            return Err(AppError::SignInInactive);
        }
        Some(id)
    } else {
        None
    };

    let permit = state.hash_limiter.acquire().await?;
    let mut attempt = match crate::models::login_throttle::admit(&state.pool, &body.email)
        .await
        .map_err(|e| AppError::Internal(e.into()))?
    {
        crate::models::login_throttle::Admission::Allowed(attempt) => attempt,
        crate::models::login_throttle::Admission::Blocked(retry_after_secs) => {
            return Err(AppError::RateLimited { retry_after_secs });
        }
    };
    // THREAT: rejected requests reach neither real nor dummy verification.
    let session_user =
        verify_local_account(attempt.connection(), &body.email, body.password, &permit).await?;
    if let Some(user) = session_user {
        if let Some(id) = &cohort_id
            && !sign_in::claim_win(attempt.connection(), id)
                .await
                .map_err(|e| AppError::Internal(e.into()))?
        {
            return Err(AppError::SignInInactive);
        }
        let login_result = async {
            #[cfg(test)]
            tests::password_checkpoint(tests::PasswordStage::BeforeCommit).await;
            attempt
                .succeed()
                .await
                .map_err(|e| AppError::Internal(e.into()))?;
            drop(permit);

            #[cfg(test)]
            tests::password_checkpoint(tests::PasswordStage::AfterCommit).await;

            // Session contract: rotate id + persist identity, mint the
            // CSRF synchronizer token, seed the theme cookie. Identical to the OIDC
            // callback, returning 204 rather than a redirect.
            crate::auth::session::login(&session, &user)
                .await
                .map_err(|e| AppError::Internal(anyhow::anyhow!("login failed: {e}")))?;
            Ok::<_, AppError>(())
        }
        .await;
        if let Err(error) = login_result {
            if cohort_id.is_some()
                && let Err(e) = session.flush().await
            {
                tracing::warn!(error = %e, "session flush failed after winning local sign-in");
            }
            return Err(error);
        }
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

    #[cfg(test)]
    tests::password_checkpoint(tests::PasswordStage::BeforeCommit).await;
    attempt
        .fail()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    drop(permit);
    #[cfg(test)]
    tests::password_checkpoint(tests::PasswordStage::AfterCommit).await;
    Err(AppError::Validation("invalid email or password".to_owned()))
}

async fn verify_local_account(
    connection: &mut sqlx::PgConnection,
    email: &str,
    password: String,
    permit: &crate::auth::password::HashPermit,
) -> Result<Option<user::User>, AppError> {
    let account = user::find_by_email(&mut *connection, email)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    let credential = match &account {
        Some(u) => crate::models::local_credentials::find_by_user_id(&mut *connection, u.id)
            .await
            .map_err(|e| AppError::Internal(e.into()))?,
        None => None,
    };
    #[cfg(test)]
    tests::password_checkpoint(tests::PasswordStage::Verify).await;
    let verified = if let Some(credential) = credential {
        permit
            .verify(password, credential.password_hash)
            .await
            .map_err(|e| AppError::Internal(e.into()))?
    } else {
        permit
            .verify_dummy(password)
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
        false
    };

    Ok(match account {
        Some(user) if verified && user.disabled_at.is_none() => Some(user),
        _ => None,
    })
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
    /// Minimum new-password length in Unicode scalar values.
    #[schema(format = Int64)]
    password_min_length: usize,
    /// Maximum new-password length in Unicode scalar values.
    #[schema(format = Int64)]
    password_max_length: usize,
}

/// `GET /auth/setup/status`: public provider/bootstrap state for the SPA.
///
/// # Errors
/// - [`AppError::Internal`] on database failure.
#[utoipa::path(
    get,
    path = "/auth/setup/status",
    summary = "Get setup status",
    description = "Returns whether initial setup is required, which sign-in providers are enabled, and the configured minimum and maximum new-password lengths. Sign-in and password-setting forms use this state. No authentication is required to call it.",
    tag = "auth",
    security(()),
    responses(
        (status = 200, description = "Setup, provider and password-length policy state", body = SetupStatusResponse),
    )
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
        password_min_length: state.config.password_min_length,
        password_max_length: state.config.password_max_length,
    }))
}

/// Request body for `POST /auth/setup`. Explicit allow-list (no mass-assignment).
#[derive(serde::Deserialize, utoipa::ToSchema)]
struct SetupRequest {
    /// First administrator's email in the common ASCII format with a dotted
    /// domain and an alphabetic suffix of at least two letters.
    email: String,
    /// Display name.
    display_name: String,
    /// Plaintext password; checked against the shared password policy, then hashed.
    #[schema(max_length = 256)]
    password: String,
}

/// `POST /auth/setup`: first-run bootstrap: mint the first administrator.
///
/// THREAT: bootstrap is the ONLY first-admin path. The race
/// guarantee is the DB `instance_bootstrap` singleton insert inside
/// [`user::create_first_admin`], NOT this `admin_exists` pre-check (which is only
/// a cheap fast-reject; a `SELECT EXISTS ... INSERT` does not serialize under
/// READ COMMITTED). A second concurrent setup loses the marker-row race and maps
/// to 409. Setup does NOT auto-login (parity with recovery; the session contract
/// lives only at `/auth/local/login`).
///
/// Rate-limited per source like the repeatable pre-auth handlers, despite
/// succeeding only once. The 409 fast-reject bounds this endpoint only after an
/// administrator exists; before that, every call reaches the Argon2 hash, so an
/// un-bootstrapped instance would otherwise grind on demand for anyone who can
/// reach it. The limit is enforced first, ahead of the `admin_exists` query.
///
/// # Errors
/// - [`AppError::RateLimited`] (429) when the per-source limit is exceeded.
/// - [`AppError::SetupAlreadyComplete`] (409) when an administrator already
///   exists (fast-reject or the marker-row race).
/// - [`AppError::Validation`] (422) on a malformed email or rejected password.
/// - [`AppError::Internal`] on hashing or database failure.
#[utoipa::path(
    post,
    path = "/auth/setup",
    summary = "Create the first administrator",
    description = "Creates the first administrator account during initial setup and does not sign the caller in. No authentication is required to call it. Returns 409 if an administrator already exists and 429 when too many requests have been made. Returns 503 when authentication processing is busy.",
    tag = "auth",
    security(()),
    request_body = SetupRequest,
    responses(
        (status = 503, description = "Authentication processing is busy", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 201, description = "First administrator created (no auto-login)"),
        (status = 409, description = "An administrator already exists", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 422, description = "Validation failed", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 429, description = "Too many requests", body = crate::openapi::ProblemDetails, content_type = "application/problem+json", headers(("Retry-After" = u64, description = "Positive delay in seconds before retrying"))),
    )
)]
async fn setup(
    State(state): State<AppState>,
    headers: HeaderMap,
    peer: crate::auth::rate_limit::PeerAddr,
    ApiJson(body): ApiJson<SetupRequest>,
) -> Result<StatusCode, AppError> {
    enforce_source_rate_limit(&state, &state.login_limiter, &headers, &peer)?;

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
    crate::auth::password_policy::enforce_from_config(
        &state.config,
        &body.password,
        &[&body.email, &body.display_name],
    )
    .await
    .map_err(|e| AppError::Validation(e.to_string()))?;
    let phc = state
        .hash_limiter
        .hash(body.password)
        .await?
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

/// Request body for `POST /auth/register`. Explicit allow-list (no
/// mass-assignment): a self-registrant cannot smuggle a role or child flag.
#[derive(serde::Deserialize, utoipa::ToSchema)]
struct RegisterRequest {
    /// New account email in the common ASCII format with a dotted domain and an
    /// alphabetic suffix of at least two letters.
    email: String,
    /// Display name.
    display_name: String,
    /// Plaintext password; enforced against the password policy, then hashed.
    #[schema(max_length = 256)]
    password: String,
}

/// `POST /auth/register`: config-gated self-service registration.
///
/// Gated on `self_registration_enabled` (404 when off) AND `local_auth_enabled`.
/// Unauthenticated and repeatable, and each call fires an Argon2 hash plus an
/// outbound HIBP request, so the per-source rate limit is enforced BEFORE any
/// policy/hash/breach work, as in `forgot_password`/`reset_password`. `setup`
/// takes the same limit for the pre-bootstrap window, where its 409 fast-reject
/// does not yet apply.
///
/// THREAT (privilege escalation): a self-registered account is
/// always an `adult`, never an admin or child. The DTO carries no role, and
/// `create_local` is called with [`Role::Adult`]; admin and child accounts are
/// minted only by an existing administrator. No session is established (parity
/// with setup/reset: the session contract lives only at `/auth/local/login`).
///
/// # Errors
/// - [`AppError::NotFound`] when self-registration or local auth is disabled.
/// - [`AppError::RateLimited`] (429) when the per-source limit is exceeded.
/// - [`AppError::Validation`] (422) on a malformed email or a policy failure.
/// - [`AppError::EmailConflict`] (409) when the email is already in use.
/// - [`AppError::Internal`] on hashing or database failure.
#[utoipa::path(
    post,
    path = "/auth/register",
    summary = "Register a new account",
    description = "Creates a new account with the `adult` role and does not sign the caller in. No authentication is required to call it. Returns 404 if self-registration or local authentication is disabled, 409 if the email is already in use, and 429 when too many requests have been made. Returns 503 when authentication processing is busy.",
    tag = "auth",
    security(()),
    request_body = RegisterRequest,
    responses(
        (status = 503, description = "Authentication processing is busy", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 201, description = "Account created (no auto-login)"),
        (status = 404, description = "Self-registration is disabled on this instance", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 409, description = "Email already in use", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 422, description = "Validation failed", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 429, description = "Too many requests", body = crate::openapi::ProblemDetails, content_type = "application/problem+json", headers(("Retry-After" = u64, description = "Positive delay in seconds before retrying"))),
    )
)]
async fn register(
    State(state): State<AppState>,
    headers: HeaderMap,
    peer: crate::auth::rate_limit::PeerAddr,
    ApiJson(body): ApiJson<RegisterRequest>,
) -> Result<StatusCode, AppError> {
    if !state.config.self_registration_enabled || !state.config.local_auth_enabled {
        return Err(AppError::NotFound);
    }
    enforce_source_rate_limit(&state, &state.login_limiter, &headers, &peer)?;

    if !user::is_addr_spec(&body.email) {
        return Err(AppError::Validation("invalid email address".to_owned()));
    }
    crate::auth::password_policy::enforce_from_config(
        &state.config,
        &body.password,
        &[&body.email, &body.display_name],
    )
    .await
    .map_err(|e| AppError::Validation(e.to_string()))?;

    let phc = state
        .hash_limiter
        .hash(body.password)
        .await?
        .map_err(|e| AppError::Internal(anyhow::anyhow!("password hash failed: {e}")))?;

    match user::create_local(
        &state.pool,
        &body.email,
        &body.display_name,
        crate::models::role::Role::Adult,
        Some(&phc),
    )
    .await
    {
        Ok(_user) => Ok(StatusCode::CREATED),
        Err(user::CreateUserError::EmailExists) => Err(AppError::EmailConflict),
        Err(user::CreateUserError::Db(e)) => Err(AppError::Internal(e.into())),
    }
}

/// Apply the selected source allowance using the existing trusted-header boundary.
fn enforce_source_rate_limit(
    state: &AppState,
    limiter: &crate::auth::rate_limit::LoginLimiter,
    headers: &HeaderMap,
    peer: &crate::auth::rate_limit::PeerAddr,
) -> Result<(), AppError> {
    let ip = peer.0.map(|addr| addr.ip());
    if let Some(ip) = crate::auth::rate_limit::client_ip(
        headers,
        ip,
        state.config.trusted_client_ip_header.as_deref(),
    ) && let Err(denial) = limiter.check_key(&ip)
    {
        use governor::clock::Clock;
        let wait = denial.wait_time_from(governor::clock::DefaultClock::default().now());
        let retry_after_secs = wait
            .as_secs()
            .saturating_add(u64::from(wait.subsec_nanos() > 0))
            .max(1);
        return Err(AppError::RateLimited { retry_after_secs });
    }
    Ok(())
}

#[derive(serde::Deserialize, utoipa::ToSchema)]
struct CredentialLinkLookupRequest {
    #[schema(
        min_length = 43,
        max_length = 43,
        pattern = "^[A-Za-z0-9_-]{42}[AEIMQUYcgkosw048]$"
    )]
    token: String,
}

#[derive(serde::Serialize, utoipa::ToSchema)]
struct CredentialLinkLookupResponse {
    email: String,
    has_password: bool,
}

#[derive(serde::Deserialize, utoipa::ToSchema)]
struct CredentialLinkRedemptionRequest {
    #[schema(
        min_length = 43,
        max_length = 43,
        pattern = "^[A-Za-z0-9_-]{42}[AEIMQUYcgkosw048]$"
    )]
    token: String,
    #[schema(max_length = 256)]
    password: String,
}

fn invalid_credential_link() -> AppError {
    AppError::Validation("invalid or expired link".into())
}

/// Resolve account context using a live credential link.
#[utoipa::path(
    post, path = "/auth/credential-links/lookups", summary = "Look up a credential link",
    description = "Returns the email and password state for a valid link. Invalid, expired, used, disabled-account and no-email links share one generic refusal.",
    tag = "auth", security(()), request_body = CredentialLinkLookupRequest,
    responses(
        (status = 200, description = "Valid link", body = CredentialLinkLookupResponse, headers(("Cache-Control" = String, description = "no-store"))),
        (status = 404, description = "Local authentication is disabled", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 422, description = "Invalid or expired link", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 429, description = "Too many recovery requests", body = crate::openapi::ProblemDetails, content_type = "application/problem+json", headers(("Retry-After" = u64, description = "Positive delay in seconds before retrying"))),
    )
)]
async fn lookup_credential_link(
    State(state): State<AppState>,
    headers: HeaderMap,
    peer: crate::auth::rate_limit::PeerAddr,
    ApiJson(body): ApiJson<CredentialLinkLookupRequest>,
) -> Result<impl IntoResponse, AppError> {
    if !state.config.local_auth_enabled {
        return Err(AppError::NotFound);
    }
    enforce_source_rate_limit(&state, &state.recovery_limiter, &headers, &peer)?;
    let account = crate::models::credential_link::lookup(&state.pool, &body.token)
        .await
        .map_err(|e| AppError::Internal(e.into()))?
        .ok_or_else(invalid_credential_link)?;
    Ok((
        [(header::CACHE_CONTROL, "no-store")],
        Json(CredentialLinkLookupResponse {
            email: account.email,
            has_password: account.has_password,
        }),
    ))
}

/// Consume a credential link and invalidate earlier sessions without signing in.
#[utoipa::path(
    post, path = "/auth/credential-links/redemptions", summary = "Redeem a credential link",
    description = "Sets the account's password once before the link expires, invalidates earlier sessions and creates no session. Leaves login throttling and device tokens unchanged.",
    tag = "auth", security(()), request_body = CredentialLinkRedemptionRequest,
    responses(
        (status = 204, description = "Password set; sign-in required"),
        (status = 404, description = "Local authentication is disabled", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 422, description = "Invalid or expired link, or password policy rejection", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 429, description = "Too many recovery requests", body = crate::openapi::ProblemDetails, content_type = "application/problem+json", headers(("Retry-After" = u64, description = "Positive delay in seconds before retrying"))),
        (status = 503, description = "Authentication processing is busy", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
    )
)]
async fn redeem_credential_link(
    State(state): State<AppState>,
    headers: HeaderMap,
    peer: crate::auth::rate_limit::PeerAddr,
    ApiJson(body): ApiJson<CredentialLinkRedemptionRequest>,
) -> Result<impl IntoResponse, AppError> {
    let request_id = Uuid::new_v4();
    let source_ip = crate::auth::rate_limit::client_ip(
        &headers,
        peer.0.map(|address| address.ip()),
        state.config.trusted_client_ip_header.as_deref(),
    );
    // THREAT: Untrusted agent text is bounded and stripped of control characters before logging.
    let user_agent = headers.get(header::USER_AGENT).map(|value| {
        String::from_utf8_lossy(value.as_bytes())
            .chars()
            .filter(|c| !c.is_control())
            .take(256)
            .collect::<String>()
    });
    let mut user_id = None;
    let result: Result<StatusCode, AppError> = async {
        if !state.config.local_auth_enabled {
            return Err(AppError::NotFound);
        }
        enforce_source_rate_limit(&state, &state.recovery_limiter, &headers, &peer)?;
        let account = crate::models::credential_link::lookup(&state.pool, &body.token)
            .await
            .map_err(|e| AppError::Internal(e.into()))?
            .ok_or_else(invalid_credential_link)?;
        user_id = Some(account.user_id);
        crate::auth::password_policy::enforce_from_config(
            &state.config,
            &body.password,
            &[&account.email, &account.display_name],
        )
        .await
        .map_err(|e| AppError::Validation(e.to_string()))?;
        let phc = state
            .hash_limiter
            .hash(body.password)
            .await?
            .map_err(|e| AppError::Internal(e.into()))?;
        let mut tx = state
            .pool
            .begin()
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
        // THREAT: Every password writer locks the user before credential or proof rows.
        let target = sqlx::query!(
            "SELECT disabled_at, email FROM users WHERE id = $1 FOR NO KEY UPDATE",
            account.user_id
        )
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| AppError::Internal(e.into()))?
        .ok_or_else(invalid_credential_link)?;
        let consumed =
            crate::models::credential_link::consume(&mut *tx, account.user_id, &body.token)
                .await
                .map_err(|e| AppError::Internal(e.into()))?;
        if !consumed || target.disabled_at.is_some() || target.email.is_none() {
            return Err(invalid_credential_link());
        }
        crate::models::local_credentials::set_password(&mut *tx, account.user_id, &phc)
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
        // THREAT: Sessions issued before replacement must no longer authenticate.
        user::increment_session_version(&mut *tx, account.user_id)
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
        tx.commit()
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
        Ok(StatusCode::NO_CONTENT)
    }
    .await;
    tracing::info!(
        action = "credential_link_redemption",
        outcome = if result.is_ok() { "success" } else { "failure" },
        user_id = ?user_id,
        request_id = %request_id,
        source_ip = ?source_ip,
        user_agent = user_agent.as_deref(),
        "credential link redemption"
    );
    result
}

/// Request body for `POST /auth/forgot-password`.
#[derive(serde::Deserialize, utoipa::ToSchema)]
struct ForgotPasswordRequest {
    /// Account email to start recovery for.
    email: String,
}

/// `POST /auth/forgot-password`: start email-less PIN recovery.
///
/// THREAT (codeguard #2): always returns a generic 200; the response never
/// reveals whether the email exists. When it does, a fresh CSPRNG PIN is
/// generated, prior active PINs for the user are superseded (at most one active
/// PIN), the Argon2id hash row is persisted FIRST, then the clear PIN is written
/// to a per-user operator-readable file (mode 0600). Both writes happen under one
/// per-user issuance lock, so the published file always describes the single
/// active row even when another process issues concurrently. A failed file write
/// leaves an unconsumed-but-unusable row that simply expires; it is never a
/// cleartext PIN with no consuming row. On an unknown email, equivalent Argon2 work is spent so
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
    summary = "Start password recovery",
    description = "Starts PIN-based password recovery for the given email and always returns the same generic response so it cannot be used to determine whether an account exists. No authentication is required to call it. Returns 404 if local authentication is disabled and 429 when too many requests have been made. Returns 503 when authentication processing is busy.",
    tag = "auth",
    security(()),
    request_body = ForgotPasswordRequest,
    responses(
        (status = 503, description = "Authentication processing is busy", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 200, description = "Recovery started if the account exists (generic; no enumeration)"),
        (status = 404, description = "Local authentication is disabled", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 429, description = "Too many requests", body = crate::openapi::ProblemDetails, content_type = "application/problem+json", headers(("Retry-After" = u64, description = "Positive delay in seconds before retrying"))),
    )
)]
async fn forgot_password(
    State(state): State<AppState>,
    headers: HeaderMap,
    peer: crate::auth::rate_limit::PeerAddr,
    ApiJson(body): ApiJson<ForgotPasswordRequest>,
) -> Result<StatusCode, AppError> {
    if !state.config.local_auth_enabled {
        return Err(AppError::NotFound);
    }
    enforce_source_rate_limit(&state, &state.recovery_limiter, &headers, &peer)?;

    let user = user::find_by_email(&state.pool, &body.email)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    if let Some(user) = user {
        // Hash before opening the transaction (Argon2 is CPU-bound; do not hold a
        // connection across it).
        let pin = crate::auth::recovery::generate_pin();
        let pin_hash = state
            .hash_limiter
            .hash(pin.clone())
            .await?
            .map_err(|e| AppError::Internal(anyhow::anyhow!("pin hash failed: {e}")))?;
        let expires_at =
            chrono::Utc::now() + chrono::TimeDelta::seconds(state.config.recovery_pin_ttl_secs);

        // THREAT (publishing an unpersisted PIN): supersede, persist, and write
        // the operator file as one serialized step, so this request publishes a
        // PIN only if it also owns the active row when the file is written. A
        // request that stands down for a concurrent issuance publishes nothing:
        // the winner's PIN is the live one, and overwriting the file with an
        // unowned PIN would deny recovery. Either way the response is the same
        // generic 200 (no enumeration, no 500).
        let email = user.email.as_deref().unwrap_or(&body.email);
        let issuance = crate::auth::recovery::PinIssuance {
            user_id: user.id,
            email: email.to_owned(),
            pin,
            pin_hash,
            expires_at,
        };
        match crate::auth::recovery::issue_pin(
            &state.pool,
            std::path::Path::new(&state.config.recovery_pin_dir),
            issuance,
        )
        .await
        {
            Ok(_) => {}
            // Never surface to the client; the row expires harmlessly.
            Err(e @ crate::auth::recovery::IssuePinError::Publish(_)) => {
                tracing::error!(error = %e, "failed to write recovery PIN file");
            }
            Err(e) => return Err(AppError::Internal(e.into())),
        }
    } else {
        // Unknown email: spend comparable Argon2 work so response timing does not
        // reveal account existence.
        let permit = state.hash_limiter.acquire().await?;
        permit
            .verify_dummy(body.email)
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
        drop(permit);
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
    /// New plaintext password; enforced against the full password policy.
    #[schema(max_length = 256)]
    new_password: String,
}

/// `POST /auth/reset-password`: complete recovery with a PIN.
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
    summary = "Reset password with a PIN",
    description = "Completes password recovery using the PIN issued by `/auth/forgot-password` and does not sign the caller in. No authentication is required to call it. Returns 404 if local authentication is disabled, 422 for an invalid or expired PIN, unknown email, or a new password that fails the password policy, and 429 when too many requests have been made. Returns 503 when authentication processing is busy.",
    tag = "auth",
    security(()),
    request_body = ResetPasswordRequest,
    responses(
        (status = 503, description = "Authentication processing is busy", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 200, description = "Password reset; no session established (re-authentication required)"),
        (status = 404, description = "Local authentication is disabled", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 422, description = "Invalid or expired reset request (generic)", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 429, description = "Too many requests", body = crate::openapi::ProblemDetails, content_type = "application/problem+json", headers(("Retry-After" = u64, description = "Positive delay in seconds before retrying"))),
    )
)]
async fn reset_password(
    State(state): State<AppState>,
    headers: HeaderMap,
    peer: crate::auth::rate_limit::PeerAddr,
    ApiJson(body): ApiJson<ResetPasswordRequest>,
) -> Result<StatusCode, AppError> {
    if !state.config.local_auth_enabled {
        return Err(AppError::NotFound);
    }
    enforce_source_rate_limit(&state, &state.recovery_limiter, &headers, &peer)?;

    let generic = || AppError::Validation("invalid or expired reset request".to_owned());

    // The recovery path sets a credential, so it runs the same strength policy as
    // every other credential-setting path: an attacker holding a PIN must not be
    // able to install a weak or breached password here. Enforced before the email
    // lookup, and a rejection is mapped to the generic error, so neither the
    // outcome nor the timing reveals whether the account or PIN exists.
    crate::auth::password_policy::enforce_from_config(
        &state.config,
        &body.new_password,
        &[&body.email],
    )
    .await
    .map_err(|_| generic())?;

    let user = user::find_by_email(&state.pool, &body.email)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    let active = match &user {
        Some(u) => crate::models::password_reset_pin::find_active_by_user(&state.pool, u.id)
            .await
            .map_err(|e| AppError::Internal(e.into()))?,
        None => None,
    };
    let verified = {
        let permit = state.hash_limiter.acquire().await?;
        let verified = if let Some(pin) = &active {
            permit
                .verify(body.pin, pin.pin_hash.clone())
                .await
                .map_err(|e| AppError::Internal(e.into()))?
        } else {
            permit
                .verify_dummy(body.pin)
                .await
                .map_err(|e| AppError::Internal(e.into()))?;
            false
        };
        drop(permit);
        verified
    };

    if verified && let (Some(user), Some(pin)) = (&user, &active) {
        // Hash before opening the transaction: Argon2 is CPU-bound (~100 ms) and
        // must not be held across an open DB connection/locks.
        let phc = state
            .hash_limiter
            .hash(body.new_password)
            .await?
            .map_err(|e| AppError::Internal(anyhow::anyhow!("password hash failed: {e}")))?;

        // One transaction: consume the PIN, write the new credential, and
        // invalidate existing sessions atomically. A partial apply (PIN spent
        // but password unchanged) would lock the user out with no usable PIN.
        let mut tx = state
            .pool
            .begin()
            .await
            .map_err(|e| AppError::Internal(e.into()))?;

        // THREAT: lock the user before PIN and credential rows to match the other password writers.
        let exists = sqlx::query_scalar!(
            "SELECT id FROM users WHERE id = $1 FOR NO KEY UPDATE",
            user.id
        )
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
        if exists.is_none() {
            return Err(generic());
        }

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
/// Public per the OpenAPI security scheme (no session is required to call
/// it), but a session-authenticated caller is still a mutating request under
/// [`crate::security::csrf::csrf_required`], which runs ahead of this
/// handler and enforces the synchronizer token.
///
/// # Errors
/// - [`AppError::CsrfMissing`] (428) when a session-authenticated caller
///   sends no `X-CSRF-Token` header.
/// - [`AppError::CsrfMismatch`] (403) when the header does not match the
///   session-stored token.
/// - [`AppError::Internal`] when the session store read (CSRF check) or
///   session deletion fails.
#[utoipa::path(
    post,
    path = "/auth/logout",
    summary = "Sign out",
    description = "Destroys the caller's session, if any; calling it without a session is a no-op. No authentication is required to call it, but a session-authenticated caller must send a valid `X-CSRF-Token` header. Returns 428 if that header is missing and 403 if it does not match the session's token.",
    tag = "auth",
    security(()),
    responses(
        (status = 204, description = "Session destroyed (no-op without one)"),
        (status = 403, description = "X-CSRF-Token header present but does not match the session token", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 428, description = "X-CSRF-Token header required for a session-authenticated caller", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
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
    /// Whether this account has a local password, independently of OIDC identities.
    has_local_password: bool,
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
    summary = "Get the current profile",
    description = "Returns the authenticated caller's profile, including the `csrf_token` to echo back as `X-CSRF-Token` on unsafe requests (`null` for sessions authenticated with OPDS Basic auth).",
    tag = "auth",
    responses(
        (status = 200, description = "Caller profile; `csrf_token` is the synchronizer token for unsafe verbs (null for Basic-auth sessions)", body = MeResponse),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
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
    // first-party SPA (see docs/adr/0011-json-api-conventions-for-the-browser-facing-rest-surface.md). The
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
    let has_local_password = crate::models::local_credentials::exists_for_user(&state.pool, u.id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    Ok(Json(MeResponse {
        id: u.id,
        display_name: u.display_name,
        email: u.email,
        role: u.role,
        is_child: u.is_child,
        has_local_password,
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

// Serde is the wire-boundary gate: an invalid theme_preference is a 422 from
// the JSON extractor, which the patch_theme_rejects_invalid_value test pins.
/// `PATCH /auth/me/theme` — persist the caller's theme preference and
/// refresh the FOUC theme cookie.
///
/// # Errors
/// - [`AppError::Internal`] on database errors.
#[utoipa::path(
    patch,
    path = "/auth/me/theme",
    summary = "Set the theme preference",
    description = "Persists the caller's theme preference and refreshes the `reverie_theme` cookie.",
    tag = "auth",
    request_body = UpdateThemeRequest,
    responses(
        (status = 200, description = "Preference persisted; `reverie_theme` cookie refreshed", body = ThemeResponse),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 422, description = "Unknown theme_preference value", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
    )
)]
async fn update_theme(
    current_user: CurrentUser,
    State(state): State<AppState>,
    jar: CookieJar,
    ApiJson(body): ApiJson<UpdateThemeRequest>,
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
    use crate::state::AppState;
    use axum::http::StatusCode;

    use crate::models::theme_preference::ThemePreference;
    use crate::models::{sign_in, user};
    use crate::test_support;

    fn callback_log_subscriber(
        logs: &test_support::CapturedLogs,
    ) -> impl tracing::Subscriber + Send + Sync + use<> {
        let writer = logs.clone();
        tracing_subscriber::fmt()
            .without_time()
            .with_ansi(false)
            .with_max_level(tracing::Level::DEBUG)
            .with_writer(move || writer.clone())
            .finish()
    }

    fn assert_private_cause(logs: &test_support::CapturedLogs, cause: &str, secrets: &[&str]) {
        let captured = logs.text();
        let field = format!("cause=\"{cause}\"");
        let events: Vec<_> = captured
            .lines()
            .filter(|line| line.contains(&field))
            .collect();
        assert_eq!(events.len(), 1, "{captured}");
        assert!(events[0].trim_start().starts_with("WARN"), "{captured}");
        for secret in secrets {
            assert!(!captured.contains(secret), "cause log leaked {secret}");
        }
    }

    #[test]
    fn oidc_outcome_internal_causes_are_static_and_private() {
        for cause in [
            "signed_in_check_failed",
            "attempt_claim_failed",
            "exchange_setup_failed",
            "token_request_failed",
            "win_claim_failed",
            "csrf_insert_failed",
            "session_save_failed",
            "discovery_pending",
            "id_token_missing",
            "oidc_runtime_unavailable",
            "claims_verification_failed",
            "provisioning_failed",
        ] {
            let logs = test_support::CapturedLogs::default();
            let outcome = tracing::subscriber::with_default(callback_log_subscriber(&logs), || {
                super::SignInOutcome::unavailable(cause)
            });
            assert!(matches!(outcome, super::SignInOutcome::Unavailable));
            assert_private_cause(
                &logs,
                cause,
                &[
                    "private-code",
                    "private-state",
                    "private-nonce",
                    "private-provider-description",
                    "private-token-body",
                ],
            );
        }
    }

    #[test]
    fn oidc_outcome_throttled_logs_at_debug_while_other_outcomes_warn() {
        use super::SignInOutcome;

        let cases = [
            (
                SignInOutcome::Throttled,
                "DEBUG",
                "/login?outcome=throttled",
            ),
            (SignInOutcome::Denied, "WARN", "/login?outcome=denied"),
            (SignInOutcome::Expired, "WARN", "/login?outcome=expired"),
            (
                SignInOutcome::Unavailable,
                "WARN",
                "/login?outcome=unavailable",
            ),
            (SignInOutcome::Keys, "WARN", "/login?outcome=keys"),
            (SignInOutcome::Account, "WARN", "/login?outcome=account"),
        ];
        let logs = test_support::CapturedLogs::default();
        let writer = logs.clone();
        let subscriber = tracing_subscriber::fmt()
            .without_time()
            .with_ansi(false)
            .with_max_level(tracing::Level::DEBUG)
            .with_writer(move || writer.clone())
            .finish();
        tracing::subscriber::with_default(subscriber, || {
            for (outcome, _, _) in &cases {
                let _response = outcome.redirect();
            }
        });
        let captured = logs.text();
        let events: Vec<_> = captured
            .lines()
            .filter(|line| line.contains("OIDC sign-in did not complete"))
            .collect();
        assert_eq!(events.len(), cases.len());
        for (event, (_, level, location)) in events.iter().zip(cases) {
            assert!(event.trim_start().starts_with(level), "{event}");
            assert!(event.contains(location), "{event}");
        }
        assert!(
            !captured
                .lines()
                .any(|line| { line.contains("/login?outcome=throttled") && line.contains("WARN") })
        );
    }

    #[derive(Clone, Copy, PartialEq, Eq)]
    pub(super) enum PasswordStage {
        Verify,
        BeforeCommit,
        AfterCommit,
    }

    struct PasswordProbe {
        calls: std::sync::atomic::AtomicUsize,
        pause: Option<PasswordStage>,
        reached: tokio::sync::Semaphore,
        resume: tokio::sync::Semaphore,
    }

    impl PasswordProbe {
        fn new(pause: Option<PasswordStage>) -> std::sync::Arc<Self> {
            std::sync::Arc::new(Self {
                calls: std::sync::atomic::AtomicUsize::new(0),
                pause,
                reached: tokio::sync::Semaphore::new(0),
                resume: tokio::sync::Semaphore::new(0),
            })
        }
        async fn reached(&self) {
            tokio::time::timeout(std::time::Duration::from_secs(30), self.reached.acquire())
                .await
                .unwrap()
                .unwrap()
                .forget();
        }
        fn calls(&self) -> usize {
            self.calls.load(std::sync::atomic::Ordering::SeqCst)
        }
    }

    tokio::task_local! {
        static PASSWORD_PROBE: std::sync::Arc<PasswordProbe>;
    }

    pub(super) async fn password_checkpoint(stage: PasswordStage) {
        let Ok(probe) = PASSWORD_PROBE.try_with(std::sync::Arc::clone) else {
            return;
        };
        if stage == PasswordStage::Verify {
            probe
                .calls
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
        if probe.pause == Some(stage) {
            probe.reached.add_permits(1);
            probe.resume.acquire().await.unwrap().forget();
        }
    }

    fn password_probe_router(
        state: AppState,
        store: impl tower_sessions::SessionStore + Clone,
        probe: std::sync::Arc<PasswordProbe>,
    ) -> axum::Router {
        crate::build_router_with_session_store(state, store).layer(axum::middleware::from_fn(
            move |request: axum::extract::Request, next: axum::middleware::Next| {
                let probe = probe.clone();
                async move { PASSWORD_PROBE.scope(probe, next.run(request)).await }
            },
        ))
    }

    async fn password_state(pool: &sqlx::PgPool) -> AppState {
        let mut state = test_support::test_state();
        state.pool = test_support::db::app_pool_for(pool).await;
        state.ingestion_pool = test_support::db::ingestion_pool_for(pool).await;
        state
    }

    fn hash_counter(state: &mut AppState) -> std::sync::Arc<std::sync::atomic::AtomicUsize> {
        let count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let hook_count = count.clone();
        state.hash_limiter = crate::auth::password::HashLimiter::new(1).with_before_compute(
            std::sync::Arc::new(move || {
                hook_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            }),
        );
        count
    }

    fn hash_server(state: &AppState) -> axum_test::TestServer {
        axum_test::TestServer::new(crate::build_router_with_session_store(
            state.clone(),
            crate::auth::store::PostgresStore::new(state.pool.clone()),
        ))
    }

    fn assert_hash_busy(response: &axum_test::TestResponse, path: &str) {
        assert_eq!(response.status_code(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(
            response.headers()["content-type"],
            "application/problem+json"
        );
        assert!(!response.headers().contains_key("retry-after"));
        assert_eq!(
            response.json::<serde_json::Value>(),
            serde_json::json!({
                "type": "https://reverie.example/probs/auth-busy",
                "title": "Service Unavailable", "status": 503,
                "detail": "The server is busy. Try again in a moment.", "instance": path,
            })
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn auth_hash_census_login(pool: sqlx::PgPool) {
        let mut state = password_state(&pool).await;
        test_support::db::create_adult_with_password(
            &state.pool,
            "Known",
            "known@example.com",
            "correct password",
        )
        .await;
        let count = hash_counter(&mut state);
        let server = hash_server(&state);
        for (email, status) in [
            ("known@example.com", StatusCode::NO_CONTENT),
            ("unknown@example.com", StatusCode::UNPROCESSABLE_ENTITY),
        ] {
            count.store(0, std::sync::atomic::Ordering::SeqCst);
            let response = server
                .post("/auth/local/login")
                .json(&serde_json::json!({"email":email,"password":"correct password"}))
                .await;
            assert_eq!(response.status_code(), status);
            assert_eq!(
                count.load(std::sync::atomic::Ordering::SeqCst),
                1,
                "{email}"
            );
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn auth_hash_census_setup(pool: sqlx::PgPool) {
        let mut state = password_state(&pool).await;
        let count = hash_counter(&mut state);
        let response = hash_server(&state).post("/auth/setup").json(&serde_json::json!({"email":"setup@example.com","display_name":"Setup","password":"lithe cobalt meadow drift"})).await;
        assert_eq!(response.status_code(), StatusCode::CREATED);
        assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert!(user::admin_exists(&state.pool).await.unwrap());
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn auth_hash_census_register(pool: sqlx::PgPool) {
        let mut state = password_state(&pool).await;
        state.config.self_registration_enabled = true;
        let count = hash_counter(&mut state);
        let response = hash_server(&state).post("/auth/register").json(&serde_json::json!({"email":"register@example.com","display_name":"Registrant","password":"lithe cobalt meadow drift"})).await;
        assert_eq!(response.status_code(), StatusCode::CREATED);
        assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert!(
            user::find_by_email(&state.pool, "register@example.com")
                .await
                .unwrap()
                .is_some()
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn auth_hash_census_forgot(pool: sqlx::PgPool) {
        let mut state = password_state(&pool).await;
        let directory = tempfile::tempdir().unwrap();
        state.config.recovery_pin_dir = directory.path().to_str().unwrap().to_owned();
        let id = test_support::db::create_adult_with_password(
            &state.pool,
            "Known",
            "known@example.com",
            "correct password",
        )
        .await;
        let count = hash_counter(&mut state);
        let server = hash_server(&state);
        for email in ["known@example.com", "unknown@example.com"] {
            count.store(0, std::sync::atomic::Ordering::SeqCst);
            let response = server
                .post("/auth/forgot-password")
                .json(&serde_json::json!({"email":email}))
                .await;
            assert_eq!(response.status_code(), StatusCode::OK);
            assert_eq!(
                count.load(std::sync::atomic::Ordering::SeqCst),
                1,
                "{email}"
            );
        }
        assert!(
            crate::models::password_reset_pin::find_active_by_user(&state.pool, id)
                .await
                .unwrap()
                .is_some()
        );
        assert!(directory.path().join(format!("{id}.pin")).exists());
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn auth_hash_census_reset(pool: sqlx::PgPool) {
        let mut state = password_state(&pool).await;
        let directory = tempfile::tempdir().unwrap();
        state.config.recovery_pin_dir = directory.path().to_str().unwrap().to_owned();
        let id = test_support::db::create_adult_with_password(
            &state.pool,
            "Known",
            "known@example.com",
            "correct password",
        )
        .await;
        let phc = crate::auth::password::hash_password(b"1234567890").unwrap();
        crate::models::password_reset_pin::insert(
            &state.pool,
            id,
            &phc,
            chrono::Utc::now() + chrono::TimeDelta::minutes(15),
        )
        .await
        .unwrap();
        let count = hash_counter(&mut state);
        let server = hash_server(&state);
        for (expected, status) in [(2, StatusCode::OK), (1, StatusCode::UNPROCESSABLE_ENTITY)] {
            count.store(0, std::sync::atomic::Ordering::SeqCst);
            let response = server.post("/auth/reset-password").json(&serde_json::json!({"email":"known@example.com","pin":"1234567890","new_password":"lithe cobalt meadow drift"})).await;
            assert_eq!(response.status_code(), status);
            assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), expected);
        }
        let credential = crate::models::local_credentials::find_by_user_id(&state.pool, id)
            .await
            .unwrap()
            .unwrap();
        crate::auth::password::verify_password(
            b"lithe cobalt meadow drift",
            &credential.password_hash,
        )
        .unwrap();
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn auth_hash_census_credential_link_redemption(pool: sqlx::PgPool) {
        let mut state = password_state(&pool).await;
        let id = user::create_local(
            &state.pool,
            "census-link@example.test",
            "Holder",
            crate::models::role::Role::Adult,
            None,
        )
        .await
        .unwrap()
        .id;
        let (token, _) = crate::models::credential_link::issue(&state.pool, id)
            .await
            .unwrap();
        let count = hash_counter(&mut state);
        let server = hash_server(&state);
        let response = server
            .post("/auth/credential-links/redemptions")
            .json(&serde_json::json!({"token":token,"password":"lithe cobalt meadow drift"}))
            .await;
        assert_eq!(response.status_code(), StatusCode::NO_CONTENT);
        assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn auth_hash_busy_auth_endpoints(pool: sqlx::PgPool) {
        let mut state = password_state(&pool).await;
        state.config.self_registration_enabled = true;
        let directory = tempfile::tempdir().unwrap();
        state.config.recovery_pin_dir = directory.path().to_str().unwrap().to_owned();
        let id = test_support::db::create_adult_with_password(
            &state.pool,
            "Known",
            "known@example.com",
            "correct password",
        )
        .await;
        let credential = crate::models::local_credentials::find_by_user_id(&state.pool, id)
            .await
            .unwrap()
            .unwrap();
        let pin_hash = crate::auth::password::hash_password(b"1234567890").unwrap();
        let pin_id = crate::models::password_reset_pin::insert(
            &state.pool,
            id,
            &pin_hash,
            chrono::Utc::now() + chrono::TimeDelta::minutes(15),
        )
        .await
        .unwrap();
        let (link_token, _) = crate::models::credential_link::issue(&state.pool, id)
            .await
            .unwrap();
        let count = hash_counter(&mut state);
        state.hash_limiter = state
            .hash_limiter
            .with_wait_limit(std::time::Duration::from_millis(10));
        let _held = state.hash_limiter.acquire().await.unwrap();
        let probe = PasswordProbe::new(None);
        let server = axum_test::TestServer::new(password_probe_router(
            state.clone(),
            crate::auth::store::PostgresStore::new(state.pool.clone()),
            probe.clone(),
        ));
        for (path, body) in [
            (
                "/auth/local/login",
                serde_json::json!({"email":"source-rejected@example.com","password":"wrong"}),
            ),
            (
                "/auth/setup",
                serde_json::json!({"email":"setup@example.com","display_name":"Setup","password":"lithe cobalt meadow drift"}),
            ),
            (
                "/auth/register",
                serde_json::json!({"email":"register@example.com","display_name":"Registrant","password":"lithe cobalt meadow drift"}),
            ),
            (
                "/auth/forgot-password",
                serde_json::json!({"email":"known@example.com"}),
            ),
            (
                "/auth/reset-password",
                serde_json::json!({"email":"known@example.com","pin":"1234567890","new_password":"lithe cobalt meadow drift"}),
            ),
            (
                "/auth/credential-links/redemptions",
                serde_json::json!({"token":link_token,"password":"lithe cobalt meadow drift"}),
            ),
        ] {
            let response = server.post(path).json(&body).await;
            assert_hash_busy(&response, path);
        }
        assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 0);
        assert_eq!(probe.calls(), 0);
        assert_eq!(sqlx::query_scalar!("SELECT count(*) FROM local_login_throttle WHERE email_lower = 'source-rejected@example.com'").fetch_one(&pool).await.unwrap(), Some(0));
        assert_eq!(
            crate::models::local_credentials::find_by_user_id(&state.pool, id)
                .await
                .unwrap()
                .unwrap()
                .password_hash,
            credential.password_hash
        );
        let active = crate::models::password_reset_pin::find_active_by_user(&state.pool, id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(active.id, pin_id);
        assert_eq!(active.pin_hash, pin_hash);
        assert!(
            std::fs::read_dir(directory.path())
                .unwrap()
                .next()
                .is_none()
        );
        for email in ["setup@example.com", "register@example.com"] {
            assert!(
                user::find_by_email(&state.pool, email)
                    .await
                    .unwrap()
                    .is_none()
            );
        }
        assert!(!user::admin_exists(&state.pool).await.unwrap());
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn auth_hash_busy_login_parity(pool: sqlx::PgPool) {
        let mut state = password_state(&pool).await;
        for email in [
            "known@example.com",
            "disabled@example.com",
            "passwordless@example.com",
        ] {
            let id = test_support::db::create_adult_with_password(
                &state.pool,
                "Parity",
                email,
                "correct password",
            )
            .await;
            if email == "disabled@example.com" {
                user::disable_account(&state.pool, id).await.unwrap();
            }
            if email == "passwordless@example.com" {
                sqlx::query!("DELETE FROM local_credentials WHERE user_id = $1", id)
                    .execute(&pool)
                    .await
                    .unwrap();
            }
        }
        let count = hash_counter(&mut state);
        state.hash_limiter = state
            .hash_limiter
            .with_wait_limit(std::time::Duration::from_millis(10));
        let _held = state.hash_limiter.acquire().await.unwrap();
        let probe = PasswordProbe::new(None);
        let server = axum_test::TestServer::new(password_probe_router(
            state.clone(),
            crate::auth::store::PostgresStore::new(state.pool.clone()),
            probe.clone(),
        ));
        let mut previous = None;
        for email in [
            "known@example.com",
            "unknown@example.com",
            "disabled@example.com",
            "passwordless@example.com",
        ] {
            let response = server
                .post("/auth/local/login")
                .json(&serde_json::json!({"email":email,"password":"wrong"}))
                .await;
            assert_hash_busy(&response, "/auth/local/login");
            if let Some(previous) = &previous {
                assert_eq!(&response.text(), previous);
            }
            previous = Some(response.text());
        }
        assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 0);
        assert_eq!(probe.calls(), 0);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn auth_hash_login_precedence(pool: sqlx::PgPool) {
        use crate::models::login_throttle::tests::{seed, snapshot};
        use tower_sessions::SessionStore;
        let mut state = password_state(&pool).await;
        test_support::db::create_adult_with_password(
            &state.pool,
            "Known",
            "known@example.com",
            "correct password",
        )
        .await;
        let count = hash_counter(&mut state);
        state.hash_limiter = state
            .hash_limiter
            .with_wait_limit(std::time::Duration::from_millis(10));
        let probe = PasswordProbe::new(None);
        let store = crate::auth::store::PostgresStore::new(state.pool.clone());
        let mut server = axum_test::TestServer::new(password_probe_router(
            state.clone(),
            store.clone(),
            probe.clone(),
        ));
        server.save_cookies();
        let login = server
            .post("/auth/local/login")
            .json(&serde_json::json!({"email":"known@example.com","password":"correct password"}))
            .await;
        assert_eq!(login.status_code(), StatusCode::NO_CONTENT);
        let id = login.cookie("id").value().parse().unwrap();
        let record = store.load(&id).await.unwrap().unwrap();
        let _held = state.hash_limiter.acquire().await.unwrap();
        let signed_in = server
            .post("/auth/local/login")
            .add_header("x-csrf-token", record.data["csrf_token"].as_str().unwrap())
            .json(&serde_json::json!({"email":"known@example.com","password":"wrong"}))
            .await;
        assert_eq!(signed_in.status_code(), StatusCode::CONFLICT);
        state.config.trusted_client_ip_header = Some("x-test-source".into());
        state.login_limiter =
            crate::auth::rate_limit::build_login_limiter(std::num::NonZeroU32::new(1).unwrap());
        assert!(
            state
                .login_limiter
                .check_key(&"192.0.2.1".parse().unwrap())
                .is_ok()
        );
        let server = axum_test::TestServer::new(password_probe_router(
            state.clone(),
            crate::auth::store::PostgresStore::new(state.pool.clone()),
            probe.clone(),
        ));
        let exhausted = server
            .post("/auth/local/login")
            .add_header("x-test-source", "192.0.2.1")
            .json(&serde_json::json!({"email":"unknown@example.com","password":"wrong"}))
            .await;
        assert_eq!(exhausted.status_code(), StatusCode::TOO_MANY_REQUESTS);
        seed(
            &pool,
            "blocked@example.com",
            &[chrono::Utc::now(); 10],
            Some(chrono::Utc::now() + chrono::TimeDelta::minutes(15)),
        )
        .await;
        let before = snapshot(&pool, "blocked@example.com").await;
        let blocked = server
            .post("/auth/local/login")
            .add_header("x-test-source", "192.0.2.2")
            .json(&serde_json::json!({"email":"blocked@example.com","password":"wrong"}))
            .await;
        assert_hash_busy(&blocked, "/auth/local/login");
        assert_eq!(snapshot(&pool, "blocked@example.com").await, before);
        assert_eq!(probe.calls(), 1);
        assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    struct HashPause {
        released: std::sync::Mutex<bool>,
        resume: std::sync::Condvar,
        reached: tokio::sync::Notify,
    }

    impl HashPause {
        fn install(state: &mut AppState) -> std::sync::Arc<Self> {
            let pause = std::sync::Arc::new(Self {
                released: std::sync::Mutex::new(false),
                resume: std::sync::Condvar::new(),
                reached: tokio::sync::Notify::new(),
            });
            let hook_pause = pause.clone();
            state.hash_limiter = crate::auth::password::HashLimiter::new(1)
                .with_wait_limit(std::time::Duration::from_secs(5))
                .with_before_compute(std::sync::Arc::new(move || {
                    hook_pause.reached.notify_one();
                    let mut released = hook_pause.released.lock().unwrap();
                    while !*released {
                        released = hook_pause.resume.wait(released).unwrap();
                    }
                    drop(released);
                }));
            pause
        }
        fn release(&self) {
            *self.released.lock().unwrap() = true;
            self.resume.notify_all();
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn auth_hash_login_pool_progress(pool: sqlx::PgPool) {
        let mut state = password_state(&pool).await;
        state.pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(2)
            .acquire_timeout(std::time::Duration::from_secs(2))
            .connect_with((*state.pool.connect_options()).clone())
            .await
            .unwrap();
        let pause = HashPause::install(&mut state);
        let probe = PasswordProbe::new(None);
        let server = std::sync::Arc::new(axum_test::TestServer::new(password_probe_router(
            state.clone(),
            crate::auth::store::PostgresStore::new(state.pool.clone()),
            probe.clone(),
        )));
        let first_server = server.clone();
        let first = tokio::spawn(async move {
            first_server
                .post("/auth/local/login")
                .json(&serde_json::json!({"email":"first@example.com","password":"wrong"}))
                .await
        });
        pause.reached.notified().await;
        let mut waiting = Vec::new();
        for number in 0..4 {
            let server = server.clone();
            waiting.push(Box::pin(async move { server.post("/auth/local/login").json(&serde_json::json!({"email":format!("waiting{number}@example.com"),"password":"wrong"})).await }));
        }
        for request in &mut waiting {
            std::future::poll_fn(|cx| {
                assert!(std::future::Future::poll(request.as_mut(), cx).is_pending());
                std::task::Poll::Ready(())
            })
            .await;
        }
        let unrelated = server.get("/health/ready").await;
        pause.release();
        assert_eq!(
            unrelated.status_code(),
            StatusCode::OK,
            "pool-backed GET /health/ready must progress"
        );
        assert_eq!(
            first.await.unwrap().status_code(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        for request in waiting {
            assert_eq!(
                request.await.status_code(),
                StatusCode::UNPROCESSABLE_ENTITY
            );
        }
        assert_eq!(probe.calls(), 5);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn auth_hash_login_cancel_during_hash(pool: sqlx::PgPool) {
        use crate::models::login_throttle::tests::{seed, snapshot};
        let mut state = password_state(&pool).await;
        seed(&pool, "cancel@example.com", &[chrono::Utc::now()], None).await;
        let before = snapshot(&pool, "cancel@example.com").await;
        let pause = HashPause::install(&mut state);
        let server = hash_server(&state);
        let task = tokio::spawn(async move {
            server
                .post("/auth/local/login")
                .json(&serde_json::json!({"email":"cancel@example.com","password":"wrong"}))
                .await
        });
        pause.reached.notified().await;
        task.abort();
        let cancelled = task.await.unwrap_err().is_cancelled();
        let unavailable = tokio::time::timeout(
            std::time::Duration::from_millis(10),
            state.hash_limiter.acquire(),
        )
        .await
        .is_err();
        pause.release();
        drop(state.hash_limiter.acquire().await.unwrap());
        let after = snapshot(&pool, "cancel@example.com").await;
        let sessions =
            sqlx::query_scalar!(r#"SELECT count(*) AS "count!" FROM tower_sessions.session"#)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(cancelled);
        assert!(unavailable);
        assert_eq!(after, before);
        assert_eq!(sessions, 0);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn password_admission_route_parity_blocked_correct_and_no_verification(
        pool: sqlx::PgPool,
    ) {
        use crate::models::login_throttle::tests::{seed, snapshot};
        let state = password_state(&pool).await;
        let disabled = test_support::db::create_adult_with_password(
            &state.pool,
            "Disabled",
            "disabled@example.com",
            "correct password",
        )
        .await;
        user::disable_account(&state.pool, disabled).await.unwrap();
        test_support::db::create_adult_with_password(
            &state.pool,
            "Known",
            "known@example.com",
            "correct password",
        )
        .await;
        let passwordless = test_support::db::create_adult_with_password(
            &state.pool,
            "Passwordless",
            "passwordless@example.com",
            "correct password",
        )
        .await;
        sqlx::query!(
            "DELETE FROM local_credentials WHERE user_id = $1",
            passwordless
        )
        .execute(&pool)
        .await
        .unwrap();
        let probe = PasswordProbe::new(None);
        let store = crate::auth::store::PostgresStore::new(state.pool.clone());
        let server = axum_test::TestServer::new(password_probe_router(state, store, probe.clone()));
        let mut body = None;
        for email in [
            "known@example.com",
            "unknown@example.com",
            "disabled@example.com",
            "passwordless@example.com",
        ] {
            let password = if email == "known@example.com" {
                "wrong"
            } else {
                "correct password"
            };
            let response = server
                .post("/auth/local/login")
                .json(&serde_json::json!({"email":email,"password":password}))
                .await;
            assert_eq!(response.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
            if let Some(body) = &body {
                assert_eq!(&response.text(), body);
            } else {
                body = Some(response.text());
            }
        }
        assert_eq!(probe.calls(), 4);
        let deadline = chrono::Utc::now() + chrono::TimeDelta::minutes(15);
        let history = vec![chrono::Utc::now(); 10];
        let mut body = None;
        for email in [
            "known@example.com",
            "unknown@example.com",
            "disabled@example.com",
            "passwordless@example.com",
        ] {
            seed(&pool, email, &history, Some(deadline)).await;
            let before = snapshot(&pool, email).await;
            for password in ["wrong", "correct password"] {
                let response = server
                    .post("/auth/local/login")
                    .json(&serde_json::json!({"email":email,"password":password}))
                    .await;
                assert_eq!(response.status_code(), StatusCode::TOO_MANY_REQUESTS);
                assert!(
                    response
                        .cookies()
                        .iter()
                        .all(|cookie| cookie.name() != "id")
                );
                if let Some(body) = &body {
                    assert_eq!(&response.text(), body);
                } else {
                    body = Some(response.text());
                }
            }
            assert_eq!(snapshot(&pool, email).await, before);
        }
        assert_eq!(probe.calls(), 4);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn password_admission_route_two_pools_ninth_failure_race(pool: sqlx::PgPool) {
        use crate::models::login_throttle::tests::{seed, snapshot};
        seed(&pool, "race@example.com", &[chrono::Utc::now(); 9], None).await;
        let probe = PasswordProbe::new(Some(PasswordStage::Verify));
        let mut first = password_state(&pool).await;
        let mut second = password_state(&pool).await;
        first.config.trusted_client_ip_header = Some("x-test-source".into());
        second.config.trusted_client_ip_header = Some("x-test-source".into());
        let server = std::sync::Arc::new(axum_test::TestServer::new(password_probe_router(
            first.clone(),
            crate::auth::store::PostgresStore::new(first.pool.clone()),
            probe.clone(),
        )));
        let other = axum_test::TestServer::new(password_probe_router(
            second.clone(),
            crate::auth::store::PostgresStore::new(second.pool.clone()),
            probe.clone(),
        ));
        let request = tokio::spawn(async move {
            server
                .post("/auth/local/login")
                .add_header("x-test-source", "192.0.2.1")
                .json(&serde_json::json!({"email":"race@example.com","password":"wrong"}))
                .await
        });
        probe.reached().await;
        let waiting = tokio::spawn(async move {
            other
                .post("/auth/local/login")
                .add_header("x-test-source", "192.0.2.2")
                .json(&serde_json::json!({"email":"RACE@example.com","password":"wrong"}))
                .await
        });
        test_support::wait_for_blocked_query(&pool, "%INSERT INTO local_login_throttle%", 1).await;
        probe.resume.add_permits(1);
        assert_eq!(
            request.await.unwrap().status_code(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(
            waiting.await.unwrap().status_code(),
            StatusCode::TOO_MANY_REQUESTS
        );
        assert_eq!(probe.calls(), 1);
        assert_eq!(snapshot(&pool, "race@example.com").await.0.len(), 10);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn password_admission_route_cancellation_and_late_result(pool: sqlx::PgPool) {
        use crate::models::login_throttle::tests::{seed, snapshot};
        for stage in [PasswordStage::BeforeCommit, PasswordStage::AfterCommit] {
            seed(&pool, "cancel@example.com", &[], None).await;
            let state = password_state(&pool).await;
            let probe = PasswordProbe::new(Some(stage));
            let server = axum_test::TestServer::new(password_probe_router(
                state.clone(),
                crate::auth::store::PostgresStore::new(state.pool.clone()),
                probe.clone(),
            ));
            let request = tokio::spawn(async move {
                server
                    .post("/auth/local/login")
                    .json(&serde_json::json!({"email":"cancel@example.com","password":"wrong"}))
                    .await
            });
            probe.reached().await;
            request.abort();
            assert!(request.await.unwrap_err().is_cancelled());
            let mut tx = pool.begin().await.unwrap();
            sqlx::query!("SELECT email_lower FROM local_login_throttle WHERE email_lower = 'cancel@example.com' FOR UPDATE").fetch_one(&mut *tx).await.unwrap();
            tx.commit().await.unwrap();
            assert_eq!(
                snapshot(&pool, "cancel@example.com").await.0.len(),
                usize::from(stage == PasswordStage::AfterCommit)
            );
            crate::models::login_throttle::reset(&pool, "cancel@example.com")
                .await
                .unwrap();
            probe.resume.add_permits(1);
            tokio::task::yield_now().await;
            assert_eq!(snapshot(&pool, "cancel@example.com").await.0.len(), 0);
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn password_admission_route_retains_verified_version_snapshot(pool: sqlx::PgPool) {
        let state = password_state(&pool).await;
        let id = test_support::db::create_adult_with_password(
            &state.pool,
            "Version",
            "version@example.com",
            "correct password",
        )
        .await;
        let probe = PasswordProbe::new(Some(PasswordStage::BeforeCommit));
        let mut server = axum_test::TestServer::new(password_probe_router(
            state.clone(),
            crate::auth::store::PostgresStore::new(state.pool.clone()),
            probe.clone(),
        ));
        server.save_cookies();
        let server = std::sync::Arc::new(server);
        let request_server = server.clone();
        let request = tokio::spawn(async move {
            request_server.post("/auth/local/login").json(&serde_json::json!({"email":"version@example.com","password":"correct password"})).await
        });
        probe.reached().await;
        sqlx::query!(
            "UPDATE users SET session_version = session_version + 1 WHERE id = $1",
            id
        )
        .execute(&pool)
        .await
        .unwrap();
        probe.resume.add_permits(1);
        assert_eq!(request.await.unwrap().status_code(), StatusCode::NO_CONTENT);
        assert_eq!(
            server.get("/auth/me").await.status_code(),
            StatusCode::UNAUTHORIZED
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn password_admission_success_clears_history_and_preserves_existing_session(
        pool: sqlx::PgPool,
    ) {
        use crate::models::login_throttle::tests::{seed, snapshot};
        let state = password_state(&pool).await;
        test_support::db::create_adult_with_password(
            &state.pool,
            "Success",
            "success@example.com",
            "correct password",
        )
        .await;
        seed(&pool, "success@example.com", &[chrono::Utc::now(); 9], None).await;
        let probe = PasswordProbe::new(None);
        let mut server = axum_test::TestServer::new(password_probe_router(
            state.clone(),
            crate::auth::store::PostgresStore::new(state.pool.clone()),
            probe.clone(),
        ));
        server.save_cookies();
        let response = server
            .post("/auth/local/login")
            .json(&serde_json::json!({"email":"success@example.com","password":"correct password"}))
            .await;
        assert_eq!(response.status_code(), StatusCode::NO_CONTENT);
        assert_eq!(snapshot(&pool, "success@example.com").await, (vec![], None));
        assert_eq!(probe.calls(), 1);
        seed(
            &pool,
            "success@example.com",
            &[chrono::Utc::now(); 10],
            Some(chrono::Utc::now() + chrono::TimeDelta::minutes(15)),
        )
        .await;
        assert_eq!(server.get("/auth/me").await.status_code(), StatusCode::OK);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn password_admission_cohort_loser_preserves_history(pool: sqlx::PgPool) {
        use crate::models::login_throttle::tests::{seed, snapshot};
        let fixture = CallbackFixture::start(&pool).await;
        test_support::db::create_adult_with_password(
            &fixture.app_pool,
            "Loser",
            "loser@example.com",
            "correct password",
        )
        .await;
        seed(&pool, "loser@example.com", &[chrono::Utc::now()], None).await;
        let before = snapshot(&pool, "loser@example.com").await;
        let probe = PasswordProbe::new(Some(PasswordStage::Verify));
        let server = axum_test::TestServer::new(password_probe_router(
            fixture.state.clone(),
            fixture.store.clone(),
            probe.clone(),
        ));
        let cookie = axum_extra::extract::cookie::Cookie::new("id", fixture.session_id.to_string());
        let request = tokio::spawn(async move {
            server
                .post("/auth/local/login")
                .add_cookie(cookie)
                .json(
                    &serde_json::json!({"email":"loser@example.com","password":"correct password"}),
                )
                .await
        });
        probe.reached().await;
        assert!(
            sign_in::claim_win(&fixture.app_pool, &fixture.session_id.to_string())
                .await
                .unwrap()
        );
        probe.resume.add_permits(1);
        assert_eq!(request.await.unwrap().status_code(), StatusCode::CONFLICT);
        assert_eq!(snapshot(&pool, "loser@example.com").await, before);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn password_admission_session_failure_after_commit_clears_history(pool: sqlx::PgPool) {
        use crate::models::login_throttle::tests::{seed, snapshot};
        use tower_sessions::SessionStore;
        let fixture = CallbackFixture::start(&pool).await;
        test_support::db::create_adult_with_password(
            &fixture.app_pool,
            "Storage",
            "storage@example.com",
            "correct password",
        )
        .await;
        seed(&pool, "storage@example.com", &[chrono::Utc::now()], None).await;
        let store = OutcomeStore {
            inner: fixture.store.clone(),
            operation: StoreOperation::Delete,
            failures: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(1)),
        };
        let server = axum_test::TestServer::new(crate::build_router_with_session_store(
            fixture.state.clone(),
            store,
        ));
        let response = server
            .post("/auth/local/login")
            .add_cookie(axum_extra::extract::cookie::Cookie::new(
                "id",
                fixture.session_id.to_string(),
            ))
            .json(&serde_json::json!({"email":"storage@example.com","password":"correct password"}))
            .await;
        assert_eq!(response.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(snapshot(&pool, "storage@example.com").await, (vec![], None));
        let retained = fixture
            .store
            .load(&fixture.session_id)
            .await
            .unwrap()
            .unwrap();
        assert!(!retained.data.contains_key("user_id"));
        assert_eq!(
            sign_in::join_pending(&fixture.app_pool, &fixture.session_id.to_string())
                .await
                .unwrap(),
            sign_in::CohortState::Won
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn password_admission_disconnect_with_running_handler_commits_result(pool: sqlx::PgPool) {
        use crate::models::login_throttle::tests::{seed, snapshot};
        use tokio::io::AsyncWriteExt;
        seed(&pool, "disconnect@example.com", &[], None).await;
        let state = password_state(&pool).await;
        let probe = PasswordProbe::new(Some(PasswordStage::BeforeCommit));
        let app = password_probe_router(
            state.clone(),
            crate::auth::store::PostgresStore::new(state.pool.clone()),
            probe.clone(),
        )
        .layer(axum::middleware::from_fn(
            |request: axum::extract::Request, next: axum::middleware::Next| async move {
                tokio::spawn(next.run(request)).await.unwrap()
            },
        ));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let serving = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let mut stream = tokio::net::TcpStream::connect(address).await.unwrap();
        let body = r#"{"email":"disconnect@example.com","password":"wrong"}"#;
        stream.write_all(format!("POST /auth/local/login HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
        probe.reached().await;
        stream.shutdown().await.unwrap();
        drop(stream);
        probe.resume.add_permits(1);
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            loop {
                if snapshot(&pool, "disconnect@example.com").await.0.len() == 1 {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("a running handler commits despite client disconnect");
        assert_eq!(probe.calls(), 1);
        serving.abort();
        assert!(serving.await.unwrap_err().is_cancelled());
    }

    fn assert_retry(response: &axum_test::TestResponse, upper: u64) -> u64 {
        assert_eq!(response.status_code(), StatusCode::TOO_MANY_REQUESTS);
        let seconds = response
            .header("retry-after")
            .to_str()
            .unwrap()
            .parse::<u64>()
            .unwrap();
        assert!((1..=upper).contains(&seconds));
        seconds
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn auth_allowance_independent_groups_and_rejected_source_creates_no_state(
        pool: sqlx::PgPool,
    ) {
        let mut state = password_state(&pool).await;
        state.config.trusted_client_ip_header = Some("x-test-source".into());
        let quota = std::num::NonZeroU32::new(1).unwrap();
        state.login_limiter = crate::auth::rate_limit::build_login_limiter(quota);
        state.recovery_limiter = crate::auth::rate_limit::build_login_limiter(quota);
        state.oidc_limiter = crate::auth::rate_limit::build_login_limiter(quota);
        let probe = PasswordProbe::new(None);
        let server = axum_test::TestServer::new(password_probe_router(
            state.clone(),
            crate::auth::store::PostgresStore::new(state.pool.clone()),
            probe.clone(),
        ));
        let password = server
            .post("/auth/local/login")
            .add_header("x-test-source", "192.0.2.5")
            .json(&serde_json::json!({"email":"first@example.com","password":"wrong"}))
            .await;
        assert_eq!(password.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
        let rejected = server
            .post("/auth/local/login")
            .add_header("x-test-source", "192.0.2.5")
            .json(&serde_json::json!({"email":"source-rejected@example.com","password":"wrong"}))
            .await;
        assert_retry(&rejected, 60);
        assert_eq!(probe.calls(), 1);
        assert_eq!(sqlx::query_scalar!("SELECT count(*) FROM local_login_throttle WHERE email_lower = 'source-rejected@example.com'").fetch_one(&pool).await.unwrap(), Some(0));
        let recovery = server
            .post("/auth/forgot-password")
            .add_header("x-test-source", "192.0.2.5")
            .json(&serde_json::json!({"email":"unknown@example.com"}))
            .await;
        assert_eq!(recovery.status_code(), StatusCode::OK);
        let rejected = server.post("/auth/reset-password").add_header("x-test-source", "192.0.2.5").json(&serde_json::json!({"email":"unknown@example.com","pin":"wrong","new_password":"a perfectly good new password"})).await;
        assert_retry(&rejected, 60);
        let oidc = server
            .get("/auth/oidc/login")
            .add_header("x-test-source", "192.0.2.5")
            .await;
        assert_eq!(oidc.status_code(), StatusCode::TEMPORARY_REDIRECT);
        let attempts = sqlx::query_scalar!("SELECT count(*) FROM oidc_sign_in_attempts")
            .fetch_one(&pool)
            .await
            .unwrap();
        let rejected = server
            .get("/auth/oidc/login")
            .add_header("x-test-source", "192.0.2.5")
            .await;
        assert_outcome(&rejected, "/login?outcome=throttled");
        assert_eq!(
            sqlx::query_scalar!("SELECT count(*) FROM oidc_sign_in_attempts")
                .fetch_one(&pool)
                .await
                .unwrap(),
            attempts
        );
        for (source, group) in [
            ("192.0.2.6", &state.recovery_limiter),
            ("192.0.2.7", &state.oidc_limiter),
        ] {
            let ip = source.parse().unwrap();
            assert!(group.check_key(&ip).is_ok());
            assert!(group.check_key(&ip).is_err());
            let password = server
                .post("/auth/local/login")
                .add_header("x-test-source", source)
                .json(&serde_json::json!({"email":"separate@example.com","password":"wrong"}))
                .await;
            assert_eq!(password.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn auth_allowance_account_retry_decreases_without_extending_block(pool: sqlx::PgPool) {
        use crate::models::login_throttle::tests::{seed, snapshot};
        let state = password_state(&pool).await;
        let probe = PasswordProbe::new(None);
        let server = axum_test::TestServer::new(password_probe_router(
            state.clone(),
            crate::auth::store::PostgresStore::new(state.pool.clone()),
            probe.clone(),
        ));
        seed(
            &pool,
            "retry@example.com",
            &[chrono::Utc::now(); 10],
            Some(chrono::Utc::now() + chrono::TimeDelta::seconds(900)),
        )
        .await;
        let before = snapshot(&pool, "retry@example.com").await;
        let first = server
            .post("/auth/local/login")
            .json(&serde_json::json!({"email":"retry@example.com","password":"wrong"}))
            .await;
        let first_retry = assert_retry(&first, 900);
        assert_eq!(snapshot(&pool, "retry@example.com").await, before);
        tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
        let later = server
            .post("/auth/local/login")
            .json(&serde_json::json!({"email":"retry@example.com","password":"correct password"}))
            .await;
        assert!(assert_retry(&later, 900) < first_retry);
        assert_eq!(first.text(), later.text());
        assert_eq!(snapshot(&pool, "retry@example.com").await, before);
        assert_eq!(probe.calls(), 0);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn login_redirects_to_oidc_provider(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
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
    async fn callback_malformed_query_redirects_without_secret_leak() {
        let server = test_support::test_server();
        let response = server
            .get("/auth/callback?code=secret-a&code=secret-b")
            .await;
        assert_outcome(&response, "/login?outcome=expired");
        assert!(!response.text().contains("secret-a"));
        assert!(!response.text().contains("secret-b"));
    }

    fn assert_outcome(response: &axum_test::TestResponse, location: &str) {
        assert_eq!(response.status_code(), StatusCode::SEE_OTHER);
        assert_eq!(response.header("location"), location);
        assert_eq!(response.text(), "");
        assert!(!response.headers().contains_key("www-authenticate"));
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

    #[sqlx::test(migrations = "./migrations")]
    async fn session_cookie_carries_httponly_lax_and_no_secure_by_default(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
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

    #[sqlx::test(migrations = "./migrations")]
    async fn session_cookie_secure_tracks_behind_https(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
        let mut config = test_support::test_config();
        config.security.behind_https = true;
        let server = test_support::db::server_with_config(&app_pool, &ingestion_pool, config);
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
    async fn callback_redirects_to_expired_without_session_state() {
        let server = test_support::test_server();
        // Callback without a prior login flow (no CSRF/PKCE in session) should fail
        let response = server
            .get("/auth/callback")
            .add_query_param("code", "fake-code")
            .add_query_param("state", "fake-state")
            .await;
        assert_outcome(&response, "/login?outcome=expired");
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

    #[sqlx::test(migrations = "./migrations")]
    async fn callback_succeeds_first_user_not_promoted(pool: sqlx::PgPool) {
        use crate::models::role::Role;
        use crate::state::AppState;
        use crate::test_support::oidc_mock::MockOidcProvider;

        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;

        // Mock IdP: spins up wiremock + signs a key + serves /jwks.
        // client_id matches `test_config().oidc_client_id` (empty string).
        let mock = MockOidcProvider::start("").await;
        let oidc = Some(std::sync::Arc::new(
            mock.runtime("http://localhost:3000/auth/callback"),
        ));

        // Shared persisted session store exercises rotation and cascading deletes.
        let store = crate::auth::store::PostgresStore::new(app_pool.clone());
        let (config, library_files) = crate::test_support::test_storage_config(
            None,
            crate::models::storage_library::LibraryId::from_uuid(uuid::Uuid::new_v4()),
        );
        let state = AppState {
            pool: app_pool.clone(),
            ingestion_pool,
            ingestion: crate::services::ingestion::coordinator_channel().0,
            library_files,
            hash_limiter: crate::auth::password::HashLimiter::new(
                usize::try_from(config.auth_hash_concurrency).unwrap(),
            ),
            config,
            oidc,
            login_limiter: test_support::test_login_limiter(),
            recovery_limiter: test_support::test_login_limiter(),
            oidc_limiter: test_support::test_login_limiter(),
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
        // the authorization request parameters from the redirect.
        let session_cookie_value = login_resp.cookie("id").value().to_string();
        let csrf: String = authorization_parameter(&login_resp, "state");
        let nonce: String = authorization_parameter(&login_resp, "nonce");

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
        // docs/adr/0011-json-api-conventions-for-the-browser-facing-rest-surface.md.
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
        // NOT rotate per request; otherwise the frontend's cached token
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

    fn authorization_parameter(response: &axum_test::TestResponse, name: &str) -> String {
        url::Url::parse(response.header("location").to_str().expect("location"))
            .expect("authorization URL")
            .query_pairs()
            .find(|(key, _)| key == name)
            .expect("authorization parameter")
            .1
            .into_owned()
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn local_login_loses_to_a_recent_winner(pool: sqlx::PgPool) {
        use tower_sessions::SessionStore;
        let fixture = CallbackFixture::start(&pool).await;
        test_support::db::create_adult_with_password(
            &fixture.app_pool,
            "Local",
            "local@example.com",
            "a perfectly good password",
        )
        .await;
        assert!(
            sign_in::claim_win(&fixture.app_pool, &fixture.session_id.to_string())
                .await
                .unwrap()
        );
        let before = fixture
            .store
            .load(&fixture.session_id)
            .await
            .unwrap()
            .unwrap()
            .data;
        let response = fixture.server.post("/auth/local/login").json(&serde_json::json!({"email":"local@example.com", "password":"a perfectly good password"})).await;
        let problem = test_support::assert_problem(
            &response,
            crate::error::problems::SIGN_IN_INACTIVE,
            StatusCode::CONFLICT,
        );
        assert_eq!(problem["title"], "Conflict");
        assert_eq!(
            problem["detail"],
            "This sign-in is no longer active. Reload the page and sign in again."
        );
        let after = fixture
            .store
            .load(&fixture.session_id)
            .await
            .unwrap()
            .unwrap()
            .data;
        assert_eq!(after, before);
        assert!(!after.contains_key("user_id"));
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn local_login_loses_to_a_concurrent_winner(pool: sqlx::PgPool) {
        use tower_sessions::SessionStore;
        let fixture = CallbackFixture::start(&pool).await;
        test_support::db::create_adult_with_password(
            &fixture.app_pool,
            "Local",
            "local@example.com",
            "a perfectly good password",
        )
        .await;
        let mut tx = pool.begin().await.unwrap();
        sqlx::query!(
            "UPDATE sign_in_cohorts SET won_at = now() WHERE session_id = $1 AND won_at IS NULL",
            fixture.session_id.to_string()
        )
        .execute(&mut *tx)
        .await
        .unwrap();
        let server = std::sync::Arc::new(fixture.server);
        let request_server = server.clone();
        let request = tokio::spawn(async move {
            request_server.post("/auth/local/login").json(&serde_json::json!({"email":"local@example.com", "password":"a perfectly good password"})).await
        });
        test_support::wait_for_blocked_query(&pool, "%sign_in_cohorts%", 1).await;
        tx.commit().await.unwrap();
        let response = request.await.unwrap();
        test_support::assert_problem(
            &response,
            crate::error::problems::SIGN_IN_INACTIVE,
            StatusCode::CONFLICT,
        );
        assert!(
            !fixture
                .store
                .load(&fixture.session_id)
                .await
                .unwrap()
                .unwrap()
                .data
                .contains_key("user_id")
        );
        assert_eq!(
            server.get("/auth/me").await.status_code(),
            StatusCode::UNAUTHORIZED
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn local_login_when_signed_in_is_refused(pool: sqlx::PgPool) {
        use tower_sessions::SessionStore;
        let fixture = CallbackFixture::start(&pool).await;
        let (_, id) = sign_in_fixture_locally(&fixture).await;
        let before = fixture.store.load(&id).await.unwrap().unwrap().data;
        let response = fixture.server.post("/auth/local/login").add_header("x-csrf-token", before["csrf_token"].as_str().unwrap()).json(&serde_json::json!({"email":"local@example.com", "password":"a perfectly good password"})).await;
        test_support::assert_problem(
            &response,
            crate::error::problems::SIGN_IN_INACTIVE,
            StatusCode::CONFLICT,
        );
        assert_eq!(fixture.store.load(&id).await.unwrap().unwrap().data, before);
        assert_eq!(response.cookie("id").value(), id.to_string());
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn local_login_without_session_creates_no_cohort(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
        let response = server
            .post("/auth/local/login")
            .json(&serde_json::json!({"email":"unknown@example.com", "password":"wrong"}))
            .await;
        assert_eq!(response.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(
            sqlx::query_scalar!("SELECT count(*) FROM sign_in_cohorts")
                .fetch_one(&pool)
                .await
                .unwrap(),
            Some(0)
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn local_login_with_session_keeps_failure_parity(pool: sqlx::PgPool) {
        let fixture = CallbackFixture::start(&pool).await;
        test_support::db::create_adult_with_password(
            &fixture.app_pool,
            "Local",
            "local@example.com",
            "a perfectly good password",
        )
        .await;
        let unknown = fixture
            .server
            .post("/auth/local/login")
            .json(&serde_json::json!({"email":"unknown@example.com", "password":"wrong"}))
            .await;
        let wrong = fixture
            .server
            .post("/auth/local/login")
            .json(&serde_json::json!({"email":"local@example.com", "password":"wrong"}))
            .await;
        assert_eq!(unknown.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(unknown.status_code(), wrong.status_code());
        assert_eq!(unknown.text(), wrong.text());
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn local_login_after_forced_logout_succeeds(pool: sqlx::PgPool) {
        use tower_sessions::SessionStore;
        let fixture = CallbackFixture::start(&pool).await;
        let (_, old_id) = sign_in_fixture_locally(&fixture).await;
        let me: serde_json::Value = fixture.server.get("/auth/me").await.json();
        let changed = fixture.server.post("/api/v1/account/password")
            .add_header("x-csrf-token", me["csrf_token"].as_str().unwrap())
            .json(&serde_json::json!({"current_password":"a perfectly good password", "new_password":"correct-horse-battery-staple-7!"})).await;
        assert_eq!(changed.status_code(), StatusCode::OK);
        let response = fixture.server.post("/auth/local/login").add_header("x-csrf-token", me["csrf_token"].as_str().unwrap()).json(&serde_json::json!({"email":"local@example.com", "password":"correct-horse-battery-staple-7!"})).await;
        assert_eq!(response.status_code(), StatusCode::NO_CONTENT);
        assert!(fixture.store.load(&old_id).await.unwrap().is_none());
        assert_eq!(
            fixture.server.get("/auth/me").await.status_code(),
            StatusCode::OK
        );
    }

    async fn sign_in_fixture_locally(
        fixture: &CallbackFixture,
    ) -> (uuid::Uuid, tower_sessions::session::Id) {
        let user_id = test_support::db::create_adult_with_password(
            &fixture.app_pool,
            "Local",
            "local@example.com",
            "a perfectly good password",
        )
        .await;
        let response = fixture.server.post("/auth/local/login").json(&serde_json::json!({"email":"local@example.com", "password":"a perfectly good password"})).await;
        assert_eq!(response.status_code(), StatusCode::NO_CONTENT);
        (user_id, response.cookie("id").value().parse().unwrap())
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn oidc_start_persists_one_attempt_per_session(pool: sqlx::PgPool) {
        let fixture = CallbackFixture::start(&pool).await;
        let second = fixture.server.get("/auth/oidc/login").await;
        assert_eq!(second.status_code(), StatusCode::TEMPORARY_REDIRECT);
        let state = authorization_parameter(&second, "state");
        assert_ne!(state, fixture.csrf);
        assert_eq!(
            sqlx::query_scalar!("SELECT count(*) FROM oidc_sign_in_attempts")
                .fetch_one(&pool)
                .await
                .unwrap(),
            Some(1)
        );
        let old = fixture
            .server
            .get("/auth/callback")
            .add_query_param("code", "code")
            .add_query_param("state", &fixture.csrf)
            .await;
        assert_outcome(&old, "/login?outcome=expired");
        fixture.mock.assert_no_requests().await;
        fixture
            .mock
            .mount_token_endpoint(
                "second",
                Some("second@example.com"),
                Some("Second"),
                &authorization_parameter(&second, "nonce"),
            )
            .await;
        let response = fixture
            .server
            .get("/auth/callback")
            .add_query_param("code", "code")
            .add_query_param("state", &state)
            .await;
        assert_eq!(response.status_code(), StatusCode::TEMPORARY_REDIRECT);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn oidc_start_over_source_limit_redirects_to_throttled_outcome(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
        let server = test_support::db::server_with_source_rate_limit(&app_pool, &ingestion_pool, 1);
        let first = server
            .get("/auth/oidc/login")
            .add_header("x-forwarded-for", "198.51.100.24")
            .await;
        assert_eq!(first.status_code(), StatusCode::TEMPORARY_REDIRECT);
        let second = server
            .get("/auth/oidc/login")
            .add_header("x-forwarded-for", "198.51.100.24")
            .await;
        assert_eq!(second.status_code(), StatusCode::SEE_OTHER);
        assert_eq!(second.header("location"), "/login?outcome=throttled");
        assert_eq!(
            sqlx::query_scalar!("SELECT count(*) FROM oidc_sign_in_attempts")
                .fetch_one(&pool)
                .await
                .unwrap(),
            Some(1)
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn oidc_start_when_signed_in_redirects_home(pool: sqlx::PgPool) {
        let fixture = CallbackFixture::start(&pool).await;
        sign_in_fixture_locally(&fixture).await;
        let response = fixture.server.get("/auth/oidc/login").await;
        assert_eq!(response.status_code(), StatusCode::SEE_OTHER);
        assert_eq!(response.header("location"), "/");
        assert_eq!(
            sqlx::query_scalar!("SELECT count(*) FROM oidc_sign_in_attempts")
                .fetch_one(&pool)
                .await
                .unwrap(),
            Some(0)
        );
        fixture.mock.assert_no_requests().await;
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn oidc_callback_replay_after_failure_is_refused(pool: sqlx::PgPool) {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};
        let token_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(500))
            .up_to_n_times(1)
            .expect(1)
            .mount(&token_server)
            .await;
        let fixture = CallbackFixture::start_with_token_endpoint(
            &pool,
            Some(&format!("{}/token", token_server.uri())),
        )
        .await;
        let first = fixture
            .server
            .get("/auth/callback")
            .add_query_param("code", "code")
            .add_query_param("state", &fixture.csrf)
            .await;
        assert_outcome(&first, "/login?outcome=unavailable");
        let replay = fixture
            .server
            .get("/auth/callback")
            .add_query_param("code", "code")
            .add_query_param("state", &fixture.csrf)
            .await;
        assert_outcome(&replay, "/login?outcome=expired");
        assert_eq!(token_server.received_requests().await.unwrap().len(), 1);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn oidc_callback_when_signed_in_redirects_home_without_exchange(pool: sqlx::PgPool) {
        let fixture = CallbackFixture::start(&pool).await;
        sign_in_fixture_locally(&fixture).await;
        let response = fixture
            .server
            .get("/auth/callback")
            .add_query_param("code", "code")
            .add_query_param("state", "irrelevant")
            .await;
        assert_eq!(response.status_code(), StatusCode::SEE_OTHER);
        assert_eq!(response.header("location"), "/");
        fixture.mock.assert_no_requests().await;
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn oidc_callback_losing_arbitration_never_signs_in(pool: sqlx::PgPool) {
        use tower_sessions::SessionStore;
        let fixture = CallbackFixture::start(&pool).await;
        assert!(
            sign_in::claim_win(&fixture.app_pool, &fixture.session_id.to_string())
                .await
                .unwrap()
        );
        fixture
            .mock
            .mount_token_endpoint(
                "loser",
                Some("loser@example.com"),
                Some("Loser"),
                &fixture.nonce,
            )
            .await;
        let response = fixture
            .server
            .get("/auth/callback")
            .add_query_param("code", "code")
            .add_query_param("state", &fixture.csrf)
            .await;
        assert_outcome(&response, "/login?outcome=expired");
        assert!(
            !fixture
                .store
                .load(&fixture.session_id)
                .await
                .unwrap()
                .unwrap()
                .data
                .contains_key("user_id")
        );
        assert_eq!(
            sqlx::query_scalar!("SELECT count(*) FROM user_identities")
                .fetch_one(&pool)
                .await
                .unwrap(),
            Some(0)
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn oidc_start_with_stale_session_starts_fresh(pool: sqlx::PgPool) {
        use tower_sessions::SessionStore;
        let fixture = CallbackFixture::start(&pool).await;
        let (user_id, old_id) = sign_in_fixture_locally(&fixture).await;
        sqlx::query!(
            "UPDATE users SET session_version = session_version + 1 WHERE id = $1",
            user_id
        )
        .execute(&fixture.app_pool)
        .await
        .unwrap();
        let response = fixture.server.get("/auth/oidc/login").await;
        assert_eq!(response.status_code(), StatusCode::TEMPORARY_REDIRECT);
        assert_ne!(response.cookie("id").value(), old_id.to_string());
        assert!(fixture.store.load(&old_id).await.unwrap().is_none());
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn oidc_start_with_disabled_session_starts_fresh(pool: sqlx::PgPool) {
        use tower_sessions::SessionStore;
        let fixture = CallbackFixture::start(&pool).await;
        let (user_id, old_id) = sign_in_fixture_locally(&fixture).await;
        user::disable_account(&fixture.app_pool, user_id)
            .await
            .unwrap();
        let response = fixture.server.get("/auth/oidc/login").await;
        assert_eq!(response.status_code(), StatusCode::TEMPORARY_REDIRECT);
        assert!(fixture.store.load(&old_id).await.unwrap().is_none());
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn oidc_start_loses_to_a_recent_winner(pool: sqlx::PgPool) {
        use tower_sessions::SessionStore;
        let fixture = CallbackFixture::start(&pool).await;
        assert!(
            sign_in::claim_win(&fixture.app_pool, &fixture.session_id.to_string())
                .await
                .unwrap()
        );
        assert!(
            sign_in::claim_oidc_attempt(
                &fixture.app_pool,
                &fixture.session_id.to_string(),
                &fixture.csrf
            )
            .await
            .unwrap()
            .is_some()
        );
        let before = fixture
            .store
            .load(&fixture.session_id)
            .await
            .unwrap()
            .unwrap()
            .data;
        let response = fixture.server.get("/auth/oidc/login").await;
        assert_eq!(response.status_code(), StatusCode::SEE_OTHER);
        assert_eq!(response.header("location"), "/login?outcome=expired");
        assert_eq!(
            fixture
                .store
                .load(&fixture.session_id)
                .await
                .unwrap()
                .unwrap()
                .data,
            before
        );
        assert_eq!(
            sqlx::query_scalar!("SELECT count(*) FROM oidc_sign_in_attempts")
                .fetch_one(&pool)
                .await
                .unwrap(),
            Some(0)
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn oidc_callback_refusal_after_winning_clears_session(pool: sqlx::PgPool) {
        use tower_sessions::SessionStore;
        let fixture = CallbackFixture::start(&pool).await;
        test_support::db::create_adult_with_password(
            &fixture.app_pool,
            "Existing",
            "collision@example.com",
            "a perfectly good password",
        )
        .await;
        let response = fixture
            .refused_callback("collision", "collision@example.com", "Collision")
            .await;
        assert!(
            fixture
                .store
                .load(&fixture.session_id)
                .await
                .unwrap()
                .is_none()
        );
        assert_eq!(
            sqlx::query_scalar!("SELECT count(*) FROM sign_in_cohorts")
                .fetch_one(&pool)
                .await
                .unwrap(),
            Some(0)
        );
        let cookie = response.cookie("id");
        assert_eq!(cookie.value(), "");
        assert_eq!(cookie.max_age().unwrap().whole_seconds(), 0);
        assert_eq!(
            fixture.server.get("/auth/oidc/login").await.status_code(),
            StatusCode::TEMPORARY_REDIRECT
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn oidc_key_callback_rotated_key_succeeds_after_one_fetch(pool: sqlx::PgPool) {
        let mut fixture = CallbackFixture::start(&pool).await;
        fixture
            .runtime()
            .age_for_tests(std::time::Duration::from_secs(30))
            .await;
        fixture.mock.rotate().await;
        fixture
            .mock
            .mount_token_endpoint("rotation-subject", None, Some("Rotated"), &fixture.nonce)
            .await;
        let response = fixture.key_callback().await;
        assert_eq!(response.status_code(), StatusCode::TEMPORARY_REDIRECT);
        assert_ne!(
            response.cookie("id").value(),
            fixture.session_id.to_string()
        );
        assert_eq!(response.cookie("reverie_theme").value(), "system");
        assert_eq!(fixture.mock.request_count("/token").await, 1);
        assert_eq!(fixture.mock.request_count("/jwks").await, 1);
        assert_eq!(
            fixture
                .mock
                .request_count("/.well-known/openid-configuration")
                .await,
            0
        );
        let identity = crate::models::user_identities::find_user_id_by_oidc(
            &pool,
            fixture.mock.issuer(),
            "rotation-subject",
        )
        .await
        .unwrap()
        .unwrap();
        let user = user::find_by_id(&pool, identity).await.unwrap().unwrap();
        assert_eq!(user.role, crate::models::role::Role::Adult);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn oidc_key_callback_unknown_key_inside_floor_fails_without_fetch(pool: sqlx::PgPool) {
        let mut fixture = CallbackFixture::start(&pool).await;
        fixture.mock.rotate().await;
        fixture
            .mock
            .mount_token_endpoint("rotation-subject", None, None, &fixture.nonce)
            .await;
        assert_outcome(&fixture.key_callback().await, "/login?outcome=keys");
        fixture.assert_no_key_identity("rotation-subject").await;
        assert_outcome(&fixture.key_callback().await, "/login?outcome=expired");
        assert_eq!(fixture.mock.request_count("/jwks").await, 0);
        assert_eq!(fixture.mock.request_count("/token").await, 1);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn oidc_key_callback_stale_snapshot_rechecks_before_verifying(pool: sqlx::PgPool) {
        use wiremock::ResponseTemplate;
        for outcome in ["known", "unknown", "failed"] {
            let mut fixture = CallbackFixture::start(&pool).await;
            let subject = format!("rotation-{outcome}");
            fixture
                .runtime()
                .age_for_tests(std::time::Duration::from_secs(601))
                .await;
            if outcome == "unknown" {
                let old_keys = fixture.mock.jwks();
                fixture.mock.rotate().await;
                fixture
                    .mock
                    .set_jwks_response(ResponseTemplate::new(200).set_body_json(old_keys), 1)
                    .await;
            } else if outcome == "failed" {
                fixture
                    .mock
                    .set_jwks_response(ResponseTemplate::new(500), 1)
                    .await;
            }
            fixture
                .mock
                .mount_token_endpoint(&subject, None, None, &fixture.nonce)
                .await;
            let response = fixture.key_callback().await;
            if outcome == "known" {
                assert_eq!(response.status_code(), StatusCode::TEMPORARY_REDIRECT);
            } else {
                assert_outcome(&response, "/login?outcome=keys");
            }
            if outcome != "known" {
                fixture.assert_no_key_identity(&subject).await;
            }
            assert_eq!(fixture.mock.request_count("/jwks").await, 1);
            assert_eq!(fixture.mock.request_count("/token").await, 1);
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn oidc_key_callback_other_validation_errors_do_not_refresh(pool: sqlx::PgPool) {
        use openidconnect::core::CoreJsonWebKeySet;
        for outcome in [
            "ambiguous",
            "signature",
            "issuer",
            "audience",
            "expired",
            "nonce",
        ] {
            let fixture = CallbackFixture::start(&pool).await;
            let nonce = openidconnect::Nonce::new(fixture.nonce.clone());
            let token = fixture
                .mock
                .token("rotation-subject", None, None, &fixture.nonce);
            let claims = token
                .claims(
                    &fixture
                        .mock
                        .client("http://localhost/auth/callback")
                        .id_token_verifier(),
                    &nonce,
                )
                .unwrap()
                .clone();
            let token = match outcome {
                "ambiguous" => {
                    let key = fixture.mock.jwks().keys()[0].clone();
                    fixture
                        .runtime()
                        .set_keys_for_tests(CoreJsonWebKeySet::new(vec![key.clone(), key]))
                        .await;
                    token
                }
                "signature" => {
                    let other = crate::test_support::oidc_mock::MockOidcProvider::start("").await;
                    other.token_with_claims(claims)
                }
                "issuer" => fixture.mock.token_with_claims(claims.set_issuer(
                    openidconnect::IssuerUrl::new("https://wrong.example.com".into()).unwrap(),
                )),
                "audience" => fixture.mock.token_with_claims(
                    claims.set_audiences(vec![openidconnect::Audience::new("wrong-client".into())]),
                ),
                "expired" => fixture.mock.token_with_claims(
                    claims.set_expiration(chrono::Utc::now() - chrono::TimeDelta::seconds(1)),
                ),
                _ => fixture.mock.token_with_claims(
                    claims.set_nonce(Some(openidconnect::Nonce::new("wrong-nonce".into()))),
                ),
            };
            fixture.mock.mount_id_token(token).await;
            let location = if outcome == "ambiguous" {
                "/login?outcome=keys"
            } else {
                "/login?outcome=unavailable"
            };
            assert_outcome(&fixture.key_callback().await, location);
            fixture.assert_no_key_identity("rotation-subject").await;
            assert_eq!(fixture.mock.request_count("/jwks").await, 0);
            assert_eq!(fixture.mock.request_count("/token").await, 1);
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn oidc_key_callback_refresh_failure_keeps_one_shot_boundary(pool: sqlx::PgPool) {
        use wiremock::ResponseTemplate;
        let mut fixture = CallbackFixture::start(&pool).await;
        fixture
            .runtime()
            .age_for_tests(std::time::Duration::from_secs(30))
            .await;
        fixture.mock.rotate().await;
        fixture
            .mock
            .set_jwks_response(ResponseTemplate::new(500), 1)
            .await;
        fixture
            .mock
            .mount_token_endpoint("rotation-subject", None, None, &fixture.nonce)
            .await;
        assert_outcome(&fixture.key_callback().await, "/login?outcome=keys");
        fixture.assert_no_key_identity("rotation-subject").await;
        assert_outcome(&fixture.key_callback().await, "/login?outcome=expired");
        assert_eq!(fixture.mock.request_count("/jwks").await, 1);
        assert_eq!(fixture.mock.request_count("/token").await, 1);
        fixture
            .runtime()
            .age_for_tests(std::time::Duration::from_secs(30))
            .await;
        fixture
            .mock
            .set_jwks_response(
                ResponseTemplate::new(200).set_body_json(fixture.mock.jwks()),
                1,
            )
            .await;
        let start = fixture.server.get("/auth/oidc/login").await;
        fixture.csrf = authorization_parameter(&start, "state");
        fixture.nonce = authorization_parameter(&start, "nonce");
        fixture
            .mock
            .mount_token_endpoint("rotation-subject", None, None, &fixture.nonce)
            .await;
        assert_eq!(
            fixture.key_callback().await.status_code(),
            StatusCode::TEMPORARY_REDIRECT
        );
        assert_eq!(fixture.mock.request_count("/jwks").await, 2);
        assert_eq!(fixture.mock.request_count("/token").await, 2);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn oidc_key_callback_pending_runtime_preserves_precedence(pool: sqlx::PgPool) {
        let fixture = CallbackFixture::start(&pool).await;
        fixture.runtime().clear_snapshot_for_tests().await;
        assert_outcome(
            &fixture.server.get("/auth/callback").await,
            "/login?outcome=expired",
        );
        assert_outcome(
            &fixture
                .server
                .get("/auth/callback?code=test&state=wrong")
                .await,
            "/login?outcome=expired",
        );
        assert_outcome(&fixture.key_callback().await, "/login?outcome=unavailable");
        fixture.assert_no_key_identity("rotation-subject").await;
        sign_in_fixture_locally(&fixture).await;
        let signed_in = fixture.key_callback().await;
        assert_eq!(signed_in.status_code(), StatusCode::SEE_OTHER);
        assert_eq!(signed_in.header("location"), "/");
        fixture.mock.assert_no_requests().await;
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn oidc_key_callback_start_pending_writes_nothing(pool: sqlx::PgPool) {
        let fixture = CallbackFixture::start(&pool).await;
        fixture.runtime().clear_snapshot_for_tests().await;
        let anonymous = axum_test::TestServer::new(crate::build_router(fixture.state.clone()));
        let pending = anonymous.get("/auth/oidc/login").await;
        assert_eq!(pending.status_code(), StatusCode::SEE_OTHER);
        assert_eq!(pending.header("location"), "/login?outcome=unavailable");
        assert!(!pending.cookies().iter().any(|cookie| cookie.name() == "id"));
        assert_eq!(
            sqlx::query_scalar!("SELECT count(*) FROM oidc_sign_in_attempts")
                .fetch_one(&pool)
                .await
                .unwrap(),
            Some(1)
        );
        assert_eq!(
            sqlx::query_scalar!("SELECT count(*) FROM sign_in_cohorts")
                .fetch_one(&pool)
                .await
                .unwrap(),
            Some(1)
        );
        let mut limited_state = fixture.state.clone();
        limited_state.config.trusted_client_ip_header = Some("x-forwarded-for".to_owned());
        limited_state.oidc_limiter =
            crate::auth::rate_limit::build_login_limiter(std::num::NonZeroU32::new(1).unwrap());
        let limited = axum_test::TestServer::new(crate::build_router(limited_state));
        assert_eq!(
            limited
                .get("/auth/oidc/login")
                .add_header("x-forwarded-for", "198.51.100.24")
                .await
                .header("location"),
            "/login?outcome=unavailable"
        );
        assert_eq!(
            limited
                .get("/auth/oidc/login")
                .add_header("x-forwarded-for", "198.51.100.24")
                .await
                .header("location"),
            "/login?outcome=throttled"
        );
        sign_in_fixture_locally(&fixture).await;
        assert_eq!(
            fixture
                .server
                .get("/auth/oidc/login")
                .await
                .header("location"),
            "/"
        );
        fixture.mock.assert_no_requests().await;
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn oidc_outcome_provider_denial_consumes_state_without_exchange(pool: sqlx::PgPool) {
        let fixture = CallbackFixture::start(&pool).await;
        let response = fixture
            .server
            .get("/auth/callback")
            .add_query_param("error", "access_denied")
            .add_query_param("error_description", "private-provider-description")
            .add_query_param("state", &fixture.csrf)
            .await;
        assert_eq!(response.status_code(), StatusCode::SEE_OTHER);
        assert_eq!(response.header("location"), "/login?outcome=denied");
        let replay = fixture.key_callback().await;
        assert_eq!(replay.status_code(), StatusCode::SEE_OTHER);
        assert_eq!(replay.header("location"), "/login?outcome=expired");
        fixture.mock.assert_no_requests().await;
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn oidc_outcome_provider_errors_require_valid_state(pool: sqlx::PgPool) {
        for provider_error in ["access_denied", "server_error", "invalid_grant"] {
            let fixture = CallbackFixture::start(&pool).await;
            for state in [None, Some("wrong-state")] {
                let request = fixture
                    .server
                    .get("/auth/callback")
                    .add_query_param("error", provider_error);
                let response = if let Some(state) = state {
                    request.add_query_param("state", state).await
                } else {
                    request.await
                };
                assert_outcome(&response, "/login?outcome=expired");
            }
            let response = fixture
                .server
                .get("/auth/callback")
                .add_query_param("error", provider_error)
                .add_query_param("state", &fixture.csrf)
                .await;
            assert_outcome(
                &response,
                if provider_error == "access_denied" {
                    "/login?outcome=denied"
                } else {
                    "/login?outcome=unavailable"
                },
            );
            assert_outcome(&fixture.key_callback().await, "/login?outcome=expired");
            fixture.mock.assert_no_requests().await;
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn oidc_outcome_malformed_callbacks_preserve_pending_attempt(pool: sqlx::PgPool) {
        let fixture = CallbackFixture::start(&pool).await;
        for query in [
            "",
            "code=",
            "error=",
            "code=a&error=access_denied",
            "code=a&code=b",
            "error=access_denied&error=server_error",
            "code=a&state=wrong",
        ] {
            let response = fixture
                .server
                .get(&format!("/auth/callback?{query}"))
                .add_query_param("state", &fixture.csrf)
                .await;
            assert_outcome(&response, "/login?outcome=expired");
        }
        let response = fixture
            .server
            .get("/auth/callback?error=access_denied")
            .add_query_param("state", &fixture.csrf)
            .await;
        assert_outcome(&response, "/login?outcome=denied");
        fixture.mock.assert_no_requests().await;
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn oidc_outcome_expired_state_never_exchanges(pool: sqlx::PgPool) {
        let fixture = CallbackFixture::start(&pool).await;
        sqlx::query!("UPDATE oidc_sign_in_attempts SET expires_at = now() - interval '1 second'")
            .execute(&pool)
            .await
            .unwrap();
        assert_outcome(&fixture.key_callback().await, "/login?outcome=expired");
        let response = fixture
            .server
            .get("/auth/callback?error=access_denied")
            .add_query_param("state", &fixture.csrf)
            .await;
        assert_outcome(&response, "/login?outcome=expired");
        fixture.mock.assert_no_requests().await;
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn oidc_outcome_signed_in_malformed_callback_goes_home(pool: sqlx::PgPool) {
        let fixture = CallbackFixture::start(&pool).await;
        sign_in_fixture_locally(&fixture).await;
        for query in ["", "?code=a&code=b", "?error=access_denied"] {
            let response = fixture.server.get(&format!("/auth/callback{query}")).await;
            assert_eq!(response.status_code(), StatusCode::SEE_OTHER);
            assert_eq!(response.header("location"), "/");
        }
        fixture.mock.assert_no_requests().await;
    }

    #[tokio::test]
    async fn oidc_outcome_unconfigured_callback_keeps_not_found() {
        let mut state = test_support::test_state();
        state.oidc = None;
        let server = axum_test::TestServer::new(crate::build_router_with_session_store(
            state,
            tower_sessions::MemoryStore::default(),
        ));
        assert_eq!(
            server.get("/auth/callback").await.status_code(),
            StatusCode::NOT_FOUND
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn oidc_outcome_token_errors_are_fixed_and_private(pool: sqlx::PgPool) {
        use tracing::instrument::WithSubscriber;
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};
        for kind in [
            "invalid_grant",
            "server_error",
            "server_status",
            "parse",
            "missing_id_token",
        ] {
            let token_server = MockServer::start().await;
            let response = match kind {
                "server_status" => ResponseTemplate::new(500).set_body_string("private-token-body"),
                "parse" => ResponseTemplate::new(200).set_body_string("private-token-body"),
                "missing_id_token" => ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "access_token": "private-access-token", "token_type": "Bearer"})),
                _ => ResponseTemplate::new(400).set_body_json(serde_json::json!({
                    "error": kind, "error_description": "private-provider-description"})),
            };
            Mock::given(method("POST"))
                .and(path("/token"))
                .respond_with(response)
                .expect(1)
                .mount(&token_server)
                .await;
            let fixture = CallbackFixture::start_with_token_endpoint(
                &pool,
                Some(&format!("{}/token", token_server.uri())),
            )
            .await;
            let logs = test_support::CapturedLogs::default();
            let response = fixture
                .key_callback()
                .with_subscriber(callback_log_subscriber(&logs))
                .await;
            assert_outcome(
                &response,
                if kind == "invalid_grant" {
                    "/login?outcome=expired"
                } else {
                    "/login?outcome=unavailable"
                },
            );
            if kind == "invalid_grant" {
                assert!(!logs.text().contains("token_request_failed"));
            } else {
                assert_private_cause(
                    &logs,
                    if kind == "missing_id_token" {
                        "id_token_missing"
                    } else {
                        "token_request_failed"
                    },
                    &[
                        "private-provider-description",
                        "private-token-body",
                        "mock-code",
                        &fixture.csrf,
                        &fixture.nonce,
                    ],
                );
            }
            let captured = logs.text();
            assert!(captured.contains("OIDC sign-in did not complete"));
            for secret in [
                "private-provider-description",
                "private-token-body",
                "private-access-token",
                "mock-code",
                &fixture.csrf,
                &fixture.nonce,
            ] {
                assert!(!captured.contains(secret), "callback log leaked {secret}");
                assert!(!response.text().contains(secret));
            }
            assert_outcome(&fixture.key_callback().await, "/login?outcome=expired");
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn oidc_outcome_token_transport_failure_is_unavailable(pool: sqlx::PgPool) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}/token", listener.local_addr().unwrap());
        drop(listener);
        let fixture = CallbackFixture::start_with_token_endpoint(&pool, Some(&endpoint)).await;
        assert_outcome(&fixture.key_callback().await, "/login?outcome=unavailable");
        fixture.assert_no_key_identity("unused").await;
        assert_outcome(&fixture.key_callback().await, "/login?outcome=expired");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn oidc_outcome_debug_trace_never_records_provider_query(pool: sqlx::PgPool) {
        use tracing::instrument::WithSubscriber;
        let fixture = CallbackFixture::start(&pool).await;
        let logs = test_support::CapturedLogs::default();
        let writer = logs.clone();
        let subscriber = tracing_subscriber::fmt()
            .without_time()
            .with_ansi(false)
            .with_max_level(tracing::Level::DEBUG)
            .with_writer(move || writer.clone())
            .finish();
        let request = fixture
            .server
            .get("/auth/callback?error=access_denied&error_description=private-description")
            .add_query_param("state", &fixture.csrf);
        let response = async { request.await }.with_subscriber(subscriber).await;
        assert_outcome(&response, "/login?outcome=denied");
        let captured = logs.text();
        assert!(captured.contains("/auth/callback"));
        assert!(captured.contains("OIDC sign-in did not complete"));
        for secret in [
            "private-description",
            "access_denied",
            &fixture.csrf,
            &fixture.nonce,
        ] {
            assert!(!captured.contains(secret), "callback log leaked {secret}");
        }
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum StoreOperation {
        Load,
        Create,
        Save,
        Delete,
    }

    #[derive(Clone, Debug)]
    struct OutcomeStore {
        inner: crate::auth::store::PostgresStore,
        operation: StoreOperation,
        failures: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    }

    impl OutcomeStore {
        fn check(&self, operation: StoreOperation) -> tower_sessions::session_store::Result<()> {
            if self.operation == operation
                && self
                    .failures
                    .try_update(
                        std::sync::atomic::Ordering::SeqCst,
                        std::sync::atomic::Ordering::SeqCst,
                        |remaining| remaining.checked_sub(1),
                    )
                    .is_ok()
            {
                return Err(tower_sessions::session_store::Error::Backend(
                    "store unavailable".into(),
                ));
            }
            Ok(())
        }
    }

    #[async_trait::async_trait]
    impl tower_sessions::SessionStore for OutcomeStore {
        async fn create(
            &self,
            record: &mut tower_sessions::session::Record,
        ) -> tower_sessions::session_store::Result<()> {
            self.check(StoreOperation::Create)?;
            self.inner.create(record).await
        }
        async fn save(
            &self,
            record: &tower_sessions::session::Record,
        ) -> tower_sessions::session_store::Result<()> {
            self.check(StoreOperation::Save)?;
            self.inner.save(record).await
        }
        async fn load(
            &self,
            id: &tower_sessions::session::Id,
        ) -> tower_sessions::session_store::Result<Option<tower_sessions::session::Record>>
        {
            self.check(StoreOperation::Load)?;
            self.inner.load(id).await
        }
        async fn delete(
            &self,
            id: &tower_sessions::session::Id,
        ) -> tower_sessions::session_store::Result<()> {
            self.check(StoreOperation::Delete)?;
            self.inner.delete(id).await
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn oidc_outcome_explicit_save_and_rotation_failures_flush_winner(pool: sqlx::PgPool) {
        use tower_sessions::SessionStore;
        use tracing::instrument::WithSubscriber;
        for operation in [StoreOperation::Create, StoreOperation::Delete] {
            let fixture = CallbackFixture::start(&pool).await;
            fixture
                .mock
                .mount_token_endpoint("storage-failure", None, None, &fixture.nonce)
                .await;
            let store = OutcomeStore {
                inner: fixture.store.clone(),
                operation,
                failures: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(1)),
            };
            let failures = store.failures.clone();
            let mut server = axum_test::TestServer::new(crate::build_router_with_session_store(
                fixture.state.clone(),
                store,
            ));
            server.save_cookies();
            let logs = test_support::CapturedLogs::default();
            let request = server
                .get("/auth/callback")
                .add_header(
                    axum::http::header::COOKIE,
                    format!("id={}", fixture.session_id),
                )
                .add_query_param("code", "mock-code")
                .add_query_param("state", &fixture.csrf);
            let response = async { request.await }
                .with_subscriber(callback_log_subscriber(&logs))
                .await;
            assert_private_cause(
                &logs,
                if operation == StoreOperation::Create {
                    "session_save_failed"
                } else {
                    "old_session_removal_failed"
                },
                &[
                    "mock-code",
                    &fixture.csrf,
                    &fixture.nonce,
                    "private-provider-description",
                    "private-token-body",
                ],
            );
            assert_outcome(&response, "/login?outcome=unavailable");
            assert_eq!(failures.load(std::sync::atomic::Ordering::SeqCst), 0);
            if operation == StoreOperation::Create {
                assert!(
                    fixture
                        .store
                        .load(&fixture.session_id)
                        .await
                        .unwrap()
                        .is_none()
                );
                assert!(!logs.text().contains("old_session_removal_failed"));
            } else {
                assert!(logs.text().contains("old_session_removal_failed"));
                test_support::db::create_adult_with_password(
                    &fixture.app_pool,
                    "Later local",
                    "later@example.com",
                    "a perfectly good password",
                )
                .await;
                assert_retained_cohort_is_terminal(&fixture, &server, "later@example.com").await;
            }
            assert_eq!(response.cookie("id").value(), "");
            assert!(
                !response
                    .headers()
                    .get_all("set-cookie")
                    .iter()
                    .any(|value| value.to_str().unwrap().starts_with("reverie_theme="))
            );
            assert_eq!(
                server.get("/auth/me").await.status_code(),
                StatusCode::UNAUTHORIZED
            );
            assert!(
                crate::models::user_identities::find_user_id_by_oidc(
                    &fixture.app_pool,
                    fixture.mock.issuer(),
                    "storage-failure"
                )
                .await
                .unwrap()
                .is_some()
            );
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn oidc_outcome_session_load_failure_is_unavailable(pool: sqlx::PgPool) {
        use tracing::instrument::WithSubscriber;
        let logs = test_support::CapturedLogs::default();
        let fixture = CallbackFixture::start(&pool).await;
        let store = OutcomeStore {
            inner: fixture.store.clone(),
            operation: StoreOperation::Load,
            failures: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(1)),
        };
        let server = axum_test::TestServer::new(crate::build_router_with_session_store(
            fixture.state.clone(),
            store,
        ));
        let response = server
            .get("/auth/callback")
            .add_header(
                axum::http::header::COOKIE,
                format!("id={}", fixture.session_id),
            )
            .add_query_param("code", "mock-code")
            .add_query_param("error_description", "private-provider-description")
            .add_query_param("state", &fixture.csrf)
            .into_future()
            .with_subscriber(callback_log_subscriber(&logs))
            .await;
        assert_private_cause(
            &logs,
            "signed_in_check_failed",
            &[
                "mock-code",
                &fixture.csrf,
                &fixture.nonce,
                "private-provider-description",
                "private-token-body",
            ],
        );
        assert_outcome(&response, "/login?outcome=unavailable");
        fixture.assert_no_key_identity("unused").await;
        fixture.mock.assert_no_requests().await;
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn oidc_outcome_failed_flush_keeps_cohort_won(pool: sqlx::PgPool) {
        let fixture = CallbackFixture::start(&pool).await;
        test_support::db::create_adult_with_password(
            &fixture.app_pool,
            "Local",
            "collision@example.com",
            "a perfectly good password",
        )
        .await;
        fixture
            .mock
            .mount_token_endpoint(
                "collision",
                Some("collision@example.com"),
                None,
                &fixture.nonce,
            )
            .await;
        let store = OutcomeStore {
            inner: fixture.store.clone(),
            operation: StoreOperation::Delete,
            failures: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(1)),
        };
        let server = axum_test::TestServer::new(crate::build_router_with_session_store(
            fixture.state.clone(),
            store,
        ));
        let response = server
            .get("/auth/callback")
            .add_header(
                axum::http::header::COOKIE,
                format!("id={}", fixture.session_id),
            )
            .add_query_param("code", "mock-code")
            .add_query_param("state", &fixture.csrf)
            .await;
        assert_outcome(&response, "/login?outcome=account");
        assert_eq!(
            response.cookie("id").value(),
            fixture.session_id.to_string()
        );
        assert_retained_cohort_is_terminal(&fixture, &server, "collision@example.com").await;
    }

    async fn assert_retained_cohort_is_terminal(
        fixture: &CallbackFixture,
        server: &axum_test::TestServer,
        email: &str,
    ) {
        use tower_sessions::SessionStore;
        let old_cookie = format!("id={}", fixture.session_id);
        let record = fixture
            .store
            .load(&fixture.session_id)
            .await
            .unwrap()
            .unwrap();
        assert!(!record.data.contains_key("user_id"));
        assert!(!record.data.contains_key("session_version"));
        assert_eq!(
            sign_in::join_pending(&fixture.app_pool, &fixture.session_id.to_string())
                .await
                .unwrap(),
            sign_in::CohortState::Won
        );
        assert!(
            !sign_in::claim_win(&fixture.app_pool, &fixture.session_id.to_string())
                .await
                .unwrap()
        );
        let callback = server
            .get("/auth/callback")
            .add_header(axum::http::header::COOKIE, old_cookie.clone())
            .add_query_param("code", "late-code")
            .add_query_param("state", &fixture.csrf)
            .await;
        assert_outcome(&callback, "/login?outcome=expired");
        assert_eq!(
            callback.cookie("id").value(),
            fixture.session_id.to_string()
        );
        let local = server
            .post("/auth/local/login")
            .add_header(axum::http::header::COOKIE, old_cookie.clone())
            .json(&serde_json::json!({"email":email,"password":"a perfectly good password"}))
            .await;
        test_support::assert_problem(
            &local,
            crate::error::problems::SIGN_IN_INACTIVE,
            StatusCode::CONFLICT,
        );
        assert_eq!(local.cookie("id").value(), fixture.session_id.to_string());
        assert_eq!(
            server
                .get("/auth/me")
                .add_header(axum::http::header::COOKIE, old_cookie)
                .await
                .status_code(),
            StatusCode::UNAUTHORIZED
        );
        let after = fixture
            .store
            .load(&fixture.session_id)
            .await
            .unwrap()
            .unwrap();
        assert!(!after.data.contains_key("user_id"));
        assert!(!after.data.contains_key("session_version"));
        assert!(
            !sign_in::claim_win(&fixture.app_pool, &fixture.session_id.to_string())
                .await
                .unwrap()
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn oidc_outcome_attempt_storage_failure_is_unavailable(pool: sqlx::PgPool) {
        use tracing::instrument::WithSubscriber;
        let logs = test_support::CapturedLogs::default();
        let fixture = CallbackFixture::start(&pool).await;
        let mut state = fixture.state.clone();
        let closed = sqlx::PgPool::connect_lazy("postgres://localhost/unavailable").unwrap();
        closed.close().await;
        state.pool = closed;
        let server = axum_test::TestServer::new(crate::build_router_with_session_store(
            state,
            fixture.store.clone(),
        ));
        let response = server
            .get("/auth/callback")
            .add_header(
                axum::http::header::COOKIE,
                format!("id={}", fixture.session_id),
            )
            .add_query_param("code", "mock-code")
            .add_query_param("error_description", "private-provider-description")
            .add_query_param("state", &fixture.csrf)
            .into_future()
            .with_subscriber(callback_log_subscriber(&logs))
            .await;
        assert_private_cause(
            &logs,
            "attempt_claim_failed",
            &[
                "mock-code",
                &fixture.csrf,
                &fixture.nonce,
                "private-provider-description",
                "private-token-body",
            ],
        );
        assert_outcome(&response, "/login?outcome=unavailable");
        fixture.mock.assert_no_requests().await;
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn oidc_outcome_provisioning_failure_flushes_winner(pool: sqlx::PgPool) {
        use tower_sessions::SessionStore;
        use tracing::instrument::WithSubscriber;
        let logs = test_support::CapturedLogs::default();
        let fixture = CallbackFixture::start(&pool).await;
        fixture
            .mock
            .mount_token_endpoint(
                "provision-failure",
                None,
                Some("provider\0name"),
                &fixture.nonce,
            )
            .await;
        let response = fixture
            .key_callback()
            .with_subscriber(callback_log_subscriber(&logs))
            .await;
        assert_outcome(&response, "/login?outcome=unavailable");
        assert_private_cause(
            &logs,
            "provisioning_failed",
            &[
                "mock-code",
                &fixture.csrf,
                &fixture.nonce,
                "private-provider-description",
                "private-token-body",
            ],
        );
        assert!(
            fixture
                .store
                .load(&fixture.session_id)
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            crate::models::user_identities::find_user_id_by_oidc(
                &fixture.app_pool,
                fixture.mock.issuer(),
                "provision-failure"
            )
            .await
            .unwrap()
            .is_none()
        );
    }

    struct CallbackFixture {
        state: crate::state::AppState,
        app_pool: sqlx::PgPool,
        mock: crate::test_support::oidc_mock::MockOidcProvider,
        store: crate::auth::store::PostgresStore,
        server: axum_test::TestServer,
        session_id: tower_sessions::session::Id,
        csrf: String,
        nonce: String,
    }

    impl CallbackFixture {
        fn runtime(&self) -> &std::sync::Arc<crate::auth::oidc::OidcRuntime> {
            self.state.oidc.as_ref().unwrap()
        }

        async fn key_callback(&self) -> axum_test::TestResponse {
            self.server
                .get("/auth/callback")
                .add_query_param("code", "mock-code")
                .add_query_param("error_description", "private-provider-description")
                .add_query_param("state", &self.csrf)
                .await
        }

        async fn assert_no_key_identity(&self, subject: &str) {
            use tower_sessions::SessionStore;
            assert!(
                crate::models::user_identities::find_user_id_by_oidc(
                    &self.app_pool,
                    self.mock.issuer(),
                    subject
                )
                .await
                .unwrap()
                .is_none()
            );
            assert!(matches!(
                sign_in::join_pending(&self.app_pool, &self.session_id.to_string())
                    .await
                    .unwrap(),
                sign_in::CohortState::Pending
            ));
            let session = self.store.load(&self.session_id).await.unwrap().unwrap();
            assert!(!session.data.contains_key("user_id"));
        }

        async fn start(pool: &sqlx::PgPool) -> Self {
            Self::start_with_token_endpoint(pool, None).await
        }

        async fn start_with_token_endpoint(
            pool: &sqlx::PgPool,
            token_endpoint: Option<&str>,
        ) -> Self {
            use crate::state::AppState;
            use crate::test_support::oidc_mock::MockOidcProvider;
            use tower_sessions::session::Id as SessionId;

            let app_pool = test_support::db::app_pool_for(pool).await;
            let ingestion_pool = test_support::db::ingestion_pool_for(pool).await;
            let mock = MockOidcProvider::start("").await;
            let runtime = if let Some(endpoint) = token_endpoint {
                mock.mount_discovery_with_token_endpoint(endpoint).await;
                let transport = crate::auth::oidc::OidcTransport::for_tests();
                let metadata = openidconnect::core::CoreProviderMetadata::discover_async(
                    openidconnect::IssuerUrl::new(mock.issuer().to_owned()).unwrap(),
                    &transport.oauth_client(),
                )
                .await
                .unwrap();
                crate::auth::oidc::OidcRuntime::new(
                    mock.configuration("http://localhost:3000/auth/callback"),
                    metadata,
                    transport,
                )
            } else {
                mock.runtime("http://localhost:3000/auth/callback")
            };
            let oidc = Some(std::sync::Arc::new(runtime));
            let store = crate::auth::store::PostgresStore::new(app_pool.clone());
            let (config, library_files) = crate::test_support::test_storage_config(
                None,
                crate::models::storage_library::LibraryId::from_uuid(uuid::Uuid::new_v4()),
            );
            let state = AppState {
                pool: app_pool.clone(),
                ingestion_pool,
                ingestion: crate::services::ingestion::coordinator_channel().0,
                library_files,
                hash_limiter: crate::auth::password::HashLimiter::new(
                    usize::try_from(config.auth_hash_concurrency).unwrap(),
                ),
                config,
                oidc,
                login_limiter: test_support::test_login_limiter(),
                recovery_limiter: test_support::test_login_limiter(),
                oidc_limiter: test_support::test_login_limiter(),
                settings: test_support::test_settings(),
                last_settings_reload: std::sync::Arc::new(tokio::sync::RwLock::new(None)),
            };
            let app = crate::build_router_with_session_store(state.clone(), store.clone());
            let mut server = axum_test::TestServer::new(app);
            server.save_cookies();

            let login_resp = server.get("/auth/oidc/login").await;
            assert_eq!(login_resp.status_code(), StatusCode::TEMPORARY_REDIRECT);
            let session_id: SessionId = login_resp
                .cookie("id")
                .value()
                .parse()
                .expect("parse session id");
            let csrf: String = authorization_parameter(&login_resp, "state");
            let nonce: String = authorization_parameter(&login_resp, "nonce");
            Self {
                state,
                app_pool,
                mock,
                store,
                server,
                session_id,
                csrf,
                nonce,
            }
        }

        async fn refused_callback(
            &self,
            subject: &str,
            email: &str,
            name: &str,
        ) -> axum_test::TestResponse {
            use tower_sessions::SessionStore;

            self.mock
                .mount_token_endpoint(subject, Some(email), Some(name), &self.nonce)
                .await;
            let cb_resp = self
                .server
                .get("/auth/callback")
                .add_query_param("code", "mock-auth-code")
                .add_query_param("state", &self.csrf)
                .await;
            assert_outcome(&cb_resp, "/login?outcome=account");
            assert!(
                self.store
                    .load(&self.session_id)
                    .await
                    .expect("load session")
                    .is_none()
            );
            assert_eq!(
                self.server.get("/auth/me").await.status_code(),
                StatusCode::UNAUTHORIZED,
                "a refused callback must not establish a session"
            );
            cb_resp
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn callback_refuses_disabled_account_without_writing_profile(pool: sqlx::PgPool) {
        use crate::models::user;

        let fixture = CallbackFixture::start(&pool).await;
        let subject = "test-subject-disabled";
        let existing = user::upsert_from_oidc(
            &fixture.app_pool,
            fixture.mock.issuer(),
            subject,
            "Original Name",
            Some("original@example.com"),
        )
        .await
        .expect("seed identity");
        user::disable_account(&fixture.app_pool, existing.id)
            .await
            .expect("disable");
        let before = user::find_by_id(&fixture.app_pool, existing.id)
            .await
            .expect("reload")
            .expect("exists");

        fixture
            .refused_callback(subject, "changed@example.com", "Changed Name")
            .await;

        let after = user::find_by_id(&fixture.app_pool, existing.id)
            .await
            .expect("reload")
            .expect("exists");
        assert_eq!(after.display_name, "Original Name");
        assert_eq!(after.email.as_deref(), Some("original@example.com"));
        assert_eq!(after.updated_at, before.updated_at);
        assert!(after.disabled_at.is_some());
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn callback_refuses_an_email_held_by_another_account_and_writes_nothing(
        pool: sqlx::PgPool,
    ) {
        use crate::models::role::Role;
        use crate::models::user;

        let fixture = CallbackFixture::start(&pool).await;
        let local = user::create_local(
            &fixture.app_pool,
            "shared@example.com",
            "Local Owner",
            Role::Adult,
            None,
        )
        .await
        .expect("local account");
        let users_before = sqlx::query_scalar!("SELECT count(*) AS \"c!\" FROM users")
            .fetch_one(&pool)
            .await
            .expect("count users");

        let subject = "test-subject-collision";
        fixture
            .refused_callback(subject, "Shared@Example.com", "Newcomer")
            .await;

        let users_after = sqlx::query_scalar!("SELECT count(*) AS \"c!\" FROM users")
            .fetch_one(&pool)
            .await
            .expect("count users");
        assert_eq!(users_after, users_before, "no user row is written");
        let identities = sqlx::query_scalar!(
            "SELECT count(*) AS \"c!\" FROM user_identities WHERE issuer = $1 AND subject = $2",
            fixture.mock.issuer(),
            subject,
        )
        .fetch_one(&pool)
        .await
        .expect("count identities");
        assert_eq!(identities, 0, "no identity link is written");
        let untouched = user::find_by_id(&fixture.app_pool, local.id)
            .await
            .expect("reload")
            .expect("exists");
        assert_eq!(untouched.display_name, "Local Owner");
        assert_eq!(untouched.updated_at, local.updated_at);
        assert_eq!(
            sqlx::query_scalar!(
                "SELECT count(*) AS \"c!\" FROM user_identities WHERE user_id = $1",
                local.id
            )
            .fetch_one(&pool)
            .await
            .expect("count links"),
            0,
            "the existing account is not linked"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn email_collision_refusal_is_indistinguishable_from_a_disabled_account(
        pool: sqlx::PgPool,
    ) {
        use crate::models::role::Role;
        use crate::models::user;

        fn observable(response: &axum_test::TestResponse) -> serde_json::Value {
            let removal = response.cookie("id");
            assert_eq!(removal.value(), "");
            assert_eq!(
                removal.max_age().expect("removal max-age").whole_seconds(),
                0
            );
            assert!(
                removal
                    .expires_datetime()
                    .expect("removal expiry")
                    .unix_timestamp()
                    < chrono::Utc::now().timestamp()
            );
            let mut headers = Vec::new();
            let mut cookies = Vec::new();
            for (name, value) in response.headers() {
                let value = value.to_str().expect("ascii header").to_owned();
                if name == axum::http::header::SET_COOKIE {
                    let attributes: Vec<&str> = value
                        .split(';')
                        .map(str::trim)
                        .filter(|attribute| !attribute.starts_with("Expires="))
                        .collect();
                    cookies.push(attributes.join("; "));
                } else {
                    headers.push(format!("{name}: {value}"));
                }
            }
            headers.sort();
            cookies.sort();
            serde_json::json!({
                "status": response.status_code().as_u16(),
                "headers": headers,
                "set_cookie_attributes": cookies,
                "body": response.text(),
            })
        }

        let disabled_fixture = CallbackFixture::start(&pool).await;
        let disabled_subject = "test-subject-disabled";
        let disabled = user::upsert_from_oidc(
            &disabled_fixture.app_pool,
            disabled_fixture.mock.issuer(),
            disabled_subject,
            "Original Name",
            Some("original@example.com"),
        )
        .await
        .expect("seed identity");
        user::disable_account(&disabled_fixture.app_pool, disabled.id)
            .await
            .expect("disable");
        let disabled_response = disabled_fixture
            .refused_callback(disabled_subject, "changed@example.com", "Changed Name")
            .await;

        let collision_fixture = CallbackFixture::start(&pool).await;
        user::create_local(
            &collision_fixture.app_pool,
            "shared@example.com",
            "Local Owner",
            Role::Adult,
            None,
        )
        .await
        .expect("local account");
        let collision_response = collision_fixture
            .refused_callback("test-subject-collision", "shared@example.com", "Newcomer")
            .await;

        let disabled_observed = observable(&disabled_response);
        assert_eq!(disabled_observed["body"], "");
        assert_eq!(observable(&collision_response), disabled_observed);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn re_login_preserves_app_csrf_token(pool: sqlx::PgPool) {
        use crate::state::AppState;
        use crate::test_support::oidc_mock::MockOidcProvider;
        use tower_sessions::SessionStore;
        use tower_sessions::session::Id as SessionId;

        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;

        let mock = MockOidcProvider::start("").await;
        let oidc = Some(std::sync::Arc::new(
            mock.runtime("http://localhost:3000/auth/callback"),
        ));
        let store = crate::auth::store::PostgresStore::new(app_pool.clone());
        let (config, library_files) = crate::test_support::test_storage_config(
            None,
            crate::models::storage_library::LibraryId::from_uuid(uuid::Uuid::new_v4()),
        );
        let state = AppState {
            pool: app_pool.clone(),
            ingestion_pool,
            ingestion: crate::services::ingestion::coordinator_channel().0,
            library_files,
            hash_limiter: crate::auth::password::HashLimiter::new(
                usize::try_from(config.auth_hash_concurrency).unwrap(),
            ),
            config,
            oidc,
            login_limiter: test_support::test_login_limiter(),
            recovery_limiter: test_support::test_login_limiter(),
            oidc_limiter: test_support::test_login_limiter(),
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
        let oidc_state_1: String = authorization_parameter(&login_resp, "state");
        assert!(
            !record.data.contains_key("csrf_token"),
            "/auth/oidc/login must NOT write the app-level csrf_token key; that \
             would clobber a returning user's existing app token",
        );
        let nonce: String = authorization_parameter(&login_resp, "nonce");
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

        let login_resp_2 = server.get("/auth/oidc/login").await;
        assert_eq!(login_resp_2.status_code(), StatusCode::SEE_OTHER);
        assert_eq!(login_resp_2.header("location"), "/");
        let session_id_2: SessionId = login_resp_2
            .cookie("id")
            .value()
            .parse()
            .expect("session id");
        let record_2 = store
            .load(&session_id_2)
            .await
            .expect("load")
            .expect("record");

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
        use tower_sessions::SessionStore;
        use tower_sessions::session::Id as SessionId;

        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;

        let mock = MockOidcProvider::start("").await;
        let oidc = Some(std::sync::Arc::new(
            mock.runtime("http://localhost:3000/auth/callback"),
        ));
        let store = crate::auth::store::PostgresStore::new(app_pool.clone());
        let (config, library_files) = crate::test_support::test_storage_config(
            None,
            crate::models::storage_library::LibraryId::from_uuid(uuid::Uuid::new_v4()),
        );
        let state = AppState {
            pool: app_pool.clone(),
            ingestion_pool,
            ingestion: crate::services::ingestion::coordinator_channel().0,
            library_files,
            hash_limiter: crate::auth::password::HashLimiter::new(
                usize::try_from(config.auth_hash_concurrency).unwrap(),
            ),
            config,
            oidc,
            login_limiter: test_support::test_login_limiter(),
            recovery_limiter: test_support::test_login_limiter(),
            oidc_limiter: test_support::test_login_limiter(),
            settings: test_support::test_settings(),
            last_settings_reload: std::sync::Arc::new(tokio::sync::RwLock::new(None)),
        };
        let app = crate::build_router_with_session_store(state, store.clone());
        let mut server = axum_test::TestServer::new(app);
        server.save_cookies();

        // Drive /auth/oidc/login → /auth/callback to establish an authenticated session.
        let login_resp = server.get("/auth/oidc/login").await;
        let csrf: String = authorization_parameter(&login_resp, "state");
        let nonce: String = authorization_parameter(&login_resp, "nonce");
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
        use tower_sessions::SessionStore;
        use tower_sessions::session::Id as SessionId;

        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;

        let mock = MockOidcProvider::start("").await;
        let oidc = Some(std::sync::Arc::new(
            mock.runtime("http://localhost:3000/auth/callback"),
        ));
        let store = crate::auth::store::PostgresStore::new(app_pool.clone());
        let (config, library_files) = crate::test_support::test_storage_config(
            None,
            crate::models::storage_library::LibraryId::from_uuid(uuid::Uuid::new_v4()),
        );
        let state = AppState {
            pool: app_pool.clone(),
            ingestion_pool,
            ingestion: crate::services::ingestion::coordinator_channel().0,
            library_files,
            hash_limiter: crate::auth::password::HashLimiter::new(
                usize::try_from(config.auth_hash_concurrency).unwrap(),
            ),
            config,
            oidc,
            login_limiter: test_support::test_login_limiter(),
            recovery_limiter: test_support::test_login_limiter(),
            oidc_limiter: test_support::test_login_limiter(),
            settings: test_support::test_settings(),
            last_settings_reload: std::sync::Arc::new(tokio::sync::RwLock::new(None)),
        };
        let app = crate::build_router_with_session_store(state, store.clone());
        let mut server = axum_test::TestServer::new(app);
        server.save_cookies();

        // Establish an authenticated session via /auth/oidc/login → /auth/callback.
        let login_resp = server.get("/auth/oidc/login").await;
        let csrf: String = authorization_parameter(&login_resp, "state");
        let nonce: String = authorization_parameter(&login_resp, "nonce");
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
        use tower_sessions::SessionStore;
        use tower_sessions::session::Id as SessionId;

        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;

        let mock = MockOidcProvider::start("").await;
        let oidc = Some(std::sync::Arc::new(
            mock.runtime("http://localhost:3000/auth/callback"),
        ));
        let store = crate::auth::store::PostgresStore::new(app_pool.clone());
        let (config, library_files) = crate::test_support::test_storage_config(
            None,
            crate::models::storage_library::LibraryId::from_uuid(uuid::Uuid::new_v4()),
        );
        let state = AppState {
            pool: app_pool.clone(),
            ingestion_pool,
            ingestion: crate::services::ingestion::coordinator_channel().0,
            library_files,
            hash_limiter: crate::auth::password::HashLimiter::new(
                usize::try_from(config.auth_hash_concurrency).unwrap(),
            ),
            config,
            oidc,
            login_limiter: test_support::test_login_limiter(),
            recovery_limiter: test_support::test_login_limiter(),
            oidc_limiter: test_support::test_login_limiter(),
            settings: test_support::test_settings(),
            last_settings_reload: std::sync::Arc::new(tokio::sync::RwLock::new(None)),
        };
        let app = crate::build_router_with_session_store(state, store.clone());
        let mut server = axum_test::TestServer::new(app);
        server.save_cookies();

        let login_resp = server.get("/auth/oidc/login").await;
        let csrf: String = authorization_parameter(&login_resp, "state");
        let nonce: String = authorization_parameter(&login_resp, "nonce");
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
        let mut server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
        // Persist the session cookie login sets so the follow-up /auth/me is
        // session-authenticated; axum-test drops cookies between requests
        // otherwise, making the read anonymous (401).
        server.save_cookies();

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

        // The local-login session carries through and a non-empty CSRF
        // synchronizer token was minted (same contract as the OIDC callback).
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
        test_support::db::create_adult_with_password(
            &app_pool,
            "known",
            "known@example.com",
            "the real password",
        )
        .await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

        // An unknown email and a wrong password for a real account must be
        // indistinguishable: identical status AND identical body, or the
        // difference is an account-enumeration oracle (CWE-204).
        let unknown = server
            .post("/auth/local/login")
            .json(&serde_json::json!({"email": "ghost@example.com", "password": "anything"}))
            .await;
        let wrong = server
            .post("/auth/local/login")
            .json(&serde_json::json!({"email": "known@example.com", "password": "wrong password"}))
            .await;
        assert_eq!(
            unknown.status_code(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "an unknown email returns a generic 422"
        );
        assert_eq!(
            wrong.status_code(),
            unknown.status_code(),
            "a wrong password returns the same status as an unknown email"
        );
        assert_eq!(
            unknown.text(),
            wrong.text(),
            "unknown-email and wrong-password bodies must be byte-identical"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn local_login_disabled_account_is_generic_422(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
        let id = test_support::db::create_adult_with_password(
            &app_pool,
            "disabled-login",
            "disabled-login@example.com",
            "a perfectly good password",
        )
        .await;
        crate::models::user::disable_account(&app_pool, id)
            .await
            .expect("disable account");
        let mut server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
        server.save_cookies();

        // THREAT (account-state enumeration): a CORRECT password against a
        // disabled account must be indistinguishable from a wrong password —
        // generic 422 and no session established.
        let resp = server
            .post("/auth/local/login")
            .json(&serde_json::json!({
                "email": "disabled-login@example.com",
                "password": "a perfectly good password",
            }))
            .await;
        assert_eq!(
            resp.status_code(),
            StatusCode::UNPROCESSABLE_ENTITY,
            "a disabled account with the right password gets the generic failure"
        );

        let me = server.get("/auth/me").await;
        assert_eq!(
            me.status_code(),
            StatusCode::UNAUTHORIZED,
            "a disabled-account login must not establish a session"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn disabled_user_live_session_is_rejected(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
        let id = test_support::db::create_adult_with_password(
            &app_pool,
            "disabled-session",
            "disabled-session@example.com",
            "a perfectly good password",
        )
        .await;
        let mut server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
        server.save_cookies();

        let login = server
            .post("/auth/local/login")
            .json(&serde_json::json!({
                "email": "disabled-session@example.com",
                "password": "a perfectly good password",
            }))
            .await;
        assert_eq!(login.status_code(), StatusCode::NO_CONTENT);
        assert_eq!(server.get("/auth/me").await.status_code(), StatusCode::OK);

        crate::models::user::disable_account(&app_pool, id)
            .await
            .expect("disable account");

        // The live cookie is now worthless: session rehydration rejects a
        // disabled user on the very next request.
        assert_eq!(
            server.get("/auth/me").await.status_code(),
            StatusCode::UNAUTHORIZED,
            "disabling a user invalidates their live session immediately"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn disabled_user_device_token_is_rejected(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (id, basic) =
            test_support::db::create_adult_and_basic_auth(&app_pool, "disabled-token").await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

        let before = server
            .get("/auth/me")
            .add_header(axum::http::header::AUTHORIZATION, basic.clone())
            .await;
        assert_eq!(
            before.status_code(),
            StatusCode::OK,
            "the device token authenticates before the account is disabled"
        );

        crate::models::user::disable_account(&app_pool, id)
            .await
            .expect("disable account");

        let after = server
            .get("/auth/me")
            .add_header(axum::http::header::AUTHORIZATION, basic)
            .await;
        assert_eq!(
            after.status_code(),
            StatusCode::UNAUTHORIZED,
            "a disabled user's device token is inert on the Basic-auth path"
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
    async fn setup_rate_limits_per_source_before_bootstrap(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
        let server = test_support::db::server_with_source_rate_limit(&app_pool, &ingestion_pool, 1);

        let body = serde_json::json!({
            "email": "admin@example.com",
            "display_name": "Admin",
            "password": "a strong password",
        });

        let first = server
            .post("/auth/setup")
            .add_header("x-forwarded-for", "203.0.113.7")
            .json(&body)
            .await;
        assert_eq!(
            first.status_code(),
            StatusCode::CREATED,
            "the first request from a source is within the limit"
        );

        let second = server
            .post("/auth/setup")
            .add_header("x-forwarded-for", "203.0.113.7")
            .json(&body)
            .await;
        assert_eq!(
            second.status_code(),
            StatusCode::TOO_MANY_REQUESTS,
            "the limit is enforced ahead of the admin_exists fast-reject, so a \
             throttled source sees 429 rather than 409"
        );

        let other_source = server
            .post("/auth/setup")
            .add_header("x-forwarded-for", "203.0.113.8")
            .json(&body)
            .await;
        assert_eq!(
            other_source.status_code(),
            StatusCode::CONFLICT,
            "the limit is keyed per source, so an unthrottled one still reaches \
             the fast-reject"
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

        // A second setup is rejected (409) once an admin exists.
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
    async fn setup_applies_shared_policy_and_exposes_configured_bounds(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
        let mut config = test_support::test_config();
        config.password_min_length = 24;
        config.password_max_length = 64;
        let server = test_support::db::server_with_config(&app_pool, &ingestion_pool, config);
        let status: serde_json::Value = server.get("/auth/setup/status").await.json();
        assert_eq!(status["password_min_length"], 24);
        assert_eq!(status["password_max_length"], 64);
        for password in [
            "strong-enough-but-short".to_owned(),
            "a".repeat(24),
            "x".repeat(65),
        ] {
            let response = server
                .post("/auth/setup")
                .json(&serde_json::json!({
                    "email": "admin@example.com", "display_name": "Admin", "password": password,
                }))
                .await;
            assert_eq!(response.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
            assert!(!crate::models::user::admin_exists(&app_pool).await.unwrap());
        }
        let response = server
            .post("/auth/setup")
            .json(&serde_json::json!({
                "email": "admin@example.com", "display_name": "Admin",
                "password": "correct-horse-battery-staple-7!",
            }))
            .await;
        assert_eq!(response.status_code(), StatusCode::CREATED);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn me_exposes_local_password_independently_of_oidc(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (id, basic) =
            test_support::db::create_adult_and_basic_auth(&app_pool, "capability").await;
        let mut server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
        let me: serde_json::Value = server
            .get("/auth/me")
            .add_header("authorization", &basic)
            .await
            .json();
        assert_eq!(me["has_local_password"], false);
        let hash = crate::auth::password::hash_password(b"old").unwrap();
        crate::models::local_credentials::set_password(&app_pool, id, &hash)
            .await
            .unwrap();
        let me: serde_json::Value = server
            .get("/auth/me")
            .add_header("authorization", &basic)
            .await
            .json();
        assert_eq!(me["has_local_password"], true);

        crate::models::user::create_local(
            &app_pool,
            "local@example.com",
            "Local",
            crate::models::role::Role::Adult,
            Some(&hash),
        )
        .await
        .unwrap();
        server.save_cookies();
        let login = server
            .post("/auth/local/login")
            .json(&serde_json::json!({ "email": "local@example.com", "password": "old" }))
            .await;
        assert_eq!(login.status_code(), StatusCode::NO_CONTENT);
        let me: serde_json::Value = server.get("/auth/me").await.json();
        assert_eq!(me["has_local_password"], true);
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
    async fn a_json_body_missing_a_required_field_is_a_422_problem(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

        let resp = server
            .post("/auth/forgot-password")
            .json(&serde_json::json!({}))
            .await;
        test_support::assert_problem(
            &resp,
            crate::error::problems::INVALID_REQUEST_BODY,
            StatusCode::UNPROCESSABLE_ENTITY,
        );
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
                "new_password": "lithe cobalt meadow drift",
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
                "new_password": "lithe cobalt meadow drift",
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
                "password": "lithe cobalt meadow drift",
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
                "new_password": "amber thistle vault ridge",
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
    async fn reset_password_serializes_with_administrator_reset(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
        let user_id = test_support::db::create_adult_with_password(
            &app_pool,
            "recover-race",
            "recover-race@example.com",
            "old",
        )
        .await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
        server
            .post("/auth/forgot-password")
            .json(&serde_json::json!({"email": "recover-race@example.com"}))
            .await
            .assert_status_ok();
        let pin_path = std::path::Path::new(&test_support::test_config().recovery_pin_dir)
            .join(format!("{user_id}.pin"));
        let contents = std::fs::read_to_string(&pin_path).unwrap();
        let pin = contents
            .lines()
            .find_map(|line| line.strip_prefix("pin: "))
            .unwrap()
            .to_owned();
        let administrator_hash =
            crate::auth::password::hash_password(b"amber thistle vault ridge").unwrap();
        let mut administrator = app_pool.begin().await.unwrap();
        sqlx::query_scalar!(
            "SELECT id FROM users WHERE id = $1 FOR NO KEY UPDATE",
            user_id,
        )
        .fetch_one(&mut *administrator)
        .await
        .unwrap();
        let recovery = tokio::spawn(async move {
            server
                .post("/auth/reset-password")
                .json(&serde_json::json!({
                    "email": "recover-race@example.com",
                    "pin": pin,
                    "new_password": "lithe cobalt meadow drift",
                }))
                .await
        });
        test_support::wait_for_blocked_query(&pool, "%users%", 1).await;
        crate::models::local_credentials::set_password(
            &mut *administrator,
            user_id,
            &administrator_hash,
        )
        .await
        .unwrap();
        crate::models::user::increment_session_version(&mut *administrator, user_id)
            .await
            .unwrap();
        administrator.commit().await.unwrap();
        recovery.await.unwrap().assert_status_ok();
        let credential = crate::models::local_credentials::find_by_user_id(&app_pool, user_id)
            .await
            .unwrap()
            .unwrap();
        assert!(
            crate::auth::password::verify_password(
                b"lithe cobalt meadow drift",
                &credential.password_hash,
            )
            .is_ok()
        );
        assert_eq!(
            crate::models::user::find_by_id(&app_pool, user_id)
                .await
                .unwrap()
                .unwrap()
                .session_version,
            2
        );
        assert!(
            crate::models::password_reset_pin::find_active_by_user(&app_pool, user_id)
                .await
                .unwrap()
                .is_none()
        );
        assert!(!pin_path.exists());
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn reset_password_rejects_a_weak_password(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
        let user_id = test_support::db::create_adult_with_password(
            &app_pool,
            "weakset",
            "weakset@example.com",
            "the old password",
        )
        .await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

        let forgot = server
            .post("/auth/forgot-password")
            .json(&serde_json::json!({"email": "weakset@example.com"}))
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

        // A valid PIN with a policy-failing password is rejected: the recovery
        // path is not a bypass for the strength gate.
        let weak = server
            .post("/auth/reset-password")
            .json(&serde_json::json!({
                "email": "weakset@example.com",
                "pin": &pin,
                "new_password": "password",
            }))
            .await;
        assert_eq!(weak.status_code(), StatusCode::UNPROCESSABLE_ENTITY);

        // The rejection happened before the PIN was consumed, so the same PIN
        // still completes a reset with a strong password.
        let strong = server
            .post("/auth/reset-password")
            .json(&serde_json::json!({
                "email": "weakset@example.com",
                "pin": &pin,
                "new_password": "lithe cobalt meadow drift",
            }))
            .await;
        assert_eq!(
            strong.status_code(),
            StatusCode::OK,
            "the PIN survived the rejected weak attempt"
        );
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
        let me = server.get("/auth/me").await;
        assert_eq!(
            me.status_code(),
            StatusCode::OK,
            "the session authenticates before the reset"
        );
        // csrf_required gates mutating verbs by auth method, and this caller now
        // holds a session, so the recovery POSTs below must carry the token.
        // Anonymous recovery (the common case) is exempt; this stolen-cookie
        // scenario is not.
        let csrf_token = me
            .json::<serde_json::Value>()
            .get("csrf_token")
            .and_then(|v| v.as_str())
            .expect("login mints a csrf token")
            .to_owned();

        // Recover and reset the password. A session that predates the reset (e.g.
        // an attacker's stolen cookie) must not survive it.
        let forgot = server
            .post("/auth/forgot-password")
            .add_header(
                axum::http::HeaderName::from_static("x-csrf-token"),
                axum::http::HeaderValue::from_str(&csrf_token).expect("valid header"),
            )
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
            .add_header(
                axum::http::HeaderName::from_static("x-csrf-token"),
                axum::http::HeaderValue::from_str(&csrf_token).expect("valid header"),
            )
            .json(&serde_json::json!({
                "email": "stale@example.com",
                "pin": pin,
                "new_password": "lithe cobalt meadow drift",
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

    // ---------- POST /auth/register ----------

    #[sqlx::test(migrations = "./migrations")]
    async fn register_creates_adult_when_enabled(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
        let server = test_support::db::server_with_self_registration(&app_pool, &ingestion_pool);

        let r = server
            .post("/auth/register")
            .json(&serde_json::json!({
                "email": "self-reg@example.com",
                "display_name": "Self Reg",
                "password": "correct-horse-battery-staple-7!",
            }))
            .await;
        assert_eq!(r.status_code(), StatusCode::CREATED);

        // A self-registered account is an adult, never an admin or
        // child, and it does not flip the first-admin gate.
        let role: String = sqlx::query_scalar!(
            r#"SELECT role::text AS "role!" FROM users WHERE lower(email) = lower($1)"#,
            "self-reg@example.com",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(role, "adult");
        assert!(
            !crate::models::user::admin_exists(&app_pool).await.unwrap(),
            "self-registration must not create an administrator"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn register_returns_404_when_disabled(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
        // Base config has self_registration_enabled = false.
        let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

        let r = server
            .post("/auth/register")
            .json(&serde_json::json!({
                "email": "nope@example.com",
                "display_name": "Nope",
                "password": "correct-horse-battery-staple-7!",
            }))
            .await;
        assert_eq!(r.status_code(), StatusCode::NOT_FOUND);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn register_duplicate_email_returns_409(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
        let server = test_support::db::server_with_self_registration(&app_pool, &ingestion_pool);

        let body = serde_json::json!({
            "email": "dup-reg@example.com",
            "display_name": "Dup",
            "password": "correct-horse-battery-staple-7!",
        });
        assert_eq!(
            server
                .post("/auth/register")
                .json(&body)
                .await
                .status_code(),
            StatusCode::CREATED
        );
        assert_eq!(
            server
                .post("/auth/register")
                .json(&body)
                .await
                .status_code(),
            StatusCode::CONFLICT
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn register_weak_password_returns_422(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
        let server = test_support::db::server_with_self_registration(&app_pool, &ingestion_pool);

        let r = server
            .post("/auth/register")
            .json(&serde_json::json!({
                "email": "weak-reg@example.com",
                "display_name": "Weak",
                "password": "password",
            }))
            .await;
        assert_eq!(r.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn register_invalid_email_returns_422(pool: sqlx::PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
        let server = test_support::db::server_with_self_registration(&app_pool, &ingestion_pool);

        let r = server
            .post("/auth/register")
            .json(&serde_json::json!({
                "email": "not-an-email",
                "display_name": "Bad Email",
                "password": "correct-horse-battery-staple-7!",
            }))
            .await;
        assert_eq!(r.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
    }
    #[sqlx::test(migrations = "./migrations")]
    async fn credential_link_redemption_audit_success_failure_and_redaction(pool: sqlx::PgPool) {
        use tracing::instrument::WithSubscriber;
        let app = test_support::db::app_pool_for(&pool).await;
        let ingestion = test_support::db::ingestion_pool_for(&pool).await;
        let id = crate::models::user::create_local(
            &app,
            "audit-holder@example.test",
            "Private Holder",
            crate::models::role::Role::Adult,
            None,
        )
        .await
        .unwrap()
        .id;
        let (token, _) = crate::models::credential_link::issue(&app, id)
            .await
            .unwrap();
        let server = test_support::db::server_with_real_pools(&app, &ingestion);
        let logs = test_support::CapturedLogs::default();
        let password = "lithe cobalt meadow drift";
        async {
            for (submitted_token, submitted_password, expected) in [
                ("private-invalid-token", password, StatusCode::UNPROCESSABLE_ENTITY),
                (token.as_str(), "short", StatusCode::UNPROCESSABLE_ENTITY),
                (token.as_str(), password, StatusCode::NO_CONTENT),
                (token.as_str(), password, StatusCode::UNPROCESSABLE_ENTITY),
            ] {
                let response = server.post("/auth/credential-links/redemptions")
                    .add_header(axum::http::header::USER_AGENT, "audit\tclient")
                    .json(&serde_json::json!({"token": submitted_token, "password": submitted_password}))
                    .await;
                assert_eq!(response.status_code(), expected);
            }
        }.with_subscriber(logs.subscriber()).await;
        let captured = logs.text();
        let events: Vec<_> = captured
            .lines()
            .filter(|line| line.contains("action=\"credential_link_redemption\""))
            .collect();
        assert_eq!(events.len(), 4, "{captured}");
        assert_eq!(
            events
                .iter()
                .filter(|line| line.contains("outcome=\"success\""))
                .count(),
            1
        );
        assert_eq!(
            events
                .iter()
                .filter(|line| line.contains("outcome=\"failure\""))
                .count(),
            3
        );
        let mut request_ids = std::collections::HashSet::new();
        for event in events {
            assert!(event.contains("user_id="));
            assert!(event.contains("source_ip="));
            assert!(event.contains("user_agent=\"auditclient\""));
            let request_id = event
                .split("request_id=")
                .nth(1)
                .unwrap()
                .split_whitespace()
                .next()
                .unwrap();
            assert!(uuid::Uuid::parse_str(request_id).is_ok());
            assert!(request_ids.insert(request_id));
        }
        assert!(captured.contains(&id.to_string()));
        for secret in [
            token.as_str(),
            password,
            "short",
            "audit-holder@example.test",
            "Private Holder",
            "private-invalid-token",
            "audit\tclient",
        ] {
            assert!(
                !captured.contains(secret),
                "redemption audit leaked a submitted value"
            );
        }
    }
}
