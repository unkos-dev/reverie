//! Managed-library authority and compatibility for stored filesystem paths.

use std::fs::{File, Metadata};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use cap_std::fs::Dir;
use tokio::sync::OnceCell;

/// Shared library root, opened only on an authorised filesystem access.
#[derive(Clone)]
pub struct LibraryFiles {
    inner: Arc<LibraryFilesInner>,
}

struct LibraryFilesInner {
    configured_root: PathBuf,
    root: OnceCell<Arc<LibraryRoot>>,
}

struct LibraryRoot {
    canonical_path: PathBuf,
    dir: Dir,
}

/// One opened file and metadata obtained from that same handle.
pub struct OpenedLibraryFile {
    /// File to transfer into the asynchronous stream without reopening its path.
    pub file: File,
    /// Metadata describing the opened file.
    pub metadata: Metadata,
}

/// Typed failures at the library filesystem boundary.
#[derive(Debug, thiserror::Error)]
pub enum LibraryFileError {
    /// Ambient classification established a target outside the library.
    #[error("file resolves outside the library")]
    OutsideLibrary,
    /// Root acquisition, classification, contained opening or metadata failed.
    #[error("library filesystem operation failed")]
    Io(#[from] io::Error),
    /// Blocking filesystem work could not complete.
    #[error("library filesystem task failed")]
    Task(#[from] tokio::task::JoinError),
}

impl LibraryFiles {
    /// Retain the configured root without accessing the filesystem.
    #[must_use]
    pub fn new(configured_root: impl Into<PathBuf>) -> Self {
        Self {
            inner: Arc::new(LibraryFilesInner {
                configured_root: configured_root.into(),
                root: OnceCell::new(),
            }),
        }
    }

    /// Classify a stored path, then open its target through the pinned root.
    ///
    /// Relative and absolute links resolving inside the root are supported.
    /// Callers must authorise the database lookup before invoking this method.
    ///
    /// # Errors
    /// Returns an established escape, an I/O error or a blocking-task failure.
    /// Failed root acquisition is retried on a later call.
    pub async fn open_download(
        &self,
        stored_path: &Path,
    ) -> Result<OpenedLibraryFile, LibraryFileError> {
        let root = self.root().await?;
        let stored_path = stored_path.to_owned();
        tokio::task::spawn_blocking(move || {
            let relative = root.classify(&stored_path)?;
            root.open(&relative)
        })
        .await?
    }

    async fn root(&self) -> Result<Arc<LibraryRoot>, LibraryFileError> {
        let root = self
            .inner
            .root
            .get_or_try_init(|| async {
                let configured_root = self.inner.configured_root.clone();
                tokio::task::spawn_blocking(move || {
                    if configured_root.as_os_str().is_empty() {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidInput,
                            "library root is empty",
                        ));
                    }
                    let canonical_path = std::fs::canonicalize(configured_root)?;
                    let dir = Dir::open_ambient_dir(&canonical_path, cap_std::ambient_authority())?;
                    Ok(Arc::new(LibraryRoot {
                        canonical_path,
                        dir,
                    }))
                })
                .await?
                .map_err(LibraryFileError::from)
            })
            .await?;
        Ok(Arc::clone(root))
    }
}

impl LibraryRoot {
    fn classify(&self, stored_path: &Path) -> Result<PathBuf, LibraryFileError> {
        let canonical = std::fs::canonicalize(self.canonical_path.join(stored_path))?;
        canonical
            .strip_prefix(&self.canonical_path)
            .map(Path::to_owned)
            .map_err(|_| LibraryFileError::OutsideLibrary)
    }

    fn open(&self, relative: &Path) -> Result<OpenedLibraryFile, LibraryFileError> {
        // THREAT: Classification can race with replacement; only the pinned Dir grants authority to open.
        let file = self.dir.open(relative)?.into_std();
        let metadata = file.metadata()?;
        Ok(OpenedLibraryFile { file, metadata })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    #[tokio::test]
    async fn capability_root_successful_acquisition_reused() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("library");
        std::fs::create_dir(&path).unwrap();
        std::fs::write(path.join("book"), b"original").unwrap();
        let files = LibraryFiles::new(&path);
        let first = files.root().await.unwrap();
        std::fs::rename(&path, tmp.path().join("old-library")).unwrap();
        std::fs::create_dir(&path).unwrap();
        std::fs::write(path.join("book"), b"replacement").unwrap();
        let second = files.clone().root().await.unwrap();
        assert!(Arc::ptr_eq(&first, &second));
        let mut opened = files.open_download(&path.join("book")).await.unwrap();
        let mut bytes = Vec::new();
        opened.file.read_to_end(&mut bytes).unwrap();
        assert_eq!(bytes, b"original");
        assert_eq!(opened.metadata.len(), 8);
    }

    #[tokio::test]
    async fn capability_root_failed_acquisition_recoverable() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("missing");
        let files = LibraryFiles::new(&path);
        assert!(files.inner.root.get().is_none());
        assert!(
            matches!(files.root().await, Err(LibraryFileError::Io(e)) if e.kind() == io::ErrorKind::NotFound)
        );
        assert!(files.inner.root.get().is_none());
        std::fs::create_dir(&path).unwrap();
        std::fs::write(path.join("book"), b"recovered").unwrap();
        let opened = files.open_download(&path.join("book")).await.unwrap();
        assert_eq!(opened.metadata.len(), 9);
        assert!(files.inner.root.get().is_some());
    }

    #[tokio::test]
    async fn capability_root_empty_refused() {
        let files = LibraryFiles::new("");
        assert!(
            matches!(files.root().await, Err(LibraryFileError::Io(e)) if e.kind() == io::ErrorKind::InvalidInput)
        );
        assert!(files.inner.root.get().is_none());
    }

    #[tokio::test]
    async fn capability_root_relative_target_opened() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("book"), b"inside").unwrap();
        let files = LibraryFiles::new(tmp.path());
        let mut opened = files.open_download(Path::new("book")).await.unwrap();
        let mut bytes = Vec::new();
        opened.file.read_to_end(&mut bytes).unwrap();
        assert_eq!(bytes, b"inside");
        assert_eq!(opened.metadata.len(), 6);
    }

    #[tokio::test]
    async fn capability_root_outside_refused() {
        let tmp = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let path = outside.path().join("book");
        std::fs::write(&path, b"outside").unwrap();
        let files = LibraryFiles::new(tmp.path());
        assert!(matches!(
            files.open_download(&path).await,
            Err(LibraryFileError::OutsideLibrary)
        ));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn capability_root_classified_target_swapped_to_escaping_link() {
        let tmp = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let path = tmp.path().join("book");
        let outside_path = outside.path().join("book");
        std::fs::write(&path, b"inside").unwrap();
        std::fs::write(&outside_path, b"outside").unwrap();
        let files = LibraryFiles::new(tmp.path());
        let root = files.root().await.unwrap();
        let relative = root.classify(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        std::os::unix::fs::symlink(&outside_path, &path).unwrap();
        assert!(
            matches!(root.open(&relative), Err(LibraryFileError::Io(e)) if e.kind() == io::ErrorKind::PermissionDenied)
        );
    }
}
