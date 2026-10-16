//! Metadata enrichment pipeline — three-layer architecture (journal, policy,
//! canonical pointers).
//!
//! Each submodule owns a single concern: `HTTP` transport and `SSRF` guards
//! (`http`), cache read/write (`cache`), source fan-out and canonical
//! write-back (`orchestrator`), lock management (`field_lock`), and so on.
//! The public entry point for production use is `spawn_queue`.

/// Per-`(source, lookup_key)` response cache with per-kind `TTL` enforcement.
pub mod cache;
/// Confidence score computation combining source weight, match quality, and quorum.
pub mod confidence;
/// Cover image download, validation, and atomic staging.
pub mod cover_download;
/// Dry-run preview: simulates enrichment without mutating canonical columns.
pub mod dry_run;
/// `CRUD` helpers for `field_locks` — pin a field against further auto-enrichment.
pub mod field_lock;
/// `SSRF`-safe `HTTP` client factory for metadata and cover image fetches.
pub mod http;
/// Canonical cache-key derivation from `ISBN` and title/author pairs.
pub mod lookup_key;
/// Per-manifestation enrichment orchestrator: fan-out, journal, canonical write-back.
pub mod orchestrator;
/// Field-level policy engine: `AutoFill`, `Propose`, or `Lock` per field.
pub mod policy;
/// Background queue worker with `FOR UPDATE SKIP LOCKED` claim and exponential backoff.
pub mod queue;
/// Pluggable metadata source implementations (`OpenLibrary`, `GoogleBooks`, Hardcover).
pub mod sources;
/// Canonical-`JSON` + `SHA-256` hashing for `metadata_versions` deduplication.
pub mod value_hash;

pub use queue::spawn_queue;

pub(crate) enum RetryTiming {
    PreserveProviderDelay,
    ClearProviderDelay,
}

pub(crate) async fn reset_retries(
    connection: &mut sqlx::PgConnection,
    ids: &[uuid::Uuid],
    timing: RetryTiming,
) -> sqlx::Result<u64> {
    let clear_delay = matches!(timing, RetryTiming::ClearProviderDelay);
    let rows = sqlx::query!(
        "UPDATE manifestations
         SET enrichment_rerun_requested = (enrichment_status = 'in_progress'),
             enrichment_status = CASE WHEN enrichment_status = 'in_progress'
                                      THEN enrichment_status ELSE 'pending'::enrichment_status END,
             enrichment_attempt_count = CASE WHEN enrichment_status = 'in_progress'
                                             THEN enrichment_attempt_count ELSE 0 END,
             enrichment_attempted_at = CASE WHEN enrichment_status = 'in_progress'
                                            THEN enrichment_attempted_at ELSE NULL END,
             enrichment_retry_not_before = CASE WHEN enrichment_status = 'in_progress' OR NOT $2
                                                THEN enrichment_retry_not_before ELSE NULL END,
             enrichment_error = CASE WHEN enrichment_status = 'in_progress'
                                     THEN enrichment_error ELSE NULL END,
             enrichment_failures = CASE WHEN enrichment_status = 'in_progress'
                                        THEN enrichment_failures ELSE '[]'::jsonb END
         WHERE id = ANY($1)",
        ids,
        clear_delay,
    )
    .execute(connection)
    .await?;
    Ok(rows.rows_affected())
}
