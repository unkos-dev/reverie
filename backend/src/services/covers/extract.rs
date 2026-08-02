//! Extract cover bytes from an EPUB on disk. Mirrors Step 5 detection
//! semantics exactly — any divergence would cause enrichment + OPDS to
//! disagree on whether a cover exists.
//!
//! Synchronous. Call from `tokio::task::spawn_blocking`.

use std::path::Path;

use image::ImageFormat;

use super::error::CoverError;
use super::svg;
use crate::services::epub::{container_layer, cover_layer, is_safe_path, opf_layer, zip_layer};

/// Read the EPUB at `epub_path`, locate the cover (EPUB 3
/// `properties="cover-image"`, falling back to the EPUB 2 `<meta
/// name="cover" content="ID"/>` declaration, falling back last to the legacy
/// id heuristic), and return its raw bytes + detected `ImageFormat`.
/// `SVG`-declared covers are rasterized to `PNG` at this boundary. Returns
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
    let opf_dir = opf.opf_path.rfind('/').map_or("", |i| &opf.opf_path[..i]);
    let entry_path = if opf_dir.is_empty() {
        href
    } else {
        format!("{opf_dir}/{href}")
    };

    let bytes = zip_layer::read_entry(&handle, &entry_path).ok_or(CoverError::NoCover)?;

    match image::guess_format(&bytes) {
        Ok(fmt) => Ok((bytes, fmt)),
        // SVG-declared covers (e.g. Standard Ebooks `images/cover.svg`) are not
        // decodable by the raster pipeline. Rasterize to PNG at this boundary;
        // everything downstream already handles PNG. The href resolver is
        // restricted to sibling entries inside this same archive.
        Err(_) if svg::looks_like_svg(&bytes) => {
            let cover_dir = entry_path.rfind('/').map_or("", |i| &entry_path[..i]);
            let png = svg::rasterize_svg(&bytes, |href| {
                let sibling_path = join_sibling_path(cover_dir, href)?;
                zip_layer::read_entry(&handle, &sibling_path)
            })?;
            Ok((png, ImageFormat::Png))
        }
        Err(e) => Err(CoverError::Decode(e.to_string())),
    }
}

/// Join a sibling `href` (referenced from an SVG cover's `<image>`) against the
/// cover's directory inside the EPUB ZIP, re-validating the combined path.
///
/// THREAT: hrefs come from attacker-controlled SVG. The SVG resolver already
/// runs [`is_safe_path`] on the raw `href`; this re-checks the joined
/// `{cover_dir}/{href}` so a future change to how `cover_dir` is derived cannot
/// silently reintroduce path traversal before the ZIP lookup. Returns `None`
/// for any unsafe path.
///
/// `pub(crate)` so [`crate::services::epub::cover_layer::validate`] can resolve
/// siblings the same way this module does: one path-join implementation for
/// both the serve and ingestion routes to a rasterized SVG cover.
pub(crate) fn join_sibling_path(cover_dir: &str, href: &str) -> Option<String> {
    let sibling_path = if cover_dir.is_empty() {
        href.to_owned()
    } else {
        format!("{cover_dir}/{href}")
    };
    is_safe_path(&sibling_path).then_some(sibling_path)
}

#[cfg(test)]
mod tests {
    use super::join_sibling_path;

    #[test]
    fn join_sibling_path_scopes_to_cover_dir() {
        assert_eq!(
            join_sibling_path("OEBPS/images", "cover.jpg").as_deref(),
            Some("OEBPS/images/cover.jpg")
        );
        assert_eq!(
            join_sibling_path("", "cover.jpg").as_deref(),
            Some("cover.jpg")
        );
    }

    #[test]
    fn join_sibling_path_rejects_unsafe_combined() {
        // Defence-in-depth: even if a future cover_dir derivation let an unsafe
        // join through, is_safe_path on the combined path blocks traversal and
        // absolute paths before they reach the ZIP lookup.
        assert!(join_sibling_path("OEBPS", "../secret.png").is_none());
        assert!(join_sibling_path("", "/etc/hostname").is_none());
    }
}
