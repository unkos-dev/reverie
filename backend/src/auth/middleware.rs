//! `CurrentUser` extractor and Basic-auth verification for Reverie.
//!
//! [`crate::auth::middleware::CurrentUser`] is the primary identity extractor used by route handlers.
//! It resolves the caller in two steps: session cookie first (rehydrated from
//! the first-party [`tower_sessions::Session`] — read `user_id`, reload the user,
//! compare `session_version`), Basic auth second (via
//! [`crate::auth::middleware::verify_basic`]). Handlers that receive a `CurrentUser` are guaranteed
//! an authenticated identity; unauthenticated requests are rejected with
//! `AppError::Unauthorized` before the handler body runs.
//!
//! # Tier 2 — security-critical
//!
//! This module is the authentication seam for every non-public route.
//! Threat annotations mark the timing-side-channel mitigations in
//! [`crate::auth::middleware::verify_basic`], the `session_version` force-logout
//! check, and the role-assertion invariants on [`crate::auth::middleware::CurrentUser`].

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use base64ct::Encoding;
use tower_sessions::Session;
use uuid::Uuid;

use crate::error::AppError;
use crate::models::role::Role;
use crate::models::{device_token, user};
use crate::state::AppState;

/// Resolved identity for an authenticated request.
///
/// Extracted from the request by [`FromRequestParts`]. Resolution order:
/// session cookie (rehydrated from [`tower_sessions::Session`]) →
/// `Authorization: Basic` (via [`verify_basic`]). Returns
/// [`AppError::Unauthorized`] if neither path yields a valid identity.
///
/// Role-assertion methods ([`require_admin`](CurrentUser::require_admin),
/// [`require_not_child`](CurrentUser::require_not_child)) are the canonical
/// way for handlers to enforce access control; callers must not read `role`
/// or `is_child` and implement their own checks.
#[derive(Debug, Clone)]
pub struct CurrentUser {
    /// Database UUID of the authenticated user.
    pub user_id: Uuid,

    /// Access-control role assigned to this user.
    ///
    /// Use [`require_admin`](CurrentUser::require_admin) rather than matching
    /// directly — keeps role-assertion logic in one place and simplifies future
    /// role-model changes.
    pub role: Role,

    /// Whether this account is flagged as a child profile.
    ///
    /// Use [`require_not_child`](CurrentUser::require_not_child) for access
    /// control rather than reading this field directly.
    pub is_child: bool,
}

impl CurrentUser {
    /// Return `Err(Forbidden)` unless the user is an admin.
    ///
    /// Role-assertion invariant: callers that gate on admin must use this
    /// method. Directly matching `self.role == Role::Admin` bypasses the
    /// single point of enforcement and will not automatically extend to
    /// future role-model changes.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Forbidden`] when `self.role` is not [`Role::Admin`].
    pub const fn require_admin(&self) -> Result<(), AppError> {
        if matches!(self.role, Role::Admin) {
            Ok(())
        } else {
            Err(AppError::Forbidden)
        }
    }

    /// Return `Err(Forbidden)` for child accounts. Adult and admin pass.
    ///
    /// Used to gate metadata/enrichment endpoints that should not be visible
    /// to children.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Forbidden`] when `self.is_child` is `true`.
    #[allow(dead_code)] // wired up by Step 7 tasks 25/26 (metadata + enrichment routes)
    pub const fn require_not_child(&self) -> Result<(), AppError> {
        if self.is_child {
            Err(AppError::Forbidden)
        } else {
            Ok(())
        }
    }
}

/// Verify an `Authorization: Basic <b64>` header against the device-token
/// registry. Shared by [`CurrentUser`] (cookie-or-Basic) and
/// [`crate::auth::basic_only::BasicOnly`] (Basic-only).
///
/// Timing-side-channel mitigation: all tokens for the user are iterated in
/// full before returning a match result — see `// THREAT:` inline below.
///
/// Returns `Ok(Some(user))` when Basic credentials validate, `Ok(None)` when
/// no `Authorization: Basic ...` is present, and `Err(Unauthorized)` when a
/// Basic header is present but credentials are malformed or don't match any
/// active token. Side-effect: schedules an async `update_last_used` write
/// (SQL-side debounced to at most one UPDATE per token per 5 minutes).
///
/// # Errors
///
/// Returns [`AppError::Unauthorized`] when the `Authorization: Basic` header
/// is present but the credentials are malformed, the user UUID is unknown, or
/// no stored token matches. Returns [`AppError::Internal`] on database errors.
pub async fn verify_basic(
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

    // THREAT: early-exit on first match would leak the token's position in the
    // list via response timing, allowing an attacker to narrow guesses to
    // recently-issued tokens. Iterating all tokens in full — combined with
    // constant-time comparison of the SHA-256 hex digests (performed via
    // `subtle::ConstantTimeEq` inside `token::verify_device_token`) — closes
    // this side-channel. Only the digest comparison is constant-time; the
    // SHA-256 computation itself is not a cryptographic constant-time
    // primitive and its wall-clock cost grows with input length, which is
    // attacker-controlled here (`password` comes from the Basic credentials).
    // This mitigation targets secret-dependent early-exit in the
    // comparison/match step, not input-length-driven timing variance.
    // `matched_token_id` is overwritten on each match so only the last
    // matching token wins. Note that `device_tokens.token_hash` carries no DB
    // `UNIQUE` constraint, so uniqueness is not a structural/DB invariant;
    // SHA-256 collision resistance (2^256 work factor) makes duplicate hashes
    // cryptographically infeasible in practice, so the overwrite is a
    // belt-and-braces choice that avoids conditional branching on match count
    // rather than a load-bearing invariant.
    let mut matched_token_id = None;
    for token in &tokens {
        if crate::auth::token::verify_device_token(password, &token.token_hash) {
            matched_token_id = Some(token.id);
        }
    }

    let token_id = matched_token_id.ok_or(AppError::Unauthorized)?;
    let pool = state.pool.clone();
    tokio::spawn(async move {
        if let Err(e) = device_token::update_last_used(&pool, token_id).await {
            tracing::warn!(
                error = %e,
                %token_id,
                "device_token: update_last_used failed (non-fatal)"
            );
        }
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
        // Session cookie path. The `Session` is populated by
        // `SessionManagerLayer`; on a request with no session cookie it is
        // empty and `get("user_id")` is `None`, so we fall through cleanly
        // (no DB hit, no 500) to the Basic-auth path below.
        if let Ok(session) = Session::from_request_parts(parts, state).await
            && let Some(user_id) = session
                .get::<Uuid>("user_id")
                .await
                .map_err(|e| AppError::Internal(e.into()))?
            && let Some(user) = user::find_by_id(&state.pool, user_id)
                .await
                .map_err(|e| AppError::Internal(e.into()))?
        {
            // THREAT (force-logout): the session stores the `session_version`
            // captured at login; if `users.session_version` has since been
            // bumped (role change, security event) the stored copy is stale and
            // the session is rejected. Plain `==` is safe here — session
            // contents are server-side state, not attacker-controlled (the
            // cookie carries only the random session id).
            let stored_version = session
                .get::<i32>("session_version")
                .await
                .map_err(|e| AppError::Internal(e.into()))?;
            if stored_version == Some(user.session_version) {
                return Ok(Self {
                    user_id: user.id,
                    role: user.role,
                    is_child: user.is_child,
                });
            }
            // Stale version: wipe the session row server-side, then fall
            // through. No silent discard (backend/CLAUDE.md): log on failure.
            if let Err(e) = session.flush().await {
                tracing::warn!(error = %e, "force-logout flush failed");
            }
        }

        // Fall back to Basic auth
        if let Some(user) = verify_basic(state, parts).await? {
            return Ok(user);
        }

        Err(AppError::Unauthorized)
    }
}
