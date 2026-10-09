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

#[derive(Clone, Copy, Debug, Eq, PartialEq, sqlx::Type, serde::Serialize, utoipa::ToSchema)]
#[sqlx(type_name = "ingestion_input_status", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
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

#[derive(Clone, Copy, Debug, Eq, PartialEq, sqlx::Type, serde::Serialize, utoipa::ToSchema)]
#[sqlx(type_name = "ingestion_attempt_outcome", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
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

/// Why the validator rejected an EPUB, declared most severe first so the
/// head of a sorted list is the primary reason.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum RejectionReason {
    UnsafeContents,
    Damaged,
    InvalidStructure,
    OverLimits,
    Unspecified,
}

impl RejectionReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::UnsafeContents => "unsafe_contents",
            Self::Damaged => "damaged",
            Self::InvalidStructure => "invalid_structure",
            Self::OverLimits => "over_limits",
            Self::Unspecified => "unspecified",
        }
    }

    const fn from_issue_kind(kind: &crate::services::epub::IssueKind) -> Self {
        use crate::services::epub::IssueKind;
        match kind {
            IssueKind::PathTraversal { .. }
            | IssueKind::ZipBomb { .. }
            | IssueKind::UnsafeOpfPath { .. }
            | IssueKind::UnsafeManifestHref { .. } => Self::UnsafeContents,
            IssueKind::CorruptEntry { .. }
            | IssueKind::PreludeBeforeArchive { .. }
            | IssueKind::DuplicateEntry { .. }
            | IssueKind::UnsupportedCompression { .. }
            | IssueKind::EncryptedEntry { .. } => Self::Damaged,
            IssueKind::InvalidMimetype { .. }
            | IssueKind::MissingContainer { .. }
            | IssueKind::BrokenSpineRef { .. }
            | IssueKind::EncodingMismatch { .. }
            | IssueKind::AmbiguousEncoding { .. }
            | IssueKind::MalformedXhtml { .. }
            | IssueKind::MissingCover { .. }
            | IssueKind::UndecodableCover { .. } => Self::InvalidStructure,
            IssueKind::EntryCapExceeded { .. }
            | IssueKind::ArchiveTooLarge { .. }
            | IssueKind::SpineCapExceeded { .. } => Self::OverLimits,
        }
    }

    /// Distinct classes of the issues that made the report irrecoverable,
    /// most severe first; `[Unspecified]` when none did.
    pub fn from_issues(issues: &[crate::services::epub::Issue]) -> Vec<Self> {
        let mut reasons: Vec<Self> = issues
            .iter()
            .filter(|issue| issue.severity == crate::services::epub::Severity::Irrecoverable)
            .map(|issue| Self::from_issue_kind(&issue.kind))
            .collect();
        reasons.sort_unstable();
        reasons.dedup();
        if reasons.is_empty() {
            reasons.push(Self::Unspecified);
        }
        reasons
    }
}

/// Renders an ingestion-relative path for display: valid UTF-8 spans verbatim
/// and each invalid byte escaped as `\xNN`, so a stored name always renders.
pub fn display_path(bytes: &[u8]) -> String {
    bytes
        .utf8_chunks()
        .map(|chunk| format!("{}{}", chunk.valid(), chunk.invalid().escape_ascii()))
        .collect()
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

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicationIdentity {
    pub device: u64,
    pub inode: u64,
}

pub struct Publication {
    pub job: Uuid,
    pub input_id: Uuid,
    pub input_generation: i64,
    pub library_id: Uuid,
    pub path: String,
    pub identity: sqlx::types::Json<PublicationIdentity>,
    pub hash: String,
    pub size: i64,
    pub failure_class: Option<AttemptOutcome>,
    pub failure_reason: Option<String>,
    pub imported: bool,
}

pub async fn record_publication(
    tx: &mut Transaction<'_, Postgres>,
    input: &Input,
    job: Uuid,
    location: &crate::services::files::LibraryLocation,
    identity: &PublicationIdentity,
    hash: &str,
    size: u64,
) -> sqlx::Result<()> {
    let size = i64::try_from(size).map_err(|error| sqlx::Error::Encode(Box::new(error)))?;
    let updated = sqlx::query!(
        "UPDATE ingestion_jobs SET publication_library_id = $4, publication_path = $5,
         publication_identity = $6, publication_hash = $7, publication_size = $8
         WHERE id = $1 AND input_id = $2 AND input_generation = $3 AND outcome IS NULL
           AND publication_library_id IS NULL",
        job,
        input.id,
        input.generation,
        location.library_id.as_uuid(),
        location.path.as_str(),
        sqlx::types::Json(identity) as _,
        hash,
        size,
    )
    .execute(&mut **tx)
    .await?;
    if updated.rows_affected() != 1 {
        return Err(sqlx::Error::RowNotFound);
    }
    Ok(())
}

#[cfg(test)]
pub async fn publication_page(
    pool: &PgPool,
    after: Option<Uuid>,
) -> sqlx::Result<Vec<Publication>> {
    publication_page_selected(pool, after, None).await
}

pub async fn publication_page_selected(
    pool: &PgPool,
    after: Option<Uuid>,
    selected: Option<&[InputPath]>,
) -> sqlx::Result<Vec<Publication>> {
    let paths = selected.map(|paths| paths.iter().map(|path| path.0.clone()).collect::<Vec<_>>());
    sqlx::query_as!(Publication,
        r#"SELECT id AS job, input_id AS "input_id!", input_generation AS "input_generation!",
             publication_library_id AS "library_id!", publication_path AS "path!",
             publication_identity AS "identity!: sqlx::types::Json<PublicationIdentity>",
             publication_hash AS "hash!", publication_size AS "size!",
             publication_failure_class AS "failure_class: AttemptOutcome", publication_failure_reason AS failure_reason,
             COALESCE(outcome = 'imported', false) AS "imported!"
           FROM ingestion_jobs WHERE publication_library_id IS NOT NULL AND ($1::uuid IS NULL OR id > $1)
             AND ($2::bytea[] IS NULL OR EXISTS (
               SELECT 1 FROM ingestion_inputs i, UNNEST($2::bytea[]) AS selected(path)
               WHERE i.id = ingestion_jobs.input_id AND (i.source_path = selected.path
                 OR substring(i.source_path FROM 1 FOR octet_length(selected.path) + 1) = selected.path || '\x2f'::bytea)))
           ORDER BY id LIMIT 100"#, after, paths.as_deref(),
    ).fetch_all(pool).await
}

pub async fn clear_publication(tx: &mut Transaction<'_, Postgres>, job: Uuid) -> sqlx::Result<()> {
    sqlx::query!(
        "UPDATE ingestion_jobs SET publication_library_id = NULL, publication_path = NULL,
         publication_identity = NULL, publication_hash = NULL, publication_size = NULL,
         publication_failure_class = NULL, publication_failure_reason = NULL WHERE id = $1",
        job,
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn foreign_publication(
    tx: &mut Transaction<'_, Postgres>,
    publication: &Publication,
) -> sqlx::Result<()> {
    clear_publication(tx, publication.job).await?;
    sqlx::query!(
        "UPDATE ingestion_jobs SET outcome = 'needs_change', status = 'failed', completed_at = now(),
         error_message = 'publication name has another owner or changed content' WHERE id = $1 AND outcome IS NULL",
        publication.job,
    ).execute(&mut **tx).await?;
    sqlx::query!(
        "UPDATE ingestion_inputs SET status = 'operational_failure', completed_at = now(),
         reason = 'publication name has another owner or changed content'
         WHERE id = $1 AND generation = $2 AND status <> 'removed'",
        publication.input_id,
        publication.input_generation,
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn defer_publication(
    pool: &PgPool,
    job: Uuid,
    class: AttemptOutcome,
    reason: &str,
) -> sqlx::Result<()> {
    sqlx::query!(
        "UPDATE ingestion_jobs SET publication_failure_class = $2, publication_failure_reason = $3
         WHERE id = $1 AND publication_library_id IS NOT NULL",
        job,
        class as AttemptOutcome,
        reason,
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn imported_attempt(pool: &PgPool, job: Uuid) -> sqlx::Result<bool> {
    sqlx::query_scalar!(
        "SELECT COALESCE(outcome = 'imported', false) AS \"imported!\" FROM ingestion_jobs WHERE id = $1", job,
    ).fetch_one(pool).await
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
             generation = ingestion_inputs.generation + 1,
             status = 'pending', reason = NULL, work_id = NULL, completed_at = NULL,
             retry_reset_at = now(), observed_at = now()
           WHERE ingestion_inputs.fingerprint <> EXCLUDED.fingerprint
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

pub async fn selected_page(
    pool: &PgPool,
    paths: &[InputPath],
    after: Option<Uuid>,
) -> sqlx::Result<Vec<Input>> {
    let paths = paths.iter().map(|path| path.0.clone()).collect::<Vec<_>>();
    sqlx::query_as!(Input,
        r#"SELECT id, source_path, fingerprint AS "fingerprint: sqlx::types::Json<Fingerprint>",
             generation, status AS "status: InputStatus", reason, work_id, retry_reset_at, observed_at
           FROM ingestion_inputs WHERE status <> 'removed' AND ($2::uuid IS NULL OR id > $2)
             AND EXISTS (SELECT 1 FROM UNNEST($1::bytea[]) AS selected(path)
               WHERE source_path = path OR substring(source_path FROM 1 FOR octet_length(path) + 1) = path || '/'::bytea)
           ORDER BY id LIMIT 100"#, &paths, after,
    ).fetch_all(pool).await
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
         WHERE input_id IS NOT NULL AND status IN ('queued', 'running') AND publication_library_id IS NULL",
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query!(
        "UPDATE ingestion_inputs SET status = 'pending', reason = NULL, retry_reset_at = clock_timestamp()
         WHERE status = 'processing' AND NOT EXISTS (SELECT 1 FROM ingestion_jobs j
           WHERE j.input_id = ingestion_inputs.id AND j.publication_library_id IS NOT NULL)",
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
         WHERE id = $1 AND generation = $2 AND status IN ('pending', 'operational_failure')
           AND NOT EXISTS (SELECT 1 FROM ingestion_jobs j WHERE j.input_id = ingestion_inputs.id
             AND j.publication_library_id IS NOT NULL) RETURNING id",
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
        display_path(&input.source_path),
        input.id,
        input.generation,
    )
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(id)
}

#[expect(
    clippy::too_many_arguments,
    reason = "one terminal write threads the attempt, outcome, status, reason and classes"
)]
pub async fn finish(
    tx: &mut Transaction<'_, Postgres>,
    input: &Input,
    job: Uuid,
    outcome: AttemptOutcome,
    status: InputStatus,
    reason: Option<&str>,
    rejection_reasons: &[RejectionReason],
    work: Option<Uuid>,
) -> sqlx::Result<()> {
    let history = sqlx::query!(
        "UPDATE ingestion_jobs SET outcome = $2, completed_at = now(), error_message = $3,
         publication_library_id = NULL, publication_path = NULL, publication_identity = NULL,
         publication_hash = NULL, publication_size = NULL, publication_failure_class = NULL,
         publication_failure_reason = NULL,
         status = CASE WHEN $2::ingestion_attempt_outcome = 'imported' THEN 'complete'::job_status
                       WHEN $2::ingestion_attempt_outcome IN ('duplicate', 'changed') THEN 'skipped'::job_status ELSE 'failed'::job_status END
         WHERE id = $1 AND input_id = $4 AND input_generation = $5
           AND (publication_library_id IS NULL OR $2::ingestion_attempt_outcome = 'imported')",
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
    let rejection_reasons: Vec<String> = rejection_reasons
        .iter()
        .map(|reason| reason.as_str().to_owned())
        .collect();
    sqlx::query!(
        "UPDATE ingestion_inputs SET status = $3, reason = $4, work_id = $5, rejection_reasons = $6,
         completed_at = CASE WHEN $3::ingestion_input_status = 'pending' THEN NULL ELSE now() END
         WHERE id = $1 AND generation = $2 AND status <> 'removed'",
        input.id,
        input.generation,
        status as InputStatus,
        reason,
        work,
        &rejection_reasons,
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

    fn issue(
        severity: crate::services::epub::Severity,
        kind: crate::services::epub::IssueKind,
    ) -> crate::services::epub::Issue {
        crate::services::epub::Issue {
            layer: crate::services::epub::Layer::Zip,
            severity,
            kind,
        }
    }

    #[test]
    fn rejection_reasons_are_distinct_and_most_severe_first() {
        use crate::services::epub::{IssueKind, Severity};
        let issues = [
            issue(
                Severity::Irrecoverable,
                IssueKind::ArchiveTooLarge { size: 2, limit: 1 },
            ),
            issue(
                Severity::Irrecoverable,
                IssueKind::CorruptEntry {
                    entry_name: "a".into(),
                },
            ),
            issue(
                Severity::Irrecoverable,
                IssueKind::DuplicateEntry {
                    entry_name: "b".into(),
                },
            ),
            issue(
                Severity::Irrecoverable,
                IssueKind::PathTraversal {
                    entry_name: "../x".into(),
                },
            ),
        ];
        assert_eq!(
            RejectionReason::from_issues(&issues),
            vec![
                RejectionReason::UnsafeContents,
                RejectionReason::Damaged,
                RejectionReason::OverLimits,
            ]
        );
    }

    #[test]
    fn rejection_reasons_ignore_issues_that_did_not_cause_the_rejection() {
        use crate::services::epub::{IssueKind, Severity};
        let issues = [
            issue(
                Severity::Repaired,
                IssueKind::PathTraversal {
                    entry_name: "../x".into(),
                },
            ),
            issue(
                Severity::Irrecoverable,
                IssueKind::EntryCapExceeded { count: 9, limit: 1 },
            ),
        ];
        assert_eq!(
            RejectionReason::from_issues(&issues),
            vec![RejectionReason::OverLimits]
        );
    }

    #[test]
    fn rejection_reasons_fall_back_to_unspecified_without_an_irrecoverable_issue() {
        assert_eq!(
            RejectionReason::from_issues(&[]),
            vec![RejectionReason::Unspecified]
        );
    }

    #[test]
    fn display_path_escapes_only_non_utf8_bytes() {
        assert_eq!(
            display_path("dir/caf\u{e9}.epub".as_bytes()),
            "dir/caf\u{e9}.epub"
        );
        assert_eq!(display_path(b"dir/\xffname.epub"), "dir/\\xffname.epub");
    }

    #[test]
    fn display_path_keeps_valid_utf8_spans_around_invalid_bytes() {
        assert_eq!(
            display_path(b"na\xc3\xafve/\xff.epub"),
            "na\u{ef}ve/\\xff.epub"
        );
        assert_eq!(
            display_path(b"\xfe\xffa\xc3\xa9\x80"),
            "\\xfe\\xffa\u{e9}\\x80"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn capability_ingestion_publication_acknowledged_evidence_blocks_reclaim_and_acquisition(
        pool: PgPool,
    ) {
        let input = observed(&pool, 10).await;
        let job = begin_attempt(&pool, &input, Uuid::new_v4()).await.unwrap();
        let location = crate::services::files::LibraryLocation {
            library_id: crate::models::storage_library::default_library_id(&pool)
                .await
                .unwrap(),
            path: "book.epub".parse().unwrap(),
        };
        let identity = PublicationIdentity {
            device: 42,
            inode: u64::MAX,
        };
        let mut tx = pool.begin().await.unwrap();
        record_publication(
            &mut tx,
            &input,
            job,
            &location,
            &identity,
            &"a".repeat(64),
            10,
        )
        .await
        .unwrap();
        assert!(publication_page(&pool, None).await.unwrap().is_empty());
        tx.commit().await.unwrap();
        let evidence = publication_page(&pool, None).await.unwrap().remove(0);
        assert_eq!(evidence.job, job);
        assert_eq!(evidence.identity.0, identity);
        assert_eq!(evidence.hash, "a".repeat(64));
        assert_eq!(evidence.size, 10);
        assert!(!evidence.imported);
        reclaim(&pool).await.unwrap();
        assert_eq!(
            current(&pool, input.id).await.unwrap().unwrap().status,
            InputStatus::Processing
        );
        assert!(begin_attempt(&pool, &input, Uuid::new_v4()).await.is_err());
        let mut tx = pool.begin().await.unwrap();
        assert!(
            finish(
                &mut tx,
                &input,
                job,
                AttemptOutcome::NeedsChange,
                InputStatus::OperationalFailure,
                Some("unresolved"),
                &[],
                None
            )
            .await
            .is_err()
        );
        tx.rollback().await.unwrap();
        let mut tx = pool.begin().await.unwrap();
        clear_publication(&mut tx, job).await.unwrap();
        tx.commit().await.unwrap();
        reclaim(&pool).await.unwrap();
        assert_eq!(
            current(&pool, input.id).await.unwrap().unwrap().status,
            InputStatus::Pending
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn capability_ingestion_publication_pages_and_stale_disposition(pool: PgPool) {
        let library_id = crate::models::storage_library::default_library_id(&pool)
            .await
            .unwrap();
        let paths = (0..101)
            .map(|index| InputPath::from_path(Path::new(&format!("{index}.epub"))).unwrap())
            .collect::<Vec<_>>();
        let inputs = observe(&pool, &paths, &vec![fingerprint(10); 101])
            .await
            .unwrap();
        for input in &inputs {
            let job = begin_attempt(&pool, input, Uuid::new_v4()).await.unwrap();
            let location = crate::services::files::LibraryLocation {
                library_id,
                path: format!("{}.epub", input.id).parse().unwrap(),
            };
            let mut tx = pool.begin().await.unwrap();
            record_publication(
                &mut tx,
                input,
                job,
                &location,
                &PublicationIdentity {
                    device: 1,
                    inode: 2,
                },
                &"a".repeat(64),
                10,
            )
            .await
            .unwrap();
            tx.commit().await.unwrap();
        }
        let first = publication_page(&pool, None).await.unwrap();
        assert_eq!(first.len(), 100);
        let second = publication_page(&pool, Some(first.last().unwrap().job))
            .await
            .unwrap();
        assert_eq!(second.len(), 1);
        assert!(
            publication_page(&pool, Some(second[0].job))
                .await
                .unwrap()
                .is_empty()
        );
        let old = &first[0];
        let input = current(&pool, old.input_id).await.unwrap().unwrap();
        let path = InputPath::from_bytes(input.source_path).unwrap();
        let changed = observe(&pool, &[path], &[fingerprint(11)])
            .await
            .unwrap()
            .remove(0);
        let mut tx = pool.begin().await.unwrap();
        foreign_publication(&mut tx, old).await.unwrap();
        tx.commit().await.unwrap();
        let current = current(&pool, input.id).await.unwrap().unwrap();
        assert_eq!(current.generation, changed.generation);
        assert_eq!(current.status, InputStatus::Pending);
        assert!(current.reason.is_none());
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn capability_ingestion_publication_rejects_invalid_hash_and_oversized_length(
        pool: PgPool,
    ) {
        let input = observed(&pool, 10).await;
        let job = begin_attempt(&pool, &input, Uuid::new_v4()).await.unwrap();
        let location = crate::services::files::LibraryLocation {
            library_id: crate::models::storage_library::default_library_id(&pool)
                .await
                .unwrap(),
            path: "book.epub".parse().unwrap(),
        };
        let identity = PublicationIdentity {
            device: 1,
            inode: 2,
        };
        for (hash, size) in [("bad".into(), 10), ("a".repeat(64), u64::MAX)] {
            let mut tx = pool.begin().await.unwrap();
            assert!(
                record_publication(&mut tx, &input, job, &location, &identity, &hash, size)
                    .await
                    .is_err()
            );
            tx.rollback().await.unwrap();
        }
        assert!(publication_page(&pool, None).await.unwrap().is_empty());
    }

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
        let paths = [InputPath::from_path(Path::new("book.epub")).unwrap()];
        observe(pool, &paths, &[fingerprint(size)]).await.unwrap();
        selected_page(pool, &paths, None).await.unwrap().remove(0)
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn capability_ingestion_inputs_current_uniqueness_and_removed_replacement(pool: PgPool) {
        let original = observed(&pool, 10).await;
        assert!(
            observe(
                &pool,
                &[InputPath::from_path(Path::new("book.epub")).unwrap()],
                &[fingerprint(10)]
            )
            .await
            .unwrap()
            .is_empty()
        );
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
            &[],
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
            &[],
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
                &[],
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
                &[],
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
                &[],
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
