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
/// Returns extraction, rasterisation, encoding or cache publication errors.
fn generate_into_cache(
    cache: &CoverCache,
    manifestation_id: Uuid,
    file_hash: &str,
    epub_file: std::fs::File,
    size: CoverSize,
) -> Result<CoverArtifact, CoverError> {
    let (raw_bytes, in_fmt) = extract::extract_cover_bytes(epub_file)?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::PgPool;

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
        use std::io::Read;

        let tmp = tempfile::tempdir().unwrap();
        let library_id = LibraryId::from_uuid(Uuid::new_v4());
        let root = tmp.path().to_str().unwrap().parse().unwrap();
        let files = crate::test_support::test_library_files_at(&root, library_id);
        let epub = write_epub(
            tmp.path(),
            &crate::test_support::db::make_minimal_epub_with_cover_tagged("warm-thumb"),
        );
        let id = Uuid::from_u128(0x0123_4567_89ab_cdef);
        let hash = "abcd1234abcd1234abcd1234abcd1234";

        let path = warm_one(
            &files,
            library_id,
            id,
            hash,
            std::fs::File::open(&epub).unwrap(),
            CoverSize::Thumb,
        )
        .await
        .expect("warm thumb should succeed");

        assert_eq!(path.encoding, CoverEncoding::Jpeg);
        let mut bytes = Vec::new();
        let mut first_file = path.file;
        first_file.read_to_end(&mut bytes).unwrap();
        assert_eq!(
            image::guess_format(&bytes).unwrap(),
            image::ImageFormat::Jpeg
        );

        // The second call is a cache hit.
        let again = warm_one(
            &files,
            library_id,
            id,
            hash,
            std::fs::File::open(&epub).unwrap(),
            CoverSize::Thumb,
        )
        .await
        .unwrap();
        assert_eq!(again.encoding, CoverEncoding::Jpeg);
        let mut again_file = again.file;
        let mut again_bytes = Vec::new();
        again_file.read_to_end(&mut again_bytes).unwrap();
        assert_eq!(again_bytes, bytes);
    }

    #[tokio::test]
    async fn warm_one_full_preserves_png_for_svg_cover() {
        // An SVG cover rasterizes to PNG; the Full tier preserves it, unlike
        // the Thumb tier which always re-encodes to JPEG.
        let tmp = tempfile::tempdir().unwrap();
        let library_id = LibraryId::from_uuid(Uuid::new_v4());
        let root = tmp.path().to_str().unwrap().parse().unwrap();
        let files = crate::test_support::test_library_files_at(&root, library_id);
        let epub = write_epub(
            tmp.path(),
            &crate::test_support::db::make_minimal_epub_with_svg_cover_sibling_ref("warm-full"),
        );
        let id = Uuid::from_u128(0xfeed_face);
        let hash = "00112233445566778899aabbccddeeff";

        let path = warm_one(
            &files,
            library_id,
            id,
            hash,
            std::fs::File::open(&epub).unwrap(),
            CoverSize::Full,
        )
        .await
        .expect("warm full should succeed");
        assert_eq!(path.encoding, CoverEncoding::Png);
    }

    fn artifact_bytes(mut artifact: CoverArtifact) -> Vec<u8> {
        use std::io::Read;
        let mut bytes = Vec::new();
        artifact.file.read_to_end(&mut bytes).unwrap();
        bytes
    }

    #[tokio::test]
    async fn capability_cover_warm_retains_source_and_owning_library() {
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        let first = LibraryId::from_uuid(Uuid::new_v4());
        let second = LibraryId::from_uuid(Uuid::new_v4());
        let root_a = a.path().to_str().unwrap().parse().unwrap();
        let root_b = b.path().to_str().unwrap().parse().unwrap();
        let files = LibraryFiles::open(
            [(first, root_a), (second, root_b)],
            &a.path().to_str().unwrap().parse().unwrap(),
        )
        .unwrap();
        std::fs::create_dir(b.path().join("Nested")).unwrap();
        let source = b.path().join("Nested/book.epub");
        std::fs::write(
            &source,
            crate::test_support::db::make_minimal_epub_with_cover_tagged("accepted"),
        )
        .unwrap();
        let opened = std::fs::File::open(&source).unwrap();
        std::fs::remove_file(&source).unwrap();
        let id = Uuid::new_v4();
        let hash = "0123456789abcdef0123456789abcdef";
        let first_bytes = artifact_bytes(
            warm_one(&files, second, id, hash, opened, CoverSize::Thumb)
                .await
                .unwrap(),
        );
        assert_eq!(
            image::guess_format(&first_bytes).unwrap(),
            image::ImageFormat::Jpeg
        );
        assert!(!a.path().join("_covers").exists());
        assert!(
            b.path()
                .join(format!("_covers/cache/{id}-0123456789abcdef-thumb.jpg"))
                .is_file()
        );
        let unused_source = tempfile::tempfile().unwrap();
        assert_eq!(
            artifact_bytes(
                warm_one(&files, second, id, hash, unused_source, CoverSize::Thumb)
                    .await
                    .unwrap()
            ),
            first_bytes
        );
        assert!(matches!(
            warm_one(
                &files,
                LibraryId::from_uuid(Uuid::new_v4()),
                id,
                hash,
                tempfile::tempfile().unwrap(),
                CoverSize::Thumb
            )
            .await,
            Err(CoverError::Library(LibraryFileError::UnknownLibrary))
        ));
    }

    #[tokio::test]
    async fn capability_cover_warm_ignores_stale_thumbnail_encoding_and_concurrent_writers() {
        let tmp = tempfile::tempdir().unwrap();
        let id = LibraryId::from_uuid(Uuid::new_v4());
        let root = tmp.path().to_str().unwrap().parse().unwrap();
        let files = crate::test_support::test_library_files_at(&root, id);
        let source = write_epub(
            tmp.path(),
            &crate::test_support::db::make_minimal_epub_with_svg_cover_sibling_ref("concurrent"),
        );
        let manifestation = Uuid::new_v4();
        let hash = "0123456789abcdef0123456789abcdef";
        CoverCache::new(files.library(id).unwrap())
            .unwrap()
            .publish(
                manifestation,
                hash,
                CoverSize::Thumb,
                CoverEncoding::Png,
                b"stale png",
            )
            .unwrap();
        let (one, two) = tokio::join!(
            warm_one(
                &files,
                id,
                manifestation,
                hash,
                std::fs::File::open(&source).unwrap(),
                CoverSize::Thumb
            ),
            warm_one(
                &files,
                id,
                manifestation,
                hash,
                std::fs::File::open(&source).unwrap(),
                CoverSize::Thumb
            ),
        );
        let one = artifact_bytes(one.unwrap());
        let two = artifact_bytes(two.unwrap());
        assert_eq!(one, two);
        assert_eq!(image::guess_format(&one).unwrap(), image::ImageFormat::Jpeg);
        assert!(image::load_from_memory(&one).is_ok());
        assert_eq!(
            std::fs::read(tmp.path().join(format!(
                "_covers/cache/{manifestation}-0123456789abcdef-thumb.png"
            )))
            .unwrap(),
            b"stale png"
        );
    }
    async fn insert_cover_manifestation(
        pool: &PgPool,
        library_id: crate::models::storage_library::LibraryId,
        path: &str,
        hash: &str,
    ) -> (Uuid, Uuid) {
        let work = sqlx::query_scalar!(
        "INSERT INTO works (title, sort_title) VALUES ('Original title', 'original title') RETURNING id"
    ).fetch_one(pool).await.unwrap();
        let file = sqlx::query_scalar!(
        "WITH inserted AS (INSERT INTO manifestations
         (library_id, work_id, file_path, format, ingestion_file_hash, current_file_hash, file_size_bytes)
         VALUES ($1, $2, $3, 'epub', $4, $4, 12345) RETURNING *), claimed AS (INSERT INTO library_path_claims (library_id, path, manifestation_id) SELECT library_id, file_path, id FROM inserted) SELECT id AS \"id!\" FROM inserted",
        library_id.as_uuid(), work, path, hash,
    ).fetch_one(pool).await.unwrap();
        (work, file)
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn capability_cover_request_recorded_location_and_library_ownership(pool: sqlx::PgPool) {
        use crate::models::storage_library::default_library_id;
        use sha2::{Digest, Sha256};

        let app_pool = crate::test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = crate::test_support::db::ingestion_pool_for(&pool).await;
        let (user, _) = crate::test_support::db::create_admin_and_basic_auth(&app_pool).await;
        let first = default_library_id(&pool).await.unwrap();
        let second = LibraryId::from_uuid(
            sqlx::query_scalar!(
                "INSERT INTO libraries (configuration_key) VALUES ('second') RETURNING id"
            )
            .fetch_one(&pool)
            .await
            .unwrap(),
        );
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        for (dir, contents) in [
            (
                &a,
                crate::test_support::db::make_minimal_epub_with_cover_tagged("first"),
            ),
            (
                &b,
                crate::test_support::db::make_minimal_epub_with_svg_cover_sibling_ref("second"),
            ),
        ] {
            std::fs::create_dir(dir.path().join("Nested")).unwrap();
            std::fs::write(dir.path().join("Nested/book.epub"), contents).unwrap();
        }
        let root_a: crate::config::AbsoluteRootPath = a.path().to_str().unwrap().parse().unwrap();
        let root_b = b.path().to_str().unwrap().parse().unwrap();
        let files =
            LibraryFiles::open([(first, root_a.clone()), (second, root_b)], &root_a).unwrap();
        let mut state = crate::test_support::test_state();
        state.pool = app_pool;
        state.ingestion_pool = ingestion_pool.clone();
        state.library_files = files.clone();
        let hash = |dir: &tempfile::TempDir| {
            use std::fmt::Write;
            Sha256::digest(std::fs::read(dir.path().join("Nested/book.epub")).unwrap())
                .iter()
                .fold(String::new(), |mut text, byte| {
                    write!(text, "{byte:02x}").unwrap();
                    text
                })
        };
        let first_hash = hash(&a);
        let second_hash = hash(&b);
        let (work, first_file) =
            insert_cover_manifestation(&ingestion_pool, first, "Nested/book.epub", &first_hash)
                .await;
        let (_, second_file) =
            insert_cover_manifestation(&ingestion_pool, second, "Nested/book.epub", &second_hash)
                .await;
        let first_bytes = artifact_bytes(
            get_or_create(&state, first_file, user, CoverSize::Full)
                .await
                .unwrap(),
        );
        let second_bytes = artifact_bytes(
            get_or_create(&state, second_file, user, CoverSize::Full)
                .await
                .unwrap(),
        );
        assert_eq!(
            image::guess_format(&first_bytes).unwrap(),
            image::ImageFormat::Jpeg
        );
        assert_eq!(
            image::guess_format(&second_bytes).unwrap(),
            image::ImageFormat::Png
        );
        assert_ne!(first_bytes, second_bytes);
        assert!(
            !a.path()
                .join(format!(
                    "_covers/cache/{second_file}-{}-full.png",
                    &second_hash[..16]
                ))
                .exists()
        );
        sqlx::query!(
            "UPDATE works SET title = $1 WHERE id = $2",
            "Renamed title",
            work
        )
        .execute(&ingestion_pool)
        .await
        .unwrap();
        crate::test_support::db::insert_contributor(
            &ingestion_pool,
            work,
            "Renamed Author",
            "author",
            0,
        )
        .await;
        assert_eq!(
            artifact_bytes(
                get_or_create(&state, first_file, user, CoverSize::Full)
                    .await
                    .unwrap()
            ),
            first_bytes
        );
        assert!(a.path().join("Nested/book.epub").exists());
        std::fs::remove_file(b.path().join("Nested/book.epub")).unwrap();
        assert_eq!(
            artifact_bytes(
                get_or_create(&state, second_file, user, CoverSize::Full)
                    .await
                    .unwrap()
            ),
            second_bytes
        );

        let wrong_cache = CoverCache::new(files.library(first).unwrap()).unwrap();
        wrong_cache
            .publish(
                second_file,
                &second_hash,
                CoverSize::Full,
                CoverEncoding::Png,
                &second_bytes,
            )
            .unwrap();
        state.library_files = crate::test_support::test_library_files_at(&root_a, first);
        assert!(matches!(
            get_or_create(&state, second_file, user, CoverSize::Full).await,
            Err(CoverError::Library(LibraryFileError::UnknownLibrary))
        ));
    }
}
