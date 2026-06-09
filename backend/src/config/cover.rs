//! Cover-image acquisition configuration ([`CoverConfig`]).

use validator::Validate;

/// Cover-image acquisition limits applied by the cover service when
/// fetching from third-party metadata providers.
#[derive(Debug, Clone, serde::Deserialize, schemars::JsonSchema, Validate)]
#[serde(default)]
pub struct CoverConfig {
    /// Maximum bytes accepted per cover image
    /// (`REVERIE_COVER_MAX_BYTES`, default `10_485_760` — 10 MiB).
    pub max_bytes: u64,
    /// Per-download HTTP timeout
    /// (`REVERIE_COVER_DOWNLOAD_TIMEOUT_SECS`, default `30`).
    pub download_timeout_secs: u64,
    /// Minimum long-edge pixel dimension; smaller images are rejected
    /// (`REVERIE_COVER_MIN_LONG_EDGE_PX`, default `1000`).
    pub min_long_edge_px: u32,
    /// Maximum HTTP redirect hops the cover fetcher will follow
    /// (`REVERIE_COVER_REDIRECT_LIMIT`, default `3`).
    pub redirect_limit: usize,
}

impl Default for CoverConfig {
    fn default() -> Self {
        Self {
            max_bytes: 10_485_760,
            download_timeout_secs: 30,
            min_long_edge_px: 1000,
            redirect_limit: 3,
        }
    }
}
