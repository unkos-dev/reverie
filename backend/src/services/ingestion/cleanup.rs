use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct CleanupResult {
    pub removed_files: usize,
    pub removed_dirs: usize,
}

/// Delete source files after a successful batch, then prune empty parent directories.
///
/// `ingestion_root` bounds directory removal — the root itself is never deleted.
/// Missing files are treated as success (handles TOCTOU races).
pub fn cleanup_batch(
    paths: &[PathBuf],
    ingestion_root: &Path,
) -> Result<CleanupResult, std::io::Error> {
    let mut removed_files = 0;

    for path in paths {
        match std::fs::remove_file(path) {
            Ok(()) => removed_files += 1,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // Already gone — not an error
            }
            Err(e) => return Err(e),
        }
    }

    // Collect unique parent directories, ordered deepest-first for bottom-up removal
    let canonical_root = ingestion_root
        .canonicalize()
        .unwrap_or(ingestion_root.to_path_buf());
    let mut dirs: Vec<PathBuf> = paths
        .iter()
        .filter_map(|p| p.parent().map(|d| d.to_path_buf()))
        .collect();
    dirs.sort();
    dirs.dedup();
    // Sort longest path first (deepest directories first)
    dirs.sort_by_key(|b| std::cmp::Reverse(b.components().count()));

    let mut removed_dirs = 0;
    for dir in &dirs {
        let canonical_dir = dir.canonicalize().unwrap_or(dir.to_path_buf());
        if canonical_dir == canonical_root {
            continue;
        }
        // Only remove if truly empty
        if let Ok(mut entries) = std::fs::read_dir(dir)
            && entries.next().is_none()
        {
            match std::fs::remove_dir(dir) {
                Ok(()) => removed_dirs += 1,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => {
                    tracing::warn!(path = %dir.display(), error = %e, "failed to remove directory during cleanup");
                }
            }
        }
    }

    Ok(CleanupResult {
        removed_files,
        removed_dirs,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleanup_removes_files_and_empty_dirs() {
        let root = tempfile::tempdir().unwrap();
        let sub = root.path().join("author");
        std::fs::create_dir_all(&sub).unwrap();

        let f1 = sub.join("book.epub");
        let f2 = sub.join("book.pdf");
        std::fs::write(&f1, b"1").unwrap();
        std::fs::write(&f2, b"2").unwrap();

        let result = cleanup_batch(&[f1.clone(), f2.clone()], root.path()).unwrap();
        assert_eq!(result.removed_files, 2);
        assert_eq!(result.removed_dirs, 1);
        assert!(!f1.exists());
        assert!(!f2.exists());
        assert!(!sub.exists());
        // Root still exists
        assert!(root.path().exists());
    }

    #[test]
    fn cleanup_missing_file_is_ok() {
        let root = tempfile::tempdir().unwrap();
        let missing = root.path().join("gone.epub");

        let result = cleanup_batch(&[missing], root.path()).unwrap();
        assert_eq!(result.removed_files, 0);
    }

    #[test]
    fn cleanup_preserves_non_empty_dirs() {
        let root = tempfile::tempdir().unwrap();
        let sub = root.path().join("author");
        std::fs::create_dir_all(&sub).unwrap();

        let f1 = sub.join("book.epub");
        let f2 = sub.join("other.epub");
        std::fs::write(&f1, b"1").unwrap();
        std::fs::write(&f2, b"2").unwrap();

        // Only remove f1 — f2 keeps the dir alive
        let result = cleanup_batch(&[f1.clone()], root.path()).unwrap();
        assert_eq!(result.removed_files, 1);
        assert_eq!(result.removed_dirs, 0);
        assert!(sub.exists());
    }
}
