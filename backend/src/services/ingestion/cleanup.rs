//! Remove unchanged, eligible source files and prune only their ancestors.

pub(crate) fn remove_verified(
    root: &cap_std::fs::Dir,
    path: &crate::models::ingestion_input::InputPath,
    expected: &crate::models::ingestion_input::Fingerprint,
) -> std::io::Result<bool> {
    use crate::models::ingestion_input::Fingerprint;
    use crate::services::ingestion::copier;
    use cap_std::fs::MetadataExt;
    let metadata = match copier::input_metadata(root, path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error),
    };
    if !metadata.is_file() || Fingerprint::from_metadata(&metadata) != *expected {
        return Ok(false);
    }
    let (parent, name) = copier::source_parent(root, path)?;
    let current = parent.symlink_metadata(&name)?;
    if !current.is_file() || (current.dev(), current.ino()) != (expected.device, expected.inode) {
        return Ok(false);
    }
    parent.remove_file(&name)?;
    if let Err(error) = prune_ancestors(root, &path.path()) {
        tracing::warn!(kind = ?error.kind(), "ingestion ancestor pruning stopped");
    }
    Ok(true)
}

fn prune_ancestors(root: &cap_std::fs::Dir, path: &std::path::Path) -> std::io::Result<()> {
    use crate::models::ingestion_input::InputPath;
    use crate::services::ingestion::copier;
    use cap_std::fs::MetadataExt;
    let mut ancestor = path.parent();
    while let Some(path) = ancestor.filter(|path| !path.as_os_str().is_empty()) {
        let location = InputPath::from_path(&path.join("entry"))?;
        let (directory, _) = copier::source_parent(root, &location)?;
        let mut metadata_files = Vec::new();
        for entry in directory.entries()? {
            let entry = entry?;
            let name = entry.file_name();
            if (name != ".DS_Store" && name != "Thumbs.db") || !entry.file_type()?.is_file() {
                return Ok(());
            }
            let metadata = directory.symlink_metadata(&name)?;
            if !metadata.is_file() {
                return Ok(());
            }
            metadata_files.push((name, metadata.dev(), metadata.ino()));
        }
        for (name, device, inode) in metadata_files {
            let metadata = directory.symlink_metadata(&name)?;
            if !metadata.is_file() || (metadata.dev(), metadata.ino()) != (device, inode) {
                return Ok(());
            }
            directory.remove_file(name)?;
        }
        let location = InputPath::from_path(path)?;
        let (parent, name) = copier::source_parent(root, &location)?;
        match parent.remove_dir(name) {
            Ok(()) => ancestor = path.parent(),
            Err(error) if error.kind() == std::io::ErrorKind::DirectoryNotEmpty => break,
            Err(error) => {
                tracing::warn!(kind = ?error.kind(), "ingestion ancestor pruning stopped");
                break;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
#[derive(Debug)]
pub(crate) struct CleanupResult {
    pub(crate) removed_files: usize,
    pub(crate) removed_dirs: usize,
}

#[cfg(test)]
pub(crate) fn cleanup_batch(
    paths: &[std::path::PathBuf],
    ingestion_root: &std::path::Path,
) -> std::io::Result<CleanupResult> {
    use crate::models::ingestion_input::{Fingerprint, InputPath};
    let root = cap_std::fs::Dir::open_ambient_dir(ingestion_root, cap_std::ambient_authority())?;
    let mut result = CleanupResult {
        removed_files: 0,
        removed_dirs: 0,
    };
    for path in paths {
        let Ok(relative) = path.strip_prefix(ingestion_root) else {
            continue;
        };
        let Ok(relative) = InputPath::from_path(relative) else {
            continue;
        };
        let Ok(metadata) = crate::services::ingestion::copier::input_metadata(&root, &relative)
        else {
            continue;
        };
        let ancestors = path
            .ancestors()
            .skip(1)
            .take_while(|parent| *parent != ingestion_root)
            .filter(|parent| parent.is_dir())
            .collect::<Vec<_>>();
        if remove_verified(&root, &relative, &Fingerprint::from_metadata(&metadata))? {
            result.removed_files += 1;
            result.removed_dirs += ancestors.iter().filter(|parent| !parent.exists()).count();
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn capability_ingestion_cleanup_nested_ancestors_fixed_metadata_and_root_preserved() {
        use crate::models::ingestion_input::{Fingerprint, InputPath};
        let root_dir = tempfile::tempdir().unwrap();
        let root =
            cap_std::fs::Dir::open_ambient_dir(root_dir.path(), cap_std::ambient_authority())
                .unwrap();
        root.create_dir_all("a/b").unwrap();
        root.create_dir("unrelated").unwrap();
        root.write("a/b/book.epub", b"source").unwrap();
        root.write("a/b/.DS_Store", b"metadata").unwrap();
        root.write("a/Thumbs.db", b"metadata").unwrap();
        let path = InputPath::from_path(Path::new("a/b/book.epub")).unwrap();
        let fingerprint = Fingerprint::from_metadata(
            &crate::services::ingestion::copier::input_metadata(&root, &path).unwrap(),
        );
        assert!(remove_verified(&root, &path, &fingerprint).unwrap());
        assert!(!root.try_exists("a").unwrap());
        assert!(root.try_exists("unrelated").unwrap());
        assert!(root_dir.path().is_dir());
    }

    #[test]
    fn capability_ingestion_cleanup_sidecars_hidden_files_and_symlinks_preserve_ancestors() {
        use crate::models::ingestion_input::{Fingerprint, InputPath};
        for remaining in [
            "cover.jpg",
            ".private",
            "subdirectory",
            ".DS_Store",
            "Thumbs.db",
        ] {
            let root_dir = tempfile::tempdir().unwrap();
            let outside = tempfile::tempdir().unwrap();
            let root =
                cap_std::fs::Dir::open_ambient_dir(root_dir.path(), cap_std::ambient_authority())
                    .unwrap();
            root.create_dir("a").unwrap();
            root.write("a/book.epub", b"source").unwrap();
            if remaining == "subdirectory" {
                root.create_dir("a/subdirectory").unwrap();
            } else if remaining == ".DS_Store" || remaining == "Thumbs.db" {
                std::fs::write(outside.path().join("target"), b"outside").unwrap();
                std::os::unix::fs::symlink(
                    outside.path().join("target"),
                    root_dir.path().join("a").join(remaining),
                )
                .unwrap();
            } else {
                root.write(Path::new("a").join(remaining), b"retain")
                    .unwrap();
            }
            let path = InputPath::from_path(Path::new("a/book.epub")).unwrap();
            let fingerprint = Fingerprint::from_metadata(
                &crate::services::ingestion::copier::input_metadata(&root, &path).unwrap(),
            );
            assert!(remove_verified(&root, &path, &fingerprint).unwrap());
            assert!(
                root_dir
                    .path()
                    .join("a")
                    .join(remaining)
                    .symlink_metadata()
                    .is_ok()
            );
            assert!(root.try_exists("a").unwrap());
        }
    }

    #[test]
    fn capability_ingestion_cleanup_changed_source_and_outside_symlink_boundary_preserved() {
        use crate::models::ingestion_input::{Fingerprint, InputPath};
        let root_dir = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let root =
            cap_std::fs::Dir::open_ambient_dir(root_dir.path(), cap_std::ambient_authority())
                .unwrap();
        root.write("book.epub", b"source").unwrap();
        let path = InputPath::from_path(Path::new("book.epub")).unwrap();
        let fingerprint = Fingerprint::from_metadata(
            &crate::services::ingestion::copier::input_metadata(&root, &path).unwrap(),
        );
        root.write("book.epub", b"changed bytes").unwrap();
        assert!(!remove_verified(&root, &path, &fingerprint).unwrap());
        std::fs::write(outside.path().join("book.epub"), b"outside").unwrap();
        std::os::unix::fs::symlink(outside.path(), root_dir.path().join("link")).unwrap();
        let escape = InputPath::from_path(Path::new("link/book.epub")).unwrap();
        assert!(remove_verified(&root, &escape, &fingerprint).is_err());
        assert_eq!(
            std::fs::read(outside.path().join("book.epub")).unwrap(),
            b"outside"
        );
        assert_eq!(root.read("book.epub").unwrap(), b"changed bytes");
    }

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
        assert_eq!(result.removed_dirs, 0);
        assert!(stray.exists(), "file outside ingestion root must survive");
        assert!(
            outside.path().exists(),
            "directory outside ingestion root must survive"
        );
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
    fn cleanup_skips_real_file_in_sibling_prefix_directory() {
        // A real file in a sibling whose name extends the root's (`ingest-evil`
        // vs `ingest`) must survive. Locks the file guard to component-wise
        // matching — the same property the dir guard's sibling-prefix test locks.
        let base = tempfile::tempdir().unwrap();
        let root = base.path().join("ingest");
        let evil = base.path().join("ingest-evil");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&evil).unwrap();

        let stray = evil.join("book.epub");
        std::fs::write(&stray, b"keep me").unwrap();

        let result = cleanup_batch(std::slice::from_ref(&stray), &root).unwrap();

        assert_eq!(result.removed_files, 0);
        assert!(stray.exists(), "file in sibling-prefix dir must survive");
    }

    #[cfg(unix)]
    #[test]
    fn cleanup_skips_file_whose_parent_symlinks_outside_root() {
        // A symlinked directory inside the root resolving outside it must not let
        // a file escape the bound. This is the exact scenario the parent
        // canonicalisation exists for: a naive `starts_with` on the raw path
        // would see `root/link/...` as in-tree and delete the external target.
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let target = outside.path().join("real.epub");
        std::fs::write(&target, b"do not delete").unwrap();

        let link_dir = root.path().join("link");
        std::os::unix::fs::symlink(outside.path(), &link_dir).unwrap();

        let path_via_link = link_dir.join("real.epub");
        let result = cleanup_batch(std::slice::from_ref(&path_via_link), root.path()).unwrap();

        assert_eq!(result.removed_files, 0);
        assert!(target.exists(), "real file behind symlink must survive");
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
