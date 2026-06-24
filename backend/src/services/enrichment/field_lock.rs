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

/// Insert a lock row for `(manifestation_id, entity_type, field)`, recording the
/// user who set the lock in `locked_by`.
///
/// Idempotent: a duplicate lock on the same triple is silently ignored (`ON CONFLICT DO NOTHING`).
///
/// # Errors
///
/// Returns a [`sqlx::Error`] if the insert fails for any reason other than a
/// duplicate-key conflict.
pub async fn lock(
    pool: &PgPool,
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
    .execute(pool)
    .await?;
    Ok(())
}

/// Remove a lock. Returns `true` if a row was deleted, `false` if none
/// existed (callers may surface 404).
///
/// # Errors
///
/// Returns a [`sqlx::Error`] if the delete query fails.
pub async fn unlock(
    pool: &PgPool,
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
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::db::{app_pool_for, ingestion_pool_for};

    async fn setup_fixture(pool: &PgPool) -> (Uuid, Uuid) {
        let work_id = sqlx::query_scalar!(
            "INSERT INTO works (title, sort_title) VALUES ('fl_test', 'fl_test') RETURNING id",
        )
        .fetch_one(pool)
        .await
        .unwrap();
        let file_path = format!("/tmp/fl-test-{work_id}.epub");
        let hash = format!("hash-fl-{work_id}");
        let m_id = sqlx::query_scalar!(
            "INSERT INTO manifestations \
             (work_id, format, file_path, ingestion_file_hash, current_file_hash, \
              file_size_bytes, ingestion_status, validation_status) \
             VALUES ($1, 'epub'::manifestation_format, $2, $3, $3, 100, \
                     'complete'::ingestion_status, 'clean'::validation_status) \
             RETURNING id",
            work_id,
            file_path,
            hash,
        )
        .fetch_one(pool)
        .await
        .unwrap();
        (work_id, m_id)
    }

    async fn a_user(pool: &PgPool) -> Uuid {
        let display_name = format!("lock-test-{}", Uuid::new_v4());
        sqlx::query_scalar!(
            "INSERT INTO users (display_name) VALUES ($1) RETURNING id",
            display_name,
        )
        .fetch_one(pool)
        .await
        .unwrap()
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn lock_unlock_roundtrip(pool: PgPool) {
        let ingestion = ingestion_pool_for(&pool).await;
        let app = app_pool_for(&pool).await;

        let (_work_id, m_id) = setup_fixture(&ingestion).await;
        let user_id = a_user(&app).await;

        assert!(
            !is_locked(&app, m_id, EntityType::Work, "title")
                .await
                .unwrap()
        );

        lock(&app, m_id, EntityType::Work, "title", user_id)
            .await
            .unwrap();
        assert!(
            is_locked(&app, m_id, EntityType::Work, "title")
                .await
                .unwrap()
        );

        lock(&app, m_id, EntityType::Work, "title", user_id)
            .await
            .unwrap();

        let removed = unlock(&app, m_id, EntityType::Work, "title").await.unwrap();
        assert!(removed);
        assert!(
            !is_locked(&app, m_id, EntityType::Work, "title")
                .await
                .unwrap()
        );

        let removed = unlock(&app, m_id, EntityType::Work, "title").await.unwrap();
        assert!(!removed, "second unlock should report no-op");
    }
}
