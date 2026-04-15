use std::path::{Path, PathBuf};

use sqlx::PgPool;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;
use walkdir::WalkDir;

use crate::config::{CleanupMode, Config, SUPPORTED_FORMATS};
use crate::models::ingestion_job;
use crate::services::ingestion::{cleanup, copier, format_filter, path_template, quarantine};

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
const SCAN_ADVISORY_LOCK_ID: i64 = 0x546F6D65_00000004; // "Tome" + step 4

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
        "SELECT EXISTS(SELECT 1 FROM manifestations WHERE file_hash = $1 OR file_path = $2)",
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

    // Step 5: Create work + manifestation in a single CTE
    let title = vars.get("Title").cloned().unwrap_or("Unknown".into());
    let result = sqlx::query(
        "WITH new_work AS ( \
            INSERT INTO works (title, sort_title) VALUES ($1, $2) RETURNING id \
         ) \
         INSERT INTO manifestations (work_id, format, file_path, file_hash, file_size_bytes, ingestion_status) \
         SELECT id, $3::manifestation_format, $4, $5, $6, 'complete'::ingestion_status FROM new_work",
    )
    .bind(&title)
    .bind(&title) // sort_title = title for now
    .bind(format_str)
    .bind(&dest_path_str)
    .bind(&copy_result.sha256)
    .bind(copy_result.file_size as i64)
    .execute(pool)
    .await;

    match result {
        Ok(_) => ProcessResult::Complete,
        Err(e) => {
            tracing::error!(error = %e, "failed to create work/manifestation");
            // Clean up the orphaned library file to avoid stranded copies
            let dest = dest_path_str.clone();
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
            ProcessResult::Failed(format!("DB insert failed: {e}"))
        }
    }
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

    fn db_url() -> String {
        std::env::var("DATABASE_URL_INGESTION").unwrap_or_else(|_| {
            "postgres://tome_ingestion:tome_ingestion@localhost:5433/tome_dev".into()
        })
    }

    fn test_config_for(ingestion: &str, library: &str, quarantine: &str) -> Config {
        Config {
            port: 3000,
            database_url: db_url(),
            library_path: library.to_string(),
            ingestion_path: ingestion.to_string(),
            quarantine_path: quarantine.to_string(),
            log_level: "info".into(),
            db_max_connections: 5,
            oidc_issuer_url: String::new(),
            oidc_client_id: String::new(),
            oidc_client_secret: String::new(),
            oidc_redirect_uri: String::new(),
            ingestion_database_url: db_url(),
            format_priority: vec!["epub".into(), "pdf".into()],
            // Preserve source files during tests so we can run multiple scans
            cleanup_mode: CleanupMode::None,
        }
    }

    /// Clean up DB records created during a test run.
    async fn cleanup_test_data(pool: &PgPool, library_file_path: &str, source_path: &str) {
        let work_id: Option<uuid::Uuid> =
            sqlx::query_scalar("DELETE FROM manifestations WHERE file_path = $1 RETURNING work_id")
                .bind(library_file_path)
                .fetch_optional(pool)
                .await
                .ok()
                .flatten();
        if let Some(wid) = work_id {
            let _ = sqlx::query("DELETE FROM works WHERE id = $1")
                .bind(wid)
                .execute(pool)
                .await;
        }
        let _ = sqlx::query("DELETE FROM ingestion_jobs WHERE source_path = $1")
            .bind(source_path)
            .execute(pool)
            .await;
    }

    #[tokio::test]
    #[ignore] // Requires running postgres with applied migrations
    async fn scan_once_empty_dir_returns_zero() {
        let pool = sqlx::PgPool::connect(&db_url()).await.expect("connect");
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

    #[tokio::test]
    #[ignore] // Requires running postgres with applied migrations
    async fn scan_once_processes_epub_end_to_end() {
        let pool = sqlx::PgPool::connect(&db_url()).await.expect("connect");
        let ingestion = tempfile::tempdir().unwrap();
        let library = tempfile::tempdir().unwrap();
        let quarantine = tempfile::tempdir().unwrap();

        let source = ingestion.path().join("Tolkien - The Hobbit.epub");
        std::fs::write(&source, b"fake epub bytes for scan_once test").unwrap();

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
        let dest = library.path().join("Tolkien/The Hobbit.epub");
        assert!(dest.exists(), "expected file at {}", dest.display());
        assert_eq!(
            std::fs::read(&dest).unwrap(),
            b"fake epub bytes for scan_once test"
        );

        // Manifestation row should exist
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM manifestations WHERE file_path = $1")
                .bind(dest.to_str().unwrap())
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(count, 1, "expected 1 manifestation row");

        cleanup_test_data(&pool, dest.to_str().unwrap(), source.to_str().unwrap()).await;
    }

    #[tokio::test]
    #[ignore] // Requires running postgres with applied migrations
    async fn scan_once_skips_duplicate_on_second_run() {
        let pool = sqlx::PgPool::connect(&db_url()).await.expect("connect");
        let ingestion = tempfile::tempdir().unwrap();
        let library = tempfile::tempdir().unwrap();
        let quarantine = tempfile::tempdir().unwrap();

        // Unique content to avoid collisions with other test data
        let unique_content = format!("dedup-test-{}", uuid::Uuid::new_v4());
        let source = ingestion.path().join("Author - Book.epub");
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

        let dest = library.path().join("Author/Book.epub");
        cleanup_test_data(&pool, dest.to_str().unwrap(), source.to_str().unwrap()).await;
    }
}
