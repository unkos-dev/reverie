//! Current ingestion inputs and generation-bound attempt history.

use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Component, Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InputPath(Vec<u8>);

impl InputPath {
    pub fn from_path(path: &Path) -> std::io::Result<Self> {
        if path.as_os_str().is_empty()
            || path
                .components()
                .any(|part| !matches!(part, Component::Normal(_)))
            || path.as_os_str().as_bytes().contains(&0)
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "invalid ingestion-relative location",
            ));
        }
        Ok(Self(path.as_os_str().as_bytes().to_vec()))
    }

    pub fn from_bytes(bytes: Vec<u8>) -> std::io::Result<Self> {
        Self::from_path(&PathBuf::from(std::ffi::OsString::from_vec(bytes)))
    }

    pub fn path(&self) -> PathBuf {
        PathBuf::from(std::ffi::OsString::from_vec(self.0.clone()))
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Fingerprint {
    pub device: u64,
    pub inode: u64,
    pub size: u64,
    pub mtime_seconds: i64,
    pub mtime_nanoseconds: i64,
    pub ctime_seconds: i64,
    pub ctime_nanoseconds: i64,
}

impl Fingerprint {
    pub fn from_metadata(metadata: &std::fs::Metadata) -> Self {
        use std::os::unix::fs::MetadataExt;
        Self {
            device: metadata.dev(),
            inode: metadata.ino(),
            size: metadata.len(),
            mtime_seconds: metadata.mtime(),
            mtime_nanoseconds: metadata.mtime_nsec(),
            ctime_seconds: metadata.ctime(),
            ctime_nanoseconds: metadata.ctime_nsec(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, sqlx::Type)]
#[sqlx(type_name = "ingestion_input_status", rename_all = "snake_case")]
pub enum InputStatus {
    Pending,
    Processing,
    Imported,
    Duplicate,
    Rejected,
    NotAccepted,
    OperationalFailure,
    Removed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, sqlx::Type)]
#[sqlx(type_name = "ingestion_attempt_outcome", rename_all = "snake_case")]
pub enum AttemptOutcome {
    Imported,
    Duplicate,
    Rejected,
    Changed,
    SharedDependency,
    TransientInput,
    NeedsChange,
    Interrupted,
}

#[derive(Clone, Debug)]
pub struct Input {
    pub id: Uuid,
    pub source_path: Vec<u8>,
    pub fingerprint: sqlx::types::Json<Fingerprint>,
    pub generation: i64,
    pub status: InputStatus,
    pub reason: Option<String>,
    pub work_id: Option<Uuid>,
    pub retry_reset_at: DateTime<Utc>,
    pub observed_at: DateTime<Utc>,
}

pub async fn observe(
    pool: &PgPool,
    paths: &[InputPath],
    fingerprints: &[Fingerprint],
) -> sqlx::Result<Vec<Input>> {
    let paths: Vec<Vec<u8>> = paths.iter().map(|path| path.0.clone()).collect();
    let fingerprints: Vec<serde_json::Value> = fingerprints
        .iter()
        .map(serde_json::to_value)
        .collect::<Result<_, _>>()
        .map_err(|error| sqlx::Error::Encode(Box::new(error)))?;
    sqlx::query_as!(
        Input,
        r#"INSERT INTO ingestion_inputs (source_path, fingerprint)
           SELECT path, fingerprint FROM UNNEST($1::bytea[], $2::jsonb[]) AS observed(path, fingerprint)
           ON CONFLICT (source_path) WHERE status <> 'removed'
           DO UPDATE SET
             fingerprint = EXCLUDED.fingerprint,
             generation = ingestion_inputs.generation + (ingestion_inputs.fingerprint <> EXCLUDED.fingerprint)::int,
             status = CASE WHEN ingestion_inputs.fingerprint <> EXCLUDED.fingerprint
                      THEN 'pending'::ingestion_input_status ELSE ingestion_inputs.status END,
             reason = CASE WHEN ingestion_inputs.fingerprint <> EXCLUDED.fingerprint THEN NULL ELSE ingestion_inputs.reason END,
             work_id = CASE WHEN ingestion_inputs.fingerprint <> EXCLUDED.fingerprint THEN NULL ELSE ingestion_inputs.work_id END,
             completed_at = CASE WHEN ingestion_inputs.fingerprint <> EXCLUDED.fingerprint THEN NULL ELSE ingestion_inputs.completed_at END,
             retry_reset_at = CASE WHEN ingestion_inputs.fingerprint <> EXCLUDED.fingerprint THEN now() ELSE ingestion_inputs.retry_reset_at END,
             observed_at = CASE WHEN ingestion_inputs.fingerprint <> EXCLUDED.fingerprint THEN now() ELSE ingestion_inputs.observed_at END
           RETURNING id, source_path, fingerprint AS "fingerprint: sqlx::types::Json<Fingerprint>",
             generation, status AS "status: InputStatus", reason, work_id, retry_reset_at, observed_at"#,
        &paths,
        &fingerprints,
    )
    .fetch_all(pool)
    .await
}

pub async fn current_page(pool: &PgPool, after: Option<Uuid>) -> sqlx::Result<Vec<Input>> {
    sqlx::query_as!(
        Input,
        r#"SELECT id, source_path, fingerprint AS "fingerprint: sqlx::types::Json<Fingerprint>",
             generation, status AS "status: InputStatus", reason, work_id, retry_reset_at, observed_at
           FROM ingestion_inputs WHERE status <> 'removed' AND ($1::uuid IS NULL OR id > $1)
           ORDER BY id LIMIT 100"#,
        after,
    )
    .fetch_all(pool)
    .await
}

pub async fn current(pool: &PgPool, id: Uuid) -> sqlx::Result<Option<Input>> {
    sqlx::query_as!(Input,
        r#"SELECT id, source_path, fingerprint AS "fingerprint: sqlx::types::Json<Fingerprint>",
             generation, status AS "status: InputStatus", reason, work_id, retry_reset_at, observed_at
           FROM ingestion_inputs WHERE id = $1 AND status <> 'removed'"#, id,
    ).fetch_optional(pool).await
}

pub async fn reset_retries(pool: &PgPool) -> sqlx::Result<()> {
    sqlx::query!(
        "UPDATE ingestion_inputs SET status = 'pending', reason = NULL, completed_at = NULL, retry_reset_at = clock_timestamp()
         WHERE status = 'operational_failure' AND (
           EXISTS (SELECT 1 FROM ingestion_jobs j WHERE j.input_id = ingestion_inputs.id
             AND j.input_generation = ingestion_inputs.generation AND j.created_at >= ingestion_inputs.retry_reset_at
             AND j.outcome = 'needs_change')
           OR (SELECT COUNT(*) FROM ingestion_jobs j WHERE j.input_id = ingestion_inputs.id
             AND j.input_generation = ingestion_inputs.generation AND j.created_at >= ingestion_inputs.retry_reset_at
             AND j.outcome = 'transient_input') >= 6)",
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn reclaim(pool: &PgPool) -> sqlx::Result<()> {
    let mut tx = pool.begin().await?;
    sqlx::query!(
        "UPDATE ingestion_jobs SET status = 'failed', outcome = 'interrupted', completed_at = now()
         WHERE input_id IS NOT NULL AND status IN ('queued', 'running')",
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query!(
        "UPDATE ingestion_inputs SET status = 'pending', reason = NULL, retry_reset_at = clock_timestamp()
         WHERE status = 'processing'",
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    reset_retries(pool).await
}

pub async fn begin_attempt(pool: &PgPool, input: &Input, batch: Uuid) -> sqlx::Result<Uuid> {
    let mut tx = pool.begin().await?;
    let claimed = sqlx::query!(
        "UPDATE ingestion_inputs SET status = 'processing'
         WHERE id = $1 AND generation = $2 AND status IN ('pending', 'operational_failure') RETURNING id",
        input.id,
        input.generation,
    )
    .fetch_optional(&mut *tx)
    .await?;
    if claimed.is_none() {
        return Err(sqlx::Error::RowNotFound);
    }
    let id = sqlx::query_scalar!(
        "INSERT INTO ingestion_jobs (batch_id, source_path, input_id, input_generation, status, started_at)
         VALUES ($1, $2, $3, $4, 'running', now()) RETURNING id",
        batch,
        String::from_utf8(input.source_path.clone()).unwrap_or_else(|_| input.source_path.escape_ascii().to_string()),
        input.id,
        input.generation,
    )
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(id)
}

pub async fn finish(
    tx: &mut Transaction<'_, Postgres>,
    input: &Input,
    job: Uuid,
    outcome: AttemptOutcome,
    status: InputStatus,
    reason: Option<&str>,
    work: Option<Uuid>,
) -> sqlx::Result<()> {
    let history = sqlx::query!(
        "UPDATE ingestion_jobs SET outcome = $2, completed_at = now(), error_message = $3,
         status = CASE WHEN $2::ingestion_attempt_outcome = 'imported' THEN 'complete'::job_status
                       WHEN $2::ingestion_attempt_outcome IN ('duplicate', 'changed') THEN 'skipped'::job_status ELSE 'failed'::job_status END
         WHERE id = $1 AND input_id = $4 AND input_generation = $5",
        job,
        outcome as AttemptOutcome,
        reason,
        input.id,
        input.generation,
    )
    .execute(&mut **tx)
    .await?;
    if history.rows_affected() != 1 {
        return Err(sqlx::Error::RowNotFound);
    }
    sqlx::query!(
        "UPDATE ingestion_inputs SET status = $3, reason = $4, work_id = $5,
         completed_at = CASE WHEN $3::ingestion_input_status = 'pending' THEN NULL ELSE now() END
         WHERE id = $1 AND generation = $2 AND status <> 'removed'",
        input.id,
        input.generation,
        status as InputStatus,
        reason,
        work,
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub struct RetryState {
    pub id: Uuid,
    pub count: i64,
    pub failed_at: Option<DateTime<Utc>>,
    pub needs_change: bool,
}

pub async fn retry_states(pool: &PgPool, ids: &[Uuid]) -> sqlx::Result<Vec<RetryState>> {
    sqlx::query_as!(
        RetryState,
        r#"SELECT i.id, COUNT(j.id) FILTER (WHERE j.outcome = 'transient_input') AS "count!",
             MAX(j.completed_at) FILTER (WHERE j.outcome = 'transient_input') AS failed_at,
             COALESCE(BOOL_OR(j.outcome = 'needs_change'), false) AS "needs_change!"
           FROM ingestion_inputs i LEFT JOIN ingestion_jobs j ON j.input_id = i.id
             AND j.input_generation = i.generation AND j.created_at >= i.retry_reset_at
           WHERE i.id = ANY($1) GROUP BY i.id"#,
        ids,
    )
    .fetch_all(pool)
    .await
}

#[cfg(test)]
pub async fn transient_count(pool: &PgPool, input: &Input) -> sqlx::Result<i64> {
    sqlx::query_scalar!(
        "SELECT COUNT(*) AS \"count!\" FROM ingestion_jobs
         WHERE input_id = $1 AND input_generation = $2 AND created_at >= $3 AND outcome = 'transient_input'",
        input.id,
        input.generation,
        input.retry_reset_at,
    )
    .fetch_one(pool)
    .await
}

pub async fn set_unaccepted_many(
    pool: &PgPool,
    ids: &[Uuid],
    generations: &[i64],
) -> sqlx::Result<()> {
    sqlx::query!(
        "UPDATE ingestion_inputs SET status = 'not_accepted', reason = 'format not accepted'
         FROM UNNEST($1::uuid[], $2::bigint[]) AS selected(input_id, input_generation)
         WHERE id = selected.input_id AND generation = selected.input_generation AND status = 'pending'",
        ids,
        generations,
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn set_accepted_many(
    pool: &PgPool,
    ids: &[Uuid],
    generations: &[i64],
) -> sqlx::Result<()> {
    sqlx::query!(
        "UPDATE ingestion_inputs SET status = 'pending', reason = NULL
         FROM UNNEST($1::uuid[], $2::bigint[]) AS selected(input_id, input_generation)
         WHERE id = selected.input_id AND generation = selected.input_generation AND status = 'not_accepted'",
        ids, generations,
    ).execute(pool).await?;
    Ok(())
}

pub async fn remove(pool: &PgPool, input: &Input, cause: &str) -> sqlx::Result<()> {
    remove_many(pool, &[input.id], &[input.generation], cause).await
}

pub async fn remove_many(
    pool: &PgPool,
    ids: &[Uuid],
    generations: &[i64],
    cause: &str,
) -> sqlx::Result<()> {
    sqlx::query!(
        "UPDATE ingestion_inputs SET status = 'removed', removed_at = now(), removal_cause = $3
         FROM UNNEST($1::uuid[], $2::bigint[]) AS selected(input_id, input_generation)
         WHERE id = selected.input_id AND generation = selected.input_generation AND status <> 'removed'",
        ids,
        generations,
        cause,
    )
    .execute(pool)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_ingestion_inputs_epub_only_and_imported_cleanup_defaults() {
        let config = crate::config::Config::default();
        assert_eq!(
            config.accepted_formats,
            vec![crate::models::manifestation_format::ManifestationFormat::Epub]
        );
        assert!(config.cleanup_imported);
        assert!(!config.cleanup_duplicates);
        for (formats, valid) in [
            (serde_json::json!([]), true),
            (serde_json::json!(["epub"]), true),
            (serde_json::json!(["pdf"]), false),
            (serde_json::json!(["epub", "epub"]), false),
        ] {
            let update: crate::models::settings::UpdateSettings =
                serde_json::from_value(serde_json::json!({"accepted_formats": formats})).unwrap();
            assert_eq!(
                crate::models::settings::validate_update(&update).is_ok(),
                valid
            );
        }
    }

    fn fingerprint(size: u64) -> Fingerprint {
        Fingerprint {
            device: 1,
            inode: 2,
            size,
            mtime_seconds: 3,
            mtime_nanoseconds: 4,
            ctime_seconds: 5,
            ctime_nanoseconds: 6,
        }
    }

    async fn observed(pool: &PgPool, size: u64) -> Input {
        observe(
            pool,
            &[InputPath::from_path(Path::new("book.epub")).unwrap()],
            &[fingerprint(size)],
        )
        .await
        .unwrap()
        .remove(0)
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn capability_ingestion_inputs_current_uniqueness_and_removed_replacement(pool: PgPool) {
        let original = observed(&pool, 10).await;
        let same = observed(&pool, 10).await;
        assert_eq!(same.id, original.id);
        assert_eq!(same.generation, 1);
        let changed = observed(&pool, 11).await;
        assert_eq!(changed.id, original.id);
        assert_eq!(changed.generation, 2);
        remove(&pool, &changed, "external_disappearance")
            .await
            .unwrap();
        let replacement = observed(&pool, 11).await;
        assert_ne!(replacement.id, original.id);
        assert_eq!(replacement.generation, 1);
        assert_eq!(current_page(&pool, None).await.unwrap().len(), 1);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn capability_ingestion_inputs_stale_completion_preserves_new_generation(pool: PgPool) {
        let input = observed(&pool, 10).await;
        let job = begin_attempt(&pool, &input, Uuid::new_v4()).await.unwrap();
        let changed = observed(&pool, 11).await;
        let mut tx = pool.begin().await.unwrap();
        finish(
            &mut tx,
            &input,
            job,
            AttemptOutcome::Rejected,
            InputStatus::Rejected,
            Some("invalid EPUB"),
            None,
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        let current = current_page(&pool, None).await.unwrap().remove(0);
        assert_eq!(current.generation, changed.generation);
        assert_eq!(current.status, InputStatus::Pending);
        let history = sqlx::query!("SELECT input_id, input_generation, outcome AS \"outcome: AttemptOutcome\" FROM ingestion_jobs WHERE id = $1", job).fetch_one(&pool).await.unwrap();
        assert_eq!(history.input_id, Some(input.id));
        assert_eq!(history.input_generation, Some(1));
        assert_eq!(history.outcome, Some(AttemptOutcome::Rejected));
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn capability_ingestion_inputs_terminal_atomicity(pool: PgPool) {
        let input = observed(&pool, 10).await;
        let job = begin_attempt(&pool, &input, Uuid::new_v4()).await.unwrap();
        let mut tx = pool.begin().await.unwrap();
        finish(
            &mut tx,
            &input,
            job,
            AttemptOutcome::Duplicate,
            InputStatus::Duplicate,
            None,
            None,
        )
        .await
        .unwrap();
        tx.rollback().await.unwrap();
        assert_eq!(
            current_page(&pool, None).await.unwrap()[0].status,
            InputStatus::Processing
        );
        let outcome = sqlx::query_scalar!(
            "SELECT outcome AS \"outcome: AttemptOutcome\" FROM ingestion_jobs WHERE id = $1",
            job
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(outcome, None);
        let mut tx = pool.begin().await.unwrap();
        assert!(
            finish(
                &mut tx,
                &input,
                Uuid::new_v4(),
                AttemptOutcome::Rejected,
                InputStatus::Rejected,
                None,
                None
            )
            .await
            .is_err()
        );
        tx.rollback().await.unwrap();
        assert_eq!(
            current_page(&pool, None).await.unwrap()[0].status,
            InputStatus::Processing
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn capability_ingestion_inputs_six_transient_failures_before_exhaustion_reset(
        pool: PgPool,
    ) {
        let input = observed(&pool, 10).await;
        for outcome in [
            AttemptOutcome::SharedDependency,
            AttemptOutcome::Interrupted,
        ] {
            let job = begin_attempt(&pool, &input, Uuid::new_v4()).await.unwrap();
            let mut tx = pool.begin().await.unwrap();
            finish(
                &mut tx,
                &input,
                job,
                outcome,
                InputStatus::Pending,
                None,
                None,
            )
            .await
            .unwrap();
            tx.commit().await.unwrap();
        }
        assert_eq!(transient_count(&pool, &input).await.unwrap(), 0);
        for count in 1..=6 {
            let job = begin_attempt(&pool, &input, Uuid::new_v4()).await.unwrap();
            let mut tx = pool.begin().await.unwrap();
            finish(
                &mut tx,
                &input,
                job,
                AttemptOutcome::TransientInput,
                InputStatus::OperationalFailure,
                None,
                None,
            )
            .await
            .unwrap();
            tx.commit().await.unwrap();
            assert_eq!(transient_count(&pool, &input).await.unwrap(), count);
            reset_retries(&pool).await.unwrap();
            let current = current_page(&pool, None).await.unwrap().remove(0);
            if count < 6 {
                assert_eq!(current.status, InputStatus::OperationalFailure);
                sqlx::query!(
                    "UPDATE ingestion_inputs SET status = 'pending' WHERE id = $1",
                    input.id
                )
                .execute(&pool)
                .await
                .unwrap();
            } else {
                assert_eq!(current.status, InputStatus::Pending);
                assert_eq!(transient_count(&pool, &current).await.unwrap(), 0);
            }
        }
    }

    #[test]
    fn capability_ingestion_inputs_raw_relative_paths() {
        let bytes = vec![0xff, b'.', b'e', b'p', b'u', b'b'];
        let path = InputPath::from_bytes(bytes.clone()).unwrap();
        assert_eq!(path.path().as_os_str().as_bytes(), bytes);
        for invalid in ["", "/book.epub", "../book.epub", "book/../other.epub"] {
            assert!(InputPath::from_path(Path::new(invalid)).is_err());
        }
    }
}
