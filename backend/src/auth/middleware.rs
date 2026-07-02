//! `CurrentUser` extractor and Bearer/Basic verification for Reverie.
//!
//! [`crate::auth::middleware::CurrentUser`] is the primary identity extractor used by route handlers.
//! It resolves the caller in three steps: session cookie first (rehydrated from
//! the first-party [`tower_sessions::Session`] — read `user_id`, reload the user,
//! compare `session_version`), then `Authorization: Basic` (via
//! [`verify_basic`](crate::auth::middleware::verify_basic)), then
//! `Authorization: Bearer` (via
//! [`verify_bearer`](crate::auth::middleware::verify_bearer)). `verify_bearer`
//! itself dispatches on the credential's prefix: a `{prefix}{token_id}.{secret}`
//! value resolves through `resolve_device_token` (shared with Basic — a single
//! indexed lookup, see `auth::token::TOKEN_PREFIX`), anything else is handed to
//! `resolve_jwt` as an RFC 9068 resource-server access token when a
//! [`crate::auth::jwt::JwtValidator`] is configured. Handlers that receive a
//! `CurrentUser` are guaranteed an authenticated identity; unauthenticated
//! requests are rejected with `AppError::Unauthorized` (no credential
//! presented) or `AppError::InvalidCredential` (a credential was presented and
//! rejected) before the handler body runs.
//!
//! # Tier 2 — security-critical
//!
//! This module is the authentication seam for every non-public route.
//! Inline `// THREAT:` annotations mark the account-lockout and
//! `session_version` force-logout checks, and the retirement of the old
//! per-user-scan timing mitigation in `resolve_device_token`. The
//! role/scope-assertion contract on
//! [`CurrentUser`](crate::auth::middleware::CurrentUser) is enforced
//! structurally (private `role`/`is_child`/`scopes` fields, access only via the
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
/// (via [`verify_bearer`], itself device-token-or-JWT — see the module docs).
/// Returns [`AppError::Unauthorized`] if no credential was presented at all,
/// or [`AppError::InvalidCredential`] if one was presented and rejected.
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
    /// explicit scope chosen at mint. Private, gated via
    /// [`require_scope`](CurrentUser::require_scope).
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

    /// Floor check against the scope hierarchy (`read` < `write` < `admin`):
    /// `Ok` when this credential holds `needed` or any higher scope.
    ///
    /// An endpoint names the least scope it requires; a `write` credential
    /// clears a `read` gate and a `admin` credential clears a `write` gate,
    /// because a higher scope subsumes every lower one. Every session derives
    /// at least `{read, write}` from role (see [`Scope::for_role`]) and every
    /// token is minted with at least `read`, so a `read` gate never blocks an
    /// authenticated caller; only a narrowed token is blocked from `write` or
    /// `admin`. Child restrictions come from
    /// [`require_not_child`](CurrentUser::require_not_child), not from
    /// withholding scope.
    ///
    /// # Errors
    ///
    /// Returns [`AppError::Forbidden`] when no held scope reaches `needed`.
    pub fn require_scope(&self, needed: Scope) -> Result<(), AppError> {
        if self.scopes.iter().any(|held| *held >= needed) {
            Ok(())
        } else {
            // THREAT (capability overreach): a credential reached for a scope
            // above its own. Rare on the failure branch (a narrowed token
            // pointed at a higher-scope endpoint), so a warn here is signal,
            // not noise, for a token being used beyond its grant.
            tracing::warn!(
                user_id = %self.user_id,
                needed = %needed,
                "scope check rejected: credential lacks required scope"
            );
            Err(AppError::Forbidden)
        }
    }

    /// Whether this user's role permits minting a token carrying `scope`
    /// ([`Scope::grantable_by`]; a non-admin cannot mint an admin-scoped
    /// token). Narrow accessor for the mint-time ceiling check; `role`
    /// itself stays private.
    ///
    /// THREAT (privilege escalation): this is the role ceiling on delegated
    /// capability. It bounds a minted token to no more than its owner's role
    /// already grants, so a compromised non-admin session cannot forge an
    /// admin-scoped credential. The mint handler logs a rejected grant.
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
/// Returns [`AppError::InvalidCredential`] when `prefixed_id` does not carry
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
        return Err(AppError::InvalidCredential);
    };
    let token_id: Uuid = id_str.parse().map_err(|_| AppError::InvalidCredential)?;

    let dt = device_token::find_by_id(&state.pool, token_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?
        .ok_or(AppError::InvalidCredential)?;

    let u = user::find_by_id(&state.pool, dt.user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?
        .ok_or(AppError::InvalidCredential)?;
    // THREAT (account lockout): a soft-disabled account must not authenticate
    // on any transport. Reject before the secret comparison so a disabled
    // user's token is inert regardless of whether the secret itself is
    // correct.
    if u.disabled_at.is_some() {
        return Err(AppError::InvalidCredential);
    }

    if !token::verify_device_token(secret, &dt.token_hash) {
        return Err(AppError::InvalidCredential);
    }

    // THREAT (read floor): scopes form a hierarchy (read < write < admin) with
    // read as the floor, so every valid credential must carry at least one
    // scope. A scopeless token has zero capability; admit it and it would reach
    // read surface with no gate ever rejecting it. Mint refuses an empty set at
    // issue; this is the defence-in-depth seam that also rejects any empty-scope
    // row reaching auth by another path (legacy data, direct DB write).
    if dt.scopes.is_empty() {
        return Err(AppError::InvalidCredential);
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
/// no `Authorization: Basic ...` is present, and `Err(InvalidCredential)`
/// when a Basic header is present but does not resolve — see
/// [`resolve_device_token`] for the failure modes.
///
/// # Errors
///
/// Returns [`AppError::InvalidCredential`] when the `Authorization: Basic`
/// header is present but malformed or does not resolve to an active token.
/// Returns [`AppError::Internal`] on database errors.
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
        .map_err(|_| AppError::InvalidCredential)?;
    let decoded_str = std::str::from_utf8(decoded).map_err(|_| AppError::InvalidCredential)?;
    let (username, secret) = decoded_str
        .split_once(':')
        .ok_or(AppError::InvalidCredential)?;

    resolve_device_token(state, username, secret).await
}

/// Verify an `Authorization: Bearer` header, dispatching on the credential's
/// prefix. A `{prefix}{token_id}.{secret}` value (see [`token::TOKEN_PREFIX`])
/// is a device token and resolves through [`resolve_device_token`], shared
/// with [`verify_basic`]. Anything else is handed to [`resolve_jwt`] as an
/// RFC 9068 resource-server access token.
///
/// Returns `Ok(Some(user))` when the Bearer credential validates, `Ok(None)`
/// when no `Authorization: Bearer ...` is present (or the credential is a
/// non-device-token value and no [`crate::auth::jwt::JwtValidator`] is
/// configured — the resource-server feature is inert, so this is
/// indistinguishable from "no credential" rather than a rejection), and
/// `Err(InvalidCredential)` when a Bearer header is present but does not
/// resolve.
///
/// # Errors
///
/// Returns [`AppError::InvalidCredential`] when the `Authorization: Bearer`
/// header is present but malformed (a device-token-prefixed value missing the
/// `.` separator) or does not resolve to an active identity. Returns
/// [`AppError::Internal`] on database errors.
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

    if credential.starts_with(token::TOKEN_PREFIX) {
        let (prefixed_id, secret) = credential
            .split_once('.')
            .ok_or(AppError::InvalidCredential)?;
        return resolve_device_token(state, prefixed_id, secret).await;
    }

    resolve_jwt(state, credential).await
}

/// Resolve the effective scope set for a validated JWT's `scope` claim
/// against the caller's role ceiling: claimed names are matched against the
/// fixed `read`/`write`/`admin` literals, unknown names are ignored (never an
/// error), and the granted set can never exceed
/// [`Scope::for_role`] — the role ceiling always holds regardless of what the
/// claim asks for. An absent claim carries the full role-derived set,
/// matching session parity: an unscoped delegation is the issuer granting
/// everything the role already allows. A present claim that narrows to
/// nothing (every named scope unknown, or above the role ceiling) is
/// rejected — `None` — rather than silently falling back to the full
/// role-derived set, mirroring the empty-scope rejection in
/// [`resolve_device_token`]: an explicit narrowing that resolves to zero
/// capability is a rejected credential, not an ignored one.
fn resolve_jwt_scopes(claimed: Option<&[String]>, role: Role) -> Option<Vec<Scope>> {
    let ceiling = Scope::for_role(role);
    let Some(names) = claimed else {
        return Some(ceiling.to_vec());
    };

    let granted: Vec<Scope> = ceiling
        .iter()
        .copied()
        .filter(|scope| names.iter().any(|name| name == scope.as_str()))
        .collect();

    if granted.is_empty() {
        None
    } else {
        Some(granted)
    }
}

/// Resolve an `Authorization: Bearer` credential as an RFC 9068
/// resource-server access token and look up the canonical identity it
/// carries. Read-only: an unlinked `(iss, sub)` is rejected, never
/// provisioned here — the interactive OIDC callback (`upsert_from_oidc`) is
/// the only path that creates a `user_identities` row, so an access token
/// alone can never mint an account.
///
/// Returns `Ok(None)` when no [`crate::auth::jwt::JwtValidator`] is
/// configured, so [`verify_bearer`] falls through to the generic
/// `Unauthorized` challenge exactly as it would for an absent header — the
/// `rvpat_` device-token path is unaffected either way. Once a validator is
/// configured, every rejection (signature/claim failure, unknown identity, a
/// disabled account, or a `scope` claim that narrows to nothing the role
/// permits) is [`AppError::InvalidCredential`].
///
/// # Errors
///
/// Returns [`AppError::InvalidCredential`] on any rejection described above.
/// Returns [`AppError::Internal`] on a database error during identity lookup.
async fn resolve_jwt(state: &AppState, token: &str) -> Result<Option<CurrentUser>, AppError> {
    let Some(validator) = &state.jwt_validator else {
        return Ok(None);
    };

    let claims = validator
        .validate(token)
        .await
        .map_err(|_| AppError::InvalidCredential)?;

    let user = user::find_by_oidc_identity(&state.pool, &claims.iss, &claims.sub)
        .await
        .map_err(|e| AppError::Internal(e.into()))?
        .ok_or(AppError::InvalidCredential)?;

    // THREAT (account lockout): mirrors resolve_device_token — a
    // soft-disabled account must not authenticate on any transport.
    if user.disabled_at.is_some() {
        return Err(AppError::InvalidCredential);
    }

    let scopes = resolve_jwt_scopes(claims.scope.as_deref(), user.role)
        .ok_or(AppError::InvalidCredential)?;

    Ok(Some(CurrentUser {
        user_id: user.id,
        role: user.role,
        is_child: user.is_child,
        scopes,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn names(literals: &[&str]) -> Vec<String> {
        literals.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn absent_scope_claim_grants_full_role_ceiling() {
        assert_eq!(
            resolve_jwt_scopes(None, Role::Adult),
            Some(vec![Scope::Read, Scope::Write])
        );
        assert_eq!(
            resolve_jwt_scopes(None, Role::Admin),
            Some(vec![Scope::Read, Scope::Write, Scope::Admin])
        );
    }

    #[test]
    fn claimed_scope_narrows_to_the_intersection() {
        let claimed = names(&["read"]);
        assert_eq!(
            resolve_jwt_scopes(Some(&claimed), Role::Admin),
            Some(vec![Scope::Read]),
            "an admin JWT claiming only read must not carry write/admin"
        );
    }

    #[test]
    fn unknown_scope_literals_are_ignored_not_rejected() {
        let claimed = names(&["read", "superuser"]);
        assert_eq!(
            resolve_jwt_scopes(Some(&claimed), Role::Adult),
            Some(vec![Scope::Read]),
            "an unrecognised literal must be dropped, not treated as an error"
        );
    }

    #[test]
    fn claim_above_role_ceiling_rejects_rather_than_falling_back() {
        // An adult's role ceiling is {read, write} — it never includes admin,
        // so claiming only "admin" intersects to nothing. This must reject
        // the credential outright, not silently substitute the full
        // role-derived set (that would ignore an explicit, if unreachable,
        // narrowing request instead of treating it as a rejected credential).
        let claimed = names(&["admin"]);
        assert_eq!(
            resolve_jwt_scopes(Some(&claimed), Role::Adult),
            None,
            "a claim that narrows to nothing the role permits must reject"
        );
    }

    #[test]
    fn all_unknown_scope_literals_reject() {
        let claimed = names(&["superuser", "wizard"]);
        assert_eq!(resolve_jwt_scopes(Some(&claimed), Role::Admin), None);
    }

    #[test]
    fn explicitly_empty_scope_list_rejects() {
        let claimed: Vec<String> = Vec::new();
        assert_eq!(resolve_jwt_scopes(Some(&claimed), Role::Admin), None);
    }

    // Extractor integration: the full CurrentUser resolution path through a
    // real router + real database, not just resolve_jwt/resolve_jwt_scopes in
    // isolation. These are #[sqlx::test] (DB-backed) and ride CI; see
    // CLAUDE.local.md for why they don't run in this workspace.

    use axum::http::StatusCode;
    use axum::http::header::{AUTHORIZATION, WWW_AUTHENTICATE};
    use sqlx::PgPool;

    use crate::error::problems;
    use crate::test_support::oidc_mock::{MockOidcProvider, now_unix};
    use crate::test_support::{self, db};

    const AUDIENCE: &str = "reverie-integration-tests";

    fn claims_for(mock: &MockOidcProvider, subject: &str) -> serde_json::Value {
        let now = now_unix();
        serde_json::json!({
            "iss": mock.issuer(),
            "aud": AUDIENCE,
            "sub": subject,
            "exp": now + 300,
            "iat": now,
        })
    }

    /// Mint a `{prefix}{id}.{secret}` device-token Bearer value for
    /// `user_id`, role-derived scope set (matches the Basic-auth test
    /// helpers in `test_support::db`).
    async fn bearer_device_token(pool: &PgPool, user_id: Uuid) -> String {
        let (plaintext, hash) = token::generate_device_token();
        let dt = device_token::create_with_limit(
            pool,
            user_id,
            "bearer-integration-test",
            &hash,
            Scope::for_role(Role::Adult),
            None,
        )
        .await
        .expect("create device token");
        format!("Bearer {}{}.{plaintext}", token::TOKEN_PREFIX, dt.id)
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn jwt_caller_reaches_protected_endpoint(pool: PgPool) {
        let ingestion_pool = db::ingestion_pool_for(&pool).await;
        let app_pool = db::app_pool_for(&pool).await;
        let mock = MockOidcProvider::start("").await;
        mock.mount_resource_server_jwks().await;
        let server =
            db::server_with_jwt_validator(&app_pool, &ingestion_pool, &mock, AUDIENCE, false).await;

        let subject = format!("jwt-e2e-{}", Uuid::new_v4());
        user::upsert_from_oidc(&app_pool, mock.issuer(), &subject, "JWT E2E", None)
            .await
            .expect("seed identity via interactive-login-equivalent upsert");
        let token = mock.sign_access_token(&claims_for(&mock, &subject), |_| {});

        let response = server
            .get("/api/v1/shelves")
            .add_header(AUTHORIZATION, format!("Bearer {token}"))
            .await;

        assert_eq!(response.status_code(), StatusCode::OK);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn unknown_oidc_identity_rejected_and_not_provisioned(pool: PgPool) {
        let ingestion_pool = db::ingestion_pool_for(&pool).await;
        let app_pool = db::app_pool_for(&pool).await;
        let mock = MockOidcProvider::start("").await;
        mock.mount_resource_server_jwks().await;
        let server =
            db::server_with_jwt_validator(&app_pool, &ingestion_pool, &mock, AUDIENCE, false).await;

        // Never seeded via upsert_from_oidc — this (iss, sub) has no
        // user_identities row.
        let subject = format!("never-provisioned-{}", Uuid::new_v4());
        let token = mock.sign_access_token(&claims_for(&mock, &subject), |_| {});

        let response = server
            .get("/api/v1/shelves")
            .add_header(AUTHORIZATION, format!("Bearer {token}"))
            .await;

        test_support::assert_problem(&response, problems::UNAUTHORIZED, StatusCode::UNAUTHORIZED);
        assert_eq!(
            response
                .headers()
                .get(WWW_AUTHENTICATE)
                .and_then(|v| v.to_str().ok()),
            Some(r#"Bearer error="invalid_token""#),
        );

        // The load-bearing assertion: a valid, well-formed JWT for an
        // unlinked identity must never provision an account.
        let resolved = user::find_by_oidc_identity(&app_pool, mock.issuer(), &subject)
            .await
            .expect("lookup succeeds");
        assert!(
            resolved.is_none(),
            "an access token for an unknown identity must never create a user row"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn disabled_account_rejected_via_jwt(pool: PgPool) {
        let ingestion_pool = db::ingestion_pool_for(&pool).await;
        let app_pool = db::app_pool_for(&pool).await;
        let mock = MockOidcProvider::start("").await;
        mock.mount_resource_server_jwks().await;
        let server =
            db::server_with_jwt_validator(&app_pool, &ingestion_pool, &mock, AUDIENCE, false).await;

        let subject = format!("jwt-disabled-{}", Uuid::new_v4());
        let seeded = user::upsert_from_oidc(&app_pool, mock.issuer(), &subject, "Disabled", None)
            .await
            .expect("seed identity");
        user::disable_account(&app_pool, seeded.id)
            .await
            .expect("disable account");
        let token = mock.sign_access_token(&claims_for(&mock, &subject), |_| {});

        let response = server
            .get("/api/v1/shelves")
            .add_header(AUTHORIZATION, format!("Bearer {token}"))
            .await;

        test_support::assert_problem(&response, problems::UNAUTHORIZED, StatusCode::UNAUTHORIZED);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn malformed_bearer_value_rejected_when_configured(pool: PgPool) {
        let ingestion_pool = db::ingestion_pool_for(&pool).await;
        let app_pool = db::app_pool_for(&pool).await;
        let mock = MockOidcProvider::start("").await;
        mock.mount_resource_server_jwks().await;
        let server =
            db::server_with_jwt_validator(&app_pool, &ingestion_pool, &mock, AUDIENCE, false).await;

        let response = server
            .get("/api/v1/shelves")
            .add_header(AUTHORIZATION, "Bearer not-a-jwt-at-all")
            .await;

        test_support::assert_problem(&response, problems::UNAUTHORIZED, StatusCode::UNAUTHORIZED);
        assert_eq!(
            response
                .headers()
                .get(WWW_AUTHENTICATE)
                .and_then(|v| v.to_str().ok()),
            Some(r#"Bearer error="invalid_token""#),
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn device_token_bearer_unaffected_when_resource_server_unconfigured(pool: PgPool) {
        let ingestion_pool = db::ingestion_pool_for(&pool).await;
        let app_pool = db::app_pool_for(&pool).await;
        let server = db::server_with_real_pools(&app_pool, &ingestion_pool);

        let subject = format!("rvpat-off-{}", Uuid::new_v4());
        let seeded = user::upsert_from_oidc(
            &app_pool,
            "https://test-issuer.example.com",
            &subject,
            "Rvpat",
            None,
        )
        .await
        .expect("seed identity");
        let bearer = bearer_device_token(&app_pool, seeded.id).await;

        let response = server
            .get("/api/v1/shelves")
            .add_header(AUTHORIZATION, bearer)
            .await;

        assert_eq!(
            response.status_code(),
            StatusCode::OK,
            "an rvpat_ device-token Bearer credential must be unaffected by S5 when no \
             JwtValidator is configured"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn non_device_bearer_falls_through_to_generic_unauthorized_when_unconfigured(
        pool: PgPool,
    ) {
        let ingestion_pool = db::ingestion_pool_for(&pool).await;
        let app_pool = db::app_pool_for(&pool).await;
        let server = db::server_with_real_pools(&app_pool, &ingestion_pool);

        let response = server
            .get("/api/v1/shelves")
            .add_header(AUTHORIZATION, "Bearer looks.like.a.jwt")
            .await;

        test_support::assert_problem(&response, problems::UNAUTHORIZED, StatusCode::UNAUTHORIZED);
        assert_eq!(
            response
                .headers()
                .get(WWW_AUTHENTICATE)
                .and_then(|v| v.to_str().ok()),
            Some("Bearer"),
            "with the resource-server feature off, a non-device-token Bearer value must \
             be indistinguishable from no credential at all — never claim invalid_token \
             for a feature that isn't even configured"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn device_token_bearer_still_works_when_resource_server_configured(pool: PgPool) {
        let ingestion_pool = db::ingestion_pool_for(&pool).await;
        let app_pool = db::app_pool_for(&pool).await;
        let mock = MockOidcProvider::start("").await;
        mock.mount_resource_server_jwks().await;
        let server =
            db::server_with_jwt_validator(&app_pool, &ingestion_pool, &mock, AUDIENCE, false).await;

        let subject = format!("rvpat-on-{}", Uuid::new_v4());
        let seeded = user::upsert_from_oidc(&app_pool, mock.issuer(), &subject, "Rvpat On", None)
            .await
            .expect("seed identity");
        let bearer = bearer_device_token(&app_pool, seeded.id).await;

        let response = server
            .get("/api/v1/shelves")
            .add_header(AUTHORIZATION, bearer)
            .await;

        assert_eq!(
            response.status_code(),
            StatusCode::OK,
            "dispatch must route an rvpat_-prefixed credential to the device-token path \
             even with a JwtValidator configured, never attempt to parse it as a JWT"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn basic_auth_path_unaffected_when_resource_server_configured(pool: PgPool) {
        let ingestion_pool = db::ingestion_pool_for(&pool).await;
        let app_pool = db::app_pool_for(&pool).await;
        let mock = MockOidcProvider::start("").await;
        mock.mount_resource_server_jwks().await;
        let server =
            db::server_with_jwt_validator(&app_pool, &ingestion_pool, &mock, AUDIENCE, false).await;

        let (_, basic) = db::create_adult_and_basic_auth(&app_pool, "basic-regression").await;

        let response = server
            .get("/api/v1/shelves")
            .add_header(AUTHORIZATION, basic)
            .await;

        assert_eq!(response.status_code(), StatusCode::OK);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn no_credential_at_all_gets_bare_bearer_challenge(pool: PgPool) {
        let ingestion_pool = db::ingestion_pool_for(&pool).await;
        let app_pool = db::app_pool_for(&pool).await;
        let mock = MockOidcProvider::start("").await;
        mock.mount_resource_server_jwks().await;
        let server =
            db::server_with_jwt_validator(&app_pool, &ingestion_pool, &mock, AUDIENCE, false).await;

        let response = server.get("/api/v1/shelves").await;

        test_support::assert_problem(&response, problems::UNAUTHORIZED, StatusCode::UNAUTHORIZED);
        assert_eq!(
            response
                .headers()
                .get(WWW_AUTHENTICATE)
                .and_then(|v| v.to_str().ok()),
            Some("Bearer"),
        );
    }
}
