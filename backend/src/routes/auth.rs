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
#[expect(
    clippy::struct_excessive_bools,
    reason = "each flag is a separate public capability the SPA reads independently"
)]
struct SetupStatusResponse {
    /// `true` when no administrator exists yet, so the SPA shows the setup form.
    setup_required: bool,
    /// Whether local email+password login is enabled.
    local_auth_enabled: bool,
    /// Whether OIDC is configured (computed from the issuer). Drives
    /// the SPA's provider-aware redirect and the "Sign in with OIDC" action.
    oidc_enabled: bool,
    /// Whether password recovery by email is available: local sign-in and
    /// authentication mail are both enabled.
    email_recovery_enabled: bool,
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
    description = "Returns whether initial setup is required, which sign-in providers are enabled, whether password recovery by email is available, and the configured minimum and maximum new-password lengths. Sign-in, recovery and password-setting forms use this state. No authentication is required to call it.",
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
        email_recovery_enabled: state.config.local_auth_enabled && state.mail.is_some(),
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
/// policy/hash/breach work, as in credential-link redemption. `setup`
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
    description = "Returns the email and password state for a valid administrator or host link, or an emailed password recovery link for an eligible account. Verification links never resolve. Invalid, expired, used, ineligible, disabled-account and no-email links share one generic refusal.",
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
    description = "Sets the account's password once before the link expires, using an administrator or host link or an emailed password recovery link for an eligible account. Deletes every other password-replacement link, invalidates earlier sessions and creates no session. Leaves login throttling and device tokens unchanged.",
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
        let target = crate::models::account_email_proof::lock_account(&mut tx, account.user_id)
            .await
            .map_err(|e| AppError::Internal(e.into()))?
            .ok_or_else(invalid_credential_link)?;
        let consumed = crate::models::credential_link::consume_replacement(
            &mut tx,
            account.user_id,
            &body.token,
        )
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
        if !consumed || target.disabled || target.email.is_none() {
            return Err(invalid_credential_link());
        }
        let had_password =
            crate::models::local_credentials::find_by_user_id(&mut *tx, account.user_id)
                .await
                .map_err(|e| AppError::Internal(e.into()))?
                .is_some();
        crate::models::local_credentials::set_password(&mut *tx, account.user_id, &phc)
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
        // THREAT: Sessions issued before replacement must no longer authenticate.
        user::increment_session_version(&mut *tx, account.user_id)
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
        if let Some(outbox) = crate::services::mail::dispatch::Outbox::new(
            state.mail.as_ref(),
            state.config.public_url.as_ref(),
        )
        .map_err(|e| AppError::Internal(e.into()))?
        {
            let event = if had_password {
                crate::services::mail::render::Notice::PasswordChanged
            } else {
                crate::services::mail::render::Notice::PasswordSet
            };
            outbox
                .notice_to_verified_address(&mut tx, account.user_id, &target, event)
                .await
                .map_err(|e| AppError::Internal(e.into()))?;
        }
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

/// `POST /auth/forgot-password`: start password recovery by email.
///
/// THREAT (codeguard #2): always returns a generic 200 for a valid request,
/// whether or not the address belongs to an eligible account and whether or
/// not mail is configured. The handler does no account lookup and no hashing:
/// it admits at most four background tasks without waiting and returns. Only a
/// task that finds an enabled account with a verified current address and an
/// existing local password writes anything.
///
/// # Errors
/// - [`AppError::NotFound`] when local authentication is disabled.
/// - [`AppError::RateLimited`] (429) when the per-source limit is exceeded.
/// - [`AppError::Internal`] when the mail origin is misconfigured.
#[utoipa::path(
    post,
    path = "/auth/forgot-password",
    summary = "Start password recovery",
    description = "Starts password recovery by email for the given address and always returns the same generic response so it cannot be used to determine whether an account exists or whether email is available. No authentication is required to call it. Returns 404 if local authentication is disabled and 429 when too many requests have been made.",
    tag = "auth",
    security(()),
    request_body = ForgotPasswordRequest,
    responses(
        (status = 200, description = "Recovery email requested if the address belongs to an eligible account (generic; no enumeration)"),
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

    if let Some(outbox) = crate::services::mail::dispatch::Outbox::new(
        state.mail.as_ref(),
        state.config.public_url.as_ref(),
    )
    .map_err(|e| AppError::Internal(e.into()))?
        && user::is_addr_spec(&body.email)
    {
        outbox.spawn_recovery(state.pool, body.email);
    }

    Ok(StatusCode::OK)
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
    async fn logout_returns_204_without_session() {
        let server = test_support::test_server();
        let response = server.post("/auth/logout").await;
        // logout on a non-authenticated session still succeeds (no-op)
        assert_eq!(response.status_code(), StatusCode::NO_CONTENT);
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
            mail: None,
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
                mail: None,
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
            mail: None,
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
            mail: None,
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

    // ---------- email recovery ----------

    mod mail_recovery {
        use super::*;
        use crate::models::account_email_proof::{self, ProofPurpose, lock_account};
        use crate::models::mail_job::MailPurpose;
        use crate::services::mail::dispatch::Outbox;
        use crate::services::mail::render::{self, Context, Notice};
        use crate::test_support::mail::{self, ORIGIN, PASSWORD, SUPPORT};
        use sqlx::PgPool;
        use uuid::Uuid;

        const OLD_PASSWORD: &str = "the old password";

        struct Fixture {
            app: PgPool,
            runtime: std::sync::Arc<crate::services::mail::queue::MailRuntime>,
            server: axum_test::TestServer,
        }

        async fn fixture(pool: &PgPool) -> Fixture {
            let app = test_support::db::app_pool_for(pool).await;
            let ingestion = test_support::db::ingestion_pool_for(pool).await;
            let runtime = mail::shared_runtime(1);
            let server = mail::server(&app, &ingestion, &runtime);
            Fixture {
                app,
                runtime,
                server,
            }
        }

        async fn holder(f: &Fixture, pool: &PgPool, email: &str) -> Uuid {
            let id =
                test_support::db::create_adult_with_password(&f.app, "Holder", email, OLD_PASSWORD)
                    .await;
            mail::verify_address(pool, id).await;
            id
        }

        fn outbox(f: &Fixture) -> Outbox {
            Outbox::new(Some(&f.runtime), Some(&url::Url::parse(ORIGIN).unwrap()))
                .unwrap()
                .unwrap()
        }

        fn context() -> Context {
            Context {
                origin: ORIGIN.to_owned(),
                support: SUPPORT.to_owned(),
            }
        }

        async fn forgot(server: &axum_test::TestServer, email: &str) -> axum_test::TestResponse {
            server
                .post("/auth/forgot-password")
                .json(&serde_json::json!({"email": email}))
                .await
        }

        async fn recovery_token(f: &Fixture, pool: &PgPool, id: Uuid, email: &str) -> String {
            forgot(&f.server, email).await.assert_status_ok();
            mail::wait_for_job(pool, id, "email_recovery").await;
            let stored = mail::messages(pool, id).await;
            let message = stored
                .iter()
                .find(|m| m.purpose == "email_recovery")
                .expect("recovery message");
            mail::token_in(&message.body)
        }

        async fn redeem(
            server: &axum_test::TestServer,
            token: &str,
            password: &str,
        ) -> axum_test::TestResponse {
            server
                .post("/auth/credential-links/redemptions")
                .json(&serde_json::json!({"token": token, "password": password}))
                .await
        }

        fn assert_notice(message: &mail::Stored, event: Notice, email: &str) {
            let candidates: Vec<_> = [0, -1, 1]
                .into_iter()
                .map(|offset| {
                    render::notice(
                        &context(),
                        event,
                        email,
                        chrono::Utc::now() + chrono::TimeDelta::minutes(offset),
                    )
                })
                .collect();
            assert!(
                candidates
                    .iter()
                    .any(|c| c.subject == message.subject && c.body == message.body),
                "unexpected notice: {message:?}"
            );
            assert_eq!(message.recipient, email);
        }

        #[sqlx::test(migrations = "./migrations")]
        async fn smtp_recovery_forgot_password_queues_only_for_eligible_accounts(pool: PgPool) {
            let f = fixture(&pool).await;
            let eligible = holder(&f, &pool, "eligible@example.test").await;
            let unverified = test_support::db::create_adult_with_password(
                &f.app,
                "Unverified",
                "unverified@example.test",
                OLD_PASSWORD,
            )
            .await;
            let disabled = holder(&f, &pool, "disabled@example.test").await;
            user::disable_account(&f.app, disabled).await.unwrap();
            let oidc_only = user::upsert_from_oidc(
                &f.app,
                "https://issuer.example.test",
                "subject-oidc-only",
                "Oidc",
                Some("oidc-only@example.test"),
            )
            .await
            .unwrap()
            .id;
            mail::verify_address(&pool, oidc_only).await;

            let mut bodies = Vec::new();
            for email in [
                "unverified@example.test",
                "disabled@example.test",
                "oidc-only@example.test",
                "ghost@example.test",
                "not an address",
                "ELIGIBLE@example.test",
            ] {
                let response = forgot(&f.server, email).await;
                assert_eq!(response.status_code(), StatusCode::OK, "{email}");
                bodies.push(response.text());
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
            assert!(bodies.iter().all(String::is_empty));

            mail::wait_for_job(&pool, eligible, "email_recovery").await;
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
            for other in [unverified, disabled, oidc_only] {
                assert_eq!(mail::job_count(&pool, other, None).await, 0);
            }
            assert_eq!(
                sqlx::query_scalar!(r#"SELECT count(*) AS "n!" FROM mail_jobs"#)
                    .fetch_one(&pool)
                    .await
                    .unwrap(),
                1
            );
            assert_eq!(
                sqlx::query_scalar!(r#"SELECT count(*) AS "n!" FROM account_email_proofs"#)
                    .fetch_one(&pool)
                    .await
                    .unwrap(),
                1
            );

            let stored = mail::messages(&pool, eligible).await;
            let [message] = stored.as_slice() else {
                panic!("expected one message, got {stored:?}");
            };
            assert_eq!(message.purpose, "email_recovery");
            assert_eq!(message.recipient, "eligible@example.test");
            let token = mail::token_in(&message.body);
            assert_eq!(token.len(), 43);
            let proof = sqlx::query!(
                "SELECT token_hash, expires_at, email FROM account_email_proofs WHERE user_id = $1 AND purpose = 'recovery'",
                eligible
            )
            .fetch_one(&pool)
            .await
            .unwrap();
            assert_eq!(proof.token_hash, account_email_proof::digest(&token));
            assert_eq!(proof.email, "eligible@example.test");
            let expected = render::recovery(
                &context(),
                "eligible@example.test",
                &token,
                proof.expires_at,
            );
            assert_eq!(message.subject, expected.subject);
            assert_eq!(message.body, expected.body);
            assert!(
                message
                    .body
                    .contains(&format!("{ORIGIN}/reset-password#{token}"))
            );
            let lifetime = proof.expires_at - chrono::Utc::now();
            assert!(
                lifetime > chrono::TimeDelta::minutes(14)
                    && lifetime <= chrono::TimeDelta::minutes(15)
            );
        }

        #[sqlx::test(migrations = "./migrations")]
        async fn smtp_recovery_emailed_link_resets_the_password_without_signing_in(pool: PgPool) {
            let f = fixture(&pool).await;
            let id = holder(&f, &pool, "recover@example.test").await;
            let token = recovery_token(&f, &pool, id, "recover@example.test").await;
            let lookup = f
                .server
                .post("/auth/credential-links/lookups")
                .json(&serde_json::json!({"token": token}))
                .await;
            assert_eq!(lookup.status_code(), StatusCode::OK);
            assert_eq!(lookup.headers()["cache-control"], "no-store");
            assert_eq!(
                lookup.json::<serde_json::Value>(),
                serde_json::json!({"email": "recover@example.test", "has_password": true})
            );

            let mut tx = f.app.begin().await.unwrap();
            lock_account(&mut tx, id).await.unwrap();
            let verification = account_email_proof::issue(
                &mut tx,
                id,
                ProofPurpose::Verification,
                "recover@example.test",
            )
            .await
            .unwrap();
            tx.commit().await.unwrap();
            let lookup = f
                .server
                .post("/auth/credential-links/lookups")
                .json(&serde_json::json!({"token": verification.token}))
                .await;
            assert_eq!(lookup.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
            let attempt = redeem(&f.server, &verification.token, PASSWORD).await;
            assert_eq!(attempt.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
            assert!(
                mail::proof_live(&pool, id, ProofPurpose::Verification.as_str()).await,
                "a verification token never consumes anything on the reset path"
            );

            let failed = f
                .server
                .post("/auth/local/login")
                .json(&serde_json::json!({"email": "recover@example.test", "password": "wrong password"}))
                .await;
            assert_eq!(failed.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
            let before_throttle =
                sqlx::query_scalar!(r#"SELECT count(*) AS "n!" FROM local_login_throttle"#)
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            assert!(
                before_throttle > 0,
                "the failed sign-in must leave throttle state to preserve"
            );
            let response = redeem(&f.server, &token, PASSWORD).await;
            assert_eq!(response.status_code(), StatusCode::NO_CONTENT);
            assert_eq!(
                sqlx::query_scalar!(r#"SELECT count(*) AS "n!" FROM local_login_throttle"#)
                    .fetch_one(&pool)
                    .await
                    .unwrap(),
                before_throttle,
                "redemption leaves login throttling alone"
            );
            assert_eq!(
                f.server.get("/auth/me").await.status_code(),
                StatusCode::UNAUTHORIZED,
                "redemption signs nobody in"
            );
            assert_eq!(
                redeem(&f.server, &token, PASSWORD).await.status_code(),
                StatusCode::UNPROCESSABLE_ENTITY,
                "a recovery link works once"
            );
            let old = f
                .server
                .post("/auth/local/login")
                .json(
                    &serde_json::json!({"email": "recover@example.test", "password": OLD_PASSWORD}),
                )
                .await;
            assert_eq!(old.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
            let new = f
                .server
                .post("/auth/local/login")
                .json(&serde_json::json!({"email": "recover@example.test", "password": PASSWORD}))
                .await;
            assert_eq!(new.status_code(), StatusCode::NO_CONTENT);
            let stored = mail::messages(&pool, id).await;
            let kinds: Vec<_> = stored.iter().map(|m| m.purpose.as_str()).collect();
            assert!(!kinds.contains(&"email_recovery"), "{kinds:?}");
            let notice = stored
                .iter()
                .find(|m| m.purpose == "notice_password_changed")
                .expect("password changed notice");
            assert_notice(notice, Notice::PasswordChanged, "recover@example.test");
            assert_eq!(
                sqlx::query_scalar!(
                    "SELECT state FROM mail_jobs WHERE user_id = $1 AND purpose = 'email_recovery'",
                    id
                )
                .fetch_one(&pool)
                .await
                .unwrap(),
                "cancelled"
            );
        }

        #[sqlx::test(migrations = "./migrations")]
        async fn smtp_recovery_redemption_invalidates_sessions(pool: PgPool) {
            let f = fixture(&pool).await;
            let id = holder(&f, &pool, "stale-mail@example.test").await;
            let token = recovery_token(&f, &pool, id, "stale-mail@example.test").await;
            let csrf = mail::login(&f.server, "stale-mail@example.test", OLD_PASSWORD).await;
            assert_eq!(f.server.get("/auth/me").await.status_code(), StatusCode::OK);
            let version = user::find_by_id(&f.app, id)
                .await
                .unwrap()
                .unwrap()
                .session_version;

            let (name, value) = mail::csrf(&csrf);
            let response = f
                .server
                .post("/auth/credential-links/redemptions")
                .add_header(name, value)
                .json(&serde_json::json!({"token": token, "password": PASSWORD}))
                .await;
            assert_eq!(response.status_code(), StatusCode::NO_CONTENT);
            assert_eq!(
                f.server.get("/auth/me").await.status_code(),
                StatusCode::UNAUTHORIZED,
                "a session established before the emailed reset is invalidated by it"
            );
            assert_eq!(
                user::find_by_id(&f.app, id)
                    .await
                    .unwrap()
                    .unwrap()
                    .session_version,
                version + 1
            );
        }

        #[sqlx::test(migrations = "./migrations")]
        async fn smtp_recovery_redemption_withdraws_every_other_replacement_proof(pool: PgPool) {
            let f = fixture(&pool).await;
            let outbox = outbox(&f);
            let email = "withdraw@example.test";
            let id = holder(&f, &pool, email).await;

            for winner_is_email in [true, false] {
                let mut tx = f.app.begin().await.unwrap();
                lock_account(&mut tx, id).await.unwrap();
                let (link, expiry) = crate::models::credential_link::issue(
                    &mut tx,
                    id,
                    crate::models::credential_link::CREDENTIAL_LINK_TTL,
                )
                .await
                .unwrap();
                let admin_job = outbox
                    .admin_link(&mut tx, id, email, true, &link, expiry)
                    .await
                    .unwrap();
                tx.commit().await.unwrap();
                let recovery = recovery_token(&f, &pool, id, email).await;
                let mut tx = f.app.begin().await.unwrap();
                lock_account(&mut tx, id).await.unwrap();
                outbox
                    .request_proof(&mut tx, id, ProofPurpose::Verification, email)
                    .await
                    .unwrap();
                tx.commit().await.unwrap();
                let recovery_job =
                    mail::wait_for_job(&pool, id, MailPurpose::EmailRecovery.as_str()).await;

                let token = if winner_is_email { &recovery } else { &link };
                let password = if winner_is_email {
                    "amber thistle vault ridge"
                } else {
                    PASSWORD
                };
                assert_eq!(
                    redeem(&f.server, token, password).await.status_code(),
                    StatusCode::NO_CONTENT
                );

                assert_eq!(
                    sqlx::query_scalar!(
                        r#"SELECT count(*) AS "n!" FROM credential_links WHERE user_id = $1"#,
                        id
                    )
                    .fetch_one(&pool)
                    .await
                    .unwrap(),
                    0
                );
                assert!(!mail::proof_live(&pool, id, ProofPurpose::Recovery.as_str()).await);
                assert!(mail::proof_live(&pool, id, ProofPurpose::Verification.as_str()).await);
                assert_eq!(mail::state_of(&pool, admin_job).await, "cancelled");
                assert!(!mail::payload_present(&pool, admin_job).await);
                assert_eq!(mail::state_of(&pool, recovery_job).await, "cancelled");
                assert!(!mail::payload_present(&pool, recovery_job).await);
                let verification_job = mail::messages(&pool, id)
                    .await
                    .into_iter()
                    .find(|m| m.purpose == "verification")
                    .expect("the verification challenge and its mail survive");
                assert_eq!(verification_job.state, "pending");
                sqlx::query!("DELETE FROM mail_jobs WHERE user_id = $1", id)
                    .execute(&pool)
                    .await
                    .unwrap();
                sqlx::query!("DELETE FROM account_email_proofs WHERE user_id = $1", id)
                    .execute(&pool)
                    .await
                    .unwrap();
            }
        }

        #[sqlx::test(migrations = "./migrations")]
        async fn smtp_recovery_redemption_rechecks_eligibility_after_the_account_lock(
            pool: PgPool,
        ) {
            let f = std::sync::Arc::new(fixture(&pool).await);
            let changes = ["disabled", "unverified", "address changed", "expired"];
            for (index, label) in changes.into_iter().enumerate() {
                let email = format!("late-{index}@example.test");
                let id = holder(&f, &pool, &email).await;
                let token = recovery_token(&f, &pool, id, &email).await;
                let mut held = f.app.begin().await.unwrap();
                lock_account(&mut held, id).await.unwrap().unwrap();
                let runner = std::sync::Arc::clone(&f);
                let submitted = token.clone();
                let redemption =
                    tokio::spawn(async move { redeem(&runner.server, &submitted, PASSWORD).await });
                test_support::wait_for_blocked_query(&pool, "%users%", 1).await;
                match label {
                    "disabled" => {
                        sqlx::query!("UPDATE users SET disabled_at = now() WHERE id = $1", id)
                            .execute(&mut *held)
                            .await
                            .unwrap();
                    }
                    "unverified" => {
                        sqlx::query!(
                            "UPDATE users SET email_verified_at = NULL WHERE id = $1",
                            id
                        )
                        .execute(&mut *held)
                        .await
                        .unwrap();
                    }
                    "address changed" => {
                        sqlx::query!(
                            "UPDATE users SET email = 'moved@example.test' WHERE id = $1",
                            id
                        )
                        .execute(&mut *held)
                        .await
                        .unwrap();
                    }
                    _ => {
                        sqlx::query!(
                            "UPDATE account_email_proofs SET expires_at = clock_timestamp() \
                              WHERE user_id = $1",
                            id
                        )
                        .execute(&mut *held)
                        .await
                        .unwrap();
                    }
                }
                held.commit().await.unwrap();
                let response = redemption.await.unwrap();
                assert_eq!(
                    response.status_code(),
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "{label}"
                );
                let credential = crate::models::local_credentials::find_by_user_id(&f.app, id)
                    .await
                    .unwrap()
                    .unwrap();
                assert!(
                    crate::auth::password::verify_password(
                        OLD_PASSWORD.as_bytes(),
                        &credential.password_hash
                    )
                    .is_ok(),
                    "{label}: the password is unchanged"
                );
            }
        }

        #[sqlx::test(migrations = "./migrations")]
        async fn smtp_recovery_concurrent_redemption_has_one_winner(pool: PgPool) {
            let f = fixture(&pool).await;
            let id = holder(&f, &pool, "race@example.test").await;
            let token = recovery_token(&f, &pool, id, "race@example.test").await;
            let version = user::find_by_id(&f.app, id)
                .await
                .unwrap()
                .unwrap()
                .session_version;
            let (a, b) = tokio::join!(
                redeem(&f.server, &token, PASSWORD),
                redeem(&f.server, &token, "amber thistle vault ridge"),
            );
            let mut statuses = [a.status_code(), b.status_code()];
            statuses.sort();
            assert_eq!(
                statuses,
                [StatusCode::NO_CONTENT, StatusCode::UNPROCESSABLE_ENTITY]
            );
            assert_eq!(
                mail::job_count(&pool, id, Some("notice_password_changed")).await,
                1
            );
            assert_eq!(
                user::find_by_id(&f.app, id)
                    .await
                    .unwrap()
                    .unwrap()
                    .session_version,
                version + 1
            );
        }

        #[sqlx::test(migrations = "./migrations")]
        async fn smtp_recovery_redemption_rolls_back_when_the_notice_cannot_be_queued(
            pool: PgPool,
        ) {
            let f = fixture(&pool).await;
            let id = holder(&f, &pool, "rollback@example.test").await;
            let token = recovery_token(&f, &pool, id, "rollback@example.test").await;
            let version = user::find_by_id(&f.app, id)
                .await
                .unwrap()
                .unwrap()
                .session_version;
            mail::break_enqueue(&pool).await;
            let response = redeem(&f.server, &token, PASSWORD).await;
            assert_eq!(response.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
            let credential = crate::models::local_credentials::find_by_user_id(&f.app, id)
                .await
                .unwrap()
                .unwrap();
            assert!(
                crate::auth::password::verify_password(
                    OLD_PASSWORD.as_bytes(),
                    &credential.password_hash
                )
                .is_ok()
            );
            assert_eq!(
                user::find_by_id(&f.app, id)
                    .await
                    .unwrap()
                    .unwrap()
                    .session_version,
                version
            );
            assert!(mail::proof_live(&pool, id, ProofPurpose::Recovery.as_str()).await);
        }

        #[sqlx::test(migrations = "./migrations")]
        async fn smtp_recovery_raw_logs_carry_no_secrets(pool: PgPool) {
            let logs = test_support::CapturedLogs::default();
            let _guard = tracing::subscriber::set_default(logs.trace_subscriber());
            let f = fixture(&pool).await;
            let email = "private-logs@example.test";
            let id = holder(&f, &pool, email).await;
            let token = recovery_token(&f, &pool, id, email).await;
            assert_eq!(
                redeem(&f.server, &token, PASSWORD).await.status_code(),
                StatusCode::NO_CONTENT
            );
            forgot(&f.server, "unknown-private@example.test")
                .await
                .assert_status_ok();
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            let captured = logs.text();
            assert!(
                captured.contains("credential_link_redemption"),
                "the capture must see the request path: {captured}"
            );
            for secret in [
                token.as_str(),
                PASSWORD,
                OLD_PASSWORD,
                email,
                "unknown-private@example.test",
                "Reset your Reverie password",
            ] {
                assert!(
                    !captured.contains(secret),
                    "a log line leaked a secret value"
                );
            }
        }
    }
}
