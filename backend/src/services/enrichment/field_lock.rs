//! CRUD helpers for `field_locks`.
//!
//! A lock pins a specific (manifestation, entity_type, field) so the policy
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
    Work,
    Manifestation,
}

impl EntityType {
    pub fn as_str(self) -> &'static str {
        match self {
            EntityType::Work => "work",
            EntityType::Manifestation => "manifestation",
        }
    }
}

pub async fn is_locked(
    pool: &PgPool,
    manifestation_id: Uuid,
    entity_type: EntityType,
    field: &str,
) -> sqlx::Result<bool> {
    let hit: Option<Uuid> = sqlx::query_scalar(
        "SELECT manifestation_id FROM field_locks \
         WHERE manifestation_id = $1 AND entity_type = $2 AND field_name = $3",
    )
    .bind(manifestation_id)
    .bind(entity_type.as_str())
    .bind(field)
    .fetch_optional(pool)
    .await?;
    Ok(hit.is_some())
}

/// Same as [`is_locked`] but reads within an open transaction.
pub async fn is_locked_tx(
    conn: &mut PgConnection,
    manifestation_id: Uuid,
    entity_type: EntityType,
    field: &str,
) -> sqlx::Result<bool> {
    let hit: Option<Uuid> = sqlx::query_scalar(
        "SELECT manifestation_id FROM field_locks \
         WHERE manifestation_id = $1 AND entity_type = $2 AND field_name = $3",
    )
    .bind(manifestation_id)
    .bind(entity_type.as_str())
    .bind(field)
    .fetch_optional(&mut *conn)
    .await?;
    Ok(hit.is_some())
}

pub async fn lock(
    pool: &PgPool,
    manifestation_id: Uuid,
    entity_type: EntityType,
    field: &str,
    user_id: Uuid,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO field_locks (manifestation_id, entity_type, field_name, locked_by) \
         VALUES ($1, $2, $3, $4) \
         ON CONFLICT (manifestation_id, entity_type, field_name) DO NOTHING",
    )
    .bind(manifestation_id)
    .bind(entity_type.as_str())
    .bind(field)
    .bind(user_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Remove a lock. Returns `true` if a row was deleted, `false` if none
/// existed (callers may surface 404).
pub async fn unlock(
    pool: &PgPool,
    manifestation_id: Uuid,
    entity_type: EntityType,
    field: &str,
) -> sqlx::Result<bool> {
    let result = sqlx::query(
        "DELETE FROM field_locks \
         WHERE manifestation_id = $1 AND entity_type = $2 AND field_name = $3",
    )
    .bind(manifestation_id)
    .bind(entity_type.as_str())
    .bind(field)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// tome_ingestion URL for fixture INSERTs.  The role holds the
    /// `manifestations_ingestion_full_access` RLS policy, so it can insert
    /// manifestations without setting an `app.current_user_id` session var.
    /// The companion migration 20260417000002 grants it SELECT on
    /// `field_locks` for read-side assertions.
    fn ingestion_db_url() -> String {
        std::env::var("DATABASE_URL_INGESTION").unwrap_or_else(|_| {
            "postgres://tome_ingestion:tome_ingestion@localhost:5433/tome_dev".into()
        })
    }

    /// tome_app URL for `field_locks` writes.  The migration deliberately
    /// restricts lock/unlock to this role — tome_ingestion only has SELECT.
    fn app_db_url() -> String {
        std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://tome_app:tome_app@localhost:5433/tome_dev".into())
    }

    async fn setup_fixture(pool: &PgPool) -> (Uuid, Uuid) {
        let work_id: Uuid = sqlx::query_scalar(
            "INSERT INTO works (title, sort_title) VALUES ('fl_test', 'fl_test') RETURNING id",
        )
        .fetch_one(pool)
        .await
        .unwrap();
        let m_id: Uuid = sqlx::query_scalar(
            "INSERT INTO manifestations \
             (work_id, format, file_path, file_hash, file_size_bytes, \
              ingestion_status, validation_status) \
             VALUES ($1, 'epub'::manifestation_format, $2, $3, 100, \
                     'complete'::ingestion_status, 'valid'::validation_status) \
             RETURNING id",
        )
        .bind(work_id)
        .bind(format!("/tmp/fl-test-{work_id}.epub"))
        .bind(format!("hash-fl-{work_id}"))
        .fetch_one(pool)
        .await
        .unwrap();
        (work_id, m_id)
    }

    async fn cleanup(pool: &PgPool, work_id: Uuid, m_id: Uuid) {
        let _ = sqlx::query("DELETE FROM field_locks WHERE manifestation_id = $1")
            .bind(m_id)
            .execute(pool)
            .await;
        let _ = sqlx::query("DELETE FROM manifestations WHERE id = $1")
            .bind(m_id)
            .execute(pool)
            .await;
        let _ = sqlx::query("DELETE FROM works WHERE id = $1")
            .bind(work_id)
            .execute(pool)
            .await;
    }

    async fn a_user(pool: &PgPool) -> Uuid {
        sqlx::query_scalar(
            "INSERT INTO users (oidc_subject, email, display_name, role, is_child) \
             VALUES ($1, $2, 'lock-test', 'adult'::user_role, false) \
             RETURNING id",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(format!("lock-test-{}@example.com", Uuid::new_v4()))
        .fetch_one(pool)
        .await
        .unwrap()
    }

    #[tokio::test]
    #[ignore]
    async fn lock_unlock_roundtrip() {
        // tome_ingestion: fixture INSERTs on manifestations/works/users
        // (bypasses app.current_user_id RLS check).
        let ingestion = PgPool::connect(&ingestion_db_url()).await.unwrap();
        // tome_app: field_locks writes (tome_ingestion only has SELECT).
        let app = PgPool::connect(&app_db_url()).await.unwrap();

        let (work_id, m_id) = setup_fixture(&ingestion).await;
        // tome_ingestion has no grants on `users`; insert via tome_app.
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

        // Idempotent: second lock() is a no-op.
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

        cleanup(&ingestion, work_id, m_id).await;
        sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(user_id)
            .execute(&app)
            .await
            .unwrap();
    }
}
