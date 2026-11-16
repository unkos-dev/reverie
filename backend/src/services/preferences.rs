//! Caller-owned sparse library preferences shared by HTTP and GraphQL.
//!
//! THREAT: caller scope and RLS confine every operation to the authenticated user's row.

use crate::auth::{middleware::CurrentUser, scope::Scope};
use crate::models::user_preferences::{
    LibraryDensity, LibraryView, PreferenceDefaults, PreferenceOverrides, validate_hidden_columns,
    validate_sort_stack,
};
use crate::{db, error::AppError};
use serde::Deserialize;
use sqlx::PgPool;

/// Both endpoints answer with the caller's overrides and the installation
/// defaults together, so the client never has to hold a second copy of the
/// defaults and cannot drift from the server's.
#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
pub struct PreferencesResponse {
    /// The caller's overrides; a `null` group inherits from `defaults`.
    #[serde(flatten)]
    pub(crate) overrides: PreferenceOverrides,
    /// Installation defaults in force for every group the caller has not
    /// overridden.
    pub(crate) defaults: PreferenceDefaults,
}

impl PreferencesResponse {
    const fn new(overrides: PreferenceOverrides) -> Self {
        Self {
            overrides,
            defaults: PreferenceDefaults::installation(),
        }
    }
}

/// Body for `PATCH /auth/me/preferences`. RFC 7396 JSON Merge Patch: an
/// absent key leaves that group unchanged, an explicit `null` resets it to
/// the installation default.
#[expect(
    clippy::option_option,
    reason = "RFC 7396 sparse-update encoding: outer Option distinguishes absent (None) from present-and-null (Some(None)), which is the whole reset mechanism"
)]
#[derive(Debug, Default, Deserialize, utoipa::ToSchema)]
pub struct UpdatePreferencesRequest {
    /// Column keys to hide. Absent = unchanged; `null` resets to the default.
    #[serde(default, with = "::serde_with::rust::double_option")]
    #[schema(value_type = Option<Vec<String>>, max_items = 64, max_length = 64)]
    pub(crate) hidden_columns: Option<Option<Vec<String>>>,
    /// Row density. Absent = unchanged; `null` resets to the default.
    #[serde(default, with = "::serde_with::rust::double_option")]
    #[schema(value_type = Option<LibraryDensity>)]
    pub(crate) density: Option<Option<LibraryDensity>>,
    /// Library view. Absent = unchanged; `null` resets to the default.
    #[serde(default, with = "::serde_with::rust::double_option")]
    #[schema(value_type = Option<LibraryView>)]
    pub(crate) view: Option<Option<LibraryView>>,
    /// Default sort in the `?sort=` wire grammar. Absent = unchanged; `null`
    /// resets to the default.
    #[serde(default, with = "::serde_with::rust::double_option")]
    #[schema(value_type = Option<String>, max_length = 26)]
    pub(crate) sort_stack: Option<Option<String>>,
}

impl UpdatePreferencesRequest {
    /// Reject a body that names no group. A patch that changes nothing is a
    /// client bug, and answering 200 to it would hide the bug behind a
    /// successful-looking round trip.
    const fn names_a_group(&self) -> bool {
        self.hidden_columns.is_some()
            || self.density.is_some()
            || self.view.is_some()
            || self.sort_stack.is_some()
    }

    /// Check every submitted value before any of them is written, so a
    /// rejected patch persists nothing at all.
    fn validate(&self) -> Result<(), AppError> {
        if let Some(Some(keys)) = &self.hidden_columns {
            validate_hidden_columns(keys).map_err(|e| AppError::Validation(e.to_string()))?;
        }
        if let Some(Some(raw)) = &self.sort_stack {
            validate_sort_stack(raw).map_err(|e| AppError::Validation(e.to_string()))?;
        }
        Ok(())
    }
}

pub async fn get_preferences(
    pool: &PgPool,
    current_user: &CurrentUser,
) -> Result<PreferencesResponse, AppError> {
    current_user.require_scope(Scope::Read)?;

    let mut tx = db::acquire_with_rls(pool, current_user.user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let row = sqlx::query_as!(
        PreferenceOverrides,
        r#"SELECT hidden_columns,
                  density AS "density?: LibraryDensity",
                  view AS "view?: LibraryView",
                  sort_stack
             FROM user_preferences
            WHERE user_id = $1"#,
        current_user.user_id,
    )
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    tx.commit()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    Ok(PreferencesResponse::new(row.unwrap_or_default()))
}

pub async fn update_preferences(
    pool: &PgPool,
    current_user: &CurrentUser,
    req: UpdatePreferencesRequest,
) -> Result<PreferencesResponse, AppError> {
    current_user.require_scope(Scope::Write)?;

    if !req.names_a_group() {
        return Err(AppError::Validation("no fields".into()));
    }
    req.validate()?;

    // Bound before the query so the borrows outlive the macro's arguments.
    let hidden_columns = req.hidden_columns.clone().flatten();
    let sort_stack = req.sort_stack.clone().flatten();

    let mut tx = db::acquire_with_rls(pool, current_user.user_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    // One statement rather than read-modify-write. Each group travels as a
    // value plus a "was it named?" flag, so the UPDATE arm can tell an
    // absent group (keep what is stored) from an explicit null (reset),
    // which a merged struct written back wholesale could not. It also means
    // two concurrent patches serialise on the row instead of racing between
    // a read and a write, which is what keeps last-write-wins from ever
    // persisting half of one patch and half of the other.
    let updated = sqlx::query_as!(
        PreferenceOverrides,
        r#"INSERT INTO user_preferences (user_id, hidden_columns, density, view, sort_stack)
                VALUES ($1, $2, $3, $4, $5)
           ON CONFLICT (user_id) DO UPDATE SET
                hidden_columns = CASE WHEN $6 THEN EXCLUDED.hidden_columns
                                      ELSE user_preferences.hidden_columns END,
                density        = CASE WHEN $7 THEN EXCLUDED.density
                                      ELSE user_preferences.density END,
                view           = CASE WHEN $8 THEN EXCLUDED.view
                                      ELSE user_preferences.view END,
                sort_stack     = CASE WHEN $9 THEN EXCLUDED.sort_stack
                                      ELSE user_preferences.sort_stack END,
                updated_at     = now()
             RETURNING hidden_columns,
                       density AS "density?: LibraryDensity",
                       view AS "view?: LibraryView",
                       sort_stack"#,
        current_user.user_id,
        hidden_columns.as_deref(),
        req.density.flatten() as Option<LibraryDensity>,
        req.view.flatten() as Option<LibraryView>,
        sort_stack.as_deref(),
        req.hidden_columns.is_some(),
        req.density.is_some(),
        req.view.is_some(),
        req.sort_stack.is_some(),
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    tx.commit()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    Ok(PreferencesResponse::new(updated))
}
