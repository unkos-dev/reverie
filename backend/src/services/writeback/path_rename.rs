//! Contained relocation with bounded, independently verified EXDEV copying.

use super::error::WritebackError;
use crate::services::epub::repack::hash_file;
use crate::services::files::RelativeFilePath;
use cap_std::fs::Dir;
use cap_tempfile::TempFile;
use sha2::{Digest, Sha256};
use std::ffi::{OsStr, OsString};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// Visible relocation and its durability result.
pub enum MoveResult {
    /// Both directory updates were synchronised.
    Durable,
    /// The file moved, but a directory sync failed.
    VisibleUncertain(std::io::Error),
}

/// Open the actual parent of a checked relative location.
///
/// # Errors
/// Returns contained lookup or missing-basename errors.
pub fn parent(root: &Dir, path: &RelativeFilePath) -> std::io::Result<(Dir, OsString)> {
    let path = path.as_path();
    let directory = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let basename = path
        .file_name()
        .ok_or_else(|| std::io::Error::other("location has no basename"))?;
    Ok((root.open_dir(directory)?, basename.to_owned()))
}

fn is_cross_device(error: &std::io::Error) -> bool {
    error.kind() == std::io::ErrorKind::CrossesDevices || error.raw_os_error() == Some(18)
}

fn persist(temp: TempFile<'_>, parent: &Dir, name: &OsStr) -> Result<(), WritebackError> {
    temp.as_file().sync_all()?;
    temp.replace(name)?;
    parent.open(".")?.sync_all()?;
    Ok(())
}

/// Relocate within one library, retaining the source until an EXDEV destination verifies.
///
/// # Errors
/// Non-EXDEV rename, copy, publication, verification and removal errors preserve the source where it still exists.
pub fn move_existing(
    root: &Dir,
    src: &RelativeFilePath,
    dest: &RelativeFilePath,
    expected_hash: &str,
) -> Result<MoveResult, WritebackError> {
    move_existing_with(
        root,
        src,
        dest,
        expected_hash,
        |source, name, target, destination| source.rename(name, target, destination),
        persist,
    )
}

fn move_existing_with(
    root: &Dir,
    src: &RelativeFilePath,
    dest: &RelativeFilePath,
    expected_hash: &str,
    rename: impl FnOnce(&Dir, &OsStr, &Dir, &OsStr) -> std::io::Result<()>,
    publish: impl FnOnce(TempFile<'_>, &Dir, &OsStr) -> Result<(), WritebackError>,
) -> Result<MoveResult, WritebackError> {
    let (source_parent, source_name) = parent(root, src)?;
    let (dest_parent, dest_name) = parent(root, dest)?;
    match rename(&source_parent, &source_name, &dest_parent, &dest_name) {
        Ok(()) => {
            let sync = dest_parent
                .open(".")
                .and_then(|dir| dir.sync_all())
                .and_then(|()| source_parent.open(".").and_then(|dir| dir.sync_all()));
            return Ok(match sync {
                Ok(()) => MoveResult::Durable,
                Err(error) => MoveResult::VisibleUncertain(error),
            });
        }
        Err(error) if !is_cross_device(&error) => return Err(error.into()),
        Err(_) => {}
    }
    let result = (|| {
        let mut source = source_parent.open(&source_name)?;
        let mut candidate = TempFile::new(&dest_parent)?;
        let mut buffer = vec![0; 64 * 1024];
        let mut hash = Sha256::new();
        loop {
            let n = source.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            candidate.write_all(&buffer[..n])?;
            hash.update(&buffer[..n]);
        }
        let mut copied_hash = String::with_capacity(64);
        for byte in hash.finalize() {
            use std::fmt::Write;
            write!(copied_hash, "{byte:02x}").map_err(std::io::Error::other)?;
        }
        if copied_hash != expected_hash {
            return Err(WritebackError::Persist("source copy hash mismatch".into()));
        }
        publish(candidate, &dest_parent, &dest_name)?;
        let mut destination = dest_parent.open(&dest_name)?.into_std();
        if hash_file(&mut destination)? != expected_hash {
            return Err(WritebackError::Persist("post-copy hash mismatch".into()));
        }
        source_parent.remove_file(&source_name)?;
        Ok(
            match source_parent.open(".").and_then(|dir| dir.sync_all()) {
                Ok(()) => MoveResult::Durable,
                Err(error) => MoveResult::VisibleUncertain(error),
            },
        )
    })();
    if let Err(error) = &result {
        tracing::error!(destination = dest.as_str(), error = %error, "relocation failed; source retained where it still exists, destination may be published");
    }
    result
}

/// Normalise the existing template output before parsing its checked location.
///
/// # Errors
/// Returns an error for parent or absolute components.
pub fn normalise_relative(p: &Path) -> Result<PathBuf, WritebackError> {
    let mut out = PathBuf::new();
    for comp in p.components() {
        match comp {
            std::path::Component::Normal(c) => out.push(c),
            std::path::Component::RootDir | std::path::Component::Prefix(_) => {
                return Err(WritebackError::Persist(format!(
                    "rendered path contains absolute component: {}",
                    p.display()
                )));
            }
            std::path::Component::ParentDir => {
                return Err(WritebackError::Persist(format!(
                    "rendered path contains ..: {}",
                    p.display()
                )));
            }
            std::path::Component::CurDir => {}
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (
        tempfile::TempDir,
        Dir,
        RelativeFilePath,
        RelativeFilePath,
        String,
    ) {
        let dir = tempfile::tempdir().unwrap();
        let root = Dir::open_ambient_dir(dir.path(), cap_std::ambient_authority()).unwrap();
        root.write("orig.epub", b"PAYLOAD").unwrap();
        root.create_dir("subdir").unwrap();
        let mut source = root.open("orig.epub").unwrap().into_std();
        let hash = hash_file(&mut source).unwrap();
        (
            dir,
            root,
            "orig.epub".parse().unwrap(),
            "subdir/new.epub".parse().unwrap(),
            hash,
        )
    }

    #[test]
    fn contained_move_same_fs_renames_atomically() {
        let (_dir, root, src, dest, hash) = fixture();
        assert!(matches!(
            move_existing(&root, &src, &dest, &hash).unwrap(),
            MoveResult::Durable
        ));
        assert!(!root.exists(src.as_path()));
        assert_eq!(root.read(dest.as_path()).unwrap(), b"PAYLOAD");
    }

    #[test]
    fn contained_move_cross_fs_relocates_verified_bytes() {
        let (_dir, root, src, dest, hash) = fixture();
        move_existing_with(
            &root,
            &src,
            &dest,
            &hash,
            |_, _, _, _| Err(std::io::ErrorKind::CrossesDevices.into()),
            persist,
        )
        .unwrap();
        assert!(!root.exists(src.as_path()));
        assert_eq!(root.read(dest.as_path()).unwrap(), b"PAYLOAD");
    }

    #[test]
    fn contained_move_non_exdev_preserves_source() {
        let (_dir, root, src, dest, hash) = fixture();
        let result = move_existing_with(
            &root,
            &src,
            &dest,
            &hash,
            |_, _, _, _| Err(std::io::ErrorKind::PermissionDenied.into()),
            |_, _, _| panic!("non-EXDEV must never copy"),
        );
        assert!(
            matches!(result, Err(WritebackError::Io(error)) if error.kind() == std::io::ErrorKind::PermissionDenied)
        );
        assert_eq!(root.read(src.as_path()).unwrap(), b"PAYLOAD");
        assert!(!root.exists(dest.as_path()));
    }

    #[test]
    fn contained_move_cross_fs_hash_mismatch_preserves_source_and_destination() {
        let (_dir, root, src, dest, hash) = fixture();
        let result = move_existing_with(
            &root,
            &src,
            &dest,
            &hash,
            |_, _, _, _| Err(std::io::ErrorKind::CrossesDevices.into()),
            |temp, parent, name| {
                persist(temp, parent, name)?;
                parent.write(name, b"CORRUPTED")?;
                Ok(())
            },
        );
        assert!(
            matches!(result, Err(WritebackError::Persist(ref reason)) if reason == "post-copy hash mismatch")
        );
        assert_eq!(root.read(src.as_path()).unwrap(), b"PAYLOAD");
        assert_eq!(root.read(dest.as_path()).unwrap(), b"CORRUPTED");
        let retry = "subdir/new (2).epub".parse().unwrap();
        move_existing_with(
            &root,
            &src,
            &retry,
            &hash,
            |_, _, _, _| Err(std::io::ErrorKind::CrossesDevices.into()),
            persist,
        )
        .unwrap();
        assert!(!root.exists(src.as_path()));
        assert_eq!(root.read(dest.as_path()).unwrap(), b"CORRUPTED");
        assert_eq!(root.read(retry.as_path()).unwrap(), b"PAYLOAD");
    }

    #[test]
    fn contained_move_cross_fs_verification_read_error_preserves_source() {
        let (_dir, root, src, dest, hash) = fixture();
        let result = move_existing_with(
            &root,
            &src,
            &dest,
            &hash,
            |_, _, _, _| Err(std::io::ErrorKind::CrossesDevices.into()),
            |temp, parent, name| {
                persist(temp, parent, name)?;
                parent.remove_file(name)?;
                parent.symlink(name, name)?;
                Ok(())
            },
        );
        assert!(matches!(result, Err(WritebackError::Io(_))));
        assert_eq!(root.read(src.as_path()).unwrap(), b"PAYLOAD");
        assert!(
            root.symlink_metadata(dest.as_path())
                .unwrap()
                .file_type()
                .is_symlink()
        );
    }

    #[test]
    fn normalise_rejects_parent_dir() {
        assert!(normalise_relative(Path::new("../evil.epub")).is_err());
    }
    #[test]
    fn normalise_rejects_absolute() {
        assert!(normalise_relative(Path::new("/etc/passwd")).is_err());
    }
    #[test]
    fn normalise_strips_cur_dir() {
        assert_eq!(
            normalise_relative(Path::new("./sub/file.epub")).unwrap(),
            PathBuf::from("sub/file.epub")
        );
    }
}
