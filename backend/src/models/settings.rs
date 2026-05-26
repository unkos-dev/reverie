//! Persisted operator-tunable settings (single-row `settings` table).
//!
//! See ADR `adr/2026-05-26-persisted-settings.md` for storage shape,
//! precedence, and reload decisions.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

/// Runtime-tunable settings loaded from the `settings` table.
///
/// Fields map 1:1 to the singleton row columns. The struct is held in
/// `AppState` behind an `Arc<RwLock<Settings>>` and refreshed via
/// LISTEN/NOTIFY + periodic fallback poll.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Settings {
    /// Whether the enrichment pipeline is active.
    pub enrichment_enabled: bool,
    /// Maximum concurrent enrichment workers (1–10).
    pub enrichment_concurrency: i32,
    /// Seconds the enrichment poller sleeps when the queue is empty.
    pub enrichment_poll_idle_secs: i32,
    /// Per-source HTTP fetch budget in seconds.
    pub enrichment_fetch_budget_secs: i32,

    /// Maximum cover image size in bytes.
    pub cover_max_bytes: i64,
    /// Cover download timeout in seconds.
    pub cover_download_timeout_secs: i32,
    /// Minimum cover long-edge resolution in pixels.
    pub cover_min_long_edge_px: i32,
    /// Maximum HTTP redirects when fetching covers.
    pub cover_redirect_limit: i32,

    /// Whether the writeback worker is active.
    pub writeback_enabled: bool,
    /// Maximum concurrent writeback workers (1–10).
    pub writeback_concurrency: i32,
    /// Seconds the writeback poller sleeps when the queue is empty.
    pub writeback_poll_idle_secs: i32,
    /// Maximum writeback retry attempts per job.
    pub writeback_max_attempts: i32,

    /// Whether the OPDS catalogue is mounted.
    pub opds_enabled: bool,
    /// OPDS feed page size (1–500).
    pub opds_page_size: i32,

    /// Ranked format preference for ingestion (`["epub","pdf",…]`).
    pub format_priority: Vec<String>,
    /// Post-ingestion cleanup mode (`all`, `ingested`, or `none`).
    pub cleanup_mode: String,

    /// `OpenLibrary` API base URL.
    pub openlibrary_base_url: String,
    /// Google Books API base URL.
    pub googlebooks_base_url: String,
    /// Hardcover `GraphQL` API base URL.
    pub hardcover_base_url: String,

    /// Last mutation timestamp.
    pub updated_at: OffsetDateTime,
}

/// Fields that require a process restart to take effect.
const RESTART_REQUIRED_FIELDS: &[&str] = &[
    "port",
    "database_url",
    "oidc_issuer_url",
    "oidc_client_id",
    "oidc_client_secret",
    "oidc_redirect_uri",
    "library_path",
];

/// Partial update request for `PUT /api/settings`.
///
/// All fields optional — absent fields are left unchanged (JSON Merge
/// Patch semantics per RFC 7396).
#[derive(Debug, Deserialize)]
pub struct UpdateSettings {
    /// Whether the enrichment pipeline is active.
    pub enrichment_enabled: Option<bool>,
    /// Maximum concurrent enrichment workers (1–10).
    pub enrichment_concurrency: Option<i32>,
    /// Seconds the enrichment poller sleeps when the queue is empty.
    pub enrichment_poll_idle_secs: Option<i32>,
    /// Per-source HTTP fetch budget in seconds.
    pub enrichment_fetch_budget_secs: Option<i32>,

    /// Maximum cover image size in bytes.
    pub cover_max_bytes: Option<i64>,
    /// Cover download timeout in seconds.
    pub cover_download_timeout_secs: Option<i32>,
    /// Minimum cover long-edge resolution in pixels.
    pub cover_min_long_edge_px: Option<i32>,
    /// Maximum HTTP redirects when fetching covers.
    pub cover_redirect_limit: Option<i32>,

    /// Whether the writeback worker is active.
    pub writeback_enabled: Option<bool>,
    /// Maximum concurrent writeback workers (1–10).
    pub writeback_concurrency: Option<i32>,
    /// Seconds the writeback poller sleeps when the queue is empty.
    pub writeback_poll_idle_secs: Option<i32>,
    /// Maximum writeback retry attempts per job.
    pub writeback_max_attempts: Option<i32>,

    /// Whether the OPDS catalogue is mounted.
    pub opds_enabled: Option<bool>,
    /// OPDS feed page size (1–500).
    pub opds_page_size: Option<i32>,

    /// Ranked format preference for ingestion.
    pub format_priority: Option<Vec<String>>,
    /// Post-ingestion cleanup mode (`all`, `ingested`, or `none`).
    pub cleanup_mode: Option<String>,

    /// `OpenLibrary` API base URL.
    pub openlibrary_base_url: Option<String>,
    /// Google Books API base URL.
    pub googlebooks_base_url: Option<String>,
    /// Hardcover `GraphQL` API base URL.
    pub hardcover_base_url: Option<String>,
}

impl UpdateSettings {
    /// Returns true if the update touches no fields (empty body).
    pub const fn is_empty(&self) -> bool {
        self.enrichment_enabled.is_none()
            && self.enrichment_concurrency.is_none()
            && self.enrichment_poll_idle_secs.is_none()
            && self.enrichment_fetch_budget_secs.is_none()
            && self.cover_max_bytes.is_none()
            && self.cover_download_timeout_secs.is_none()
            && self.cover_min_long_edge_px.is_none()
            && self.cover_redirect_limit.is_none()
            && self.writeback_enabled.is_none()
            && self.writeback_concurrency.is_none()
            && self.writeback_poll_idle_secs.is_none()
            && self.writeback_max_attempts.is_none()
            && self.opds_enabled.is_none()
            && self.opds_page_size.is_none()
            && self.format_priority.is_none()
            && self.cleanup_mode.is_none()
            && self.openlibrary_base_url.is_none()
            && self.googlebooks_base_url.is_none()
            && self.hardcover_base_url.is_none()
    }
}

fn validate_url_field(value: Option<&String>, field_name: &str) -> Result<(), String> {
    if let Some(v) = value {
        let parsed = url::Url::parse(v).map_err(|_| format!("{field_name} must be a valid URL"))?;
        if !["http", "https"].contains(&parsed.scheme()) {
            return Err(format!("{field_name} must use http or https scheme"));
        }
    }
    Ok(())
}

/// Validate an [`UpdateSettings`] payload.
///
/// Returns `Err(message)` on first validation failure.
///
/// # Errors
/// Returns a user-facing validation message string.
pub fn validate_update(req: &UpdateSettings) -> Result<(), String> {
    if let Some(c) = req.enrichment_concurrency
        && !(1..=10).contains(&c)
    {
        return Err("enrichment_concurrency must be between 1 and 10".into());
    }
    if let Some(c) = req.writeback_concurrency
        && !(1..=10).contains(&c)
    {
        return Err("writeback_concurrency must be between 1 and 10".into());
    }
    if let Some(ps) = req.opds_page_size
        && !(1..=500).contains(&ps)
    {
        return Err("opds_page_size must be between 1 and 500".into());
    }
    if let Some(ref fp) = req.format_priority {
        if fp.is_empty() {
            return Err("format_priority must not be empty".into());
        }
        let valid = ["epub", "pdf", "mobi", "azw3", "cbz", "cbr"];
        let mut seen = std::collections::HashSet::new();
        for f in fp {
            if !valid.contains(&f.as_str()) {
                return Err(format!("format_priority contains unknown format: {f}"));
            }
            if !seen.insert(f.as_str()) {
                return Err(format!("format_priority contains duplicate format: {f}"));
            }
        }
    }
    if let Some(ref cm) = req.cleanup_mode
        && !["all", "ingested", "none"].contains(&cm.as_str())
    {
        return Err(format!(
            "cleanup_mode must be one of: all, ingested, none (got: {cm})"
        ));
    }
    if let Some(v) = req.enrichment_poll_idle_secs
        && v < 1
    {
        return Err("enrichment_poll_idle_secs must be positive".into());
    }
    if let Some(v) = req.enrichment_fetch_budget_secs
        && v < 1
    {
        return Err("enrichment_fetch_budget_secs must be positive".into());
    }
    if let Some(v) = req.cover_max_bytes
        && v < 1
    {
        return Err("cover_max_bytes must be positive".into());
    }
    if let Some(v) = req.cover_download_timeout_secs
        && v < 1
    {
        return Err("cover_download_timeout_secs must be positive".into());
    }
    if let Some(v) = req.cover_min_long_edge_px
        && v < 1
    {
        return Err("cover_min_long_edge_px must be positive".into());
    }
    if let Some(v) = req.cover_redirect_limit
        && v < 0
    {
        return Err("cover_redirect_limit must be non-negative".into());
    }
    if let Some(v) = req.writeback_poll_idle_secs
        && v < 1
    {
        return Err("writeback_poll_idle_secs must be positive".into());
    }
    if let Some(v) = req.writeback_max_attempts
        && v < 1
    {
        return Err("writeback_max_attempts must be positive".into());
    }
    validate_url_field(req.openlibrary_base_url.as_ref(), "openlibrary_base_url")?;
    validate_url_field(req.googlebooks_base_url.as_ref(), "googlebooks_base_url")?;
    validate_url_field(req.hardcover_base_url.as_ref(), "hardcover_base_url")?;
    Ok(())
}

/// Check whether any field name in an update maps to a restart-required field.
///
/// This is used for non-persisted fields (port, `database_url`, etc.) that the
/// frontend might attempt to PUT. Currently the settings table doesn't include
/// restart-required fields, so this always returns false — but the API response
/// includes the flag for forward-compatibility when restart-required fields are
/// eventually added to the table.
pub const fn has_restart_required_field(_req: &UpdateSettings) -> bool {
    // Currently the settings table contains only hot-reloadable fields.
    // Restart-required fields (port, database_url, OIDC, library_path) live
    // in env-only Config and are not in the settings table.
    false
}

/// List of restart-required field names (for frontend display).
pub const fn restart_required_fields() -> &'static [&'static str] {
    RESTART_REQUIRED_FIELDS
}
