//! Writeback-worker configuration ([`WritebackConfig`]).

use validator::Validate;

/// Writeback-worker knobs (the background task that flushes pending
/// canonical-metadata mutations into the source manifestation files).
#[derive(Debug, Clone, serde::Deserialize, schemars::JsonSchema, Validate)]
#[serde(default)]
pub struct WritebackConfig {
    /// Whether the writeback worker is spawned
    /// (`REVERIE_WRITEBACK_ENABLED`, default `true`).
    pub enabled: bool,
    /// In-flight writeback job concurrency
    /// (`REVERIE_WRITEBACK_CONCURRENCY`, default `2`; valid range 1-10).
    #[validate(range(min = 1, max = 10, message = "must be between 1 and 10"))]
    pub concurrency: u32,
    /// Sleep between empty-queue polls
    /// (`REVERIE_WRITEBACK_POLL_IDLE_SECS`, default `5`).
    pub poll_idle_secs: u64,
    /// Maximum retry attempts before a writeback job is considered
    /// exhausted (`REVERIE_WRITEBACK_MAX_ATTEMPTS`, default `10`).
    #[validate(range(min = 1, message = "must be at least 1"))]
    pub max_attempts: u32,
}

impl Default for WritebackConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            concurrency: 2,
            poll_idle_secs: 5,
            max_attempts: 10,
        }
    }
}
