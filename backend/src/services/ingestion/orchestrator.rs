use std::path::{Path, PathBuf};

use sqlx::PgPool;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;
use walkdir::WalkDir;

use crate::config::{CleanupMode, Config};
use crate::models::ingestion_status::IngestionStatus;
use crate::models::manifestation_format::ManifestationFormat;
use crate::models::{ingestion_job, work};
use crate::services::epub::{self, ValidationOutcome};
use crate::services::ingestion::{cleanup, copier, format_filter, path_template, quarantine};
use crate::services::metadata;

/// Counts returned by a completed [`scan_once`] call.
#[derive(Debug)]
pub struct ScanResult {
    /// Files copied to the library and committed to the database.
    pub processed: usize,
    /// Files that errored during hashing, copying, validation, or DB insert.
    pub failed: usize,
    /// Files whose `SHA-256` hash or destination path already exists in `manifestations`
    /// (duplicate detection); not re-ingested.
    pub skipped: usize,
}

/// Start the filesystem watcher and process batches in a loop.
///
/// Spawns the `notify`-based watcher as a background task. Each time the watcher
/// delivers a batch of changed paths, a full [`scan_once`] is triggered. Exits
/// cleanly when `cancel` is triggered or when the watcher channel closes.
///
/// # Errors
///
/// This function does not return errors during normal operation: per-batch
/// `scan_once` failures are logged via `tracing::error!` and the loop
/// continues. The `Result` return type is preserved for parity with the
/// `tokio::spawn` callsite. The function reaches `Ok(())` when `cancel`
/// fires or when the watcher channel closes (the latter typically because
/// the spawned watcher task itself errored and exited).
pub async fn run_watcher(
    config: Config,
    pool: PgPool,
    cancel: CancellationToken,
) -> Result<(), anyhow::Error> {
    let (tx, mut rx) = mpsc::channel::<Vec<PathBuf>>(16);
    let ingestion_path = PathBuf::from(&config.ingestion_path);
    let watcher_cancel = cancel.clone();

    tokio::spawn(async move {
        if let Err(e) = super::watcher::watch(ingestion_path, tx, watcher_cancel).await {
            tracing::error!(error = %e, "filesystem watcher failed");
        }
    });

    loop {
        tokio::select! {
            () = cancel.cancelled() => {
                tracing::info!("orchestrator shutting down");
                break;
            }
            batch = rx.recv() => {
                if let Some(_paths) = batch {
                    // Watcher detected files — do a full scan of the ingestion dir.
                    // We scan rather than use the watcher's paths because walkdir
                    // gives us the complete picture (handles late-arriving files).
                    let result = scan_once(&config, &pool).await;
                    match result {
                        Ok(r) => {
                            tracing::info!(
                                processed = r.processed,
                                failed = r.failed,
                                skipped = r.skipped,
                                "batch complete"
                            );
                        }
                        Err(e) => {
                            tracing::error!(error = %e, "batch processing failed");
                        }
                    }
                } else {
                    tracing::warn!("watcher channel closed, stopping orchestrator");
                    break;
                }
            }
        }
    }

    Ok(())
}

/// Advisory lock ID for serializing ingestion scans. Prevents concurrent `scan_once`
/// calls (watcher + manual POST) from racing on duplicate checks and file copies.
const SCAN_ADVISORY_LOCK_ID: i64 = 0x5265_7665_0000_0004; // "Reve" + step 4

/// One-shot ingestion scan: walk the ingestion directory, filter by format priority,
/// copy to library, and track via `ingestion_jobs`.
///
/// Acquires a Postgres advisory lock (`pg_advisory_lock`) to serialize concurrent
/// scans. A second call that arrives while one is in progress will block at the
/// lock acquire until the first completes. The lock is session-scoped and released
/// when the connection returns to the pool.
///
/// # Errors
///
/// Returns `anyhow::Error` if the advisory lock cannot be acquired, if the
/// `spawn_blocking` tasks panic, or if a fatal database error occurs outside
/// the per-file error path. Per-file failures are counted in `ScanResult::failed`
/// and do not propagate as errors.
pub async fn scan_once(config: &Config, pool: &PgPool) -> Result<ScanResult, anyhow::Error> {
    // Serialize scans — only one can run at a time. Uses a session-level advisory
    // lock (released when the connection returns to the pool) rather than a
    // transaction-level lock, because the scan spans many transactions.
    let mut lock_conn = pool.acquire().await?;
    sqlx::query!("SELECT pg_advisory_lock($1)", SCAN_ADVISORY_LOCK_ID)
        .fetch_one(&mut *lock_conn)
        .await?;

    let result = scan_once_inner(config, pool).await;

    // Release the advisory lock explicitly (also released on connection drop).
    // Log a warning if the unlock fails — the lock will still release on connection drop.
    if let Err(e) = sqlx::query!("SELECT pg_advisory_unlock($1)", SCAN_ADVISORY_LOCK_ID)
        .fetch_one(&mut *lock_conn)
        .await
    {
        tracing::warn!(error = %e, "failed to explicitly release advisory scan lock; will release on connection drop");
    }

    result
}

#[allow(
    clippy::too_many_lines,
    reason = "scan_once_inner orchestrates the full ingestion pipeline: walk → dedup → copy → DB; the steps have data dependencies that make splitting into helpers awkward without additional Arc-sharing"
)]
async fn scan_once_inner(config: &Config, pool: &PgPool) -> Result<ScanResult, anyhow::Error> {
    let ingestion_path = PathBuf::from(&config.ingestion_path);
    let library_path = PathBuf::from(&config.library_path);
    let quarantine_path = PathBuf::from(&config.quarantine_path);
    let format_priority = config.format_priority.clone();

    // Walk the ingestion directory and collect all regular files.
    // follow_links(false) prevents symlink-based file exfiltration.
    // Wrapped in spawn_blocking because WalkDir performs synchronous I/O that
    // would otherwise block the tokio runtime thread.
    let all_source_files: Vec<PathBuf> = {
        let ingestion_path = ingestion_path.clone();
        tokio::task::spawn_blocking(move || {
            WalkDir::new(&ingestion_path)
                .follow_links(false)
                .into_iter()
                .filter_map(|entry| match entry {
                    Ok(e) => Some(e),
                    Err(e) => {
                        tracing::warn!(error = %e, "skipping inaccessible path during ingestion scan");
                        None
                    }
                })
                .filter(|e| e.file_type().is_file())
                .map(walkdir::DirEntry::into_path)
                .collect::<Vec<PathBuf>>()
        })
        .await?
    };

    if all_source_files.is_empty() {
        tracing::info!("ingestion directory empty, nothing to process");
        return Ok(ScanResult {
            processed: 0,
            failed: 0,
            skipped: 0,
        });
    }

    // Select highest-priority format per stem
    let selected = format_filter::select_by_priority(&all_source_files, &format_priority);
    if selected.is_empty() {
        tracing::info!(
            total_files = all_source_files.len(),
            "no files matched format priority"
        );
        return Ok(ScanResult {
            processed: 0,
            failed: 0,
            skipped: 0,
        });
    }

    let batch_id = Uuid::new_v4();
    let mut processed = 0usize;
    let mut failed = 0usize;
    let mut skipped = 0usize;

    for source in &selected {
        let source_str = source.display().to_string();
        let job = ingestion_job::create(pool, batch_id, &source_str).await?;
        ingestion_job::mark_running(pool, job.id).await?;

        match process_file(source, &library_path, &quarantine_path, pool).await {
            ProcessResult::Complete => {
                ingestion_job::mark_complete(pool, job.id).await?;
                processed += 1;
            }
            ProcessResult::Skipped => {
                ingestion_job::mark_skipped(pool, job.id).await?;
                skipped += 1;
            }
            ProcessResult::Failed(reason) => {
                ingestion_job::mark_failed(pool, job.id, &reason).await?;
                failed += 1;
            }
        }
    }

    // Cleanup only if ALL jobs succeeded or were skipped (none failed)
    if failed == 0 && config.cleanup_mode != CleanupMode::None {
        let cleanup_files = match config.cleanup_mode {
            CleanupMode::All => all_source_files.clone(),
            CleanupMode::Ingested => selected.clone(),
            CleanupMode::None => unreachable!(),
        };
        let ingestion_path_clone = config.ingestion_path.clone();
        tokio::task::spawn_blocking(move || {
            let ingestion_root = PathBuf::from(&ingestion_path_clone);
            match cleanup::cleanup_batch(&cleanup_files, &ingestion_root) {
                Ok(r) => {
                    tracing::info!(
                        files = r.removed_files,
                        dirs = r.removed_dirs,
                        "cleanup complete"
                    );
                }
                Err(e) => {
                    tracing::error!(error = %e, "cleanup failed");
                }
            }
        })
        .await?;
    } else if failed > 0 {
        tracing::warn!(
            failed,
            "skipping cleanup because {failed} job(s) failed — source files preserved"
        );
    }

    Ok(ScanResult {
        processed,
        failed,
        skipped,
    })
}

enum ProcessResult {
    Complete,
    Skipped,
    Failed(String),
}

#[allow(
    clippy::too_many_lines,
    reason = "process_file executes a sequential 8-step ingest pipeline (hash, dedup, copy, validate, rename, DB commit) where each step needs output from the previous; decomposing further requires passing a large context struct between helpers"
)]
async fn process_file(
    source: &Path,
    library_path: &Path,
    quarantine_path: &Path,
    pool: &PgPool,
) -> ProcessResult {
    let source = source.to_path_buf();
    let library_path = library_path.to_path_buf();
    let quarantine_path = quarantine_path.to_path_buf();

    // Step 1: Parse filename and hash source (in spawn_blocking)
    let prep_result = {
        let source = source.clone();
        let library_path = library_path.clone();
        tokio::task::spawn_blocking(move || {
            let filename = source
                .file_name()
                .and_then(|f| f.to_str())
                .unwrap_or("unknown");
            let vars = path_template::heuristic_vars_from_filename(filename);
            let relative = path_template::render(path_template::DEFAULT_TEMPLATE, &vars);

            let final_relative =
                match path_template::resolve_collision(&library_path.join(&relative)) {
                    Ok(full_path) => full_path
                        .strip_prefix(&library_path)
                        .unwrap_or(&relative)
                        .to_path_buf(),
                    Err(e) => return Err(format!("collision resolution failed: {e}")),
                };

            let source_hash = match copier::hash_file(&source) {
                Ok(h) => h,
                Err(e) => return Err(format!("failed to hash source: {e}")),
            };

            let dest_path_str = library_path.join(&final_relative).display().to_string();
            Ok((vars, final_relative, source_hash, dest_path_str))
        })
        .await
    };

    let (vars, final_relative, source_hash, dest_path_str) = match prep_result {
        Ok(Ok(tuple)) => tuple,
        Ok(Err(reason)) => {
            quarantine_async(&source, &quarantine_path, &reason).await;
            return ProcessResult::Failed(reason);
        }
        Err(e) => return ProcessResult::Failed(format!("spawn_blocking panicked: {e}")),
    };

    // Step 2: Duplicate check BEFORE copying
    let duplicate = sqlx::query_scalar!(
        "SELECT EXISTS(SELECT 1 FROM manifestations WHERE ingestion_file_hash = $1 OR file_path = $2) AS \"exists!\"",
        &source_hash,
        &dest_path_str,
    )
    .fetch_one(pool)
    .await;

    match duplicate {
        Ok(true) => return ProcessResult::Skipped,
        Ok(false) => {}
        Err(e) => {
            // Fail the job rather than proceeding without the safety check.
            // A transient DB error should not silently disable deduplication.
            return ProcessResult::Failed(format!("duplicate check query failed: {e}"));
        }
    }

    // Step 3: Copy with verification (in spawn_blocking).
    // Pass pre-computed source_hash so the copier only reads the source once (for
    // copying) and verifies the dest hash against it inline.
    let copy_result = {
        let source = source.clone();
        let library_path = library_path.clone();
        let final_relative = final_relative.clone();
        let hash_for_copy = source_hash.clone();
        tokio::task::spawn_blocking(move || {
            copier::copy_verified(&source, &library_path, &final_relative, &hash_for_copy)
        })
        .await
    };

    let copy_result = match copy_result {
        Ok(Ok(result)) => result,
        Ok(Err(e)) => {
            let reason = format!("copy failed: {e}");
            quarantine_async(&source, &quarantine_path, &reason).await;
            return ProcessResult::Failed(reason);
        }
        Err(e) => return ProcessResult::Failed(format!("spawn_blocking panicked: {e}")),
    };

    // Step 4: Determine manifestation_format from extension.
    // The format_filter::select_by_priority earlier guarantees ext parses to a
    // ManifestationFormat — this is the safety net for any code path that
    // bypasses that filter.
    let ext = vars.get("ext").cloned().unwrap_or_default();
    let Ok(format) = ext.parse::<ManifestationFormat>() else {
        let dest = dest_path_str.clone();
        if let Err(e) = tokio::task::spawn_blocking(move || {
            if let Err(e) = std::fs::remove_file(&dest) {
                tracing::warn!(path = %dest, error = %e, "failed to remove orphaned library file after format check");
            }
        })
        .await
        {
            tracing::warn!(error = %e, "cleanup spawn_blocking panicked after format check");
        }
        return ProcessResult::Failed(format!("unsupported format: {ext}"));
    };

    // Step 4.5: EPUB structural validation and auto-repair.
    // Only applies to EPUB files; other formats pass through as 'valid'.
    let (validation_status_str, accessibility_metadata, opf_data): (
        &'static str,
        Option<serde_json::Value>,
        Option<epub::opf_layer::OpfData>,
    ) = if ext == "epub" {
        let lib_file = library_path.join(&final_relative);
        let validation = {
            let lib_file = lib_file.clone();
            tokio::task::spawn_blocking(move || epub::validate_and_repair(&lib_file)).await
        };

        match validation {
            Ok(Ok(report)) => {
                tracing::info!(
                    path = %lib_file.display(),
                    outcome = ?report.outcome,
                    issues = report.issues.len(),
                    "epub validation complete"
                );
                let a11y = report.accessibility_metadata;
                let opf = report.opf_data;
                let issues = report.issues;
                match report.outcome {
                    ValidationOutcome::Quarantined => {
                        let lib_file_str = lib_file.display().to_string();
                        if let Err(e) = tokio::task::spawn_blocking(move || {
                            if let Err(e) = std::fs::remove_file(&lib_file_str) {
                                tracing::warn!(
                                    path = %lib_file_str,
                                    error = %e,
                                    "failed to remove library file for quarantined EPUB"
                                );
                            }
                        })
                        .await
                        {
                            tracing::warn!(error = %e, "cleanup spawn_blocking panicked for quarantined EPUB removal");
                        }
                        let reason = issues
                            .iter()
                            .map(|i| format!("{:?}", i.kind))
                            .collect::<Vec<_>>()
                            .join("; ");
                        quarantine_async(&source, &quarantine_path, &reason).await;
                        return ProcessResult::Failed(format!("EPUB quarantined: {reason}"));
                    }
                    ValidationOutcome::Clean => ("valid", a11y, opf),
                    ValidationOutcome::Repaired => ("repaired", a11y, opf),
                    ValidationOutcome::Degraded => ("degraded", a11y, opf),
                }
            }
            Ok(Err(e)) => {
                tracing::warn!(error = %e, "epub validation error; proceeding as degraded");
                ("degraded", None, None)
            }
            Err(e) => return ProcessResult::Failed(format!("spawn_blocking panicked: {e}")),
        }
    } else {
        ("valid", None, None)
    };

    // Step 5: Extract metadata and create work + manifestation
    let extracted = opf_data.as_ref().map(metadata::extractor::extract);

    // Compute metadata-based path if extraction succeeded
    let final_path_str = if let Some(ref meta) = extracted {
        if meta.title.is_some() || !meta.creators.is_empty() {
            let mut meta_vars = vars.clone();
            if let Some(ref t) = meta.title {
                meta_vars.insert("Title".into(), t.clone());
            }
            if let Some(first) = meta.creators.first() {
                meta_vars.insert("Author".into(), first.sort_name.clone());
            }
            let new_relative = path_template::render(path_template::DEFAULT_TEMPLATE, &meta_vars);
            let new_full = library_path.join(&new_relative);

            // Attempt rename if path changed
            if new_full.display().to_string() == dest_path_str {
                dest_path_str.clone()
            } else {
                let old_path = dest_path_str.clone();
                let new_full_clone = new_full.clone();
                let rename_result = tokio::task::spawn_blocking(move || {
                    // Resolve collision on new path
                    let resolved = path_template::resolve_collision(&new_full_clone)?;
                    if let Some(parent) = resolved.parent() {
                        std::fs::create_dir_all(parent)?;
                    }
                    std::fs::rename(&old_path, &resolved)?;
                    // Try to clean up empty parent dirs of old path
                    if let Some(old_parent) = Path::new(&old_path).parent() {
                        // Best-effort: only succeeds if the directory is empty.
                        // Failure (non-empty dir, permissions) is expected and ignored.
                        if let Err(e) = std::fs::remove_dir(old_parent) {
                            tracing::debug!(path = %old_parent.display(), error = %e, "could not remove old parent dir (non-empty or permissions); expected");
                        }
                    }
                    Ok::<String, std::io::Error>(resolved.display().to_string())
                })
                .await;

                match rename_result {
                    Ok(Ok(new_path)) => {
                        tracing::info!(
                            old_path = %dest_path_str,
                            new_path = %new_path,
                            "renamed file to metadata-based path"
                        );
                        new_path
                    }
                    Ok(Err(e)) => {
                        tracing::warn!(
                            error = %e,
                            old_path = %dest_path_str,
                            "metadata rename failed; keeping heuristic path"
                        );
                        dest_path_str.clone()
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "rename spawn_blocking panicked");
                        dest_path_str.clone()
                    }
                }
            }
        } else {
            dest_path_str.clone()
        }
    } else {
        dest_path_str.clone()
    };

    // DB section — single transaction so the ingest invariant holds:
    // every non-NULL canonical field on the manifestation has a corresponding
    // metadata_versions row pointed to by its *_version_id column.
    let db_outcome = commit_ingest(
        pool,
        &extracted,
        &vars,
        &final_path_str,
        &copy_result,
        format,
        validation_status_str,
        &accessibility_metadata,
    )
    .await;

    let (work_id, manifestation_id) = match db_outcome {
        Ok(pair) => pair,
        Err(e) => {
            tracing::error!(error = %e, "ingest DB commit failed");
            let dest = final_path_str.clone();
            if let Err(e) = tokio::task::spawn_blocking(move || {
                if let Err(rm_err) = std::fs::remove_file(&dest) {
                    tracing::warn!(
                        path = %dest,
                        error = %rm_err,
                        "failed to remove orphaned library file after DB error"
                    );
                }
            })
            .await
            {
                tracing::warn!(error = %e, "cleanup spawn_blocking panicked after DB error");
            }
            return ProcessResult::Failed(format!("DB insert failed: {e}"));
        }
    };

    if let Some(ref meta) = extracted {
        tracing::info!(
            title = meta.title.as_deref().unwrap_or("unknown"),
            authors = meta.creators.len(),
            confidence = meta.confidence,
            has_isbn = meta.isbn.is_some(),
            work_id = %work_id,
            manifestation_id = %manifestation_id,
            "metadata extraction complete"
        );
    } else {
        tracing::info!(
            work_id = %work_id,
            manifestation_id = %manifestation_id,
            "ingest complete without OPF (heuristic-fallback journal row written)"
        );
    }

    ProcessResult::Complete
}

/// Run the ingest DB sequence atomically and return `(work_id, manifestation_id)`.
///
/// Sequence:
///   1. match work (if OPF has enough signal)
///   2. create stub work if no match
///   3. insert manifestation with NULL canonical + NULL pointers
///   4. write drafts (OPF drafts, or synthetic heuristic-title draft at 0.2)
///   5. upgrade stub work with pointers if newly created
///   6. UPDATE manifestation canonical values + pointer columns from draft IDs
#[allow(clippy::too_many_arguments)]
#[allow(
    clippy::ref_option,
    reason = "commit_ingest is called with &extracted and &accessibility_metadata from a site that holds owned Options; changing to Option<&T> would require .as_ref() at every call site with no readability benefit"
)]
async fn commit_ingest(
    pool: &PgPool,
    extracted: &Option<crate::services::metadata::extractor::ExtractedMetadata>,
    vars: &std::collections::HashMap<String, String>,
    final_path_str: &str,
    copy_result: &copier::CopyResult,
    format: ManifestationFormat,
    validation_status_str: &str,
    accessibility_metadata: &Option<serde_json::Value>,
) -> Result<(Uuid, Uuid), sqlx::Error> {
    use crate::services::metadata::draft;
    use crate::services::metadata::extractor::ExtractedMetadata;

    let mut tx = pool.begin().await?;

    // 1. Try to match an existing work (only when OPF gave us signal).
    let matched = match extracted.as_ref() {
        Some(meta) => work::match_existing(&mut tx, meta).await?,
        None => None,
    };

    let (work_id, was_created) = match matched {
        Some(id) => (id, false),
        None => (work::create_stub(&mut tx).await?, true),
    };

    // 2. Insert manifestation with NULL canonical + NULL pointers.
    //    `format` and `ingestion_status` are bound as their typed Rust enums
    //    (sqlx::Type impls). `validation_status` has no Rust counterpart and
    //    is bound as text + cast in SQL (`($N::text)::validation_status`).
    let file_size = copy_result.file_size.cast_signed();
    let ingestion_status = IngestionStatus::Complete;
    let manifestation_id = sqlx::query_scalar!(
        "INSERT INTO manifestations \
             (work_id, format, file_path, ingestion_file_hash, current_file_hash, \
              file_size_bytes, ingestion_status, validation_status, accessibility_metadata) \
         VALUES ($1, $2, $3, $4, $4, $5, $6, ($7::text)::validation_status, $8) \
         RETURNING id",
        work_id,
        format as ManifestationFormat,
        final_path_str,
        &copy_result.sha256,
        file_size,
        ingestion_status as IngestionStatus,
        validation_status_str,
        accessibility_metadata.as_ref(),
    )
    .fetch_one(&mut *tx)
    .await?;

    // 3. Write drafts — OPF metadata when available, heuristic fallback otherwise.
    //    The heuristic row gives the canonical title_version_id pointer even
    //    when no OPF metadata exists, preserving the ingest invariant.
    let metadata_for_drafts: ExtractedMetadata = extracted.as_ref().map_or_else(
        || {
            let title = vars
                .get("Title")
                .cloned()
                .unwrap_or_else(|| "Unknown".into());
            ExtractedMetadata {
                title: Some(title.clone()),
                sort_title: Some(title),
                description: None,
                language: None,
                creators: Vec::new(),
                publisher: None,
                pub_date: None,
                isbn: None,
                subjects: Vec::new(),
                series: None,
                inversion: None,
                confidence: 0.2,
            }
        },
        ExtractedMetadata::clone,
    );
    let draft_ids = draft::write_drafts(&mut tx, manifestation_id, &metadata_for_drafts).await?;

    // 4. Upgrade stub work with real values + pointers (create path only).
    if was_created {
        work::upgrade_stub(&mut tx, work_id, &metadata_for_drafts, &draft_ids).await?;
    }

    // 5. Populate manifestation canonical columns + *_version_id pointers
    //    from OPF extraction (not the heuristic row — only real OPF values
    //    become canonical ISBN/publisher/pub_date).
    let (isbn_10, isbn_13) = extracted
        .as_ref()
        .and_then(|m| m.isbn.as_ref())
        .map_or((None, None), |i| (i.isbn_10.clone(), i.isbn_13.clone()));
    let publisher = extracted.as_ref().and_then(|m| m.publisher.clone());
    let pub_date = extracted.as_ref().and_then(|m| m.pub_date);

    sqlx::query!(
        "UPDATE manifestations SET \
            isbn_10 = $1, isbn_13 = $2, publisher = $3, pub_date = $4, \
            isbn_10_version_id = $5, isbn_13_version_id = $6, \
            publisher_version_id = $7, pub_date_version_id = $8 \
         WHERE id = $9",
        isbn_10.as_deref(),
        isbn_13.as_deref(),
        publisher.as_deref(),
        pub_date,
        draft_ids.get("isbn_10").copied(),
        draft_ids.get("isbn_13").copied(),
        draft_ids.get("publisher").copied(),
        draft_ids.get("pub_date").copied(),
        manifestation_id,
    )
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok((work_id, manifestation_id))
}

async fn quarantine_async(source: &Path, quarantine_path: &Path, reason: &str) {
    let source = source.to_path_buf();
    let qpath = quarantine_path.to_path_buf();
    let reason = reason.to_string();
    if let Err(e) = tokio::task::spawn_blocking(move || {
        if let Err(e) = quarantine::quarantine_file(&source, &qpath, &reason) {
            tracing::error!(error = %e, "quarantine failed");
        }
    })
    .await
    {
        tracing::error!(error = %e, "quarantine spawn_blocking panicked");
    }
}

#[cfg(test)]
#[allow(
    clippy::items_after_statements,
    reason = "test code: local struct definitions inside test functions are idiomatic for sqlx::FromRow test helpers"
)]
mod tests {
    use super::*;
    use crate::config::CleanupMode;
    use crate::test_support::db::ingestion_pool_for;

    fn test_config_for(ingestion: &str, library: &str, quarantine: &str) -> Config {
        Config {
            port: 3000,
            database_url: String::new(),
            library_path: library.to_string(),
            ingestion_path: ingestion.to_string(),
            quarantine_path: quarantine.to_string(),
            log_level: "info".into(),
            db_max_connections: 5,
            oidc_issuer_url: String::new(),
            oidc_client_id: String::new(),
            oidc_client_secret: String::new(),
            oidc_redirect_uri: String::new(),
            migration_database_url: String::new(),
            ingestion_database_url: String::new(),
            format_priority: vec![ManifestationFormat::Epub, ManifestationFormat::Pdf],
            // Preserve source files during tests so we can run multiple scans
            cleanup_mode: CleanupMode::None,
            enrichment: crate::config::EnrichmentConfig {
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
            cover: crate::config::CoverConfig {
                max_bytes: 10_485_760,
                download_timeout_secs: 30,
                min_long_edge_px: 1000,
                redirect_limit: 3,
            },
            writeback: crate::config::WritebackConfig {
                enabled: false,
                concurrency: 1,
                poll_idle_secs: 5,
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
            openlibrary_base_url: "https://openlibrary.org".into(),
            googlebooks_base_url: "https://www.googleapis.com/books/v1".into(),
            googlebooks_api_key: None,
            hardcover_base_url: "https://api.hardcover.app/v1/graphql".into(),
            hardcover_api_token: None,
            operator_contact: None,
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn scan_once_empty_dir_returns_zero(pool: PgPool) {
        let pool = ingestion_pool_for(&pool).await;
        let ingestion = tempfile::tempdir().unwrap();
        let library = tempfile::tempdir().unwrap();
        let quarantine = tempfile::tempdir().unwrap();
        let config = test_config_for(
            ingestion.path().to_str().unwrap(),
            library.path().to_str().unwrap(),
            quarantine.path().to_str().unwrap(),
        );
        let result = scan_once(&config, &pool).await.unwrap();
        assert_eq!(result.processed, 0);
        assert_eq!(result.failed, 0);
        assert_eq!(result.skipped, 0);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn scan_once_processes_pdf_end_to_end(pool: PgPool) {
        let pool = ingestion_pool_for(&pool).await;
        let ingestion = tempfile::tempdir().unwrap();
        let library = tempfile::tempdir().unwrap();
        let quarantine = tempfile::tempdir().unwrap();

        let source = ingestion.path().join("Tolkien - The Hobbit.pdf");
        std::fs::write(&source, b"fake pdf bytes for scan_once test").unwrap();

        let config = test_config_for(
            ingestion.path().to_str().unwrap(),
            library.path().to_str().unwrap(),
            quarantine.path().to_str().unwrap(),
        );
        let result = scan_once(&config, &pool).await.unwrap();
        assert_eq!(result.processed, 1, "expected 1 processed");
        assert_eq!(result.failed, 0);
        assert_eq!(result.skipped, 0);

        // File should exist in the library under Author/Title.ext
        let dest = library.path().join("Tolkien/The Hobbit.pdf");
        assert!(dest.exists(), "expected file at {}", dest.display());
        assert_eq!(
            std::fs::read(&dest).unwrap(),
            b"fake pdf bytes for scan_once test"
        );

        // Manifestation row should exist
        let dest_str = dest.to_str().unwrap();
        let count = sqlx::query_scalar!(
            "SELECT COUNT(*) AS \"count!\" FROM manifestations WHERE file_path = $1",
            dest_str,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(count, 1, "expected 1 manifestation row");
    }

    /// Build a minimal valid EPUB ZIP in memory.
    ///
    /// Structure: mimetype (stored) + META-INF/container.xml + OEBPS/content.opf.
    /// All layers pass cleanly: valid ZIP, valid container, valid OPF with empty
    /// manifest and spine, no XHTML to check, no cover declared.
    fn make_minimal_epub() -> Vec<u8> {
        use std::io::Write as _;
        use zip::write::{ExtendedFileOptions, FileOptions};

        let buf = std::io::Cursor::new(Vec::new());
        let mut w = zip::ZipWriter::new(buf);

        // mimetype must be first and stored (not deflated) per EPUB spec
        let stored: FileOptions<ExtendedFileOptions> =
            FileOptions::default().compression_method(zip::CompressionMethod::Stored);
        w.start_file("mimetype", stored).unwrap();
        w.write_all(b"application/epub+zip").unwrap();

        let default: FileOptions<ExtendedFileOptions> = FileOptions::default();

        w.start_file("META-INF/container.xml", default.clone())
            .unwrap();
        w.write_all(
            br#"<?xml version="1.0" encoding="UTF-8"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles>
    <rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/>
  </rootfiles>
</container>"#,
        )
        .unwrap();

        w.start_file("OEBPS/content.opf", default).unwrap();
        w.write_all(
            br#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0">
  <metadata/>
  <manifest/>
  <spine/>
</package>"#,
        )
        .unwrap();

        w.finish().unwrap().into_inner()
    }

    /// Build an EPUB with Dublin Core metadata for integration testing.
    fn make_metadata_epub() -> Vec<u8> {
        use std::io::Write as _;
        use zip::write::{ExtendedFileOptions, FileOptions};

        let buf = std::io::Cursor::new(Vec::new());
        let mut w = zip::ZipWriter::new(buf);

        let stored: FileOptions<ExtendedFileOptions> =
            FileOptions::default().compression_method(zip::CompressionMethod::Stored);
        w.start_file("mimetype", stored).unwrap();
        w.write_all(b"application/epub+zip").unwrap();

        let default: FileOptions<ExtendedFileOptions> = FileOptions::default();

        w.start_file("META-INF/container.xml", default.clone())
            .unwrap();
        w.write_all(
            br#"<?xml version="1.0" encoding="UTF-8"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles>
    <rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/>
  </rootfiles>
</container>"#,
        )
        .unwrap();

        w.start_file("OEBPS/content.opf", default).unwrap();
        w.write_all(
            br#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" xmlns:dc="http://purl.org/dc/elements/1.1/" version="3.0">
  <metadata>
    <dc:title>The Integration Test</dc:title>
    <dc:creator opf:role="aut">Test McAuthor</dc:creator>
    <dc:language>en</dc:language>
    <dc:identifier>urn:isbn:9780306406157</dc:identifier>
    <dc:publisher>Test Press</dc:publisher>
    <dc:description>A book for testing metadata extraction</dc:description>
    <meta name="calibre:series" content="Test Series"/>
    <meta name="calibre:series_index" content="1"/>
  </metadata>
  <manifest/>
  <spine/>
</package>"#,
        )
        .unwrap();

        w.finish().unwrap().into_inner()
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn scan_once_extracts_metadata_from_epub(pool: PgPool) {
        let pool = ingestion_pool_for(&pool).await;
        let ingestion = tempfile::tempdir().unwrap();
        let library = tempfile::tempdir().unwrap();
        let quarantine = tempfile::tempdir().unwrap();

        // Use a filename that differs from the OPF metadata to test rename
        let source = ingestion.path().join("Unknown - somefile.epub");
        std::fs::write(&source, make_metadata_epub()).unwrap();

        let config = test_config_for(
            ingestion.path().to_str().unwrap(),
            library.path().to_str().unwrap(),
            quarantine.path().to_str().unwrap(),
        );
        let result = scan_once(&config, &pool).await.unwrap();
        assert_eq!(result.processed, 1);
        assert_eq!(result.failed, 0);

        // File should be renamed to metadata-based path: "McAuthor, Test/The Integration Test.epub"
        let dest = library
            .path()
            .join("McAuthor, Test/The Integration Test.epub");
        assert!(
            dest.exists(),
            "expected metadata-renamed file at {}",
            dest.display()
        );

        // Verify work title
        let dest_str = dest.to_str().unwrap();
        let title = sqlx::query_scalar!(
            "SELECT w.title FROM works w \
             JOIN manifestations m ON m.work_id = w.id \
             WHERE m.file_path = $1",
            dest_str,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(title, "The Integration Test");

        // Verify author was created and linked
        let author_name = sqlx::query_scalar!(
            "SELECT a.name FROM authors a \
             JOIN work_authors wa ON wa.author_id = a.id \
             JOIN manifestations m ON m.work_id = wa.work_id \
             WHERE m.file_path = $1",
            dest_str,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(author_name, "Test McAuthor");

        // Verify ISBN was populated on the manifestation
        let isbn = sqlx::query_scalar!(
            "SELECT isbn_13 FROM manifestations WHERE file_path = $1",
            dest_str,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(isbn.as_deref(), Some("9780306406157"));

        // Verify metadata_versions drafts were created
        let draft_count = sqlx::query_scalar!(
            "SELECT COUNT(*) AS \"count!\" FROM metadata_versions mv \
             JOIN manifestations m ON m.id = mv.manifestation_id \
             WHERE m.file_path = $1 AND mv.source = 'opf' AND mv.status::text = 'pending'",
            dest_str,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(
            draft_count >= 5,
            "expected at least 5 draft rows, got {draft_count}"
        );

        // Verify series was created
        let series_count = sqlx::query_scalar!(
            "SELECT COUNT(*) AS \"count!\" FROM series_works sw \
             JOIN manifestations m ON m.work_id = sw.work_id \
             WHERE m.file_path = $1",
            dest_str,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(series_count, 1, "expected series link");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn scan_once_processes_epub_end_to_end(pool: PgPool) {
        // P1: exercise the EPUB validation path end-to-end, verifying that a valid
        // EPUB gets validation_status='valid' in the manifestation row.
        let pool = ingestion_pool_for(&pool).await;
        let ingestion = tempfile::tempdir().unwrap();
        let library = tempfile::tempdir().unwrap();
        let quarantine = tempfile::tempdir().unwrap();

        let source = ingestion.path().join("Tolkien - The Hobbit.epub");
        std::fs::write(&source, make_minimal_epub()).unwrap();

        let config = test_config_for(
            ingestion.path().to_str().unwrap(),
            library.path().to_str().unwrap(),
            quarantine.path().to_str().unwrap(),
        );
        let result = scan_once(&config, &pool).await.unwrap();
        assert_eq!(result.processed, 1, "expected 1 processed");
        assert_eq!(result.failed, 0);
        assert_eq!(result.skipped, 0);

        let dest = library.path().join("Tolkien/The Hobbit.epub");
        assert!(dest.exists(), "expected file at {}", dest.display());

        // validation_status must be 'valid' for a clean EPUB
        let dest_str = dest.to_str().unwrap();
        let status = sqlx::query_scalar!(
            "SELECT validation_status::text AS \"validation_status!\" FROM manifestations WHERE file_path = $1",
            dest_str,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(status, "valid", "expected validation_status=valid");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn scan_once_quarantines_corrupt_epub(pool: PgPool) {
        // P2: a corrupt EPUB (not a valid ZIP) must be quarantined — the source
        // gets a quarantine sidecar, the library copy is removed, and failed=1.
        let pool = ingestion_pool_for(&pool).await;
        let ingestion = tempfile::tempdir().unwrap();
        let library = tempfile::tempdir().unwrap();
        let quarantine = tempfile::tempdir().unwrap();

        let source = ingestion.path().join("Bad - Corrupt Book.epub");
        std::fs::write(&source, b"this is not a zip file").unwrap();

        let config = test_config_for(
            ingestion.path().to_str().unwrap(),
            library.path().to_str().unwrap(),
            quarantine.path().to_str().unwrap(),
        );
        let result = scan_once(&config, &pool).await.unwrap();
        assert_eq!(result.failed, 1, "expected 1 failed (quarantined)");
        assert_eq!(result.processed, 0);

        // Quarantine directory must contain a sidecar file for the corrupt EPUB
        let quarantine_entries: Vec<_> = std::fs::read_dir(quarantine.path())
            .unwrap()
            .filter_map(std::result::Result::ok)
            .collect();
        assert!(
            !quarantine_entries.is_empty(),
            "expected a quarantine sidecar file, found none"
        );

        // Library must NOT contain the corrupt file
        let dest = library.path().join("Bad/Corrupt Book.epub");
        assert!(!dest.exists(), "corrupt EPUB must not remain in library");

        // No manifestation row must have been written
        let dest_str = dest.to_str().unwrap();
        let count = sqlx::query_scalar!(
            "SELECT COUNT(*) AS \"count!\" FROM manifestations WHERE file_path = $1",
            dest_str,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            count, 0,
            "no manifestation row should exist for quarantined EPUB"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn scan_once_skips_duplicate_on_second_run(pool: PgPool) {
        let pool = ingestion_pool_for(&pool).await;
        let ingestion = tempfile::tempdir().unwrap();
        let library = tempfile::tempdir().unwrap();
        let quarantine = tempfile::tempdir().unwrap();

        // Unique content to avoid collisions with other test data
        let unique_content = format!("dedup-test-{}", uuid::Uuid::new_v4());
        let source = ingestion.path().join("Author - Book.pdf");
        std::fs::write(&source, unique_content.as_bytes()).unwrap();

        let config = test_config_for(
            ingestion.path().to_str().unwrap(),
            library.path().to_str().unwrap(),
            quarantine.path().to_str().unwrap(),
        );

        // First scan: should process the file
        let r1 = scan_once(&config, &pool).await.unwrap();
        assert_eq!(r1.processed, 1, "first scan: expected processed=1");
        assert_eq!(r1.failed, 0);

        // Second scan: same file still in ingestion dir, same hash → skip
        let r2 = scan_once(&config, &pool).await.unwrap();
        assert_eq!(r2.skipped, 1, "second scan: expected skipped=1");
        assert_eq!(r2.processed, 0);
    }

    // ── Task 30: ingest-invariant DB tests ────────────────────────────────

    /// Every non-NULL canonical field set by ingestion must have a matching
    /// `*_version_id` pointer referencing a real `metadata_versions` row with
    /// `source='opf'`.  Without this invariant, `metadata_versions` is optional
    /// instead of authoritative.
    #[sqlx::test(migrations = "./migrations")]
    async fn ingest_sets_version_pointers_for_all_canonical_fields(pool: PgPool) {
        let pool = ingestion_pool_for(&pool).await;
        let ingestion = tempfile::tempdir().unwrap();
        let library = tempfile::tempdir().unwrap();
        let quarantine = tempfile::tempdir().unwrap();

        let marker = uuid::Uuid::new_v4().simple().to_string();
        let source = ingestion.path().join(format!("invariant-{marker}.epub"));
        std::fs::write(&source, make_metadata_epub()).unwrap();

        let config = test_config_for(
            ingestion.path().to_str().unwrap(),
            library.path().to_str().unwrap(),
            quarantine.path().to_str().unwrap(),
        );
        let result = scan_once(&config, &pool).await.unwrap();
        assert_eq!(result.processed, 1, "expected 1 processed");

        let dest = library
            .path()
            .join("McAuthor, Test/The Integration Test.epub");
        assert!(dest.exists(), "expected file at {}", dest.display());

        // Pull every canonical field + its pointer in one query.
        // `w.title` is NOT NULL in the schema; force it to nullable via
        // `AS "title?"` so the field type stays `Option<String>` matching
        // the truly-nullable peers. The uniform `if x.is_some()` asserts
        // below then handle every canonical/pointer pair the same way.
        struct Invariant {
            title: Option<String>,
            title_version_id: Option<uuid::Uuid>,
            language: Option<String>,
            language_version_id: Option<uuid::Uuid>,
            publisher: Option<String>,
            publisher_version_id: Option<uuid::Uuid>,
            pub_date_version_id: Option<uuid::Uuid>,
            isbn_13: Option<String>,
            isbn_13_version_id: Option<uuid::Uuid>,
        }
        let dest_str = dest.to_str().unwrap();
        let inv = sqlx::query_as!(
            Invariant,
            "SELECT w.title AS \"title?\", w.title_version_id, \
                    w.language, w.language_version_id, \
                    m.publisher, m.publisher_version_id, \
                    m.pub_date_version_id, \
                    m.isbn_13, m.isbn_13_version_id \
             FROM manifestations m \
             JOIN works w ON w.id = m.work_id \
             WHERE m.file_path = $1",
            dest_str,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let Invariant {
            title,
            title_version_id: title_ptr,
            language,
            language_version_id: language_ptr,
            publisher,
            publisher_version_id: publisher_ptr,
            pub_date_version_id: pub_date_ptr,
            isbn_13,
            isbn_13_version_id: isbn_13_ptr,
        } = inv;

        // Invariant: non-NULL canonical value ⇒ non-NULL pointer.
        if title.is_some() {
            assert!(title_ptr.is_some(), "title set but title_version_id NULL");
        }
        if language.is_some() {
            assert!(
                language_ptr.is_some(),
                "language set but language_version_id NULL"
            );
        }
        if publisher.is_some() {
            assert!(
                publisher_ptr.is_some(),
                "publisher set but publisher_version_id NULL"
            );
        }
        if isbn_13.is_some() {
            assert!(
                isbn_13_ptr.is_some(),
                "isbn_13 set but isbn_13_version_id NULL"
            );
        }

        // Every non-NULL pointer must reference a real source='opf' row.
        for pointer in [
            title_ptr,
            language_ptr,
            publisher_ptr,
            pub_date_ptr,
            isbn_13_ptr,
        ]
        .into_iter()
        .flatten()
        {
            let source_for_ptr = sqlx::query_scalar!(
                "SELECT source FROM metadata_versions WHERE id = $1",
                pointer,
            )
            .fetch_one(&pool)
            .await
            .unwrap_or_else(|e| {
                panic!("pointer {pointer} did not resolve to a metadata_versions row: {e}")
            });
            assert_eq!(
                source_for_ptr, "opf",
                "pointer {pointer} resolved to source '{source_for_ptr}', expected 'opf'"
            );
        }
    }

    /// When ingestion cannot extract OPF (e.g. for a non-EPUB file), a
    /// heuristic-fallback row is written to `metadata_versions` with
    /// `source='opf'`, `field_name='title'`, `confidence_score=0.2` and the
    /// work's `title_version_id` pointer references it.
    #[sqlx::test(migrations = "./migrations")]
    async fn ingest_without_opf_writes_heuristic_title_journal(pool: PgPool) {
        let pool = ingestion_pool_for(&pool).await;
        let ingestion = tempfile::tempdir().unwrap();
        let library = tempfile::tempdir().unwrap();
        let quarantine = tempfile::tempdir().unwrap();

        // PDF has no OPF extraction path → heuristic fallback engages.
        let marker = uuid::Uuid::new_v4().simple().to_string();
        let source = ingestion
            .path()
            .join(format!("Heuristic Author - Heuristic Title {marker}.pdf"));
        std::fs::write(&source, format!("heuristic-pdf-{marker}")).unwrap();

        let config = test_config_for(
            ingestion.path().to_str().unwrap(),
            library.path().to_str().unwrap(),
            quarantine.path().to_str().unwrap(),
        );
        let result = scan_once(&config, &pool).await.unwrap();
        assert_eq!(result.processed, 1, "expected 1 processed");

        let dest = library
            .path()
            .join(format!("Heuristic Author/Heuristic Title {marker}.pdf"));
        assert!(dest.exists(), "expected file at {}", dest.display());

        // The work should have its title_version_id pointing at the heuristic
        // row, which must have source='opf', field_name='title', confidence=0.2.
        let dest_str = dest.to_str().unwrap();
        let row = sqlx::query!(
            "SELECT w.title_version_id, w.title FROM works w \
             JOIN manifestations m ON m.work_id = w.id \
             WHERE m.file_path = $1",
            dest_str,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(
            row.title.contains("Heuristic Title"),
            "title should include heuristic value, got '{}'",
            row.title,
        );
        let ptr = row
            .title_version_id
            .expect("title_version_id must be wired for heuristic row");

        let row = sqlx::query!(
            "SELECT source, field_name, confidence_score \
             FROM metadata_versions WHERE id = $1",
            ptr,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(row.source, "opf");
        assert_eq!(row.field_name, "title");
        let score = row.confidence_score;
        assert!(
            (score - 0.2).abs() < 1e-4,
            "heuristic confidence should be ~0.2, got {score}"
        );
    }

    /// `work_authors.source_version_id` must be wired to the `creators`
    /// journal row so authors on the work trace back to their draft.
    #[sqlx::test(migrations = "./migrations")]
    async fn ingest_sets_work_authors_source_version_id(pool: PgPool) {
        let pool = ingestion_pool_for(&pool).await;
        let ingestion = tempfile::tempdir().unwrap();
        let library = tempfile::tempdir().unwrap();
        let quarantine = tempfile::tempdir().unwrap();

        let marker = uuid::Uuid::new_v4().simple().to_string();
        let source = ingestion.path().join(format!("authors-{marker}.epub"));
        std::fs::write(&source, make_metadata_epub()).unwrap();

        let config = test_config_for(
            ingestion.path().to_str().unwrap(),
            library.path().to_str().unwrap(),
            quarantine.path().to_str().unwrap(),
        );
        let result = scan_once(&config, &pool).await.unwrap();
        assert_eq!(result.processed, 1, "expected 1 processed");

        let dest = library
            .path()
            .join("McAuthor, Test/The Integration Test.epub");

        // Every work_author row for this work must carry a source_version_id
        // pointing at a metadata_versions row with field_name='creators'.
        let dest_str = dest.to_str().unwrap();
        let rows = sqlx::query!(
            "SELECT wa.author_id, wa.source_version_id \
             FROM work_authors wa \
             JOIN manifestations m ON m.work_id = wa.work_id \
             WHERE m.file_path = $1",
            dest_str,
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert!(!rows.is_empty(), "expected at least one work_author row");

        for row in rows {
            let ptr = row.source_version_id.unwrap_or_else(|| {
                panic!(
                    "work_authors.source_version_id NULL for author {}",
                    row.author_id
                )
            });
            let field_name = sqlx::query_scalar!(
                "SELECT field_name FROM metadata_versions WHERE id = $1",
                ptr,
            )
            .fetch_one(&pool)
            .await
            .unwrap();
            assert_eq!(
                field_name, "creators",
                "source_version_id should reference a 'creators' journal row"
            );
        }
    }
}
