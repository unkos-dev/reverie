//! `CurrentUser` extractor and Bearer/Basic verification for Reverie.
//!
//! [`crate::auth::middleware::CurrentUser`] is the primary identity extractor used by route handlers.
//! It resolves the caller in three steps: session cookie first (rehydrated from
//! the first-party [`tower_sessions::Session`] — read `user_id`, reload the user,
//! compare `session_version`), then `Authorization: Basic` (via
//! [`verify_basic`]), then `Authorization: Bearer` (via [`verify_bearer`]).
//! Basic and Bearer both resolve through [`resolve_device_token`], a single
//! indexed lookup on the unified `{prefix}{token_id}.{secret}` credential
//! (see `auth::token::TOKEN_PREFIX`). Handlers that receive a `CurrentUser`
//! are guaranteed an authenticated identity; unauthenticated requests are
//! rejected with `AppError::Unauthorized` before the handler body runs.
//!
//! # Tier 2 — security-critical
//!
//! This module is the authentication seam for every non-public route.
//! Inline `// THREAT:` annotations mark the account-lockout and
//! `session_version` force-logout checks, and the retirement of the old
//! per-user-scan timing mitigation in [`resolve_device_token`]. The
//! role/scope-assertion contract on [`CurrentUser`] is enforced structurally
//! (private `role`/`is_child`/`scopes` fields, access only via the
//! `require_*` methods) and documented in `///` prose on those items.

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use base64ct::Encoding;
use tower_sessions::Session;
use uuid::Uuid;

use crate::auth::scope::Scope;
use crate::auth::session::{SESSION_KEY_SESSION_VERSION, SESSION_KEY_USER_ID};
use crate::auth::token;
use crate::error::AppError;
use crate::models::role::Role;
use crate::models::{device_token, user};
use crate::state::AppState;

/// Resolved identity for an authenticated request.
///
/// Extracted from the request by [`FromRequestParts`]. Resolution order:
/// session cookie (rehydrated from [`tower_sessions::Session`]) →
/// `Authorization: Basic` (via [`verify_basic`]) → `Authorization: Bearer`
/// (via [`verify_bearer`]). Returns [`AppError::Unauthorized`] if no path
/// yields a valid identity.
///
/// Role/scope-assertion methods ([`require_admin`](CurrentUser::require_admin),
/// [`require_not_child`](CurrentUser::require_not_child),
/// [`require_scope`](CurrentUser::require_scope)) are the canonical way for
/// handlers to enforce access control. `role`, `is_child`, and `scopes` are
/// private precisely so they cannot be read directly — the assertion methods
/// are the single, compile-enforced point of access control, which keeps the
/// logic in one place and survives future role-model changes.
#[derive(Debug, Clone)]
pub struct CurrentUser {
    /// Database UUID of the authenticated user.
    pub user_id: Uuid,

    /// Access-control role assigned to this user. Private — gate via
    /// [`require_admin`](CurrentUser::require_admin).
    role: Role,

    /// Whether this account is flagged as a child profile. Private — gate via
    /// [`require_not_child`](CurrentUser::require_not_child).
    is_child: bool,

    /// Capabilities this credential carries. A session derives the full
    /// role-ceiling set ([`Scope::for_role`]); a personal token carries the
    /// explicit scopes chosen at mint. Private — gate via
    /// [`require_scope`](CurrentUser::require_scope) /
    /// [`require_scopes`](CurrentUser::require_scopes).
    scopes: Vec<Scope>,
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

    /// Return `Err(Forbidden)` unless this credential carries `needed`.
    ///
    /// Every session derives at least `{read, write}` from role (see
    /// [`Scope::for_role`]), so a session is never blocked by this check on a
    /// mutation — only a narrowed personal token can lack `write`. Child
    /// restrictions come from
    /// [`require_not_child`](CurrentUser::require_not_child), not from
    /// withholding scope.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Forbidden`] when `needed` is not in `self.scopes`.
    pub fn require_scope(&self, needed: Scope) -> Result<(), AppError> {
        if self.scopes.contains(&needed) {
            Ok(())
        } else {
            Err(AppError::Forbidden)
        }
    }

    /// All-of check for endpoints requiring multiple scopes. An admin
    /// mutation needs `write` AND `admin` — it is both a mutation and
    /// administrative, so `admin` alone would let a `[read, admin]` audit
    /// token mutate admin endpoints (see `auth::scope` module docs). Keeps
    /// the call site one line and the required set aligned with the OpenAPI
    /// `security(...)` array.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Forbidden`] when any scope in `needed` is missing
    /// from `self.scopes`.
    pub fn require_scopes(&self, needed: &[Scope]) -> Result<(), AppError> {
        if needed.iter().all(|s| self.scopes.contains(s)) {
            Ok(())
        } else {
            Err(AppError::Forbidden)
        }
    }

    /// Whether this user's role permits minting a token carrying `scope`
    /// ([`Scope::grantable_by`] — a non-admin cannot mint an admin-scoped
    /// token). Narrow accessor for the mint-time ceiling check; `role`
    /// itself stays private.
    pub const fn may_grant_scope(&self, scope: Scope) -> bool {
        scope.grantable_by(self.role)
    }
}

/// Resolve a device-token credential shared by [`verify_basic`] and
/// [`verify_bearer`]: `prefixed_id` is `{prefix}{token_id}` (the Basic
/// username field or the Bearer value's segment before `.`), `secret` is the
/// token plaintext.
///
/// One indexed [`device_token::find_by_id`] lookup — no per-user scan.
///
/// THREAT (timing side-channel, retired): the earlier implementation
/// iterated every token for the claimed user in full so a match's position
/// in the list could not be inferred from response timing. That mitigation
/// no longer applies: `find_by_id` addresses a token by primary key, not by
/// rank within a scan, so there is no list-position to leak. The SHA-256
/// digest comparison remains constant-time (`subtle::ConstantTimeEq` inside
/// [`token::verify_device_token`]) to avoid leaking the hash itself.
///
/// Side-effect: schedules an async `update_last_used` write (SQL-side
/// debounced to at most one UPDATE per token per 5 minutes).
///
/// # Errors
///
/// Returns [`AppError::Unauthorized`] when `prefixed_id` does not carry
/// [`token::TOKEN_PREFIX`], the remainder is not a UUID, the id resolves to
/// no active (non-revoked, non-expired) token, the owning account is
/// disabled, or `secret` does not match the stored hash. Returns
/// [`AppError::Internal`] on database errors.
async fn resolve_device_token(
    state: &AppState,
    prefixed_id: &str,
    secret: &str,
) -> Result<Option<CurrentUser>, AppError> {
    let Some(id_str) = prefixed_id.strip_prefix(token::TOKEN_PREFIX) else {
        return Err(AppError::Unauthorized);
    };
    let token_id: Uuid = id_str.parse().map_err(|_| AppError::Unauthorized)?;

    let dt = device_token::find_by_id(&state.pool, token_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?
        .ok_or(AppError::Unauthorized)?;

    let u = user::find_by_id(&state.pool, dt.user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?
        .ok_or(AppError::Unauthorized)?;
    // THREAT (account lockout): a soft-disabled account must not authenticate
    // on any transport. Reject before the secret comparison so a disabled
    // user's token is inert regardless of whether the secret itself is
    // correct.
    if u.disabled_at.is_some() {
        return Err(AppError::Unauthorized);
    }

    if !token::verify_device_token(secret, &dt.token_hash) {
        return Err(AppError::Unauthorized);
    }

    let pool = state.pool.clone();
    let matched_token_id = dt.id;
    tokio::spawn(async move {
        if let Err(e) = device_token::update_last_used(&pool, matched_token_id).await {
            tracing::warn!(
                error = %e,
                token_id = %matched_token_id,
                "device_token: update_last_used failed (non-fatal)"
            );
        }
    });

    Ok(Some(CurrentUser {
        user_id: u.id,
        role: u.role,
        is_child: u.is_child,
        scopes: dt.scopes,
    }))
}

/// Verify an `Authorization: Basic <b64({prefix}{token_id}:{secret})>`
/// header. Shared by [`CurrentUser`] (cookie-or-Basic-or-Bearer) and
/// [`crate::auth::basic_only::BasicOnly`] (Basic-only, for OPDS clients that
/// only speak Basic).
///
/// Returns `Ok(Some(user))` when Basic credentials validate, `Ok(None)` when
/// no `Authorization: Basic ...` is present, and `Err(Unauthorized)` when a
/// Basic header is present but does not resolve — see
/// [`resolve_device_token`] for the failure modes.
///
/// # Errors
///
/// Returns [`AppError::Unauthorized`] when the `Authorization: Basic` header
/// is present but malformed or does not resolve to an active token. Returns
/// [`AppError::Internal`] on database errors.
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
    let (username, secret) = decoded_str.split_once(':').ok_or(AppError::Unauthorized)?;

    resolve_device_token(state, username, secret).await
}

/// Verify an `Authorization: Bearer {prefix}{token_id}.{secret}` header.
/// Shares [`resolve_device_token`] with [`verify_basic`] — both transports
/// converge on the same indexed lookup.
///
/// Returns `Ok(Some(user))` when the Bearer credential validates, `Ok(None)`
/// when no `Authorization: Bearer ...` is present, and `Err(Unauthorized)`
/// when a Bearer header is present but does not resolve — see
/// [`resolve_device_token`] for the failure modes.
///
/// # Errors
///
/// Returns [`AppError::Unauthorized`] when the `Authorization: Bearer` header
/// is present but malformed (missing the `.` separator) or does not resolve
/// to an active token. Returns [`AppError::Internal`] on database errors.
pub async fn verify_bearer(
    state: &AppState,
    parts: &Parts,
) -> Result<Option<CurrentUser>, AppError> {
    let Some(auth) = parts.headers.get(axum::http::header::AUTHORIZATION) else {
        return Ok(None);
    };
    let Ok(auth_str) = auth.to_str() else {
        return Ok(None);
    };
    let Some(credential) = auth_str.strip_prefix("Bearer ") else {
        return Ok(None);
    };

    let (prefixed_id, secret) = credential.split_once('.').ok_or(AppError::Unauthorized)?;
    resolve_device_token(state, prefixed_id, secret).await
}

impl FromRequestParts<AppState> for CurrentUser {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        // Session cookie path. The `Session` is populated by
        // `SessionManagerLayer`; on a request with no session cookie it is
        // empty and `get(user_id)` is `None`, so we fall through cleanly
        // (no DB hit, no 500) to the Basic-auth path below.
        if let Ok(session) = Session::from_request_parts(parts, state).await
            && let Some(user_id) = session
                .get::<Uuid>(SESSION_KEY_USER_ID)
                .await
                .map_err(|e| AppError::Internal(e.into()))?
        {
            // The session claims an identity. It is valid only if the user row
            // still exists AND the stored `session_version` matches the live
            // one; any other outcome (user deleted, or version bumped) means the
            // session must be torn down server-side.
            let user = user::find_by_id(&state.pool, user_id)
                .await
                .map_err(|e| AppError::Internal(e.into()))?;
            // THREAT (account lockout): a soft-disabled account is rejected on
            // session rehydration independent of `session_version`. Disabling
            // also bumps the version (so the check below would catch it too),
            // but this explicit gate is defence-in-depth: a disabled user is
            // never re-admitted from a live cookie. Tear the session down so it
            // cannot keep re-loading until idle expiry.
            if let Some(u) = &user
                && u.disabled_at.is_some()
            {
                if let Err(e) = session.flush().await {
                    tracing::warn!(error = %e, "session flush failed (disabled account)");
                }
                return Err(AppError::Unauthorized);
            }
            // THREAT (force-logout): the session stores the `session_version`
            // captured at login; if `users.session_version` has since been
            // bumped (role change, security event) the stored copy is stale and
            // the session is rejected. Plain `==` is safe here — session
            // contents are server-side state, not attacker-controlled (the
            // cookie carries only the random session id).
            let stored_version = session
                .get::<i32>(SESSION_KEY_SESSION_VERSION)
                .await
                .map_err(|e| AppError::Internal(e.into()))?;
            if let Some(user) = user
                && stored_version == Some(user.session_version)
            {
                return Ok(Self {
                    user_id: user.id,
                    role: user.role,
                    is_child: user.is_child,
                    scopes: Scope::for_role(user.role).to_vec(),
                });
            }
            // Invalid session — user deleted, or `session_version` stale. Wipe
            // the row server-side so an orphaned/stale session can't keep
            // re-loading on every request until 24h idle expiry, then fall
            // through. No silent discard (backend/CLAUDE.md): log on failure.
            if let Err(e) = session.flush().await {
                tracing::warn!(error = %e, "session flush failed (force-logout / deleted user)");
            }
        }

        // Fall back to Basic, then Bearer. Mutually exclusive by header
        // prefix, so trying both is safe — only one can match a given
        // `Authorization` header value.
        if let Some(user) = verify_basic(state, parts).await? {
            return Ok(user);
        }
        if let Some(user) = verify_bearer(state, parts).await? {
            return Ok(user);
        }

        Err(AppError::Unauthorized)
    }
}
