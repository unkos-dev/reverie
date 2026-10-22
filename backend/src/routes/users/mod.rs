//! `/api/v1/users*` admin-only user management routes.
//!
//! THREAT: Privilege escalation and horizontal privilege abuse. All mutations
//! are admin-gated via `require_admin()`; `session_version` bumps ensure role
//! changes take effect on active sessions immediately.
//!
//! All endpoints require `role = admin`; non-admin callers receive
//! `AppError::Forbidden` (403).
//!
//! Session invalidation policy: `users.session_version` is bumped in the same
//! transaction only for mutations that change access-control state:
//! - `PUT …/role` — role governs RLS visibility and admin gates.
//! - `PUT …/child-status` — child flag controls content-visibility rules.
//! `PATCH …` `email` and `display_name` do not bump session_version: neither
//! gates access. Login identity is the OIDC `sub`, RLS keys on user
//! id/role/`is_child`, and the session auth hash is `session_version` only — so
//! a stale email or name in an active session has no security consequence.
//!
//! # Last-admin protection (TOCTOU-safe)
//!
//! `PUT /api/v1/users/{id}/role` and `PUT /api/v1/users/{id}/child-status` —
//! acquire `SELECT … FOR UPDATE` on all admin rows (`ORDER BY id`) first,
//! then lock the target row. Consistent lock order (admin rows always
//! before target) prevents deadlock when two concurrent demotions each
//! hold a different admin row and wait for the other. Under READ COMMITTED
//! the second transaction sees the first's committed state and rejects
//! with 422 "would leave zero admins".

use axum::extract::State;
use axum::http::{StatusCode, header};
use axum::response::IntoResponse;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::auth::middleware::CurrentUser;
use crate::auth::scope::Scope;
use crate::error::AppError;
use crate::extract::{ApiJson, ApiPath};
use crate::models::account_email_proof::{self, ProofPurpose};
use crate::models::mail_job::{self, JobState};
use crate::models::role::Role;
use crate::models::user::is_addr_spec;
use crate::services::mail::dispatch::{Outbox, Requested};
use crate::services::mail::render::Notice;
use crate::state::AppState;

#[cfg(test)]
mod tests;

/// Build the `/api/v1/users*` router as an [`OpenApiRouter`] so each
/// handler's `#[utoipa::path]` contributes to the generated spec (a missing
/// annotation fails to compile).
///
/// Merged into `crate::openapi::pilot_router`
/// and split into its runtime and spec halves there.
pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_users))
        .routes(routes!(create_user))
        .routes(routes!(update_role))
        .routes(routes!(update_child_status))
        .routes(routes!(update_user))
        .routes(routes!(update_account_status))
        .routes(routes!(issue_credential_link))
        .routes(routes!(cancel_credential_link))
        .routes(routes!(send_credential_link_email))
        .routes(routes!(credential_link_email_status))
        .routes(routes!(change_own_password))
        .routes(routes!(email_verification_status))
        .routes(routes!(request_email_verification))
        .routes(routes!(confirm_email_verification))
}

/// Enforce the configured password policy (length bounds, zxcvbn floor, HIBP
/// breach check) against a candidate, mapping a rejection to a 422. The breach
/// client is built per call from the configured user-agent and carries the SSRF
/// resolver, so the operator-overridable HIBP URL cannot be aimed at an internal
/// address.
async fn enforce_password_policy(
    state: &AppState,
    password: &str,
    user_inputs: &[&str],
) -> Result<(), AppError> {
    crate::auth::password_policy::enforce_from_config(&state.config, password, user_inputs)
        .await
        .map_err(|e| AppError::Validation(e.to_string()))
}

/// Wire-format user row returned by list and mutation endpoints.
#[derive(Debug, Serialize, utoipa::ToSchema)]
struct UserResponse {
    /// User id.
    id: Uuid,
    /// Human-readable display name.
    display_name: String,
    /// Email address; `null` when the user has none on file.
    email: Option<String>,
    /// Access-control role.
    role: Role,
    /// Whether child content-visibility rules apply to this user.
    is_child: bool,
    /// Row creation timestamp.
    created_at: DateTime<Utc>,
    /// Last mutation timestamp.
    updated_at: DateTime<Utc>,
    /// Whether the account is soft-disabled (cannot authenticate). Derived from
    /// `disabled_at`; the timestamp itself is not exposed on the wire.
    disabled: bool,
    has_password: bool,
    credential_link_expires_at: Option<DateTime<Utc>>,
}

/// Defensive bound on the users list (the justified
/// single-page exception of `docs/adr/0019-keyset-pagination-as-the-default-list-contract.md`). A household/self-hosted instance's user
/// table has a genuinely small natural ceiling — a multi-hundred-user
/// deployment is outside Reverie's design scope — so a hard `LIMIT`
/// beats paginating an endpoint whose realistic cardinality is single
/// digits. Bounded by construction rather than by assumption.
const MAX_LISTED_USERS: i64 = 500;

/// `GET /api/v1/users` — list all users (admin only).
///
/// # Errors
/// - [`AppError::Forbidden`] when the caller is not an admin.
/// - [`AppError::Internal`] on database errors.
#[utoipa::path(
    get,
    path = "/api/v1/users",
    summary = "List users",
    description = "Returns every user, oldest first, capped at 500 rows. Admin only.",
    tag = "users",
    security(("session_cookie" = ["admin"]), ("device_token_bearer" = ["admin"]), ("opds_basic" = ["admin"])),
    responses(
        (status = 200, description = "All users, oldest first, defensively capped at 500 rows. Admin only.", body = [UserResponse]),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 403, description = "Caller is not an admin", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
    )
)]
async fn list_users(
    current_user: CurrentUser,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    current_user.require_scope(Scope::Admin)?;
    current_user.require_admin()?;

    let rows = sqlx::query_as!(
        UserResponse,
        r#"SELECT users.id,
                  users.display_name,
                  users.email,
                  users.role AS "role: Role",
                  users.is_child,
                  users.created_at,
                  users.updated_at,
                  (users.disabled_at IS NOT NULL) AS "disabled!",
                  (c.user_id IS NOT NULL) AS "has_password!",
                  l.expires_at AS "credential_link_expires_at?"
             FROM users
 LEFT JOIN local_credentials c ON c.user_id = users.id
 LEFT JOIN credential_links l ON l.user_id = users.id AND l.expires_at > now()
            ORDER BY users.created_at ASC, users.id ASC
            LIMIT $1"#,
        MAX_LISTED_USERS,
    )
    .fetch_all(&state.pool)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    Ok(axum::Json(rows))
}

/// Body for `PUT /api/v1/users/{id}/role`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
struct UpdateRoleRequest {
    /// New role for the target user.
    role: Role,
}

/// `PUT /api/v1/users/{id}/role` — change a user's role (admin only).
///
/// Last-admin protection: acquires `FOR UPDATE` on all admin rows
/// before the demotion check to prevent TOCTOU races.
///
/// Bumps `session_version` for the target user in the same transaction
/// so their active sessions are invalidated.
///
/// # Errors
/// - [`AppError::Forbidden`] when the caller is not an admin.
/// - [`AppError::NotFound`] when the target user does not exist.
/// - [`AppError::Validation`] with detail "would leave zero admins"
///   when the demotion would remove the last admin.
/// - [`AppError::Internal`] on database errors.
#[utoipa::path(
    put,
    path = "/api/v1/users/{id}/role",
    summary = "Change a user's role",
    description = "Sets the target user's role and child status together and invalidates their active sessions. Child status is enabled only for the child role. Admin only. Returns 422 if the change would leave zero administrators.",
    tag = "users",
    security(("session_cookie" = ["admin"]), ("device_token_bearer" = ["admin"]), ("opds_basic" = ["admin"])),
    params(("id" = Uuid, Path, max_length = 45, description = "Target user id")),
    request_body = UpdateRoleRequest,
    responses(
        (status = 200, description = "Updated user. The target's active sessions are invalidated. Admin only.", body = UserResponse),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 403, description = "Caller is not an admin", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 404, description = "Target user does not exist", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 422, description = "Demotion would leave zero admins, or the request body is malformed / contains an unknown role value", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
    )
)]
async fn update_role(
    current_user: CurrentUser,
    State(state): State<AppState>,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(req): ApiJson<UpdateRoleRequest>,
) -> Result<impl IntoResponse, AppError> {
    current_user.require_scope(Scope::Admin)?;
    current_user.require_admin()?;

    let mut tx = state
        .pool
        .begin()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    // Lock all admin rows first (ORDER BY id for consistent acquisition order),
    // then lock the target. This order prevents deadlock when two concurrent
    // demotions each try to lock a different admin row and then all admin rows —
    // both transactions acquire admin locks in the same id sequence, so one
    // blocks and waits rather than forming a cycle.
    let admin_ids: Vec<Uuid> =
        sqlx::query_scalar!("SELECT id FROM users WHERE role = 'admin' ORDER BY id FOR UPDATE")
            .fetch_all(&mut *tx)
            .await
            .map_err(|e| AppError::Internal(e.into()))?;

    let target = sqlx::query!(
        r#"SELECT role AS "role: Role", is_child FROM users WHERE id = $1 FOR UPDATE"#,
        id,
    )
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?
    .ok_or(AppError::NotFound)?;

    // Last-admin protection: recount using the locked snapshot.
    if target.role == Role::Admin && req.role != Role::Admin && admin_ids.len() <= 1 {
        return Err(AppError::Validation("would leave zero admins".into()));
    }

    let row = sqlx::query_as!(
        UserResponse,
        r#"WITH changed AS (UPDATE users
              SET role = ($1::text)::user_role,
                  is_child = ($1::text = 'child'),
                  session_version = session_version + 1,
                  updated_at = now()
            WHERE id = $2
        RETURNING *)
 SELECT users.id,
                  users.display_name,
                  users.email,
                  users.role AS "role: Role",
                  users.is_child,
                  users.created_at,
                  users.updated_at,
                  (users.disabled_at IS NOT NULL) AS "disabled!",
                  (c.user_id IS NOT NULL) AS "has_password!",
                  l.expires_at AS "credential_link_expires_at?"
 FROM changed AS users
 LEFT JOIN local_credentials c ON c.user_id = users.id
 LEFT JOIN credential_links l ON l.user_id = users.id AND l.expires_at > now()"#,
        req.role.as_str(),
        id,
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    tx.commit()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    Ok(axum::Json(row))
}

/// Body for `PUT /api/v1/users/{id}/child-status`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
struct UpdateChildStatusRequest {
    /// New child status for the target user.
    is_child: bool,
}

/// `PUT /api/v1/users/{id}/child-status` — toggle child status (admin only).
///
/// When `is_child` is set to `true`, `role` is simultaneously set to
/// `'child'` in the same transaction to satisfy `chk_child_role_sync`.
/// When `is_child` is set to `false`, `role` is reverted to `'adult'` only
/// if the current role is `'child'`; other roles (e.g. `'admin'`) are left
/// unchanged to prevent privilege escalation through the child-status toggle.
///
/// Bumps `session_version` — child/adult visibility rules differ under
/// RLS, so existing sessions must re-evaluate.
///
/// # Errors
/// - [`AppError::Forbidden`] when the caller is not an admin.
/// - [`AppError::NotFound`] when the target user does not exist.
/// - [`AppError::Validation`] with detail "would leave zero admins"
///   when marking an admin as child would remove the last admin.
/// - [`AppError::Internal`] on database errors.
#[utoipa::path(
    put,
    path = "/api/v1/users/{id}/child-status",
    summary = "Change a user's child status",
    description = "Toggles whether child content-visibility rules apply to the target user and invalidates their active sessions. Enabling child status also sets the role to `child`; disabling it reverts `child` to `adult` and leaves other roles unchanged. Admin only. Returns 422 if marking the last administrator as a child would leave zero administrators.",
    tag = "users",
    security(("session_cookie" = ["admin"]), ("device_token_bearer" = ["admin"]), ("opds_basic" = ["admin"])),
    params(("id" = Uuid, Path, max_length = 45, description = "Target user id")),
    request_body = UpdateChildStatusRequest,
    responses(
        (status = 200, description = "Updated user. Enabling child status also sets role to `child`; disabling reverts `child` to `adult` (other roles unchanged). The target's active sessions are invalidated. Admin only.", body = UserResponse),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 403, description = "Caller is not an admin", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 404, description = "Target user does not exist", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 422, description = "Marking the last admin as child would leave zero admins, or the request body is malformed", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
    )
)]
async fn update_child_status(
    current_user: CurrentUser,
    State(state): State<AppState>,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(req): ApiJson<UpdateChildStatusRequest>,
) -> Result<impl IntoResponse, AppError> {
    current_user.require_scope(Scope::Admin)?;
    current_user.require_admin()?;

    let mut tx = state
        .pool
        .begin()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    // Lock all admin rows first (ORDER BY id) then lock the target —
    // same acquisition order as update_role to prevent deadlock.
    let admin_ids: Vec<Uuid> =
        sqlx::query_scalar!("SELECT id FROM users WHERE role = 'admin' ORDER BY id FOR UPDATE")
            .fetch_all(&mut *tx)
            .await
            .map_err(|e| AppError::Internal(e.into()))?;

    let target = sqlx::query!(
        r#"SELECT role AS "role: Role" FROM users WHERE id = $1 FOR UPDATE"#,
        id,
    )
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?
    .ok_or(AppError::NotFound)?;

    // Toggling child ON for an admin = demotion to child. Last-admin check.
    if req.is_child && target.role == Role::Admin && admin_ids.len() <= 1 {
        return Err(AppError::Validation("would leave zero admins".into()));
    }

    let new_role = if req.is_child {
        Role::Child
    } else {
        // When un-childing, revert to adult (not admin — privilege
        // escalation must be an explicit role PUT).
        match target.role {
            Role::Child => Role::Adult,
            other => other,
        }
    };

    let row = sqlx::query_as!(
        UserResponse,
        r#"WITH changed AS (UPDATE users
              SET is_child = $1,
                  role = ($2::text)::user_role,
                  session_version = session_version + 1,
                  updated_at = now()
            WHERE id = $3
        RETURNING *)
 SELECT users.id,
                  users.display_name,
                  users.email,
                  users.role AS "role: Role",
                  users.is_child,
                  users.created_at,
                  users.updated_at,
                  (users.disabled_at IS NOT NULL) AS "disabled!",
                  (c.user_id IS NOT NULL) AS "has_password!",
                  l.expires_at AS "credential_link_expires_at?"
 FROM changed AS users
 LEFT JOIN local_credentials c ON c.user_id = users.id
 LEFT JOIN credential_links l ON l.user_id = users.id AND l.expires_at > now()"#,
        req.is_child,
        new_role.as_str(),
        id,
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    tx.commit()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    Ok(axum::Json(row))
}

/// Body for `POST /api/v1/users`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
struct CreateUserRequest {
    /// Account email in the common ASCII format with a dotted domain and an
    /// alphabetic suffix of at least two letters; unique (case-insensitive).
    email: String,
    /// Human-readable display name.
    display_name: String,
    /// Role for the new account. `child` and `admin` accounts are created only
    /// here, by an existing administrator.
    role: Role,
}

#[derive(Serialize, utoipa::ToSchema)]
struct CredentialLinkResponse {
    token: String,
    expires_at: DateTime<Utc>,
}

#[derive(Serialize, utoipa::ToSchema)]
struct CreateUserResponse {
    user: UserResponse,
    link: CredentialLinkResponse,
}

/// Create a password-less account and its single-use credential link atomically.
#[utoipa::path(
    post, path = "/api/v1/users", summary = "Create a user",
    description = "Creates an account and returns a single-use link for its holder to choose a password through `/auth/credential-links/redemptions`. Requires an administrator browser session and enabled local authentication.",
    tag = "users", security(("session_cookie" = ["admin"])),
    request_body = CreateUserRequest,
    responses(
        (status = 201, description = "Account and credential link created", body = CreateUserResponse, headers(("Cache-Control" = String, description = "no-store"))),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 403, description = "Administrator browser session required", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 404, description = "Local authentication is disabled", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 409, description = "Email already in use", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 422, description = "Invalid email or request", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
    )
)]
async fn create_user(
    current_user: CurrentUser,
    State(state): State<AppState>,
    ApiJson(req): ApiJson<CreateUserRequest>,
) -> Result<impl IntoResponse, AppError> {
    if !state.config.local_auth_enabled {
        return Err(AppError::NotFound);
    }
    current_user.require_scope(Scope::Admin)?;
    current_user.require_admin()?;
    current_user.require_not_child()?;
    current_user.require_browser_session()?;
    if !is_addr_spec(&req.email) {
        return Err(AppError::Validation("invalid email address".into()));
    }
    let mut tx = state
        .pool
        .begin()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    let user =
        crate::models::user::create_local(&mut *tx, &req.email, &req.display_name, req.role, None)
            .await
            .map_err(|e| match e {
                crate::models::user::CreateUserError::EmailExists => AppError::EmailConflict,
                crate::models::user::CreateUserError::Db(db) => AppError::Internal(db.into()),
            })?;
    let (token, expires_at) = crate::models::credential_link::issue(&mut tx, user.id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    tx.commit()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    let body = UserResponse {
        id: user.id,
        display_name: user.display_name,
        email: user.email,
        role: user.role,
        is_child: user.is_child,
        created_at: user.created_at,
        updated_at: user.updated_at,
        disabled: user.disabled_at.is_some(),
        has_password: false,
        credential_link_expires_at: Some(expires_at),
    };
    Ok((
        StatusCode::CREATED,
        [(header::CACHE_CONTROL, "no-store")],
        axum::Json(CreateUserResponse {
            user: body,
            link: CredentialLinkResponse { token, expires_at },
        }),
    ))
}

/// Body for `PUT /api/v1/users/{id}/account-status`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
struct AccountStatusRequest {
    /// `true` to soft-disable the account, `false` to re-enable it.
    disabled: bool,
}

/// Soft-disable or re-enable an account (admin only).
///
/// Disabling stamps `disabled_at`, bumps the target's `session_version`
/// (killing live sessions at once), and locks them out of every auth path;
/// re-enabling clears it.
///
/// Last-enabled-admin protection (TOCTOU-safe): when disabling an admin, all
/// enabled admin rows are locked `FOR UPDATE` (same `ORDER BY id` order as
/// [`update_role`]) and the request is rejected if it would leave zero enabled
/// admins. An admin cannot disable their own account.
///
/// # Errors
/// - [`AppError::Forbidden`] when the caller is not an admin or is a child.
/// - [`AppError::Validation`] (422) on a self-disable attempt or when disabling
///   the last enabled admin.
/// - [`AppError::NotFound`] when the target does not exist.
/// - [`AppError::Internal`] on database errors.
#[utoipa::path(
    put,
    path = "/api/v1/users/{id}/account-status",
    summary = "Enable or disable an account",
    description = "Soft-disables or re-enables the target account. Disabling invalidates the target's active sessions; an administrator cannot disable their own account. Admin only; the caller must not be a child. Returns 422 if disabling would leave zero enabled administrators.",
    tag = "users",
    security(("session_cookie" = ["admin"]), ("device_token_bearer" = ["admin"]), ("opds_basic" = ["admin"])),
    params(("id" = Uuid, Path, max_length = 45, description = "Target user id")),
    request_body = AccountStatusRequest,
    responses(
        (status = 200, description = "Updated user. Disabling invalidates the target's sessions. Admin only.", body = UserResponse),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 403, description = "Caller is not an admin", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 404, description = "Target user does not exist", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 422, description = "Cannot disable your own account, or disabling would leave zero enabled admins", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
    )
)]
async fn update_account_status(
    current_user: CurrentUser,
    State(state): State<AppState>,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(req): ApiJson<AccountStatusRequest>,
) -> Result<impl IntoResponse, AppError> {
    current_user.require_scope(Scope::Admin)?;
    current_user.require_admin()?;
    current_user.require_not_child()?;

    if req.disabled && id == current_user.user_id {
        return Err(AppError::Validation(
            "cannot disable your own account".into(),
        ));
    }

    let mut tx = state
        .pool
        .begin()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    // Lock the currently-enabled admin rows first (consistent ORDER BY id with
    // update_role), then the target. The snapshot is the basis for the
    // last-enabled-admin recount.
    let enabled_admin_ids: Vec<Uuid> = sqlx::query_scalar!(
        "SELECT id FROM users WHERE role = 'admin' AND disabled_at IS NULL ORDER BY id FOR UPDATE"
    )
    .fetch_all(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    let target = sqlx::query!(
        r#"SELECT role AS "role: Role", disabled_at FROM users WHERE id = $1 FOR UPDATE"#,
        id,
    )
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?
    .ok_or(AppError::NotFound)?;

    // Disabling the sole enabled admin would brick the instance (recovery is
    // DB-only). target.disabled_at.is_none() means it is counted in the locked
    // set; len <= 1 means it is the only one.
    if req.disabled
        && target.role == Role::Admin
        && target.disabled_at.is_none()
        && enabled_admin_ids.len() <= 1
    {
        return Err(AppError::Validation(
            "would leave zero enabled admins".into(),
        ));
    }

    // Idempotent: only write when the state actually changes. A retry or
    // double-submit against an account already in the requested state must not
    // bump session_version (which would evict live sessions) or touch updated_at.
    if req.disabled != target.disabled_at.is_some() {
        if req.disabled {
            crate::models::user::disable_account(&mut *tx, id).await
        } else {
            crate::models::user::enable_account(&mut *tx, id).await
        }
        .map_err(|e| AppError::Internal(e.into()))?;
    }

    let row = sqlx::query_as!(
        UserResponse,
        r#"SELECT users.id,
                  users.display_name,
                  users.email,
                  users.role AS "role: Role",
                  users.is_child,
                  users.created_at,
                  users.updated_at,
                  (users.disabled_at IS NOT NULL) AS "disabled!",
                  (c.user_id IS NOT NULL) AS "has_password!",
                  l.expires_at AS "credential_link_expires_at?"
             FROM users
 LEFT JOIN local_credentials c ON c.user_id = users.id
 LEFT JOIN credential_links l ON l.user_id = users.id AND l.expires_at > now() WHERE users.id = $1"#,
        id,
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    tx.commit()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    Ok(axum::Json(row))
}

/// Issue or replace an enabled account's credential link.
#[utoipa::path(
    post, path = "/api/v1/users/{id}/credential-links", summary = "Issue a credential link",
    description = "Returns a single-use link for the account holder to choose a password. Replaces any prior link without changing the current password or sessions.",
    tag = "users", security(("session_cookie" = ["admin"])),
    params(("id" = Uuid, Path, max_length = 45, description = "Target user id")),
    responses(
        (status = 201, description = "Credential link issued", body = CredentialLinkResponse, headers(("Cache-Control" = String, description = "no-store"))),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 403, description = "Administrator browser session required", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 404, description = "Local authentication disabled or account not found", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 422, description = "Account disabled or without an email address", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
    )
)]
async fn issue_credential_link(
    current_user: CurrentUser,
    State(state): State<AppState>,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    if !state.config.local_auth_enabled {
        return Err(AppError::NotFound);
    }
    current_user.require_scope(Scope::Admin)?;
    current_user.require_admin()?;
    current_user.require_not_child()?;
    current_user.require_browser_session()?;
    let mut tx = state
        .pool
        .begin()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    let target = sqlx::query!(
        "SELECT disabled_at, email FROM users WHERE id = $1 FOR NO KEY UPDATE",
        id
    )
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?
    .ok_or(AppError::NotFound)?;
    if target.disabled_at.is_some() {
        return Err(AppError::Validation("the account is disabled".into()));
    }
    if target.email.is_none() {
        return Err(AppError::Validation(
            "the account has no email address".into(),
        ));
    }
    let (token, expires_at) = crate::models::credential_link::issue(&mut tx, id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    tx.commit()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    Ok((
        StatusCode::CREATED,
        [(header::CACHE_CONTROL, "no-store")],
        axum::Json(CredentialLinkResponse { token, expires_at }),
    ))
}

/// Cancel an account's credential link idempotently.
#[utoipa::path(
    delete, path = "/api/v1/users/{id}/credential-links", summary = "Cancel a credential link",
    description = "Cancels the account's link without changing its password or sessions.",
    tag = "users", security(("session_cookie" = ["admin"])),
    params(("id" = Uuid, Path, max_length = 45, description = "Target user id")),
    responses(
        (status = 204, description = "No active link remains"),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 403, description = "Administrator browser session required", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 404, description = "Local authentication is disabled", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
    )
)]
async fn cancel_credential_link(
    current_user: CurrentUser,
    State(state): State<AppState>,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<impl IntoResponse, AppError> {
    if !state.config.local_auth_enabled {
        return Err(AppError::NotFound);
    }
    current_user.require_scope(Scope::Admin)?;
    current_user.require_admin()?;
    current_user.require_not_child()?;
    current_user.require_browser_session()?;
    let mut tx = state
        .pool
        .begin()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    crate::models::account_email_proof::lock_account(&mut tx, id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    crate::models::credential_link::cancel(&mut tx, id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    tx.commit()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    Ok(StatusCode::NO_CONTENT)
}

fn outbox(state: &AppState) -> Result<Option<Outbox>, AppError> {
    Outbox::new(state.mail.as_ref(), state.config.public_url.as_ref())
        .map_err(|e| AppError::Internal(e.into()))
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
struct CredentialLinkEmailRequest {
    /// The link token the administrator holds; it must match the account's live link.
    #[schema(
        min_length = 43,
        max_length = 43,
        pattern = "^[A-Za-z0-9_-]{42}[AEIMQUYcgkosw048]$"
    )]
    token: String,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
enum CredentialLinkEmailState {
    Queued,
    Accepted,
    Failed,
    Inactive,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
struct CredentialLinkEmailStatus {
    /// The delivery's identifier.
    job_id: Uuid,
    /// `queued` while waiting or in flight, `accepted` once the mail server took
    /// it, `failed` when delivery gave up, `inactive` once the link was
    /// replaced, cancelled or expired. Acceptance does not confirm inbox delivery.
    status: CredentialLinkEmailState,
    /// Safe failure category (`connection`, `tls`, `authentication`,
    /// `temporary_refusal`, `permanent_refusal`, `timeout` or
    /// `payload_unreadable`) while `status` is `failed`; otherwise `null`.
    failure_category: Option<String>,
}

impl From<mail_job::JobStatus> for CredentialLinkEmailState {
    fn from(status: mail_job::JobStatus) -> Self {
        match status.state {
            JobState::Pending | JobState::InProgress => Self::Queued,
            JobState::Accepted => Self::Accepted,
            JobState::Failed => Self::Failed,
            JobState::Cancelled | JobState::Expired => Self::Inactive,
        }
    }
}

/// Email an account's live credential link to its stored address.
#[utoipa::path(
    post, path = "/api/v1/users/{id}/credential-links/emails", summary = "Email a credential link",
    description = "Queues the link the caller holds for the account's stored email address. The request supplies the live link token and no address or URL; it must match the account's live link, and the account must be enabled with an address. Returns the delivery's identifier and status. Requires an administrator browser session and enabled local authentication. Acceptance by the mail server does not confirm inbox delivery.",
    tag = "users", security(("session_cookie" = ["admin"])),
    params(("id" = Uuid, Path, max_length = 45, description = "Target user id")),
    request_body = CredentialLinkEmailRequest,
    responses(
        (status = 202, description = "Delivery queued", body = CredentialLinkEmailStatus, headers(("Cache-Control" = String, description = "no-store"))),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 403, description = "Administrator browser session required", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 404, description = "Local authentication disabled or account not found", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 422, description = "The link is not the account's live link, or the account is disabled or has no email address", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 503, description = "Email is not available on this server", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
    )
)]
async fn send_credential_link_email(
    current_user: CurrentUser,
    State(state): State<AppState>,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(req): ApiJson<CredentialLinkEmailRequest>,
) -> Result<impl IntoResponse, AppError> {
    if !state.config.local_auth_enabled {
        return Err(AppError::NotFound);
    }
    current_user.require_scope(Scope::Admin)?;
    current_user.require_admin()?;
    current_user.require_not_child()?;
    current_user.require_browser_session()?;
    let outbox = outbox(&state)?.ok_or(AppError::MailUnavailable)?;
    let inactive = || AppError::Validation("the link is no longer active".into());
    let mut tx = state
        .pool
        .begin()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    let account = account_email_proof::lock_account(&mut tx, id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?
        .ok_or(AppError::NotFound)?;
    // THREAT: The recipient is the stored address and the link must still be the live one.
    let recipient = account
        .email
        .as_deref()
        .filter(|_| !account.disabled)
        .ok_or_else(inactive)?;
    let expires_at = crate::models::credential_link::live_expiry(&mut tx, id, &req.token)
        .await
        .map_err(|e| AppError::Internal(e.into()))?
        .ok_or_else(inactive)?;
    let has_password = crate::models::local_credentials::find_by_user_id(&mut *tx, id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?
        .is_some();
    let job_id = outbox
        .admin_link(&mut tx, id, recipient, has_password, &req.token, expires_at)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    tx.commit()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    tracing::info!(
        action = "credential_link_email_queued",
        admin_id = %current_user.user_id,
        user_id = %id,
        job_id = %job_id,
        "credential link email queued"
    );
    Ok((
        StatusCode::ACCEPTED,
        [(header::CACHE_CONTROL, "no-store")],
        axum::Json(CredentialLinkEmailStatus {
            job_id,
            status: CredentialLinkEmailState::Queued,
            failure_category: None,
        }),
    ))
}

/// Report the status of one emailed credential link.
#[utoipa::path(
    get, path = "/api/v1/users/{id}/credential-links/emails/{job_id}", summary = "Get a link email's status",
    description = "Returns whether the delivery is queued, accepted by the mail server, failed with a safe category, or inactive because the link was replaced, cancelled or expired. Requires an administrator browser session and enabled local authentication.",
    tag = "users", security(("session_cookie" = ["admin"])),
    params(
        ("id" = Uuid, Path, max_length = 45, description = "Target user id"),
        ("job_id" = Uuid, Path, max_length = 45, description = "Delivery identifier returned when the email was queued"),
    ),
    responses(
        (status = 200, description = "Delivery status", body = CredentialLinkEmailStatus, headers(("Cache-Control" = String, description = "no-store"))),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 403, description = "Administrator browser session required", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 404, description = "Local authentication disabled, or no such delivery for this account", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
    )
)]
async fn credential_link_email_status(
    current_user: CurrentUser,
    State(state): State<AppState>,
    ApiPath((id, job_id)): ApiPath<(Uuid, Uuid)>,
) -> Result<impl IntoResponse, AppError> {
    if !state.config.local_auth_enabled {
        return Err(AppError::NotFound);
    }
    current_user.require_scope(Scope::Admin)?;
    current_user.require_admin()?;
    current_user.require_not_child()?;
    current_user.require_browser_session()?;
    let status = mail_job::admin_link_status(&state.pool, id, job_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?
        .ok_or(AppError::NotFound)?;
    let failure_category = status.failure_category.clone();
    Ok((
        [(header::CACHE_CONTROL, "no-store")],
        axum::Json(CredentialLinkEmailStatus {
            job_id,
            status: status.into(),
            failure_category,
        }),
    ))
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
struct EmailVerificationStatus {
    /// The account's current email address; `null` when none is on file.
    email: Option<String>,
    /// Whether Reverie has verified the current address.
    email_verified: bool,
    /// Whether this server can send authentication mail.
    mail_enabled: bool,
}

/// Report the caller's address and whether it is verified.
#[utoipa::path(
    get, path = "/api/v1/account/email-verifications", summary = "Get your email verification status",
    description = "Returns the caller's current email address, whether it is verified, and whether the server can send mail. Requires a browser session.",
    tag = "users", security(("session_cookie" = ["read"])),
    responses(
        (status = 200, description = "Verification status", body = EmailVerificationStatus, headers(("Cache-Control" = String, description = "no-store"))),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 403, description = "Browser session required", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
    )
)]
async fn email_verification_status(
    current_user: CurrentUser,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    current_user.require_scope(Scope::Read)?;
    current_user.require_browser_session()?;
    let row = sqlx::query!(
        "SELECT email, email_verified_at FROM users WHERE id = $1",
        current_user.user_id,
    )
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| AppError::Internal(e.into()))?
    .ok_or(AppError::Unauthorized)?;
    Ok((
        [(header::CACHE_CONTROL, "no-store")],
        axum::Json(EmailVerificationStatus {
            email: row.email,
            email_verified: row.email_verified_at.is_some(),
            mail_enabled: state.mail.is_some(),
        }),
    ))
}

/// Queue or reuse a mailbox verification email for the caller's address.
#[utoipa::path(
    post, path = "/api/v1/account/email-verifications", summary = "Request a verification email",
    description = "Queues a verification email for the caller's current address, or reuses the pending one. A request within 60 seconds of the previous one, or while a delivery is pending, queues nothing more. Returns 422 when the account has no address or it is already verified. Requires a browser session and configured mail.",
    tag = "users", security(("session_cookie" = ["write"])),
    responses(
        (status = 202, description = "Verification email queued or already pending", headers(("Cache-Control" = String, description = "no-store"))),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 403, description = "Browser session required", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 422, description = "The account has no email address, or it is already verified", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 503, description = "Email is not available on this server", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
    )
)]
async fn request_email_verification(
    current_user: CurrentUser,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    current_user.require_scope(Scope::Write)?;
    current_user.require_browser_session()?;
    let outbox = outbox(&state)?.ok_or(AppError::MailUnavailable)?;
    let mut tx = state
        .pool
        .begin()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    let account = account_email_proof::lock_account(&mut tx, current_user.user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?
        .ok_or(AppError::Unauthorized)?;
    if account.disabled {
        return Err(AppError::Unauthorized);
    }
    let address = account
        .email
        .as_deref()
        .ok_or_else(|| AppError::Validation("the account has no email address".into()))?;
    if account.email_verified_at.is_some() {
        return Err(AppError::Validation(
            "the email address is already verified".into(),
        ));
    }
    let outcome = outbox
        .request_proof(
            &mut tx,
            current_user.user_id,
            ProofPurpose::Verification,
            address,
        )
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    tx.commit()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    if let Requested::Queued(job_id) = outcome {
        tracing::info!(
            action = "email_verification_queued",
            user_id = %current_user.user_id,
            job_id = %job_id,
            "email verification queued"
        );
    }
    Ok((StatusCode::ACCEPTED, [(header::CACHE_CONTROL, "no-store")]))
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
struct EmailVerificationConfirmation {
    /// The token from the verification email.
    #[schema(
        min_length = 43,
        max_length = 43,
        pattern = "^[A-Za-z0-9_-]{42}[AEIMQUYcgkosw048]$"
    )]
    token: String,
}

/// Confirm control of the caller's address with an emailed token.
#[utoipa::path(
    post, path = "/api/v1/account/email-verifications/confirmations", summary = "Confirm your email address",
    description = "Consumes the verification token mailed to the caller's current address and records the address as verified. The token must belong to the signed-in account; an expired, used, replaced, foreign or outdated token returns one generic 422. Changes no password or address and signs nobody in. Requires a browser session.",
    tag = "users", security(("session_cookie" = ["write"])),
    request_body = EmailVerificationConfirmation,
    responses(
        (status = 204, description = "Address verified"),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 403, description = "Browser session required", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 422, description = "Invalid or expired link", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
    )
)]
async fn confirm_email_verification(
    current_user: CurrentUser,
    State(state): State<AppState>,
    ApiJson(req): ApiJson<EmailVerificationConfirmation>,
) -> Result<impl IntoResponse, AppError> {
    current_user.require_scope(Scope::Write)?;
    current_user.require_browser_session()?;
    let user_id = current_user.user_id;
    let mut tx = state
        .pool
        .begin()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    account_email_proof::lock_account(&mut tx, user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?
        .ok_or(AppError::Unauthorized)?;
    // THREAT: Only this account's challenge for its current address can verify it.
    let address =
        account_email_proof::consume(&mut tx, user_id, ProofPurpose::Verification, &req.token)
            .await
            .map_err(|e| AppError::Internal(e.into()))?
            .ok_or_else(|| AppError::Validation("invalid or expired link".into()))?;
    crate::models::user::set_email_verified(&mut tx, user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    if let Some(outbox) = outbox(&state)? {
        outbox
            .notice_to_address(&mut tx, user_id, Notice::AddressVerified, &address)
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
    }
    tx.commit()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    Ok((
        StatusCode::NO_CONTENT,
        [(header::CACHE_CONTROL, "no-store")],
    ))
}

/// Body for `POST /api/v1/account/password`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
struct ChangePasswordRequest {
    /// The caller's current password, re-verified before the change.
    #[schema(max_length = 256)]
    current_password: String,
    /// The new password; enforced against the password policy.
    #[schema(max_length = 256)]
    new_password: String,
}

/// `POST /api/v1/account/password` — the authenticated caller changes their own
/// password.
///
/// THREAT (IDOR): the target is ALWAYS the session user. The
/// endpoint accepts no id from the path or body, so it cannot be turned against
/// another account. It verifies the current password, enforces the policy on the
/// new one, writes it, and bumps `session_version`, which forces re-auth of every
/// session including the caller's own.
///
/// # Errors
/// - [`AppError::Unauthorized`] when unauthenticated.
/// - [`AppError::Validation`] (422) when the current password is wrong, the new
///   password fails the policy, or the account has no local credential (it signs
///   in through an identity provider), or the verified credential changed
///   concurrently.
/// - [`AppError::Internal`] on database errors.
#[utoipa::path(
    post,
    path = "/api/v1/account/password",
    summary = "Change your own password",
    description = "Verifies the caller's current password and sets a new one only if that credential is unchanged, invalidating all of the caller's sessions including the one making this request. Returns 422 for a wrong current password, a rejected new password, no local credential, or a concurrent credential change. Returns 503 when authentication processing is busy.",
    tag = "users",
    security(("session_cookie" = ["write"]), ("device_token_bearer" = ["write"]), ("opds_basic" = ["write"])),
    request_body = ChangePasswordRequest,
    responses(
        (status = 503, description = "Authentication processing is busy", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 200, description = "Password changed; all of the caller's sessions are invalidated."),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 422, description = "Wrong current password, rejected new password, no local credential, or concurrent credential change", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
    )
)]
async fn change_own_password(
    current_user: CurrentUser,
    State(state): State<AppState>,
    ApiJson(req): ApiJson<ChangePasswordRequest>,
) -> Result<impl IntoResponse, AppError> {
    current_user.require_scope(Scope::Write)?;
    let user_id = current_user.user_id;

    crate::auth::password_policy::enforce_max_length(
        &req.current_password,
        crate::auth::password_policy::MAX_PASSWORD_LENGTH,
    )
    .map_err(|e| AppError::Validation(e.to_string()))?;
    crate::auth::password_policy::enforce_max_length(
        &req.new_password,
        state.config.password_max_length,
    )
    .map_err(|e| AppError::Validation(e.to_string()))?;

    let credential = crate::models::local_credentials::find_by_user_id(&state.pool, user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?
        .ok_or_else(|| {
            AppError::Validation(
                "this account has no password to change; it signs in through an identity provider"
                    .to_owned(),
            )
        })?;

    let verified = {
        let permit = state.hash_limiter.acquire().await?;
        permit
            .verify(req.current_password, credential.password_hash.clone())
            .await
            .map_err(|e| AppError::Internal(e.into()))?
    };
    if !verified {
        return Err(AppError::Validation(
            "current password is incorrect".to_owned(),
        ));
    }

    // Same context-word treatment as the other credential-setting paths: the
    // caller's own email and display name penalize a password that echoes them.
    let me = crate::models::user::find_by_id(&state.pool, user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?
        .ok_or(AppError::Unauthorized)?;
    let mut context: Vec<&str> = vec![me.display_name.as_str()];
    if let Some(email) = me.email.as_deref() {
        context.push(email);
    }
    enforce_password_policy(&state, &req.new_password, &context).await?;
    let phc = state
        .hash_limiter
        .hash(req.new_password)
        .await?
        .map_err(|e| AppError::Internal(anyhow::anyhow!("password hash failed: {e}")))?;

    let mut tx = state
        .pool
        .begin()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    // THREAT: user-before-credential locking prevents inversion with recovery writers.
    let account = crate::models::account_email_proof::lock_account(&mut tx, user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?
        .ok_or(AppError::Unauthorized)?;
    let replaced = crate::models::local_credentials::replace_verified_password(
        &mut *tx,
        user_id,
        &credential.password_hash,
        &phc,
    )
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    if !replaced {
        return Err(AppError::Validation(
            "password changed while this request was in progress; sign in again before retrying"
                .to_owned(),
        ));
    }
    crate::models::user::increment_session_version(&mut *tx, user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    if let Some(outbox) = outbox(&state)? {
        outbox
            .notice_to_verified_address(&mut tx, user_id, &account, Notice::PasswordChanged)
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
    }
    tx.commit()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    Ok(StatusCode::OK)
}

/// Body for `PATCH /api/v1/users/{id}`.
///
/// RFC 7396 JSON Merge Patch: absent keys are untouched; explicit
/// `null` on `email` clears; `null` on `display_name` is rejected
/// (NOT NULL column).
#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[expect(
    clippy::option_option,
    reason = "RFC 7396: None = absent, Some(None) = null, Some(Some) = value"
)]
struct UpdateUserRequest {
    /// New display name. Absent = unchanged; explicit `null` is rejected
    /// (NOT NULL column).
    // value_type = String (not Option<String>): the schema must NOT say
    // nullable — the runtime 422s an explicit null. Optionality (absent =
    // unchanged) is carried by the field not being required.
    #[serde(default, deserialize_with = "deserialize_optional_string")]
    #[schema(value_type = String)]
    display_name: Option<Option<String>>,
    /// New account email in the common ASCII format with a dotted domain and an
    /// alphabetic suffix of at least two letters.
    /// Absent = unchanged; explicit `null` clears the stored address.
    #[serde(default, deserialize_with = "deserialize_optional_string")]
    #[schema(value_type = Option<String>)]
    email: Option<Option<String>>,
}

#[expect(
    clippy::option_option,
    reason = "RFC 7396: None = absent, Some(None) = null, Some(Some) = value"
)]
fn deserialize_optional_string<'de, D>(deserializer: D) -> Result<Option<Option<String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Some(Option::<String>::deserialize(deserializer)?))
}

/// Converts a sqlx error: unique-constraint violation → [`AppError::Validation`]
/// with the given message; everything else → [`AppError::Internal`].
///
/// Used by `update_user` to translate a DB-level unique violation on the
/// email column into a 422 even when the race slips past the proactive SELECT.
fn unique_violation_or_internal(e: sqlx::Error, msg: &'static str) -> AppError {
    if let sqlx::Error::Database(ref db_err) = e
        && db_err.is_unique_violation()
    {
        return AppError::Validation(msg.into());
    }
    AppError::Internal(e.into())
}

/// Validate an admin-supplied `email` for `PATCH /api/v1/users/{id}`.
///
/// Returns the trimmed addr-spec on success. Rejects an empty/whitespace-only
/// value and addresses outside the supported common ASCII format (see
/// [`is_addr_spec`]) with [`AppError::Validation`] (422).
///
/// THREAT: an admin submitting a malformed value is surfaced server-side for
/// security observability. The rejection is logged by shape (length) only —
/// never the value verbatim (Hard Rule 7).
fn validate_patch_email(raw: &str, admin_id: Uuid, target_user_id: Uuid) -> Result<&str, AppError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(AppError::Validation("email must not be empty".into()));
    }
    if !is_addr_spec(trimmed) {
        tracing::warn!(
            admin_id = %admin_id,
            target_user_id = %target_user_id,
            rejected_email_len = trimmed.len(),
            "admin PATCH rejected malformed email value"
        );
        return Err(AppError::Validation("email must be a valid address".into()));
    }
    Ok(trimmed)
}

/// Write an admin-supplied email (`None` clears it) and report whether the
/// address actually changed, which is what clears its verification.
///
/// Email is not an access-control input: login identity resolves through
/// `user_identities` keyed on `(issuer, subject)`, not email; RLS keys on user
/// id/role/`is_child`, and the session auth hash is `session_version` only. So
/// changing or clearing email does not bump `session_version`. The uniqueness
/// constraint is still enforced for the set case.
async fn apply_email_patch(
    tx: &mut sqlx::PgConnection,
    id: Uuid,
    admin_id: Uuid,
    before: &account_email_proof::LockedAccount,
    email: Option<&str>,
) -> Result<bool, AppError> {
    let Some(email) = email else {
        sqlx::query!(
            "UPDATE users SET email = NULL, email_verified_at = NULL, updated_at = now() \
             WHERE id = $1",
            id,
        )
        .execute(tx)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
        return Ok(before.email.is_some());
    };
    let trimmed = validate_patch_email(email, admin_id, id)?;
    // Check unique constraint proactively for a clear error message.
    let conflict = sqlx::query_scalar!(
        "SELECT id FROM users WHERE LOWER(email) = LOWER($1) AND id != $2",
        trimmed,
        id,
    )
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    if conflict.is_some() {
        return Err(AppError::Validation("email already in use".into()));
    }
    let changed = !before
        .email
        .as_deref()
        .is_some_and(|old| old.eq_ignore_ascii_case(trimmed));
    sqlx::query!(
        "UPDATE users SET email = $1, updated_at = now(), \
                email_verified_at = CASE WHEN $3 THEN NULL ELSE email_verified_at END \
          WHERE id = $2",
        trimmed,
        id,
        changed,
    )
    .execute(tx)
    .await
    // Proactive SELECT covers the common case; translate any
    // race-escaped unique violation to 422 rather than 500.
    .map_err(|e| unique_violation_or_internal(e, "email already in use"))?;
    Ok(changed)
}

/// Revoke what the account's former address authorised after an administrator
/// changed or removed it, and tell that address when it was verified.
async fn retire_old_address(
    state: &AppState,
    tx: &mut sqlx::PgConnection,
    id: Uuid,
    before: &account_email_proof::LockedAccount,
    removed: bool,
) -> Result<(), AppError> {
    account_email_proof::cancel_all(tx, id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    mail_job::revoke_for_authority(tx, id, mail_job::AuthorityKind::CredentialLink)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    if let (Some(old), Some(_), Some(outbox)) = (
        before.email.as_deref(),
        before.email_verified_at,
        outbox(state)?,
    ) {
        let event = if removed {
            Notice::AddressRemoved
        } else {
            Notice::AddressChanged
        };
        outbox
            .notice_to_address(tx, id, event, old)
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
    }
    Ok(())
}

/// `PATCH /api/v1/users/{id}` — update `display_name` / `email` (admin only).
///
/// Does not bump `session_version`: neither `email` nor `display_name` gates
/// access (login identity is the OIDC `sub`, not email; see module-level
/// session-invalidation policy).
///
/// # Errors
/// - [`AppError::Forbidden`] when the caller is not an admin.
/// - [`AppError::NotFound`] when the target user does not exist.
/// - [`AppError::Validation`] when `display_name` is null or empty, when
///   `email` is not a valid RFC 5322 addr-spec (display-name and domain-literal
///   forms are rejected), or when `email` is already in use by another user.
/// - [`AppError::Internal`] on database errors.
#[utoipa::path(
    patch,
    path = "/api/v1/users/{id}",
    summary = "Update a user's name or email",
    description = "Updates the target user's `display_name` and `email` using JSON Merge Patch: an absent field is unchanged, and an explicit `null` clears `email` but is rejected for `display_name`. Neither field invalidates the target's sessions. An actual change or removal of `email` clears its verification and mailbox proofs and withdraws queued link mail. Admin only. Returns 422 for an empty `display_name` or an `email` already in use.",
    tag = "users",
    security(("session_cookie" = ["admin"]), ("device_token_bearer" = ["admin"]), ("opds_basic" = ["admin"])),
    params(("id" = Uuid, Path, max_length = 45, description = "Target user id")),
    request_body(content = UpdateUserRequest, description = "RFC 7396 JSON Merge Patch: absent fields are unchanged; explicit `null` clears `email` and is rejected for `display_name`"),
    responses(
        (status = 200, description = "Updated user. Does not invalidate the target's sessions (neither field gates access). Admin only.", body = UserResponse),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 403, description = "Caller is not an admin", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 404, description = "Target user does not exist", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 422, description = "Null/empty display_name, malformed email, or email already in use", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
    )
)]
async fn update_user(
    current_user: CurrentUser,
    State(state): State<AppState>,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(req): ApiJson<UpdateUserRequest>,
) -> Result<impl IntoResponse, AppError> {
    current_user.require_scope(Scope::Admin)?;
    current_user.require_admin()?;

    // Validate display_name: null → 422, empty → 422.
    if let Some(ref dn_opt) = req.display_name {
        match dn_opt {
            None => {
                return Err(AppError::Validation("display_name cannot be null".into()));
            }
            Some(name) if name.trim().is_empty() => {
                return Err(AppError::Validation(
                    "display_name must not be empty".into(),
                ));
            }
            _ => {}
        }
    }

    let mut tx = state
        .pool
        .begin()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let before = account_email_proof::lock_account(&mut tx, id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?
        .ok_or(AppError::NotFound)?;
    let mut address_changed = false;

    // Apply display_name if present.
    if let Some(Some(ref name)) = req.display_name {
        sqlx::query!(
            "UPDATE users SET display_name = $1, updated_at = now() WHERE id = $2",
            name.trim(),
            id,
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    }

    if let Some(ref email_opt) = req.email {
        address_changed = apply_email_patch(
            &mut tx,
            id,
            current_user.user_id,
            &before,
            email_opt.as_deref(),
        )
        .await?;
    }

    if address_changed {
        retire_old_address(&state, &mut tx, id, &before, req.email == Some(None)).await?;
    }

    let row = sqlx::query_as!(
        UserResponse,
        r#"SELECT users.id,
                  users.display_name,
                  users.email,
                  users.role AS "role: Role",
                  users.is_child,
                  users.created_at,
                  users.updated_at,
                  (users.disabled_at IS NOT NULL) AS "disabled!",
                  (c.user_id IS NOT NULL) AS "has_password!",
                  l.expires_at AS "credential_link_expires_at?"
             FROM users
 LEFT JOIN local_credentials c ON c.user_id = users.id
 LEFT JOIN credential_links l ON l.user_id = users.id AND l.expires_at > now()
            WHERE users.id = $1"#,
        id,
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    tx.commit()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    Ok(axum::Json(row))
}
