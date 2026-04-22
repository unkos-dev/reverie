//! Extract cover bytes from an EPUB on disk. Mirrors Step 5 detection
//! semantics exactly — any divergence would cause enrichment + OPDS to
//! disagree on whether a cover exists.
//!
//! Synchronous. Call from `tokio::task::spawn_blocking`.

use std::path::Path;

use image::ImageFormat;

use super::error::CoverError;
use crate::services::epub::{container_layer, cover_layer, opf_layer, zip_layer};

/// Read the EPUB at `epub_path`, locate the cover via manifest ids, return
/// its raw bytes + the detected `ImageFormat`. Returns
/// [`CoverError::NoCover`] when no cover is declared or the declared file is
/// missing.
pub fn extract_cover_bytes(epub_path: &Path) -> Result<(Vec<u8>, ImageFormat), CoverError> {
    // zip_layer::validate emits issues into a Vec rather than returning them;
    // we need the ZipHandle but don't care about its advisory issues here
    // — the download handler would have bounced a corrupt archive earlier.
    let mut issues = Vec::new();
    let handle = zip_layer::validate(epub_path, &mut issues).map_err(|e| match e {
        crate::services::epub::EpubError::Zip(z) => CoverError::Zip(z),
        crate::services::epub::EpubError::Io(io) => CoverError::Io(io),
        other => CoverError::Decode(other.to_string()),
    })?;

    let opf_path = container_layer::validate(&handle, &mut issues);
    let opf_data = opf_layer::validate(&handle, opf_path.as_deref(), &mut issues);
    let opf = opf_data.ok_or(CoverError::NoCover)?;

    let href = cover_layer::find_cover_href(&opf).ok_or(CoverError::NoCover)?;
    let opf_dir = opf
        .opf_path
        .rfind('/')
        .map(|i| &opf.opf_path[..i])
        .unwrap_or("");
    let entry_path = if opf_dir.is_empty() {
        href.clone()
    } else {
        format!("{opf_dir}/{href}")
    };

    let bytes = zip_layer::read_entry(&handle, &entry_path).ok_or(CoverError::NoCover)?;
    let fmt = image::guess_format(&bytes).map_err(|e| CoverError::Decode(e.to_string()))?;

    Ok((bytes, fmt))
}
