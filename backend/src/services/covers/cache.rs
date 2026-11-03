//! Capability-based publication of rebuildable, content-addressed raster covers.

use std::fs::File;
use std::io::{self, Seek, Write};

use cap_std::fs::Dir;
use uuid::Uuid;

use super::{CoverArtifact, CoverError, CoverSize};
use crate::services::files::RelativeFilePath;

/// Closed set of raster encodings served from the cache.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CoverEncoding {
    /// JPEG raster.
    Jpeg,
    /// PNG raster.
    Png,
    /// WebP raster.
    Webp,
}

impl CoverEncoding {
    /// Generated filename extension.
    #[must_use]
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Jpeg => "jpg",
            Self::Png => "png",
            Self::Webp => "webp",
        }
    }

    /// Raster response content type.
    #[must_use]
    pub const fn content_type(self) -> &'static str {
        match self {
            Self::Jpeg => "image/jpeg",
            Self::Png => "image/png",
            Self::Webp => "image/webp",
        }
    }
}

impl TryFrom<image::ImageFormat> for CoverEncoding {
    type Error = CoverError;

    fn try_from(format: image::ImageFormat) -> Result<Self, Self::Error> {
        match format {
            image::ImageFormat::Jpeg => Ok(Self::Jpeg),
            image::ImageFormat::Png => Ok(Self::Png),
            image::ImageFormat::WebP => Ok(Self::Webp),
            other => Err(CoverError::UnsupportedFormat(format!("{other:?}"))),
        }
    }
}

/// Cache directory authority retained for one blocking operation.
pub struct CoverCache {
    root: Dir,
}

impl CoverCache {
    /// Open the lazy cache beneath the owning library, creating it only when absent.
    ///
    /// # Errors
    /// Returns an I/O error if contained directory creation or opening fails.
    pub fn new(library: &Dir) -> Result<Self, CoverError> {
        // THREAT: Cache authority comes only from the recorded library's pinned directory.
        let root = match library.open_dir("_covers/cache") {
            Ok(root) => root,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                library.create_dir_all("_covers/cache")?;
                library.open_dir("_covers/cache")?
            }
            Err(error) => return Err(error.into()),
        };
        Ok(Self { root })
    }

    fn basename(
        manifestation_id: Uuid,
        hash: &str,
        size: CoverSize,
        encoding: CoverEncoding,
    ) -> Result<String, CoverError> {
        if hash.len() < 16 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "invalid cover hash").into());
        }
        let tier = match size {
            CoverSize::Full => "full",
            CoverSize::Thumb => "thumb",
        };
        Ok(format!(
            "{manifestation_id}-{}-{tier}.{}",
            &hash[..16],
            encoding.extension()
        ))
    }

    /// Open the first available encoding for this tier without reopening a path.
    ///
    /// # Errors
    /// Returns invalid hash, non-file or contained I/O errors; only absent entries are misses.
    pub fn open(
        &self,
        id: Uuid,
        hash: &str,
        size: CoverSize,
    ) -> Result<Option<CoverArtifact>, CoverError> {
        let encodings: &[CoverEncoding] = match size {
            CoverSize::Thumb => &[CoverEncoding::Jpeg],
            CoverSize::Full => &[CoverEncoding::Jpeg, CoverEncoding::Png, CoverEncoding::Webp],
        };
        for &encoding in encodings {
            let name = Self::basename(id, hash, size, encoding)?;
            match self.root.open(&name) {
                Ok(file) => {
                    if !file.metadata()?.is_file() {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "cover is not a file",
                        )
                        .into());
                    }
                    return Ok(Some(CoverArtifact {
                        file: file.into_std(),
                        encoding,
                        etag: super::etag_for(hash, size),
                    }));
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }
        Ok(None)
    }

    /// Publish complete encoded bytes with identical-content last-writer-wins semantics.
    ///
    /// # Errors
    /// Returns invalid hash or temporary creation, write, flush, clone or replacement errors.
    pub fn publish(
        &self,
        id: Uuid,
        hash: &str,
        size: CoverSize,
        encoding: CoverEncoding,
        bytes: &[u8],
    ) -> Result<CoverArtifact, CoverError> {
        let name = Self::basename(id, hash, size, encoding)?;
        Ok(CoverArtifact {
            file: publish_cover_bytes(&self.root, &name, bytes)?,
            encoding,
            etag: super::etag_for(hash, size),
        })
    }
}

/// Publish a generated basename beneath an opened directory, returning its rewound handle.
///
/// # Errors
/// Returns an invalid basename or temporary-file I/O error; no forced cache sync is performed.
pub(crate) fn publish_cover_bytes(dir: &Dir, basename: &str, bytes: &[u8]) -> io::Result<File> {
    let name: RelativeFilePath = basename
        .parse()
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
    if name.as_path().components().count() != 1 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "cover name must be a basename",
        ));
    }
    let mut temporary = cap_tempfile::TempFile::new(dir)?;
    temporary.write_all(bytes)?;
    temporary.flush()?;
    let mut file = temporary.as_file().try_clone()?.into_std();
    file.rewind()?;
    temporary.replace(basename)?;
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    const HASH: &str = "0123456789abcdef0123456789abcdef";

    fn library(tmp: &tempfile::TempDir) -> Dir {
        Dir::open_ambient_dir(tmp.path(), cap_std::ambient_authority()).unwrap()
    }

    fn bytes(mut artifact: CoverArtifact) -> Vec<u8> {
        let mut bytes = Vec::new();
        artifact.file.read_to_end(&mut bytes).unwrap();
        bytes
    }

    #[cfg(unix)]
    #[test]
    fn capability_cover_cache_outside_symlinks_never_supply_or_mutate_bytes() {
        let tmp = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let library = library(&tmp);
        library.create_dir("_covers").unwrap();
        std::os::unix::fs::symlink(outside.path(), tmp.path().join("_covers/cache")).unwrap();
        assert!(CoverCache::new(&library).is_err());
        assert_eq!(std::fs::read_dir(outside.path()).unwrap().count(), 0);
        library.remove_file("_covers/cache").unwrap();
        let cache = CoverCache::new(&library).unwrap();
        let id = Uuid::new_v4();
        let name = CoverCache::basename(id, HASH, CoverSize::Full, CoverEncoding::Png).unwrap();
        std::fs::write(outside.path().join("foreign"), b"outside").unwrap();
        std::os::unix::fs::symlink(
            outside.path().join("foreign"),
            tmp.path().join("_covers/cache").join(&name),
        )
        .unwrap();
        assert!(cache.open(id, HASH, CoverSize::Full).is_err());
        let artifact = cache
            .publish(id, HASH, CoverSize::Full, CoverEncoding::Png, b"inside")
            .unwrap();
        assert_eq!(bytes(artifact), b"inside");
        assert_eq!(
            std::fs::read(outside.path().join("foreign")).unwrap(),
            b"outside"
        );
    }
}
