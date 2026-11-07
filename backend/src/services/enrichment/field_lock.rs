//! CRUD helpers for `field_locks`.
//!
//! A lock pins a specific (manifestation, `entity_type`, field) so the policy
//! engine's `decide` silently discards incoming observations for it.
//! The orchestrator pre-resolves locks before calling into `policy::decide`
//! so the policy module stays pure.

use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

/// Entity type string written into `field_locks.entity_type`.
/// `"work"` means the field lives on `works`; `"manifestation"` means
/// `manifestations`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntityType {
    /// The field belongs to the `works` table (e.g. `"title"`, `"language"`).
    Work,
    /// The field belongs to the `manifestations` table (e.g. `"isbn_13"`, `"publisher"`).
    Manifestation,
}

impl EntityType {
    /// Return the string value written to `field_locks.entity_type`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Work => "work",
            Self::Manifestation => "manifestation",
        }
    }
}

/// Check whether a specific `(manifestation_id, entity_type, field)` triple is locked.
///
/// Returns `true` if a matching row exists in `field_locks`; `false` otherwise.
///
/// # Errors
///
/// Returns a [`sqlx::Error`] if the query fails (connection error, pool timeout, etc.).
pub async fn is_locked(
    pool: &PgPool,
    manifestation_id: Uuid,
    entity_type: EntityType,
    field: &str,
) -> sqlx::Result<bool> {
    let hit = sqlx::query_scalar!(
        "SELECT 1 AS \"exists!\" FROM field_locks \
         WHERE manifestation_id = $1 AND entity_type = $2 AND field_name = $3",
        manifestation_id,
        entity_type.as_str(),
        field,
    )
    .fetch_optional(pool)
    .await?;
    Ok(hit.is_some())
}

/// Same as [`is_locked`] but reads within an open transaction.
///
/// # Errors
///
/// Returns a [`sqlx::Error`] if the query fails.
pub async fn is_locked_tx(
    conn: &mut PgConnection,
    manifestation_id: Uuid,
    entity_type: EntityType,
    field: &str,
) -> sqlx::Result<bool> {
    let hit = sqlx::query_scalar!(
        "SELECT 1 AS \"exists!\" FROM field_locks \
         WHERE manifestation_id = $1 AND entity_type = $2 AND field_name = $3",
        manifestation_id,
        entity_type.as_str(),
        field,
    )
    .fetch_optional(&mut *conn)
    .await?;
    Ok(hit.is_some())
}

/// Load matching edition locks for bounded sets of canonical field names.
///
/// # Errors
/// Returns database errors from the lookup.
pub async fn locked_fields_tx(
    conn: &mut PgConnection,
    manifestation_id: Uuid,
    work_fields: &[String],
    manifestation_fields: &[String],
) -> sqlx::Result<Vec<String>> {
    sqlx::query_scalar!(
        "SELECT field_name FROM field_locks WHERE manifestation_id = $1 \
         AND ((entity_type = 'work' AND field_name = ANY($2)) \
           OR (entity_type = 'manifestation' AND field_name = ANY($3))) \
         ORDER BY field_name",
        manifestation_id,
        work_fields,
        manifestation_fields,
    )
    .fetch_all(conn)
    .await
}

/// Insert a lock on the caller's connection; duplicate locks are ignored.
///
/// The caller owns the manifestation visibility check and transaction.
///
/// # Errors
///
/// Returns a [`sqlx::Error`] if the insert fails.
pub async fn lock_tx(
    conn: &mut PgConnection,
    manifestation_id: Uuid,
    entity_type: EntityType,
    field: &str,
    user_id: Uuid,
) -> sqlx::Result<()> {
    sqlx::query!(
        "INSERT INTO field_locks (manifestation_id, entity_type, field_name, locked_by) \
         VALUES ($1, $2, $3, $4) \
         ON CONFLICT (manifestation_id, entity_type, field_name) DO NOTHING",
        manifestation_id,
        entity_type.as_str(),
        field,
        user_id,
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Remove a lock on the caller's connection, returning whether a row was deleted.
///
/// The caller owns the manifestation visibility check and transaction.
///
/// # Errors
///
/// Returns a [`sqlx::Error`] if the delete query fails.
pub async fn unlock_tx(
    conn: &mut PgConnection,
    manifestation_id: Uuid,
    entity_type: EntityType,
    field: &str,
) -> sqlx::Result<bool> {
    let result = sqlx::query!(
        "DELETE FROM field_locks \
         WHERE manifestation_id = $1 AND entity_type = $2 AND field_name = $3",
        manifestation_id,
        entity_type.as_str(),
        field,
    )
    .execute(&mut *conn)
    .await?;
    Ok(result.rows_affected() > 0)
}
