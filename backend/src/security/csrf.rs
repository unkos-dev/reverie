//! CSRF synchronizer-token validating middleware.
//!
//! # Tier 2 — security-critical
//!
//! Token issuance (already shipped) mints a per-session `csrf_token` at login
//! and exposes it via `GET /auth/me`; the frontend echoes it as `X-CSRF-Token`
//! on mutating requests. This middleware enforces that token on
//! session-authenticated mutating requests.
//!
//! THREAT (CSRF): `SameSite=Lax` cookies do not cover every cross-site mutating
//! vector, so a session-bound synchronizer token is the primary defence. The
//! exemption keys on the AUTH METHOD, not on token presence: only a
//! session-authenticated caller is subject to the check. Keying on "is a
//! csrf_token in the session" would be a bypass (an attacker-shaped request with
//! no token would be exempted). Basic/Bearer callers (OPDS clients) carry no
//! session user and are exempt; pre-auth POSTs (`/auth/local/login`,
//! `/auth/setup`, recovery) likewise carry no session user yet. The comparison
//! is constant-time to avoid leaking the token through timing.

use axum::extract::Request;
use axum::http::Method;
use axum::middleware::Next;
use axum::response::Response;
use subtle::ConstantTimeEq;
use tower_sessions::Session;
use uuid::Uuid;

use crate::auth::session::SESSION_KEY_USER_ID;
use crate::error::AppError;

/// Request header carrying the synchronizer token. Mirrors the frontend
/// `apiFetch` writer (`frontend/src/api/fetch.ts`).
const CSRF_HEADER: &str = "X-CSRF-Token";

/// Session-store key under which the token is minted at login.
const CSRF_SESSION_KEY: &str = "csrf_token";

/// Reject a session-authenticated mutating request whose `X-CSRF-Token` header
/// is missing or does not match the session token. Safe methods and
/// non-session-authenticated callers pass through untouched.
///
/// # Errors
///
/// - [`AppError::CsrfMissing`] (428) when a session-authenticated mutating
///   request carries no token (header or session side).
/// - [`AppError::CsrfMismatch`] (403) when the header is present but does not
///   match the session token.
/// - [`AppError::Internal`] when the session store read fails.
pub async fn csrf_required(
    session: Session,
    request: Request,
    next: Next,
) -> Result<Response, AppError> {
    // Safe methods never mutate state.
    if matches!(
        *request.method(),
        Method::GET | Method::HEAD | Method::OPTIONS | Method::TRACE
    ) {
        return Ok(next.run(request).await);
    }

    // Exemption keys on auth method: only a session-authenticated caller is
    // subject to CSRF. No session user => Basic/Bearer or pre-auth.
    let session_user: Option<Uuid> = session
        .get(SESSION_KEY_USER_ID)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    if session_user.is_none() {
        return Ok(next.run(request).await);
    }

    let header = request
        .headers()
        .get(CSRF_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let Some(header) = header else {
        return Err(AppError::CsrfMissing);
    };
    let stored: Option<String> = session
        .get(CSRF_SESSION_KEY)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    let Some(stored) = stored else {
        return Err(AppError::CsrfMissing);
    };

    if bool::from(header.as_bytes().ct_eq(stored.as_bytes())) {
        Ok(next.run(request).await)
    } else {
        Err(AppError::CsrfMismatch)
    }
}
