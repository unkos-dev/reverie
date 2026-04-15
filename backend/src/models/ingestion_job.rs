use serde::Serialize;
use sqlx::PgPool;
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct IngestionJob {
    pub id: Uuid,
    pub batch_id: Uuid,
    pub source_path: String,
    pub status: String,
    pub error_message: Option<String>,
    pub started_at: Option<OffsetDateTime>,
    pub completed_at: Option<OffsetDateTime>,
    pub created_at: OffsetDateTime,
}

pub async fn create(
    pool: &PgPool,
    batch_id: Uuid,
    source_path: &str,
) -> Result<IngestionJob, sqlx::Error> {
    sqlx::query_as::<_, IngestionJob>(
        "INSERT INTO ingestion_jobs (batch_id, source_path) \
         VALUES ($1, $2) \
         RETURNING id, batch_id, source_path, status::text, error_message, \
                   started_at, completed_at, created_at",
    )
    .bind(batch_id)
    .bind(source_path)
    .fetch_one(pool)
    .await
}

pub async fn mark_running(pool: &PgPool, id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE ingestion_jobs SET status = 'running', started_at = now() \
         WHERE id = $1",
    )
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn mark_complete(pool: &PgPool, id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE ingestion_jobs SET status = 'complete', completed_at = now() \
         WHERE id = $1",
    )
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn mark_skipped(pool: &PgPool, id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE ingestion_jobs SET status = 'skipped', completed_at = now() \
         WHERE id = $1",
    )
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn mark_failed(pool: &PgPool, id: Uuid, error_message: &str) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE ingestion_jobs SET status = 'failed', error_message = $2, \
         completed_at = now() WHERE id = $1",
    )
    .bind(id)
    .bind(error_message)
    .execute(pool)
    .await?;
    Ok(())
}

#[allow(dead_code)] // Used by ingestion status API in Step 10
pub async fn find_by_batch(
    pool: &PgPool,
    batch_id: Uuid,
) -> Result<Vec<IngestionJob>, sqlx::Error> {
    sqlx::query_as::<_, IngestionJob>(
        "SELECT id, batch_id, source_path, status::text, error_message, \
                started_at, completed_at, created_at \
         FROM ingestion_jobs WHERE batch_id = $1 \
         ORDER BY created_at",
    )
    .bind(batch_id)
    .fetch_all(pool)
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore] // Requires running postgres
    async fn job_lifecycle() {
        let url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgres://tome_ingestion:tome_ingestion@localhost:5433/tome_dev".into()
        });
        let pool = sqlx::PgPool::connect(&url).await.expect("connect");

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

        // Cleanup
        sqlx::query("DELETE FROM ingestion_jobs WHERE batch_id = $1")
            .bind(batch_id)
            .execute(&pool)
            .await
            .expect("cleanup");
    }

    #[tokio::test]
    #[ignore] // Requires running postgres
    async fn job_skipped_and_failed() {
        let url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgres://tome_ingestion:tome_ingestion@localhost:5433/tome_dev".into()
        });
        let pool = sqlx::PgPool::connect(&url).await.expect("connect");

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

        // Cleanup
        sqlx::query("DELETE FROM ingestion_jobs WHERE batch_id = $1")
            .bind(batch_id)
            .execute(&pool)
            .await
            .expect("cleanup");
    }
}
