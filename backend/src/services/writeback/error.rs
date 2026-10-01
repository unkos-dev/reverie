//! `WritebackError` — module-boundary error type for the writeback pipeline.
//!
//! Converts freely to `anyhow::Error` for the worker's `Result` return, and
//! thence to `AppError::Internal` at the route boundary via the blanket
//! `From<anyhow::Error>`.  No direct `StatusCode` at handlers.

/// All failure modes that can abort a writeback job.
///
/// Implements `From` for `std::io::Error`, `zip::result::ZipError`,
/// `quick_xml::Error`, `EpubError`, and `sqlx::Error` so callers can
/// propagate with `?` at each pipeline stage without per-stage wrapping.
#[derive(Debug, thiserror::Error)]
pub enum WritebackError {
    /// An `std::io` operation failed (reads, writes, rename, fsync).
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    /// A `zip` archive operation failed (open, entry read, repack write).
    #[error("zip: {0}")]
    Zip(#[from] zip::result::ZipError),
    /// A `quick_xml` parse or write event failed.
    #[error("xml: {0}")]
    Xml(#[from] quick_xml::Error),
    /// EPUB inspection, candidate validation or publication failed.
    #[error("epub: {0}")]
    Epub(#[from] crate::services::epub::EpubError),
    /// Cover planning refused an invalid transformation before publication.
    #[error("writeback transformation rejected: {0}")]
    ValidationRegressed(String),
    /// `META-INF/container.xml` was absent or contained no `OPF` root-file path.
    #[error("missing container.xml or OPF entry")]
    MissingOpf,
    /// The `writeback_jobs` row for the given `UUID` does not exist (e.g.
    /// CASCADE-deleted when the parent manifestation was removed).
    #[error("writeback job {0} not found")]
    JobNotFound(uuid::Uuid),
    /// A `sqlx` database operation failed.
    #[error("sqlx: {0}")]
    Db(#[from] sqlx::Error),
    /// Recorded library location cannot be opened.
    #[error("library: {0}")]
    Library(#[from] crate::services::files::LibraryFileError),
    /// Blocking filesystem work failed to complete.
    #[error("blocking writeback: {0}")]
    Task(#[from] tokio::task::JoinError),
    /// A tempfile persist, path-render, or cross-filesystem copy failed;
    /// the human-readable cause is in the wrapped `String`.
    #[error("tempfile persist: {0}")]
    Persist(String),
}
