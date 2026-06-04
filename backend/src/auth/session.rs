//! First-party session login / logout helpers on [`tower_sessions::Session`].
//!
//! Replaces the abandoned `axum-login` crate's `AuthSession::login`/`logout`
//! (ADR `2026-06-04-first-party-session-layer.md`). Per-request user
//! rehydration lives in [`crate::auth::middleware::CurrentUser`].
//!
//! # Tier 2 — security-critical
//!
//! THREAT (session fixation): [`login`] rotates the session id via
//! [`Session::cycle_id`] before persisting identity, so a pre-auth attacker who
//! plants a known session id cannot have it become authenticated post-login.
//! Identity is stored as two server-side keys (`user_id`, `session_version`);
//! the cookie carries only the random id, so the values are trusted on read.
//! Force-logout rides on `session_version`: bumping `users.session_version`
//! makes the stored value stale, which [`crate::auth::middleware::CurrentUser`]
//! rejects on the next request.

use tower_sessions::Session;

use crate::models::user::User;

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
    session.insert("user_id", user.id).await?;
    session
        .insert("session_version", user.session_version)
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
