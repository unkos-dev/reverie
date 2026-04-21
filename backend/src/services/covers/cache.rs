//! On-disk cover cache. Content-addressed by `current_file_hash` prefix so a
//! Step 8 writeback (which rewrites `current_file_hash`) naturally evicts
//! stale entries — the next read computes a different key and the old file
//! becomes an orphan for Step 11 to sweep.

use std::path::{Path, PathBuf};

use super::error::CoverError;
use super::resize::CoverSize;
use uuid::Uuid;

pub struct CoverCache {
    root: PathBuf,
}

impl CoverCache {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

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
