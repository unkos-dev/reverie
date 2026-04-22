//! Cover serving errors. Maps to HTTP status at the handler boundary:
//! `NoCover` → 404, everything else → 500.

#[derive(Debug, thiserror::Error)]
pub enum CoverError {
    /// EPUB has no cover image per Step 5 detection (no manifest item with
    /// one of the four expected cover ids).
    #[error("no cover")]
    NoCover,
    /// Decoded but the bytes don't form a JPEG/PNG/WebP the `image` crate
    /// can read.
    #[error("decode: {0}")]
    Decode(String),
    /// Database error while looking up the manifestation row or acquiring
    /// the RLS-scoped transaction.
    #[error("db: {0}")]
    Db(String),
    /// Format detected successfully but not one we serve (GIF, BMP, …).
    #[error("unsupported format: {0}")]
    UnsupportedFormat(String),
    #[error("zip: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}
