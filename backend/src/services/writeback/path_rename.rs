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

/// Abort if `dest` already exists AND is not the same file as `src`.
#[allow(dead_code)] // wired in future path-rename iteration; tests exercise it today
pub fn check_collision(src: &Path, dest: &Path) -> Result<(), WritebackError> {
    if !dest.exists() {
        return Ok(());
    }
    match (std::fs::canonicalize(src), std::fs::canonicalize(dest)) {
        (Ok(a), Ok(b)) if a == b => Ok(()),
        _ => Err(WritebackError::PathCollision(dest.to_path_buf())),
    }
}

/// Persist `temp` onto `dest` atomically on same-FS, or fall back to
/// copy + verify + unlink when crossing filesystem boundaries.
pub fn commit(temp: NamedTempFile, dest: &Path) -> Result<(), WritebackError> {
    match temp.persist(dest) {
        Ok(_) => Ok(()),
        Err(err) if is_cross_device(&err.error) => exdev_fallback(err.file, dest),
        Err(err) => Err(WritebackError::Persist(err.error.to_string())),
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

/// Normalise a rendered path: reject `..` components and absolute paths
/// that escape the library root.  Returns the input unchanged if it is
/// already safe.  This is a defensive second line — primary sanitisation
/// happens inside `services::ingestion::path_template::render`.
#[allow(dead_code)] // wired in future path-rename iteration; tests exercise it today
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
    fn check_collision_no_dest() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src.epub");
        std::fs::write(&src, b"x").unwrap();
        let dest = dir.path().join("dest.epub");
        assert!(check_collision(&src, &dest).is_ok());
    }

    #[test]
    fn check_collision_same_path_ok() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("file.epub");
        std::fs::write(&path, b"x").unwrap();
        assert!(check_collision(&path, &path).is_ok());
    }

    #[test]
    fn check_collision_different_content_errors() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src.epub");
        let dest = dir.path().join("dest.epub");
        std::fs::write(&src, b"A").unwrap();
        std::fs::write(&dest, b"B").unwrap();
        assert!(matches!(
            check_collision(&src, &dest),
            Err(WritebackError::PathCollision(_))
        ));
    }

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
