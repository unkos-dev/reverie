//! Atomic, integrity-verified file copy from the ingestion drop-zone to the library.
//!
//! Owned library staging separates validation from the source; publication moves
//! accepted bytes directly or verifies a destination copy across filesystems.

use std::fmt::Write as _;

use crate::models::ingestion_input::{Fingerprint, InputPath};
use crate::services::files::RelativeFilePath;
use crate::services::writeback::path_rename;
use cap_std::fs::Dir;
use cap_tempfile::{TempDir, TempFile};
use sha2::{Digest, Sha256};
#[cfg(test)]
use std::io::BufReader;
use std::io::{Read, Write};
use std::io::{Seek, SeekFrom};
#[cfg(test)]
use std::path::Path;
use std::path::PathBuf;
use std::sync::{
    Arc,
    atomic::{AtomicU8, AtomicU64, Ordering},
};
use tokio_util::sync::CancellationToken;

const BUF_SIZE: usize = 64 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub(crate) enum Phase {
    Streaming,
    Validation,
    Publication,
}

#[derive(Clone)]
pub(crate) struct Progress {
    pub(crate) cancel: CancellationToken,
    chunks: Arc<AtomicU64>,
    phase: Arc<AtomicU8>,
    transitions: Arc<AtomicU64>,
}

impl Progress {
    pub(crate) fn new(shutdown: &CancellationToken) -> Self {
        Self {
            cancel: shutdown.child_token(),
            chunks: Arc::new(AtomicU64::new(0)),
            phase: Arc::new(AtomicU8::new(Phase::Streaming as u8)),
            transitions: Arc::new(AtomicU64::new(0)),
        }
    }

    pub(crate) fn count(&self) -> u64 {
        self.chunks.load(Ordering::Relaxed)
    }

    pub(crate) fn phase(&self) -> Phase {
        match self.phase.load(Ordering::Acquire) {
            1 => Phase::Validation,
            2 => Phase::Publication,
            _ => Phase::Streaming,
        }
    }

    pub(crate) fn transitions(&self) -> u64 {
        self.transitions.load(Ordering::Acquire)
    }

    pub(crate) fn check(&self) -> Result<(), CopyError> {
        if self.cancel.is_cancelled() {
            Err(CopyError::Cancelled)
        } else {
            Ok(())
        }
    }

    pub(crate) fn enter(&self, phase: Phase) -> Result<(), CopyError> {
        self.check()?;
        self.phase.store(phase as u8, Ordering::Release);
        self.transitions.fetch_add(1, Ordering::Release);
        Ok(())
    }
}

/// Identity and integrity evidence for a published candidate.
#[derive(Clone, Debug)]
pub struct CopyResult {
    /// Relative location beneath the supplied library capability.
    pub dest_path: PathBuf,
    /// Lowercase hex `SHA-256` digest of the copied bytes, verified against the source.
    pub sha256: String,
    /// File size in bytes, read from source metadata before copying.
    pub file_size: u64,
    /// Device and inode of the published candidate.
    pub identity: (u64, u64),
}

/// Opened acquisition and publication failures.
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
    /// Contained preparation or no-overwrite publication failed.
    #[error("tempfile persist failed: {0}")]
    Persist(#[from] crate::services::writeback::error::WritebackError),
    /// The observed source changed during acquisition.
    #[error("source changed during acquisition")]
    Changed,
    /// Cooperative cancellation observed between chunks.
    #[error("attempt cancelled after no progress")]
    Cancelled,
    /// The source is not a regular file.
    #[error("source is not a regular file")]
    NonRegular,
    /// A destination operation failed.
    #[error("destination I/O error: {0}")]
    DestinationIo(std::io::Error),
    /// The owned candidate requires destination-filesystem staging.
    #[error("publication crosses filesystems: {0}")]
    CrossDevice(#[source] std::io::Error),
    /// Publication may have made the owned final name visible.
    #[error("publication failed: {error}")]
    Publication {
        /// Identity of the independently owned bytes.
        copied: Box<CopyResult>,
        /// Publication or durability failure.
        error: crate::services::writeback::error::WritebackError,
    },
}

pub(crate) fn source_parent(
    root: &Dir,
    path: &InputPath,
) -> std::io::Result<(Dir, std::ffi::OsString)> {
    use rustix::fs::{Mode, OFlags, openat};
    let path = path.path();
    let name = path
        .file_name()
        .ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "source has no basename")
        })?
        .to_owned();
    let mut directory = root.try_clone()?;
    if let Some(parent) = path.parent() {
        for component in parent.components() {
            // THREAT: An external writer must not redirect acquisition through a symlink parent.
            let fd = openat(
                &directory,
                component.as_os_str(),
                OFlags::RDONLY | OFlags::CLOEXEC | OFlags::DIRECTORY | OFlags::NOFOLLOW,
                Mode::empty(),
            )
            .map_err(std::io::Error::from)?;
            directory = Dir::from_std_file(std::fs::File::from(fd));
        }
    }
    Ok((directory, name))
}

pub(crate) fn open_input(root: &Dir, path: &InputPath) -> Result<std::fs::File, CopyError> {
    use rustix::fs::{Mode, OFlags, openat};
    let (parent, name) = source_parent(root, path)?;
    // THREAT: Symlinks and special files cannot confer authority to read outside ingestion.
    let fd = openat(
        &parent,
        &name,
        OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW | OFlags::NONBLOCK,
        Mode::empty(),
    )
    .map_err(std::io::Error::from)?;
    let file = std::fs::File::from(fd);
    if !file.metadata()?.is_file() {
        return Err(CopyError::NonRegular);
    }
    Ok(file)
}

pub(crate) fn input_metadata(root: &Dir, path: &InputPath) -> std::io::Result<std::fs::Metadata> {
    let (parent, name) = source_parent(root, path)?;
    let fd = rustix::fs::openat(
        &parent,
        &name,
        rustix::fs::OFlags::PATH | rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::empty(),
    )
    .map_err(std::io::Error::from)?;
    std::fs::File::from(fd).metadata()
}

pub(crate) struct Candidate {
    staging: Arc<std::sync::Mutex<Option<TempDir>>>,
    source: std::fs::File,
    fingerprint: Fingerprint,
    pub(crate) ingestion_hash: String,
    progress: Progress,
}

enum PreparedStaging {
    Direct(Arc<std::sync::Mutex<Option<TempDir>>>),
    Copied(TempDir),
}

pub(crate) struct Prepared {
    staging: PreparedStaging,
    pub(crate) copied: CopyResult,
}

impl Prepared {
    pub(crate) fn publish(
        self,
        root: &Dir,
        relative: &RelativeFilePath,
    ) -> Result<CopyResult, CopyError> {
        self.publish_with_close(root, relative, TempDir::close)
    }

    fn publish_with_close(
        self,
        root: &Dir,
        relative: &RelativeFilePath,
        close: impl FnOnce(TempDir) -> std::io::Result<()>,
    ) -> Result<CopyResult, CopyError> {
        let (parent, name) = path_rename::parent(root, relative)?;
        let (staging, direct) = match self.staging {
            PreparedStaging::Copied(staging) => (staging, None),
            PreparedStaging::Direct(staging) => staging
                .lock()
                .map_err(|_| std::io::Error::other("candidate staging lock poisoned"))?
                .take()
                .ok_or_else(|| std::io::Error::other("candidate already published"))
                .map(|owned| (owned, Some(staging.clone())))?,
        };
        let result = path_rename::commit_no_replace(
            &staging,
            std::ffi::OsStr::new("candidate.epub"),
            &parent,
            &name,
        );
        let result = match result {
            Ok(path_rename::MoveResult::Durable) => Ok(()),
            Ok(path_rename::MoveResult::VisibleUncertain(error)) | Err(error) => Err(error),
        };
        let result = match (result, direct) {
            (Err(error), Some(direct)) if error.kind() == std::io::ErrorKind::CrossesDevices => {
                *direct
                    .lock()
                    .map_err(|_| std::io::Error::other("candidate staging lock poisoned"))? =
                    Some(staging);
                return Err(CopyError::CrossDevice(error));
            }
            (result, _) => result,
        };
        let close = close(staging);
        if let Err(error) = &close {
            tracing::warn!(kind = ?error.kind(), "ingestion publication staging cleanup failed");
        }
        if let Err(error) = result.and(close) {
            return Err(CopyError::Publication {
                copied: Box::new(self.copied),
                error: error.into(),
            });
        }
        Ok(self.copied)
    }
}

impl Candidate {
    pub(crate) fn prepare(
        &self,
        root: &Dir,
        relative: &RelativeFilePath,
        accepted_hash: &str,
        accepted_size: u64,
        force_copy: bool,
    ) -> Result<Prepared, CopyError> {
        self.progress.enter(Phase::Streaming)?;
        path_rename::prepare_destination(root, relative)?;
        let (parent, name) = path_rename::parent(root, relative)?;
        match parent.symlink_metadata(name) {
            Ok(_) => return Err(std::io::Error::from(std::io::ErrorKind::AlreadyExists).into()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        {
            use cap_std::fs::MetadataExt as _;
            use std::os::unix::fs::MetadataExt;
            let file = self.open()?;
            let metadata = file.metadata()?;
            if !force_copy && metadata.dev() == parent.dir_metadata()?.dev() {
                if metadata.len() != accepted_size {
                    return Err(CopyError::Changed);
                }
                file.sync_all()?;
                return Ok(Prepared {
                    copied: CopyResult {
                        dest_path: relative.as_path().to_owned(),
                        sha256: self.ingestion_hash.clone(),
                        file_size: accepted_size,
                        identity: (metadata.dev(), metadata.ino()),
                    },
                    staging: PreparedStaging::Direct(self.staging.clone()),
                });
            }
        }
        let staging = TempDir::new_in(&parent)?;
        let mut temp = TempFile::new(&staging)?;
        let actual = stream(&mut self.open()?, &mut temp, Some(&self.progress), true)?;
        if actual != accepted_hash || temp.as_file().metadata()?.len() != accepted_size {
            return Err(CopyError::HashMismatch {
                source_hash: accepted_hash.into(),
                dest_hash: actual,
            });
        }
        temp.as_file().sync_all()?;
        temp.replace("candidate.epub")?;
        let mut verify = staging.open("candidate.epub")?.into_std();
        let verified = crate::services::epub::repack::hash_file(&mut verify)?;
        if verified != accepted_hash {
            return Err(CopyError::HashMismatch {
                source_hash: accepted_hash.into(),
                dest_hash: verified,
            });
        }
        let metadata = verify.metadata()?;
        Ok(Prepared {
            copied: CopyResult {
                dest_path: relative.as_path().to_owned(),
                sha256: self.ingestion_hash.clone(),
                file_size: metadata.len(),
                identity: {
                    use std::os::unix::fs::MetadataExt;
                    (metadata.dev(), metadata.ino())
                },
            },
            staging: PreparedStaging::Copied(staging),
        })
    }
    pub(crate) fn progress(&self) -> Progress {
        self.progress.clone()
    }

    pub(crate) fn accepted_bytes(
        &self,
        validation: &Result<crate::services::epub::Validated, crate::services::epub::EpubError>,
    ) -> Result<(String, u64), CopyError> {
        match validation {
            Ok(validated) => Ok(validated
                .rewritten
                .clone()
                .unwrap_or_else(|| (self.ingestion_hash.clone(), self.ingestion_size()))),
            Err(crate::services::epub::EpubError::PublicationUncertain { .. }) => {
                self.progress.enter(Phase::Streaming)?;
                let mut file = self.open()?;
                let hash = stream(&mut file, &mut std::io::sink(), Some(&self.progress), true)?;
                Ok((hash, file.metadata()?.len()))
            }
            Err(_) => Ok((self.ingestion_hash.clone(), self.ingestion_size())),
        }
    }
    pub(crate) const fn ingestion_size(&self) -> u64 {
        self.fingerprint.size
    }
    pub(crate) fn open(&self) -> std::io::Result<std::fs::File> {
        self.staging
            .lock()
            .map_err(|_| std::io::Error::other("candidate staging lock poisoned"))?
            .as_ref()
            .ok_or_else(|| std::io::Error::other("candidate already published"))?
            .open("candidate.epub")
            .map(cap_std::fs::File::into_std)
    }

    pub(crate) fn validate(
        &self,
    ) -> Result<crate::services::epub::Validated, crate::services::epub::EpubError> {
        let file = self.open()?;
        let staging = self
            .staging
            .lock()
            .map_err(|_| std::io::Error::other("candidate staging lock poisoned"))?;
        let result = crate::services::epub::validate_and_repair(
            file,
            staging
                .as_ref()
                .ok_or_else(|| std::io::Error::other("candidate already published"))?,
            std::ffi::OsStr::new("candidate.epub"),
        );
        drop(staging);
        result
    }

    pub(crate) fn verify_source(&self, root: &Dir, path: &InputPath) -> Result<(), CopyError> {
        if Fingerprint::from_metadata(&self.source.metadata()?) != self.fingerprint {
            return Err(CopyError::Changed);
        }
        let current = match open_input(root, path) {
            Ok(current) => current,
            Err(CopyError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
                return Err(CopyError::Changed);
            }
            Err(error) => return Err(error),
        };
        if Fingerprint::from_metadata(&current.metadata()?) != self.fingerprint {
            return Err(CopyError::Changed);
        }
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn publish(
        &self,
        root: &Dir,
        relative: &RelativeFilePath,
    ) -> Result<CopyResult, CopyError> {
        self.publish_with_close(root, relative, TempDir::close)
    }

    #[cfg(test)]
    fn publish_with_close(
        &self,
        root: &Dir,
        relative: &RelativeFilePath,
        close: impl FnOnce(TempDir) -> std::io::Result<()>,
    ) -> Result<CopyResult, CopyError> {
        let mut file = self.open()?;
        let hash = crate::services::epub::repack::hash_file(&mut file)?;
        let size = file.metadata()?.len();
        self.prepare(root, relative, &hash, size, false)?
            .publish_with_close(root, relative, close)
    }

    pub(crate) fn close(self) -> std::io::Result<()> {
        let staging = self
            .staging
            .lock()
            .map_err(|_| std::io::Error::other("candidate staging lock poisoned"))?
            .take();
        if let Some(staging) = staging {
            staging.close()?;
        }
        Ok(())
    }
}

#[cfg(test)]
pub(crate) fn acquire(
    source: &Dir,
    path: &InputPath,
    destination: &Dir,
    relative: &RelativeFilePath,
    expected: &Fingerprint,
) -> Result<Candidate, CopyError> {
    acquire_controlled(
        source,
        path,
        destination,
        relative,
        expected,
        Progress::new(&CancellationToken::new()),
    )
}

pub(crate) fn acquire_controlled(
    source: &Dir,
    path: &InputPath,
    destination: &Dir,
    _relative: &RelativeFilePath,
    expected: &Fingerprint,
    progress: Progress,
) -> Result<Candidate, CopyError> {
    let mut file = open_input(source, path)?;
    let fingerprint = Fingerprint::from_metadata(&file.metadata()?);
    if &fingerprint != expected {
        return Err(CopyError::Changed);
    }
    let staging = TempDir::new_in(destination).map_err(CopyError::DestinationIo)?;
    let mut temp = TempFile::new(&staging).map_err(CopyError::DestinationIo)?;
    let ingestion_hash = stream(&mut file, &mut std::io::sink(), Some(&progress), true)?;
    file.seek(SeekFrom::Start(0))?;
    let streamed = stream(&mut file, &mut temp, Some(&progress), true)?;
    let candidate = Candidate {
        staging: Arc::new(std::sync::Mutex::new(Some({
            temp.replace("candidate.epub")
                .map_err(CopyError::DestinationIo)?;
            staging
        }))),
        source: file,
        fingerprint,
        ingestion_hash,
        progress,
    };
    candidate.verify_source(source, path)?;
    if candidate.ingestion_hash != streamed {
        return Err(CopyError::HashMismatch {
            source_hash: candidate.ingestion_hash,
            dest_hash: streamed,
        });
    }
    Ok(candidate)
}

fn stream(
    reader: &mut impl Read,
    writer: &mut impl Write,
    progress: Option<&Progress>,
    cancellable: bool,
) -> Result<String, CopyError> {
    let mut hasher = Sha256::new();
    let mut buffer = vec![0; BUF_SIZE];
    loop {
        if cancellable && progress.is_some_and(|progress| progress.cancel.is_cancelled()) {
            return Err(CopyError::Cancelled);
        }
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        writer
            .write_all(&buffer[..count])
            .map_err(CopyError::DestinationIo)?;
        hasher.update(&buffer[..count]);
        if let Some(progress) = progress {
            progress.chunks.fetch_add(1, Ordering::Relaxed);
        }
    }
    writer.flush().map_err(CopyError::DestinationIo)?;
    let mut digest = String::with_capacity(64);
    for byte in hasher.finalize() {
        write!(&mut digest, "{byte:02x}").map_err(std::io::Error::other)?;
    }
    Ok(digest)
}

/// Hash a file using streaming `SHA-256` with a 64 KB buffer.
///
/// Returns the lowercase hex digest.
///
/// # Errors
///
/// Returns `std::io::Error` if the file cannot be opened or read.
#[cfg(test)]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_ingestion_publication_uncertain_validation_uses_actual_owned_bytes() {
        let source_dir = tempfile::tempdir().unwrap();
        let library_dir = tempfile::tempdir().unwrap();
        let source =
            Dir::open_ambient_dir(source_dir.path(), cap_std::ambient_authority()).unwrap();
        let library =
            Dir::open_ambient_dir(library_dir.path(), cap_std::ambient_authority()).unwrap();
        source.write("book.epub", b"original").unwrap();
        let path = InputPath::from_path(Path::new("book.epub")).unwrap();
        let fingerprint = Fingerprint::from_metadata(&input_metadata(&source, &path).unwrap());
        let candidate = acquire(
            &source,
            &path,
            &library,
            &"book.epub".parse().unwrap(),
            &fingerprint,
        )
        .unwrap();
        let uncertainty = Err(crate::services::epub::EpubError::PublicationUncertain {
            hash: "unsupported reported digest".into(),
            error: Box::new(crate::services::epub::EpubError::Io(std::io::Error::other(
                "uncertain",
            ))),
        });
        for bytes in [b"original".as_slice(), b"accepted candidate".as_slice()] {
            let staging = candidate.staging.lock().unwrap();
            staging
                .as_ref()
                .unwrap()
                .write("candidate.epub", bytes)
                .unwrap();
            drop(staging);
            let (hash, size) = candidate.accepted_bytes(&uncertainty).unwrap();
            let expected = Sha256::digest(bytes)
                .iter()
                .fold(String::new(), |mut digest, byte| {
                    write!(digest, "{byte:02x}").unwrap();
                    digest
                });
            assert_eq!(hash, expected);
            assert_eq!(size, bytes.len() as u64);
            assert_eq!(source.read("book.epub").unwrap(), b"original");
        }
        candidate
            .staging
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .remove_file("candidate.epub")
            .unwrap();
        assert!(candidate.accepted_bytes(&uncertainty).is_err());
        let error = Err(crate::services::epub::EpubError::Io(std::io::Error::other(
            "validation",
        )));
        assert_eq!(
            candidate.accepted_bytes(&error).unwrap(),
            (candidate.ingestion_hash.clone(), fingerprint.size)
        );
        candidate.close().unwrap();
    }

    #[test]
    fn capability_ingestion_discovery_direct_publication_reuses_candidate_identity() {
        use std::os::unix::fs::MetadataExt;
        let source_dir = tempfile::tempdir().unwrap();
        let library_dir = tempfile::tempdir().unwrap();
        let source =
            Dir::open_ambient_dir(source_dir.path(), cap_std::ambient_authority()).unwrap();
        let library =
            Dir::open_ambient_dir(library_dir.path(), cap_std::ambient_authority()).unwrap();
        source.write("book.epub", b"accepted bytes").unwrap();
        let path = InputPath::from_path(Path::new("book.epub")).unwrap();
        let relative = "book.epub".parse().unwrap();
        let fingerprint = Fingerprint::from_metadata(&input_metadata(&source, &path).unwrap());
        let candidate = acquire(&source, &path, &library, &relative, &fingerprint).unwrap();
        let original = candidate.open().unwrap().metadata().unwrap();
        let count = candidate.progress().count();
        let prepared = candidate
            .prepare(
                &library,
                &relative,
                &candidate.ingestion_hash,
                fingerprint.size,
                false,
            )
            .unwrap();
        assert_eq!(prepared.copied.identity, (original.dev(), original.ino()));
        assert_eq!(candidate.progress().count(), count);
        prepared.publish(&library, &relative).unwrap();
        assert_eq!(library.read("book.epub").unwrap(), b"accepted bytes");
        assert_eq!(source.read("book.epub").unwrap(), b"accepted bytes");
        assert_eq!(library.entries().unwrap().count(), 1);
    }

    #[test]
    fn capability_ingestion_discovery_destination_copy_verifies_owned_bytes() {
        use std::os::unix::fs::MetadataExt;
        let source_dir = tempfile::tempdir().unwrap();
        let library_dir = tempfile::tempdir().unwrap();
        let source =
            Dir::open_ambient_dir(source_dir.path(), cap_std::ambient_authority()).unwrap();
        let library =
            Dir::open_ambient_dir(library_dir.path(), cap_std::ambient_authority()).unwrap();
        source.write("book.epub", b"accepted bytes").unwrap();
        let path = InputPath::from_path(Path::new("book.epub")).unwrap();
        let relative = "book.epub".parse().unwrap();
        let fingerprint = Fingerprint::from_metadata(&input_metadata(&source, &path).unwrap());
        let candidate = acquire(&source, &path, &library, &relative, &fingerprint).unwrap();
        let original = candidate.open().unwrap().metadata().unwrap();
        assert!(matches!(
            candidate.prepare(&library, &relative, &"0".repeat(64), fingerprint.size, true),
            Err(CopyError::HashMismatch { .. })
        ));
        assert_eq!(library.entries().unwrap().count(), 1);
        let prepared = candidate
            .prepare(
                &library,
                &relative,
                &candidate.ingestion_hash,
                fingerprint.size,
                true,
            )
            .unwrap();
        assert_ne!(prepared.copied.identity, (original.dev(), original.ino()));
        prepared.publish(&library, &relative).unwrap();
        candidate.close().unwrap();
        assert_eq!(library.read("book.epub").unwrap(), b"accepted bytes");
        assert_eq!(source.read("book.epub").unwrap(), b"accepted bytes");
        assert_eq!(library.entries().unwrap().count(), 1);
    }

    #[test]
    fn capability_ingestion_review_close_failure_carries_visible_candidate() {
        let source_dir = tempfile::tempdir().unwrap();
        let library_dir = tempfile::tempdir().unwrap();
        let source =
            Dir::open_ambient_dir(source_dir.path(), cap_std::ambient_authority()).unwrap();
        let library =
            Dir::open_ambient_dir(library_dir.path(), cap_std::ambient_authority()).unwrap();
        source.write("book.epub", b"accepted bytes").unwrap();
        let path = InputPath::from_path(Path::new("book.epub")).unwrap();
        let relative = "book.epub".parse().unwrap();
        let fingerprint = Fingerprint::from_metadata(&input_metadata(&source, &path).unwrap());
        let candidate = acquire(&source, &path, &library, &relative, &fingerprint).unwrap();
        let result = candidate.publish_with_close(&library, &relative, |staging| {
            staging.close()?;
            Err(std::io::ErrorKind::PermissionDenied.into())
        });
        assert_eq!(library.read("book.epub").unwrap(), b"accepted bytes");
        assert!(
            matches!(result, Err(CopyError::Publication { .. })),
            "post-publication errors must carry ownership evidence"
        );
    }

    #[tokio::test]
    async fn capability_ingestion_coordinator_blocked_read_retains_candidate_until_closure_returns()
    {
        struct BlockedRead {
            entered: Option<std::sync::mpsc::Sender<()>>,
            release: std::sync::mpsc::Receiver<()>,
            bytes: std::io::Cursor<Vec<u8>>,
        }
        impl Read for BlockedRead {
            fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
                if let Some(entered) = self.entered.take() {
                    entered.send(()).unwrap();
                    self.release.recv().unwrap();
                }
                self.bytes.read(buffer)
            }
        }
        let directory = tempfile::tempdir().unwrap();
        let root = Dir::open_ambient_dir(directory.path(), cap_std::ambient_authority()).unwrap();
        root.write("source.epub", b"preserved source").unwrap();
        let staging_root = root.try_clone().unwrap();
        let (entered, entry) = std::sync::mpsc::channel();
        let (release, blocked) = std::sync::mpsc::channel();
        let shutdown = CancellationToken::new();
        let progress = Progress::new(&shutdown);
        let running_progress = progress.clone();
        let closure = tokio::task::spawn_blocking(move || {
            let staging = TempDir::new_in(&staging_root).unwrap();
            let mut candidate = TempFile::new(&staging).unwrap();
            let mut reader = BlockedRead {
                entered: Some(entered),
                release: blocked,
                bytes: std::io::Cursor::new(vec![7; BUF_SIZE * 2]),
            };
            stream(&mut reader, &mut candidate, Some(&running_progress), true)
        });
        entry.recv().unwrap();
        shutdown.cancel();
        assert!(!closure.is_finished());
        assert_eq!(root.entries().unwrap().count(), 2);
        assert_eq!(progress.count(), 0);
        release.send(()).unwrap();
        assert!(matches!(closure.await.unwrap(), Err(CopyError::Cancelled)));
        assert_eq!(progress.count(), 1);
        assert_eq!(root.entries().unwrap().count(), 1);
        assert_eq!(root.read("source.epub").unwrap(), b"preserved source");
    }

    #[test]
    fn capability_ingestion_coordinator_checks_cancellation_between_chunks_and_counts_completed_chunks()
     {
        struct CancelAfterRead {
            reader: std::io::Cursor<Vec<u8>>,
            cancel: CancellationToken,
        }
        impl Read for CancelAfterRead {
            fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
                let count = self.reader.read(buffer)?;
                self.cancel.cancel();
                Ok(count)
            }
        }
        let progress = Progress::new(&CancellationToken::new());
        let bytes = vec![7; BUF_SIZE * 3];
        let mut reader = CancelAfterRead {
            reader: std::io::Cursor::new(bytes),
            cancel: progress.cancel.clone(),
        };
        let mut destination = Vec::new();
        assert!(matches!(
            stream(&mut reader, &mut destination, Some(&progress), true),
            Err(CopyError::Cancelled)
        ));
        assert_eq!(destination.len(), BUF_SIZE);
        assert_eq!(progress.count(), 1);
    }

    #[test]
    fn capability_ingestion_coordinator_cancelled_acquisition_discards_candidate_preserves_source()
    {
        let source_dir = tempfile::tempdir().unwrap();
        let library_dir = tempfile::tempdir().unwrap();
        let source =
            Dir::open_ambient_dir(source_dir.path(), cap_std::ambient_authority()).unwrap();
        let library =
            Dir::open_ambient_dir(library_dir.path(), cap_std::ambient_authority()).unwrap();
        source.write("book.epub", b"owned source").unwrap();
        let path = InputPath::from_path(Path::new("book.epub")).unwrap();
        let fingerprint = Fingerprint::from_metadata(&input_metadata(&source, &path).unwrap());
        let shutdown = CancellationToken::new();
        let progress = Progress::new(&shutdown);
        shutdown.cancel();
        assert!(matches!(
            acquire_controlled(
                &source,
                &path,
                &library,
                &"book.epub".parse().unwrap(),
                &fingerprint,
                progress
            ),
            Err(CopyError::Cancelled)
        ));
        assert_eq!(source.read("book.epub").unwrap(), b"owned source");
        assert_eq!(library.entries().unwrap().count(), 0);
    }

    #[test]
    fn capability_ingestion_coordinator_publication_stream_is_uninterrupted_and_advances_progress()
    {
        let progress = Progress::new(&CancellationToken::new());
        progress.cancel.cancel();
        let bytes = vec![9; BUF_SIZE * 2 + 1];
        let mut writer = Vec::new();
        stream(
            &mut std::io::Cursor::new(bytes.clone()),
            &mut writer,
            Some(&progress),
            false,
        )
        .unwrap();
        assert_eq!(writer, bytes);
        assert_eq!(progress.count(), 3);
    }

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
