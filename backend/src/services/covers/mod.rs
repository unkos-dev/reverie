//! Cover serving service. Content-addressed on-disk cache keyed on
//! `(manifestation_id, current_file_hash[..16], size)`. Cache miss extracts
//! the EPUB's embedded cover (Step 5 detection semantics), resizes with
//! Lanczos3, and atomically writes the result.
//!
//! **Sidecar covers at `manifestations.cover_path` are NOT served here.**
//! That path is the enrichment preview sidecar (Step 7) — a distinct
//! artefact from the EPUB-embedded cover. Showing enrichment previews in
//! OPDS is an orthogonal decision out of scope for Step 9 per BLUEPRINT.

pub mod cache;
pub mod error;
pub mod extract;
pub mod resize;

use std::path::PathBuf;

use uuid::Uuid;

use crate::db;
use crate::state::AppState;

pub use cache::CoverCache;
pub use error::CoverError;
pub use resize::CoverSize;

fn cache_root(state: &AppState) -> PathBuf {
    PathBuf::from(&state.config.library_path)
        .join("_covers")
        .join("cache")
}

fn ext_for_format(fmt: image::ImageFormat) -> &'static str {
    match fmt {
        image::ImageFormat::Jpeg => "jpg",
        image::ImageFormat::Png => "png",
        image::ImageFormat::WebP => "webp",
        // other formats are rejected by `resize_cover`; return .bin defensively
        _ => "bin",
    }
}

/// Return a cached cover path for `(manifestation_id, size)`, populating the
/// cache on miss. RLS denies unauthorised users → returns
/// [`CoverError::NoCover`] so the handler emits 404 without leaking
/// existence.
pub async fn get_or_create(
    state: &AppState,
    manifestation_id: Uuid,
    user_id: Uuid,
    size: CoverSize,
) -> Result<PathBuf, CoverError> {
    let mut tx = db::acquire_with_rls(&state.pool, user_id)
        .await
        .map_err(|e| CoverError::Db(format!("covers: {e}")))?;

    let row: Option<(String, String)> =
        sqlx::query_as("SELECT file_path, current_file_hash FROM manifestations WHERE id = $1")
            .bind(manifestation_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|e| CoverError::Db(format!("covers: {e}")))?;
    drop(tx);

    let (file_path, current_file_hash) = row.ok_or(CoverError::NoCover)?;

    let cache = CoverCache::new(cache_root(state));
    cache.ensure_dir()?;

    // Try all three possible extensions on read; on miss we'll know the
    // actual format after extract. Serving handler should trust file name.
    for ext in ["jpg", "png", "webp"] {
        let path = cache.cached_path(manifestation_id, &current_file_hash, size, ext);
        if path.exists() {
            return Ok(path);
        }
    }

    // Cache miss — extract.
    let epub_path = PathBuf::from(file_path);
    let (raw_bytes, fmt) =
        tokio::task::spawn_blocking(move || extract::extract_cover_bytes(&epub_path))
            .await
            .map_err(|e| CoverError::Decode(e.to_string()))??;

    let resized = resize::resize_cover(&raw_bytes, fmt, size)?;
    let ext = ext_for_format(fmt);
    let dest = cache.cached_path(manifestation_id, &current_file_hash, size, ext);
    cache.write_atomic(&dest, &resized)?;
    Ok(dest)
}
