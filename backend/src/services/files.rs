//! Immutable filesystem capabilities selected by recorded library identity.

use std::collections::HashMap;
use std::fs::{File, Metadata};
use std::io;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;

use cap_std::fs::Dir;

use crate::config::AbsoluteRootPath;
use crate::models::storage_library::LibraryId;

/// A canonical relative location beneath a library root.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RelativeFilePath(String);

impl RelativeFilePath {
    /// The recorded path, without rendering metadata or a naming policy.
    #[must_use]
    pub fn as_path(&self) -> &Path {
        Path::new(&self.0)
    }

    /// The canonical UTF-8 value persisted in the catalogue.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for RelativeFilePath {
    type Err = LibraryFileError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let drive_prefix = value.as_bytes().get(1) == Some(&b':')
            && value
                .as_bytes()
                .first()
                .is_some_and(u8::is_ascii_alphabetic);
        if value.contains('\\')
            || drive_prefix
            || value.split('/').any(|part| matches!(part, "" | "." | ".."))
        {
            return Err(LibraryFileError::InvalidLocation);
        }
        Ok(Self(value.into()))
    }
}

/// A recorded file's owning library and canonical relative path.
#[derive(Clone, Debug)]
pub struct LibraryLocation {
    /// Persistent identity used to select directory authority.
    pub library_id: LibraryId,
    /// Location of the actual file within that library.
    pub path: RelativeFilePath,
}

/// Shared capabilities acquired before serving or starting workers.
#[derive(Clone)]
pub struct LibraryFiles {
    inner: Arc<LibraryFilesInner>,
    #[cfg(test)]
    fixtures: Option<Arc<Vec<tempfile::TempDir>>>,
}

struct LibraryFilesInner {
    libraries: HashMap<LibraryId, Arc<LibraryRoot>>,
    ingestion: Dir,
}

struct LibraryRoot {
    canonical_path: PathBuf,
    dir: Dir,
}

/// One opened file and metadata obtained from that same handle.
pub struct OpenedLibraryFile {
    /// File transferred into the asynchronous stream without reopening its path.
    pub file: File,
    /// Metadata describing the opened file.
    pub metadata: Metadata,
}

/// Typed failures at the library filesystem boundary.
#[derive(Debug, thiserror::Error)]
pub enum LibraryFileError {
    /// Ambient classification established a target outside the owning library.
    #[error("file resolves outside the library")]
    OutsideLibrary,
    /// No capability was bound for the recorded library identity.
    #[error("unknown library identity")]
    UnknownLibrary,
    /// A stored path violates the canonical relative-location contract.
    #[error("invalid library-relative file location")]
    InvalidLocation,
    /// Root acquisition, classification, contained opening or metadata failed.
    #[error("library filesystem operation failed")]
    Io(#[from] io::Error),
    /// Blocking filesystem work could not complete.
    #[error("library filesystem task failed")]
    Task(#[from] tokio::task::JoinError),
}

impl LibraryFiles {
    /// Open provisioned library and staging roots; bindings remain immutable.
    ///
    /// # Errors
    /// Returns an I/O error for unavailable roots or duplicate bindings.
    pub fn open(
        libraries: impl IntoIterator<Item = (LibraryId, AbsoluteRootPath)>,
        ingestion: &AbsoluteRootPath,
    ) -> Result<Self, LibraryFileError> {
        let mut roots = HashMap::new();
        for (id, path) in libraries {
            let root = LibraryRoot::open_root(&path)?;
            if roots.insert(id, Arc::new(root)).is_some() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "duplicate library binding",
                )
                .into());
            }
        }
        Ok(Self {
            inner: Arc::new(LibraryFilesInner {
                libraries: roots,
                ingestion: open_root_dir(ingestion)?,
            }),
            #[cfg(test)]
            fixtures: None,
        })
    }

    /// Directory capability for the recorded library, with no fallback.
    ///
    /// # Errors
    /// Returns an unknown-identity error when no root was bound.
    pub fn library(&self, id: LibraryId) -> Result<&Dir, LibraryFileError> {
        self.inner
            .libraries
            .get(&id)
            .map(|root| &root.dir)
            .ok_or(LibraryFileError::UnknownLibrary)
    }

    /// The opened ingestion drop directory.
    #[must_use]
    pub fn ingestion(&self) -> &Dir {
        &self.inner.ingestion
    }

    /// Open a recorded source from blocking filesystem work.
    ///
    /// # Errors
    /// Returns an unknown identity, established escape or I/O error.
    pub fn open_source(
        &self,
        location: &LibraryLocation,
    ) -> Result<OpenedLibraryFile, LibraryFileError> {
        let root = self
            .inner
            .libraries
            .get(&location.library_id)
            .ok_or(LibraryFileError::UnknownLibrary)?;
        let relative = root.classify(location.path.as_path())?;
        root.open(&relative)
    }

    /// Open a recorded location after the caller authorises its catalogue lookup.
    ///
    /// Relative and absolute links resolving inside the owning root are supported.
    /// Metadata and bytes describe the same opened object.
    ///
    /// # Errors
    /// Returns an unknown identity, established escape, I/O or task error.
    pub async fn open_download(
        &self,
        location: &LibraryLocation,
    ) -> Result<OpenedLibraryFile, LibraryFileError> {
        let root = Arc::clone(
            self.inner
                .libraries
                .get(&location.library_id)
                .ok_or(LibraryFileError::UnknownLibrary)?,
        );
        let path = location.path.clone();
        tokio::task::spawn_blocking(move || {
            let relative = root.classify(path.as_path())?;
            root.open(&relative)
        })
        .await?
    }

    #[cfg(test)]
    pub(crate) fn with_fixtures(mut self, fixtures: Vec<tempfile::TempDir>) -> Self {
        self.fixtures = Some(Arc::new(fixtures));
        self
    }
}

fn open_root_dir(path: &AbsoluteRootPath) -> io::Result<Dir> {
    // THREAT: Ambient authority is granted only for deployment-configured startup roots.
    Dir::open_ambient_dir(path.as_path(), cap_std::ambient_authority()).map_err(|error| {
        io::Error::new(
            error.kind(),
            format!(
                "opening required storage root {}: {error}",
                path.as_path().display()
            ),
        )
    })
}

impl LibraryRoot {
    fn open_root(path: &AbsoluteRootPath) -> io::Result<Self> {
        let canonical_path = std::fs::canonicalize(path.as_path()).map_err(|error| {
            io::Error::new(
                error.kind(),
                format!(
                    "resolving required library root {}: {error}",
                    path.as_path().display()
                ),
            )
        })?;
        let dir = open_root_dir(path)?;
        Ok(Self {
            canonical_path,
            dir,
        })
    }

    fn classify(&self, path: &Path) -> Result<PathBuf, LibraryFileError> {
        let canonical = std::fs::canonicalize(self.canonical_path.join(path))?;
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
    use uuid::Uuid;

    fn root(path: &Path) -> AbsoluteRootPath {
        path.to_str().unwrap().parse().unwrap()
    }

    fn location(id: LibraryId, path: &str) -> LibraryLocation {
        LibraryLocation {
            library_id: id,
            path: path.parse().unwrap(),
        }
    }

    fn files(path: &Path, id: LibraryId) -> LibraryFiles {
        LibraryFiles::open([(id, root(path))], &root(path)).unwrap()
    }

    #[test]
    fn library_storage_root_all_roots_required() {
        let tmp = tempfile::tempdir().unwrap();
        let id = LibraryId::from_uuid(Uuid::new_v4());
        let missing = root(&tmp.path().join("missing"));
        for (library, ingestion) in [
            (missing.clone(), root(tmp.path())),
            (root(tmp.path()), missing),
        ] {
            assert!(matches!(LibraryFiles::open([(id, library)], &ingestion),
                Err(LibraryFileError::Io(e)) if e.kind() == io::ErrorKind::NotFound));
        }
        let files = files(tmp.path(), id);
        assert!(files.library(id).unwrap().dir_metadata().unwrap().is_dir());
        assert!(files.ingestion().dir_metadata().unwrap().is_dir());
    }

    #[test]
    fn library_storage_root_non_directory_refused() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("file");
        std::fs::write(&file, b"file").unwrap();
        let id = LibraryId::from_uuid(Uuid::new_v4());
        for (library, ingestion) in [
            (root(&file), root(tmp.path())),
            (root(tmp.path()), root(&file)),
        ] {
            assert!(matches!(
                LibraryFiles::open([(id, library)], &ingestion),
                Err(LibraryFileError::Io(_))
            ));
        }
    }

    #[test]
    fn library_storage_root_relative_location_rejection() {
        for path in [
            "",
            "/book",
            "book/",
            "a//book",
            "a\\book",
            "C:book",
            ".",
            "..",
            "./book",
            "a/../book",
            "a/./book",
        ] {
            assert!(path.parse::<RelativeFilePath>().is_err(), "{path:?}");
        }
        assert_eq!(
            "Author/Book: edition.epub"
                .parse::<RelativeFilePath>()
                .unwrap()
                .as_str(),
            "Author/Book: edition.epub"
        );
    }

    #[tokio::test]
    async fn library_storage_root_two_libraries_and_unknown_identity() {
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        let a = LibraryId::from_uuid(Uuid::new_v4());
        let b = LibraryId::from_uuid(Uuid::new_v4());
        std::fs::write(first.path().join("book"), b"first").unwrap();
        std::fs::write(second.path().join("book"), b"second").unwrap();
        let files = LibraryFiles::open(
            [(a, root(first.path())), (b, root(second.path()))],
            &root(first.path()),
        )
        .unwrap();
        for (id, expected) in [(a, b"first".as_slice()), (b, b"second".as_slice())] {
            let mut opened = files.open_download(&location(id, "book")).await.unwrap();
            let mut bytes = Vec::new();
            opened.file.read_to_end(&mut bytes).unwrap();
            assert_eq!(bytes, expected);
            assert_eq!(opened.metadata.len(), expected.len() as u64);
        }
        assert!(matches!(
            files
                .open_download(&location(LibraryId::from_uuid(Uuid::new_v4()), "book"))
                .await,
            Err(LibraryFileError::UnknownLibrary)
        ));
        assert!(
            LibraryFiles::open(
                [(a, root(first.path())), (a, root(second.path()))],
                &root(first.path())
            )
            .is_err()
        );
    }

    #[tokio::test]
    async fn library_storage_root_retains_directory_object() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("library");
        std::fs::create_dir(&path).unwrap();
        std::fs::write(path.join("book"), b"original").unwrap();
        let id = LibraryId::from_uuid(Uuid::new_v4());
        let files = files(&path, id);
        std::fs::rename(&path, tmp.path().join("old-library")).unwrap();
        std::fs::create_dir(&path).unwrap();
        std::fs::write(path.join("book"), b"replacement").unwrap();
        let mut opened = files
            .clone()
            .open_download(&location(id, "book"))
            .await
            .unwrap();
        let mut bytes = Vec::new();
        opened.file.read_to_end(&mut bytes).unwrap();
        assert_eq!(bytes, b"original");
        assert_eq!(opened.metadata.len(), 8);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn library_storage_root_classified_target_swap_cannot_escape() {
        let tmp = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let path = tmp.path().join("book");
        let outside_path = outside.path().join("book");
        std::fs::write(&path, b"inside").unwrap();
        std::fs::write(&outside_path, b"outside").unwrap();
        let id = LibraryId::from_uuid(Uuid::new_v4());
        let files = files(tmp.path(), id);
        let root = &files.inner.libraries[&id];
        let relative = root.classify(Path::new("book")).unwrap();
        std::fs::remove_file(&path).unwrap();
        std::os::unix::fs::symlink(&outside_path, &path).unwrap();
        assert!(
            matches!(root.open(&relative), Err(LibraryFileError::Io(e)) if e.kind() == io::ErrorKind::PermissionDenied)
        );
    }
}
