//! `/api/users*` admin-only user management routes.
//!
//! THREAT: Privilege escalation and horizontal privilege abuse. All mutations
//! are admin-gated via `require_admin()`; `session_version` bumps ensure role
//! changes take effect on active sessions immediately.
//!
//! All endpoints require `role = admin`; non-admin callers receive
//! `AppError::Forbidden` (403).
//!
//! Session invalidation policy: `users.session_version` is bumped in the same
//! transaction for mutations that affect access-control or identity matching:
//! - `PUT …/role` — role governs RLS visibility and admin gates.
//! - `PUT …/child-status` — child flag controls content-visibility rules.
//! - `PATCH …` email field — email is used for OIDC provider matching; a
//!   stale email in an active session would bind the session to the wrong
//!   identity on next OIDC login.
//! `display_name` changes do not bump session_version (cosmetic only).
//!
//! # Last-admin protection (TOCTOU-safe)
//!
//! `PUT /api/users/{id}/role` and `PUT /api/users/{id}/child-status` —
//! acquire `SELECT … FOR UPDATE` on all admin rows (`ORDER BY id`) first,
//! then lock the target row. Consistent lock order (admin rows always
//! before target) prevents deadlock when two concurrent demotions each
//! hold a different admin row and wait for the other. Under READ COMMITTED
//! the second transaction sees the first's committed state and rejects
//! with 422 "would leave zero admins".

use axum::Router;
use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::routing::{get, patch, put};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use email_address::EmailAddress;

use crate::auth::middleware::CurrentUser;
use crate::error::AppError;
use crate::models::role::Role;
use crate::state::AppState;

#[cfg(test)]
mod tests;

/// Build the `/api/users*` router.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/users", get(list_users))
        .route("/api/users/{id}/role", put(update_role))
        .route("/api/users/{id}/child-status", put(update_child_status))
        .route("/api/users/{id}", patch(update_user))
}

/// Wire-format user row returned by list and mutation endpoints.
#[derive(Debug, Serialize)]
struct UserResponse {
    id: Uuid,
    display_name: String,
    email: Option<String>,
    role: Role,
    is_child: bool,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
}

/// `GET /api/users` — list all users (admin only).
///
/// # Errors
/// - [`AppError::Forbidden`] when the caller is not an admin.
/// - [`AppError::Internal`] on database errors.
async fn list_users(
    current_user: CurrentUser,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    current_user.require_admin()?;

    let rows = sqlx::query_as!(
        UserResponse,
        r#"SELECT id,
                  display_name,
                  email,
                  role AS "role: Role",
                  is_child,
                  created_at,
                  updated_at
             FROM users
            ORDER BY created_at ASC"#,
    )
    .fetch_all(&state.pool)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    Ok(axum::Json(rows))
}

/// Body for `PUT /api/users/{id}/role`.
#[derive(Debug, Deserialize)]
struct UpdateRoleRequest {
    role: Role,
}

/// `PUT /api/users/{id}/role` — change a user's role (admin only).
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
async fn update_role(
    current_user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    body: Result<axum::Json<UpdateRoleRequest>, axum::extract::rejection::JsonRejection>,
) -> Result<impl IntoResponse, AppError> {
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
                  updated_at"#,
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

/// Body for `PUT /api/users/{id}/child-status`.
#[derive(Debug, Deserialize)]
struct UpdateChildStatusRequest {
    is_child: bool,
}

/// `PUT /api/users/{id}/child-status` — toggle child status (admin only).
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
async fn update_child_status(
    current_user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    body: Result<axum::Json<UpdateChildStatusRequest>, axum::extract::rejection::JsonRejection>,
) -> Result<impl IntoResponse, AppError> {
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
                  updated_at"#,
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

/// Body for `PATCH /api/users/{id}`.
///
/// RFC 7396 JSON Merge Patch: absent keys are untouched; explicit
/// `null` on `email` clears; `null` on `display_name` is rejected
/// (NOT NULL column).
#[derive(Debug, Deserialize)]
#[allow(clippy::option_option)] // RFC 7396: None = absent, Some(None) = null, Some(Some) = value
struct UpdateUserRequest {
    #[serde(default, deserialize_with = "deserialize_optional_string")]
    display_name: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_optional_string")]
    email: Option<Option<String>>,
}

#[allow(clippy::option_option)]
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

/// `PATCH /api/users/{id}` — update `display_name` / `email` (admin only).
///
/// Bumps `session_version` when `email` is changed or cleared (OIDC identity
/// matching depends on email; see module-level session-invalidation policy).
/// `display_name` changes do not bump `session_version`.
///
/// # Errors
/// - [`AppError::Forbidden`] when the caller is not an admin.
/// - [`AppError::NotFound`] when the target user does not exist.
/// - [`AppError::Validation`] when `display_name` is null or empty, when
///   `email` is not a valid RFC 5322 address, or when `email` is already
///   in use by another user.
/// - [`AppError::Internal`] on database errors.
async fn update_user(
    current_user: CurrentUser,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    body: Result<axum::Json<UpdateUserRequest>, axum::extract::rejection::JsonRejection>,
) -> Result<impl IntoResponse, AppError> {
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

    // THREAT: email is the OIDC provider-matching key. Updating or clearing it
    // without bumping session_version would bind active sessions to the old
    // identity on next OIDC login.
    if let Some(ref email_opt) = req.email {
        match email_opt {
            None => {
                // Clear email. Bumps session_version — see module-level
                // session-invalidation policy (OIDC matching depends on email).
                sqlx::query!(
                    "UPDATE users SET email = NULL, session_version = session_version + 1, updated_at = now() WHERE id = $1",
                    id,
                )
                .execute(&mut *tx)
                .await
                .map_err(|e| AppError::Internal(e.into()))?;
            }
            Some(email) => {
                let trimmed = email.trim();
                if trimmed.is_empty() {
                    return Err(AppError::Validation("email must not be empty".into()));
                }
                if !EmailAddress::is_valid(trimmed) {
                    return Err(AppError::Validation("email must be a valid address".into()));
                }
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
                    "UPDATE users SET email = $1, session_version = session_version + 1, updated_at = now() WHERE id = $2",
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
                  updated_at
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
