//! Lanczos3-resize covers to the configured long-edge cap, then encode.
//!
//! Output format is tier-dependent, not a passthrough of the input:
//! - **Thumb** → always JPEG (quality 82). Grid/list views load dozens of
//!   covers at once, so payload size dominates; cover art is opaque, so the
//!   lossy/no-alpha tradeoff is invisible. (`image` 0.25 ships WebP
//!   *decode-only* — no encoder — so JPEG is the lossy option available.)
//! - **Full** → preserves the input format (JPEG → JPEG, PNG → PNG) for the
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
    pub const fn long_edge(self) -> u32 {
        match self {
            Self::Full => 1200,
            Self::Thumb => 300,
        }
    }
}

/// Decode `bytes` as `fmt`, resize to the long-edge cap of `size` using the
/// `Lanczos3` filter if the image exceeds the cap, then encode for the tier:
/// `Thumb` → JPEG, `Full` → the input format. Images already within the cap
/// are encoded without resizing. Returns the encoded bytes and the
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
        // Thumbnails always JPEG: drop the alpha channel (opaque cover art)
        // and encode lossily for a far smaller grid payload.
        CoverSize::Thumb => {
            let rgb = img.to_rgb8();
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

#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, ImageFormat};

    fn make_jpeg(width: u32, height: u32) -> Vec<u8> {
        let img = DynamicImage::new_rgb8(width, height);
        let mut buf = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut buf), ImageFormat::Jpeg)
            .expect("encode jpeg");
        buf
    }

    fn make_png(width: u32, height: u32) -> Vec<u8> {
        let img = DynamicImage::new_rgb8(width, height);
        let mut buf = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut buf), ImageFormat::Png)
            .expect("encode png");
        buf
    }

    fn decoded_long_edge(bytes: &[u8], fmt: ImageFormat) -> u32 {
        let img = image::load_from_memory_with_format(bytes, fmt).expect("decode");
        img.width().max(img.height())
    }

    #[test]
    fn resizes_to_thumb_cap() {
        let src = make_jpeg(2000, 3000);
        let (out, fmt) = resize_cover(&src, ImageFormat::Jpeg, CoverSize::Thumb).unwrap();
        assert_eq!(fmt, ImageFormat::Jpeg);
        assert!(decoded_long_edge(&out, ImageFormat::Jpeg) <= 300);
    }

    #[test]
    fn resizes_to_full_cap() {
        let src = make_jpeg(2000, 3000);
        let (out, fmt) = resize_cover(&src, ImageFormat::Jpeg, CoverSize::Full).unwrap();
        assert_eq!(fmt, ImageFormat::Jpeg);
        assert!(decoded_long_edge(&out, ImageFormat::Jpeg) <= 1200);
    }

    #[test]
    fn thumb_always_jpeg_even_from_png() {
        // PNG input → JPEG thumbnail (alpha dropped, lossy-encoded).
        let src = make_png(400, 600);
        let (out, fmt) = resize_cover(&src, ImageFormat::Png, CoverSize::Thumb).unwrap();
        assert_eq!(fmt, ImageFormat::Jpeg);
        assert_eq!(image::guess_format(&out).unwrap(), ImageFormat::Jpeg);
    }

    #[test]
    fn full_preserves_png() {
        // Full-size keeps the source format (SVG covers rasterize to PNG).
        let src = make_png(50, 50);
        let (out, fmt) = resize_cover(&src, ImageFormat::Png, CoverSize::Full).unwrap();
        assert_eq!(fmt, ImageFormat::Png);
        assert_eq!(image::guess_format(&out).unwrap(), ImageFormat::Png);
    }

    #[test]
    fn skips_resize_when_already_under() {
        let src = make_jpeg(50, 60);
        let (out, _) = resize_cover(&src, ImageFormat::Jpeg, CoverSize::Full).unwrap();
        assert_eq!(decoded_long_edge(&out, ImageFormat::Jpeg), 60);
    }

    #[test]
    fn rejects_unsupported_format() {
        let src = b"not-an-image";
        let err = resize_cover(src, ImageFormat::Gif, CoverSize::Thumb).unwrap_err();
        assert!(matches!(err, CoverError::UnsupportedFormat(_)));
    }
}
