//! Quarantine: moves rejected files out of the ingestion pipeline.
//!
//! Files that fail hashing, copying, or `EPUB` structural validation are moved to a
//! dedicated quarantine directory alongside a `JSON` sidecar that records the original
//! path, the failure reason, and a `RFC 3339` timestamp. Operators can inspect the
//! sidecar to triage failures without re-running ingestion.
//!
//! The quarantine directory is created on demand and is never pruned automatically —
//! operators own the retention policy for quarantined files.

use chrono::{SecondsFormat, Utc};
use std::path::{Path, PathBuf};

/// Move a file to the quarantine directory with a `JSON` sidecar explaining why.
///
/// If the destination filename collides, a `Unix` timestamp suffix is appended to
/// produce a unique name. Falls back to copy-then-delete if rename fails
/// (cross-filesystem move).
///
/// # Errors
///
/// Returns `std::io::Error` if:
/// - the quarantine directory cannot be created,
/// - `source` has no filename component (`InvalidInput`),
/// - the copy fallback fails (rename is tried first and a copy failure is returned),
/// - or the `JSON` sidecar cannot be written.
///
/// A best-effort source deletion failure after a successful copy is logged as a
/// warning but does not cause this function to return an error.
pub fn quarantine_file(
    source: &Path,
    quarantine_dir: &Path,
    reason: &str,
) -> Result<PathBuf, std::io::Error> {
    std::fs::create_dir_all(quarantine_dir)?;

    let filename = source
        .file_name()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "no filename"))?;

    let mut dest = quarantine_dir.join(filename);
    if dest.exists() {
        let now = Utc::now();
        let ts = now.timestamp();
        let stem = dest.file_stem().and_then(|s| s.to_str()).unwrap_or("file");
        let ext = dest.extension().and_then(|e| e.to_str());
        let new_name = ext.map_or_else(|| format!("{stem}_{ts}"), |e| format!("{stem}_{ts}.{e}"));
        dest = quarantine_dir.join(new_name);
    }

    // Try rename first (fast, same filesystem). Fall back to copy+delete.
    if std::fs::rename(source, &dest).is_err() {
        std::fs::copy(source, &dest)?;
        // Best-effort delete of source — if it fails, the file is just in both places
        if let Err(e) = std::fs::remove_file(source) {
            tracing::warn!(path = %source.display(), error = %e, "quarantine: could not remove source after copy; file exists in both locations");
        }
    }

    // Write JSON sidecar
    let sidecar_path = PathBuf::from(format!("{}.quarantine.json", dest.display()));
    let now = Utc::now();
    let sidecar = serde_json::json!({
        "original_path": source.display().to_string(),
        "reason": reason,
        "quarantined_at": now.to_rfc3339_opts(SecondsFormat::AutoSi, true),
    });
    let sidecar_bytes = serde_json::to_string_pretty(&sidecar)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    std::fs::write(&sidecar_path, sidecar_bytes)?;

    Ok(dest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quarantine_moves_file_and_writes_sidecar() {
        let src_dir = tempfile::tempdir().unwrap();
        let q_dir = tempfile::tempdir().unwrap();

        let source = src_dir.path().join("bad.epub");
        std::fs::write(&source, b"corrupted").unwrap();

        let dest = quarantine_file(&source, q_dir.path(), "hash mismatch").unwrap();

        assert!(dest.exists());
        assert!(!source.exists());
        assert_eq!(std::fs::read(&dest).unwrap(), b"corrupted");

        let sidecar = PathBuf::from(format!("{}.quarantine.json", dest.display()));
        assert!(sidecar.exists());
        let content: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&sidecar).unwrap()).unwrap();
        assert_eq!(content["reason"], "hash mismatch");
    }

    #[test]
    fn quarantine_collision_appends_timestamp() {
        let src_dir = tempfile::tempdir().unwrap();
        let q_dir = tempfile::tempdir().unwrap();

        // Pre-create a file in quarantine with the same name
        std::fs::write(q_dir.path().join("bad.epub"), b"old").unwrap();

        let source = src_dir.path().join("bad.epub");
        std::fs::write(&source, b"new").unwrap();

        let dest = quarantine_file(&source, q_dir.path(), "second failure").unwrap();

        assert!(dest.exists());
        // Should have a timestamp suffix, not the original name
        let filename = dest.file_name().unwrap().to_str().unwrap();
        assert!(filename.contains("bad_"), "got {filename}");
    }
}
