//! Per-file ingestion jobs produced by the import pipeline.
//!
//! Each job is one file's path through the
//! `queued → running → (complete | skipped | failed)` lifecycle. Jobs
//! are grouped by `batch_id` so the operator-facing status surface
//! ([`crate::models::ingestion_job::find_by_batch`]) can show progress
//! for a whole import run.
//!
//! `status` is currently a free-form `TEXT` column; tightening it to a
//! Postgres `ENUM` is tracked separately and would mirror the pattern
//! used by [`crate::models::ingestion_status::IngestionStatus`].

use serde::Serialize;
use sqlx::PgPool;
use time::OffsetDateTime;
use uuid::Uuid;

/// A single file's ingestion lifecycle row.
#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct IngestionJob {
    /// Primary key.
    pub id: Uuid,
    /// Identifier shared by every job in the same import batch.
    pub batch_id: Uuid,
    /// Filesystem path of the source file at ingestion time.
    pub source_path: String,
    /// Lifecycle state; one of `queued | running | complete | skipped | failed`.
    pub status: String,
    /// Human-readable failure cause set by [`mark_failed`]; `None` otherwise.
    pub error_message: Option<String>,
    /// `now()` of the `queued → running` transition; `None` while still queued.
    pub started_at: Option<OffsetDateTime>,
    /// `now()` of the terminal-state transition; `None` while not yet finished.
    pub completed_at: Option<OffsetDateTime>,
    /// Row insert timestamp.
    pub created_at: OffsetDateTime,
}

/// Insert a fresh `queued` job for `source_path` under `batch_id`.
///
/// # Errors
///
/// Returns [`sqlx::Error`] from the underlying `INSERT`.
pub async fn create(
    pool: &PgPool,
    batch_id: Uuid,
    source_path: &str,
) -> Result<IngestionJob, sqlx::Error> {
    sqlx::query_as!(
        IngestionJob,
        "INSERT INTO ingestion_jobs (batch_id, source_path) \
         VALUES ($1, $2) \
         RETURNING id, batch_id, source_path, status::text AS \"status!\", error_message, \
                   started_at, completed_at, created_at",
        batch_id,
        source_path,
    )
    .fetch_one(pool)
    .await
}

/// Transition a job to `running` and stamp `started_at = now()`.
///
/// # Errors
///
/// Returns [`sqlx::Error`] from the underlying `UPDATE`.
pub async fn mark_running(pool: &PgPool, id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE ingestion_jobs SET status = 'running', started_at = now() \
         WHERE id = $1",
        id,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Mark a job `complete` and stamp `completed_at = now()`.
///
/// # Errors
///
/// Returns [`sqlx::Error`] from the underlying `UPDATE`.
pub async fn mark_complete(pool: &PgPool, id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE ingestion_jobs SET status = 'complete', completed_at = now() \
         WHERE id = $1",
        id,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Mark a job `skipped` (e.g. duplicate-hash, unsupported format) and
/// stamp `completed_at = now()`. Skipped is a terminal non-failure state.
///
/// # Errors
///
/// Returns [`sqlx::Error`] from the underlying `UPDATE`.
pub async fn mark_skipped(pool: &PgPool, id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE ingestion_jobs SET status = 'skipped', completed_at = now() \
         WHERE id = $1",
        id,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Mark a job `failed` with `error_message` and stamp `completed_at = now()`.
///
/// # Errors
///
/// Returns [`sqlx::Error`] from the underlying `UPDATE`.
pub async fn mark_failed(pool: &PgPool, id: Uuid, error_message: &str) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE ingestion_jobs SET status = 'failed', error_message = $2, \
         completed_at = now() WHERE id = $1",
        id,
        error_message,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// All jobs in a given batch, ordered by `created_at` so the surface
/// renders deterministically.
///
/// # Errors
///
/// Returns [`sqlx::Error`] from the underlying `SELECT`.
pub async fn find_by_batch(
    pool: &PgPool,
    batch_id: Uuid,
) -> Result<Vec<IngestionJob>, sqlx::Error> {
    sqlx::query_as!(
        IngestionJob,
        "SELECT id, batch_id, source_path, status::text AS \"status!\", error_message, \
                started_at, completed_at, created_at \
         FROM ingestion_jobs WHERE batch_id = $1 \
         ORDER BY created_at",
        batch_id,
    )
    .fetch_all(pool)
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::db::ingestion_pool_for;

    #[sqlx::test(migrations = "./migrations")]
    async fn job_lifecycle(pool: PgPool) {
        let pool = ingestion_pool_for(&pool).await;
        let batch_id = Uuid::new_v4();
        let job = create(&pool, batch_id, "/tmp/test.epub")
            .await
            .expect("create job");
        assert_eq!(job.status, "queued");
        assert!(job.started_at.is_none());

        mark_running(&pool, job.id).await.expect("mark running");
        let jobs = find_by_batch(&pool, batch_id).await.expect("find");
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].status, "running");
        assert!(jobs[0].started_at.is_some());

        mark_complete(&pool, job.id).await.expect("mark complete");
        let jobs = find_by_batch(&pool, batch_id).await.expect("find");
        assert_eq!(jobs[0].status, "complete");
        assert!(jobs[0].completed_at.is_some());
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn job_skipped_and_failed(pool: PgPool) {
        let pool = ingestion_pool_for(&pool).await;
        let batch_id = Uuid::new_v4();

        let job1 = create(&pool, batch_id, "/tmp/dup.epub")
            .await
            .expect("create");
        mark_skipped(&pool, job1.id).await.expect("mark skipped");
        let jobs = find_by_batch(&pool, batch_id).await.expect("find");
        assert_eq!(jobs[0].status, "skipped");

        let job2 = create(&pool, batch_id, "/tmp/bad.epub")
            .await
            .expect("create");
        mark_failed(&pool, job2.id, "hash mismatch")
            .await
            .expect("mark failed");
        let jobs = find_by_batch(&pool, batch_id).await.expect("find");
        let failed = jobs.iter().find(|j| j.id == job2.id).unwrap();
        assert_eq!(failed.status, "failed");
        assert_eq!(failed.error_message.as_deref(), Some("hash mismatch"));
    }
}
