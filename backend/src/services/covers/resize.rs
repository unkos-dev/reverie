//! Lanczos3-resize covers to the configured long-edge cap, then encode.
//!
//! Output format is tier-dependent, not a passthrough of the input:
//! - **Thumb** → always JPEG (quality 82). Grid/list views load dozens of
//!   covers at once, so payload size dominates. JPEG has no alpha, so any
//!   transparency is composited over white first (most covers are opaque, but
//!   an SVG cover that does not fill its canvas would otherwise go black).
//!   (`image` 0.25 encodes WebP losslessly only, so JPEG is the lossy option.)
//! - **Full** → preserves the input format (JPEG, PNG, or WebP) for the
//!   reader view where quality matters and only one cover loads at a time.
//!
//! Only JPEG, PNG, and WebP are accepted as *input*; GIF/BMP and any other
//! `ImageFormat` are rejected with [`CoverError::UnsupportedFormat`].

use image::codecs::jpeg::JpegEncoder;
use image::{ImageFormat, imageops::FilterType};

use super::error::CoverError;

/// JPEG quality for thumbnail re-encode. 82 is the knee of the
/// quality/size curve for downsampled cover art — visually lossless at grid
/// size, ~6–8× smaller than the equivalent PNG.
const THUMB_JPEG_QUALITY: u8 = 82;

/// Two size tiers. Values in pixels of the long edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoverSize {
    /// Full-resolution tier — long edge capped at 1 200 px. Used for the
    /// reader view where cover quality matters.
    Full,
    /// Thumbnail tier — long edge capped at 300 px. Used for grid and list
    /// views where many covers load simultaneously.
    Thumb,
}

impl CoverSize {
    /// Return the maximum pixel length of the long edge for this tier.
    #[must_use]
    pub const fn long_edge(self) -> u32 {
        match self {
            Self::Full => 1200,
            Self::Thumb => 300,
        }
    }
}

/// Decode `bytes` as `fmt` and re-encode for the tier, resizing with
/// `Lanczos3` if the image exceeds `size`'s long-edge cap.
///
/// Encodes `Thumb` → JPEG, `Full` → the input format. Images already within
/// the cap are encoded without resizing. Returns the encoded bytes and the
/// `ImageFormat` they were encoded in (the caller derives the cache-file
/// extension and `Content-Type` from it).
///
/// Only `JPEG`, `PNG`, and `WebP` are accepted as input; any other
/// `ImageFormat` variant is rejected before decoding.
///
/// # Errors
///
/// - [`CoverError::UnsupportedFormat`] — `fmt` is not `JPEG`, `PNG`, or `WebP`.
/// - [`CoverError::Decode`] — `image::load_from_memory_with_format` or the
///   encoder fail (malformed bytes or encoder error).
pub fn resize_cover(
    bytes: &[u8],
    fmt: ImageFormat,
    size: CoverSize,
) -> Result<(Vec<u8>, ImageFormat), CoverError> {
    if !matches!(
        fmt,
        ImageFormat::Jpeg | ImageFormat::Png | ImageFormat::WebP
    ) {
        return Err(CoverError::UnsupportedFormat(format!("{fmt:?}")));
    }

    let img = image::load_from_memory_with_format(bytes, fmt)
        .map_err(|e| CoverError::Decode(e.to_string()))?;

    let cap = size.long_edge();
    // image::resize preserves aspect ratio. Skip entirely when already under.
    let long = img.width().max(img.height());
    let img = if long > cap {
        img.resize(cap, cap, FilterType::Lanczos3)
    } else {
        img
    };

    let mut out: Vec<u8> = Vec::new();
    let out_fmt = match size {
        // Thumbnails always JPEG. JPEG has no alpha channel, so composite over
        // an opaque white background first — a bare channel-drop would turn
        // transparent pixels (e.g. an SVG cover that does not fill its canvas)
        // black. Opaque covers (the common case) are unchanged by the overlay.
        CoverSize::Thumb => {
            let rgba = img.to_rgba8();
            let (w, h) = rgba.dimensions();
            let mut canvas = image::RgbaImage::from_pixel(w, h, image::Rgba([255, 255, 255, 255]));
            image::imageops::overlay(&mut canvas, &rgba, 0, 0);
            let rgb = image::DynamicImage::ImageRgba8(canvas).into_rgb8();
            JpegEncoder::new_with_quality(&mut out, THUMB_JPEG_QUALITY)
                .encode_image(&rgb)
                .map_err(|e| CoverError::Decode(e.to_string()))?;
            ImageFormat::Jpeg
        }
        // Full-size preserves the source format for reader-view fidelity.
        CoverSize::Full => {
            img.write_to(&mut std::io::Cursor::new(&mut out), fmt)
                .map_err(|e| CoverError::Decode(e.to_string()))?;
            fmt
        }
    };

    Ok((out, out_fmt))
}
