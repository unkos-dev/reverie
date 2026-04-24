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
use tempfile::NamedTempFile;
use uuid::Uuid;
use zip::ZipArchive;

use crate::config::Config;
use crate::services::epub::{self, ValidationOutcome, ValidationReport, repack};
use crate::services::ingestion::path_template;

use super::cover_embed;
use super::error::WritebackError;
use super::opf_rewrite::{self, Target};
use super::path_rename;

/// Terminal outcome of a single `run_once` call.  Three arms, one per
/// terminal DB transition the queue performs — illegal combinations
/// (success without a hash, skipped with an error, etc.) are structurally
/// unrepresentable.
#[derive(Debug)]
pub enum RunOutcome {
    /// Writeback completed cleanly.  `current_file_hash` is the new
    /// on-disk SHA-256; the queue emits `writeback_complete` with it.
    Success {
        manifestation_id: Uuid,
        reason: String,
        current_file_hash: String,
    },
    /// Retrying won't help (unsupported format, missing file, post-
    /// validation rollback).  Bypasses the retry path directly to
    /// `mark_skipped`.  `skip_reason` is the user-facing explanation.
    Skipped {
        manifestation_id: Uuid,
        reason: String,
        skip_reason: String,
    },
    /// Writeback failed in a way that's potentially retryable (the
    /// queue's `finish` decides whether attempt_count has reached
    /// `max_attempts` and escalates to `skipped`).
    Failed {
        manifestation_id: Uuid,
        reason: String,
        error: String,
    },
}

/// Outcome of the post-writeback validation decision.  Extracted for
/// testability: both branches need to rollback, so the shared decision
/// point lives in one place.
#[cfg_attr(test, derive(Debug))]
enum FinaliseAction {
    /// Post-validation passed — keep the new on-disk file.
    Commit,
    /// Post-validation failed (regression or validator error) — the
    /// rollback has already restored the original bytes atomically.
    RolledBack(String),
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
    /// Primary author's `sort_name` (role = 'author', position 0).
    /// Used to render the path template; `None` falls back to `Unknown`.
    primary_author: Option<String>,
}

pub async fn run_once(
    pool: &PgPool,
    config: &Config,
    job_id: Uuid,
) -> Result<RunOutcome, WritebackError> {
    let snap = load_snapshot(pool, job_id).await?;
    let manifestation_id = snap.manifestation_id;
    let reason = snap.reason.clone();

    // Skip early when retrying won't help.
    if snap.format != "epub" {
        return Ok(RunOutcome::Skipped {
            manifestation_id,
            reason,
            skip_reason: format!("format_unsupported: {}", snap.format),
        });
    }
    let src_path = PathBuf::from(&snap.file_path);
    if !src_path.exists() {
        return Ok(RunOutcome::Skipped {
            manifestation_id,
            reason,
            skip_reason: format!("file_missing: {}", snap.file_path),
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

    // `plan_embed` returns manifest hrefs relative to the OPF file's
    // location.  The repack layer keys off ZIP-absolute paths, so
    // translate hrefs by joining with the OPF's parent directory
    // (e.g. `images/cover.png` → `OEBPS/images/cover.png`).  When the
    // OPF is at ZIP root (`content.opf`), the two coincide.
    let opf_dir = opf_path.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
    let empty_replacements: HashMap<String, Vec<u8>> = HashMap::new();
    let translated_replacements: HashMap<String, Vec<u8>> = match cover_plan.as_ref() {
        Some(p) => p
            .binary_replacements
            .iter()
            .map(|(href, bytes)| {
                resolve_opf_relative(opf_dir, href).map(|path| (path, bytes.clone()))
            })
            .collect::<Result<_, _>>()?,
        None => HashMap::new(),
    };
    let binary_replacements = if translated_replacements.is_empty() {
        &empty_replacements
    } else {
        &translated_replacements
    };
    let empty_additions: Vec<(
        String,
        Vec<u8>,
        zip::write::FileOptions<'static, zip::write::ExtendedFileOptions>,
    )> = Vec::new();
    let translated_additions: Vec<(
        String,
        Vec<u8>,
        zip::write::FileOptions<'static, zip::write::ExtendedFileOptions>,
    )> = match cover_plan.as_ref() {
        Some(p) => p
            .additions
            .iter()
            .map(|(href, bytes, opts)| {
                resolve_opf_relative(opf_dir, href).map(|path| (path, bytes.clone(), opts.clone()))
            })
            .collect::<Result<_, _>>()?,
        None => Vec::new(),
    };
    let additions: &[_] = if translated_additions.is_empty() {
        &empty_additions
    } else {
        &translated_additions
    };

    // Repack into a temp file in the destination directory.
    // `src_path` is always an existing managed file on disk, so its parent
    // must exist; refuse rather than silently falling back to CWD — a rogue
    // rename that landed at `/` would otherwise write the temp into the
    // worker's working directory.
    let dest_dir = src_path.parent().ok_or_else(|| {
        WritebackError::Persist(format!(
            "src_path has no parent directory: {}",
            src_path.display()
        ))
    })?;
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

    // Post-writeback validation: rollback atomically on regression OR
    // validator error.  Both paths share `finalise_post_writeback`.
    let post_validation = epub::validate_and_repair(&src_path);
    match finalise_post_writeback(
        &pre_report.outcome,
        post_validation,
        &src_path,
        &original_bytes,
        dest_dir,
    )? {
        FinaliseAction::Commit => {}
        FinaliseAction::RolledBack(err_msg) => {
            return Ok(RunOutcome::Failed {
                manifestation_id,
                reason,
                error: err_msg,
            });
        }
    }

    // Path-rename: re-render the path template against the canonical
    // metadata and move the file if the rendered path differs.
    // Skipped when `library_path` is empty (test/dev shortcut).
    let final_path = path_rename_step(&snap, config, src_path.clone(), pool).await?;

    // Update current_file_hash from the final on-disk file.
    //
    // If this UPDATE fails the on-disk rewrite + rename already committed,
    // so `file_path` is correct but `current_file_hash` stays at the
    // pre-writeback value until the next successful retry.  Step 11's
    // library-health sweep will surface the divergence, but we log the
    // specifics at `error!` so an operator doesn't have to wait for the
    // sweep to notice.
    let new_hash = compute_hex_sha256(&final_path)?;
    if let Err(e) = sqlx::query("UPDATE manifestations SET current_file_hash = $1 WHERE id = $2")
        .bind(&new_hash)
        .bind(manifestation_id)
        .execute(pool)
        .await
    {
        tracing::error!(
            error = %e,
            %manifestation_id,
            final_path = %final_path.display(),
            attempted_hash = %new_hash,
            "writeback: current_file_hash UPDATE failed after successful on-disk commit \
             — on-disk file diverges from DB hash until Step 11 sweep or retry reconciles"
        );
        return Err(WritebackError::Db(e));
    }

    // Move cover sidecar from _covers/pending/ → _covers/accepted/ on
    // success.  Best-effort: a failed move does not fail the writeback —
    // Step 11 sweep surfaces orphans in pending/.  Log failures at warn!
    // so operators can observe stuck sidecars before the sweep lands.
    if reason == "cover"
        && let Some(pending) = snap.cover_path.as_deref()
        && let Err(e) = move_cover_sidecar(pending)
    {
        tracing::warn!(
            error = %e,
            %manifestation_id,
            pending_path = pending,
            "writeback: cover sidecar move failed (non-fatal; Step 11 sweep will reconcile)"
        );
    }

    Ok(RunOutcome::Success {
        manifestation_id,
        reason,
        current_file_hash: new_hash,
    })
}

/// Decide what to do after the post-writeback validation runs.  Both
/// regression and validator-error branches perform the atomic rollback
/// in one place, so the on-disk file can never be left in a broken state
/// while the DB still points at the original hash.
///
/// Returns `WritebackError` only when the rollback itself fails
/// (disk-full, permissions).  A failed rollback is genuinely fatal —
/// the queue will mark the job failed and Step 11 will flag the
/// divergence on its next sweep.
fn finalise_post_writeback(
    pre_outcome: &ValidationOutcome,
    post_result: Result<ValidationReport, crate::services::epub::EpubError>,
    src_path: &Path,
    original_bytes: &[u8],
    dest_dir: &Path,
) -> Result<FinaliseAction, WritebackError> {
    let err_msg = match &post_result {
        Err(e) => format!("post_writeback_validation_errored: {e}"),
        Ok(report) if is_regression(pre_outcome, &report.outcome) => format!(
            "post_writeback_validation_regressed: pre={:?} post={:?}",
            pre_outcome, report.outcome
        ),
        Ok(_) => return Ok(FinaliseAction::Commit),
    };
    rollback_atomic(src_path, original_bytes, dest_dir)?;
    Ok(FinaliseAction::RolledBack(err_msg))
}

/// Render the path template, move the on-disk file if the rendered path
/// differs from `src_path`, and `UPDATE manifestations.file_path` on
/// success.  Returns the final on-disk path (either `src_path` unchanged
/// or the new location).
///
/// Skipped (no-op) when `config.library_path` is empty — keeps tests that
/// place fixtures outside any library root from triggering renames.
/// Collisions resolve via `path_template::resolve_collision` (numeric
/// suffix), mirroring the ingestion orchestrator's behaviour.
async fn path_rename_step(
    snap: &JobSnapshot,
    config: &Config,
    src_path: PathBuf,
    pool: &PgPool,
) -> Result<PathBuf, WritebackError> {
    let candidate = match render_target_path(snap, &config.library_path, &src_path)? {
        Some(p) => p,
        None => return Ok(src_path),
    };

    if let Some(parent) = candidate.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Resolve collisions deterministically (numeric suffix), mirroring
    // ingestion behaviour.
    let new_path = path_template::resolve_collision(&candidate)?;

    // Validate UTF-8 before the FS rename: a non-UTF-8 rendered path
    // would otherwise orphan the file between the rename and the DB write.
    let new_path_str = new_path
        .to_str()
        .ok_or_else(|| {
            WritebackError::Persist(format!("non-UTF8 rendered path: {}", new_path.display()))
        })?
        .to_owned();

    path_rename::move_existing(&src_path, &new_path)?;

    // If the DB update fails the file is orphaned: `file_path` still
    // points at `src_path` (missing on disk), and the next run_once would
    // skip as `file_missing` — permanently removing the book from the
    // library index. Compensate with a best-effort move-back.
    let update_result = sqlx::query("UPDATE manifestations SET file_path = $1 WHERE id = $2")
        .bind(&new_path_str)
        .bind(snap.manifestation_id)
        .execute(pool)
        .await;

    if let Err(e) = update_result {
        if let Err(re) = path_rename::move_existing(&new_path, &src_path) {
            tracing::error!(
                error = %re,
                original = %src_path.display(),
                attempted = %new_path.display(),
                "writeback: path-rename DB update failed AND compensating move-back failed — on-disk state diverges from DB",
            );
        }
        return Err(WritebackError::Db(e));
    }

    Ok(new_path)
}

/// Pure helper: compute the rendered target path from the snapshot +
/// library root.  Returns `None` when path-rename should be skipped
/// (empty library_path, or rendered path equals current `src_path`).
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
    let candidate = PathBuf::from(library_path).join(&relative);

    if candidate == src_path {
        Ok(None)
    } else {
        Ok(Some(candidate))
    }
}

/// Restore `original_bytes` to `dest` atomically.
///
/// Uses a tempfile in the same directory (for atomic rename semantics) +
/// fsync before persist.  A SIGKILL or power loss between `write` and
/// `persist` leaves the original file intact; only the tempfile is lost.
fn rollback_atomic(
    dest: &Path,
    original_bytes: &[u8],
    dest_dir: &Path,
) -> Result<(), WritebackError> {
    let temp = NamedTempFile::new_in(dest_dir)?;
    std::fs::write(temp.path(), original_bytes)?;
    // fsync the tempfile before rename so the bytes durably hit disk.
    std::fs::File::open(temp.path())?.sync_all()?;
    path_rename::commit(temp, dest)
}

// ── Snapshot load ─────────────────────────────────────────────────────────

async fn load_snapshot(pool: &PgPool, job_id: Uuid) -> Result<JobSnapshot, WritebackError> {
    let row = sqlx::query(
        "SELECT wj.manifestation_id, wj.reason, \
                m.work_id, m.file_path, m.format::text AS format, m.cover_path, \
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
    .ok_or(WritebackError::JobNotFound(job_id))?;

    let work_id: Uuid = row.try_get("work_id")?;
    let primary_author: Option<String> = sqlx::query_scalar(
        "SELECT a.sort_name \
           FROM work_authors wa \
           JOIN authors a ON a.id = wa.author_id \
          WHERE wa.work_id = $1 AND wa.role = 'author' \
          ORDER BY wa.position \
          LIMIT 1",
    )
    .bind(work_id)
    .fetch_optional(pool)
    .await?;

    // `try_get` returns `Ok(None)` for NULL columns; any `Err` here is a
    // real decode problem (type mismatch, bad bytes) — log it instead of
    // silently falling back to `None`, otherwise the writeback reports
    // success with `<dc:date>` left stale.
    let pub_date: Option<time::Date> = match row.try_get("pub_date") {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(
                error = %e,
                %job_id,
                "writeback: pub_date decode failed; proceeding with no date"
            );
            None
        }
    };
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
        primary_author,
    })
}

// ── OPF path + entry helpers ──────────────────────────────────────────────

fn find_opf_path(epub_bytes: &[u8]) -> Result<String, WritebackError> {
    let container_bytes = read_entry_bytes(epub_bytes, "META-INF/container.xml")
        .map_err(|_| WritebackError::MissingOpf)?;
    extract_opf_path(&container_bytes).ok_or(WritebackError::MissingOpf)
}

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
            Event::Empty(e) | Event::Start(e) if e.name().as_ref() == b"rootfile" => {
                if let Some(attr) = e
                    .attributes()
                    .flatten()
                    .find(|a| a.key.as_ref() == b"full-path")
                {
                    match std::str::from_utf8(&attr.value) {
                        Ok(s) => return Some(s.to_string()),
                        Err(decode_err) => {
                            tracing::warn!(
                                error = %decode_err,
                                "writeback: container.xml rootfile@full-path is not valid UTF-8"
                            );
                            return None;
                        }
                    }
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
mod tests {
    use super::*;
    use crate::config::{CleanupMode, CoverConfig, EnrichmentConfig, WritebackConfig};
    use std::io::Write;
    use zip::ZipWriter;
    use zip::write::{ExtendedFileOptions, FileOptions};

    use crate::test_support::db::{ingestion_pool_for, writeback_pool_for};

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
                csp_api_header: String::new(),
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
            // Set enrichment_status = 'complete' so these fixtures don't
            // leak into the enrichment queue's claim_next under parallel
            // test execution (the column defaults to 'pending').
            "INSERT INTO manifestations \
               (work_id, format, file_path, ingestion_file_hash, current_file_hash, \
                file_size_bytes, ingestion_status, validation_status, enrichment_status) \
             VALUES ($1, 'epub'::manifestation_format, $2, $3, $3, 1000, \
                     'complete'::ingestion_status, 'valid'::validation_status, \
                     'complete'::enrichment_status) \
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

    /// Task 16 + Task 24: full run_once on a fixture EPUB whose OPF lives
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
        assert!(
            matches!(outcome, RunOutcome::Success { .. }),
            "run_once should succeed: {:?}",
            outcome
        );

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
    }

    /// Task 24 continuation: two successive writebacks on the same
    /// manifestation.  `ingestion_file_hash` must be constant across
    /// both; `current_file_hash` must change each time.
    #[sqlx::test(migrations = "./migrations")]
    async fn ingestion_file_hash_immutable_across_writeback_chain(pool: PgPool) {
        let app_pool = writeback_pool_for(&pool).await;
        let ing_pool = ingestion_pool_for(&pool).await;
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
    }

    /// Path-rename E2E (Step 8 acceptance criterion): when the rendered
    /// path differs from the on-disk file, run_once must move the file
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
        let author_id: Uuid = sqlx::query_scalar(
            "INSERT INTO authors (name, sort_name) VALUES ($1, $1) RETURNING id",
        )
        .bind(&author_sort)
        .fetch_one(&ing_pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO work_authors (work_id, author_id, role, position) \
             VALUES ($1, $2, 'author', 0)",
        )
        .bind(work_id)
        .bind(author_id)
        .execute(&ing_pool)
        .await
        .unwrap();

        // Set works.title to a value that drives a rename (template
        // renders to a different path than the fixture's tempdir name).
        let new_title = format!("Renamed{marker}");
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

        // Use a config with the lib_dir as library_path so path-rename engages.
        let mut cfg = test_config();
        cfg.library_path = library_root.clone();

        let outcome = run_once(&app_pool, &cfg, job_id).await.unwrap();
        assert!(
            matches!(outcome, RunOutcome::Success { .. }),
            "run_once should succeed: {:?}",
            outcome
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
        let (db_path, current_hash): (String, String) =
            sqlx::query_as("SELECT file_path, current_file_hash FROM manifestations WHERE id = $1")
                .bind(m_id)
                .fetch_one(&app_pool)
                .await
                .unwrap();
        assert_eq!(db_path, expected_new.to_str().unwrap());
        assert_ne!(current_hash, original_hash);
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
        sqlx::query("UPDATE manifestations SET cover_path = $1 WHERE id = $2")
            .bind(pending_path.to_str().unwrap())
            .bind(m_id)
            .execute(&ing_pool)
            .await
            .unwrap();

        let job_id: Uuid = sqlx::query_scalar(
            "INSERT INTO writeback_jobs (manifestation_id, reason) \
             VALUES ($1, 'cover') RETURNING id",
        )
        .bind(m_id)
        .fetch_one(&ing_pool)
        .await
        .unwrap();

        let outcome = run_once(&app_pool, &test_config(), job_id).await.unwrap();
        assert!(
            matches!(outcome, RunOutcome::Success { .. }),
            "cover writeback should succeed: {:?}",
            outcome
        );

        // The cover bytes inside the EPUB match the replacement, not
        // the original.  (Same-media replacement is in-place under the
        // existing manifest href.)
        let new_bytes = std::fs::read(&src_path).unwrap();
        let embedded_cover = read_entry_bytes(&new_bytes, "OEBPS/images/cover.png").unwrap();
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
        let (current, ingestion): (String, String) = sqlx::query_as(
            "SELECT current_file_hash, ingestion_file_hash FROM manifestations WHERE id = $1",
        )
        .bind(m_id)
        .fetch_one(&app_pool)
        .await
        .unwrap();
        assert_ne!(current, original_hash);
        assert_eq!(ingestion, original_hash);
    }

    /// Collision branch of `resolve_collision`: if the rendered target
    /// already exists, the writeback lands at `<stem> (2).<ext>` instead.
    /// Guards against regressions where the suffix logic silently
    /// overwrites an unrelated pre-existing file at the rendered path.
    #[sqlx::test(migrations = "./migrations")]
    async fn run_once_rename_resolves_collision_with_suffix(pool: PgPool) {
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
        let author_id: Uuid = sqlx::query_scalar(
            "INSERT INTO authors (name, sort_name) VALUES ($1, $1) RETURNING id",
        )
        .bind(&author_sort)
        .fetch_one(&ing_pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO work_authors (work_id, author_id, role, position) \
             VALUES ($1, $2, 'author', 0)",
        )
        .bind(work_id)
        .bind(author_id)
        .execute(&ing_pool)
        .await
        .unwrap();

        let new_title = format!("Renamed{marker}");
        sqlx::query("UPDATE works SET title = $1 WHERE id = $2")
            .bind(&new_title)
            .bind(work_id)
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

        let job_id: Uuid = sqlx::query_scalar(
            "INSERT INTO writeback_jobs (manifestation_id, reason) \
             VALUES ($1, 'metadata') RETURNING id",
        )
        .bind(m_id)
        .fetch_one(&ing_pool)
        .await
        .unwrap();

        let mut cfg = test_config();
        cfg.library_path = library_root.clone();
        let outcome = run_once(&app_pool, &cfg, job_id).await.unwrap();
        assert!(
            matches!(outcome, RunOutcome::Success { .. }),
            "run_once should succeed: {:?}",
            outcome
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

        let db_path: String =
            sqlx::query_scalar("SELECT file_path FROM manifestations WHERE id = $1")
                .bind(m_id)
                .fetch_one(&app_pool)
                .await
                .unwrap();
        assert_eq!(
            db_path,
            expected_collision_path.to_str().unwrap(),
            "DB file_path must record the collision-suffixed path"
        );
    }

    // ── Rollback + post-validation decision tests ───────────────────────
    //
    // These exercise the S1 (atomic rollback) and S2 (rollback on
    // validator Err, not just regression) invariants.  Live-regression
    // end-to-end fixtures are covered by the BLUEPRINT manual-smoke
    // checklist — the simple in-test fixtures don't reliably trigger
    // `ValidationOutcome::Quarantined` under `validate_and_repair`.

    fn scratch_with_bytes(bytes: &[u8]) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("book.epub");
        std::fs::write(&path, bytes).unwrap();
        (dir, path)
    }

    /// Rollback restores the original bytes byte-for-byte.
    #[test]
    fn rollback_atomic_restores_original_bytes() {
        let original = b"ORIGINAL EPUB BYTES 1234567890".repeat(16);
        let modified = b"CORRUPT WRITEBACK RESULT".repeat(4);
        let (dir, path) = scratch_with_bytes(&original);
        std::fs::write(&path, &modified).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), modified);

        rollback_atomic(&path, &original, dir.path()).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), original);
    }

    /// Rollback leaves no orphan tempfiles in the destination directory.
    #[test]
    fn rollback_atomic_cleans_up_tempfiles() {
        let original = b"ORIGINAL".to_vec();
        let (dir, path) = scratch_with_bytes(&original);
        std::fs::write(&path, b"OVERWRITTEN").unwrap();

        rollback_atomic(&path, &original, dir.path()).unwrap();

        let remaining: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().into_string().unwrap())
            .collect();
        assert_eq!(
            remaining,
            vec!["book.epub".to_string()],
            "tempfile must not linger"
        );
    }

    fn ok_report(outcome: ValidationOutcome) -> ValidationReport {
        ValidationReport {
            issues: vec![],
            outcome,
            accessibility_metadata: None,
            opf_data: None,
        }
    }

    /// `Ok(Clean)` post-validation → Commit.  No rollback, file unchanged.
    #[test]
    fn finalise_post_writeback_commits_when_clean() {
        let original = b"ORIG".to_vec();
        let written = b"NEW".to_vec();
        let (dir, path) = scratch_with_bytes(&written);

        let action = finalise_post_writeback(
            &ValidationOutcome::Clean,
            Ok(ok_report(ValidationOutcome::Clean)),
            &path,
            &original,
            dir.path(),
        )
        .unwrap();

        assert!(matches!(action, FinaliseAction::Commit));
        assert_eq!(std::fs::read(&path).unwrap(), written, "file untouched");
    }

    /// `Ok(Quarantined)` post-validation → RolledBack.  File restored.
    #[test]
    fn finalise_post_writeback_rolls_back_on_regression() {
        let original = b"ORIG".to_vec();
        let written = b"CORRUPT".to_vec();
        let (dir, path) = scratch_with_bytes(&written);

        let action = finalise_post_writeback(
            &ValidationOutcome::Clean,
            Ok(ok_report(ValidationOutcome::Quarantined)),
            &path,
            &original,
            dir.path(),
        )
        .unwrap();

        match action {
            FinaliseAction::RolledBack(msg) => {
                assert!(msg.contains("regressed"), "msg: {msg}");
            }
            _ => panic!("expected RolledBack, got {:?}", action),
        }
        assert_eq!(std::fs::read(&path).unwrap(), original, "rollback restored");
    }

    /// `Err(EpubError)` post-validation → RolledBack.  This is the S2
    /// branch: a validator error must not leave a corrupted file on disk.
    #[test]
    fn finalise_post_writeback_rolls_back_on_validator_error() {
        use crate::services::epub::EpubError;

        let original = b"ORIG".to_vec();
        let written = b"CORRUPT".to_vec();
        let (dir, path) = scratch_with_bytes(&written);

        let err: Result<ValidationReport, EpubError> =
            Err(EpubError::Io(std::io::Error::other("simulated")));

        let action =
            finalise_post_writeback(&ValidationOutcome::Clean, err, &path, &original, dir.path())
                .unwrap();

        match action {
            FinaliseAction::RolledBack(msg) => {
                assert!(msg.contains("errored"), "msg: {msg}");
                assert!(msg.contains("simulated"), "msg: {msg}");
            }
            _ => panic!("expected RolledBack, got {:?}", action),
        }
        assert_eq!(
            std::fs::read(&path).unwrap(),
            original,
            "validator-err rollback restored"
        );
    }

    // ── Path-rename target rendering ────────────────────────────────────

    fn snap_with(title: Option<&str>, author: Option<&str>) -> JobSnapshot {
        JobSnapshot {
            manifestation_id: Uuid::nil(),
            reason: "metadata".into(),
            file_path: String::new(),
            format: "epub".into(),
            cover_path: None,
            title: title.map(|s| s.to_string()),
            description: None,
            language: None,
            publisher: None,
            pub_date: None,
            isbn_10: None,
            isbn_13: None,
            primary_author: author.map(|s| s.to_string()),
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
        let src = std::path::Path::new("/lib/old/file.epub");
        let target = render_target_path(&snap, "/lib", src).unwrap().unwrap();
        assert_eq!(
            target,
            std::path::PathBuf::from("/lib/Shelley, Mary/Frankenstein.epub")
        );
    }

    #[test]
    fn render_target_path_returns_none_when_unchanged() {
        let snap = snap_with(Some("Frankenstein"), Some("Shelley, Mary"));
        let already = std::path::PathBuf::from("/lib/Shelley, Mary/Frankenstein.epub");
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
        assert_eq!(
            target,
            std::path::PathBuf::from("/lib/Unknown/Unknown.epub")
        );
    }

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
