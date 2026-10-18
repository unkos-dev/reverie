//! First-party session login / logout helpers on [`tower_sessions::Session`].
//!
//! Replaces the abandoned `axum-login` crate's `AuthSession::login`/`logout`
//! (ADR `docs/adr/0015-first-party-session-layer-on-the-tower-sessions-core.md`). Per-request user
//! rehydration lives in [`crate::auth::middleware::CurrentUser`].
//!
//! # Tier 2 — security-critical
//!
//! THREAT (session fixation): [`login`](crate::auth::session::login) rotates the session id via
//! [`Session::cycle_id`](tower_sessions::Session::cycle_id) before persisting
//! identity, so a pre-auth attacker who
//! plants a known session id cannot have it become authenticated post-login.
//! Identity is stored as two server-side keys (`user_id`, `session_version`);
//! the cookie carries only the random id, so the values are trusted on read.
//! Force-logout rides on `session_version`: bumping `users.session_version`
//! makes the stored value stale, which [`crate::auth::middleware::CurrentUser`]
//! rejects on the next request.

use tower_sessions::Session;

use crate::models::user::User;

/// Session-store key holding the authenticated user's UUID.
///
/// Single source of truth shared by [`login`] (write) and
/// [`crate::auth::middleware::CurrentUser`] (read) — a one-sided rename would
/// otherwise silently force every session to fail rehydration.
pub(crate) const SESSION_KEY_USER_ID: &str = "user_id";

/// Session-store key holding the `session_version` snapshot captured at login.
///
/// Shared write/read constant; see [`SESSION_KEY_USER_ID`] for the rationale.
pub(crate) const SESSION_KEY_SESSION_VERSION: &str = "session_version";

/// Log `user` into `session`: rotate the id (fixation defence), then persist
/// the identity claims read back per-request by
/// [`crate::auth::middleware::CurrentUser`].
///
/// # Errors
///
/// Returns [`tower_sessions::session::Error`] if id rotation or either insert
/// fails (store I/O or serialization).
pub async fn login(session: &Session, user: &User) -> Result<(), tower_sessions::session::Error> {
    session.cycle_id().await?;
    session.insert(SESSION_KEY_USER_ID, user.id).await?;
    session
        .insert(SESSION_KEY_SESSION_VERSION, user.session_version)
        .await?;
    Ok(())
}

/// Log out: clear session data and delete the row server-side via
/// [`Session::flush`]. The browser's stale cookie then points at nothing.
///
/// # Errors
///
/// Returns [`tower_sessions::session::Error`] if the store delete fails.
pub async fn logout(session: &Session) -> Result<(), tower_sessions::session::Error> {
    session.flush().await
}

/// Rehydrated identity state, preserving the disabled-account refusal for protected routes.
pub enum SessionUser {
    /// Live identity with a matching session version.
    SignedIn(User),
    /// Disabled identity whose session has been flushed.
    Disabled,
    /// No identity, or a deleted or stale identity whose session has been flushed.
    NotSignedIn,
}

/// Resolve a session identity and flush identities that are no longer valid.
///
/// # Errors
/// Returns storage or database errors while reading the identity.
pub async fn signed_in_user(
    session: &Session,
    pool: &sqlx::PgPool,
) -> Result<SessionUser, crate::error::AppError> {
    use crate::error::AppError;
    use crate::models::user;
    let Some(user_id) = session
        .get::<uuid::Uuid>(SESSION_KEY_USER_ID)
        .await
        .map_err(|e| AppError::Internal(e.into()))?
    else {
        return Ok(SessionUser::NotSignedIn);
    };
    let user = user::find_by_id(pool, user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    // THREAT: disabled accounts cannot re-enter through a live session.
    if let Some(u) = &user
        && u.disabled_at.is_some()
    {
        if let Err(e) = session.flush().await {
            tracing::warn!(error = %e, "session flush failed (disabled account)");
        }
        return Ok(SessionUser::Disabled);
    }
    // THREAT: a stale session version cannot survive a forced logout.
    let stored_version = session
        .get::<i32>(SESSION_KEY_SESSION_VERSION)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    if let Some(user) = user
        && stored_version == Some(user.session_version)
    {
        return Ok(SessionUser::SignedIn(user));
    }
    if let Err(e) = session.flush().await {
        tracing::warn!(error = %e, "session flush failed (force-logout / deleted user)");
    }
    Ok(SessionUser::NotSignedIn)
}
