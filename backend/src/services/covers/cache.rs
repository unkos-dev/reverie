//! On-disk cover cache, content-addressed by `current_file_hash` prefix.
//!
//! A writeback (which rewrites `current_file_hash`) naturally evicts stale
//! entries: the next read computes a different key and the old file becomes
//! an orphan for the cache sweep.

use std::path::{Path, PathBuf};

use super::error::CoverError;
use super::resize::CoverSize;
use uuid::Uuid;

/// Filesystem handle for the cover cache directory. All paths are derived
/// relative to `root`; the directory is created on demand rather than at
/// startup so the library path need not exist at process boot.
pub struct CoverCache {
    root: PathBuf,
}

impl CoverCache {
    /// Create a `CoverCache` rooted at `root`. Does not touch the filesystem.
    pub const fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// Create the cache directory (and any missing parents) if it does not
    /// already exist.
    ///
    /// # Errors
    ///
    /// Returns [`CoverError::Io`] if `create_dir_all` fails (e.g. permission
    /// denied or a non-directory inode already occupies the path).
    pub fn ensure_dir(&self) -> Result<(), CoverError> {
        std::fs::create_dir_all(&self.root).map_err(CoverError::Io)
    }

    /// Build a content-addressed path:
    /// `{root}/{manifestation_id}-{hash16}-{size}.{ext}`.
    pub fn cached_path(
        &self,
        manifestation_id: Uuid,
        file_hash_prefix: &str,
        size: CoverSize,
        ext: &str,
    ) -> PathBuf {
        let size_tag = match size {
            CoverSize::Full => "full",
            CoverSize::Thumb => "thumb",
        };
        let prefix: String = file_hash_prefix.chars().take(16).collect();
        self.root
            .join(format!("{manifestation_id}-{prefix}-{size_tag}.{ext}"))
    }

    /// Atomic write: tempfile in the cache dir, then rename. Last-writer-wins
    /// on identical content is benign.
    ///
    /// # Errors
    ///
    /// Returns [`CoverError::Io`] if creating the cache directory, creating
    /// or writing the temporary file, or renaming it into place fails.
    pub fn write_atomic(&self, dest: &Path, bytes: &[u8]) -> Result<(), CoverError> {
        use std::io::Write;
        self.ensure_dir()?;
        let tmp = tempfile::NamedTempFile::new_in(&self.root)?;
        let (mut file, tmp_path) = tmp.into_parts();
        file.write_all(bytes)?;
        file.flush()?;
        drop(file);
        tmp_path
            .persist(dest)
            .map_err(|e| CoverError::Io(e.error))?;
        Ok(())
    }
}
