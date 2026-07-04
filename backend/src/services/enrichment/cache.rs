//! Read/write the `api_cache` table with per-kind TTL enforcement.
//!
//! Cache rows record the result of an external API call keyed on
//! `(source, lookup_key)`.  Each row expires at `expires_at`; stale rows are
//! invisible to `read` (filtered by `expires_at > now()`).

// Phase B building block: callers are wired in Phase C.  Until then this module
// is unused from the binary entry point but is fully tested.

use serde_json::Value;
use sqlx::PgPool;
use time::OffsetDateTime;

/// The kind of API response recorded in a cache row.
///
/// Mapped via `sqlx::Type` to the Postgres `api_cache_kind` ENUM so an
/// unknown DB variant surfaces as a decode error rather than coercing
/// into a silent fallback.
#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::Type)]
#[sqlx(type_name = "api_cache_kind", rename_all = "lowercase")]
pub enum ApiCacheKind {
    /// The source returned a usable result.
    Hit,
    /// The source confirmed the key does not exist (e.g. `HTTP 404`).
    Miss,
    /// The source returned an error (e.g. rate-limit, server fault).
    Error,
}

/// A live (non-expired) cache row returned by [`read`].
#[derive(Debug, Clone)]
pub struct CachedResponse {
    /// Raw `JSON` body returned by the upstream source.
    pub response: Value,
    /// Whether the stored response was a hit, miss, or error.
    pub kind: ApiCacheKind,
    /// `HTTP` status code from the upstream response, if applicable.
    pub http_status: Option<i32>,
    /// Timestamp when the upstream call was made (stored in `UTC`).
    pub fetched_at: OffsetDateTime,
}

/// Per-kind `TTL` configuration passed to `write`.
pub struct CacheTtls {
    /// How long to keep a successful hit before expiry.
    pub hit: time::Duration,
    /// How long to remember a confirmed miss (avoids immediate re-lookup).
    pub miss: time::Duration,
    /// How long to suppress retries after a source error.
    pub error: time::Duration,
}

/// Read a live cache entry for `(source, lookup_key)`.
///
/// Returns `None` if no row exists or the row is expired (`expires_at <= now()`).
///
/// # Errors
///
/// Returns a [`sqlx::Error`] if the query fails (connection error, decode failure, etc.).
pub async fn read(
    pool: &PgPool,
    source: &str,
    lookup_key: &str,
) -> sqlx::Result<Option<CachedResponse>> {
    // `AS "response_kind!: ApiCacheKind"` decodes the `api_cache_kind`
    // column via the `sqlx::Type` impl. `!` restores NOT NULL inference
    // dropped by the column override; an unknown PG variant becomes a
    // decode error rather than a silent fallback.
    let row = sqlx::query!(
        "SELECT response, response_kind AS \"response_kind!: ApiCacheKind\", http_status, fetched_at \
         FROM api_cache \
         WHERE source = $1 AND lookup_key = $2 AND expires_at > now()",
        source,
        lookup_key,
    )
    .fetch_optional(pool)
    .await?;

    let Some(row) = row else {
        return Ok(None);
    };

    Ok(Some(CachedResponse {
        response: row.response,
        kind: row.response_kind,
        http_status: row.http_status,
        fetched_at: row.fetched_at,
    }))
}

/// Insert or update a cache row for `(source, lookup_key)`.
///
/// The `expires_at` timestamp is computed in Rust as
/// `now + ttls.<kind>` so the `TTL` logic stays testable without a DB.
/// On conflict the existing row is fully replaced.
///
/// # Errors
///
/// Returns a [`sqlx::Error`] if the upsert fails.
pub async fn write(
    pool: &PgPool,
    source: &str,
    lookup_key: &str,
    response: &Value,
    kind: ApiCacheKind,
    http_status: Option<i32>,
    ttls: &CacheTtls,
) -> sqlx::Result<()> {
    let now = OffsetDateTime::now_utc();
    let ttl = match kind {
        ApiCacheKind::Hit => ttls.hit,
        ApiCacheKind::Miss => ttls.miss,
        ApiCacheKind::Error => ttls.error,
    };
    let expires_at = now + ttl;

    sqlx::query!(
        "INSERT INTO api_cache \
             (source, lookup_key, response, response_kind, http_status, fetched_at, expires_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7) \
         ON CONFLICT (source, lookup_key) DO UPDATE SET \
             response      = EXCLUDED.response, \
             response_kind = EXCLUDED.response_kind, \
             http_status   = EXCLUDED.http_status, \
             fetched_at    = EXCLUDED.fetched_at, \
             expires_at    = EXCLUDED.expires_at",
        source,
        lookup_key,
        response,
        kind as ApiCacheKind,
        http_status,
        now,
        expires_at,
    )
    .execute(pool)
    .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::db::ingestion_pool_for;
    use serde_json::json;
    use time::Duration;

    fn ttls_standard() -> CacheTtls {
        CacheTtls {
            hit: Duration::hours(1),
            miss: Duration::minutes(5),
            error: Duration::minutes(1),
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn write_then_read_roundtrip(pool: PgPool) {
        let pool = ingestion_pool_for(&pool).await;
        let source = "test-cache-roundtrip";
        let key = "test-roundtrip";
        let payload = json!({"title": "Dune", "author": "Frank Herbert"});

        write(
            &pool,
            source,
            key,
            &payload,
            ApiCacheKind::Hit,
            Some(200),
            &ttls_standard(),
        )
        .await
        .unwrap();

        let cached = read(&pool, source, key).await.unwrap();
        let cached = cached.expect("expected a cache hit");

        assert_eq!(cached.response, payload);
        assert_eq!(cached.kind, ApiCacheKind::Hit);
        assert_eq!(cached.http_status, Some(200));
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn expired_entry_returns_none(pool: PgPool) {
        let pool = ingestion_pool_for(&pool).await;
        let source = "test-cache-expired";
        let key = "test-expired";
        let ttls = CacheTtls {
            hit: Duration::ZERO,
            miss: Duration::ZERO,
            error: Duration::ZERO,
        };

        write(
            &pool,
            source,
            key,
            &json!({"x": 1}),
            ApiCacheKind::Hit,
            None,
            &ttls,
        )
        .await
        .unwrap();

        let cached = read(&pool, source, key).await.unwrap();
        assert!(cached.is_none(), "expired entry should return None");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn distinct_kinds_get_distinct_expirations(pool: PgPool) {
        let pool = ingestion_pool_for(&pool).await;
        let source = "test-cache-ttl";
        let key_hit = "test-ttl-hit";
        let key_miss = "test-ttl-miss";

        write(
            &pool,
            source,
            key_hit,
            &json!(null),
            ApiCacheKind::Hit,
            Some(200),
            &ttls_standard(),
        )
        .await
        .unwrap();

        write(
            &pool,
            source,
            key_miss,
            &json!(null),
            ApiCacheKind::Miss,
            Some(404),
            &ttls_standard(),
        )
        .await
        .unwrap();

        // expires_at - fetched_at should differ: hit = 1h, miss = 5m.
        let hit_gap = sqlx::query_scalar!(
            "SELECT EXTRACT(EPOCH FROM expires_at - fetched_at)::float8 AS \"gap!\" \
             FROM api_cache WHERE source = $1 AND lookup_key = $2",
            source,
            key_hit,
        )
        .fetch_one(&pool)
        .await
        .unwrap();

        let miss_gap = sqlx::query_scalar!(
            "SELECT EXTRACT(EPOCH FROM expires_at - fetched_at)::float8 AS \"gap!\" \
             FROM api_cache WHERE source = $1 AND lookup_key = $2",
            source,
            key_miss,
        )
        .fetch_one(&pool)
        .await
        .unwrap();

        assert!(
            (hit_gap - 3600.0).abs() < 2.0,
            "hit TTL should be ~3600s, got {hit_gap}"
        );
        assert!(
            (miss_gap - 300.0).abs() < 2.0,
            "miss TTL should be ~300s, got {miss_gap}"
        );
        assert!(
            hit_gap > miss_gap,
            "hit TTL {hit_gap} should exceed miss TTL {miss_gap}"
        );
    }

    /// ISBN-10 and ISBN-13 of the same book resolve to one cache row via
    /// `lookup_key::isbn_key` — the cache sees a single canonical key, so a
    /// write via one form is visible via the other.
    #[sqlx::test(migrations = "./migrations")]
    async fn isbn10_and_isbn13_dedupe_via_lookup_key(pool: PgPool) {
        use crate::services::enrichment::lookup_key;

        let pool = ingestion_pool_for(&pool).await;
        let source = "test-cache-isbn-dedupe";

        let key_from_isbn10 = lookup_key::isbn_key("0306406152").expect("valid ISBN-10");
        let key_from_isbn13 = lookup_key::isbn_key("9780306406157").expect("valid ISBN-13");
        assert_eq!(
            key_from_isbn10, key_from_isbn13,
            "lookup_key must converge ISBN-10 and ISBN-13"
        );

        // Prefix with 'test-' to match the cleanup predicate.
        let canonical = format!("test-{key_from_isbn10}");
        let payload = json!({"title": "Dune"});

        write(
            &pool,
            source,
            &canonical,
            &payload,
            ApiCacheKind::Hit,
            Some(200),
            &ttls_standard(),
        )
        .await
        .unwrap();

        // Recompute the key from the ISBN-13 form and ensure the read hits.
        let roundtrip_key = format!(
            "test-{}",
            lookup_key::isbn_key("9780306406157").expect("valid ISBN-13")
        );
        let cached = read(&pool, source, &roundtrip_key)
            .await
            .unwrap()
            .expect("ISBN-13 key should hit the ISBN-10-written row");
        assert_eq!(cached.response, payload);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn upsert_overwrites_previous_value(pool: PgPool) {
        let pool = ingestion_pool_for(&pool).await;
        let source = "test-cache-upsert";
        let key = "test-upsert";

        write(
            &pool,
            source,
            key,
            &json!({"v": 1}),
            ApiCacheKind::Hit,
            Some(200),
            &ttls_standard(),
        )
        .await
        .unwrap();

        write(
            &pool,
            source,
            key,
            &json!({"v": 2}),
            ApiCacheKind::Miss,
            Some(404),
            &ttls_standard(),
        )
        .await
        .unwrap();

        let cached = read(&pool, source, key).await.unwrap().unwrap();
        assert_eq!(cached.response, json!({"v": 2}));
        assert_eq!(cached.kind, ApiCacheKind::Miss);
        assert_eq!(cached.http_status, Some(404));
    }
}
