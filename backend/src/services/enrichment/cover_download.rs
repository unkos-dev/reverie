//! Cover image download and staging for the metadata enrichment pipeline.
//!
//! [`download`] fetches a remote URL, validates the content (content-type,
//! magic bytes, dimensions) against configurable limits, then writes the file
//! atomically to a staging directory under `{library_root}/_covers/pending/`.
//!
//! # Security model
//!
//! The caller must supply an SSRF-guarded [`reqwest::Client`] (see
//! [`super::http::cover_client`]).  This module validates the **initial** URL
//! before any network call because reqwest's redirect policy only fires on 3xx
//! responses — a direct request to an internal address is never intercepted by
//! the callback.

// These items are public API consumed by the enrichment pipeline.  They are not
// called from within this binary crate yet (the orchestrator wires them up after
// all Phase B agents complete), so dead_code is expected during integration.
#![allow(dead_code)]

use std::path::{Path, PathBuf};

use futures::TryStreamExt;
use image::ImageFormat;
use sha2::{Digest, Sha256};
use tracing::{debug, instrument};
use uuid::Uuid;

use super::http::validate_hop;

// ── Public types ───────────────────────────────────────────────────────────

/// The recognised cover image formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoverFormat {
    Jpeg,
    Png,
    Webp,
}

impl CoverFormat {
    fn extension(self) -> &'static str {
        match self {
            Self::Jpeg => "jpg",
            Self::Png => "png",
            Self::Webp => "webp",
        }
    }
}

/// A successfully staged cover image.
#[derive(Debug, Clone)]
pub struct CoverArtifact {
    /// Absolute path to the staged file.
    pub path: PathBuf,
    /// SHA-256 digest of the raw bytes as written to disk.
    pub sha256: Vec<u8>,
    /// Number of bytes written.
    pub size_bytes: u64,
    pub width: u32,
    pub height: u32,
    pub format: CoverFormat,
}

/// Configuration for a single cover download.
pub struct DownloadConfig {
    /// Root of the ebook library; covers are staged under
    /// `{library_root}/_covers/pending/`.
    pub library_root: PathBuf,
    /// Maximum number of bytes to accept.  The download is aborted
    /// mid-stream if this limit is exceeded.
    pub max_bytes: u64,
    /// Minimum long-edge pixel count.  Images smaller than this are rejected.
    pub min_long_edge_px: u32,
    /// When `true`, the initial-URL SSRF guard is bypassed.  Production
    /// callers MUST leave this `false`; only in-process tests that point at
    /// a `wiremock` server on `127.0.0.1` should flip it.  The redirect-hop
    /// policy on `cover_client` is always active and is not affected by
    /// this flag.
    pub allow_private_hosts: bool,
}

/// Errors that can occur during a cover download.
#[derive(Debug, thiserror::Error)]
pub enum CoverError {
    #[error("response body exceeds max bytes")]
    TooLarge,
    #[error("unexpected content-type: {0}")]
    WrongContentType(String),
    #[error("content-type did not match magic bytes")]
    MagicByteMismatch,
    #[error("image dimensions {0}x{1} below minimum")]
    DimensionsTooSmall(u32, u32),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Network(#[from] reqwest::Error),
    #[error("image decode failed: {0}")]
    Decode(String),
    #[error("SSRF guard rejected initial URL: {0}")]
    SsrfBlocked(String),
}

// ── Download ───────────────────────────────────────────────────────────────

/// Fetch a cover image from `url`, validate it, and stage it atomically.
///
/// Steps performed (in order):
/// 1. SSRF-check the initial URL.
/// 2. Send the request.
/// 3. Validate the `Content-Type` header.
/// 4. Stream the body with a hard byte-count limit.
/// 5. Verify the magic bytes match the declared content-type.
/// 6. Decode the image and check dimensions.
/// 7. Compute SHA-256.
/// 8. Write atomically via `tempfile` + `persist`.
///
/// The staging path is:
/// `{library_root}/_covers/pending/{manifestation_id}-{version_id_short}.{ext}`
/// where `version_id_short` is the first 8 hex characters of `version_id`
/// (no dashes).
#[instrument(skip(client, config), fields(url, %manifestation_id))]
pub async fn download(
    url: &str,
    client: &reqwest::Client,
    config: &DownloadConfig,
    manifestation_id: Uuid,
    version_id: Uuid,
) -> Result<CoverArtifact, CoverError> {
    // Step 1: SSRF-check the initial URL before any network I/O.
    //
    // Bypassable via `DownloadConfig::allow_private_hosts = true` for in-process
    // tests that target `wiremock` on 127.0.0.1.  Production callers always
    // leave the flag `false`; the redirect-hop policy on `cover_client`
    // remains active in all builds.
    if !config.allow_private_hosts {
        let parsed_url = reqwest::Url::parse(url)
            .map_err(|e| CoverError::SsrfBlocked(format!("invalid URL: {e}")))?;
        validate_hop(&parsed_url).map_err(|e| CoverError::SsrfBlocked(e.to_string()))?;
    }

    // Step 2: Send the request.
    let response = client.get(url).send().await?;

    // Step 3: Validate content-type.
    let ct = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_owned();
    let ct_base = ct.split(';').next().unwrap_or("").trim();
    let declared_format = match ct_base {
        "image/jpeg" => CoverFormat::Jpeg,
        "image/png" => CoverFormat::Png,
        "image/webp" => CoverFormat::Webp,
        other => return Err(CoverError::WrongContentType(other.to_string())),
    };

    // Step 4: Stream body with byte-count limit.
    let mut stream = response.bytes_stream();
    let mut buf: Vec<u8> = Vec::with_capacity(64 * 1024);
    while let Some(chunk) = stream.try_next().await? {
        if buf.len() as u64 + chunk.len() as u64 > config.max_bytes {
            return Err(CoverError::TooLarge);
        }
        buf.extend_from_slice(&chunk);
    }
    debug!(bytes = buf.len(), "cover body received");

    // Step 5: Magic-byte sniff.
    let sniffed_format =
        image::guess_format(&buf).map_err(|e| CoverError::Decode(e.to_string()))?;
    let expected_image_format = match declared_format {
        CoverFormat::Jpeg => ImageFormat::Jpeg,
        CoverFormat::Png => ImageFormat::Png,
        CoverFormat::Webp => ImageFormat::WebP,
    };
    if sniffed_format != expected_image_format {
        return Err(CoverError::MagicByteMismatch);
    }

    // Step 6: Decode and check dimensions.
    let img = image::load_from_memory_with_format(&buf, expected_image_format)
        .map_err(|e| CoverError::Decode(e.to_string()))?;
    let width = img.width();
    let height = img.height();
    let long_edge = width.max(height);
    if long_edge < config.min_long_edge_px {
        return Err(CoverError::DimensionsTooSmall(width, height));
    }

    // Step 7: SHA-256.
    let sha256 = Sha256::digest(&buf).to_vec();
    let size_bytes = buf.len() as u64;

    // Step 8: Atomic write.
    let version_id_short = &version_id.simple().to_string()[..8];
    let ext = declared_format.extension();
    let filename = format!("{manifestation_id}-{version_id_short}.{ext}");
    let pending_dir = config.library_root.join("_covers").join("pending");
    std::fs::create_dir_all(&pending_dir)?;

    let artifact_path = write_atomically(&pending_dir, &filename, &buf)?;

    Ok(CoverArtifact {
        path: artifact_path,
        sha256,
        size_bytes,
        width,
        height,
        format: declared_format,
    })
}

/// Write `data` to `dir/filename` atomically using a temp file + rename.
fn write_atomically(dir: &Path, filename: &str, data: &[u8]) -> Result<PathBuf, std::io::Error> {
    use std::io::Write;

    let tmp = tempfile::NamedTempFile::new_in(dir)?;
    let (mut file, tmp_path) = tmp.into_parts();
    file.write_all(data)?;
    file.flush()?;
    drop(file);

    let dest = dir.join(filename);
    tmp_path.persist(&dest).map_err(|e| e.error)?;
    Ok(dest)
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    use image::{ImageBuffer, ImageFormat, Rgb};
    use tempfile::TempDir;
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    /// Build a minimal valid JPEG in memory with the given dimensions.
    fn make_jpeg(width: u32, height: u32) -> Vec<u8> {
        let img: ImageBuffer<Rgb<u8>, Vec<u8>> =
            ImageBuffer::from_fn(width, height, |x, _y| Rgb([x as u8, 128u8, 200u8]));
        let mut buf = Cursor::new(Vec::new());
        img.write_to(&mut buf, ImageFormat::Jpeg).unwrap();
        buf.into_inner()
    }

    /// Build a minimal valid PNG in memory with the given dimensions.
    fn make_png(width: u32, height: u32) -> Vec<u8> {
        let img: ImageBuffer<Rgb<u8>, Vec<u8>> =
            ImageBuffer::from_fn(width, height, |x, _y| Rgb([x as u8, 100u8, 200u8]));
        let mut buf = Cursor::new(Vec::new());
        img.write_to(&mut buf, ImageFormat::Png).unwrap();
        buf.into_inner()
    }

    fn default_config(tmp: &TempDir) -> DownloadConfig {
        DownloadConfig {
            library_root: tmp.path().to_path_buf(),
            max_bytes: 10 * 1024 * 1024, // 10 MiB
            min_long_edge_px: 200,
            allow_private_hosts: true, // wiremock runs on 127.0.0.1
        }
    }

    fn client() -> reqwest::Client {
        reqwest::Client::new()
    }

    // ── Happy path ────────────────────────────────────────────────────────

    #[tokio::test]
    async fn valid_jpeg_fetched_and_staged() {
        let tmp = TempDir::new().unwrap();
        let server = MockServer::start().await;

        let jpeg_bytes = make_jpeg(1200, 1800);
        let content_length = jpeg_bytes.len();

        Mock::given(method("GET"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_bytes(jpeg_bytes.clone())
                    .insert_header("content-type", "image/jpeg")
                    .insert_header("content-length", content_length.to_string().as_str()),
            )
            .mount(&server)
            .await;

        let manifestation_id = Uuid::new_v4();
        let version_id = Uuid::new_v4();
        let config = default_config(&tmp);

        let artifact = download(
            &server.uri(),
            &client(),
            &config,
            manifestation_id,
            version_id,
        )
        .await
        .unwrap();

        assert_eq!(artifact.format, CoverFormat::Jpeg);
        assert_eq!(artifact.width, 1200);
        assert_eq!(artifact.height, 1800);
        assert_eq!(artifact.size_bytes, content_length as u64);
        assert_eq!(artifact.sha256.len(), 32);
        assert!(artifact.path.exists());

        // Verify SHA-256 is correct.
        let expected_sha256 = Sha256::digest(&jpeg_bytes).to_vec();
        assert_eq!(artifact.sha256, expected_sha256);

        // Verify filename convention.
        let version_short = &version_id.simple().to_string()[..8];
        let expected_name = format!("{manifestation_id}-{version_short}.jpg");
        assert_eq!(
            artifact.path.file_name().unwrap().to_str().unwrap(),
            expected_name
        );
    }

    // ── TooLarge ──────────────────────────────────────────────────────────

    #[tokio::test]
    async fn oversized_body_aborts_mid_stream() {
        let tmp = TempDir::new().unwrap();
        let server = MockServer::start().await;

        // 5 KiB of data, but limit is 1 KiB.
        let big_body = vec![0xFFu8; 5 * 1024];
        Mock::given(method("GET"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_bytes(big_body)
                    .insert_header("content-type", "image/jpeg"),
            )
            .mount(&server)
            .await;

        let config = DownloadConfig {
            library_root: tmp.path().to_path_buf(),
            max_bytes: 1024,
            min_long_edge_px: 1,
            allow_private_hosts: true,
        };

        let result = download(
            &server.uri(),
            &client(),
            &config,
            Uuid::new_v4(),
            Uuid::new_v4(),
        )
        .await;

        assert!(
            matches!(result, Err(CoverError::TooLarge)),
            "expected TooLarge, got {result:?}"
        );
    }

    // ── Wrong content-type ────────────────────────────────────────────────

    #[tokio::test]
    async fn wrong_content_type_rejected() {
        let tmp = TempDir::new().unwrap();
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_bytes(b"<html>not an image</html>".to_vec())
                    .insert_header("content-type", "text/html"),
            )
            .mount(&server)
            .await;

        let result = download(
            &server.uri(),
            &client(),
            &default_config(&tmp),
            Uuid::new_v4(),
            Uuid::new_v4(),
        )
        .await;

        assert!(
            matches!(result, Err(CoverError::WrongContentType(_))),
            "expected WrongContentType, got {result:?}"
        );
    }

    // ── Magic byte mismatch ───────────────────────────────────────────────

    #[tokio::test]
    async fn content_type_jpeg_but_png_bytes_rejected() {
        let tmp = TempDir::new().unwrap();
        let server = MockServer::start().await;

        let png_bytes = make_png(100, 100);
        Mock::given(method("GET"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_bytes(png_bytes)
                    .insert_header("content-type", "image/jpeg"),
            )
            .mount(&server)
            .await;

        let result = download(
            &server.uri(),
            &client(),
            &default_config(&tmp),
            Uuid::new_v4(),
            Uuid::new_v4(),
        )
        .await;

        assert!(
            matches!(result, Err(CoverError::MagicByteMismatch)),
            "expected MagicByteMismatch, got {result:?}"
        );
    }

    // ── Sub-threshold dimensions ──────────────────────────────────────────

    #[tokio::test]
    async fn dimensions_too_small_rejected() {
        let tmp = TempDir::new().unwrap();
        let server = MockServer::start().await;

        let small_jpeg = make_jpeg(100, 150);
        Mock::given(method("GET"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_bytes(small_jpeg)
                    .insert_header("content-type", "image/jpeg"),
            )
            .mount(&server)
            .await;

        let config = DownloadConfig {
            library_root: tmp.path().to_path_buf(),
            max_bytes: 10 * 1024 * 1024,
            min_long_edge_px: 1000,
            allow_private_hosts: true,
        };

        let result = download(
            &server.uri(),
            &client(),
            &config,
            Uuid::new_v4(),
            Uuid::new_v4(),
        )
        .await;

        assert!(
            matches!(result, Err(CoverError::DimensionsTooSmall(100, 150))),
            "expected DimensionsTooSmall(100, 150), got {result:?}"
        );
    }

    /// Task 37: The initial-URL SSRF pre-check must block 127.0.0.1 when
    /// `allow_private_hosts` is `false`.  This is the coverage the Phase B
    /// `#[cfg(not(test))]` gate obscured.
    #[tokio::test]
    async fn initial_url_loopback_blocked_without_allow_private_hosts() {
        let tmp = TempDir::new().unwrap();
        let config = DownloadConfig {
            library_root: tmp.path().to_path_buf(),
            max_bytes: 10 * 1024 * 1024,
            min_long_edge_px: 1,
            allow_private_hosts: false,
        };

        let result = download(
            "http://127.0.0.1:1/cover.jpg",
            &client(),
            &config,
            Uuid::new_v4(),
            Uuid::new_v4(),
        )
        .await;

        assert!(
            matches!(result, Err(CoverError::SsrfBlocked(_))),
            "expected SsrfBlocked, got {result:?}"
        );
    }
}
