use sha2::{Digest, Sha256};
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use tempfile::NamedTempFile;

const BUF_SIZE: usize = 64 * 1024;

#[derive(Debug)]
pub struct CopyResult {
    #[allow(dead_code)] // Used by future callers (e.g. status endpoints)
    pub dest_path: PathBuf,
    pub sha256: String,
    pub file_size: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum CopyError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("SHA-256 mismatch: source_hash={source_hash}, dest_hash={dest_hash}")]
    HashMismatch {
        source_hash: String,
        dest_hash: String,
    },
    #[error("tempfile persist failed: {0}")]
    Persist(#[from] tempfile::PersistError),
}

/// Hash a file using streaming SHA-256 with a 64KB buffer.
/// Returns the lowercase hex digest.
pub fn hash_file(path: &Path) -> Result<String, std::io::Error> {
    let file = std::fs::File::open(path)?;
    let mut reader = BufReader::with_capacity(BUF_SIZE, file);
    let mut hasher = Sha256::new();
    let mut buf = [0u8; BUF_SIZE];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

/// Atomically copy `source` to `dest_dir/dest_relative`, verifying SHA-256 integrity.
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
        format!("{:x}", dest_hasher.finalize())
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
