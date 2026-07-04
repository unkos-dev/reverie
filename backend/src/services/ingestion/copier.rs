//! Atomic, integrity-verified file copy from the ingestion drop-zone to the library.
//!
//! Files are written to a `tempfile` on the same filesystem as the destination, then
//! renamed atomically. A `SHA-256` digest is computed inline during the write and
//! compared against the pre-computed source hash — any corruption introduced during
//! the copy causes `copy_verified` to return `CopyError::HashMismatch` before the
//! rename, leaving no partial file in the library directory.

use std::fmt::Write as _;

use sha2::{Digest, Sha256};
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use tempfile::NamedTempFile;

const BUF_SIZE: usize = 64 * 1024;

/// Outcome of a successful [`copy_verified`] call.
#[derive(Debug)]
pub struct CopyResult {
    /// Path of the file as it now exists in the library directory
    /// (`dest_dir.join(dest_relative)`; absolute only if `dest_dir` was
    /// absolute — `copy_verified` does not canonicalise).
    pub dest_path: PathBuf,
    /// Lowercase hex `SHA-256` digest of the copied bytes, verified against the source.
    pub sha256: String,
    /// File size in bytes, read from source metadata before copying.
    pub file_size: u64,
}

/// Errors returned by [`copy_verified`] and [`hash_file`].
#[derive(Debug, thiserror::Error)]
pub enum CopyError {
    /// An underlying I/O failure (open, read, write, rename, or metadata).
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    /// The destination `SHA-256` digest did not match `source_hash` after copying,
    /// indicating corruption in transit. The temp file is discarded automatically.
    #[error("SHA-256 mismatch: source_hash={source_hash}, dest_hash={dest_hash}")]
    HashMismatch {
        /// `SHA-256` digest of the source file (caller-supplied; the value
        /// the copy was meant to reproduce).
        source_hash: String,
        /// `SHA-256` digest computed from the destination bytes during the
        /// streaming write — diverges from `source_hash` on transit corruption.
        dest_hash: String,
    },
    /// `tempfile` failed to rename the temp file to the final destination path.
    #[error("tempfile persist failed: {0}")]
    Persist(#[from] tempfile::PersistError),
}

/// Hash a file using streaming `SHA-256` with a 64 KB buffer.
///
/// Returns the lowercase hex digest.
///
/// # Errors
///
/// Returns `std::io::Error` if the file cannot be opened or read.
pub fn hash_file(path: &Path) -> Result<String, std::io::Error> {
    let file = std::fs::File::open(path)?;
    let mut reader = BufReader::with_capacity(BUF_SIZE, file);
    let mut hasher = Sha256::new();
    #[expect(
        clippy::large_stack_arrays,
        reason = "64 KiB I/O buffer; heap-allocated BufReader wraps it so the size is intentional for throughput"
    )]
    let mut buf = [0u8; BUF_SIZE];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let digest = hasher.finalize();
    Ok(digest
        .iter()
        .fold(String::with_capacity(digest.len() * 2), |mut s, b| {
            write!(s, "{b:02x}").ok();
            s
        }))
}

/// Atomically copy `source` to `dest_dir/dest_relative`, verifying `SHA-256` integrity.
///
/// Accepts a pre-computed `source_hash` to avoid re-reading the source file for hashing.
/// The source is read once (for copying), and the destination bytes are hashed inline
/// during the write. The destination hash is compared against `source_hash` to detect
/// corruption during the copy.
///
/// Algorithm:
/// 1. Create parent directories for dest
/// 2. Create a temp file in `dest_dir` (same filesystem for atomic rename)
/// 3. Copy bytes from source to temp, hashing the destination stream inline
/// 4. Compare dest hash against provided `source_hash`
/// 5. Persist (atomic rename) to final path
///
/// # Errors
///
/// - `CopyError::Io` — source cannot be opened/read, parent directory creation fails,
///   or source metadata cannot be read.
/// - `CopyError::HashMismatch` — the digest of the written bytes does not match
///   `source_hash`; the temp file is discarded before returning.
/// - `CopyError::Persist` — the atomic rename of the temp file to the final path fails.
pub fn copy_verified(
    source: &Path,
    dest_dir: &Path,
    dest_relative: &Path,
    source_hash: &str,
) -> Result<CopyResult, CopyError> {
    let final_path = dest_dir.join(dest_relative);

    // Ensure parent directories exist
    if let Some(parent) = final_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let source_meta = std::fs::metadata(source)?;
    let file_size = source_meta.len();

    // Create temp file in dest_dir (for same-filesystem atomic rename)
    let temp = NamedTempFile::new_in(dest_dir)?;

    let dest_hash = {
        let mut writer = BufWriter::new(&temp);
        let mut reader = BufReader::with_capacity(BUF_SIZE, std::fs::File::open(source)?);
        let mut dest_hasher = Sha256::new();
        #[expect(
            clippy::large_stack_arrays,
            reason = "64 KiB I/O buffer; intentional for throughput"
        )]
        let mut buf = [0u8; BUF_SIZE];

        loop {
            let n = reader.read(&mut buf)?;
            if n == 0 {
                break;
            }
            writer.write_all(&buf[..n])?;
            dest_hasher.update(&buf[..n]);
        }
        writer.flush()?;
        {
            let digest = dest_hasher.finalize();
            digest
                .iter()
                .fold(String::with_capacity(digest.len() * 2), |mut s, b| {
                    write!(s, "{b:02x}").ok();
                    s
                })
        }
    };

    if source_hash != dest_hash {
        // Temp file drops automatically on error
        return Err(CopyError::HashMismatch {
            source_hash: source_hash.to_string(),
            dest_hash,
        });
    }

    // Atomic rename
    temp.persist(&final_path)?;

    Ok(CopyResult {
        dest_path: final_path,
        sha256: dest_hash,
        file_size,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_file_known_content() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.txt");
        std::fs::write(&path, b"hello world").unwrap();
        let hash = hash_file(&path).unwrap();
        // SHA-256 of "hello world"
        assert_eq!(
            hash,
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );
    }

    #[test]
    fn copy_verified_success() {
        let src_dir = tempfile::tempdir().unwrap();
        let dest_dir = tempfile::tempdir().unwrap();

        let source = src_dir.path().join("book.epub");
        std::fs::write(&source, b"epub content here").unwrap();

        let source_hash = hash_file(&source).unwrap();
        let result = copy_verified(
            &source,
            dest_dir.path(),
            Path::new("Author/Title.epub"),
            &source_hash,
        )
        .unwrap();

        assert_eq!(result.dest_path, dest_dir.path().join("Author/Title.epub"));
        assert_eq!(result.file_size, 17);
        assert_eq!(result.sha256, source_hash);

        // Verify contents match
        let dest_content = std::fs::read(&result.dest_path).unwrap();
        assert_eq!(dest_content, b"epub content here");
    }

    #[test]
    fn copy_verified_detects_hash_mismatch() {
        let src_dir = tempfile::tempdir().unwrap();
        let dest_dir = tempfile::tempdir().unwrap();

        let source = src_dir.path().join("book.epub");
        std::fs::write(&source, b"epub content here").unwrap();

        let result = copy_verified(
            &source,
            dest_dir.path(),
            Path::new("Author/Title.epub"),
            "0000000000000000000000000000000000000000000000000000000000000000",
        );

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("mismatch"));
    }

    #[test]
    fn hash_file_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("empty");
        std::fs::write(&path, b"").unwrap();
        let hash = hash_file(&path).unwrap();
        // SHA-256 of empty string
        assert_eq!(
            hash,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }
}
