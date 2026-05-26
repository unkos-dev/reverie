//! Persisted operator-tunable settings (single-row `settings` table).
//!
//! See ADR `adr/2026-05-26-persisted-settings.md` for storage shape,
//! precedence, and reload decisions.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::config::CleanupMode;
use crate::models::manifestation_format::ManifestationFormat;

/// Runtime-tunable settings loaded from the `settings` table.
///
/// Fields map 1:1 to the singleton row columns. The struct is held in
/// `AppState` behind an `Arc<RwLock<Settings>>` and refreshed via
/// LISTEN/NOTIFY + 60-second fallback poll.
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

    /// DB-assigned (`now()` on every UPDATE); not operator-settable.
    pub updated_at: OffsetDateTime,
}

impl Settings {
    /// Parse `cleanup_mode` text column into the typed enum.
    ///
    /// DB CHECK constraint guarantees valid values; panicking on
    /// mismatch is correct (schema-vs-code drift = programming error).
    ///
    /// # Panics
    /// Panics if the stored value is outside `"all" | "ingested" | "none"`.
    pub fn cleanup_mode(&self) -> CleanupMode {
        match self.cleanup_mode.as_str() {
            "all" => CleanupMode::All,
            "ingested" => CleanupMode::Ingested,
            "none" => CleanupMode::None,
            other => unreachable!("DB CHECK constraint violated: cleanup_mode = {other:?}"),
        }
    }

    /// Parse `format_priority` text[] column into typed enums.
    ///
    /// DB values were validated on write; panicking on mismatch is
    /// correct (schema-vs-code drift = programming error).
    ///
    /// # Panics
    /// Panics if any stored element is not a known [`ManifestationFormat`] wire value.
    pub fn format_priority(&self) -> Vec<ManifestationFormat> {
        self.format_priority
            .iter()
            .map(|s| {
                s.parse::<ManifestationFormat>()
                    .unwrap_or_else(|_| unreachable!("DB validated format_priority element: {s:?}"))
            })
            .collect()
    }
}

// Fields that require a process restart to take effect (env-only:
// port, database_url, OIDC, library_path). Currently empty because
// no restart-required fields are in the settings table yet.

/// Partial update request for `PUT /api/settings`.
///
/// All fields optional — absent fields are left unchanged (JSON Merge
/// Patch semantics per RFC 7396). `CleanupMode` and
/// `ManifestationFormat` are validated at deserialization time by serde;
/// collection invariants (non-empty, no duplicates) are checked by
/// [`validate_update`].
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
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
    pub format_priority: Option<Vec<ManifestationFormat>>,
    /// Post-ingestion cleanup mode.
    pub cleanup_mode: Option<CleanupMode>,

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
        let parsed =
            url::Url::parse(v).map_err(|e| format!("{field_name} must be a valid URL: {e}"))?;
        if !["http", "https"].contains(&parsed.scheme()) {
            return Err(format!("{field_name} must use http or https scheme"));
        }
    }
    Ok(())
}

/// Validate an [`UpdateSettings`] payload.
///
/// Serde validates enum membership (`CleanupMode`, `ManifestationFormat`)
/// at deserialization time. This function validates range constraints and
/// collection invariants (non-empty, no duplicates).
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
        let mut seen = std::collections::HashSet::new();
        for f in fp {
            if !seen.insert(f) {
                return Err(format!("format_priority contains duplicate format: {f}"));
            }
        }
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

/// Forward-compatibility stub: returns whether any field in the update
/// would require a process restart.
///
/// Currently always returns `false` because the `settings` table contains
/// only hot-reloadable fields. Restart-required fields (`port`, `database_url`,
/// OIDC, `library_path`) are env-only (`Config`) and cannot be PUT. The API
/// response includes `restart_required` so the frontend can surface a badge
/// when restart-required fields are eventually promoted to the table.
pub const fn has_restart_required_field(_req: &UpdateSettings) -> bool {
    false
}

/// List of restart-required field names (for frontend display).
///
/// Returns an empty slice until restart-required fields are actually
/// added to the settings table. The env-only fields that would require
/// restart (`port`, `database_url`, OIDC, `library_path`) are not in the
/// PUT schema and cannot be set through this API.
pub const fn restart_required_fields() -> &'static [&'static str] {
    &[]
}
