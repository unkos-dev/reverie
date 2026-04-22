//! Lanczos3-resize covers to the configured long-edge cap. Preserves input
//! format (JPEG → JPEG, PNG → PNG, WebP → WebP). GIF/BMP and any other
//! inputs are rejected with [`CoverError::UnsupportedFormat`].

use image::{ImageFormat, imageops::FilterType};

use super::error::CoverError;

/// Two size tiers. Values in pixels of the long edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoverSize {
    Full,
    Thumb,
}

impl CoverSize {
    pub fn long_edge(self) -> u32 {
        match self {
            CoverSize::Full => 1200,
            CoverSize::Thumb => 300,
        }
    }
}

pub fn resize_cover(
    bytes: &[u8],
    fmt: ImageFormat,
    size: CoverSize,
) -> Result<Vec<u8>, CoverError> {
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
    img.write_to(&mut std::io::Cursor::new(&mut out), fmt)
        .map_err(|e| CoverError::Decode(e.to_string()))?;

    Ok(out)
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
        let out = resize_cover(&src, ImageFormat::Jpeg, CoverSize::Thumb).unwrap();
        assert!(decoded_long_edge(&out, ImageFormat::Jpeg) <= 300);
    }

    #[test]
    fn resizes_to_full_cap() {
        let src = make_jpeg(2000, 3000);
        let out = resize_cover(&src, ImageFormat::Jpeg, CoverSize::Full).unwrap();
        assert!(decoded_long_edge(&out, ImageFormat::Jpeg) <= 1200);
    }

    #[test]
    fn preserves_format_png() {
        let src = make_png(50, 50);
        let out = resize_cover(&src, ImageFormat::Png, CoverSize::Thumb).unwrap();
        // guess_format round-trips the format.
        assert_eq!(image::guess_format(&out).unwrap(), ImageFormat::Png);
    }

    #[test]
    fn skips_resize_when_already_under() {
        let src = make_jpeg(50, 60);
        let out = resize_cover(&src, ImageFormat::Jpeg, CoverSize::Full).unwrap();
        assert_eq!(decoded_long_edge(&out, ImageFormat::Jpeg), 60);
    }

    #[test]
    fn rejects_unsupported_format() {
        let src = b"not-an-image";
        let err = resize_cover(src, ImageFormat::Gif, CoverSize::Thumb).unwrap_err();
        assert!(matches!(err, CoverError::UnsupportedFormat(_)));
    }
}
