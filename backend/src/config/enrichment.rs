//! Metadata-enrichment subsystem configuration ([`EnrichmentConfig`]).

use validator::Validate;

/// Metadata-enrichment subsystem knobs (background workers that fetch
/// from `OpenLibrary` / Google Books / Hardcover).
#[derive(Debug, Clone, serde::Deserialize, schemars::JsonSchema, Validate)]
#[serde(default)]
pub struct EnrichmentConfig {
    /// Whether the enrichment queue is spawned
    /// (`REVERIE_ENRICHMENT_ENABLED`, default `true`).
    pub enabled: bool,
    /// In-flight enrichment job concurrency
    /// (`REVERIE_ENRICHMENT_CONCURRENCY`, default `2`; valid range 1-10).
    #[validate(range(min = 1, max = 10, message = "must be between 1 and 10"))]
    pub concurrency: u32,
    /// Sleep between empty-queue polls
    /// (`REVERIE_ENRICHMENT_POLL_IDLE_SECS`, default `30`).
    pub poll_idle_secs: u64,
    /// Per-job overall fetch budget
    /// (`REVERIE_ENRICHMENT_FETCH_BUDGET_SECS`, default `15`).
    pub fetch_budget_secs: u64,
    /// Per-request HTTP timeout for outbound metadata fetches
    /// (`REVERIE_ENRICHMENT_HTTP_TIMEOUT_SECS`, default `10`).
    pub http_timeout_secs: u64,
    /// Maximum retry attempts before a job is considered exhausted
    /// (`REVERIE_ENRICHMENT_MAX_ATTEMPTS`, default `10`).
    #[validate(range(min = 1, message = "must be at least 1"))]
    pub max_attempts: u32,
    /// Cache TTL for successful (`hit`) responses
    /// (`REVERIE_ENRICHMENT_CACHE_TTL_HIT_DAYS`, default `30`).
    pub cache_ttl_hit_days: u32,
    /// Cache TTL for "not found" (`miss`) responses
    /// (`REVERIE_ENRICHMENT_CACHE_TTL_MISS_DAYS`, default `7`).
    pub cache_ttl_miss_days: u32,
    /// Cache TTL for transient-error responses
    /// (`REVERIE_ENRICHMENT_CACHE_TTL_ERROR_MINS`, default `15`).
    pub cache_ttl_error_mins: u32,
}

impl Default for EnrichmentConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            concurrency: 2,
            poll_idle_secs: 30,
            fetch_budget_secs: 15,
            http_timeout_secs: 10,
            max_attempts: 10,
            cache_ttl_hit_days: 30,
            cache_ttl_miss_days: 7,
            cache_ttl_error_mins: 15,
        }
    }
}
