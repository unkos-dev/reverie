//! Cover serving through the recorded library's capabilities and a shared raster cache.
//!
//! Enrichment sidecars are separate artefacts and are never served by this module.

/// Capability cache ownership and complete publication.
pub mod cache;
/// Cover-serving failure classes.
pub mod error;
pub mod extract;
/// Raster resizing and size tiers.
pub mod resize;
/// Hardened SVG-to-PNG rasterisation.
pub mod svg;

use uuid::Uuid;

use crate::db;
use crate::models::storage_library::LibraryId;
use crate::services::files::{LibraryFileError, LibraryFiles, LibraryLocation};
use crate::state::AppState;

pub use cache::{CoverCache, CoverEncoding};
pub use error::CoverError;
pub use resize::CoverSize;

// Cover generation must leave blocking-pool capacity for request-driven work.
static WARM_LIMIT: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(3);

/// An opened cache file with its raster encoding and content-addressed validator.
pub struct CoverArtifact {
    /// Rewound file transferred directly to the response stream.
    pub file: std::fs::File,
    /// Closed raster encoding used for the response content type.
    pub encoding: CoverEncoding,
    /// Unquoted strong validator: hash prefix and size tier.
    pub etag: String,
}

fn etag_for(file_hash: &str, size: CoverSize) -> String {
    let prefix: String = file_hash.chars().take(16).collect();
    let tier = match size {
        CoverSize::Full => "full",
        CoverSize::Thumb => "thumb",
    };
    format!("{prefix}-{tier}")
}

fn source_error(error: LibraryFileError) -> CoverError {
    match error {
        LibraryFileError::Io(error) => CoverError::Io(error),
        error => CoverError::Library(error),
    }
}

/// Extract, resize and publish one cover in blocking work.
///
/// # Errors
/// Returns extraction, rasterisation, encoding or cache publication errors; an archive rejection
/// is logged with its manifestation identifier.
fn generate_into_cache(
    cache: &CoverCache,
    manifestation_id: Uuid,
    file_hash: &str,
    epub_file: std::fs::File,
    size: CoverSize,
) -> Result<CoverArtifact, CoverError> {
    let (raw_bytes, in_fmt) = extract::extract_cover_bytes(epub_file).inspect_err(|error| {
        if let CoverError::ArchiveRejected(issues) = error {
            tracing::warn!(
                %manifestation_id,
                %issues,
                "cover extraction: archive rejected by Layer 1 validation"
            );
        }
    })?;
    let (resized, out_fmt) = resize::resize_cover(&raw_bytes, in_fmt, size)?;
    cache.publish(
        manifestation_id,
        file_hash,
        size,
        CoverEncoding::try_from(out_fmt)?,
        &resized,
    )
}

/// Authorise the recorded manifestation and return its opened cached cover.
///
/// Cache authority, lookup and generation run in blocking work after the RLS transaction ends.
///
/// # Errors
/// Returns database, invisible-row, recorded-location, contained I/O or cover-generation errors.
pub async fn get_or_create(
    state: &AppState,
    manifestation_id: Uuid,
    user_id: Uuid,
    size: CoverSize,
) -> Result<CoverArtifact, CoverError> {
    // THREAT: Cached bytes never bypass the caller's RLS-scoped catalogue lookup.
    let mut tx = db::acquire_with_rls(&state.pool, user_id)
        .await
        .map_err(|error| CoverError::Db(format!("covers: {error}")))?;
    let row = sqlx::query!(
        "SELECT library_id, file_path, current_file_hash FROM manifestations WHERE id = $1",
        manifestation_id,
    )
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| CoverError::Db(format!("covers: {error}")))?;
    drop(tx);
    let row = row.ok_or(CoverError::NoCover)?;
    let location = LibraryLocation {
        library_id: LibraryId::from_uuid(row.library_id),
        path: row.file_path.parse()?,
    };
    let files = state.library_files.clone();
    tokio::task::spawn_blocking(move || {
        let cache = CoverCache::new(files.library(location.library_id)?)?;
        if let Some(artifact) = cache.open(manifestation_id, &row.current_file_hash, size)? {
            return Ok(artifact);
        }
        generate_into_cache(
            &cache,
            manifestation_id,
            &row.current_file_hash,
            files.open_source(&location).map_err(source_error)?.file,
            size,
        )
    })
    .await
    .map_err(|error| CoverError::Decode(format!("cover generation task failed: {error}")))?
}

/// Warm one tier using its accepted source handle and recorded library identity.
///
/// # Errors
/// Returns unknown-library, cache, cover-generation or blocking-task errors.
async fn warm_one(
    files: &LibraryFiles,
    library_id: LibraryId,
    manifestation_id: Uuid,
    file_hash: &str,
    epub_file: std::fs::File,
    size: CoverSize,
) -> Result<CoverArtifact, CoverError> {
    let files = files.clone();
    let file_hash = file_hash.to_owned();
    tokio::task::spawn_blocking(move || {
        let cache = CoverCache::new(files.library(library_id)?)?;
        if let Some(artifact) = cache.open(manifestation_id, &file_hash, size)? {
            return Ok(artifact);
        }
        generate_into_cache(&cache, manifestation_id, &file_hash, epub_file, size)
    })
    .await
    .map_err(|error| CoverError::Decode(format!("cover warm task failed: {error}")))?
}

/// Warm a freshly accepted thumbnail without affecting ingestion success.
///
/// The accepted file and recorded library authority survive into bounded blocking work.
/// Full covers remain lazy; failures are logged.
pub fn spawn_warm_thumb(
    files: LibraryFiles,
    library_id: LibraryId,
    manifestation_id: Uuid,
    file_hash: String,
    epub_file: std::fs::File,
) {
    tokio::spawn(async move {
        let Ok(_permit) = WARM_LIMIT.acquire().await else {
            tracing::warn!(%manifestation_id, "cover warm skipped: warm-limit semaphore closed");
            return;
        };
        match warm_one(
            &files,
            library_id,
            manifestation_id,
            &file_hash,
            epub_file,
            CoverSize::Thumb,
        )
        .await
        {
            Ok(_) => tracing::debug!(%manifestation_id, "cover thumbnail warmed"),
            Err(CoverError::NoCover) => {
                tracing::debug!(%manifestation_id, "cover warm: EPUB declares no cover");
            }
            Err(CoverError::ArchiveRejected(_)) => {
                tracing::debug!(%manifestation_id, "cover warm: archive rejected by validation, already logged");
            }
            Err(error) => tracing::warn!(%manifestation_id, %error, "cover warm failed"),
        }
    });
}
