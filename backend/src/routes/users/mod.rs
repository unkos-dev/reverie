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

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::auth::middleware::CurrentUser;
use crate::auth::scope::Scope;
use crate::error::AppError;
use crate::models::role::Role;
use crate::models::user::is_addr_spec;
use crate::state::AppState;

#[cfg(test)]
mod tests;

/// Build the `/api/v1/users*` router as an [`OpenApiRouter`] so each
/// handler's `#[utoipa::path]` contributes to the generated spec (a missing
/// annotation fails to compile). Merged into `crate::openapi::pilot_router`
/// and split into its runtime and spec halves there.
pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_users))
        .routes(routes!(create_user))
        .routes(routes!(update_role))
        .routes(routes!(update_child_status))
        .routes(routes!(update_user))
        .routes(routes!(update_account_status))
        .routes(routes!(admin_reset_password))
        .routes(routes!(change_own_password))
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
    #[serde(with = "time::serde::rfc3339")]
    created_at: OffsetDateTime,
    /// Last mutation timestamp.
    #[serde(with = "time::serde::rfc3339")]
    updated_at: OffsetDateTime,
    /// Whether the account is soft-disabled (cannot authenticate). Derived from
    /// `disabled_at`; the timestamp itself is not exposed on the wire.
    disabled: bool,
}

/// Defensive bound on the users list (the justified
/// single-page exception of `adr/2026-06-08-keyset-pagination-list-contract.md`). A household/self-hosted instance's user
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
    tag = "users",
    security(("session_cookie" = ["admin"]), ("device_token_bearer" = ["admin"]), ("oidc_jwt_bearer" = ["admin"]), ("opds_basic" = ["admin"])),
    responses(
        (status = 200, description = "All users, oldest first, defensively capped at 500 rows. Admin only.", body = [UserResponse]),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails),
        (status = 403, description = "Caller is not an admin", body = crate::openapi::ProblemDetails)
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
        r#"SELECT id,
                  display_name,
                  email,
                  role AS "role: Role",
                  is_child,
                  created_at,
                  updated_at,
                  (disabled_at IS NOT NULL) AS "disabled!"
             FROM users
            ORDER BY created_at ASC, id ASC
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
/// - [`AppError::Validation`] with message "cannot set role to child without
///   enabling child status first" when `role = child` on an `is_child = false`
///   user, or "cannot change role from child without disabling child status
///   first" when setting a non-child role on an `is_child = true` user.
/// - [`AppError::Internal`] on database errors.
#[utoipa::path(
    put,
    path = "/api/v1/users/{id}/role",
    tag = "users",
    security(("session_cookie" = ["admin"]), ("device_token_bearer" = ["admin"]), ("oidc_jwt_bearer" = ["admin"]), ("opds_basic" = ["admin"])),
    params(("id" = Uuid, Path, description = "Target user id")),
    request_body = UpdateRoleRequest,
    responses(
        (status = 200, description = "Updated user. The target's active sessions are invalidated. Admin only.", body = UserResponse),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails),
        (status = 403, description = "Caller is not an admin", body = crate::openapi::ProblemDetails),
        (status = 404, description = "Target user does not exist", body = crate::openapi::ProblemDetails),
        (status = 422, description = "Demotion would leave zero admins, the role change conflicts with the target's child status, or the request body is malformed / contains an unknown role value", body = crate::openapi::ProblemDetails)
    )
)]
async fn update_role(
    current_user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    body: Result<axum::Json<UpdateRoleRequest>, axum::extract::rejection::JsonRejection>,
) -> Result<impl IntoResponse, AppError> {
    current_user.require_scope(Scope::Admin)?;
    current_user.require_admin()?;
    let axum::Json(req) = body.map_err(|e| AppError::Validation(e.body_text()))?;

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

    // Check child/role sync constraint: setting role='child' on
    // is_child=false (or non-child role on is_child=true) will violate
    // chk_child_role_sync.
    if req.role == Role::Child && !target.is_child {
        return Err(AppError::Validation(
            "cannot set role to child without enabling child status first".into(),
        ));
    }
    if req.role != Role::Child && target.is_child {
        return Err(AppError::Validation(
            "cannot change role from child without disabling child status first".into(),
        ));
    }

    let row = sqlx::query_as!(
        UserResponse,
        r#"UPDATE users
              SET role = ($1::text)::user_role,
                  session_version = session_version + 1,
                  updated_at = now()
            WHERE id = $2
        RETURNING id,
                  display_name,
                  email,
                  role AS "role: Role",
                  is_child,
                  created_at,
                  updated_at,
                  (disabled_at IS NOT NULL) AS "disabled!""#,
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
    tag = "users",
    security(("session_cookie" = ["admin"]), ("device_token_bearer" = ["admin"]), ("oidc_jwt_bearer" = ["admin"]), ("opds_basic" = ["admin"])),
    params(("id" = Uuid, Path, description = "Target user id")),
    request_body = UpdateChildStatusRequest,
    responses(
        (status = 200, description = "Updated user. Enabling child status also sets role to `child`; disabling reverts `child` to `adult` (other roles unchanged). The target's active sessions are invalidated. Admin only.", body = UserResponse),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails),
        (status = 403, description = "Caller is not an admin", body = crate::openapi::ProblemDetails),
        (status = 404, description = "Target user does not exist", body = crate::openapi::ProblemDetails),
        (status = 422, description = "Marking the last admin as child would leave zero admins, or the request body is malformed", body = crate::openapi::ProblemDetails)
    )
)]
async fn update_child_status(
    current_user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    body: Result<axum::Json<UpdateChildStatusRequest>, axum::extract::rejection::JsonRejection>,
) -> Result<impl IntoResponse, AppError> {
    current_user.require_scope(Scope::Admin)?;
    current_user.require_admin()?;
    let axum::Json(req) = body.map_err(|e| AppError::Validation(e.body_text()))?;

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
        r#"UPDATE users
              SET is_child = $1,
                  role = ($2::text)::user_role,
                  session_version = session_version + 1,
                  updated_at = now()
            WHERE id = $3
        RETURNING id,
                  display_name,
                  email,
                  role AS "role: Role",
                  is_child,
                  created_at,
                  updated_at,
                  (disabled_at IS NOT NULL) AS "disabled!""#,
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
    /// Email address for the new account; must be a syntactically valid
    /// addr-spec and unique (case-insensitive).
    email: String,
    /// Human-readable display name.
    display_name: String,
    /// Role for the new account. `child` and `admin` accounts are created only
    /// here, by an existing administrator.
    role: Role,
    /// Initial password, typed by the admin and handed off out-of-band. Required;
    /// enforced against the password policy (length, zxcvbn floor, HIBP breach).
    password: String,
}

/// `POST /api/v1/users` — create an account with an admin-typed initial
/// password (admin only). This is the create-and-invite path: there is no
/// separate invite mechanism, no email, and no forced first-login change. The
/// admin relays the password out-of-band; the user may self-service change it.
///
/// # Errors
/// - [`AppError::Forbidden`] when the caller is not an admin or is a child.
/// - [`AppError::Validation`] (422) on an invalid email or a password that
///   fails the policy (too short/long, too weak, or breached).
/// - [`AppError::EmailConflict`] (409) when the email is already in use.
/// - [`AppError::Internal`] on database errors.
#[utoipa::path(
    post,
    path = "/api/v1/users",
    tag = "users",
    security(("session_cookie" = ["admin"]), ("device_token_bearer" = ["admin"]), ("oidc_jwt_bearer" = ["admin"]), ("opds_basic" = ["admin"])),
    request_body = CreateUserRequest,
    responses(
        (status = 201, description = "Created user. Admin only.", body = UserResponse),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails),
        (status = 403, description = "Caller is not an admin", body = crate::openapi::ProblemDetails),
        (status = 409, description = "Email already in use", body = crate::openapi::ProblemDetails),
        (status = 422, description = "Invalid email, or password rejected by the policy", body = crate::openapi::ProblemDetails)
    )
)]
async fn create_user(
    current_user: CurrentUser,
    State(state): State<AppState>,
    body: Result<axum::Json<CreateUserRequest>, axum::extract::rejection::JsonRejection>,
) -> Result<impl IntoResponse, AppError> {
    current_user.require_scope(Scope::Admin)?;
    current_user.require_admin()?;
    current_user.require_not_child()?;
    let axum::Json(req) = body.map_err(|e| AppError::Validation(e.body_text()))?;

    if !is_addr_spec(&req.email) {
        return Err(AppError::Validation("invalid email address".into()));
    }
    enforce_password_policy(&state, &req.password, &[&req.email, &req.display_name]).await?;
    let phc = crate::auth::password::hash_password(req.password.as_bytes())
        .map_err(|e| AppError::Internal(anyhow::anyhow!("password hash failed: {e}")))?;

    let user = crate::models::user::create_local(
        &state.pool,
        &req.email,
        &req.display_name,
        req.role,
        Some(&phc),
    )
    .await
    .map_err(|e| match e {
        crate::models::user::CreateUserError::EmailExists => AppError::EmailConflict,
        crate::models::user::CreateUserError::Db(db) => AppError::Internal(db.into()),
    })?;

    let body = UserResponse {
        id: user.id,
        display_name: user.display_name,
        email: user.email,
        role: user.role,
        is_child: user.is_child,
        created_at: user.created_at,
        updated_at: user.updated_at,
        disabled: user.disabled_at.is_some(),
    };
    Ok((StatusCode::CREATED, axum::Json(body)))
}

/// Body for `PUT /api/v1/users/{id}/account-status`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
struct AccountStatusRequest {
    /// `true` to soft-disable the account, `false` to re-enable it.
    disabled: bool,
}

/// `PUT /api/v1/users/{id}/account-status` — soft-disable or re-enable an
/// account (admin only). Disabling stamps `disabled_at`, bumps the target's
/// `session_version` (killing live sessions at once), and locks them out of
/// every auth path; re-enabling clears it.
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
    tag = "users",
    security(("session_cookie" = ["admin"]), ("device_token_bearer" = ["admin"]), ("oidc_jwt_bearer" = ["admin"]), ("opds_basic" = ["admin"])),
    params(("id" = Uuid, Path, description = "Target user id")),
    request_body = AccountStatusRequest,
    responses(
        (status = 200, description = "Updated user. Disabling invalidates the target's sessions. Admin only.", body = UserResponse),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails),
        (status = 403, description = "Caller is not an admin", body = crate::openapi::ProblemDetails),
        (status = 404, description = "Target user does not exist", body = crate::openapi::ProblemDetails),
        (status = 422, description = "Cannot disable your own account, or disabling would leave zero enabled admins", body = crate::openapi::ProblemDetails)
    )
)]
async fn update_account_status(
    current_user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    body: Result<axum::Json<AccountStatusRequest>, axum::extract::rejection::JsonRejection>,
) -> Result<impl IntoResponse, AppError> {
    current_user.require_scope(Scope::Admin)?;
    current_user.require_admin()?;
    current_user.require_not_child()?;
    let axum::Json(req) = body.map_err(|e| AppError::Validation(e.body_text()))?;

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
        r#"SELECT id,
                  display_name,
                  email,
                  role AS "role: Role",
                  is_child,
                  created_at,
                  updated_at,
                  (disabled_at IS NOT NULL) AS "disabled!"
             FROM users WHERE id = $1"#,
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

/// Body for `POST /api/v1/users/{id}/password-reset`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
struct AdminPasswordResetRequest {
    /// New password the admin sets for the target and relays out-of-band.
    /// Enforced against the password policy.
    new_password: String,
}

/// `POST /api/v1/users/{id}/password-reset` — an admin sets a new password for
/// a target account (admin only). Consistent with create: the admin types the
/// password and hands it off; no PIN, no email. Bumps the target's
/// `session_version` so their existing sessions are invalidated. Works for an
/// OIDC-only account too (the local credential is upserted).
///
/// # Errors
/// - [`AppError::Forbidden`] when the caller is not an admin or is a child.
/// - [`AppError::Validation`] (422) when the password fails the policy.
/// - [`AppError::NotFound`] when the target does not exist.
/// - [`AppError::Internal`] on database errors.
#[utoipa::path(
    post,
    path = "/api/v1/users/{id}/password-reset",
    tag = "users",
    security(("session_cookie" = ["admin"]), ("device_token_bearer" = ["admin"]), ("oidc_jwt_bearer" = ["admin"]), ("opds_basic" = ["admin"])),
    params(("id" = Uuid, Path, description = "Target user id")),
    request_body = AdminPasswordResetRequest,
    responses(
        (status = 200, description = "Password reset; the target's sessions are invalidated. Admin only."),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails),
        (status = 403, description = "Caller is not an admin", body = crate::openapi::ProblemDetails),
        (status = 404, description = "Target user does not exist", body = crate::openapi::ProblemDetails),
        (status = 422, description = "Password rejected by the policy", body = crate::openapi::ProblemDetails)
    )
)]
async fn admin_reset_password(
    current_user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    body: Result<axum::Json<AdminPasswordResetRequest>, axum::extract::rejection::JsonRejection>,
) -> Result<impl IntoResponse, AppError> {
    current_user.require_scope(Scope::Admin)?;
    current_user.require_admin()?;
    current_user.require_not_child()?;
    let axum::Json(req) = body.map_err(|e| AppError::Validation(e.body_text()))?;

    // Feed the target's own email and display name to the strength estimator so a
    // password echoing them is penalized. The authoritative existence check is the
    // FOR UPDATE below; this read only supplies the context words.
    let target = crate::models::user::find_by_id(&state.pool, id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?
        .ok_or(AppError::NotFound)?;
    let mut context: Vec<&str> = vec![target.display_name.as_str()];
    if let Some(email) = target.email.as_deref() {
        context.push(email);
    }
    enforce_password_policy(&state, &req.new_password, &context).await?;
    let phc = crate::auth::password::hash_password(req.new_password.as_bytes())
        .map_err(|e| AppError::Internal(anyhow::anyhow!("password hash failed: {e}")))?;

    let mut tx = state
        .pool
        .begin()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let exists = sqlx::query_scalar!("SELECT id FROM users WHERE id = $1 FOR UPDATE", id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    if exists.is_none() {
        return Err(AppError::NotFound);
    }

    crate::models::local_credentials::set_password(&mut *tx, id, &phc)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    crate::models::user::increment_session_version(&mut *tx, id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    tx.commit()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    Ok(StatusCode::OK)
}

/// Body for `POST /api/v1/account/password`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
struct ChangePasswordRequest {
    /// The caller's current password, re-verified before the change.
    current_password: String,
    /// The new password; enforced against the password policy.
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
///   in through an identity provider).
/// - [`AppError::Internal`] on database errors.
#[utoipa::path(
    post,
    path = "/api/v1/account/password",
    tag = "users",
    security(("session_cookie" = ["write"]), ("device_token_bearer" = ["write"]), ("oidc_jwt_bearer" = ["write"]), ("opds_basic" = ["write"])),
    request_body = ChangePasswordRequest,
    responses(
        (status = 200, description = "Password changed; all of the caller's sessions are invalidated."),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails),
        (status = 422, description = "Wrong current password, new password rejected by the policy, or no local credential", body = crate::openapi::ProblemDetails)
    )
)]
async fn change_own_password(
    current_user: CurrentUser,
    State(state): State<AppState>,
    body: Result<axum::Json<ChangePasswordRequest>, axum::extract::rejection::JsonRejection>,
) -> Result<impl IntoResponse, AppError> {
    current_user.require_scope(Scope::Write)?;
    let axum::Json(req) = body.map_err(|e| AppError::Validation(e.body_text()))?;
    let user_id = current_user.user_id;

    let credential = crate::models::local_credentials::find_by_user_id(&state.pool, user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?
        .ok_or_else(|| {
            AppError::Validation(
                "this account has no password to change; it signs in through an identity provider"
                    .to_owned(),
            )
        })?;

    if crate::auth::password::verify_password(
        req.current_password.as_bytes(),
        &credential.password_hash,
    )
    .is_err()
    {
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
    let phc = crate::auth::password::hash_password(req.new_password.as_bytes())
        .map_err(|e| AppError::Internal(anyhow::anyhow!("password hash failed: {e}")))?;

    let mut tx = state
        .pool
        .begin()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    crate::models::local_credentials::set_password(&mut *tx, user_id, &phc)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    crate::models::user::increment_session_version(&mut *tx, user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
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
    /// New email (RFC 5322 addr-spec). Absent = unchanged; explicit `null`
    /// clears the stored address.
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
/// value and any non-addr-spec form (display-name, domain-literal — see
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
    tag = "users",
    security(("session_cookie" = ["admin"]), ("device_token_bearer" = ["admin"]), ("oidc_jwt_bearer" = ["admin"]), ("opds_basic" = ["admin"])),
    params(("id" = Uuid, Path, description = "Target user id")),
    request_body(content = UpdateUserRequest, description = "RFC 7396 JSON Merge Patch: absent fields are unchanged; explicit `null` clears `email` and is rejected for `display_name`"),
    responses(
        (status = 200, description = "Updated user. Does not invalidate the target's sessions (neither field gates access). Admin only.", body = UserResponse),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails),
        (status = 403, description = "Caller is not an admin", body = crate::openapi::ProblemDetails),
        (status = 404, description = "Target user does not exist", body = crate::openapi::ProblemDetails),
        (status = 422, description = "Null/empty display_name, malformed email, or email already in use", body = crate::openapi::ProblemDetails)
    )
)]
async fn update_user(
    current_user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    body: Result<axum::Json<UpdateUserRequest>, axum::extract::rejection::JsonRejection>,
) -> Result<impl IntoResponse, AppError> {
    current_user.require_scope(Scope::Admin)?;
    current_user.require_admin()?;
    let axum::Json(req) = body.map_err(|e| AppError::Validation(e.body_text()))?;

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

    // Verify target exists.
    let exists = sqlx::query_scalar!("SELECT id FROM users WHERE id = $1 FOR UPDATE", id,)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    if exists.is_none() {
        return Err(AppError::NotFound);
    }

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

    // Email is not an access-control input: login identity resolves through
    // user_identities keyed on `(issuer, subject)`, not email; RLS keys on user
    // id/role/`is_child`, and the session auth hash is `session_version` only.
    // So changing or clearing email does not bump `session_version` — no active
    // session needs invalidating. The uniqueness constraint is still enforced
    // below for the set case.
    if let Some(ref email_opt) = req.email {
        match email_opt {
            None => {
                // Clear email — no session_version bump (email gates nothing).
                sqlx::query!(
                    "UPDATE users SET email = NULL, updated_at = now() WHERE id = $1",
                    id,
                )
                .execute(&mut *tx)
                .await
                .map_err(|e| AppError::Internal(e.into()))?;
            }
            Some(email) => {
                let trimmed = validate_patch_email(email, current_user.user_id, id)?;
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
                sqlx::query!(
                    "UPDATE users SET email = $1, updated_at = now() WHERE id = $2",
                    trimmed,
                    id,
                )
                .execute(&mut *tx)
                .await
                // Proactive SELECT covers the common case; translate any
                // race-escaped unique violation to 422 rather than 500.
                .map_err(|e| unique_violation_or_internal(e, "email already in use"))?;
            }
        }
    }

    let row = sqlx::query_as!(
        UserResponse,
        r#"SELECT id,
                  display_name,
                  email,
                  role AS "role: Role",
                  is_child,
                  created_at,
                  updated_at,
                  (disabled_at IS NOT NULL) AS "disabled!"
             FROM users
            WHERE id = $1"#,
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
