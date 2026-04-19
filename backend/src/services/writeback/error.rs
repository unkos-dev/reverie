//! `WritebackError` — module-boundary error type for the writeback pipeline.
//!
//! Converts freely to `anyhow::Error` for the worker's `Result` return, and
//! thence to `AppError::Internal` at the route boundary via the blanket
//! `From<anyhow::Error>`.  No direct `StatusCode` at handlers.

use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
#[allow(dead_code)] // NoManagedFile / UnsupportedFormat reserved for future orchestrator paths
pub enum WritebackError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("zip: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("xml: {0}")]
    Xml(#[from] quick_xml::Error),
    #[error("epub: {0}")]
    Epub(#[from] crate::services::epub::EpubError),
    #[error("post-writeback validation regressed: {0}")]
    ValidationRegressed(String),
    #[error("path collision: {0}")]
    PathCollision(PathBuf),
    #[error("manifestation has no managed file")]
    NoManagedFile,
    #[error("unsupported format: {0}")]
    UnsupportedFormat(String),
    #[error("missing container.xml or OPF entry")]
    MissingOpf,
    #[error("sqlx: {0}")]
    Db(#[from] sqlx::Error),
    #[error("tempfile persist: {0}")]
    Persist(String),
}
