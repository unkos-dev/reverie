//! Cover serving service. Content-addressed on-disk cache keyed on
//! `(manifestation_id, current_file_hash[..16], size)`. Cache miss extracts
//! the EPUB's embedded cover (Step 5 detection semantics), resizes with
//! Lanczos3, and atomically writes the result.
//!
//! **Sidecar covers at `manifestations.cover_path` are NOT served here.**
//! That path is the enrichment preview sidecar (Step 7) — a distinct
//! artefact from the EPUB-embedded cover. Showing enrichment previews in
//! OPDS is an orthogonal decision out of scope for Step 9 per BLUEPRINT.

/// On-disk content-addressed cover cache; handles atomic writes and path
/// derivation.
pub mod cache;
/// Error type for all cover-serving failure classes.
pub mod error;
/// Synchronous `EPUB` cover extraction; mirrors Step 5 detection semantics.
pub mod extract;
/// `Lanczos3` resize logic and the `CoverSize` size-tier enum.
pub mod resize;
/// Hardened SVG-to-PNG rasterization for SVG-declared covers.
pub mod svg;

use std::path::{Path, PathBuf};

use uuid::Uuid;

use crate::db;
use crate::state::AppState;

pub use cache::CoverCache;
pub use error::CoverError;
pub use resize::CoverSize;

/// Bound on concurrent background cover-warm tasks (see [`spawn_warm_thumb`]).
/// Cover generation is CPU-bound (rasterize + Lanczos3 resize); an unbounded
/// burst at ingest would saturate the blocking pool and starve request-driven
/// [`get_or_create`] work. 3 keeps a steady warm trickle without that.
static WARM_LIMIT: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(3);

/// Cover cache root under a library path: `{library_path}/_covers/cache`.
fn cover_cache_root(library_path: &str) -> PathBuf {
    PathBuf::from(library_path).join("_covers").join("cache")
}

fn cache_root(state: &AppState) -> PathBuf {
    cover_cache_root(&state.config.library_path)
}

const fn ext_for_format(fmt: image::ImageFormat) -> &'static str {
    match fmt {
        image::ImageFormat::Jpeg => "jpg",
        image::ImageFormat::Png => "png",
        image::ImageFormat::WebP => "webp",
        // other formats are rejected by `resize_cover`; return .bin defensively
        _ => "bin",
    }
}

/// A resolved cover: the on-disk cache path plus the strong `ETag` validator
/// the handler emits. The validator is content-addressed by the EPUB file-hash
/// prefix and size tier, so a Step 8 writeback (new `current_file_hash`)
/// produces a new validator and the browser revalidates to the fresh cover.
pub struct CoverArtifact {
    /// Absolute path to the cached cover file.
    pub path: PathBuf,
    /// Strong `ETag` value (unquoted): `{file_hash[..16]}-{size}`.
    pub etag: String,
}

/// Strong `ETag` validator for a cover: `{file_hash[..16]}-{size}`. Matches the
/// content-addressed cache key, so it changes exactly when the cover content
/// would.
fn etag_for(file_hash: &str, size: CoverSize) -> String {
    let prefix: String = file_hash.chars().take(16).collect();
    let size_tag = match size {
        CoverSize::Full => "full",
        CoverSize::Thumb => "thumb",
    };
    format!("{prefix}-{size_tag}")
}

/// Return the cached path for `(manifestation_id, file_hash, size)` if a
/// valid-for-this-tier encoding is already on disk. The `Thumb` tier is always
/// JPEG, so it probes only `jpg` — this ignores a stale `png`/`webp` thumbnail
/// from before the JPEG-thumbnail policy and regenerates it as JPEG. The `Full`
/// tier preserves the source format, so it probes the raster encodings.
fn cached_hit(
    cache: &CoverCache,
    manifestation_id: Uuid,
    file_hash: &str,
    size: CoverSize,
) -> Option<PathBuf> {
    let exts: &[&str] = match size {
        CoverSize::Thumb => &["jpg"],
        CoverSize::Full => &["jpg", "png", "webp"],
    };
    for &ext in exts {
        let path = cache.cached_path(manifestation_id, file_hash, size, ext);
        if path.exists() {
            return Some(path);
        }
    }
    None
}

/// Synchronous extract → resize → atomic cache-write for one size tier.
/// CPU-bound (rasterize + Lanczos3); **call from `spawn_blocking`**. Returns
/// the written cache path.
///
/// # Errors
/// - [`CoverError::NoCover`] — the EPUB declares no cover.
/// - [`CoverError::UnsupportedFormat`] / [`CoverError::Decode`] — extract or
///   resize/encode failure.
/// - [`CoverError::Io`] — cache write failure.
fn generate_into_cache(
    cache: &CoverCache,
    manifestation_id: Uuid,
    file_hash: &str,
    epub_path: &Path,
    size: CoverSize,
) -> Result<PathBuf, CoverError> {
    let (raw_bytes, in_fmt) = extract::extract_cover_bytes(epub_path)?;
    let (resized, out_fmt) = resize::resize_cover(&raw_bytes, in_fmt, size)?;
    let ext = ext_for_format(out_fmt);
    let dest = cache.cached_path(manifestation_id, file_hash, size, ext);
    cache.write_atomic(&dest, &resized)?;
    Ok(dest)
}

/// Return a cached cover for `(manifestation_id, size)`, populating the cache
/// on miss. RLS denies unauthorised users → returns [`CoverError::NoCover`] so
/// the handler emits 404 without leaking existence.
pub async fn get_or_create(
    state: &AppState,
    manifestation_id: Uuid,
    user_id: Uuid,
    size: CoverSize,
) -> Result<CoverArtifact, CoverError> {
    let mut tx = db::acquire_with_rls(&state.pool, user_id)
        .await
        .map_err(|e| CoverError::Db(format!("covers: {e}")))?;

    let row = sqlx::query!(
        "SELECT file_path, current_file_hash FROM manifestations WHERE id = $1",
        manifestation_id,
    )
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| CoverError::Db(format!("covers: {e}")))?;
    drop(tx);

    let (file_path, current_file_hash) = row
        .map(|r| (r.file_path, r.current_file_hash))
        .ok_or(CoverError::NoCover)?;

    let etag = etag_for(&current_file_hash, size);
    let cache = CoverCache::new(cache_root(state));
    cache.ensure_dir()?;

    if let Some(path) = cached_hit(&cache, manifestation_id, &current_file_hash, size) {
        return Ok(CoverArtifact { path, etag });
    }

    // Cache miss — generate. extract + resize + write are all CPU/IO-bound and
    // must not run on the async runtime thread.
    let epub_path = PathBuf::from(file_path);
    let path = tokio::task::spawn_blocking(move || {
        generate_into_cache(
            &cache,
            manifestation_id,
            &current_file_hash,
            &epub_path,
            size,
        )
    })
    .await
    .map_err(|e| CoverError::Decode(format!("cover generation task failed: {e}")))??;

    Ok(CoverArtifact { path, etag })
}

/// Generate + cache one size tier off the request path, skipping work when the
/// cover is already cached. Used by [`spawn_warm_thumb`] for ingest pre-warm.
///
/// # Errors
/// - [`CoverError::NoCover`] — the EPUB declares no cover.
/// - Other [`CoverError`] variants on extract/resize/IO failure, or if the
///   blocking generation task panics.
async fn warm_one(
    library_path: &str,
    manifestation_id: Uuid,
    file_hash: &str,
    epub_path: &str,
    size: CoverSize,
) -> Result<PathBuf, CoverError> {
    let cache = CoverCache::new(cover_cache_root(library_path));
    cache.ensure_dir()?;

    if let Some(path) = cached_hit(&cache, manifestation_id, file_hash, size) {
        return Ok(path);
    }

    let epub_path = PathBuf::from(epub_path);
    let file_hash = file_hash.to_owned();
    tokio::task::spawn_blocking(move || {
        generate_into_cache(&cache, manifestation_id, &file_hash, &epub_path, size)
    })
    .await
    .map_err(|e| CoverError::Decode(format!("cover warm task failed: {e}")))?
}

/// Fire-and-forget: warm the **thumbnail** cover for a freshly-ingested
/// manifestation so the first library-grid request is a warm ~20 ms hit
/// instead of a cold rasterize. Concurrency is bounded by `WARM_LIMIT`;
/// best-effort — a coverless EPUB or any failure is logged, never surfaced
/// (warming must not affect ingest success). Full-size covers stay lazy (the
/// reader view loads one cover at a time).
pub fn spawn_warm_thumb(
    library_path: String,
    manifestation_id: Uuid,
    file_hash: String,
    epub_path: String,
) {
    tokio::spawn(async move {
        // WARM_LIMIT is a process-static semaphore that is never closed, so
        // acquire only fails in shutdown teardown — skip warming if so.
        let Ok(_permit) = WARM_LIMIT.acquire().await else {
            tracing::warn!(%manifestation_id, "cover warm skipped: warm-limit semaphore closed");
            return;
        };
        match warm_one(
            &library_path,
            manifestation_id,
            &file_hash,
            &epub_path,
            CoverSize::Thumb,
        )
        .await
        {
            Ok(path) => {
                tracing::debug!(%manifestation_id, path = %path.display(), "cover thumbnail warmed");
            }
            Err(CoverError::NoCover) => {
                tracing::debug!(%manifestation_id, "cover warm: EPUB declares no cover");
            }
            Err(e) => {
                tracing::warn!(%manifestation_id, error = %e, "cover warm failed");
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Write generated EPUB bytes into `dir` and return the path as a string.
    /// Tests generate EPUBs in-memory (the committed-fixture tree is not
    /// tracked in git, so it is absent in CI).
    fn write_epub(dir: &std::path::Path, bytes: &[u8]) -> String {
        let path = dir.join("book.epub");
        std::fs::write(&path, bytes).unwrap();
        path.display().to_string()
    }

    #[test]
    fn etag_is_hash_prefix_and_size() {
        let hash = "0123456789abcdef0123456789abcdef";
        assert_eq!(etag_for(hash, CoverSize::Thumb), "0123456789abcdef-thumb");
        assert_eq!(etag_for(hash, CoverSize::Full), "0123456789abcdef-full");
    }

    #[tokio::test]
    async fn warm_one_writes_jpeg_thumb_and_is_idempotent() {
        let tmp = tempfile::tempdir().unwrap();
        let lib = tmp.path().display().to_string();
        let epub = write_epub(
            tmp.path(),
            &crate::test_support::db::make_minimal_epub_with_cover_tagged("warm-thumb"),
        );
        let id = Uuid::from_u128(0x0123_4567_89ab_cdef);
        let hash = "abcd1234abcd1234abcd1234abcd1234";

        let path = warm_one(&lib, id, hash, &epub, CoverSize::Thumb)
            .await
            .expect("warm thumb should succeed");

        assert!(path.exists());
        assert_eq!(path.extension().and_then(|e| e.to_str()), Some("jpg"));
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(
            image::guess_format(&bytes).unwrap(),
            image::ImageFormat::Jpeg
        );

        // Second call is a cache hit — same path, no regeneration.
        let again = warm_one(&lib, id, hash, &epub, CoverSize::Thumb)
            .await
            .unwrap();
        assert_eq!(again, path);
    }

    #[tokio::test]
    async fn warm_one_full_preserves_png_for_svg_cover() {
        // An SVG cover rasterizes to PNG; the Full tier preserves it, unlike
        // the Thumb tier which always re-encodes to JPEG.
        let tmp = tempfile::tempdir().unwrap();
        let lib = tmp.path().display().to_string();
        let epub = write_epub(
            tmp.path(),
            &crate::test_support::db::make_minimal_epub_with_svg_cover_sibling_ref("warm-full"),
        );
        let id = Uuid::from_u128(0xfeed_face);
        let hash = "00112233445566778899aabbccddeeff";

        let path = warm_one(&lib, id, hash, &epub, CoverSize::Full)
            .await
            .expect("warm full should succeed");
        assert_eq!(path.extension().and_then(|e| e.to_str()), Some("png"));
    }
}
