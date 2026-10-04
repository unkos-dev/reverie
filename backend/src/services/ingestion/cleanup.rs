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
    // THREAT: Recheck the owned object immediately before unlink; external replacement remains possible between syscalls.
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
            // THREAT: Only the fixed regular metadata files can be removed while pruning an owned deletion's ancestors.
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
    fn capability_ingestion_cleanup_multiple_sources_preserve_retained_file_and_root() {
        use crate::models::ingestion_input::{Fingerprint, InputPath};
        let directory = tempfile::tempdir().unwrap();
        let root =
            cap_std::fs::Dir::open_ambient_dir(directory.path(), cap_std::ambient_authority())
                .unwrap();
        root.create_dir("author").unwrap();
        root.write("author/book.epub", b"1").unwrap();
        root.write("author/book.pdf", b"2").unwrap();
        for (name, remains) in [("author/book.epub", true), ("author/book.pdf", false)] {
            let path = InputPath::from_path(Path::new(name)).unwrap();
            let fingerprint = Fingerprint::from_metadata(
                &crate::services::ingestion::copier::input_metadata(&root, &path).unwrap(),
            );
            assert!(remove_verified(&root, &path, &fingerprint).unwrap());
            assert_eq!(root.try_exists("author").unwrap(), remains);
        }
        assert!(directory.path().is_dir());
    }

    #[test]
    fn capability_ingestion_cleanup_uncontained_and_missing_sources_preserve_outside_files() {
        use crate::models::ingestion_input::{Fingerprint, InputPath};
        let directory = tempfile::tempdir().unwrap();
        let inside = directory.path().join("ingest");
        let sibling = directory.path().join("ingest-evil");
        std::fs::create_dir(&inside).unwrap();
        std::fs::create_dir(&sibling).unwrap();
        std::fs::write(sibling.join("book.epub"), b"outside").unwrap();
        let root =
            cap_std::fs::Dir::open_ambient_dir(&inside, cap_std::ambient_authority()).unwrap();
        root.write("book.epub", b"inside").unwrap();
        let path = InputPath::from_path(Path::new("book.epub")).unwrap();
        let fingerprint = Fingerprint::from_metadata(
            &crate::services::ingestion::copier::input_metadata(&root, &path).unwrap(),
        );
        for invalid in [
            sibling.join("book.epub"),
            directory.path().join("missing.epub"),
            std::path::PathBuf::from("../ingest-evil/book.epub"),
            std::path::PathBuf::new(),
        ] {
            assert!(InputPath::from_path(&invalid).is_err());
        }
        let absent = InputPath::from_path(Path::new("missing.epub")).unwrap();
        assert!(!remove_verified(&root, &absent, &fingerprint).unwrap());
        assert_eq!(
            std::fs::read(sibling.join("book.epub")).unwrap(),
            b"outside"
        );
        assert_eq!(root.read("book.epub").unwrap(), b"inside");
        assert!(sibling.is_dir());
        assert!(inside.is_dir());
    }
}
