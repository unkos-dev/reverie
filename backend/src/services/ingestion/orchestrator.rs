use std::path::{Path, PathBuf};

use sqlx::PgPool;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;
use walkdir::WalkDir;

use crate::config::{CleanupMode, Config, SUPPORTED_FORMATS};
use crate::models::{ingestion_job, work};
use crate::services::epub::{self, ValidationOutcome};
use crate::services::ingestion::{cleanup, copier, format_filter, path_template, quarantine};
use crate::services::metadata;

#[derive(Debug)]
pub struct ScanResult {
    pub processed: usize,
    pub failed: usize,
    pub skipped: usize,
}

/// Start the filesystem watcher and process batches in a loop.
/// Exits when `cancel` is triggered or the watcher errors.
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
            _ = cancel.cancelled() => {
                tracing::info!("orchestrator shutting down");
                break;
            }
            batch = rx.recv() => {
                match batch {
                    Some(_paths) => {
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
                    }
                    None => {
                        tracing::warn!("watcher channel closed, stopping orchestrator");
                        break;
                    }
                }
            }
        }
    }

    Ok(())
}

/// Advisory lock ID for serializing ingestion scans. Prevents concurrent scan_once
/// calls (watcher + manual POST) from racing on duplicate checks and file copies.
const SCAN_ADVISORY_LOCK_ID: i64 = 0x52657665_00000004; // "Reve" + step 4

/// One-shot ingestion scan: walk the ingestion directory, filter by format priority,
/// copy to library, and track via ingestion_jobs.
///
/// Acquires a database advisory lock to serialize concurrent scans. A second scan
/// that arrives while one is in progress will block until the first completes.
pub async fn scan_once(config: &Config, pool: &PgPool) -> Result<ScanResult, anyhow::Error> {
    // Serialize scans — only one can run at a time. Uses a session-level advisory
    // lock (released when the connection returns to the pool) rather than a
    // transaction-level lock, because the scan spans many transactions.
    let mut lock_conn = pool.acquire().await?;
    sqlx::query("SELECT pg_advisory_lock($1)")
        .bind(SCAN_ADVISORY_LOCK_ID)
        .execute(&mut *lock_conn)
        .await?;

    let result = scan_once_inner(config, pool).await;

    // Release the advisory lock explicitly (also released on connection drop)
    let _ = sqlx::query("SELECT pg_advisory_unlock($1)")
        .bind(SCAN_ADVISORY_LOCK_ID)
        .execute(&mut *lock_conn)
        .await;

    result
}

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
                .map(|e| e.into_path())
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
    let duplicate = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM manifestations WHERE ingestion_file_hash = $1 OR file_path = $2)",
    )
    .bind(&source_hash)
    .bind(&dest_path_str)
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
    // This check is invariant-enforced (format_filter only selects extensions from
    // format_priority ⊆ SUPPORTED_FORMATS), but we keep it as a safety net.
    let ext = vars.get("ext").cloned().unwrap_or_default();
    if !SUPPORTED_FORMATS.contains(&ext.as_str()) {
        // Clean up the copy that was already written to the library.
        let dest = dest_path_str.clone();
        let _ = tokio::task::spawn_blocking(move || {
            if let Err(e) = std::fs::remove_file(&dest) {
                tracing::warn!(path = %dest, error = %e, "failed to remove orphaned library file after format check");
            }
        })
        .await;
        return ProcessResult::Failed(format!("unsupported format: {ext}"));
    }
    let format_str = ext.as_str();

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
                        let _ = tokio::task::spawn_blocking(move || {
                            if let Err(e) = std::fs::remove_file(&lib_file_str) {
                                tracing::warn!(
                                    path = %lib_file_str,
                                    error = %e,
                                    "failed to remove library file for quarantined EPUB"
                                );
                            }
                        })
                        .await;
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
            if new_full.display().to_string() != dest_path_str {
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
                        let _ = std::fs::remove_dir(old_parent); // only removes if empty
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
            } else {
                dest_path_str.clone()
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
        format_str,
        validation_status_str,
        &accessibility_metadata,
    )
    .await;

    let (work_id, manifestation_id) = match db_outcome {
        Ok(pair) => pair,
        Err(e) => {
            tracing::error!(error = %e, "ingest DB commit failed");
            let dest = final_path_str.clone();
            let _ = tokio::task::spawn_blocking(move || {
                if let Err(rm_err) = std::fs::remove_file(&dest) {
                    tracing::warn!(
                        path = %dest,
                        error = %rm_err,
                        "failed to remove orphaned library file after DB error"
                    );
                }
            })
            .await;
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
async fn commit_ingest(
    pool: &PgPool,
    extracted: &Option<crate::services::metadata::extractor::ExtractedMetadata>,
    vars: &std::collections::HashMap<String, String>,
    final_path_str: &str,
    copy_result: &copier::CopyResult,
    format_str: &str,
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
    let manifestation_id: Uuid = sqlx::query_scalar(
        "INSERT INTO manifestations \
             (work_id, format, file_path, ingestion_file_hash, current_file_hash, \
              file_size_bytes, ingestion_status, validation_status, accessibility_metadata) \
         VALUES ($1, $2::manifestation_format, $3, $4, $4, $5, \
                 'complete'::ingestion_status, $6::validation_status, $7) \
         RETURNING id",
    )
    .bind(work_id)
    .bind(format_str)
    .bind(final_path_str)
    .bind(&copy_result.sha256)
    .bind(copy_result.file_size as i64)
    .bind(validation_status_str)
    .bind(accessibility_metadata)
    .fetch_one(&mut *tx)
    .await?;

    // 3. Write drafts — OPF or heuristic-fallback (Step 7 task 5).
    //    The heuristic row gives the canonical title_version_id pointer even
    //    when no OPF metadata exists, preserving the ingest invariant.
    let metadata_for_drafts: ExtractedMetadata = match extracted.as_ref() {
        Some(meta) => meta.clone(),
        None => {
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
        }
    };
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
        .map(|i| (i.isbn_10.clone(), i.isbn_13.clone()))
        .unwrap_or((None, None));
    let publisher = extracted.as_ref().and_then(|m| m.publisher.clone());
    let pub_date = extracted.as_ref().and_then(|m| m.pub_date);

    sqlx::query(
        "UPDATE manifestations SET \
            isbn_10 = $1, isbn_13 = $2, publisher = $3, pub_date = $4, \
            isbn_10_version_id = $5, isbn_13_version_id = $6, \
            publisher_version_id = $7, pub_date_version_id = $8 \
         WHERE id = $9",
    )
    .bind(&isbn_10)
    .bind(&isbn_13)
    .bind(&publisher)
    .bind(pub_date)
    .bind(draft_ids.get("isbn_10").copied())
    .bind(draft_ids.get("isbn_13").copied())
    .bind(draft_ids.get("publisher").copied())
    .bind(draft_ids.get("pub_date").copied())
    .bind(manifestation_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok((work_id, manifestation_id))
}

async fn quarantine_async(source: &Path, quarantine_path: &Path, reason: &str) {
    let source = source.to_path_buf();
    let qpath = quarantine_path.to_path_buf();
    let reason = reason.to_string();
    let _ = tokio::task::spawn_blocking(move || {
        if let Err(e) = quarantine::quarantine_file(&source, &qpath, &reason) {
            tracing::error!(error = %e, "quarantine failed");
        }
    })
    .await;
}

#[cfg(test)]
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
            ingestion_database_url: String::new(),
            format_priority: vec!["epub".into(), "pdf".into()],
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
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM manifestations WHERE file_path = $1")
                .bind(dest.to_str().unwrap())
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
        let title: String = sqlx::query_scalar(
            "SELECT w.title FROM works w \
             JOIN manifestations m ON m.work_id = w.id \
             WHERE m.file_path = $1",
        )
        .bind(dest.to_str().unwrap())
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(title, "The Integration Test");

        // Verify author was created and linked
        let author_name: String = sqlx::query_scalar(
            "SELECT a.name FROM authors a \
             JOIN work_authors wa ON wa.author_id = a.id \
             JOIN manifestations m ON m.work_id = wa.work_id \
             WHERE m.file_path = $1",
        )
        .bind(dest.to_str().unwrap())
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(author_name, "Test McAuthor");

        // Verify ISBN was populated on the manifestation
        let isbn: Option<String> =
            sqlx::query_scalar("SELECT isbn_13 FROM manifestations WHERE file_path = $1")
                .bind(dest.to_str().unwrap())
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(isbn.as_deref(), Some("9780306406157"));

        // Verify metadata_versions drafts were created
        let draft_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM metadata_versions mv \
             JOIN manifestations m ON m.id = mv.manifestation_id \
             WHERE m.file_path = $1 AND mv.source = 'opf' AND mv.status::text = 'pending'",
        )
        .bind(dest.to_str().unwrap())
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(
            draft_count >= 5,
            "expected at least 5 draft rows, got {draft_count}"
        );

        // Verify series was created
        let series_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM series_works sw \
             JOIN manifestations m ON m.work_id = sw.work_id \
             WHERE m.file_path = $1",
        )
        .bind(dest.to_str().unwrap())
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
        let status: String = sqlx::query_scalar(
            "SELECT validation_status::text FROM manifestations WHERE file_path = $1",
        )
        .bind(dest.to_str().unwrap())
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
            .filter_map(|e| e.ok())
            .collect();
        assert!(
            !quarantine_entries.is_empty(),
            "expected a quarantine sidecar file, found none"
        );

        // Library must NOT contain the corrupt file
        let dest = library.path().join("Bad/Corrupt Book.epub");
        assert!(!dest.exists(), "corrupt EPUB must not remain in library");

        // No manifestation row must have been written
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM manifestations WHERE file_path = $1")
                .bind(dest.to_str().unwrap())
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
    /// `source='opf'`.  Without this invariant, metadata_versions is optional
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
        #[derive(sqlx::FromRow)]
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
        let inv: Invariant = sqlx::query_as(
            "SELECT w.title, w.title_version_id, \
                    w.language, w.language_version_id, \
                    m.publisher, m.publisher_version_id, \
                    m.pub_date_version_id, \
                    m.isbn_13, m.isbn_13_version_id \
             FROM manifestations m \
             JOIN works w ON w.id = m.work_id \
             WHERE m.file_path = $1",
        )
        .bind(dest.to_str().unwrap())
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
            let source_for_ptr: String =
                sqlx::query_scalar("SELECT source FROM metadata_versions WHERE id = $1")
                    .bind(pointer)
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
        let (title_ptr, title): (Option<uuid::Uuid>, String) = sqlx::query_as(
            "SELECT w.title_version_id, w.title FROM works w \
             JOIN manifestations m ON m.work_id = w.id \
             WHERE m.file_path = $1",
        )
        .bind(dest.to_str().unwrap())
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(
            title.contains("Heuristic Title"),
            "title should include heuristic value, got '{title}'"
        );
        let ptr = title_ptr.expect("title_version_id must be wired for heuristic row");

        let (src, field_name, confidence_score): (String, String, Option<f32>) = sqlx::query_as(
            "SELECT source, field_name, confidence_score \
             FROM metadata_versions WHERE id = $1",
        )
        .bind(ptr)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(src, "opf");
        assert_eq!(field_name, "title");
        let score = confidence_score.expect("confidence_score must be set for heuristic row");
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
        let rows: Vec<(uuid::Uuid, Option<uuid::Uuid>)> = sqlx::query_as(
            "SELECT wa.author_id, wa.source_version_id \
             FROM work_authors wa \
             JOIN manifestations m ON m.work_id = wa.work_id \
             WHERE m.file_path = $1",
        )
        .bind(dest.to_str().unwrap())
        .fetch_all(&pool)
        .await
        .unwrap();
        assert!(!rows.is_empty(), "expected at least one work_author row");

        for (author_id, source_version_id) in rows {
            let ptr = source_version_id.unwrap_or_else(|| {
                panic!("work_authors.source_version_id NULL for author {author_id}")
            });
            let field_name: String =
                sqlx::query_scalar("SELECT field_name FROM metadata_versions WHERE id = $1")
                    .bind(ptr)
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
