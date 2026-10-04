//! Persisted operator-tunable settings (single-row `settings` table).
//!
//! See ADR `docs/adr/0012-persist-operator-tunable-settings-to-database-with-live-reload.md` for storage shape,
//! precedence, and reload decisions.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::models::manifestation_format::ManifestationFormat;

/// Live acquisition and source-cleanup policy.
#[derive(Debug, Clone, Serialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct IngestionSettings {
    /// Formats accepted for ingestion; currently only EPUB is supported.
    #[schema(value_type = Vec<ManifestationFormat>)]
    pub accepted_formats: Vec<String>,
    /// Remove an imported source after its outcome is committed.
    pub cleanup_imported: bool,
    /// Remove a duplicate source after its outcome is committed.
    pub cleanup_duplicates: bool,
}

/// Runtime-tunable settings loaded from the `settings` table.
///
/// Fields map 1:1 to the singleton row columns. The struct is held in
/// `AppState` behind an `Arc<RwLock<Settings>>` and refreshed via
/// LISTEN/NOTIFY + 60-second fallback poll.
#[derive(Debug, Clone, Serialize, sqlx::FromRow, utoipa::ToSchema)]
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

    /// Ingestion policy exposed as flat settings fields.
    #[serde(flatten)]
    #[sqlx(flatten)]
    pub ingestion: IngestionSettings,

    /// Per-provider display visibility for external identifiers and ratings
    /// (`{"googlebooks": false}` hides that provider from projections).
    /// Keys are validated on write against the union of `identifier_schemes`
    /// and `rating_sources`; absent keys mean visible. Display-only: hiding
    /// a provider never gates fetching.
    #[schema(value_type = Object)]
    pub provider_visibility: serde_json::Value,

    /// DB-generated monotonic row version, incremented inside every settings
    /// UPDATE. Cache writers only install a snapshot whose revision exceeds
    /// the resident one, which makes the shared settings cache independent
    /// of lock-acquisition order and transaction-start timestamps
    /// (`updated_at` is transaction-start time and can move backwards across
    /// concurrent writers). Not operator-settable.
    pub revision: i64,

    /// DB-assigned (`now()` on every UPDATE); not operator-settable.
    // Wire-facing despite this struct also being the in-process settings
    // cache: it is `#[serde(flatten)]`ed into the `GET /api/v1/settings`
    // response.
    pub updated_at: DateTime<Utc>,
}

// Fields that require a process restart to take effect (env-only:
// port, database_url, OIDC, library_path). Currently empty because
// no restart-required fields are in the settings table yet.

/// Partial update request for `PUT /api/v1/settings`.
///
/// All fields optional — absent fields are left unchanged (JSON Merge
/// Patch semantics per RFC 7396). `ManifestationFormat` is validated at
/// deserialization time; EPUB-only membership and duplicates are checked by
/// [`validate_update`].
#[derive(Debug, Deserialize, utoipa::ToSchema)]
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

    /// Formats accepted for ingestion; an empty list suspends acquisition.
    pub accepted_formats: Option<Vec<ManifestationFormat>>,
    /// Remove imported source files.
    pub cleanup_imported: Option<bool>,
    /// Remove duplicate source files.
    pub cleanup_duplicates: Option<bool>,

    /// Per-provider display visibility, replacing the stored map wholesale.
    /// Values must be booleans; keys are validated against the union of
    /// `identifier_schemes` and `rating_sources`.
    #[schema(schema_with = provider_visibility_schema)]
    pub provider_visibility: Option<std::collections::BTreeMap<String, bool>>,
}

/// Size of the union of `identifier_schemes` and `rating_sources` (11 rows,
/// seeded in the initial migration) with headroom:
/// bounds `provider_visibility` entries accepted in one patch, checked in
/// [`validate_update`] before the registry lookup in `validate_provider_keys`.
const MAX_PROVIDER_VISIBILITY_ENTRIES: usize = 64;

/// Schema for `provider_visibility`: a nullable map of provider id to
/// boolean, bounded by [`MAX_PROVIDER_VISIBILITY_ENTRIES`]. utoipa 5.5's
/// `#[schema(...)]` derive attribute has no `max_properties`, so the bound
/// is built by hand.
fn provider_visibility_schema() -> utoipa::openapi::schema::Object {
    use utoipa::openapi::schema::{ObjectBuilder, SchemaType, Type};
    ObjectBuilder::new()
        .schema_type(SchemaType::from_iter([Type::Object, Type::Null]))
        .description(Some(
            "Per-provider display visibility, replacing the stored map wholesale.\n\
             Values must be booleans; keys are validated against the union of\n\
             `identifier_schemes` and `rating_sources`.",
        ))
        .additional_properties(Some(ObjectBuilder::new().schema_type(Type::Boolean)))
        .property_names(Some(ObjectBuilder::new().schema_type(Type::String)))
        .max_properties(Some(MAX_PROVIDER_VISIBILITY_ENTRIES))
        .build()
}

impl UpdateSettings {
    /// Returns true if the update touches no fields (empty body).
    #[must_use]
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
            && self.accepted_formats.is_none()
            && self.cleanup_imported.is_none()
            && self.cleanup_duplicates.is_none()
            && self.provider_visibility.is_none()
    }
}

/// Validate an [`UpdateSettings`] payload.
///
/// Serde validates enum membership (`ManifestationFormat`)
/// at deserialization time. This function validates range constraints and
/// EPUB-only membership and duplicate entries.
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
    if let Some(ref fp) = req.accepted_formats {
        let mut seen = std::collections::HashSet::new();
        for f in fp {
            if *f != ManifestationFormat::Epub {
                return Err("accepted_formats supports only epub".into());
            }
            if !seen.insert(f) {
                return Err(format!("accepted_formats contains duplicate format: {f}"));
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
    if let Some(ref v) = req.provider_visibility
        && v.len() > MAX_PROVIDER_VISIBILITY_ENTRIES
    {
        return Err(format!(
            "provider_visibility exceeds {MAX_PROVIDER_VISIBILITY_ENTRIES} entries"
        ));
    }
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
#[must_use]
pub const fn has_restart_required_field(_req: &UpdateSettings) -> bool {
    false
}

/// List of restart-required field names (for frontend display).
///
/// Returns an empty slice until restart-required fields are actually
/// added to the settings table. The env-only fields that would require
/// restart (`port`, `database_url`, OIDC, `library_path`) are not in the
/// PUT schema and cannot be set through this API.
#[must_use]
pub const fn restart_required_fields() -> &'static [&'static str] {
    &[]
}
