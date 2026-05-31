//! Post-ingestion cleanup: remove processed source files and prune empty directories.
//!
//! Cleanup runs only after a batch completes with zero failures, so source files
//! are preserved whenever any ingest step errors. The ingestion root is never
//! deleted — it is the sentinel that bounds upward directory removal.

use std::path::{Path, PathBuf};

/// Counts of filesystem objects removed during a cleanup pass.
#[derive(Debug)]
pub struct CleanupResult {
    /// Number of individual source files successfully deleted.
    pub removed_files: usize,
    /// Number of now-empty parent directories successfully pruned.
    pub removed_dirs: usize,
}

/// Delete source files after a successful batch, then prune empty parent directories.
///
/// `ingestion_root` bounds both file deletion and directory removal: only paths
/// resolving inside the ingestion tree are touched, and the root itself is never
/// deleted. Missing files are treated as success (handles TOCTOU races).
pub fn cleanup_batch(
    paths: &[PathBuf],
    ingestion_root: &Path,
) -> Result<CleanupResult, std::io::Error> {
    // Resolve the ingestion root once; both the file-deletion and
    // directory-pruning loops bound their writes to descendants of it.
    let canonical_root = ingestion_root.canonicalize().unwrap_or_else(|e| {
        // THREAT: a non-canonical root weakens every containment check below
        // (a `..`-bearing path could pass `starts_with`). Surface the degraded
        // mode so an operator can audit, rather than failing silently.
        tracing::warn!(
            path = %ingestion_root.display(),
            error = %e,
            "failed to canonicalize ingestion root; cleanup containment guard may be weakened"
        );
        ingestion_root.to_path_buf()
    });

    let mut removed_files = 0;
    for path in paths {
        // Defence in depth: only delete files whose parent resolves inside the
        // ingestion tree. Canonicalising the parent (not the file) preserves the
        // NotFound-is-success contract — a file is in-tree iff its parent is, and
        // the parent still exists when the file has already been removed.
        // THREAT: arbitrary file deletion if a caller passes an out-of-tree path.
        let parent_in_root = path
            .parent()
            .and_then(|parent| parent.canonicalize().ok())
            .is_some_and(|parent| parent.starts_with(&canonical_root));
        if !parent_in_root {
            tracing::warn!(path = %path.display(), "skipping file outside ingestion root during cleanup");
            continue;
        }
        match std::fs::remove_file(path) {
            Ok(()) => removed_files += 1,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // Already gone — not an error
            }
            Err(e) => return Err(e),
        }
    }

    // Collect unique parent directories, ordered deepest-first for bottom-up removal
    let mut dirs: Vec<PathBuf> = paths
        .iter()
        .filter_map(|p| p.parent().map(std::path::Path::to_path_buf))
        .collect();
    dirs.sort();
    dirs.dedup();
    // Sort longest path first (deepest directories first)
    dirs.sort_by_key(|b| std::cmp::Reverse(b.components().count()));

    let mut removed_dirs = 0;
    for dir in &dirs {
        let Ok(canonical_dir) = dir.canonicalize() else {
            // Cannot confirm containment without a canonical path; skipping is
            // strictly safer than pruning an unverified directory.
            tracing::warn!(path = %dir.display(), "skipping directory that failed to canonicalize during cleanup");
            continue;
        };
        if canonical_dir == canonical_root {
            continue;
        }
        // Defence in depth: never prune a directory outside the ingestion
        // tree, even if a caller passes a stray path. Bounds removal to
        // descendants of the root — blocking both ancestor (upward) and
        // sibling (lateral) paths — regardless of caller correctness.
        // THREAT: arbitrary directory deletion if a path escapes the ingestion root.
        if !canonical_dir.starts_with(&canonical_root) {
            tracing::warn!(path = %dir.display(), "skipping directory outside ingestion root during cleanup");
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
    fn cleanup_skips_directory_outside_ingestion_root() {
        // An empty directory living entirely outside the ingestion root must
        // never be pruned, even when a caller passes a path rooted there.
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let stray = outside.path().join("evil.epub");

        // File never existed (NotFound is treated as success); the parent of
        // `stray` is `outside`, which is empty and outside the root.
        let result = cleanup_batch(std::slice::from_ref(&stray), root.path()).unwrap();

        assert_eq!(result.removed_files, 0);
        assert_eq!(result.removed_dirs, 0);
        assert!(outside.path().exists());
    }

    #[test]
    fn cleanup_skips_sibling_with_shared_name_prefix() {
        // A sibling directory whose name textually extends the root's
        // (`ingest` vs `ingest-evil`) shares a string prefix but is NOT a
        // descendant. Locks the component-wise `starts_with` semantics against
        // a future refactor to a naive string comparison.
        let base = tempfile::tempdir().unwrap();
        let root = base.path().join("ingest");
        let evil = base.path().join("ingest-evil");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&evil).unwrap();

        let stray = evil.join("book.epub");
        let result = cleanup_batch(std::slice::from_ref(&stray), &root).unwrap();

        assert_eq!(result.removed_dirs, 0);
        assert!(evil.exists());
    }

    #[test]
    fn cleanup_skips_existing_file_outside_ingestion_root() {
        // A real file living outside the ingestion root must never be deleted,
        // even when a caller passes its path. Unlike the NotFound case, this
        // file exists on disk, so only a containment guard prevents removal.
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let stray = outside.path().join("evil.epub");
        std::fs::write(&stray, b"keep me").unwrap();

        let result = cleanup_batch(std::slice::from_ref(&stray), root.path()).unwrap();

        assert_eq!(result.removed_files, 0);
        assert!(stray.exists(), "file outside ingestion root must survive");
    }

    #[test]
    fn cleanup_deletes_in_root_file_and_spares_outside_file_in_same_batch() {
        // Per-path guard, not batch-global: an in-root file is removed while an
        // out-of-root file in the same call is left untouched.
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();

        let inside = root.path().join("book.epub");
        let stray = outside.path().join("evil.epub");
        std::fs::write(&inside, b"1").unwrap();
        std::fs::write(&stray, b"2").unwrap();

        let result = cleanup_batch(&[inside.clone(), stray.clone()], root.path()).unwrap();

        assert_eq!(result.removed_files, 1);
        assert!(!inside.exists());
        assert!(stray.exists(), "file outside ingestion root must survive");
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
        let result = cleanup_batch(std::slice::from_ref(&f1), root.path()).unwrap();
        assert_eq!(result.removed_files, 1);
        assert_eq!(result.removed_dirs, 0);
        assert!(sub.exists());
    }
}
