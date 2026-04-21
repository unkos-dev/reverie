//! Render + commit + collision-check for on-disk EPUB path updates.
//!
//! `commit` performs an atomic rename when `src` and `dest` are on the
//! same filesystem.  When the kernel returns EXDEV (or
//! `ErrorKind::CrossesDevices` on newer Rust), it falls back to
//! copy-with-hash-verify + unlink-original.

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use tempfile::NamedTempFile;

use super::error::WritebackError;

/// Persist `temp` onto `dest` atomically on same-FS, or fall back to
/// copy + verify + unlink when crossing filesystem boundaries.
pub fn commit(temp: NamedTempFile, dest: &Path) -> Result<(), WritebackError> {
    match temp.persist(dest) {
        Ok(_) => {
            fsync_parent_dir(dest);
            Ok(())
        }
        Err(err) if is_cross_device(&err.error) => exdev_fallback(err.file, dest),
        Err(err) => Err(WritebackError::Persist(err.error.to_string())),
    }
}

/// fsync the parent directory of `path` so the preceding rename's
/// directory-entry update is durable across a power loss.  POSIX rename
/// is atomic for visibility but does not guarantee the directory
/// metadata is flushed — on ext4/xfs a crash between rename and
/// directory-inode flush can revert the rename.
///
/// Best-effort: a failure here only means durability isn't guaranteed;
/// the rename itself has already committed and Step 11's health sweep
/// will reconcile any post-crash divergence.  Logging the failure is
/// the operator's signal to investigate the underlying FS health.
fn fsync_parent_dir(path: &Path) {
    let Some(parent) = path.parent() else { return };
    // An empty parent means the caller passed a bare filename; fsyncing
    // CWD is almost never what they wanted, so skip.
    if parent.as_os_str().is_empty() {
        return;
    }
    match std::fs::File::open(parent) {
        Ok(dir) => {
            if let Err(e) = dir.sync_all() {
                tracing::warn!(
                    error = %e,
                    parent = %parent.display(),
                    "writeback: parent-dir fsync failed after rename; durability not guaranteed"
                );
            }
        }
        Err(e) => {
            tracing::warn!(
                error = %e,
                parent = %parent.display(),
                "writeback: could not open parent directory for post-rename fsync"
            );
        }
    }
}

fn is_cross_device(e: &std::io::Error) -> bool {
    // Linux 5.x returns ErrorKind::CrossesDevices (stabilised Rust 1.85).
    // Older kernels surface EXDEV via raw_os_error == 18.
    if e.kind() == std::io::ErrorKind::CrossesDevices {
        return true;
    }
    matches!(e.raw_os_error(), Some(18))
}

fn exdev_fallback(temp: NamedTempFile, dest: &Path) -> Result<(), WritebackError> {
    let temp_path = temp.path().to_path_buf();
    let bytes = std::fs::read(&temp_path)?;
    let src_hash = Sha256::digest(&bytes);

    let parent = dest
        .parent()
        .ok_or_else(|| WritebackError::Persist("dest has no parent dir".into()))?;
    let new_temp = NamedTempFile::new_in(parent)?;
    std::fs::write(new_temp.path(), &bytes)?;

    // fsync the new file before rename to ensure bytes are on disk.
    let f = std::fs::File::open(new_temp.path())?;
    f.sync_all()?;

    new_temp
        .persist(dest)
        .map_err(|e| WritebackError::Persist(e.error.to_string()))?;
    fsync_parent_dir(dest);

    // Verify the final file matches what we intended to write.
    let dest_bytes = std::fs::read(dest)?;
    let dest_hash = Sha256::digest(&dest_bytes);
    if dest_hash.as_slice() != src_hash.as_slice() {
        return Err(WritebackError::Persist("post-copy hash mismatch".into()));
    }
    // `temp` dropping will unlink the original temp file at its old path.
    drop(temp);
    Ok(())
}

/// Move an existing on-disk file to `dest`.  Same-FS → atomic rename.
/// Cross-FS (EXDEV) → tempfile-in-dest-dir + persist + unlink-original.
///
/// This is the "rename a file already on disk" sibling of [`commit`]
/// (which takes a [`NamedTempFile`]).  Used by the orchestrator's
/// path-rename step after post-writeback validation passes.
pub fn move_existing(src: &Path, dest: &Path) -> Result<(), WritebackError> {
    match std::fs::rename(src, dest) {
        Ok(_) => {
            fsync_parent_dir(dest);
            // When `src` and `dest` share a parent the two fsyncs collapse
            // to one (same inode); when they don't, flush `src`'s parent
            // too so the unlink side of the rename is durable.
            if src.parent() != dest.parent() {
                fsync_parent_dir(src);
            }
            return Ok(());
        }
        Err(e) if !is_cross_device(&e) => return Err(WritebackError::Io(e)),
        // EXDEV: src + dest sit on different mounts, so std::fs::rename
        // can't perform the atomic same-FS rename.  Fall through to the
        // copy-via-tempfile fallback below.
        Err(_) => {}
    }
    // Cross-FS fallback: copy via a tempfile in dest's dir, then unlink src.
    let parent = dest
        .parent()
        .ok_or_else(|| WritebackError::Persist("dest has no parent dir".into()))?;
    let bytes = std::fs::read(src)?;
    let temp = NamedTempFile::new_in(parent)?;
    std::fs::write(temp.path(), &bytes)?;
    std::fs::File::open(temp.path())?.sync_all()?;
    commit(temp, dest)?;
    std::fs::remove_file(src)?;
    // The `remove_file` of `src` is a directory-metadata change; flush
    // `src`'s parent so the unlink is durable.  `dest`'s parent was
    // already fsync'd inside `commit`.
    fsync_parent_dir(src);
    Ok(())
}

/// Normalise a rendered path: reject `..` components and absolute paths
/// that escape the library root.  Returns the input unchanged if it is
/// already safe.  This is a defensive second line — primary sanitisation
/// happens inside `services::ingestion::path_template::render`.
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
    use std::io::Write;

    #[test]
    fn commit_same_directory_persists() {
        let dir = tempfile::tempdir().unwrap();
        let mut temp = NamedTempFile::new_in(dir.path()).unwrap();
        temp.write_all(b"HELLO").unwrap();
        let dest = dir.path().join("out.epub");
        commit(temp, &dest).unwrap();
        let contents = std::fs::read(&dest).unwrap();
        assert_eq!(contents, b"HELLO");
    }

    #[test]
    fn normalise_rejects_parent_dir() {
        let p = Path::new("../evil.epub");
        assert!(normalise_relative(p).is_err());
    }

    #[test]
    fn normalise_rejects_absolute() {
        let p = Path::new("/etc/passwd");
        assert!(normalise_relative(p).is_err());
    }

    #[test]
    fn normalise_strips_cur_dir() {
        let p = Path::new("./sub/file.epub");
        let out = normalise_relative(p).unwrap();
        assert_eq!(out, PathBuf::from("sub/file.epub"));
    }

    /// `move_existing` performs an atomic rename within the same FS and
    /// removes the source file in the process.
    #[test]
    fn move_existing_same_fs_renames_atomically() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("orig.epub");
        let dest = dir.path().join("subdir/new.epub");
        std::fs::write(&src, b"PAYLOAD").unwrap();
        std::fs::create_dir_all(dest.parent().unwrap()).unwrap();

        move_existing(&src, &dest).unwrap();

        assert!(!src.exists(), "source must be unlinked after move");
        assert!(dest.exists(), "dest must exist after move");
        assert_eq!(std::fs::read(&dest).unwrap(), b"PAYLOAD");
    }

    /// Exercise the EXDEV branch by invoking it directly.  Real cross-FS
    /// testing requires Docker volumes on different mount points; we
    /// validate the fallback's bookkeeping here.
    #[test]
    fn exdev_fallback_writes_same_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let mut temp = NamedTempFile::new_in(dir.path()).unwrap();
        temp.write_all(b"HELLO-EXDEV").unwrap();
        // Pretend this is the cross-FS fallback path.
        let dest = dir.path().join("out-exdev.epub");
        exdev_fallback(temp, &dest).unwrap();
        let contents = std::fs::read(&dest).unwrap();
        assert_eq!(contents, b"HELLO-EXDEV");
    }
}
