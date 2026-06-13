//! SVG cover rasterization (Tier 2 — parses untrusted XML from user EPUBs).
//!
//! Standard Ebooks EPUBs declare their cover as `images/cover.svg`
//! (`media-type="image/svg+xml"`). The raster pipeline cannot decode SVG, so
//! this module sniffs SVG at the extraction boundary and rasterizes it to PNG
//! via `resvg`/`usvg`/`tiny-skia`. The PNG then flows through the existing
//! resize/cache/serve path unchanged — **no SVG bytes ever leave the server**,
//! so there is no stored-XSS surface on the cover route.
//!
//! THREAT: input is attacker-controlled XML pulled from an uploaded EPUB.
//! Defenses, all enforced here:
//! - **XXE**: `usvg` parses with `roxmltree`, which performs *zero* IO —
//!   external entities are never fetched. `usvg` hard-codes `allow_dtd: true`
//!   (verified in pinned 0.47 source), so DTDs are *parsed*, not rejected;
//!   internal entity expansion is loop/depth-guarded by `roxmltree`, and the
//!   `MAX_SVG_INPUT_BYTES` cap bounds expansion input as a belt.
//! - **Local file read via `<image href>`**: `usvg`'s default `resolve_string`
//!   reads local files. We override it to resolve only sibling ZIP entries
//!   (path-traversal-checked) and override `resolve_data` to bound base64
//!   data-URI payloads. No `resources_dir` is set.
//! - **Decode bombs**: `resvg`'s raster decoders have no built-in limits
//!   (resvg#647). Every embedded raster (sibling *and* data-URI) is bounded by
//!   byte size (`MAX_EMBEDDED_IMAGE_BYTES`) and header-sniffed megapixels
//!   (`MAX_EMBEDDED_PIXELS`) before decode.
//! - **Output-size bomb**: render output is capped at `MAX_RENDER_LONG_EDGE`;
//!   `Pixmap::new` returning `None` (zero/oversize) maps to a decode error.
//! - **Silent transparent cover**: `usvg` drops unresolvable `<image>` nodes
//!   and "succeeds" with a blank pixmap. We reject all-transparent renders so
//!   the existing `<img onError>` spine fallback is preserved.
//!
//! See ADR `2026-06-13-svg-cover-rasterization.md`.

use std::sync::Arc;

use resvg::{tiny_skia, usvg};

use super::error::CoverError;

/// Hard cap on raw SVG input. Bounds DTD entity-expansion input and base64
/// data-URI payloads (which live inside the SVG bytes).
const MAX_SVG_INPUT_BYTES: usize = 4 * 1024 * 1024;
/// Hard cap on a single embedded raster (sibling entry or decoded data-URI).
const MAX_EMBEDDED_IMAGE_BYTES: usize = 8 * 1024 * 1024;
/// Hard cap on an embedded raster's pixel count (decode-bomb guard).
const MAX_EMBEDDED_PIXELS: u64 = 16 * 1024 * 1024;
/// Long-edge cap of the rasterized output, in pixels. Matches `CoverSize::Full`
/// so the downstream resize step is a near-no-op.
const MAX_RENDER_LONG_EDGE: u32 = 1200;

/// Cheap byte-prefix sniff for SVG. Skips a UTF-8 BOM and leading ASCII
/// whitespace, then matches `<?xml` or `<svg` (case-insensitive for the tag).
///
/// Deliberately rejects gzip-compressed `.svgz` (magic `1f 8b`): keeping it out
/// of the rasterize path preserves today's behaviour for those inputs.
pub(crate) fn looks_like_svg(bytes: &[u8]) -> bool {
    let mut b = bytes;
    if let Some(rest) = b.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        b = rest;
    }
    let start = b
        .iter()
        .position(|c| !c.is_ascii_whitespace())
        .unwrap_or(b.len());
    let b = &b[start..];
    starts_with_ci(b, b"<?xml") || starts_with_ci(b, b"<svg")
}

fn starts_with_ci(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.len() >= needle.len() && haystack[..needle.len()].eq_ignore_ascii_case(needle)
}

/// Rasterize `svg_bytes` to PNG, resolving any `<image href>` siblings via
/// `resolve_sibling` (returns the raw bytes of a sibling entry, or `None`).
///
/// # Errors
///
/// Returns [`CoverError::Decode`] for: oversized input, unparsable SVG,
/// non-positive or unallocatable render dimensions, PNG-encode failure, or an
/// all-transparent render (so the caller's spine fallback still fires).
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "render dimensions are non-negative and bounded to MAX_RENDER_LONG_EDGE before the cast"
)]
pub(crate) fn rasterize_svg<F>(svg_bytes: &[u8], resolve_sibling: F) -> Result<Vec<u8>, CoverError>
where
    F: Fn(&str) -> Option<Vec<u8>> + Send + Sync,
{
    // THREAT: bound input before any parsing — caps entity-expansion and
    // data-URI payload size.
    if svg_bytes.len() > MAX_SVG_INPUT_BYTES {
        return Err(CoverError::Decode(format!(
            "svg input {} exceeds {MAX_SVG_INPUT_BYTES} byte cap",
            svg_bytes.len()
        )));
    }

    let opt = usvg::Options {
        image_href_resolver: hardened_resolver(resolve_sibling),
        ..Default::default()
    };

    let tree = usvg::Tree::from_data(svg_bytes, &opt)
        .map_err(|e| CoverError::Decode(format!("svg parse: {e}")))?;

    let size = tree.size();
    let long = size.width().max(size.height());
    if !long.is_finite() || long <= 0.0 {
        return Err(CoverError::Decode("svg has non-positive size".to_owned()));
    }

    // Render at the long-edge cap regardless of the SVG's native size — SVG is
    // resolution-independent, and this bounds output allocation no matter how
    // large the declared viewBox is.
    let scale = f32::from(u16::try_from(MAX_RENDER_LONG_EDGE).unwrap_or(u16::MAX)) / long;
    let width = ((size.width() * scale).round() as u32).max(1);
    let height = ((size.height() * scale).round() as u32).max(1);

    let mut pixmap = tiny_skia::Pixmap::new(width, height)
        .ok_or_else(|| CoverError::Decode(format!("pixmap alloc failed for {width}x{height}")))?;

    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );

    // THREAT: usvg silently drops unresolvable <image> nodes and "succeeds"
    // with a transparent pixmap. Serving that as 200 would kill the spine
    // fallback. Reject all-transparent output.
    if !pixmap.data().chunks_exact(4).any(|px| px[3] != 0) {
        return Err(CoverError::Decode(
            "svg rendered no visible content".to_owned(),
        ));
    }

    pixmap
        .encode_png()
        .map_err(|e| CoverError::Decode(format!("png encode: {e}")))
}

/// Parse-only validity check for the ingestion validator (Layer 5). Does not
/// rasterize and resolves no siblings — it only answers "does this parse as an
/// SVG?". Subject to the same input-size cap as [`rasterize_svg`].
pub(crate) fn parses_as_svg(svg_bytes: &[u8]) -> bool {
    if svg_bytes.len() > MAX_SVG_INPUT_BYTES {
        return false;
    }
    let opt = usvg::Options {
        image_href_resolver: hardened_resolver(|_: &str| None),
        ..Default::default()
    };
    usvg::Tree::from_data(svg_bytes, &opt).is_ok()
}

/// Build a hardened [`usvg::ImageHrefResolver`]: the string resolver restricts
/// `<image href>` to path-checked sibling entries via `resolve_sibling`; the
/// data resolver bounds base64 data-URI payloads. Both reject rasters over the
/// byte/megapixel caps. Replaces usvg's default, which reads local files.
fn hardened_resolver<'a, F>(resolve_sibling: F) -> usvg::ImageHrefResolver<'a>
where
    F: Fn(&str) -> Option<Vec<u8>> + Send + Sync + 'a,
{
    usvg::ImageHrefResolver {
        // THREAT: default data resolver wraps base64 payloads without bounding
        // decode size. Apply the same byte+megapixel guard.
        resolve_data: Box::new(|_mime, data, _opts| safe_image_kind(data)),
        // THREAT: default string resolver reads local files. Restrict to
        // path-checked siblings inside the same EPUB ZIP.
        resolve_string: Box::new(move |href, _opts| {
            if !crate::services::epub::is_safe_path(href) {
                return None;
            }
            let bytes = resolve_sibling(href)?;
            safe_image_kind(Arc::new(bytes))
        }),
    }
}

/// Wrap embedded raster bytes in an [`usvg::ImageKind`] only if they pass the
/// byte and megapixel caps and decode to a supported format. Dimension sniffing
/// is header-only (no full decode). GIF is intentionally excluded — the `image`
/// crate is built without the `gif` feature, so its dimensions cannot be sniffed
/// and it is not a cover format we accept.
fn safe_image_kind(data: Arc<Vec<u8>>) -> Option<usvg::ImageKind> {
    if data.len() > MAX_EMBEDDED_IMAGE_BYTES {
        return None;
    }
    let fmt = image::guess_format(&data).ok()?;
    let (width, height) =
        image::ImageReader::with_format(std::io::Cursor::new(data.as_slice()), fmt)
            .into_dimensions()
            .ok()?;
    if u64::from(width).saturating_mul(u64::from(height)) > MAX_EMBEDDED_PIXELS {
        return None;
    }
    match fmt {
        image::ImageFormat::Jpeg => Some(usvg::ImageKind::JPEG(data)),
        image::ImageFormat::Png => Some(usvg::ImageKind::PNG(data)),
        image::ImageFormat::WebP => Some(usvg::ImageKind::WEBP(data)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raster(width: u32, height: u32, fmt: image::ImageFormat) -> Vec<u8> {
        let img = image::DynamicImage::new_rgb8(width, height);
        let mut buf = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut buf), fmt)
            .expect("encode raster");
        buf
    }

    fn vector_svg(width: u32, height: u32) -> String {
        format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}"><rect width="{width}" height="{height}" fill="black"/></svg>"#
        )
    }

    /// SVG whose ONLY paintable content is a single `<image>` — so a blocked or
    /// rejected href yields a fully-transparent render. Airtight for the
    /// security guards (no background to mask a failed resolve).
    fn image_only_svg(href: &str) -> String {
        format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="150" viewBox="0 0 100 150"><image href="{href}" width="100" height="150"/></svg>"#
        )
    }

    fn long_edge(png: &[u8]) -> u32 {
        let img =
            image::load_from_memory_with_format(png, image::ImageFormat::Png).expect("decode png");
        img.width().max(img.height())
    }

    // PNG signature + a single IHDR chunk declaring `width`×`height`. Enough for
    // header-only dimension sniffing; lets us assert the megapixel guard without
    // allocating a real giant image.
    fn png_header_only(width: u32, height: u32) -> Vec<u8> {
        fn crc32(data: &[u8]) -> u32 {
            let mut crc = 0xFFFF_FFFFu32;
            for &byte in data {
                crc ^= u32::from(byte);
                for _ in 0..8 {
                    crc = if crc & 1 != 0 {
                        (crc >> 1) ^ 0xEDB8_8320
                    } else {
                        crc >> 1
                    };
                }
            }
            !crc
        }
        let mut out = vec![137, 80, 78, 71, 13, 10, 26, 10];
        out.extend_from_slice(&13u32.to_be_bytes());
        let mut chunk = b"IHDR".to_vec();
        chunk.extend_from_slice(&width.to_be_bytes());
        chunk.extend_from_slice(&height.to_be_bytes());
        chunk.extend_from_slice(&[8, 2, 0, 0, 0]); // 8-bit, RGB, no interlace
        out.extend_from_slice(&chunk);
        out.extend_from_slice(&crc32(&chunk).to_be_bytes());
        out
    }

    #[test]
    fn rasterizes_minimal_svg_to_png() {
        let out = rasterize_svg(vector_svg(100, 150).as_bytes(), |_| None).unwrap();
        assert_eq!(image::guess_format(&out).unwrap(), image::ImageFormat::Png);
        assert!(long_edge(&out) <= MAX_RENDER_LONG_EDGE);
    }

    #[test]
    fn resolves_in_zip_relative_image_href() {
        let sibling = raster(2, 2, image::ImageFormat::Png);
        let out = rasterize_svg(image_only_svg("cover.png").as_bytes(), move |href| {
            (href == "cover.png").then(|| sibling.clone())
        })
        .unwrap();
        assert_eq!(image::guess_format(&out).unwrap(), image::ImageFormat::Png);
    }

    #[test]
    fn blocks_external_path_href() {
        // Resolver would happily return a real image for ANY href; the absolute
        // path must be blocked by is_safe_path BEFORE the resolver is consulted,
        // leaving a transparent render → Decode error.
        let err = rasterize_svg(image_only_svg("/etc/hostname").as_bytes(), |_| {
            Some(raster(2, 2, image::ImageFormat::Png))
        })
        .unwrap_err();
        assert!(matches!(err, CoverError::Decode(_)));
    }

    #[test]
    fn blocks_traversal_href() {
        let err = rasterize_svg(image_only_svg("../secret.png").as_bytes(), |_| {
            Some(raster(2, 2, image::ImageFormat::Png))
        })
        .unwrap_err();
        assert!(matches!(err, CoverError::Decode(_)));
    }

    #[test]
    fn rejects_malformed_svg() {
        let err = rasterize_svg(b"<svg><broken", |_| None).unwrap_err();
        assert!(matches!(err, CoverError::Decode(_)));
    }

    #[test]
    fn rejects_zero_size_svg() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="0" height="0"></svg>"#;
        let err = rasterize_svg(svg.as_bytes(), |_| None).unwrap_err();
        assert!(matches!(err, CoverError::Decode(_)));
    }

    #[test]
    fn caps_huge_viewbox() {
        let out = rasterize_svg(vector_svg(999_999, 999_999).as_bytes(), |_| None).unwrap();
        assert!(long_edge(&out) <= MAX_RENDER_LONG_EDGE);
    }

    #[test]
    fn dtd_entities_are_inert() {
        // External SYSTEM entity: roxmltree performs no IO, so it is never
        // fetched. The text feature is off, so the only node is dropped →
        // transparent render → Decode. No panic, no file content, no hang.
        let svg = r#"<?xml version="1.0"?><!DOCTYPE svg [<!ENTITY xxe SYSTEM "file:///etc/hostname">]><svg xmlns="http://www.w3.org/2000/svg" width="100" height="100"><text>&xxe;</text></svg>"#;
        let result = rasterize_svg(svg.as_bytes(), |_| None);
        assert!(result.is_err(), "external entity must not yield a cover");
    }

    #[test]
    fn rejects_oversized_svg_input() {
        let mut svg = b"<svg xmlns=\"http://www.w3.org/2000/svg\">".to_vec();
        svg.resize(MAX_SVG_INPUT_BYTES + 1, b' ');
        let err = rasterize_svg(&svg, |_| None).unwrap_err();
        assert!(matches!(err, CoverError::Decode(_)));
    }

    #[test]
    fn rejects_oversized_sibling_image() {
        // Resolver returns more than the byte cap → rejected before decode →
        // transparent render → Decode.
        let mut huge = raster(2, 2, image::ImageFormat::Png);
        huge.resize(MAX_EMBEDDED_IMAGE_BYTES + 1, 0);
        let err = rasterize_svg(image_only_svg("cover.png").as_bytes(), move |_| {
            Some(huge.clone())
        })
        .unwrap_err();
        assert!(matches!(err, CoverError::Decode(_)));
    }

    #[test]
    fn rejects_huge_pixel_sibling() {
        // Small bytes, but the PNG header declares 30000×30000 (900 MP) → the
        // megapixel guard rejects it → transparent render → Decode.
        let bomb = png_header_only(30_000, 30_000);
        let err = rasterize_svg(image_only_svg("cover.png").as_bytes(), move |_| {
            Some(bomb.clone())
        })
        .unwrap_err();
        assert!(matches!(err, CoverError::Decode(_)));
    }

    #[test]
    fn blank_svg_yields_decode_error() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100" viewBox="0 0 100 100"></svg>"#;
        let err = rasterize_svg(svg.as_bytes(), |_| None).unwrap_err();
        assert!(matches!(err, CoverError::Decode(_)));
    }

    #[test]
    fn looks_like_svg_accepts_variants() {
        assert!(looks_like_svg(b"<svg xmlns=\"...\">"));
        assert!(looks_like_svg(b"<?xml version=\"1.0\"?><svg/>"));
        assert!(looks_like_svg(b"   \n\t<svg/>"));
        assert!(looks_like_svg(b"\xEF\xBB\xBF<svg/>"));
        assert!(looks_like_svg(b"<SVG/>"));
    }

    #[test]
    fn looks_like_svg_rejects_rasters() {
        assert!(!looks_like_svg(&raster(2, 2, image::ImageFormat::Png)));
        assert!(!looks_like_svg(&raster(2, 2, image::ImageFormat::Jpeg)));
        assert!(!looks_like_svg(&[0x1f, 0x8b, 0x08, 0x00])); // svgz/gzip
        assert!(!looks_like_svg(b""));
    }

    #[test]
    fn parses_as_svg_accepts_valid_and_image_ref() {
        assert!(parses_as_svg(vector_svg(100, 150).as_bytes()));
        // SE-shape: image ref is unresolved at validation time but the SVG
        // still parses.
        assert!(parses_as_svg(image_only_svg("cover.jpg").as_bytes()));
    }

    #[test]
    fn parses_as_svg_rejects_malformed_and_oversized() {
        assert!(!parses_as_svg(b"<svg><broken"));
        assert!(!parses_as_svg(&raster(2, 2, image::ImageFormat::Png)));
        let mut svg = b"<svg>".to_vec();
        svg.resize(MAX_SVG_INPUT_BYTES + 1, b' ');
        assert!(!parses_as_svg(&svg));
    }
}
