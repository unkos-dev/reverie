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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApiCacheKind {
    Hit,
    Miss,
    Error,
}

impl ApiCacheKind {
    fn as_str(self) -> &'static str {
        match self {
            ApiCacheKind::Hit => "hit",
            ApiCacheKind::Miss => "miss",
            ApiCacheKind::Error => "error",
        }
    }
}

#[allow(dead_code)] // only reached via `read`, which is covered by tests but not yet called from the binary.
fn kind_from_str(s: &str) -> Result<ApiCacheKind, sqlx::Error> {
    match s {
        "hit" => Ok(ApiCacheKind::Hit),
        "miss" => Ok(ApiCacheKind::Miss),
        "error" => Ok(ApiCacheKind::Error),
        other => Err(sqlx::Error::Decode(
            format!("unknown api_cache_kind value: {other}").into(),
        )),
    }
}

/// A live (non-expired) cache row returned by [`read`].
#[derive(Debug, Clone)]
#[allow(dead_code)] // fields populated for consumers of `read`; orchestrator only writes today.
pub struct CachedResponse {
    pub response: Value,
    pub kind: ApiCacheKind,
    pub http_status: Option<i32>,
    pub fetched_at: OffsetDateTime,
}

/// Per-kind TTL configuration passed to [`write`].
pub struct CacheTtls {
    pub hit: time::Duration,
    pub miss: time::Duration,
    pub error: time::Duration,
}

/// Read a live cache entry for `(source, lookup_key)`.
///
/// Returns `None` if no row exists or the row is expired.
#[allow(dead_code)] // orchestrator only writes today; read is exercised by the integration tests.
pub async fn read(
    pool: &PgPool,
    source: &str,
    lookup_key: &str,
) -> sqlx::Result<Option<CachedResponse>> {
    let row = sqlx::query(
        "SELECT response, response_kind::text AS response_kind, http_status, fetched_at \
         FROM api_cache \
         WHERE source = $1 AND lookup_key = $2 AND expires_at > now()",
    )
    .bind(source)
    .bind(lookup_key)
    .fetch_optional(pool)
    .await?;

    let Some(row) = row else {
        return Ok(None);
    };

    use sqlx::Row;
    let response: Value = row.try_get("response")?;
    let kind_str: String = row.try_get("response_kind")?;
    let kind = kind_from_str(&kind_str)?;
    let http_status: Option<i32> = row.try_get("http_status")?;
    let fetched_at: OffsetDateTime = row.try_get("fetched_at")?;

    Ok(Some(CachedResponse {
        response,
        kind,
        http_status,
        fetched_at,
    }))
}

/// Insert or update a cache row for `(source, lookup_key)`.
///
/// The `expires_at` timestamp is computed in Rust as
/// `now + ttls.<kind>` so the TTL logic stays testable without a DB.
/// On conflict the existing row is fully replaced.
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

    sqlx::query(
        "INSERT INTO api_cache \
             (source, lookup_key, response, response_kind, http_status, fetched_at, expires_at) \
         VALUES ($1, $2, $3, $4::api_cache_kind, $5, $6, $7) \
         ON CONFLICT (source, lookup_key) DO UPDATE SET \
             response      = EXCLUDED.response, \
             response_kind = EXCLUDED.response_kind, \
             http_status   = EXCLUDED.http_status, \
             fetched_at    = EXCLUDED.fetched_at, \
             expires_at    = EXCLUDED.expires_at",
    )
    .bind(source)
    .bind(lookup_key)
    .bind(response)
    .bind(kind.as_str())
    .bind(http_status)
    .bind(now)
    .bind(expires_at)
    .execute(pool)
    .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use time::Duration;

    fn db_url() -> String {
        std::env::var("DATABASE_URL_INGESTION").unwrap_or_else(|_| {
            "postgres://reverie_ingestion:reverie_ingestion@localhost:5433/reverie_dev".into()
        })
    }

    const TEST_SOURCE: &str = "test-cache";

    fn ttls_standard() -> CacheTtls {
        CacheTtls {
            hit: Duration::hours(1),
            miss: Duration::minutes(5),
            error: Duration::minutes(1),
        }
    }

    async fn cleanup(pool: &PgPool) {
        let _ = sqlx::query("DELETE FROM api_cache WHERE source = $1 AND lookup_key LIKE 'test-%'")
            .bind(TEST_SOURCE)
            .execute(pool)
            .await;
    }

    #[tokio::test]
    #[ignore]
    async fn write_then_read_roundtrip() {
        let pool = PgPool::connect(&db_url()).await.unwrap();
        cleanup(&pool).await;

        let key = "test-roundtrip";
        let payload = json!({"title": "Dune", "author": "Frank Herbert"});

        write(
            &pool,
            TEST_SOURCE,
            key,
            &payload,
            ApiCacheKind::Hit,
            Some(200),
            &ttls_standard(),
        )
        .await
        .unwrap();

        let cached = read(&pool, TEST_SOURCE, key).await.unwrap();
        let cached = cached.expect("expected a cache hit");

        assert_eq!(cached.response, payload);
        assert_eq!(cached.kind, ApiCacheKind::Hit);
        assert_eq!(cached.http_status, Some(200));

        cleanup(&pool).await;
    }

    #[tokio::test]
    #[ignore]
    async fn expired_entry_returns_none() {
        let pool = PgPool::connect(&db_url()).await.unwrap();
        cleanup(&pool).await;

        let key = "test-expired";
        let ttls = CacheTtls {
            hit: Duration::ZERO,
            miss: Duration::ZERO,
            error: Duration::ZERO,
        };

        write(
            &pool,
            TEST_SOURCE,
            key,
            &json!({"x": 1}),
            ApiCacheKind::Hit,
            None,
            &ttls,
        )
        .await
        .unwrap();

        let cached = read(&pool, TEST_SOURCE, key).await.unwrap();
        assert!(cached.is_none(), "expired entry should return None");

        cleanup(&pool).await;
    }

    #[tokio::test]
    #[ignore]
    async fn distinct_kinds_get_distinct_expirations() {
        let pool = PgPool::connect(&db_url()).await.unwrap();
        cleanup(&pool).await;

        let key_hit = "test-ttl-hit";
        let key_miss = "test-ttl-miss";

        write(
            &pool,
            TEST_SOURCE,
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
            TEST_SOURCE,
            key_miss,
            &json!(null),
            ApiCacheKind::Miss,
            Some(404),
            &ttls_standard(),
        )
        .await
        .unwrap();

        // expires_at - fetched_at should differ: hit = 1h, miss = 5m.
        let hit_gap: f64 = sqlx::query_scalar(
            "SELECT EXTRACT(EPOCH FROM expires_at - fetched_at)::float8 \
             FROM api_cache WHERE source = $1 AND lookup_key = $2",
        )
        .bind(TEST_SOURCE)
        .bind(key_hit)
        .fetch_one(&pool)
        .await
        .unwrap();

        let miss_gap: f64 = sqlx::query_scalar(
            "SELECT EXTRACT(EPOCH FROM expires_at - fetched_at)::float8 \
             FROM api_cache WHERE source = $1 AND lookup_key = $2",
        )
        .bind(TEST_SOURCE)
        .bind(key_miss)
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

        cleanup(&pool).await;
    }

    /// ISBN-10 and ISBN-13 of the same book resolve to one cache row via
    /// `lookup_key::isbn_key` — the cache sees a single canonical key, so a
    /// write via one form is visible via the other.
    #[tokio::test]
    #[ignore]
    async fn isbn10_and_isbn13_dedupe_via_lookup_key() {
        use crate::services::enrichment::lookup_key;

        let pool = PgPool::connect(&db_url()).await.unwrap();
        cleanup(&pool).await;

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
            TEST_SOURCE,
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
        let cached = read(&pool, TEST_SOURCE, &roundtrip_key)
            .await
            .unwrap()
            .expect("ISBN-13 key should hit the ISBN-10-written row");
        assert_eq!(cached.response, payload);

        cleanup(&pool).await;
    }

    #[tokio::test]
    #[ignore]
    async fn upsert_overwrites_previous_value() {
        let pool = PgPool::connect(&db_url()).await.unwrap();
        cleanup(&pool).await;

        let key = "test-upsert";

        write(
            &pool,
            TEST_SOURCE,
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
            TEST_SOURCE,
            key,
            &json!({"v": 2}),
            ApiCacheKind::Miss,
            Some(404),
            &ttls_standard(),
        )
        .await
        .unwrap();

        let cached = read(&pool, TEST_SOURCE, key).await.unwrap().unwrap();
        assert_eq!(cached.response, json!({"v": 2}));
        assert_eq!(cached.kind, ApiCacheKind::Miss);
        assert_eq!(cached.http_status, Some(404));

        cleanup(&pool).await;
    }
}
