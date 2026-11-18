//! Owner-scoped shelf and membership changes.
//!
//! THREAT: shelf tables lack RLS; every target lookup retains an explicit owner predicate.

use crate::auth::{middleware::CurrentUser, scope::Scope};
use crate::models::shelf::Shelf;
use crate::{error::AppError, state::AppState};
use uuid::Uuid;

pub async fn create_shelf(
    current_user: &CurrentUser,
    state: &AppState,
    name: &str,
) -> Result<Shelf, AppError> {
    current_user.require_scope(Scope::Write)?;
    current_user.require_not_child()?;
    let name = name.trim();
    if name.is_empty() {
        return Err(AppError::Validation("shelf name must not be empty".into()));
    }

    let row = sqlx::query!(
        r#"
        INSERT INTO shelves (user_id, name)
        VALUES ($1, $2)
        RETURNING id           AS "id!",
                  name         AS "name!",
                  is_system    AS "is_system!",
                  created_at   AS "created_at!",
                  updated_at   AS "updated_at!"
        "#,
        current_user.user_id,
        name,
    )
    .fetch_one(&state.pool)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    let shelf = Shelf {
        id: row.id,
        name: row.name,
        is_system: row.is_system,
        created_at: row.created_at.to_jiff(),
        updated_at: row.updated_at.to_jiff(),
        item_count: 0,
    };

    Ok(shelf)
}

pub async fn rename_shelf(
    current_user: &CurrentUser,
    state: &AppState,
    id: Uuid,
    name: &str,
) -> Result<Shelf, AppError> {
    current_user.require_scope(Scope::Write)?;
    current_user.require_not_child()?;
    let name = name.trim();
    if name.is_empty() {
        return Err(AppError::Validation("shelf name must not be empty".into()));
    }

    let mut tx = state
        .pool
        .begin()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    let existing = sqlx::query!(
        r#"SELECT is_system AS "is_system!" FROM shelves
           WHERE id = $1 AND user_id = $2 FOR UPDATE"#,
        id,
        current_user.user_id,
    )
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?
    .ok_or(AppError::NotFound)?;
    if existing.is_system {
        return Err(AppError::SystemShelfImmutable);
    }
    let row = sqlx::query!(
        r#"
        UPDATE shelves SET name = $1
        WHERE id = $2 AND user_id = $3
        RETURNING id           AS "id!",
                  name         AS "name!",
                  is_system    AS "is_system!",
                  created_at   AS "created_at!",
                  updated_at   AS "updated_at!"
        "#,
        name,
        id,
        current_user.user_id,
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    let item_count: i64 = sqlx::query_scalar!(
        r#"SELECT COUNT(*) AS "n!" FROM shelf_items WHERE shelf_id = $1"#,
        id,
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    tx.commit()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let shelf = Shelf {
        id: row.id,
        name: row.name,
        is_system: row.is_system,
        created_at: row.created_at.to_jiff(),
        updated_at: row.updated_at.to_jiff(),
        item_count,
    };
    Ok(shelf)
}

pub async fn delete_shelf(
    current_user: &CurrentUser,
    state: &AppState,
    id: Uuid,
) -> Result<(), AppError> {
    current_user.require_scope(Scope::Write)?;
    current_user.require_not_child()?;
    let mut tx = state
        .pool
        .begin()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    let row = sqlx::query!(
        r#"SELECT is_system AS "is_system!" FROM shelves
           WHERE id = $1 AND user_id = $2 FOR UPDATE"#,
        id,
        current_user.user_id,
    )
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?
    .ok_or(AppError::NotFound)?;
    if row.is_system {
        return Err(AppError::SystemShelfImmutable);
    }
    sqlx::query!("DELETE FROM shelves WHERE id = $1", id)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    tx.commit()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    Ok(())
}

pub async fn add_shelf_item(
    current_user: &CurrentUser,
    state: &AppState,
    id: Uuid,
    manifestation_id: Uuid,
) -> Result<(), AppError> {
    current_user.require_scope(Scope::Write)?;

    // RLS-scoped probe: confirm the caller can see the target
    // manifestation. Without this an attacker could brute-force the
    // manifestation-id space by feeding random UUIDs and watching for
    // a 200 vs a 404.
    {
        let mut rls_tx = crate::db::acquire_with_rls(&state.pool, current_user.user_id)
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
        let visible: Option<Uuid> = sqlx::query_scalar!(
            "SELECT id FROM manifestations WHERE id = $1",
            manifestation_id,
        )
        .fetch_optional(&mut *rls_tx)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
        if visible.is_none() {
            return Err(AppError::NotFound);
        }
    }

    let mut tx = state
        .pool
        .begin()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    let owned = sqlx::query_scalar!(
        "SELECT id FROM shelves WHERE id = $1 AND user_id = $2 FOR UPDATE",
        id,
        current_user.user_id,
    )
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    if owned.is_none() {
        return Err(AppError::NotFound);
    }
    sqlx::query!(
        "INSERT INTO shelf_items (shelf_id, manifestation_id) \
         VALUES ($1, $2) \
         ON CONFLICT (shelf_id, manifestation_id) DO NOTHING",
        id,
        manifestation_id,
    )
    .execute(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    sqlx::query!("UPDATE shelves SET updated_at = now() WHERE id = $1", id)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    tx.commit()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    Ok(())
}

pub async fn remove_shelf_item(
    current_user: &CurrentUser,
    state: &AppState,
    shelf_id: Uuid,
    manifestation_id: Uuid,
) -> Result<(), AppError> {
    current_user.require_scope(Scope::Write)?;
    let mut tx = state
        .pool
        .begin()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    let owned = sqlx::query_scalar!(
        "SELECT id FROM shelves WHERE id = $1 AND user_id = $2 FOR UPDATE",
        shelf_id,
        current_user.user_id,
    )
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    if owned.is_none() {
        return Err(AppError::NotFound);
    }
    let deleted = sqlx::query!(
        "DELETE FROM shelf_items WHERE shelf_id = $1 AND manifestation_id = $2",
        shelf_id,
        manifestation_id,
    )
    .execute(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    if deleted.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    sqlx::query!(
        "UPDATE shelves SET updated_at = now() WHERE id = $1",
        shelf_id
    )
    .execute(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    tx.commit()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    Ok(())
}
