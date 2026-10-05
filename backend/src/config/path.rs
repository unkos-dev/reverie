//! Absolute deployment roots, parsed without accessing the filesystem.

use std::ffi::OsStr;
use std::path::Path;
use std::str::FromStr;

/// An absolute root supplied by deployment configuration.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, schemars::JsonSchema)]
#[serde(transparent)]
#[schemars(inline, with = "String")]
pub struct AbsoluteRootPath(String);

/// A deployment root was empty or relative.
#[derive(Debug, thiserror::Error)]
#[error("must be a non-empty absolute filesystem path")]
pub struct RootPathError;

impl AbsoluteRootPath {
    pub(super) fn defaults() -> [Self; 2] {
        ["/data/library", "/data/ingestion"].map(|path| Self(path.into()))
    }

    /// The configured path without filesystem resolution.
    #[must_use]
    pub fn as_path(&self) -> &Path {
        Path::new(&self.0)
    }

    /// The configured UTF-8 string used by configuration references.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for AbsoluteRootPath {
    type Err = RootPathError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.is_empty() || !Path::new(value).is_absolute() {
            return Err(RootPathError);
        }
        Ok(Self(value.into()))
    }
}

impl<'de> serde::Deserialize<'de> for AbsoluteRootPath {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        value.parse().map_err(serde::de::Error::custom)
    }
}

impl AsRef<Path> for AbsoluteRootPath {
    fn as_ref(&self) -> &Path {
        self.as_path()
    }
}

impl AsRef<OsStr> for AbsoluteRootPath {
    fn as_ref(&self) -> &OsStr {
        self.as_path().as_os_str()
    }
}
