//! Per-job writeback orchestrator.
//!
//! Loads the job + manifestation + work snapshot, rewrites the OPF and
//! (optionally) embeds a new cover, repacks the EPUB, swaps the file
//! atomically, re-validates, rolls back on regression, and updates
//! `manifestations.current_file_hash` on success.
//!
//! The orchestrator does NOT take a transaction — the queue's `finish`
//! owns the job-status update via its own short-lived statement.  The
//! per-job file mutations here happen outside any user-facing tx.

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};

use quick_xml::Reader;
use quick_xml::events::Event;
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use sqlx::Row;
use uuid::Uuid;
use zip::ZipArchive;

use crate::config::Config;
use crate::services::epub::{self, ValidationOutcome, repack};

use super::cover_embed;
use super::error::WritebackError;
use super::events;
use super::opf_rewrite::{self, Target};
use super::path_rename;

#[derive(Debug)]
#[allow(dead_code)] // manifestation_id / reason consumed via tracing + future webhook
pub struct RunOutcome {
    pub manifestation_id: Uuid,
    pub success: bool,
    pub reason: String,
    pub error: Option<String>,
    /// Some → mark the job 'skipped' directly (bypassing the failed-retry
    /// path).  For cases where retrying won't help (unsupported format,
    /// missing file).
    pub skipped: Option<String>,
}

struct JobSnapshot {
    manifestation_id: Uuid,
    reason: String,
    file_path: String,
    format: String,
    cover_path: Option<String>,
    title: Option<String>,
    description: Option<String>,
    language: Option<String>,
    publisher: Option<String>,
    pub_date: Option<String>,
    isbn_10: Option<String>,
    isbn_13: Option<String>,
    attempt_count: i32,
}

pub async fn run_once(
    pool: &PgPool,
    _config: &Config,
    job_id: Uuid,
) -> Result<RunOutcome, WritebackError> {
    let snap = load_snapshot(pool, job_id).await?;
    let manifestation_id = snap.manifestation_id;
    let reason = snap.reason.clone();
    let attempt = snap.attempt_count;

    // Skip early when retrying won't help.
    if snap.format != "epub" {
        return Ok(RunOutcome {
            manifestation_id,
            success: false,
            reason,
            error: None,
            skipped: Some(format!("format_unsupported: {}", snap.format)),
        });
    }
    let src_path = PathBuf::from(&snap.file_path);
    if !src_path.exists() {
        return Ok(RunOutcome {
            manifestation_id,
            success: false,
            reason,
            error: None,
            skipped: Some(format!("file_missing: {}", snap.file_path)),
        });
    }

    // Snapshot original bytes for rollback + pre-validation.
    let original_bytes = std::fs::read(&src_path)?;
    let pre_report = epub::validate_and_repair(&src_path).map_err(WritebackError::Epub)?;

    // Read the OPF entry path from META-INF/container.xml.
    let opf_path = find_opf_path(&original_bytes)?;
    let opf_bytes = read_entry_bytes(&original_bytes, &opf_path)?;

    // Build writeback target from Step 7's per-field canonical columns.
    let target = Target {
        title: snap.title.as_deref(),
        description: snap.description.as_deref(),
        language: snap.language.as_deref(),
        publisher: snap.publisher.as_deref(),
        pub_date: snap.pub_date.as_deref(),
        isbn_10: snap.isbn_10.as_deref(),
        isbn_13: snap.isbn_13.as_deref(),
        series: None,
    };
    let new_opf = opf_rewrite::transform(&opf_bytes, &target)?;

    // Cover embed plan (only when the job was triggered by a cover move).
    let cover_plan = if reason == "cover"
        && let Some(cover_path) = snap.cover_path.as_deref()
    {
        let cover_bytes = std::fs::read(cover_path)?;
        Some(cover_embed::plan_embed(&new_opf, &cover_bytes)?)
    } else {
        None
    };

    let final_opf_bytes: Vec<u8> = cover_plan
        .as_ref()
        .and_then(|p| p.opf_replacement.clone())
        .unwrap_or(new_opf);

    let empty_replacements: HashMap<String, Vec<u8>> = HashMap::new();
    let binary_replacements = cover_plan
        .as_ref()
        .map(|p| &p.binary_replacements)
        .unwrap_or(&empty_replacements);
    let empty_additions: Vec<_> = Vec::new();
    let additions = cover_plan
        .as_ref()
        .map(|p| p.additions.as_slice())
        .unwrap_or(&empty_additions);

    // Repack into a temp file in the destination directory.
    let dest_dir = src_path.parent().unwrap_or(Path::new("."));
    let temp = repack::with_modifications(
        &src_path,
        dest_dir,
        Some(&opf_path),
        Some(&final_opf_bytes),
        binary_replacements,
        additions,
    )?;

    // Commit atomically.  Same-dir → tempfile persist; cross-FS surfaced
    // via path_rename::commit's EXDEV fallback.
    path_rename::commit(temp, &src_path)?;

    // Post-writeback validation: roll back if regressed.
    let post_report = epub::validate_and_repair(&src_path).map_err(WritebackError::Epub)?;
    if is_regression(&pre_report.outcome, &post_report.outcome) {
        // Rollback by re-writing the original bytes.
        std::fs::write(&src_path, &original_bytes)?;
        let err_msg = format!(
            "post_writeback_validation_regressed: pre={:?} post={:?}",
            pre_report.outcome, post_report.outcome
        );
        events::emit_writeback_failed(manifestation_id, &reason, attempt, &err_msg);
        return Ok(RunOutcome {
            manifestation_id,
            success: false,
            reason,
            error: Some(err_msg),
            skipped: None,
        });
    }

    // Update current_file_hash.  file_path is unchanged in the MVP
    // orchestrator (path rename is deferred to a future iteration).
    let new_hash = compute_hex_sha256(&src_path)?;
    sqlx::query("UPDATE manifestations SET current_file_hash = $1 WHERE id = $2")
        .bind(&new_hash)
        .bind(manifestation_id)
        .execute(pool)
        .await?;

    // Move cover sidecar from _covers/pending/ → _covers/accepted/ on
    // success.  Best-effort: a failed move does not fail the writeback —
    // Step 11 sweep surfaces orphans in pending/.
    if reason == "cover"
        && let Some(pending) = snap.cover_path.as_deref()
    {
        let _ = move_cover_sidecar(pending);
    }

    events::emit_writeback_complete(manifestation_id, &reason, attempt, &new_hash);
    Ok(RunOutcome {
        manifestation_id,
        success: true,
        reason,
        error: None,
        skipped: None,
    })
}

// ── Snapshot load ─────────────────────────────────────────────────────────

async fn load_snapshot(pool: &PgPool, job_id: Uuid) -> Result<JobSnapshot, WritebackError> {
    let row = sqlx::query(
        "SELECT wj.manifestation_id, wj.reason, wj.attempt_count, \
                m.file_path, m.format::text AS format, m.cover_path, \
                m.publisher, m.pub_date, m.isbn_10, m.isbn_13, \
                w.title, w.description, w.language \
           FROM writeback_jobs wj \
           JOIN manifestations m ON m.id = wj.manifestation_id \
           JOIN works w          ON w.id = m.work_id \
          WHERE wj.id = $1",
    )
    .bind(job_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| WritebackError::Persist(format!("job {job_id} not found")))?;

    let pub_date: Option<time::Date> = row.try_get("pub_date").ok();
    Ok(JobSnapshot {
        manifestation_id: row.try_get("manifestation_id")?,
        reason: row.try_get::<String, _>("reason")?,
        file_path: row.try_get("file_path")?,
        format: row.try_get("format")?,
        cover_path: row.try_get("cover_path")?,
        title: row.try_get("title")?,
        description: row.try_get("description")?,
        language: row.try_get("language")?,
        publisher: row.try_get("publisher")?,
        pub_date: pub_date.map(|d| d.to_string()),
        isbn_10: row.try_get("isbn_10")?,
        isbn_13: row.try_get("isbn_13")?,
        attempt_count: row.try_get("attempt_count")?,
    })
}

// ── OPF path + entry helpers ──────────────────────────────────────────────

fn find_opf_path(epub_bytes: &[u8]) -> Result<String, WritebackError> {
    let container_bytes = read_entry_bytes(epub_bytes, "META-INF/container.xml")
        .map_err(|_| WritebackError::MissingOpf)?;
    extract_opf_path(&container_bytes).ok_or(WritebackError::MissingOpf)
}

fn extract_opf_path(container_bytes: &[u8]) -> Option<String> {
    let xml = std::str::from_utf8(container_bytes).ok()?;
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    loop {
        match reader.read_event().ok()? {
            Event::Empty(e) | Event::Start(e) if e.name().as_ref() == b"rootfile" => {
                if let Some(attr) = e
                    .attributes()
                    .flatten()
                    .find(|a| a.key.as_ref() == b"full-path")
                {
                    return std::str::from_utf8(&attr.value).ok().map(|s| s.to_string());
                }
            }
            Event::Eof => return None,
            _ => {}
        }
    }
}

fn read_entry_bytes(epub_bytes: &[u8], entry: &str) -> Result<Vec<u8>, WritebackError> {
    let cursor = std::io::Cursor::new(epub_bytes);
    let mut ar = ZipArchive::new(cursor).map_err(WritebackError::Zip)?;
    let file = ar.by_name(entry).map_err(WritebackError::Zip)?;
    let mut buf = Vec::new();
    file.take(crate::services::epub::MAX_ENTRY_UNCOMPRESSED_BYTES + 1)
        .read_to_end(&mut buf)?;
    Ok(buf)
}

// ── Regression detection ──────────────────────────────────────────────────

fn is_regression(pre: &ValidationOutcome, post: &ValidationOutcome) -> bool {
    use ValidationOutcome::*;
    matches!((pre, post), (_, Quarantined) | (Clean | Repaired, Degraded))
}

// ── Hash + sidecar helpers ────────────────────────────────────────────────

fn compute_hex_sha256(path: &Path) -> Result<String, WritebackError> {
    let mut f = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(digest.len() * 2);
    for b in digest {
        use std::fmt::Write;
        let _ = write!(hex, "{b:02x}");
    }
    Ok(hex)
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
mod tests {
    use super::*;
    use crate::config::{CleanupMode, CoverConfig, EnrichmentConfig, WritebackConfig};
    use std::io::Write;
    use zip::ZipWriter;
    use zip::write::{ExtendedFileOptions, FileOptions};

    fn app_url() -> String {
        std::env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgres://reverie_app:reverie_app@localhost:5433/reverie_dev".into()
        })
    }
    fn ing_url() -> String {
        std::env::var("DATABASE_URL_INGESTION").unwrap_or_else(|_| {
            "postgres://reverie_ingestion:reverie_ingestion@localhost:5433/reverie_dev".into()
        })
    }

    fn test_config() -> Config {
        Config {
            port: 3000,
            database_url: String::new(),
            library_path: String::new(),
            ingestion_path: String::new(),
            quarantine_path: String::new(),
            log_level: "info".into(),
            db_max_connections: 5,
            oidc_issuer_url: String::new(),
            oidc_client_id: String::new(),
            oidc_client_secret: String::new(),
            oidc_redirect_uri: String::new(),
            ingestion_database_url: String::new(),
            format_priority: vec!["epub".into()],
            cleanup_mode: CleanupMode::None,
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
            openlibrary_base_url: "https://example.invalid".into(),
            googlebooks_base_url: "https://example.invalid".into(),
            googlebooks_api_key: None,
            hardcover_base_url: "https://example.invalid".into(),
            hardcover_api_token: None,
            operator_contact: None,
        }
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
        let work_id: Uuid = sqlx::query_scalar(
            "INSERT INTO works (title, sort_title) VALUES ($1, $1) RETURNING id",
        )
        .bind(format!("WbFixture-{marker}"))
        .fetch_one(ing_pool)
        .await
        .unwrap();
        let m_id: Uuid = sqlx::query_scalar(
            "INSERT INTO manifestations \
               (work_id, format, file_path, ingestion_file_hash, current_file_hash, \
                file_size_bytes, ingestion_status, validation_status) \
             VALUES ($1, 'epub'::manifestation_format, $2, $3, $3, 1000, \
                     'complete'::ingestion_status, 'valid'::validation_status) \
             RETURNING id",
        )
        .bind(work_id)
        .bind(file_path)
        .bind(ingestion_hash)
        .fetch_one(ing_pool)
        .await
        .unwrap();
        (work_id, m_id)
    }

    async fn cleanup_fixture(app_pool: &PgPool, ing_pool: &PgPool, work_id: Uuid, m_id: Uuid) {
        let _ = sqlx::query("DELETE FROM writeback_jobs WHERE manifestation_id = $1")
            .bind(m_id)
            .execute(app_pool)
            .await;
        let _ = sqlx::query("DELETE FROM manifestations WHERE id = $1")
            .bind(m_id)
            .execute(ing_pool)
            .await;
        let _ = sqlx::query("DELETE FROM works WHERE id = $1")
            .bind(work_id)
            .execute(ing_pool)
            .await;
    }

    /// Task 16 + Task 24: full run_once on a fixture EPUB whose OPF lives
    /// at `OEBPS/package.opf` (not the default `content.opf`).  Verifies:
    /// - the non-default OPF is discovered via `META-INF/container.xml`
    /// - the rewritten OPF carries the new title
    /// - `current_file_hash` changes after writeback
    /// - `ingestion_file_hash` is immutable across the writeback
    #[tokio::test]
    #[ignore]
    async fn run_once_finds_non_default_opf_and_updates_hash() {
        let app_pool = PgPool::connect(&app_url()).await.unwrap();
        let ing_pool = PgPool::connect(&ing_url()).await.unwrap();
        let marker = Uuid::new_v4().simple().to_string();

        let (_dir, path) = make_fixture_epub("Old Title");
        let original_bytes = std::fs::read(&path).unwrap();
        let original_hash = initial_hex_sha256(&original_bytes);

        let (work_id, m_id) =
            insert_fixture(&ing_pool, &marker, path.to_str().unwrap(), &original_hash).await;

        // Set the works.title to the new value.  Simulates Step 7 having
        // moved the pointer — our job represents the writeback that
        // follows.
        let new_title = format!("New Title {marker}");
        sqlx::query("UPDATE works SET title = $1 WHERE id = $2")
            .bind(&new_title)
            .bind(work_id)
            .execute(&ing_pool)
            .await
            .unwrap();

        let job_id: Uuid = sqlx::query_scalar(
            "INSERT INTO writeback_jobs (manifestation_id, reason) \
             VALUES ($1, 'metadata') RETURNING id",
        )
        .bind(m_id)
        .fetch_one(&ing_pool)
        .await
        .unwrap();

        let outcome = run_once(&app_pool, &test_config(), job_id).await.unwrap();
        assert!(outcome.success, "run_once should succeed: {:?}", outcome);

        // OPF at OEBPS/package.opf should contain the new title.
        let new_bytes = std::fs::read(&path).unwrap();
        let opf_bytes = read_entry_bytes(&new_bytes, "OEBPS/package.opf").unwrap();
        let opf_str = String::from_utf8(opf_bytes).unwrap();
        assert!(
            opf_str.contains(&format!("<dc:title>{new_title}</dc:title>")),
            "new title not present in OPF at OEBPS/package.opf: {opf_str}"
        );

        // Hash columns: current changed, ingestion unchanged.
        let (current, ingestion): (String, String) = sqlx::query_as(
            "SELECT current_file_hash, ingestion_file_hash FROM manifestations WHERE id = $1",
        )
        .bind(m_id)
        .fetch_one(&app_pool)
        .await
        .unwrap();
        assert_ne!(current, original_hash, "current_file_hash must change");
        assert_eq!(
            ingestion, original_hash,
            "ingestion_file_hash must NOT change"
        );

        cleanup_fixture(&app_pool, &ing_pool, work_id, m_id).await;
    }

    /// Task 24 continuation: two successive writebacks on the same
    /// manifestation.  `ingestion_file_hash` must be constant across
    /// both; `current_file_hash` must change each time.
    #[tokio::test]
    #[ignore]
    async fn ingestion_file_hash_immutable_across_writeback_chain() {
        let app_pool = PgPool::connect(&app_url()).await.unwrap();
        let ing_pool = PgPool::connect(&ing_url()).await.unwrap();
        let marker = Uuid::new_v4().simple().to_string();

        let (_dir, path) = make_fixture_epub("Initial");
        let original_bytes = std::fs::read(&path).unwrap();
        let original_hash = initial_hex_sha256(&original_bytes);

        let (work_id, m_id) =
            insert_fixture(&ing_pool, &marker, path.to_str().unwrap(), &original_hash).await;

        // First writeback: set title to A.
        sqlx::query("UPDATE works SET title = $1 WHERE id = $2")
            .bind(format!("First {marker}"))
            .bind(work_id)
            .execute(&ing_pool)
            .await
            .unwrap();
        let j1: Uuid = sqlx::query_scalar(
            "INSERT INTO writeback_jobs (manifestation_id, reason) VALUES ($1, 'metadata') RETURNING id",
        )
        .bind(m_id)
        .fetch_one(&ing_pool)
        .await
        .unwrap();
        run_once(&app_pool, &test_config(), j1).await.unwrap();

        let hash_after_first: String =
            sqlx::query_scalar("SELECT current_file_hash FROM manifestations WHERE id = $1")
                .bind(m_id)
                .fetch_one(&app_pool)
                .await
                .unwrap();
        assert_ne!(hash_after_first, original_hash);

        // Second writeback: set title to B.
        sqlx::query("UPDATE works SET title = $1 WHERE id = $2")
            .bind(format!("Second {marker}"))
            .bind(work_id)
            .execute(&ing_pool)
            .await
            .unwrap();
        let j2: Uuid = sqlx::query_scalar(
            "INSERT INTO writeback_jobs (manifestation_id, reason) VALUES ($1, 'metadata') RETURNING id",
        )
        .bind(m_id)
        .fetch_one(&ing_pool)
        .await
        .unwrap();
        run_once(&app_pool, &test_config(), j2).await.unwrap();

        let (current, ingestion): (String, String) = sqlx::query_as(
            "SELECT current_file_hash, ingestion_file_hash FROM manifestations WHERE id = $1",
        )
        .bind(m_id)
        .fetch_one(&app_pool)
        .await
        .unwrap();
        assert_ne!(
            current, hash_after_first,
            "second writeback must change current_file_hash again"
        );
        assert_eq!(
            ingestion, original_hash,
            "ingestion_file_hash must NEVER change"
        );

        cleanup_fixture(&app_pool, &ing_pool, work_id, m_id).await;
    }

    // Task 21 (post-validation rollback) is covered by the `is_regression_*`
    // unit tests below (decision logic) + the std::fs::write+rename
    // byte-restore semantics in `run_once`.  Triggering a real regression
    // end-to-end requires a fixture that reliably downgrades the
    // ValidationOutcome (Clean → Degraded or any → Quarantined) — the
    // simple fixtures we can build in-test don't reach that threshold
    // under `validate_and_repair`.  Live regression scenarios are covered
    // by the BLUEPRINT manual-smoke checklist.

    #[test]
    fn is_regression_detects_quarantine() {
        assert!(is_regression(
            &ValidationOutcome::Clean,
            &ValidationOutcome::Quarantined
        ));
        assert!(is_regression(
            &ValidationOutcome::Repaired,
            &ValidationOutcome::Quarantined
        ));
    }

    #[test]
    fn is_regression_detects_clean_to_degraded() {
        assert!(is_regression(
            &ValidationOutcome::Clean,
            &ValidationOutcome::Degraded
        ));
    }

    #[test]
    fn is_regression_is_false_for_clean_to_clean() {
        assert!(!is_regression(
            &ValidationOutcome::Clean,
            &ValidationOutcome::Clean
        ));
    }

    #[test]
    fn is_regression_is_false_for_degraded_to_degraded() {
        assert!(!is_regression(
            &ValidationOutcome::Degraded,
            &ValidationOutcome::Degraded
        ));
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
}
