//! Per-job writeback orchestrator.
//!
//! Loads a typed library-relative snapshot, composes entry repairs with metadata
//! and cover changes, and publishes only an accepted candidate. Relocation and
//! forward recovery remain owned by the same claimed job.
//!
//! Blocking filesystem phases retain the concurrency permit. SQL stays async;
//! the queue owns terminal bookkeeping and events.
//!
//! ## `RLS` system-context invariant
//!
//! `run_once` MUST be called with a `PgPool` whose connections set the
//! Postgres `GUC` `app.system_context = 'writeback'` on `after_connect`.
//! The `manifestations_update_system` and `manifestations_select_system`
//! `RLS` policies match against this value; without it every
//! `UPDATE manifestations` write produces zero rows affected and the
//! `current_file_hash` update silently disappears.  The writeback pool is
//! created by `crate::test_support::db::writeback_pool_for` in tests and by
//! `db::init_writeback_pool`, instantiated in `lib.rs::run` at runtime.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::models::library_path_claim;
use crate::models::storage_library::LibraryId;
use crate::services::files::{LibraryFiles, LibraryLocation, RelativeFilePath};
#[cfg(test)]
use quick_xml::Reader;
#[cfg(test)]
use quick_xml::events::Event;
#[cfg(test)]
use sha2::{Digest, Sha256};
use sqlx::{Acquire, PgPool, Postgres, Transaction};
use tokio::sync::OwnedSemaphorePermit;
use uuid::Uuid;

use crate::config::Config;
use crate::models::manifestation_format::ManifestationFormat;
use crate::services::epub::{self, repack, zip_layer};
use crate::services::ingestion::path_template;

use super::cover_embed;
use super::error::WritebackError;
use super::opf_rewrite::{self, Target};
use super::path_rename;
use super::{AttemptPhase, JobReason};

/// Terminal outcome of a single `run_once` call.
///
/// Permanent relocation diagnoses retain their intent until queue finalisation.
#[derive(Debug)]
pub enum RunOutcome {
    /// Confirmed relocation loss or external change requiring atomic finalisation.
    RelocationTerminal {
        /// The manifestation whose evidence was checked.
        manifestation_id: Uuid,
        /// The claimed job's stored reason.
        reason: String,
        /// The exact pair to clear while retaining claim exclusion.
        intent: RelocationIntent,
        /// Diagnosis recorded with the skipped job.
        diagnosis: String,
    },
    /// Writeback completed cleanly.  `current_file_hash` is the new
    /// on-disk `SHA-256`; the queue emits `writeback_complete` with it.
    Success {
        /// The manifestation that was updated.
        manifestation_id: Uuid,
        /// The `writeback_jobs.reason` string (e.g. `"metadata"`, `"cover"`).
        reason: String,
        /// Hex-encoded `SHA-256` of the on-disk file after writeback.
        current_file_hash: String,
    },
    /// Retrying will not help (unsupported format or missing file).  Bypasses the retry path directly to
    /// `mark_skipped`.  `skip_reason` is the user-facing explanation.
    Skipped {
        /// The manifestation the job referenced.
        manifestation_id: Uuid,
        /// The `writeback_jobs.reason` string.
        reason: String,
        /// Human-readable explanation stored in `writeback_jobs.error`.
        skip_reason: String,
    },
    /// Writeback failed in a way that's potentially retryable (the
    /// queue's `finish` decides whether `attempt_count` has reached
    /// `max_attempts` and escalates to `skipped`).
    Failed {
        /// The manifestation the job referenced.
        manifestation_id: Uuid,
        /// The `writeback_jobs.reason` string.
        reason: String,
        /// Error description stored in `writeback_jobs.error`.
        error: String,
    },
}

/// Checked relocation names within a manifestation's owning library.
#[derive(Clone, Debug)]
pub struct RelocationIntent {
    /// Recorded source before relocation.
    pub source: RelativeFilePath,
    /// Exact selected destination.
    pub destination: RelativeFilePath,
}

struct JobSnapshot {
    manifestation_id: Uuid,
    reason: JobReason,
    file_path: RelativeFilePath,
    library_id: LibraryId,
    current_file_hash: String,
    file_size_bytes: i64,
    relocation: Option<RelocationIntent>,
    format: ManifestationFormat,
    cover_path: Option<String>,
    title: Option<String>,
    subtitle: Option<String>,
    description: Option<String>,
    language: Option<String>,
    publisher: Option<String>,
    pub_date: Option<String>,
    isbn_10: Option<String>,
    isbn_13: Option<String>,
    /// Primary author's `sort_name` (role = 'author', position 0).
    /// Used to render the path template; `None` falls back to `Unknown`.
    primary_author: Option<String>,
}

/// Run a claimed writeback with bounded blocking phases and async SQL ownership.
///
/// # Errors
/// Returns source, candidate, relocation, task and database failures.
pub async fn run_once(
    pool: &PgPool,
    config: &Config,
    files: &LibraryFiles,
    job_id: Uuid,
    permit: Arc<OwnedSemaphorePermit>,
) -> Result<RunOutcome, WritebackError> {
    let mut snap = Arc::new(load_snapshot(pool, job_id).await?);
    if let Some(outcome) = reconcile(pool, files, job_id, &snap, Arc::clone(&permit)).await? {
        return Ok(outcome);
    }
    if snap.relocation.is_some() {
        snap = Arc::new(load_snapshot(pool, job_id).await?);
    }
    let manifestation_id = snap.manifestation_id;
    let reason = snap.reason.as_str().to_owned();
    if snap.reason.attempt_phase(false) == AttemptPhase::Carrier {
        return Ok(RunOutcome::Success {
            manifestation_id,
            reason,
            current_file_hash: snap.current_file_hash.clone(),
        });
    }
    if snap.format != ManifestationFormat::Epub {
        return Ok(RunOutcome::Skipped {
            manifestation_id,
            reason,
            skip_reason: format!("format_unsupported: {}", snap.format),
        });
    }
    let phase_snap = Arc::clone(&snap);
    let phase_files = files.clone();
    let published = blocking_phase(Arc::clone(&permit), move || {
        rewrite(&phase_snap, &phase_files)
    })
    .await?;
    let Some(published) = published else {
        return Ok(RunOutcome::Skipped {
            manifestation_id,
            reason,
            skip_reason: format!("file_missing: {}", snap.file_path.as_str()),
        });
    };
    let new_hash = published.hash;
    let post_has_cover = published.report.has_usable_embedded_cover;
    let size = i64::try_from(published.size)
        .map_err(|error| WritebackError::Persist(error.to_string()))?;
    let mut tx = pool.begin().await?;
    let mut selection = tx.begin().await?;
    let destination =
        select_destination(&mut selection, &snap, config, files, Arc::clone(&permit)).await;
    if destination.is_ok() {
        selection.commit().await?;
    } else {
        selection.rollback().await?;
    }
    let selected = destination.as_ref().map_or(None, Option::as_ref);
    if let Err(error) = sqlx::query!(
        "UPDATE manifestations SET current_file_hash = $1, has_embedded_cover = $3, file_size_bytes = $4, relocation_source_path = $5, relocation_destination_path = $6 WHERE id = $2",
        new_hash, manifestation_id, post_has_cover, size,
        selected.map(|_| snap.file_path.as_str()), selected.map(RelativeFilePath::as_str),
    ).execute(&mut *tx).await {
        tracing::error!(error = %error, %manifestation_id, final_path = snap.file_path.as_str(), attempted_hash = %new_hash,
            "writeback hash UPDATE failed after publication; a retry must reconcile");
        return Err(error.into());
    }
    tx.commit().await?;
    let durability_error = if let Some(destination) = destination? {
        let intent = RelocationIntent {
            source: snap.file_path.clone(),
            destination,
        };
        relocate(pool, files, &snap, &intent, &new_hash, Arc::clone(&permit)).await?
    } else {
        None
    };
    if let Some(error) = durability_error {
        return Ok(RunOutcome::Failed {
            manifestation_id,
            reason,
            error: format!("relocation durability uncertain: {error}"),
        });
    }
    if snap.reason == JobReason::Cover
        && let Some(pending) = &snap.cover_path
    {
        let pending = pending.clone();
        if let Err(error) = blocking_phase(Arc::clone(&permit), move || {
            move_cover_sidecar(&pending).map_err(WritebackError::Io)
        })
        .await
        {
            tracing::warn!(error = %error, %manifestation_id, "cover sidecar move failed");
        }
    }
    Ok(RunOutcome::Success {
        manifestation_id,
        reason,
        current_file_hash: new_hash,
    })
}

pub(super) async fn blocking_phase<T: Send + 'static>(
    permit: Arc<OwnedSemaphorePermit>,
    operation: impl FnOnce() -> Result<T, WritebackError> + Send + 'static,
) -> Result<T, WritebackError> {
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        operation()
    })
    .await?
}

fn rewrite(
    snap: &JobSnapshot,
    files: &LibraryFiles,
) -> Result<Option<repack::Published>, WritebackError> {
    let location = LibraryLocation {
        library_id: snap.library_id,
        path: snap.file_path.clone(),
    };
    let opened = match files.open_source(&location) {
        Ok(opened) => opened,
        Err(crate::services::files::LibraryFileError::Io(error))
            if error.kind() == std::io::ErrorKind::NotFound =>
        {
            return Ok(None);
        }
        Err(error) => return Err(error.into()),
    };
    let (handle, pre_report) = epub::inspect(opened.file)?;
    if pre_report.outcome == epub::ValidationOutcome::Quarantined {
        return Err(epub::EpubError::CandidateRejected("source archive rejected".into()).into());
    }
    let opf_path = pre_report
        .opf_data
        .as_ref()
        .map(|opf| opf.opf_path.as_str())
        .ok_or(WritebackError::MissingOpf)?;
    let repairs = epub::repair::RepairPlan::from_report(&pre_report);
    let opf_bytes = match repairs.replacement(&handle, opf_path)? {
        Some(bytes) => bytes,
        None => {
            zip_layer::read_entry(&handle, opf_path).ok_or(zip::result::ZipError::FileNotFound)?
        }
    };
    let target = Target {
        title: snap.title.as_deref(),
        subtitle: snap.subtitle.as_deref(),
        description: snap.description.as_deref(),
        language: snap.language.as_deref(),
        publisher: snap.publisher.as_deref(),
        pub_date: snap.pub_date.as_deref(),
        isbn_10: snap.isbn_10.as_deref(),
        isbn_13: snap.isbn_13.as_deref(),
        series: None,
    };
    let new_opf = opf_rewrite::transform(&opf_bytes, &target)?;
    let cover_plan = if snap.reason == JobReason::Cover
        && let Some(path) = snap.cover_path.as_deref()
    {
        Some(cover_embed::plan_embed(&new_opf, &std::fs::read(path)?)?)
    } else {
        None
    };
    let final_opf = cover_plan
        .as_ref()
        .and_then(|plan| plan.opf_replacement.as_ref())
        .unwrap_or(&new_opf);
    let opf_dir = opf_path
        .rsplit_once('/')
        .map_or("", |(directory, _)| directory);
    let mut replacements = HashMap::new();
    let mut additions = Vec::new();
    if let Some(plan) = &cover_plan {
        for (href, bytes) in &plan.binary_replacements {
            replacements.insert(resolve_opf_relative(opf_dir, href)?, bytes.clone());
        }
        for (href, bytes, options) in &plan.additions {
            additions.push((
                resolve_opf_relative(opf_dir, href)?,
                bytes.clone(),
                options.clone(),
            ));
        }
    }
    let (parent, basename) = path_rename::parent(files.library(snap.library_id)?, &snap.file_path)?;
    match repack::publish(&parent, &basename, &pre_report, |candidate| {
        repack::with_modifications(
            &handle,
            candidate,
            Some(opf_path),
            Some(final_opf),
            &replacements,
            &additions,
            &repairs,
        )
    }) {
        Ok(published) => Ok(Some(published)),
        Err(error @ epub::EpubError::PublicationUncertain { .. }) => {
            tracing::error!(%error, manifestation_id = %snap.manifestation_id, "writeback publication uncertain; no relocation or row-success update");
            Err(error.into())
        }
        Err(error) => Err(error.into()),
    }
}

async fn select_destination(
    tx: &mut Transaction<'_, Postgres>,
    snap: &JobSnapshot,
    config: &Config,
    files: &LibraryFiles,
    permit: Arc<OwnedSemaphorePermit>,
) -> Result<Option<RelativeFilePath>, WritebackError> {
    let current = LibraryLocation {
        library_id: snap.library_id,
        path: snap.file_path.clone(),
    };
    if library_path_claim::owner(tx, &current).await? != Some(snap.manifestation_id) {
        return Err(WritebackError::Persist(
            "recorded path ownership changed".into(),
        ));
    }
    let Some(candidate) =
        render_target_path(snap, config.library_path.as_str(), snap.file_path.as_path())?
    else {
        return Ok(None);
    };
    let candidate: RelativeFilePath = candidate
        .to_str()
        .ok_or_else(|| WritebackError::Persist("non-UTF8 template path".into()))?
        .parse()?;
    if owned_candidate(&snap.file_path, &candidate) {
        return Ok(None);
    }
    for suffix in 1.. {
        let path = path_template::collision_candidate(&candidate, suffix)?;
        let location = LibraryLocation {
            library_id: snap.library_id,
            path: path.clone(),
        };
        library_path_claim::exclude(tx, &location).await?;
        if library_path_claim::owner(tx, &location)
            .await?
            .is_some_and(|owner| owner != snap.manifestation_id)
        {
            continue;
        }
        let phase_files = files.clone();
        let phase_location = location.clone();
        let occupied = blocking_phase(Arc::clone(&permit), move || {
            match phase_files
                .library(phase_location.library_id)?
                .symlink_metadata(phase_location.path.as_path())
            {
                Ok(_) => Ok(true),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
                Err(error) => Err(error.into()),
            }
        })
        .await?;
        if !occupied && library_path_claim::reserve(tx, &location, snap.manifestation_id).await? {
            return Ok(Some(path));
        }
    }
    Err(WritebackError::Persist("collision suffix exhausted".into()))
}

async fn relocate(
    pool: &PgPool,
    files: &LibraryFiles,
    snap: &JobSnapshot,
    intent: &RelocationIntent,
    hash: &str,
    permit: Arc<OwnedSemaphorePermit>,
) -> Result<Option<std::io::Error>, WritebackError> {
    let phase_files = files.clone();
    let phase_intent = intent.clone();
    let library_id = snap.library_id;
    let hash = hash.to_owned();
    let movement = blocking_phase(permit, move || {
        let root = phase_files.library(library_id)?;
        path_rename::prepare_destination(root, &phase_intent.destination)?;
        path_rename::move_existing(root, &phase_intent.source, &phase_intent.destination, &hash)
    })
    .await?;
    record_movement(pool, snap.manifestation_id, intent, movement).await
}

async fn record_movement(
    pool: &PgPool,
    manifestation_id: Uuid,
    intent: &RelocationIntent,
    movement: path_rename::MoveResult,
) -> Result<Option<std::io::Error>, WritebackError> {
    match movement {
        path_rename::MoveResult::Durable => {
            finalise_location(pool, manifestation_id, intent, &intent.destination).await?;
            Ok(None)
        }
        path_rename::MoveResult::VisibleUncertain(error) => {
            tracing::error!(%error, location = intent.destination.as_str(), "relocation visible with unconfirmed durability");
            let update = sqlx::query!(
                "UPDATE manifestations SET file_path = $1 WHERE id = $2 AND relocation_source_path = $3 AND relocation_destination_path = $1",
                intent.destination.as_str(), manifestation_id, intent.source.as_str(),
            ).execute(pool).await?;
            if update.rows_affected() != 1 {
                return Err(sqlx::Error::RowNotFound.into());
            }
            Ok(Some(error))
        }
    }
}

async fn finalise_location(
    pool: &PgPool,
    manifestation_id: Uuid,
    intent: &RelocationIntent,
    location: &RelativeFilePath,
) -> Result<(), WritebackError> {
    let mut tx = pool.begin().await?;
    finalise_location_in(&mut tx, manifestation_id, intent, location).await?;
    tx.commit().await?;
    Ok(())
}

async fn finalise_location_in(
    tx: &mut Transaction<'_, Postgres>,
    manifestation_id: Uuid,
    intent: &RelocationIntent,
    location: &RelativeFilePath,
) -> Result<(), WritebackError> {
    let update = sqlx::query!(
        "UPDATE manifestations SET file_path = $1, relocation_source_path = NULL, relocation_destination_path = NULL WHERE id = $2 AND relocation_source_path = $3 AND relocation_destination_path = $4",
        location.as_str(), manifestation_id, intent.source.as_str(), intent.destination.as_str(),
    ).execute(&mut **tx).await?;
    if update.rows_affected() != 1 {
        return Err(sqlx::Error::RowNotFound.into());
    }
    library_path_claim::release_obsolete(tx, manifestation_id).await?;
    Ok(())
}

async fn reconcile(
    pool: &PgPool,
    files: &LibraryFiles,
    job_id: Uuid,
    snap: &JobSnapshot,
    permit: Arc<OwnedSemaphorePermit>,
) -> Result<Option<RunOutcome>, WritebackError> {
    reconcile_with(pool, files, job_id, snap, permit, path_rename::recover).await
}

async fn reconcile_with(
    pool: &PgPool,
    files: &LibraryFiles,
    job_id: Uuid,
    snap: &JobSnapshot,
    permit: Arc<OwnedSemaphorePermit>,
    recover: impl FnOnce(
        &cap_std::fs::Dir,
        &RelativeFilePath,
        &RelativeFilePath,
        &str,
        i64,
    ) -> Result<path_rename::Recovery, WritebackError>
    + Send
    + 'static,
) -> Result<Option<RunOutcome>, WritebackError> {
    let Some(intent) = &snap.relocation else {
        return Ok(None);
    };
    let mut tx = pool.begin().await?;
    sqlx::query!(
        "SELECT m.id FROM manifestations m JOIN writeback_jobs wj ON wj.manifestation_id = m.id
         WHERE wj.id = $1 AND wj.status = 'in_progress' AND m.id = $2
         AND m.relocation_source_path = $3 AND m.relocation_destination_path = $4
         FOR UPDATE OF m, wj",
        job_id,
        snap.manifestation_id,
        intent.source.as_str(),
        intent.destination.as_str(),
    )
    .fetch_one(&mut *tx)
    .await?;
    for path in [&intent.source, &intent.destination] {
        let location = LibraryLocation {
            library_id: snap.library_id,
            path: path.clone(),
        };
        if library_path_claim::owner(&mut tx, &location).await? != Some(snap.manifestation_id) {
            return Err(WritebackError::Persist(
                "relocation path ownership changed".into(),
            ));
        }
    }
    tx.commit().await?;
    let phase_files = files.clone();
    let phase_intent = intent.clone();
    let hash = snap.current_file_hash.clone();
    let size = snap.file_size_bytes;
    let library_id = snap.library_id;
    let recovery = blocking_phase(permit, move || {
        recover(
            phase_files.library(library_id)?,
            &phase_intent.source,
            &phase_intent.destination,
            &hash,
            size,
        )
    })
    .await?;
    let mut tx = pool.begin().await?;
    sqlx::query!(
        "SELECT id FROM writeback_jobs WHERE id = $1 AND manifestation_id = $2 AND status = 'in_progress' FOR UPDATE",
        job_id,
        snap.manifestation_id,
    )
    .fetch_one(&mut *tx)
    .await?;
    match recovery {
        path_rename::Recovery::VisibleUncertain(error) => {
            let diagnosis = format!("relocation durability uncertain: {error}");
            let update = sqlx::query!(
                "UPDATE manifestations SET file_path = $1 WHERE id = $2 AND relocation_source_path = $3 AND relocation_destination_path = $1",
                intent.destination.as_str(),
                snap.manifestation_id,
                intent.source.as_str(),
            )
            .execute(&mut *tx)
            .await?;
            if update.rows_affected() != 1 {
                return Err(sqlx::Error::RowNotFound.into());
            }
            tx.commit().await?;
            Ok(Some(RunOutcome::Failed {
                manifestation_id: snap.manifestation_id,
                reason: snap.reason.as_str().to_owned(),
                error: diagnosis,
            }))
        }
        path_rename::Recovery::Destination => {
            finalise_recovery(&mut tx, job_id, snap, intent).await?;
            tx.commit().await?;
            Ok(None)
        }
        path_rename::Recovery::SourceOccupied => {
            finalise_location_in(&mut tx, snap.manifestation_id, intent, &intent.source).await?;
            tx.commit().await?;
            Ok(Some(RunOutcome::Failed {
                manifestation_id: snap.manifestation_id,
                reason: snap.reason.as_str().to_owned(),
                error: "relocation destination occupied; source restored".into(),
            }))
        }
        path_rename::Recovery::Terminal(diagnosis) => Ok(Some(RunOutcome::RelocationTerminal {
            manifestation_id: snap.manifestation_id,
            reason: snap.reason.as_str().to_owned(),
            intent: intent.clone(),
            diagnosis: diagnosis.into(),
        })),
    }
}

async fn finalise_recovery(
    tx: &mut Transaction<'_, Postgres>,
    job_id: Uuid,
    snap: &JobSnapshot,
    intent: &RelocationIntent,
) -> Result<(), WritebackError> {
    finalise_location_in(tx, snap.manifestation_id, intent, &intent.destination).await?;
    if snap.reason.attempt_phase(true) == AttemptPhase::Recovery {
        let counted = sqlx::query!(
            "UPDATE writeback_jobs SET attempt_count = attempt_count + 1 WHERE id = $1 AND status = 'in_progress'",
            job_id,
        )
        .execute(&mut **tx)
        .await?;
        if counted.rows_affected() != 1 {
            return Err(sqlx::Error::RowNotFound.into());
        }
    }
    Ok(())
}

fn owned_candidate(current: &RelativeFilePath, candidate: &RelativeFilePath) -> bool {
    let current = current.as_path();
    let candidate = candidate.as_path();
    if current.parent() != candidate.parent() || current.extension() != candidate.extension() {
        return false;
    }
    let Some(stem) = candidate.file_stem().and_then(|stem| stem.to_str()) else {
        return false;
    };
    let Some(current_stem) = current.file_stem().and_then(|stem| stem.to_str()) else {
        return false;
    };
    if current_stem == stem {
        return true;
    }
    current_stem
        .strip_prefix(stem)
        .and_then(|suffix| suffix.strip_prefix(" ("))
        .and_then(|suffix| suffix.strip_suffix(')'))
        .is_some_and(|number| {
            number != "1"
                && !number.is_empty()
                && !number.starts_with('0')
                && number.bytes().all(|digit| digit.is_ascii_digit())
        })
}

/// Pure helper: compute the rendered target path from the snapshot +
/// library root.  Returns `None` when path-rename should be skipped
/// (empty `library_path`, or rendered path equals current `src_path`).
fn render_target_path(
    snap: &JobSnapshot,
    library_path: &str,
    src_path: &Path,
) -> Result<Option<PathBuf>, WritebackError> {
    if library_path.is_empty() {
        return Ok(None);
    }
    let mut vars: HashMap<String, String> = HashMap::new();
    if let Some(t) = snap.title.as_deref() {
        vars.insert("Title".into(), t.to_string());
    }
    if let Some(a) = snap.primary_author.as_deref() {
        vars.insert("Author".into(), a.to_string());
    }
    let ext = src_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("epub")
        .to_string();
    vars.insert("ext".into(), ext);

    let relative = path_template::render(path_template::DEFAULT_TEMPLATE, &vars);
    let relative = path_rename::normalise_relative(&relative)?;
    let candidate = relative;

    if candidate == src_path {
        Ok(None)
    } else {
        Ok(Some(candidate))
    }
}

// ── Snapshot load ─────────────────────────────────────────────────────────

async fn load_snapshot(pool: &PgPool, job_id: Uuid) -> Result<JobSnapshot, WritebackError> {
    let row = sqlx::query!(
        r#"SELECT wj.manifestation_id, wj.reason AS "reason: JobReason",
                  m.work_id, m.library_id, m.file_path, m.current_file_hash, m.file_size_bytes,
                  m.relocation_source_path, m.relocation_destination_path,
                  m.format AS "format: ManifestationFormat",
                  m.cover_path,
                  m.publisher, m.pub_date, m.isbn_10, m.isbn_13,
                  w.title, w.subtitle, w.description, w.language
             FROM writeback_jobs wj
             JOIN manifestations m ON m.id = wj.manifestation_id
             JOIN works w          ON w.id = m.work_id
            WHERE wj.id = $1"#,
        job_id,
    )
    .fetch_optional(pool)
    .await?
    .ok_or(WritebackError::JobNotFound(job_id))?;

    let primary_author: Option<String> = sqlx::query_scalar!(
        "SELECT a.sort_name \
           FROM work_authors wa \
           JOIN authors a ON a.id = wa.author_id \
          WHERE wa.work_id = $1 AND wa.role = 'author' \
          ORDER BY wa.position \
          LIMIT 1",
        row.work_id,
    )
    .fetch_optional(pool)
    .await?;

    Ok(JobSnapshot {
        manifestation_id: row.manifestation_id,
        reason: row.reason,
        file_path: row.file_path.parse()?,
        library_id: LibraryId::from_uuid(row.library_id),
        current_file_hash: row.current_file_hash,
        file_size_bytes: row.file_size_bytes,
        relocation: match (row.relocation_source_path, row.relocation_destination_path) {
            (Some(source), Some(destination)) => Some(RelocationIntent {
                source: source.parse()?,
                destination: destination.parse()?,
            }),
            (None, None) => None,
            _ => return Err(WritebackError::Persist("unpaired relocation intent".into())),
        },
        format: row.format,
        cover_path: row.cover_path,
        title: Some(row.title),
        subtitle: row.subtitle,
        description: row.description,
        language: row.language,
        publisher: row.publisher,
        // `query!` validates pub_date as Option<Date> at compile time; a
        // decode error here is an infrastructure fault, so propagate it
        // rather than masking it with a `None` fallback that would let the
        // writeback report success with `<dc:date>` left stale.
        pub_date: row.pub_date.map(|d| d.to_string()),
        isbn_10: row.isbn_10,
        isbn_13: row.isbn_13,
        primary_author,
    })
}

// ── OPF path + entry helpers ──────────────────────────────────────────────

#[cfg(test)]
fn extract_opf_path(container_bytes: &[u8]) -> Option<String> {
    let xml = match std::str::from_utf8(container_bytes) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(error = %e, "writeback: container.xml is not valid UTF-8");
            return None;
        }
    };
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    loop {
        let event = match reader.read_event() {
            Ok(ev) => ev,
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "writeback: container.xml parse error; OPF path detection aborted"
                );
                return None;
            }
        };
        match event {
            Event::Empty(e) | Event::Start(e) if e.name().as_ref() == "rootfile" => {
                if let Some(attr) = e
                    .attributes()
                    .flatten()
                    .find(|a| a.key.as_ref() == "full-path")
                {
                    return Some(attr.value.into_owned());
                }
            }
            Event::Eof => return None,
            _ => {}
        }
    }
}

/// Resolve an OPF-relative href against the OPF's directory, yielding a
/// ZIP-absolute path suitable for `repack::with_modifications`.
/// Collapses `./` and rejects any `..` segment — symmetric with
/// `path_rename::normalise_relative`. Pre-writeback validation should
/// already reject pathological hrefs; this is a belt-and-braces check
/// at the writeback boundary.
fn resolve_opf_relative(opf_dir: &str, href: &str) -> Result<String, WritebackError> {
    if href.split('/').any(|seg| seg == "..") {
        return Err(WritebackError::Persist(format!(
            "opf-relative href contains ..: {href}"
        )));
    }
    if opf_dir.is_empty() {
        return Ok(href.to_string());
    }
    let stripped = href.strip_prefix("./").unwrap_or(href);
    Ok(format!("{opf_dir}/{stripped}"))
}

fn move_cover_sidecar(pending_path: &str) -> std::io::Result<()> {
    if !pending_path.contains("_covers/pending/") {
        return Ok(());
    }
    let accepted = pending_path.replace("_covers/pending/", "_covers/accepted/");
    let accepted_path = Path::new(&accepted);
    if let Some(parent) = accepted_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::rename(pending_path, accepted_path)
}

#[cfg(test)]
#[expect(
    clippy::let_underscore_must_use,
    reason = "test code: discarding fmt::Write result on String in test helper is intentional; fmt::Write on String is infallible"
)]
mod tests {
    use super::*;

    async fn run_fixture(
        pool: &PgPool,
        config: &Config,
        job_id: Uuid,
        root: &Path,
    ) -> Result<RunOutcome, WritebackError> {
        let id = crate::models::storage_library::default_library_id(pool).await?;
        let root = root.to_str().unwrap().parse().unwrap();
        let files = LibraryFiles::open([(id, root)], &config.ingestion_path).unwrap();
        let permit = Arc::new(
            Arc::new(tokio::sync::Semaphore::new(1))
                .acquire_owned()
                .await
                .unwrap(),
        );
        super::run_once(pool, config, &files, job_id, permit).await
    }

    async fn persisted_path(pool: &PgPool, id: Uuid, root: &Path) -> PathBuf {
        let path = sqlx::query_scalar!("SELECT file_path FROM manifestations WHERE id = $1", id)
            .fetch_one(pool)
            .await
            .unwrap();
        root.join(path)
    }

    use crate::config::{CoverConfig, EnrichmentConfig, WritebackConfig};
    use crate::models::manifestation_format::ManifestationFormat;
    use std::io::Write;
    use zip::ZipWriter;
    use zip::write::{ExtendedFileOptions, FileOptions};

    use crate::test_support::db::{ingestion_pool_for, writeback_pool_for};

    fn test_config(library: &Path) -> (Config, crate::services::files::LibraryFiles) {
        let (storage_config, files) = crate::test_support::test_storage_config(
            Some(library),
            crate::models::storage_library::LibraryId::from_uuid(uuid::Uuid::new_v4()),
        );
        let config = Config {
            port: 3000,
            database_url: String::new(),
            library_path: storage_config.library_path,
            ingestion_path: storage_config.ingestion_path,
            log_level: "info".into(),
            db_max_connections: 5,
            oidc_issuer_url: String::new(),
            oidc_client_id: String::new(),
            oidc_client_secret: String::new(),
            oidc_redirect_uri: String::new(),
            local_auth_enabled: true,
            resource_server_issuer: String::new(),
            resource_server_audience: String::new(),
            resource_server_jwks_url: String::new(),
            resource_server_require_at_jwt: false,
            login_rate_per_min: 10,
            login_throttle_base_secs: 2,
            login_throttle_cap_secs: 900,
            password_min_length: 8,
            password_max_length: 256,
            password_min_zxcvbn_score: 2,
            password_breach_check_enabled: true,
            self_registration_enabled: false,
            recovery_pin_ttl_secs: 900,
            recovery_pin_dir: "./reverie-recovery".into(),
            trusted_client_ip_header: None,
            migration_database_url: None,
            auto_migrate: false,
            ingestion_database_url: String::new(),
            accepted_formats: vec![ManifestationFormat::Epub],
            cleanup_imported: false,
            cleanup_duplicates: false,
            enrichment: EnrichmentConfig {
                enabled: false,
                concurrency: 1,
                poll_idle_secs: 30,
                fetch_budget_secs: 15,
                http_timeout_secs: 10,
                max_attempts: 3,
                cache_ttl_hit_days: 1,
                cache_ttl_miss_days: 1,
                cache_ttl_error_mins: 1,
            },
            cover: CoverConfig {
                max_bytes: 10_485_760,
                download_timeout_secs: 30,
                min_long_edge_px: 1000,
                redirect_limit: 3,
            },
            writeback: WritebackConfig {
                enabled: true,
                concurrency: 1,
                poll_idle_secs: 1,
                max_attempts: 3,
            },
            opds: crate::config::OpdsConfig {
                enabled: false,
                page_size: 50,
                realm: "Reverie OPDS".into(),
                public_url: None,
            },
            security: crate::config::SecurityConfig {
                behind_https: false,
                hsts_include_subdomains: false,
                hsts_preload: false,
                csp_report_endpoint: None,
                frontend_dist_path: None,
                csp_html_header: None,
                csp_api_header: None,
            },
            googlebooks_api_key: None,
            hardcover_api_token: None,
            operator_contact: None,
            ingestion_dsn_defaulted: false,
        };
        (config, files)
    }

    /// Build an EPUB fixture whose container.xml points at a NON-default
    /// OPF path (`OEBPS/package.opf` instead of `content.opf`).  Returns
    /// the on-disk path as an owned string; the [`tempfile::TempDir`] is
    /// held in the tuple so the file persists for the test lifetime.
    fn make_fixture_epub(title: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        let container_xml = br#"<?xml version="1.0" encoding="UTF-8"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles>
    <rootfile full-path="OEBPS/package.opf" media-type="application/oebps-package+xml"/>
  </rootfiles>
</container>"#;
        let opf = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<package version="3.0" xmlns="http://www.idpf.org/2007/opf" unique-identifier="pub-id" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:opf="http://www.idpf.org/2007/opf">
  <metadata>
    <dc:identifier id="pub-id">urn:uuid:fixture</dc:identifier>
    <dc:title>{title}</dc:title>
    <dc:language>en</dc:language>
  </metadata>
  <manifest>
    <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
  </manifest>
  <spine><itemref idref="nav"/></spine>
</package>"#
        );
        let nav = br#"<?xml version="1.0"?>
<html xmlns="http://www.w3.org/1999/xhtml"><head><title>nav</title></head><body><nav epub:type="toc" xmlns:epub="http://www.idpf.org/2007/ops"><ol><li><a href="nav.xhtml">nav</a></li></ol></nav></body></html>"#;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fixture.epub");
        let file = std::fs::File::create(&path).unwrap();
        let mut w = ZipWriter::new(file);

        let stored: FileOptions<ExtendedFileOptions> =
            FileOptions::default().compression_method(zip::CompressionMethod::Stored);
        w.start_file("mimetype", stored).unwrap();
        w.write_all(b"application/epub+zip").unwrap();
        let deflate: FileOptions<ExtendedFileOptions> = FileOptions::default();
        w.start_file("META-INF/container.xml", deflate.clone())
            .unwrap();
        w.write_all(container_xml).unwrap();
        w.start_file("OEBPS/package.opf", deflate.clone()).unwrap();
        w.write_all(opf.as_bytes()).unwrap();
        w.start_file("OEBPS/nav.xhtml", deflate).unwrap();
        w.write_all(nav).unwrap();
        w.finish().unwrap();
        (dir, path)
    }

    fn initial_hex_sha256(bytes: &[u8]) -> String {
        let d = Sha256::digest(bytes);
        let mut s = String::with_capacity(64);
        for b in d {
            use std::fmt::Write;
            let _ = write!(s, "{b:02x}");
        }
        s
    }

    async fn insert_fixture(
        ing_pool: &PgPool,
        marker: &str,
        file_path: &str,
        ingestion_hash: &str,
    ) -> (Uuid, Uuid) {
        let file_path = std::path::Path::new(file_path)
            .file_name()
            .unwrap()
            .to_str()
            .unwrap();
        let title = format!("WbFixture-{marker}");
        let work_id = sqlx::query_scalar!(
            "INSERT INTO works (title, sort_title) VALUES ($1, $1) RETURNING id",
            title,
        )
        .fetch_one(ing_pool)
        .await
        .unwrap();
        let m_id = sqlx::query_scalar!(
            // Set enrichment_status = 'complete' so these fixtures don't
            // leak into the enrichment queue's claim_next under parallel
            // test execution (the column defaults to 'pending').
            "WITH inserted AS (INSERT INTO manifestations \
               (library_id, work_id, format, file_path, ingestion_file_hash, current_file_hash, \
                file_size_bytes, ingestion_status, validation_status, enrichment_status) \
             VALUES ((SELECT id FROM libraries WHERE configuration_key = 'default'), $1, 'epub'::manifestation_format, $2, $3, $3, 1000, \
                     'complete'::ingestion_status, 'clean'::validation_status, \
                     'complete'::enrichment_status) \
             RETURNING *), claimed AS (INSERT INTO library_path_claims (library_id, path, manifestation_id) SELECT library_id, file_path, id FROM inserted) SELECT id AS \"id!\" FROM inserted",
            work_id,
            file_path,
            ingestion_hash,
        )
        .fetch_one(ing_pool)
        .await
        .unwrap();
        (work_id, m_id)
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn relocation_collision_owned_bare_and_suffix_paths_remain_stable(pool: PgPool) {
        let wb = writeback_pool_for(&pool).await;
        let library = crate::models::storage_library::default_library_id(&pool)
            .await
            .unwrap();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_str().unwrap().parse().unwrap();
        let files = crate::test_support::test_library_files_at(&root, library);
        for (index, path) in [
            "Author/Title.epub",
            "Author/Title (2).epub",
            "Author/Title (3).epub",
        ]
        .into_iter()
        .enumerate()
        {
            let (_, id) = insert_fixture(
                &pool,
                &index.to_string(),
                &format!("fixture-{index}.epub"),
                &index.to_string(),
            )
            .await;
            let location = LibraryLocation {
                library_id: library,
                path: path.parse().unwrap(),
            };
            let mut tx = wb.begin().await.unwrap();
            assert!(
                library_path_claim::reserve(&mut tx, &location, id)
                    .await
                    .unwrap()
            );
            sqlx::query!(
                "UPDATE manifestations SET file_path = $2 WHERE id = $1",
                id,
                path
            )
            .execute(&mut *tx)
            .await
            .unwrap();
            library_path_claim::release_obsolete(&mut tx, id)
                .await
                .unwrap();
            tx.commit().await.unwrap();
            files
                .library(library)
                .unwrap()
                .create_dir_all("Author")
                .unwrap();
            files
                .library(library)
                .unwrap()
                .write(path, b"owned bytes")
                .unwrap();
            let job = sqlx::query_scalar!("INSERT INTO writeback_jobs (manifestation_id, reason) VALUES ($1, 'metadata') RETURNING id", id).fetch_one(&pool).await.unwrap();
            let mut snap = load_snapshot(&wb, job).await.unwrap();
            snap.title = Some("Title".into());
            snap.primary_author = Some("Author".into());
            for _ in 0..3 {
                let mut tx = wb.begin().await.unwrap();
                assert!(
                    select_destination(
                        &mut tx,
                        &snap,
                        &test_config(dir.path()).0,
                        &files,
                        fixture_permit().await
                    )
                    .await
                    .unwrap()
                    .is_none()
                );
                tx.commit().await.unwrap();
                assert_eq!(
                    files.library(library).unwrap().read(path).unwrap(),
                    b"owned bytes"
                );
            }
            sqlx::query!("DELETE FROM manifestations WHERE id = $1", id)
                .execute(&pool)
                .await
                .unwrap();
            files.library(library).unwrap().remove_file(path).unwrap();
        }
    }

    #[test]
    fn relocation_collision_exact_owned_suffix_rule() {
        let candidate = "Author/Title.epub".parse().unwrap();
        for path in [
            "Author/Title.epub",
            "Author/Title (2).epub",
            "Author/Title (3).epub",
            "Author/Title (999999999999999999999999999999999999).epub",
        ] {
            assert!(
                owned_candidate(&path.parse().unwrap(), &candidate),
                "{path}"
            );
        }
        for path in [
            "Other/Title (2).epub",
            "Author/Other (2).epub",
            "Author/Title (2).pdf",
            "Author/Title (1).epub",
            "Author/Title (02).epub",
            "Author/Title (+2).epub",
            "Author/Title (2a).epub",
        ] {
            assert!(
                !owned_candidate(&path.parse().unwrap(), &candidate),
                "{path}"
            );
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn relocation_collision_foreign_claim_without_file_selects_suffix(pool: PgPool) {
        let wb = writeback_pool_for(&pool).await;
        let (dir, files, job, mut snap) = recovery_fixture(&pool, "metadata").await;
        let intent = snap.relocation.as_ref().unwrap().clone();
        finalise_location(&wb, snap.manifestation_id, &intent, &intent.source)
            .await
            .unwrap();
        snap = load_snapshot(&wb, job).await.unwrap();
        snap.title = Some("Title".into());
        snap.primary_author = Some("Author".into());
        let (_, foreign) =
            insert_fixture(&pool, "foreign-claim", "foreign.epub", "foreign-claim").await;
        let candidate = LibraryLocation {
            library_id: snap.library_id,
            path: "Author/Title.epub".parse().unwrap(),
        };
        let mut tx = wb.begin().await.unwrap();
        assert!(
            library_path_claim::reserve(&mut tx, &candidate, foreign)
                .await
                .unwrap()
        );
        tx.commit().await.unwrap();
        assert!(!dir.path().join(candidate.path.as_path()).exists());
        let mut tx = wb.begin().await.unwrap();
        let selected = select_destination(
            &mut tx,
            &snap,
            &test_config(dir.path()).0,
            &files,
            fixture_permit().await,
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(selected.as_str(), "Author/Title (2).epub");
        assert_eq!(
            library_path_claim::owner(&mut tx, &candidate)
                .await
                .unwrap(),
            Some(foreign)
        );
        tx.rollback().await.unwrap();
        assert!(dir.path().join("fixture.epub").exists());
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn relocation_collision_stale_recovery_never_adopts_foreign_identical_bytes(
        pool: PgPool,
    ) {
        let wb = writeback_pool_for(&pool).await;
        let (dir, files, job, snap) = recovery_fixture(&pool, "metadata").await;
        let intent = snap.relocation.as_ref().unwrap();
        let original = dir.path().join(intent.source.as_path());
        let destination = dir.path().join(intent.destination.as_path());
        std::fs::copy(&original, &destination).unwrap();
        finalise_location(&wb, snap.manifestation_id, intent, &intent.source)
            .await
            .unwrap();
        let (_, foreign) = insert_fixture(
            &pool,
            "foreign-identical",
            "foreign.epub",
            "foreign-identical",
        )
        .await;
        let mut tx = wb.begin().await.unwrap();
        assert!(
            library_path_claim::reserve(
                &mut tx,
                &LibraryLocation {
                    library_id: snap.library_id,
                    path: intent.destination.clone()
                },
                foreign
            )
            .await
            .unwrap()
        );
        tx.commit().await.unwrap();
        assert!(
            reconcile(&wb, &files, job, &snap, fixture_permit().await)
                .await
                .is_err()
        );
        assert_eq!(
            std::fs::read(original).unwrap(),
            std::fs::read(destination).unwrap()
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn relocation_collision_simultaneous_ingestion_and_writeback_keep_distinct_owners(
        pool: PgPool,
    ) {
        let wb = writeback_pool_for(&pool).await;
        let ing = ingestion_pool_for(&pool).await;
        let (dir, path) = make_fixture_epub("Title");
        let (work, id) = insert_fixture(
            &ing,
            "simultaneous",
            path.to_str().unwrap(),
            "older-ingestion-hash",
        )
        .await;
        sqlx::query!(
            "UPDATE works SET title = 'Title', sort_title = 'title' WHERE id = $1",
            work
        )
        .execute(&pool)
        .await
        .unwrap();
        let input = tempfile::tempdir().unwrap();
        std::fs::copy(&path, input.path().join("Title.epub")).unwrap();
        let library_id = crate::models::storage_library::default_library_id(&pool)
            .await
            .unwrap();
        let (mut config, _files) = test_config(dir.path());
        config.library_path = dir.path().to_str().unwrap().parse().unwrap();
        config.ingestion_path = input.path().to_str().unwrap().parse().unwrap();

        let files = LibraryFiles::open(
            [(library_id, config.library_path.clone())],
            &config.ingestion_path,
        )
        .unwrap();
        let job = sqlx::query_scalar!("INSERT INTO writeback_jobs (manifestation_id, reason) VALUES ($1, 'metadata') RETURNING id", id).fetch_one(&pool).await.unwrap();
        assert_eq!(
            super::super::queue::claim_next(&wb)
                .await
                .unwrap()
                .unwrap()
                .0,
            job
        );
        let cancel = tokio_util::sync::CancellationToken::new();
        let (handle, commands) = crate::services::ingestion::coordinator_channel();
        let settings = crate::test_support::test_settings();
        settings.write().await.ingestion.cleanup_imported = false;
        let worker = tokio::spawn(crate::services::ingestion::run_watcher(
            config.clone(),
            ing.clone(),
            cancel.clone(),
            files.clone(),
            settings,
            commands,
        ));
        handle.scan().await.unwrap();
        assert!(matches!(
            run_once(&wb, &config, &files, job, fixture_permit().await)
                .await
                .unwrap(),
            RunOutcome::Success { .. }
        ));
        tokio::time::timeout(std::time::Duration::from_secs(20), async {
            loop {
                let inputs = crate::models::ingestion_input::current_page(&ing, None)
                    .await
                    .unwrap();
                if inputs.iter().any(|input| {
                    input.status == crate::models::ingestion_input::InputStatus::Imported
                }) {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        })
        .await
        .unwrap();
        cancel.cancel();
        worker.await.unwrap().unwrap();
        let rows = sqlx::query!("SELECT m.id, m.file_path, c.manifestation_id FROM manifestations m JOIN library_path_claims c ON c.library_id = m.library_id AND c.path = m.file_path WHERE m.library_id = $1 ORDER BY m.id LIMIT 3", library_id.as_uuid()).fetch_all(&pool).await.unwrap();
        assert_eq!(rows.len(), 2);
        assert_ne!(rows[0].file_path, rows[1].file_path);
        for row in rows {
            assert_eq!(row.id, row.manifestation_id);
            assert!(dir.path().join(row.file_path).is_file());
        }
    }

    #[tokio::test]
    async fn bounded_writeback_blocking_permit_survives_async_cancellation() {
        let semaphore = Arc::new(tokio::sync::Semaphore::new(1));
        let release = Arc::new(std::sync::Barrier::new(2));
        let blocking_release = Arc::clone(&release);
        let (entered, ready) = tokio::sync::oneshot::channel();
        let (completed, finished) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(blocking_phase(
            Arc::new(Arc::clone(&semaphore).acquire_owned().await.unwrap()),
            move || {
                entered.send(()).unwrap();
                blocking_release.wait();
                completed.send(()).unwrap();
                Ok(())
            },
        ));
        ready.await.unwrap();
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        assert!(Arc::clone(&semaphore).try_acquire_owned().is_err());
        tokio::task::spawn_blocking(move || release.wait())
            .await
            .unwrap();
        finished.await.unwrap();
        let permit = semaphore.acquire_owned().await.unwrap();
        drop(permit);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn candidate_publication_regression_preserves_source_and_row(pool: PgPool) {
        let app = writeback_pool_for(&pool).await;
        let ing = ingestion_pool_for(&pool).await;
        let mut png = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(1, 1)
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        let (dir, path) = make_fixture_epub_with_cover("Original", &png.into_inner());
        let original = std::fs::read(&path).unwrap();
        let hash = initial_hex_sha256(&original);
        let (_, id) = insert_fixture(&ing, "reject", path.to_str().unwrap(), &hash).await;
        let pending = dir.path().join("invalid.png");
        std::fs::write(&pending, b"\x89PNG\r\n\x1a\ninvalid image").unwrap();
        sqlx::query!(
            "UPDATE manifestations SET cover_path = $1 WHERE id = $2",
            pending.to_str().unwrap(),
            id
        )
        .execute(&ing)
        .await
        .unwrap();
        let job = sqlx::query_scalar!("INSERT INTO writeback_jobs (manifestation_id, reason) VALUES ($1, 'cover') RETURNING id", id).fetch_one(&ing).await.unwrap();
        let result = run_fixture(&app, &test_config(dir.path()).0, job, dir.path()).await;
        assert!(matches!(
            result,
            Err(WritebackError::Epub(epub::EpubError::CandidateRejected(_)))
        ));
        assert_eq!(std::fs::read(&path).unwrap(), original);
        let row = sqlx::query!("SELECT file_path, current_file_hash, ingestion_file_hash FROM manifestations WHERE id = $1", id).fetch_one(&app).await.unwrap();
        assert_eq!(row.file_path, "fixture.epub");
        assert_eq!(row.current_file_hash, hash);
        assert_eq!(row.ingestion_file_hash, hash);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn bounded_writeback_two_library_relative_sources_and_unknown_identity(pool: PgPool) {
        let app = writeback_pool_for(&pool).await;
        let ing = ingestion_pool_for(&pool).await;
        let default = crate::models::storage_library::default_library_id(&ing)
            .await
            .unwrap();
        let other = sqlx::query_scalar!(
            "INSERT INTO libraries (configuration_key) VALUES ('other') RETURNING id"
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let other = LibraryId::from_uuid(other);
        let (first, first_path) = make_fixture_epub("First");
        let (second, second_path) = make_fixture_epub("Second");
        let first_root = first.path().to_str().unwrap().parse().unwrap();
        let second_root: crate::config::AbsoluteRootPath =
            second.path().to_str().unwrap().parse().unwrap();
        let files = LibraryFiles::open(
            [(default, first_root), (other, second_root.clone())],
            &second_root,
        )
        .unwrap();
        let first_original = std::fs::read(&first_path).unwrap();
        let second_original = std::fs::read(&second_path).unwrap();
        let (_, id) = insert_fixture(
            &ing,
            "two",
            second_path.to_str().unwrap(),
            &initial_hex_sha256(&second_original),
        )
        .await;
        sqlx::query!(
            "WITH changed AS (UPDATE manifestations SET library_id = $1 WHERE id = $2 RETURNING *), removed AS (DELETE FROM library_path_claims c USING changed m WHERE c.manifestation_id = m.id AND c.library_id <> m.library_id) INSERT INTO library_path_claims (library_id, path, manifestation_id) SELECT library_id, file_path AS path, id FROM changed UNION SELECT library_id, relocation_source_path, id FROM changed WHERE relocation_source_path IS NOT NULL UNION SELECT library_id, relocation_destination_path, id FROM changed WHERE relocation_destination_path IS NOT NULL ON CONFLICT (library_id, path) DO NOTHING",
            other.as_uuid(),
            id
        )
        .execute(&ing)
        .await
        .unwrap();
        let job = sqlx::query_scalar!("INSERT INTO writeback_jobs (manifestation_id, reason) VALUES ($1, 'metadata') RETURNING id", id).fetch_one(&ing).await.unwrap();
        let semaphore = Arc::new(tokio::sync::Semaphore::new(1));
        let outcome = super::run_once(
            &app,
            &test_config(second.path()).0,
            &files,
            job,
            Arc::new(Arc::clone(&semaphore).acquire_owned().await.unwrap()),
        )
        .await
        .unwrap();
        assert!(matches!(outcome, RunOutcome::Success { .. }));
        assert_eq!(std::fs::read(first_path).unwrap(), first_original);
        let row = sqlx::query!("SELECT library_id, file_path, current_file_hash, ingestion_file_hash, file_size_bytes FROM manifestations WHERE id = $1", id).fetch_one(&app).await.unwrap();
        assert_eq!(row.library_id, other.as_uuid());
        let final_bytes = std::fs::read(second.path().join(&row.file_path)).unwrap();
        assert_eq!(row.current_file_hash, initial_hex_sha256(&final_bytes));
        assert_eq!(
            row.ingestion_file_hash,
            initial_hex_sha256(&second_original)
        );
        assert_eq!(
            row.file_size_bytes,
            i64::try_from(final_bytes.len()).unwrap()
        );
        let files = crate::test_support::test_library_files_at(
            &first.path().to_str().unwrap().parse().unwrap(),
            default,
        );
        let result = super::run_once(
            &app,
            &test_config(second.path()).0,
            &files,
            job,
            Arc::new(Arc::clone(&semaphore).acquire_owned().await.unwrap()),
        )
        .await;
        assert!(matches!(
            result,
            Err(WritebackError::Library(
                crate::services::files::LibraryFileError::UnknownLibrary
            ))
        ));
        assert_eq!(
            std::fs::read(second.path().join(row.file_path)).unwrap(),
            final_bytes
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn bounded_writeback_relocation_failure_preserves_published_metadata(pool: PgPool) {
        let app = writeback_pool_for(&pool).await;
        let ing = ingestion_pool_for(&pool).await;
        let (dir, path) = make_fixture_epub("Original");
        let original_hash = initial_hex_sha256(&std::fs::read(&path).unwrap());
        let (work_id, id) =
            insert_fixture(&ing, "overlong", path.to_str().unwrap(), &original_hash).await;
        let title = "é".repeat(150);
        sqlx::query!("UPDATE works SET title = $1 WHERE id = $2", title, work_id)
            .execute(&ing)
            .await
            .unwrap();
        let job = sqlx::query_scalar!("INSERT INTO writeback_jobs (manifestation_id, reason) VALUES ($1, 'metadata') RETURNING id", id).fetch_one(&ing).await.unwrap();
        for _ in 0..2 {
            if sqlx::query_scalar!("SELECT status::text FROM writeback_jobs WHERE id = $1", job)
                .fetch_one(&app)
                .await
                .unwrap()
                .as_deref()
                == Some("pending")
            {
                assert_eq!(
                    super::super::queue::claim_next(&app).await.unwrap(),
                    Some((job, 1))
                );
            }
            let result = run_fixture(&app, &test_config(dir.path()).0, job, dir.path()).await;
            assert!(matches!(result, Err(WritebackError::Io(_))));
            let bytes = std::fs::read(&path).unwrap();
            let row = sqlx::query!("SELECT library_id, file_path, current_file_hash, ingestion_file_hash, file_size_bytes FROM manifestations WHERE id = $1", id).fetch_one(&app).await.unwrap();
            assert_eq!(row.file_path, "fixture.epub");
            assert_eq!(row.current_file_hash, initial_hex_sha256(&bytes));
            assert_ne!(row.current_file_hash, original_hash);
            assert_eq!(row.ingestion_file_hash, original_hash);
            assert_eq!(row.file_size_bytes, i64::try_from(bytes.len()).unwrap());
            let has_cover = sqlx::query_scalar!(
                "SELECT has_embedded_cover FROM manifestations WHERE id = $1",
                id,
            )
            .fetch_one(&app)
            .await
            .unwrap();
            assert_eq!(has_cover, Some(false));
            let opf = zip_layer::read_entry_from_bytes(&bytes, "OEBPS/package.opf").unwrap();
            assert!(String::from_utf8(opf).unwrap().contains(&title));
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn bounded_writeback_location_sql_failure_retains_forward_intent(pool: PgPool) {
        let app = writeback_pool_for(&pool).await;
        let ing = ingestion_pool_for(&pool).await;
        let (dir, path) = make_fixture_epub("Original");
        let (_, id) = insert_fixture(&ing, "forward", path.to_str().unwrap(), "original").await;
        let job = sqlx::query_scalar!(
            "INSERT INTO writeback_jobs (manifestation_id, reason) VALUES ($1, $2) RETURNING id",
            id,
            "metadata"
        )
        .fetch_one(&ing)
        .await
        .unwrap();
        assert_eq!(
            super::super::queue::claim_next(&app).await.unwrap(),
            Some((job, 1))
        );
        let mut snap = load_snapshot(&app, job).await.unwrap();
        let root = dir.path().to_str().unwrap().parse().unwrap();
        let files = crate::test_support::test_library_files_at(&root, snap.library_id);
        let published = rewrite(&snap, &files).unwrap().unwrap();
        let accepted_bytes = std::fs::read(&path).unwrap();
        let intent = RelocationIntent {
            source: snap.file_path.clone(),
            destination: "relocated.epub".parse().unwrap(),
        };
        sqlx::query!("WITH changed AS (UPDATE manifestations SET current_file_hash = $1, file_size_bytes = $2, relocation_source_path = $3, relocation_destination_path = $4 WHERE id = $5 RETURNING *), removed AS (DELETE FROM library_path_claims c USING changed m WHERE c.manifestation_id = m.id AND c.library_id <> m.library_id) INSERT INTO library_path_claims (library_id, path, manifestation_id) SELECT library_id, file_path AS path, id FROM changed UNION SELECT library_id, relocation_source_path, id FROM changed WHERE relocation_source_path IS NOT NULL UNION SELECT library_id, relocation_destination_path, id FROM changed WHERE relocation_destination_path IS NOT NULL ON CONFLICT (library_id, path) DO NOTHING",
            published.hash, i64::try_from(published.size).unwrap(), intent.source.as_str(), intent.destination.as_str(), id).execute(&app).await.unwrap();
        snap = load_snapshot(&app, job).await.unwrap();
        app.close().await;
        let result = relocate(
            &app,
            &files,
            &snap,
            &intent,
            &published.hash,
            fixture_permit().await,
        )
        .await;
        assert!(matches!(
            result,
            Err(WritebackError::Db(sqlx::Error::PoolClosed))
        ));
        assert!(!path.exists());
        assert_eq!(
            std::fs::read(dir.path().join("relocated.epub")).unwrap(),
            accepted_bytes
        );
        let wb = writeback_pool_for(&pool).await;
        let row = sqlx::query!("SELECT file_path, relocation_source_path, relocation_destination_path FROM manifestations WHERE id = $1", id).fetch_one(&wb).await.unwrap();
        assert_eq!(row.file_path, "fixture.epub");
        assert_eq!(row.relocation_source_path.as_deref(), Some("fixture.epub"));
        assert_eq!(
            row.relocation_destination_path.as_deref(),
            Some("relocated.epub")
        );
        assert!(
            reconcile(&wb, &files, job, &snap, fixture_permit().await)
                .await
                .unwrap()
                .is_none()
        );
        let row = sqlx::query!("SELECT file_path, relocation_source_path, current_file_hash FROM manifestations WHERE id = $1", id).fetch_one(&wb).await.unwrap();
        assert_eq!(row.file_path, "relocated.epub");
        assert!(row.relocation_source_path.is_none());
        assert_eq!(row.current_file_hash, published.hash);
    }

    async fn fixture_permit() -> Arc<OwnedSemaphorePermit> {
        Arc::new(
            Arc::new(tokio::sync::Semaphore::new(1))
                .acquire_owned()
                .await
                .unwrap(),
        )
    }

    async fn recovery_fixture(
        pool: &PgPool,
        reason: &str,
    ) -> (tempfile::TempDir, LibraryFiles, Uuid, JobSnapshot) {
        let ing = ingestion_pool_for(pool).await;
        let wb = writeback_pool_for(pool).await;
        let (dir, path) = make_fixture_epub("Original");
        let bytes = std::fs::read(&path).unwrap();
        let hash = initial_hex_sha256(&bytes);
        let (_, id) = insert_fixture(&ing, "recovery", path.to_str().unwrap(), &hash).await;
        sqlx::query!("WITH changed AS (UPDATE manifestations SET file_size_bytes = $2, relocation_source_path = file_path, relocation_destination_path = $3 WHERE id = $1 RETURNING *), removed AS (DELETE FROM library_path_claims c USING changed m WHERE c.manifestation_id = m.id AND c.library_id <> m.library_id) INSERT INTO library_path_claims (library_id, path, manifestation_id) SELECT library_id, file_path AS path, id FROM changed UNION SELECT library_id, relocation_source_path, id FROM changed WHERE relocation_source_path IS NOT NULL UNION SELECT library_id, relocation_destination_path, id FROM changed WHERE relocation_destination_path IS NOT NULL ON CONFLICT (library_id, path) DO NOTHING", id, i64::try_from(bytes.len()).unwrap(), "recovered.epub").execute(&wb).await.unwrap();
        let job = sqlx::query_scalar!("INSERT INTO writeback_jobs (manifestation_id, reason, status) VALUES ($1, $2, $3::text::writeback_status) RETURNING id", id, reason, "in_progress").fetch_one(&ing).await.unwrap();
        let snap = load_snapshot(&wb, job).await.unwrap();
        let root = dir.path().to_str().unwrap().parse().unwrap();
        let files = crate::test_support::test_library_files_at(&root, snap.library_id);
        (dir, files, job, snap)
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn relocation_accounting_finalisation_rollback_panic_and_cancellation(pool: PgPool) {
        let wb = writeback_pool_for(&pool).await;
        let (_dir, files, job, snap) = recovery_fixture(&pool, "metadata").await;
        let intent = snap.relocation.as_ref().unwrap();
        path_rename::move_existing(
            files.library(snap.library_id).unwrap(),
            &intent.source,
            &intent.destination,
            &snap.current_file_hash,
        )
        .unwrap();
        let mut tx = wb.begin().await.unwrap();
        finalise_recovery(&mut tx, job, &snap, intent)
            .await
            .unwrap();
        sqlx::query!(
            "DELETE FROM library_path_claims WHERE manifestation_id = $1 AND path = $2",
            snap.manifestation_id,
            intent.destination.as_str()
        )
        .execute(&mut *tx)
        .await
        .unwrap();
        let error = tx.commit().await.unwrap_err();
        assert_eq!(
            error.as_database_error().unwrap().code().as_deref(),
            Some("23503")
        );
        assert_eq!(
            sqlx::query_scalar!(
                "SELECT attempt_count FROM writeback_jobs WHERE id = $1",
                job
            )
            .fetch_one(&wb)
            .await
            .unwrap(),
            0
        );
        assert!(load_snapshot(&wb, job).await.unwrap().relocation.is_some());

        let panic_pool = wb.clone();
        let panic_task = tokio::spawn(async move {
            let snap = load_snapshot(&panic_pool, job).await.unwrap();
            let mut tx = panic_pool.begin().await.unwrap();
            finalise_recovery(&mut tx, job, &snap, snap.relocation.as_ref().unwrap())
                .await
                .unwrap();
            panic!("interrupted before recovery commit");
        });
        assert!(panic_task.await.unwrap_err().is_panic());
        assert_eq!(
            sqlx::query_scalar!(
                "SELECT attempt_count FROM writeback_jobs WHERE id = $1",
                job
            )
            .fetch_one(&wb)
            .await
            .unwrap(),
            0
        );

        let cancel_pool = wb.clone();
        let (entered, ready) = tokio::sync::oneshot::channel();
        let cancel_task = tokio::spawn(async move {
            let snap = load_snapshot(&cancel_pool, job).await.unwrap();
            let mut tx = cancel_pool.begin().await.unwrap();
            finalise_recovery(&mut tx, job, &snap, snap.relocation.as_ref().unwrap())
                .await
                .unwrap();
            entered.send(()).unwrap();
            std::future::pending::<()>().await;
            tx.commit().await.unwrap();
        });
        tokio::time::timeout(std::time::Duration::from_secs(5), ready)
            .await
            .unwrap()
            .unwrap();
        cancel_task.abort();
        assert!(cancel_task.await.unwrap_err().is_cancelled());
        assert_eq!(
            sqlx::query_scalar!(
                "SELECT attempt_count FROM writeback_jobs WHERE id = $1",
                job
            )
            .fetch_one(&wb)
            .await
            .unwrap(),
            0
        );
        assert!(load_snapshot(&wb, job).await.unwrap().relocation.is_some());
        super::super::queue::revert_in_progress(&wb).await.unwrap();
        sqlx::query!(
            "UPDATE writeback_jobs SET last_attempted_at = NULL WHERE id = $1",
            job
        )
        .execute(&wb)
        .await
        .unwrap();
        assert_eq!(
            super::super::queue::claim_next(&wb).await.unwrap(),
            Some((job, 0))
        );
        assert_eq!(
            i64::try_from(
                files
                    .library(snap.library_id)
                    .unwrap()
                    .read(intent.destination.as_path())
                    .unwrap()
                    .len()
            )
            .unwrap(),
            snap.file_size_bytes
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn relocation_accounting_committed_recovery_is_one_edit_after_interruption(pool: PgPool) {
        let wb = writeback_pool_for(&pool).await;
        let (_dir, files, job, snap) = recovery_fixture(&pool, "cover").await;
        let intent = snap.relocation.as_ref().unwrap();
        path_rename::move_existing(
            files.library(snap.library_id).unwrap(),
            &intent.source,
            &intent.destination,
            &snap.current_file_hash,
        )
        .unwrap();
        let committed_pool = wb.clone();
        let (entered, ready) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            let snap = load_snapshot(&committed_pool, job).await.unwrap();
            let mut tx = committed_pool.begin().await.unwrap();
            finalise_recovery(&mut tx, job, &snap, snap.relocation.as_ref().unwrap())
                .await
                .unwrap();
            tx.commit().await.unwrap();
            entered.send(()).unwrap();
            std::future::pending::<()>().await;
        });
        tokio::time::timeout(std::time::Duration::from_secs(5), ready)
            .await
            .unwrap()
            .unwrap();
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        assert_eq!(
            sqlx::query_scalar!(
                "SELECT attempt_count FROM writeback_jobs WHERE id = $1",
                job
            )
            .fetch_one(&wb)
            .await
            .unwrap(),
            1
        );
        assert!(load_snapshot(&wb, job).await.unwrap().relocation.is_none());
        super::super::queue::revert_in_progress(&wb).await.unwrap();
        sqlx::query!(
            "UPDATE writeback_jobs SET last_attempted_at = NULL WHERE id = $1",
            job
        )
        .execute(&wb)
        .await
        .unwrap();
        assert_eq!(
            super::super::queue::claim_next(&wb).await.unwrap(),
            Some((job, 2))
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn relocation_accounting_reason_decode_rejects_unknown_text(pool: PgPool) {
        let decoded = sqlx::query!("SELECT 'unrecognised'::text AS \"reason!: JobReason\"")
            .fetch_one(&pool)
            .await;
        assert!(matches!(decoded, Err(sqlx::Error::ColumnDecode { .. })));
        for (wire, reason) in [
            ("metadata", JobReason::Metadata),
            ("cover", JobReason::Cover),
            ("relocation", JobReason::Relocation),
        ] {
            assert_eq!(
                sqlx::query!("SELECT $1::text AS \"reason!: JobReason\"", wire)
                    .fetch_one(&pool)
                    .await
                    .unwrap()
                    .reason,
                reason
            );
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn relocation_recovery_carrier_relocates_without_rewriting_or_cover_mutation(
        pool: PgPool,
    ) {
        let wb = writeback_pool_for(&pool).await;
        let (dir, files, job, snap) = recovery_fixture(&pool, "relocation").await;
        let original = std::fs::read(dir.path().join("fixture.epub")).unwrap();
        let sidecar = dir.path().join("_covers/pending/cover.png");
        std::fs::create_dir_all(sidecar.parent().unwrap()).unwrap();
        std::fs::write(&sidecar, b"sidecar must remain pending").unwrap();
        sqlx::query!(
            "UPDATE manifestations SET cover_path = $2 WHERE id = $1",
            snap.manifestation_id,
            sidecar.to_str().unwrap()
        )
        .execute(&wb)
        .await
        .unwrap();
        for _ in 0..2 {
            let outcome = run_once(
                &wb,
                &test_config(dir.path()).0,
                &files,
                job,
                fixture_permit().await,
            )
            .await
            .unwrap();
            assert!(
                matches!(outcome, RunOutcome::Success { reason, current_file_hash, .. } if reason == "relocation" && current_file_hash == snap.current_file_hash)
            );
            let fresh = load_snapshot(&wb, job).await.unwrap();
            assert_eq!(fresh.file_path.as_str(), "recovered.epub");
            assert!(fresh.relocation.is_none());
            assert_eq!(
                std::fs::read(dir.path().join("recovered.epub")).unwrap(),
                original
            );
            assert!(sidecar.exists());
        }
    }

    async fn continuation_fixture(pool: PgPool, reason: &str) {
        let wb = writeback_pool_for(&pool).await;
        let (dir, files, job, snap) = recovery_fixture(&pool, reason).await;
        let intent = snap.relocation.as_ref().unwrap();
        path_rename::move_existing(
            files.library(snap.library_id).unwrap(),
            &intent.source,
            &intent.destination,
            &snap.current_file_hash,
        )
        .unwrap();
        sqlx::query!("UPDATE works SET title = $2 WHERE id = (SELECT work_id FROM manifestations WHERE id = $1)", snap.manifestation_id, "Newer sibling edit").execute(&wb).await.unwrap();
        if reason == "cover" {
            let sidecar = dir.path().join("_covers/pending/cover.png");
            std::fs::create_dir_all(sidecar.parent().unwrap()).unwrap();
            let img = image::DynamicImage::new_rgb8(10, 10);
            let mut png = std::io::Cursor::new(Vec::new());
            img.write_to(&mut png, image::ImageFormat::Png).unwrap();
            std::fs::write(&sidecar, png.into_inner()).unwrap();
            sqlx::query!(
                "UPDATE manifestations SET cover_path = $2 WHERE id = $1",
                snap.manifestation_id,
                sidecar.to_str().unwrap()
            )
            .execute(&wb)
            .await
            .unwrap();
        }
        let outcome = run_once(
            &wb,
            &test_config(dir.path()).0,
            &files,
            job,
            fixture_permit().await,
        )
        .await
        .unwrap();
        assert!(matches!(outcome, RunOutcome::Success { reason: actual, .. } if actual == reason));
        let fresh = load_snapshot(&wb, job).await.unwrap();
        assert!(fresh.relocation.is_none());
        assert_eq!(
            sqlx::query_scalar!(
                "SELECT attempt_count FROM writeback_jobs WHERE id = $1",
                job
            )
            .fetch_one(&wb)
            .await
            .unwrap(),
            1
        );
        let bytes = files
            .library(fresh.library_id)
            .unwrap()
            .read(fresh.file_path.as_path())
            .unwrap();
        let opf = zip_layer::read_entry_from_bytes(&bytes, "OEBPS/package.opf").unwrap();
        assert!(
            String::from_utf8(opf)
                .unwrap()
                .contains("Newer sibling edit")
        );
        if reason == "cover" {
            assert!(dir.path().join("_covers/accepted/cover.png").exists());
            assert!(
                epub::validate(
                    std::fs::File::open(dir.path().join(fresh.file_path.as_path())).unwrap()
                )
                .unwrap()
                .has_usable_embedded_cover
            );
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn relocation_recovery_newer_metadata_job_reloads_and_continues(pool: PgPool) {
        continuation_fixture(pool, "metadata").await;
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn relocation_recovery_newer_cover_job_reloads_and_continues(pool: PgPool) {
        continuation_fixture(pool, "cover").await;
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn relocation_recovery_foreign_destination_restores_source_and_fails(pool: PgPool) {
        let wb = writeback_pool_for(&pool).await;
        let (dir, files, job, snap) = recovery_fixture(&pool, "metadata").await;
        std::fs::write(dir.path().join("recovered.epub"), b"FOREIGN").unwrap();
        sqlx::query!(
            "WITH changed AS (UPDATE manifestations SET file_path = $2 WHERE id = $1 RETURNING *), removed AS (DELETE FROM library_path_claims c USING changed m WHERE c.manifestation_id = m.id AND c.library_id <> m.library_id) INSERT INTO library_path_claims (library_id, path, manifestation_id) SELECT library_id, file_path AS path, id FROM changed UNION SELECT library_id, relocation_source_path, id FROM changed WHERE relocation_source_path IS NOT NULL UNION SELECT library_id, relocation_destination_path, id FROM changed WHERE relocation_destination_path IS NOT NULL ON CONFLICT (library_id, path) DO NOTHING",
            snap.manifestation_id,
            "recovered.epub"
        )
        .execute(&wb)
        .await
        .unwrap();
        let outcome = run_once(
            &wb,
            &test_config(dir.path()).0,
            &files,
            job,
            fixture_permit().await,
        )
        .await
        .unwrap();
        assert!(matches!(outcome, RunOutcome::Failed { .. }));
        let fresh = load_snapshot(&wb, job).await.unwrap();
        assert_eq!(fresh.file_path.as_str(), "fixture.epub");
        assert!(fresh.relocation.is_none());
        assert_eq!(fresh.current_file_hash, snap.current_file_hash);
        assert_eq!(
            std::fs::read(dir.path().join("recovered.epub")).unwrap(),
            b"FOREIGN"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn relocation_recovery_visible_uncertainty_records_destination_and_retains_intent(
        pool: PgPool,
    ) {
        let wb = writeback_pool_for(&pool).await;
        let (dir, files, job, snap) = recovery_fixture(&pool, "relocation").await;
        let intent = snap.relocation.as_ref().unwrap();
        path_rename::move_existing(
            files.library(snap.library_id).unwrap(),
            &intent.source,
            &intent.destination,
            &snap.current_file_hash,
        )
        .unwrap();
        let error = record_movement(
            &wb,
            snap.manifestation_id,
            intent,
            path_rename::MoveResult::VisibleUncertain(std::io::Error::other(
                "reported sync failure",
            )),
        )
        .await
        .unwrap();
        assert!(error.is_some());
        let retained = load_snapshot(&wb, job).await.unwrap();
        assert_eq!(retained.file_path.as_str(), "recovered.epub");
        assert!(retained.relocation.is_some());
        assert!(!dir.path().join("fixture.epub").exists());
        assert!(
            reconcile(&wb, &files, job, &retained, fixture_permit().await)
                .await
                .unwrap()
                .is_none()
        );
        assert!(load_snapshot(&wb, job).await.unwrap().relocation.is_none());
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn relocation_recovery_deleted_source_parent_adopts_destination(pool: PgPool) {
        deleted_source_parent_fixture(pool, false).await;
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn relocation_recovery_deleted_source_parent_after_visible_uncertainty(pool: PgPool) {
        deleted_source_parent_fixture(pool, true).await;
    }

    async fn deleted_source_parent_fixture(pool: PgPool, visible_uncertain: bool) {
        let wb = writeback_pool_for(&pool).await;
        let (dir, files, job, snap) = recovery_fixture(&pool, "relocation").await;
        let root = files.library(snap.library_id).unwrap();
        let source: RelativeFilePath = "old/fixture.epub".parse().unwrap();
        root.create_dir("old").unwrap();
        root.rename(snap.file_path.as_path(), root, source.as_path())
            .unwrap();
        sqlx::query!(
            "WITH changed AS (UPDATE manifestations SET file_path = $2, relocation_source_path = $2 WHERE id = $1 RETURNING *), removed AS (DELETE FROM library_path_claims c USING changed m WHERE c.manifestation_id = m.id AND c.library_id <> m.library_id) INSERT INTO library_path_claims (library_id, path, manifestation_id) SELECT library_id, file_path AS path, id FROM changed UNION SELECT library_id, relocation_source_path, id FROM changed WHERE relocation_source_path IS NOT NULL UNION SELECT library_id, relocation_destination_path, id FROM changed WHERE relocation_destination_path IS NOT NULL ON CONFLICT (library_id, path) DO NOTHING",
            snap.manifestation_id,
            source.as_str(),
        )
        .execute(&wb)
        .await
        .unwrap();
        let stored = load_snapshot(&wb, job).await.unwrap();
        let intent = stored.relocation.as_ref().unwrap();
        let original = root.read(source.as_path()).unwrap();
        path_rename::move_existing(
            root,
            &source,
            &intent.destination,
            &stored.current_file_hash,
        )
        .unwrap();
        if visible_uncertain {
            assert!(
                record_movement(
                    &wb,
                    stored.manifestation_id,
                    intent,
                    path_rename::MoveResult::VisibleUncertain(std::io::Error::other(
                        "reported sync failure"
                    )),
                )
                .await
                .unwrap()
                .is_some()
            );
        }
        root.remove_dir("old").unwrap();
        let before = load_snapshot(&wb, job).await.unwrap();
        assert_eq!(
            before.file_path,
            if visible_uncertain {
                intent.destination.clone()
            } else {
                source
            }
        );
        assert!(before.relocation.is_some());
        let result = run_once(
            &wb,
            &test_config(dir.path()).0,
            &files,
            job,
            fixture_permit().await,
        )
        .await
        .unwrap();
        assert!(matches!(result, RunOutcome::Success { .. }));
        let fresh = load_snapshot(&wb, job).await.unwrap();
        assert_eq!(fresh.file_path, intent.destination);
        assert!(fresh.relocation.is_none());
        assert_eq!(root.read(intent.destination.as_path()).unwrap(), original);
        assert_eq!(fresh.current_file_hash, stored.current_file_hash);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn relocation_recovery_allows_metadata_patch_during_file_work(pool: PgPool) {
        use crate::test_support::db::{
            app_pool_for, create_admin_and_basic_auth, server_with_real_pools,
        };
        use axum::http::header::{AUTHORIZATION, ETAG, IF_MATCH};

        let app = app_pool_for(&pool).await;
        let ing = ingestion_pool_for(&pool).await;
        let wb = writeback_pool_for(&pool).await;
        let (_dir, files, job, snap) = recovery_fixture(&pool, "metadata").await;
        let (_, basic) = create_admin_and_basic_auth(&app).await;
        let server = server_with_real_pools(&app, &ing);
        let uri = format!("/api/v1/books/{}/metadata", snap.manifestation_id);
        let initial = server
            .get(&uri)
            .add_header(AUTHORIZATION, basic.clone())
            .await;
        initial.assert_status_ok();
        let etag = initial.headers().get(ETAG).unwrap().clone();
        let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let phase_pool = wb.clone();
        let phase_snap = load_snapshot(&wb, job).await.unwrap();
        let recovery = tokio::spawn(async move {
            reconcile_with(
                &phase_pool,
                &files,
                job,
                &phase_snap,
                fixture_permit().await,
                move |root, source, destination, hash, size| {
                    entered_tx.send(()).unwrap();
                    release_rx.recv().unwrap();
                    path_rename::recover(root, source, destination, hash, size)
                },
            )
            .await
        });
        tokio::time::timeout(std::time::Duration::from_secs(5), entered_rx)
            .await
            .unwrap()
            .unwrap();
        let patch = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            server
                .patch(&uri)
                .add_header(AUTHORIZATION, basic)
                .add_header(IF_MATCH, etag)
                .json(&serde_json::json!({"title": "Updated during recovery"}))
                .await
        })
        .await;
        release_tx.send(()).unwrap();
        assert!(recovery.await.unwrap().unwrap().is_none());
        patch
            .expect("metadata PATCH blocked on filesystem recovery")
            .assert_status_ok();
        let fresh = load_snapshot(&wb, job).await.unwrap();
        assert_eq!(fresh.file_path.as_str(), "recovered.epub");
        assert!(fresh.relocation.is_none());
        assert_eq!(fresh.title.as_deref(), Some("Updated during recovery"));
        assert_eq!(
            sqlx::query_scalar!(
                "SELECT attempt_count FROM writeback_jobs WHERE id = $1",
                job
            )
            .fetch_one(&wb)
            .await
            .unwrap(),
            1
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn relocation_recovery_finalisation_database_failure_retains_intent(pool: PgPool) {
        let wb = writeback_pool_for(&pool).await;
        let (dir, files, job, snap) = recovery_fixture(&pool, "metadata").await;
        let phase_pool = wb.clone();
        let phase_snap = load_snapshot(&wb, job).await.unwrap();
        let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let recovery = tokio::spawn(async move {
            reconcile_with(
                &phase_pool,
                &files,
                job,
                &phase_snap,
                fixture_permit().await,
                move |root, source, destination, hash, size| {
                    entered_tx.send(()).unwrap();
                    release_rx.recv().unwrap();
                    path_rename::recover(root, source, destination, hash, size)
                },
            )
            .await
        });
        tokio::time::timeout(std::time::Duration::from_secs(5), entered_rx)
            .await
            .unwrap()
            .unwrap();
        let closed = tokio::time::timeout(std::time::Duration::from_secs(5), wb.close()).await;
        release_tx.send(()).unwrap();
        assert!(matches!(
            recovery.await.unwrap(),
            Err(WritebackError::Db(sqlx::Error::PoolClosed))
        ));
        closed.expect("recovery retained a database connection during filesystem work");
        let fresh_pool = writeback_pool_for(&pool).await;
        let fresh = load_snapshot(&fresh_pool, job).await.unwrap();
        assert!(fresh.relocation.is_some());
        assert_eq!(fresh.file_path, snap.file_path);
        assert_eq!(
            sqlx::query_scalar!(
                "SELECT attempt_count FROM writeback_jobs WHERE id = $1",
                job
            )
            .fetch_one(&fresh_pool)
            .await
            .unwrap(),
            0
        );
        assert!(dir.path().join("recovered.epub").exists());
        assert!(!dir.path().join("fixture.epub").exists());
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn relocation_recovery_sql_failure_retains_intent_before_adoption(pool: PgPool) {
        let wb = writeback_pool_for(&pool).await;
        let (dir, files, job, snap) = recovery_fixture(&pool, "relocation").await;
        let intent = snap.relocation.as_ref().unwrap();
        path_rename::move_existing(
            files.library(snap.library_id).unwrap(),
            &intent.source,
            &intent.destination,
            &snap.current_file_hash,
        )
        .unwrap();
        wb.close().await;
        assert!(matches!(
            reconcile(&wb, &files, job, &snap, fixture_permit().await).await,
            Err(WritebackError::Db(sqlx::Error::PoolClosed))
        ));
        let fresh_pool = writeback_pool_for(&pool).await;
        assert!(
            load_snapshot(&fresh_pool, job)
                .await
                .unwrap()
                .relocation
                .is_some()
        );
        assert!(dir.path().join("recovered.epub").exists());
        assert!(!dir.path().join("fixture.epub").exists());
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn relocation_recovery_unreadable_retains_intent_without_rewrite(pool: PgPool) {
        let wb = writeback_pool_for(&pool).await;
        let (dir, files, job, snap) = recovery_fixture(&pool, "metadata").await;
        std::fs::remove_file(dir.path().join("fixture.epub")).unwrap();
        std::fs::create_dir(dir.path().join("fixture.epub")).unwrap();
        assert!(matches!(
            run_once(
                &wb,
                &test_config(dir.path()).0,
                &files,
                job,
                fixture_permit().await
            )
            .await,
            Err(WritebackError::Io(_))
        ));
        let fresh = load_snapshot(&wb, job).await.unwrap();
        assert!(fresh.relocation.is_some());
        assert_eq!(fresh.current_file_hash, snap.current_file_hash);
    }

    /// Task 16 + Task 24: full `run_once` on a fixture EPUB whose OPF lives
    /// at `OEBPS/package.opf` (not the default `content.opf`).  Verifies:
    /// - the non-default OPF is discovered via `META-INF/container.xml`
    /// - the rewritten OPF carries the new title
    /// - `current_file_hash` changes after writeback
    /// - `ingestion_file_hash` is immutable across the writeback
    #[sqlx::test(migrations = "./migrations")]
    async fn run_once_finds_non_default_opf_and_updates_hash(pool: PgPool) {
        let app_pool = writeback_pool_for(&pool).await;
        let ing_pool = ingestion_pool_for(&pool).await;
        let marker = Uuid::new_v4().simple().to_string();

        let (_dir, path) = make_fixture_epub("Old Title");
        let original_bytes = std::fs::read(&path).unwrap();
        let original_hash = initial_hex_sha256(&original_bytes);

        let (work_id, m_id) =
            insert_fixture(&ing_pool, &marker, path.to_str().unwrap(), &original_hash).await;

        // Set the works.title to the new value.  Simulates the canonical
        // pointer having moved — our job represents the writeback that
        // follows.
        let new_title = format!("New Title {marker}");
        sqlx::query!(
            "UPDATE works SET title = $1 WHERE id = $2",
            new_title,
            work_id
        )
        .execute(&ing_pool)
        .await
        .unwrap();

        let job_id = sqlx::query_scalar!(
            "INSERT INTO writeback_jobs (manifestation_id, reason) \
             VALUES ($1, 'metadata') RETURNING id",
            m_id,
        )
        .fetch_one(&ing_pool)
        .await
        .unwrap();

        let outcome = run_fixture(
            &app_pool,
            &test_config(path.parent().unwrap()).0,
            job_id,
            path.parent().unwrap(),
        )
        .await
        .unwrap();
        assert!(
            matches!(outcome, RunOutcome::Success { .. }),
            "run_once should succeed: {outcome:?}"
        );

        // OPF at OEBPS/package.opf should contain the new title.
        let new_bytes =
            std::fs::read(persisted_path(&app_pool, m_id, path.parent().unwrap()).await).unwrap();
        let opf_bytes = zip_layer::read_entry_from_bytes(&new_bytes, "OEBPS/package.opf").unwrap();
        let opf_str = String::from_utf8(opf_bytes).unwrap();
        assert!(
            opf_str.contains(&format!("<dc:title>{new_title}</dc:title>")),
            "new title not present in OPF at OEBPS/package.opf: {opf_str}"
        );

        // Hash columns: current changed, ingestion unchanged.
        let row = sqlx::query!(
            "SELECT current_file_hash, ingestion_file_hash \
             FROM manifestations WHERE id = $1",
            m_id,
        )
        .fetch_one(&app_pool)
        .await
        .unwrap();
        assert_ne!(
            row.current_file_hash, original_hash,
            "current_file_hash must change"
        );
        assert_eq!(
            row.ingestion_file_hash, original_hash,
            "ingestion_file_hash must NOT change"
        );
    }

    /// Task 24 continuation: two successive writebacks on the same
    /// manifestation.  `ingestion_file_hash` must be constant across
    /// both; `current_file_hash` must change each time.
    #[sqlx::test(migrations = "./migrations")]
    async fn bounded_writeback_ingestion_file_hash_immutable_across_writeback_chain(pool: PgPool) {
        let app_pool = writeback_pool_for(&pool).await;
        let ing_pool = ingestion_pool_for(&pool).await;
        let marker = Uuid::new_v4().simple().to_string();

        let (_dir, path) = make_fixture_epub("Initial");
        let original_bytes = std::fs::read(&path).unwrap();
        let original_hash = initial_hex_sha256(&original_bytes);

        let (work_id, m_id) =
            insert_fixture(&ing_pool, &marker, path.to_str().unwrap(), &original_hash).await;

        // First writeback: set title to A.
        let title_a = format!("First {marker}");
        sqlx::query!(
            "UPDATE works SET title = $1 WHERE id = $2",
            title_a,
            work_id
        )
        .execute(&ing_pool)
        .await
        .unwrap();
        let j1 = sqlx::query_scalar!(
            "INSERT INTO writeback_jobs (manifestation_id, reason) VALUES ($1, 'metadata') RETURNING id",
            m_id,
        )
        .fetch_one(&ing_pool)
        .await
        .unwrap();
        run_fixture(
            &app_pool,
            &test_config(path.parent().unwrap()).0,
            j1,
            path.parent().unwrap(),
        )
        .await
        .unwrap();

        let hash_after_first = sqlx::query_scalar!(
            "SELECT current_file_hash FROM manifestations WHERE id = $1",
            m_id
        )
        .fetch_one(&app_pool)
        .await
        .unwrap();
        assert_ne!(hash_after_first, original_hash);

        // Second writeback: set title to B.
        let title_b = format!("Second {marker}");
        sqlx::query!(
            "UPDATE works SET title = $1 WHERE id = $2",
            title_b,
            work_id
        )
        .execute(&ing_pool)
        .await
        .unwrap();
        let j2 = sqlx::query_scalar!(
            "INSERT INTO writeback_jobs (manifestation_id, reason) VALUES ($1, 'metadata') RETURNING id",
            m_id,
        )
        .fetch_one(&ing_pool)
        .await
        .unwrap();
        run_fixture(
            &app_pool,
            &test_config(path.parent().unwrap()).0,
            j2,
            path.parent().unwrap(),
        )
        .await
        .unwrap();

        let row = sqlx::query!(
            "SELECT current_file_hash, ingestion_file_hash FROM manifestations WHERE id = $1",
            m_id,
        )
        .fetch_one(&app_pool)
        .await
        .unwrap();
        assert_ne!(
            row.current_file_hash, hash_after_first,
            "second writeback must change current_file_hash again"
        );
        assert_eq!(
            row.ingestion_file_hash, original_hash,
            "ingestion_file_hash must NEVER change"
        );
    }

    /// Path-rename E2E: when the rendered
    /// path differs from the on-disk file, `run_once` must move the file
    /// AND update `manifestations.file_path`.
    #[sqlx::test(migrations = "./migrations")]
    async fn run_once_renames_file_to_template_path(pool: PgPool) {
        let app_pool = writeback_pool_for(&pool).await;
        let ing_pool = ingestion_pool_for(&pool).await;
        let marker = Uuid::new_v4().simple().to_string();

        // Build the fixture inside a tempdir that doubles as library_root.
        let (lib_dir, src_path) = make_fixture_epub(&format!("Initial-{marker}"));
        let original_bytes = std::fs::read(&src_path).unwrap();
        let original_hash = initial_hex_sha256(&original_bytes);
        let library_root = lib_dir.path().to_str().unwrap().to_string();

        let (work_id, m_id) = insert_fixture(
            &ing_pool,
            &marker,
            src_path.to_str().unwrap(),
            &original_hash,
        )
        .await;

        // Bind an author so the template renders {Author}/{Title}.epub.
        let author_sort = format!("Author{marker}");
        let author_id = sqlx::query_scalar!(
            "INSERT INTO authors (name, sort_name) VALUES ($1, $1) RETURNING id",
            author_sort,
        )
        .fetch_one(&ing_pool)
        .await
        .unwrap();
        sqlx::query!(
            "INSERT INTO work_authors (work_id, author_id, role, position) \
             VALUES ($1, $2, 'author', 0)",
            work_id,
            author_id,
        )
        .execute(&ing_pool)
        .await
        .unwrap();

        // Set works.title to a value that drives a rename (template
        // renders to a different path than the fixture's tempdir name).
        let new_title = format!("Renamed{marker}");
        sqlx::query!(
            "UPDATE works SET title = $1 WHERE id = $2",
            new_title,
            work_id
        )
        .execute(&ing_pool)
        .await
        .unwrap();

        let job_id = sqlx::query_scalar!(
            "INSERT INTO writeback_jobs (manifestation_id, reason) \
             VALUES ($1, 'metadata') RETURNING id",
            m_id,
        )
        .fetch_one(&ing_pool)
        .await
        .unwrap();

        // Use a config with the lib_dir as library_path so path-rename engages.
        let (mut cfg, _files) = test_config(lib_dir.path());
        cfg.library_path = library_root.parse().unwrap();

        let outcome = run_fixture(&app_pool, &cfg, job_id, src_path.parent().unwrap())
            .await
            .unwrap();
        assert!(
            matches!(outcome, RunOutcome::Success { .. }),
            "run_once should succeed: {outcome:?}"
        );

        // The src_path should no longer exist; the new template path should.
        let expected_new = std::path::PathBuf::from(&library_root)
            .join(&author_sort)
            .join(format!("{new_title}.epub"));
        assert!(
            !src_path.exists(),
            "old src path must be unlinked: {}",
            src_path.display()
        );
        assert!(
            expected_new.exists(),
            "rendered path must exist: {}",
            expected_new.display()
        );

        // DB: file_path updated, current_file_hash matches new file.
        let row = sqlx::query!(
            "SELECT file_path, current_file_hash FROM manifestations WHERE id = $1",
            m_id,
        )
        .fetch_one(&app_pool)
        .await
        .unwrap();
        assert_eq!(
            row.file_path,
            expected_new
                .strip_prefix(&library_root)
                .unwrap()
                .to_str()
                .unwrap()
        );
        assert_ne!(row.current_file_hash, original_hash);
    }

    /// Build a fixture EPUB that already carries an EPUB 3
    /// `cover-image` manifest entry + placeholder PNG bytes.  Enables
    /// the cover-reason writeback to take the *same-media* branch of
    /// `plan_embed` — a binary replacement on the existing manifest
    /// item with no OPF rewrite — so the post-validation doesn't
    /// register the OPF structural change as a regression.
    fn make_fixture_epub_with_cover(
        title: &str,
        cover_bytes: &[u8],
    ) -> (tempfile::TempDir, std::path::PathBuf) {
        let container_xml = br#"<?xml version="1.0" encoding="UTF-8"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles>
    <rootfile full-path="OEBPS/package.opf" media-type="application/oebps-package+xml"/>
  </rootfiles>
</container>"#;
        let opf = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<package version="3.0" xmlns="http://www.idpf.org/2007/opf" unique-identifier="pub-id" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:opf="http://www.idpf.org/2007/opf">
  <metadata>
    <dc:identifier id="pub-id">urn:uuid:fixture</dc:identifier>
    <dc:title>{title}</dc:title>
    <dc:language>en</dc:language>
  </metadata>
  <manifest>
    <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
    <item id="cover-image" href="images/cover.png" media-type="image/png" properties="cover-image"/>
  </manifest>
  <spine><itemref idref="nav"/></spine>
</package>"#
        );
        let nav = br#"<?xml version="1.0"?>
<html xmlns="http://www.w3.org/1999/xhtml"><head><title>nav</title></head><body><nav epub:type="toc" xmlns:epub="http://www.idpf.org/2007/ops"><ol><li><a href="nav.xhtml">nav</a></li></ol></nav></body></html>"#;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fixture.epub");
        let file = std::fs::File::create(&path).unwrap();
        let mut w = ZipWriter::new(file);
        let stored: FileOptions<ExtendedFileOptions> =
            FileOptions::default().compression_method(zip::CompressionMethod::Stored);
        w.start_file("mimetype", stored).unwrap();
        w.write_all(b"application/epub+zip").unwrap();
        let deflate: FileOptions<ExtendedFileOptions> = FileOptions::default();
        w.start_file("META-INF/container.xml", deflate.clone())
            .unwrap();
        w.write_all(container_xml).unwrap();
        w.start_file("OEBPS/package.opf", deflate.clone()).unwrap();
        w.write_all(opf.as_bytes()).unwrap();
        w.start_file("OEBPS/nav.xhtml", deflate.clone()).unwrap();
        w.write_all(nav).unwrap();
        w.start_file("OEBPS/images/cover.png", deflate).unwrap();
        w.write_all(cover_bytes).unwrap();
        w.finish().unwrap();
        (dir, path)
    }

    /// Cover-reason writeback E2E: a pending cover sidecar under
    /// `_covers/pending/` must end up embedded in the EPUB and moved to
    /// `_covers/accepted/` after `run_once`.  Guards the one pipeline
    /// branch that was E2E-untested in the prior review (cover-reason
    /// path through `plan_embed` + sidecar move).
    #[sqlx::test(migrations = "./migrations")]
    async fn run_once_cover_embeds_and_moves_sidecar(pool: PgPool) {
        // Tiny valid PNG: 1x1 black pixel.  Two variants so we can tell
        // the original from the replacement when inspecting the ZIP.
        const PNG_ORIGINAL: &[u8] = &[
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48,
            0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00,
            0x00, 0x1F, 0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x44, 0x41, 0x54, 0x78,
            0x9C, 0x62, 0x00, 0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00,
            0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
        ];
        // Same minimal PNG structure but with a sentinel in the IDAT
        // payload so we can prove the bytes were swapped.
        const PNG_REPLACEMENT: &[u8] = &[
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48,
            0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00,
            0x00, 0x1F, 0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x44, 0x41, 0x54, 0x78,
            0x9C, 0x62, 0xFF, 0xFF, 0xFF, 0x7F, 0x00, 0x05, 0xFE, 0x02, 0xFE, 0xDC, 0xCC, 0x59,
            0xE7, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
        ];

        let app_pool = writeback_pool_for(&pool).await;
        let ing_pool = ingestion_pool_for(&pool).await;
        let marker = Uuid::new_v4().simple().to_string();

        // Fixture EPUB that already has a cover-image manifest entry so
        // plan_embed takes the same-media binary_replacements branch.
        let (_epub_dir, src_path) =
            make_fixture_epub_with_cover(&format!("Cover-{marker}"), PNG_ORIGINAL);
        let original_bytes = std::fs::read(&src_path).unwrap();
        let original_hash = initial_hex_sha256(&original_bytes);

        // Pending cover sidecar under `_covers/pending/`.
        let cover_dir = tempfile::tempdir().unwrap();
        let pending_dir = cover_dir.path().join("_covers").join("pending");
        std::fs::create_dir_all(&pending_dir).unwrap();
        let cover_filename = format!("{marker}.png");
        let pending_path = pending_dir.join(&cover_filename);
        std::fs::write(&pending_path, PNG_REPLACEMENT).unwrap();

        let (_work_id, m_id) = insert_fixture(
            &ing_pool,
            &marker,
            src_path.to_str().unwrap(),
            &original_hash,
        )
        .await;
        let pending_str = pending_path.to_str().unwrap();
        sqlx::query!(
            "UPDATE manifestations SET cover_path = $1 WHERE id = $2",
            pending_str,
            m_id,
        )
        .execute(&ing_pool)
        .await
        .unwrap();

        let job_id = sqlx::query_scalar!(
            "INSERT INTO writeback_jobs (manifestation_id, reason) \
             VALUES ($1, 'cover') RETURNING id",
            m_id,
        )
        .fetch_one(&ing_pool)
        .await
        .unwrap();

        let outcome = run_fixture(
            &app_pool,
            &test_config(src_path.parent().unwrap()).0,
            job_id,
            src_path.parent().unwrap(),
        )
        .await
        .unwrap();
        assert!(
            matches!(outcome, RunOutcome::Success { .. }),
            "cover writeback should succeed: {outcome:?}"
        );

        // The cover bytes inside the EPUB match the replacement, not
        // the original.  (Same-media replacement is in-place under the
        // existing manifest href.)
        let new_bytes =
            std::fs::read(persisted_path(&app_pool, m_id, src_path.parent().unwrap()).await)
                .unwrap();
        let embedded_cover =
            zip_layer::read_entry_from_bytes(&new_bytes, "OEBPS/images/cover.png").unwrap();
        assert_eq!(
            embedded_cover, PNG_REPLACEMENT,
            "embedded cover bytes should match the replacement sidecar"
        );

        // Sidecar moved pending → accepted.
        let accepted_path = cover_dir
            .path()
            .join("_covers")
            .join("accepted")
            .join(&cover_filename);
        assert!(
            !pending_path.exists(),
            "pending sidecar must be moved: {}",
            pending_path.display()
        );
        assert!(
            accepted_path.exists(),
            "accepted sidecar must exist: {}",
            accepted_path.display()
        );
        assert_eq!(
            std::fs::read(&accepted_path).unwrap(),
            PNG_REPLACEMENT,
            "accepted sidecar bytes must match the original cover"
        );

        // current_file_hash advanced; ingestion_file_hash unchanged.
        let row = sqlx::query!(
            "SELECT current_file_hash, ingestion_file_hash FROM manifestations WHERE id = $1",
            m_id,
        )
        .fetch_one(&app_pool)
        .await
        .unwrap();
        assert_ne!(row.current_file_hash, original_hash);
        assert_eq!(row.ingestion_file_hash, original_hash);
    }

    /// Cover-reason writeback must persist the freshly computed
    /// `has_usable_embedded_cover` back onto `manifestations.has_embedded_cover`.
    /// A row ingested with `has_embedded_cover = false` (e.g. no embedded
    /// cover at ingestion time) whose sidecar cover is later embedded via
    /// writeback must not keep reporting `false` forever: the post-writeback
    /// validation already recomputes the truth; this guards that it gets
    /// written, not discarded.
    #[sqlx::test(migrations = "./migrations")]
    async fn run_once_cover_writeback_updates_has_embedded_cover_flag(pool: PgPool) {
        // Real `image`-crate-encoded 1x1 PNG images (unlike the placeholder bytes
        // in `make_fixture_epub_with_cover`'s sibling test above, which are
        // deliberately undecodable and never exercise Layer 5's decode
        // check). This test needs a cover that genuinely decodes so
        // `has_usable_embedded_cover` comes back `true` post-writeback.
        const PNG_ORIGINAL: &[u8] = &[
            137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1,
            8, 2, 0, 0, 0, 144, 119, 83, 222, 0, 0, 0, 15, 73, 68, 65, 84, 120, 1, 1, 4, 0, 251,
            255, 0, 10, 20, 30, 0, 104, 0, 61, 232, 12, 187, 131, 0, 0, 0, 0, 73, 69, 78, 68, 174,
            66, 96, 130,
        ];
        const PNG_REPLACEMENT: &[u8] = &[
            137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1,
            8, 2, 0, 0, 0, 144, 119, 83, 222, 0, 0, 0, 15, 73, 68, 65, 84, 120, 1, 1, 4, 0, 251,
            255, 0, 200, 100, 50, 3, 86, 1, 95, 58, 122, 172, 164, 0, 0, 0, 0, 73, 69, 78, 68, 174,
            66, 96, 130,
        ];

        let app_pool = writeback_pool_for(&pool).await;
        let ing_pool = ingestion_pool_for(&pool).await;
        let marker = Uuid::new_v4().simple().to_string();

        let (_epub_dir, src_path) =
            make_fixture_epub_with_cover(&format!("StaleCover-{marker}"), PNG_ORIGINAL);
        let original_bytes = std::fs::read(&src_path).unwrap();
        let original_hash = initial_hex_sha256(&original_bytes);

        let cover_dir = tempfile::tempdir().unwrap();
        let pending_dir = cover_dir.path().join("_covers").join("pending");
        std::fs::create_dir_all(&pending_dir).unwrap();
        let cover_filename = format!("{marker}.png");
        let pending_path = pending_dir.join(&cover_filename);
        std::fs::write(&pending_path, PNG_REPLACEMENT).unwrap();

        let (_work_id, m_id) = insert_fixture(
            &ing_pool,
            &marker,
            src_path.to_str().unwrap(),
            &original_hash,
        )
        .await;
        let pending_str = pending_path.to_str().unwrap();
        sqlx::query!(
            // Simulate a manifestation ingested before this cover was
            // embedded: has_embedded_cover = false, sidecar pending.
            "UPDATE manifestations SET cover_path = $1, has_embedded_cover = false WHERE id = $2",
            pending_str,
            m_id,
        )
        .execute(&ing_pool)
        .await
        .unwrap();

        let job_id = sqlx::query_scalar!(
            "INSERT INTO writeback_jobs (manifestation_id, reason) \
             VALUES ($1, 'cover') RETURNING id",
            m_id,
        )
        .fetch_one(&ing_pool)
        .await
        .unwrap();

        let outcome = run_fixture(
            &app_pool,
            &test_config(src_path.parent().unwrap()).0,
            job_id,
            src_path.parent().unwrap(),
        )
        .await
        .unwrap();
        assert!(
            matches!(outcome, RunOutcome::Success { .. }),
            "cover writeback should succeed: {outcome:?}"
        );

        let has_cover: Option<bool> = sqlx::query_scalar!(
            "SELECT has_embedded_cover FROM manifestations WHERE id = $1",
            m_id,
        )
        .fetch_one(&app_pool)
        .await
        .unwrap();
        assert_eq!(
            has_cover,
            Some(true),
            "has_embedded_cover must be refreshed from the post-writeback validation, \
             not left stale at its pre-writeback value"
        );
    }

    /// Collision branch of `resolve_collision`: if the rendered target
    /// already exists, the writeback lands at `<stem> (2).<ext>` instead.
    /// Guards against regressions where the suffix logic silently
    /// overwrites an unrelated pre-existing file at the rendered path.
    #[sqlx::test(migrations = "./migrations")]
    async fn bounded_writeback_run_once_rename_resolves_collision_with_suffix(pool: PgPool) {
        let app_pool = writeback_pool_for(&pool).await;
        let ing_pool = ingestion_pool_for(&pool).await;
        let marker = Uuid::new_v4().simple().to_string();

        let (lib_dir, src_path) = make_fixture_epub(&format!("Initial-{marker}"));
        let original_bytes = std::fs::read(&src_path).unwrap();
        let original_hash = initial_hex_sha256(&original_bytes);
        let library_root = lib_dir.path().to_str().unwrap().to_string();

        let (work_id, m_id) = insert_fixture(
            &ing_pool,
            &marker,
            src_path.to_str().unwrap(),
            &original_hash,
        )
        .await;

        let author_sort = format!("Author{marker}");
        let author_id = sqlx::query_scalar!(
            "INSERT INTO authors (name, sort_name) VALUES ($1, $1) RETURNING id",
            author_sort,
        )
        .fetch_one(&ing_pool)
        .await
        .unwrap();
        sqlx::query!(
            "INSERT INTO work_authors (work_id, author_id, role, position) \
             VALUES ($1, $2, 'author', 0)",
            work_id,
            author_id,
        )
        .execute(&ing_pool)
        .await
        .unwrap();

        let new_title = format!("Renamed{marker}");
        sqlx::query!(
            "UPDATE works SET title = $1 WHERE id = $2",
            new_title,
            work_id,
        )
        .execute(&ing_pool)
        .await
        .unwrap();

        // Pre-create the exact rendered target so `resolve_collision`
        // must add the " (2)" suffix.  Place it under the expected
        // `{Author}/{Title}.epub` layout.
        let pre_existing_dir = std::path::PathBuf::from(&library_root).join(&author_sort);
        std::fs::create_dir_all(&pre_existing_dir).unwrap();
        let pre_existing_path = pre_existing_dir.join(format!("{new_title}.epub"));
        std::fs::write(&pre_existing_path, b"pre-existing-sentinel").unwrap();

        let job_id = sqlx::query_scalar!(
            "INSERT INTO writeback_jobs (manifestation_id, reason) \
             VALUES ($1, 'metadata') RETURNING id",
            m_id,
        )
        .fetch_one(&ing_pool)
        .await
        .unwrap();

        let (mut cfg, _files) = test_config(lib_dir.path());
        cfg.library_path = library_root.parse().unwrap();
        let outcome = run_fixture(&app_pool, &cfg, job_id, src_path.parent().unwrap())
            .await
            .unwrap();
        assert!(
            matches!(outcome, RunOutcome::Success { .. }),
            "run_once should succeed: {outcome:?}"
        );

        // Pre-existing file must be untouched.
        assert!(
            pre_existing_path.exists(),
            "collision target must not be overwritten"
        );
        assert_eq!(
            std::fs::read(&pre_existing_path).unwrap(),
            b"pre-existing-sentinel",
            "collision target contents must not change"
        );

        // Writeback landed at `<Title> (2).epub` instead.
        let expected_collision_path = pre_existing_dir.join(format!("{new_title} (2).epub"));
        assert!(
            expected_collision_path.exists(),
            "collision-suffixed path must exist: {}",
            expected_collision_path.display()
        );

        let db_path =
            sqlx::query_scalar!("SELECT file_path FROM manifestations WHERE id = $1", m_id,)
                .fetch_one(&app_pool)
                .await
                .unwrap();
        assert_eq!(
            db_path,
            expected_collision_path
                .strip_prefix(&library_root)
                .unwrap()
                .to_str()
                .unwrap(),
            "DB file_path must record the collision-suffixed path"
        );
    }

    // ── Path-rename target rendering ────────────────────────────────────

    fn snap_with(title: Option<&str>, author: Option<&str>) -> JobSnapshot {
        JobSnapshot {
            manifestation_id: Uuid::nil(),
            reason: JobReason::Metadata,
            file_path: "fixture.epub".parse().unwrap(),
            library_id: LibraryId::from_uuid(Uuid::nil()),
            current_file_hash: String::new(),
            file_size_bytes: 0,
            relocation: None,
            format: ManifestationFormat::Epub,
            cover_path: None,
            title: title.map(std::string::ToString::to_string),
            subtitle: None,
            description: None,
            language: None,
            publisher: None,
            pub_date: None,
            isbn_10: None,
            isbn_13: None,
            primary_author: author.map(std::string::ToString::to_string),
        }
    }

    #[test]
    fn render_target_path_skipped_when_library_path_empty() {
        let snap = snap_with(Some("T"), Some("A"));
        let src = std::path::Path::new("/tmp/anything.epub");
        assert!(render_target_path(&snap, "", src).unwrap().is_none());
    }

    #[test]
    fn render_target_path_yields_author_title_layout() {
        let snap = snap_with(Some("Frankenstein"), Some("Shelley, Mary"));
        let src = std::path::Path::new("old/file.epub");
        let target = render_target_path(&snap, "/lib", src).unwrap().unwrap();
        assert_eq!(
            target,
            std::path::PathBuf::from("Shelley, Mary/Frankenstein.epub")
        );
    }

    #[test]
    fn render_target_path_returns_none_when_unchanged() {
        let snap = snap_with(Some("Frankenstein"), Some("Shelley, Mary"));
        let already = std::path::PathBuf::from("Shelley, Mary/Frankenstein.epub");
        assert!(
            render_target_path(&snap, "/lib", &already)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn render_target_path_falls_back_to_unknown_when_metadata_missing() {
        let snap = snap_with(None, None);
        let src = std::path::Path::new("/lib/orphan.epub");
        let target = render_target_path(&snap, "/lib", src).unwrap().unwrap();
        assert_eq!(target, std::path::PathBuf::from("Unknown/Unknown.epub"));
    }

    #[test]
    fn extract_opf_path_reads_full_path_attribute() {
        let xml = br#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles>
    <rootfile full-path="OEBPS/package.opf" media-type="application/oebps-package+xml"/>
  </rootfiles>
</container>"#;
        assert_eq!(extract_opf_path(xml).as_deref(), Some("OEBPS/package.opf"));
    }

    // ── resolve_opf_relative ────────────────────────────────────────────

    #[test]
    fn resolve_opf_relative_joins_with_opf_dir() {
        assert_eq!(
            resolve_opf_relative("OEBPS", "images/cover.png").unwrap(),
            "OEBPS/images/cover.png"
        );
    }

    #[test]
    fn resolve_opf_relative_no_op_when_opf_at_zip_root() {
        assert_eq!(
            resolve_opf_relative("", "content/cover.png").unwrap(),
            "content/cover.png"
        );
    }

    #[test]
    fn resolve_opf_relative_strips_leading_dot_slash() {
        assert_eq!(
            resolve_opf_relative("OEBPS", "./images/cover.png").unwrap(),
            "OEBPS/images/cover.png"
        );
    }

    #[test]
    fn resolve_opf_relative_rejects_parent_dir_segments() {
        // Leading ..
        assert!(resolve_opf_relative("OEBPS", "../secret").is_err());
        // Interior ..
        assert!(resolve_opf_relative("OEBPS", "images/../../secret").is_err());
        // Trailing ..
        assert!(resolve_opf_relative("OEBPS", "images/..").is_err());
        // Even with empty opf_dir
        assert!(resolve_opf_relative("", "../evil").is_err());
    }
}
