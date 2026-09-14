//! Cover serving errors. Maps to HTTP status at the handler boundary:
//! `NoCover` and `ArchiveRejected` → 404, everything else → 500.

/// All failure modes that can arise when serving a cover image.
///
/// `NoCover` and `ArchiveRejected` map to 404, everything else to 500;
/// variants carry enough context for structured log fields without leaking
/// internals to the client.
#[derive(Debug, thiserror::Error)]
pub enum CoverError {
    /// No servable cover: the EPUB declares none (no `properties="cover-image"`
    /// and no legacy cover id), has no parseable `OPF`, or the declared cover
    /// file is absent from the archive. Maps to 404.
    #[error("no cover")]
    NoCover,
    /// Layer 1 validation rejected the archive structure, so no entry can be
    /// read. Carries the `Debug` rendering of the issue kinds. Maps to 404:
    /// the file is in the library but has no servable cover.
    #[error("archive rejected: {0}")]
    ArchiveRejected(String),
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
    /// Corrupt or unreadable `ZIP`/`EPUB` archive structure (propagated from
    /// the `zip` crate via `#[from]`).
    #[error("zip: {0}")]
    Zip(#[from] zip::result::ZipError),
    /// Underlying filesystem `IO` failure (e.g. cache directory creation,
    /// atomic-write rename) propagated via `#[from]`.
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}
