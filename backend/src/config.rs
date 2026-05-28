//! Environment-driven configuration loaded once at startup.
//!
//! [`Config::from_env`] is the production entry point; tests inject a
//! `HashMap`-backed closure through [`Config::from_source`] so test setup
//! never mutates the process environment (UNK-100). Subsystem configs
//! ([`OpdsConfig`], [`EnrichmentConfig`], [`CoverConfig`],
//! [`WritebackConfig`], [`SecurityConfig`]) own their own per-var parsing
//! to keep the `Config::from_source` body shallow.
//!
//! [`SecurityConfig`] is a partial value after `from_env` — the
//! `csp_html_header` / `csp_api_header` fields stay `None` until
//! [`crate::run`] precomputes them from the FOUC-script hash and the
//! configured report endpoint. Responses emit no
//! `Content-Security-Policy` header while those fields remain `None`
//! (see the `if let Some(v)` guards in [`crate::security::headers`]),
//! so embedders bypassing `run` must perform the finalisation pass
//! themselves via [`crate::security::csp`].

use std::env;

use crate::models::manifestation_format::ManifestationFormat;

// Env-var lookup function. `Config::from_env` reads from process env; tests
// inject a `HashMap`-backed closure via `Config::from_source` so test setup
// never mutates global state. UNK-100; lifts
// `debt/2026-05-05-env-lock-config-tests.md`.
type EnvGet<'a> = dyn Fn(&str) -> Option<String> + 'a;

/// Resolved process-wide configuration. Fields reflect the settled view of
/// the environment after defaults, parsing, and validation; subsystem
/// configs (OPDS, enrichment, cover, writeback, security) are nested as
/// owned values so callers do not pass the entire `Config` into helpers
/// that only need one slice.
#[derive(Debug, Clone)]
pub struct Config {
    /// HTTP listen port (`REVERIE_PORT`, default `3000`).
    pub port: u16,
    /// Primary database DSN (`DATABASE_URL`, required). Connections opened
    /// against this DSN run as `reverie_app`; user-facing queries acquire
    /// transactions through [`crate::db::acquire_with_rls`].
    pub database_url: String,
    /// Filesystem root for persisted manifestation files
    /// (`REVERIE_LIBRARY_PATH`, default `./library`). The OPDS download
    /// handler canonicalises file paths against this root.
    pub library_path: String,
    /// Watched ingestion drop directory (`REVERIE_INGESTION_PATH`,
    /// default `./ingestion`). The watcher consumes files from here.
    pub ingestion_path: String,
    /// Failed-ingestion quarantine directory
    /// (`REVERIE_QUARANTINE_PATH`, default `./quarantine`).
    pub quarantine_path: String,
    /// Log-filter directive resolved from the environment with cascading
    /// precedence: `REVERIE_LOG_LEVEL` > `RUST_LOG` > `"info"`. The
    /// `REVERIE_*` operator namespace wins on conflict so staging docs
    /// stay coherent; `RUST_LOG` is honoured as the ecosystem default for
    /// developer convenience. The subscriber filter in [`crate::run`]
    /// parses this string directly — no further env re-read — so the
    /// precedence resolved here is the single source of truth for the
    /// process lifetime.
    pub log_level: String,
    /// Per-pool connection cap (`REVERIE_DB_MAX_CONNECTIONS`, default
    /// `10`); applied identically to the primary, ingestion, and
    /// writeback pools.
    pub db_max_connections: u32,
    /// OIDC issuer URL (`OIDC_ISSUER_URL`, required) — the trust seam
    /// for the entire authentication subsystem. The boundary control
    /// is `reqwest`'s TLS validation against the bundled
    /// webpki/Mozilla root store (`reqwest` is built with the
    /// `rustls` feature, which uses `webpki-roots`, not OS system
    /// roots).
    pub oidc_issuer_url: String,
    /// OIDC client id (`OIDC_CLIENT_ID`, required).
    pub oidc_client_id: String,
    /// OIDC client secret (`OIDC_CLIENT_SECRET`, required). Treated as
    /// secret material — never logged.
    pub oidc_client_secret: String,
    /// OIDC redirect URI (`OIDC_REDIRECT_URI`, required). Must match
    /// the value registered with the issuer.
    pub oidc_redirect_uri: String,
    /// Migration DSN (`DATABASE_URL_MIGRATION`, required). Schema-owner
    /// credentials for the ephemeral migration pool. Bypasses RLS.
    pub migration_database_url: String,
    /// Ingestion-pipeline DSN (`DATABASE_URL_INGESTION`); falls back to
    /// `database_url` when unset. Connections run as
    /// `reverie_ingestion` against the `*_ingestion_full_access` RLS
    /// policies.
    pub ingestion_database_url: String,
    /// Ranked acceptable formats (`REVERIE_FORMAT_PRIORITY`,
    /// comma-separated; default `epub,pdf,mobi,azw3,cbz,cbr`). The
    /// ingestion pipeline picks the highest-ranked file when an
    /// incoming work has multiple candidates.
    pub format_priority: Vec<ManifestationFormat>,
    /// Post-ingestion cleanup behaviour (`REVERIE_CLEANUP_MODE`,
    /// default `all`). See [`CleanupMode`] for variant semantics.
    pub cleanup_mode: CleanupMode,
    /// Metadata enrichment knobs (concurrency, cache TTLs, etc.).
    pub enrichment: EnrichmentConfig,
    /// Cover-image acquisition limits (max bytes, redirect cap, etc.).
    pub cover: CoverConfig,
    /// Writeback worker knobs (concurrency, retry cap).
    pub writeback: WritebackConfig,
    /// OPDS catalogue settings (mount enable, page size, realm,
    /// `public_url`).
    pub opds: OpdsConfig,
    /// Response-header policy (CSP, HSTS, reporting endpoint, dist
    /// path). `csp_*_header` fields are finalised by [`crate::run`]
    /// after construction.
    pub security: SecurityConfig,
    /// `OpenLibrary` API base URL (`REVERIE_OPENLIBRARY_BASE_URL`,
    /// default `https://openlibrary.org`).
    pub openlibrary_base_url: String,
    /// Google Books API base URL (`REVERIE_GOOGLEBOOKS_BASE_URL`,
    /// default `https://www.googleapis.com/books/v1`).
    pub googlebooks_base_url: String,
    /// Optional Google Books API key
    /// (`REVERIE_GOOGLEBOOKS_API_KEY`); when set, requests bypass the
    /// public anonymous quota.
    pub googlebooks_api_key: Option<String>,
    /// Hardcover GraphQL endpoint (`REVERIE_HARDCOVER_BASE_URL`,
    /// default `https://api.hardcover.app/v1/graphql`).
    pub hardcover_base_url: String,
    /// Optional Hardcover bearer token
    /// (`REVERIE_HARDCOVER_API_TOKEN`); requests are skipped when
    /// unset.
    pub hardcover_api_token: Option<String>,
    /// Operator contact (`REVERIE_OPERATOR_CONTACT`); embedded into
    /// the outbound `User-Agent` to claim `OpenLibrary`'s identified
    /// 3 req/s rate-limit tier (vs. 1 req/s anonymous).
    pub operator_contact: Option<String>,
}

/// OPDS catalog configuration. When `enabled`, `/opds/*` is mounted behind a
/// Basic-only extractor and `public_url` must be set — feeds emit absolute URLs
/// rooted at `public_url`.
///
/// Note: the dual-mounted cover handlers at `/api/books/:id/cover{,/thumb}` are
/// mounted independently of `enabled` because the web UI (Step 10) needs them
/// regardless of OPDS availability.
#[derive(Debug, Clone)]
pub struct OpdsConfig {
    /// Whether the `/opds/*` routes are mounted
    /// (`REVERIE_OPDS_ENABLED`, default `true`).
    pub enabled: bool,
    /// Default page size for paginated feeds (`REVERIE_OPDS_PAGE_SIZE`,
    /// default `50`; valid range 1-500).
    pub page_size: u32,
    /// `WWW-Authenticate: Basic realm=...` value emitted on 401
    /// responses from `/opds/*` (`REVERIE_OPDS_REALM`, default
    /// `"Reverie OPDS"`). Validated to exclude `"` to keep the header
    /// well-formed.
    pub realm: String,
    /// Externally-visible base URL the catalogue emits absolute links
    /// rooted at (`REVERIE_PUBLIC_URL`). Required when `enabled=true`;
    /// optional otherwise.
    pub public_url: Option<url::Url>,
}

/// Metadata-enrichment subsystem knobs (background workers that fetch
/// from `OpenLibrary` / Google Books / Hardcover).
#[derive(Debug, Clone)]
pub struct EnrichmentConfig {
    /// Whether the enrichment queue is spawned
    /// (`REVERIE_ENRICHMENT_ENABLED`, default `true`).
    pub enabled: bool,
    /// In-flight enrichment job concurrency
    /// (`REVERIE_ENRICHMENT_CONCURRENCY`, default `2`; valid range 1-10).
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

/// Cover-image acquisition limits applied by the cover service when
/// fetching from third-party metadata providers.
#[derive(Debug, Clone)]
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

/// Writeback-worker knobs (the background task that flushes pending
/// canonical-metadata mutations into the source manifestation files).
#[derive(Debug, Clone)]
pub struct WritebackConfig {
    /// Whether the writeback worker is spawned
    /// (`REVERIE_WRITEBACK_ENABLED`, default `true`).
    pub enabled: bool,
    /// In-flight writeback job concurrency
    /// (`REVERIE_WRITEBACK_CONCURRENCY`, default `2`; valid range 1-10).
    pub concurrency: u32,
    /// Sleep between empty-queue polls
    /// (`REVERIE_WRITEBACK_POLL_IDLE_SECS`, default `5`).
    pub poll_idle_secs: u64,
    /// Maximum retry attempts before a writeback job is considered
    /// exhausted (`REVERIE_WRITEBACK_MAX_ATTEMPTS`, default `10`).
    pub max_attempts: u32,
}

/// Response-header policy.
///
/// CSP values are stored as precomputed `HeaderValue`s. They depend on
/// `validate_frontend_dist` reading the on-disk FOUC script to derive its
/// hash, so `csp_api_header` and `csp_html_header` are left as `None` after
/// `from_env()` and finalised by `main()` via
/// [`crate::security::csp::build_api_csp`] /
/// [`crate::security::csp::build_html_csp`]. A non-conformant CSP string
/// panics in `main()` rather than silently dropping the header at request
/// time.
///
/// HSTS and Reporting-Endpoints are derived from the booleans / URL stored
/// here via [`Self::hsts_header_value`] and
/// [`Self::reporting_endpoints_header_value`]. Both compose static-ASCII
/// strings from validated inputs and panic on the impossible case (a
/// programming invariant has been violated and we want to know).
///
/// A `SecurityConfig` obtained directly from `from_env()` — without the
/// CSP-finalisation pass — emits no `Content-Security-Policy` on either
/// route class (both fields stay `None`); HSTS and Reporting-Endpoints
/// are still applied because they are derived on demand.
#[derive(Debug, Clone)]
pub struct SecurityConfig {
    /// Whether the deployment is fronted by a TLS-terminating reverse
    /// proxy (`REVERIE_BEHIND_HTTPS`, default `false`). Gates HSTS
    /// emission — never emitted on plaintext HTTP because the browser
    /// would refuse the next TLS-less request to this host.
    pub behind_https: bool,
    /// Whether the HSTS header carries `; includeSubDomains`
    /// (`REVERIE_HSTS_INCLUDE_SUBDOMAINS`, default `false`). Requires
    /// `behind_https=true`.
    pub hsts_include_subdomains: bool,
    /// Whether the HSTS header carries `; preload`
    /// (`REVERIE_HSTS_PRELOAD`, default `false`). Requires
    /// `hsts_include_subdomains=true` (chrome.com / hstspreload.org
    /// rules).
    pub hsts_preload: bool,
    /// Optional CSP-violation reporting endpoint
    /// (`REVERIE_CSP_REPORT_ENDPOINT`). Pre-validated at startup to
    /// reject `"`/`;`/CR/LF (header-injection guard) and any scheme
    /// other than `http`/`https`.
    pub csp_report_endpoint: Option<url::Url>,
    /// Optional path to the built frontend dist directory
    /// (`REVERIE_FRONTEND_DIST_PATH`). When set, the SPA assets router
    /// is mounted and the FOUC-script hash is read at startup to seed
    /// the HTML CSP.
    pub frontend_dist_path: Option<std::path::PathBuf>,
    /// Precomputed `Content-Security-Policy` header for HTML
    /// responses. `None` after [`Self::from_env`]; finalised by
    /// [`crate::run`] from the FOUC-script hash + reporting endpoint.
    pub csp_html_header: Option<axum::http::HeaderValue>,
    /// Precomputed `Content-Security-Policy` header for API
    /// responses. `None` after [`Self::from_env`]; finalised by
    /// [`crate::run`] from the reporting endpoint
    /// (`default-src 'none'`-rooted, no script-src hashes).
    pub csp_api_header: Option<axum::http::HeaderValue>,
}

/// Post-ingestion cleanup behaviour selector for the watcher's
/// "after a successful batch" hook.
///
/// Wire format (JSON, DB `text` column): lowercase string —
/// `"all"` | `"ingested"` | `"none"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CleanupMode {
    /// Delete all files in the ingestion directory after a successful batch.
    All,
    /// Delete only files that were actually ingested (selected by format priority).
    Ingested,
    /// Never delete source files — user handles cleanup manually.
    None,
}

impl CleanupMode {
    /// Lowercase wire string matching the `#[serde(rename_all)]` mapping.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Ingested => "ingested",
            Self::None => "none",
        }
    }
}

impl std::fmt::Display for CleanupMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Configuration-load failure mode. Surfaces missing required vars and
/// parse/validation failures with the offending var name attached so
/// operator error messages are actionable.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// A required environment variable was unset. Carries the variable
    /// name verbatim for surfacing to operators.
    #[error("missing required environment variable: {0}")]
    MissingVar(String),
    /// A variable was set but parse/validation rejected the value.
    /// `var` names the variable; `reason` describes why the value was
    /// rejected (out of range, malformed URL, unsupported enum, etc.).
    #[error("invalid value for {var}: {reason}")]
    Invalid {
        /// Name of the offending environment variable.
        var: String,
        /// Why the supplied value was rejected.
        reason: String,
    },
}

/// Process-env adapter for `Config::from_env`. Extracted from a closure so it
/// is callable from a test (no `unsafe { env::set_var }` needed — tests read
/// vars cargo always sets, like `CARGO_PKG_NAME`).
fn process_env_get(k: &str) -> Option<String> {
    env::var(k).ok()
}

impl Config {
    /// Public entry point for production: loads `.env` (best-effort) then
    /// reads from process env via `std::env::var`.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::MissingVar`] when a required variable is
    /// unset (`DATABASE_URL`, `OIDC_*`); returns [`ConfigError::Invalid`]
    /// when an optional variable is set but fails parse or validation
    /// (out-of-range numerics, unsupported `format_priority` entries,
    /// malformed URLs, header-injection-prone characters in
    /// `REVERIE_CSP_REPORT_ENDPOINT`, etc.). The variant carries the
    /// offending variable name so the surfaced operator-facing message
    /// is actionable.
    pub fn from_env() -> Result<Self, ConfigError> {
        dotenvy::dotenv().ok();
        Self::from_source(&process_env_get)
    }

    /// Inject env via a closure. Tests pass a `HashMap`-backed `&EnvGet` so
    /// they never mutate process env (UNK-100). Production calls this via
    /// `from_env` with the `std::env::var` adapter.
    ///
    /// # Errors
    ///
    /// Same surface as [`Self::from_env`] minus the dotenv side-effect:
    /// [`ConfigError::MissingVar`] for missing required vars,
    /// [`ConfigError::Invalid`] for values that fail parse or
    /// validation.
    #[allow(
        clippy::too_many_lines,
        reason = "Config::from_source threads ~15 independent env vars with error propagation; extracting would produce boilerplate without improving readability"
    )]
    pub fn from_source(get: &EnvGet<'_>) -> Result<Self, ConfigError> {
        let database_url =
            get("DATABASE_URL").ok_or_else(|| ConfigError::MissingVar("DATABASE_URL".into()))?;

        let port = get("REVERIE_PORT")
            .unwrap_or_else(|| "3000".into())
            .parse::<u16>()
            .map_err(|e| ConfigError::Invalid {
                var: "REVERIE_PORT".into(),
                reason: e.to_string(),
            })?;

        let oidc_issuer_url = get("OIDC_ISSUER_URL")
            .ok_or_else(|| ConfigError::MissingVar("OIDC_ISSUER_URL".into()))?;
        let oidc_client_id = get("OIDC_CLIENT_ID")
            .ok_or_else(|| ConfigError::MissingVar("OIDC_CLIENT_ID".into()))?;
        let oidc_client_secret = get("OIDC_CLIENT_SECRET")
            .ok_or_else(|| ConfigError::MissingVar("OIDC_CLIENT_SECRET".into()))?;
        let oidc_redirect_uri = get("OIDC_REDIRECT_URI")
            .ok_or_else(|| ConfigError::MissingVar("OIDC_REDIRECT_URI".into()))?;

        let migration_database_url = get("DATABASE_URL_MIGRATION")
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| ConfigError::MissingVar("DATABASE_URL_MIGRATION".into()))?;

        let ingestion_database_url =
            get("DATABASE_URL_INGESTION").unwrap_or_else(|| database_url.clone());

        let format_priority: Vec<ManifestationFormat> = get("REVERIE_FORMAT_PRIORITY")
            .unwrap_or_else(|| "epub,pdf,mobi,azw3,cbz,cbr".into())
            .split(',')
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty())
            .map(|s| {
                s.parse::<ManifestationFormat>()
                    .map_err(|_| ConfigError::Invalid {
                        var: "REVERIE_FORMAT_PRIORITY".into(),
                        reason: format!(
                            "unsupported format '{s}'. Supported: epub, pdf, mobi, azw3, cbz, cbr"
                        ),
                    })
            })
            .collect::<Result<_, _>>()?;

        let cleanup_mode = match get("REVERIE_CLEANUP_MODE")
            .unwrap_or_else(|| "all".into())
            .to_lowercase()
            .as_str()
        {
            "all" => CleanupMode::All,
            "ingested" => CleanupMode::Ingested,
            "none" => CleanupMode::None,
            other => {
                return Err(ConfigError::Invalid {
                    var: "REVERIE_CLEANUP_MODE".into(),
                    reason: format!("expected 'all', 'ingested', or 'none', got '{other}'"),
                });
            }
        };

        let enrichment = EnrichmentConfig::from_source(get)?;
        let cover = CoverConfig::from_source(get)?;
        let writeback = WritebackConfig::from_source(get)?;
        let opds = OpdsConfig::from_source(get)?;
        let security = SecurityConfig::from_source(get)?;

        let openlibrary_base_url =
            get("REVERIE_OPENLIBRARY_BASE_URL").unwrap_or_else(|| "https://openlibrary.org".into());
        let googlebooks_base_url = get("REVERIE_GOOGLEBOOKS_BASE_URL")
            .unwrap_or_else(|| "https://www.googleapis.com/books/v1".into());
        let googlebooks_api_key = get("REVERIE_GOOGLEBOOKS_API_KEY").filter(|s| !s.is_empty());
        let hardcover_base_url = get("REVERIE_HARDCOVER_BASE_URL")
            .unwrap_or_else(|| "https://api.hardcover.app/v1/graphql".into());
        let hardcover_api_token = get("REVERIE_HARDCOVER_API_TOKEN").filter(|s| !s.is_empty());
        let operator_contact = get("REVERIE_OPERATOR_CONTACT").filter(|s| !s.is_empty());

        Ok(Self {
            port,
            database_url,
            library_path: get("REVERIE_LIBRARY_PATH").unwrap_or_else(|| "./library".into()),
            ingestion_path: get("REVERIE_INGESTION_PATH").unwrap_or_else(|| "./ingestion".into()),
            quarantine_path: get("REVERIE_QUARANTINE_PATH")
                .unwrap_or_else(|| "./quarantine".into()),
            // Operator namespace (REVERIE_LOG_LEVEL) wins over Rust ecosystem
            // default (RUST_LOG) so staging docs that advertise the REVERIE_*
            // prefix are honoured, while dev workflows keyed on RUST_LOG keep
            // working via fallback. See backend/CLAUDE.md "Operator env-var
            // namespacing" for the wider pattern.
            log_level: get("REVERIE_LOG_LEVEL")
                .or_else(|| get("RUST_LOG"))
                .unwrap_or_else(|| "info".into()),
            db_max_connections: get("REVERIE_DB_MAX_CONNECTIONS")
                .unwrap_or_else(|| "10".into())
                .parse::<u32>()
                .map_err(|e| ConfigError::Invalid {
                    var: "REVERIE_DB_MAX_CONNECTIONS".into(),
                    reason: e.to_string(),
                })?,
            oidc_issuer_url,
            oidc_client_id,
            oidc_client_secret,
            oidc_redirect_uri,
            migration_database_url,
            ingestion_database_url,
            format_priority,
            cleanup_mode,
            enrichment,
            cover,
            writeback,
            opds,
            security,
            openlibrary_base_url,
            googlebooks_base_url,
            googlebooks_api_key,
            hardcover_base_url,
            hardcover_api_token,
            operator_contact,
        })
    }

    /// `User-Agent` string for outbound metadata API requests.  `OpenLibrary`
    /// grants identified requests a 3 req/s rate-limit tier (vs. 1 req/s
    /// anonymous) when a contact email or URL is present in the UA.
    pub fn user_agent(&self) -> String {
        self.operator_contact.as_deref().map_or_else(
            || format!("Reverie/{} (unidentified)", env!("CARGO_PKG_VERSION")),
            |contact| format!("Reverie/{} ({contact})", env!("CARGO_PKG_VERSION")),
        )
    }
}

impl EnrichmentConfig {
    fn from_source(get: &EnvGet<'_>) -> Result<Self, ConfigError> {
        let enabled = parse_bool(get, "REVERIE_ENRICHMENT_ENABLED", true)?;
        let concurrency = parse_u32(get, "REVERIE_ENRICHMENT_CONCURRENCY", 2)?;
        if !(1..=10).contains(&concurrency) {
            return Err(ConfigError::Invalid {
                var: "REVERIE_ENRICHMENT_CONCURRENCY".into(),
                reason: format!("must be 1-10, got {concurrency}"),
            });
        }
        let poll_idle_secs = parse_u64(get, "REVERIE_ENRICHMENT_POLL_IDLE_SECS", 30)?;
        let fetch_budget_secs = parse_u64(get, "REVERIE_ENRICHMENT_FETCH_BUDGET_SECS", 15)?;
        let http_timeout_secs = parse_u64(get, "REVERIE_ENRICHMENT_HTTP_TIMEOUT_SECS", 10)?;
        let max_attempts = parse_u32(get, "REVERIE_ENRICHMENT_MAX_ATTEMPTS", 10)?;
        let cache_ttl_hit_days = parse_u32(get, "REVERIE_ENRICHMENT_CACHE_TTL_HIT_DAYS", 30)?;
        let cache_ttl_miss_days = parse_u32(get, "REVERIE_ENRICHMENT_CACHE_TTL_MISS_DAYS", 7)?;
        let cache_ttl_error_mins = parse_u32(get, "REVERIE_ENRICHMENT_CACHE_TTL_ERROR_MINS", 15)?;

        Ok(Self {
            enabled,
            concurrency,
            poll_idle_secs,
            fetch_budget_secs,
            http_timeout_secs,
            max_attempts,
            cache_ttl_hit_days,
            cache_ttl_miss_days,
            cache_ttl_error_mins,
        })
    }
}

impl WritebackConfig {
    fn from_source(get: &EnvGet<'_>) -> Result<Self, ConfigError> {
        let enabled = parse_bool(get, "REVERIE_WRITEBACK_ENABLED", true)?;
        let concurrency = parse_u32(get, "REVERIE_WRITEBACK_CONCURRENCY", 2)?;
        if !(1..=10).contains(&concurrency) {
            return Err(ConfigError::Invalid {
                var: "REVERIE_WRITEBACK_CONCURRENCY".into(),
                reason: format!("must be 1-10, got {concurrency}"),
            });
        }
        let poll_idle_secs = parse_u64(get, "REVERIE_WRITEBACK_POLL_IDLE_SECS", 5)?;
        let max_attempts = parse_u32(get, "REVERIE_WRITEBACK_MAX_ATTEMPTS", 10)?;
        Ok(Self {
            enabled,
            concurrency,
            poll_idle_secs,
            max_attempts,
        })
    }
}

impl CoverConfig {
    fn from_source(get: &EnvGet<'_>) -> Result<Self, ConfigError> {
        let max_bytes = parse_u64(get, "REVERIE_COVER_MAX_BYTES", 10_485_760)?;
        let download_timeout_secs = parse_u64(get, "REVERIE_COVER_DOWNLOAD_TIMEOUT_SECS", 30)?;
        let min_long_edge_px = parse_u32(get, "REVERIE_COVER_MIN_LONG_EDGE_PX", 1000)?;
        let redirect_limit = parse_u32(get, "REVERIE_COVER_REDIRECT_LIMIT", 3)? as usize;

        Ok(Self {
            max_bytes,
            download_timeout_secs,
            min_long_edge_px,
            redirect_limit,
        })
    }
}

impl OpdsConfig {
    fn from_source(get: &EnvGet<'_>) -> Result<Self, ConfigError> {
        let enabled = parse_bool(get, "REVERIE_OPDS_ENABLED", true)?;
        let page_size = parse_u32(get, "REVERIE_OPDS_PAGE_SIZE", 50)?;
        if !(1..=500).contains(&page_size) {
            return Err(ConfigError::Invalid {
                var: "REVERIE_OPDS_PAGE_SIZE".into(),
                reason: format!("must be 1-500, got {page_size}"),
            });
        }
        let realm = get("REVERIE_OPDS_REALM").unwrap_or_else(|| "Reverie OPDS".into());
        if realm.contains('"') {
            return Err(ConfigError::Invalid {
                var: "REVERIE_OPDS_REALM".into(),
                reason: "must not contain '\"'".into(),
            });
        }
        let public_url = match get("REVERIE_PUBLIC_URL").filter(|s| !s.is_empty()) {
            Some(s) => Some(url::Url::parse(&s).map_err(|e| ConfigError::Invalid {
                var: "REVERIE_PUBLIC_URL".into(),
                reason: e.to_string(),
            })?),
            None => None,
        };
        if enabled && public_url.is_none() {
            return Err(ConfigError::Invalid {
                var: "REVERIE_PUBLIC_URL".into(),
                reason: "required when REVERIE_OPDS_ENABLED=true".into(),
            });
        }
        Ok(Self {
            enabled,
            page_size,
            realm,
            public_url,
        })
    }
}

impl SecurityConfig {
    /// Production entry point: read security-related env vars from the
    /// process environment.
    ///
    /// The returned value is a partial — `csp_html_header` and
    /// `csp_api_header` are `None` until [`crate::run`] precomputes them
    /// from the FOUC-script hash and the configured report endpoint.
    /// Embedders that bypass `run` must perform that finalisation
    /// themselves (see [`crate::security::csp`]).
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::Invalid`] when a security-related variable
    /// fails validation: HSTS preconditions
    /// (`REVERIE_HSTS_INCLUDE_SUBDOMAINS` requires `behind_https`;
    /// `REVERIE_HSTS_PRELOAD` requires `include_subdomains`), the
    /// CSP-reporting URL header-injection guard (`"`/`;`/CR/LF rejected),
    /// or unsupported scheme on the reporting URL.
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_source(&process_env_get)
    }

    fn from_source(get: &EnvGet<'_>) -> Result<Self, ConfigError> {
        let behind_https = parse_bool(get, "REVERIE_BEHIND_HTTPS", false)?;
        let hsts_include_subdomains = parse_bool(get, "REVERIE_HSTS_INCLUDE_SUBDOMAINS", false)?;
        let hsts_preload = parse_bool(get, "REVERIE_HSTS_PRELOAD", false)?;

        if hsts_include_subdomains && !behind_https {
            return Err(ConfigError::Invalid {
                var: "REVERIE_HSTS_INCLUDE_SUBDOMAINS".into(),
                reason: "requires REVERIE_BEHIND_HTTPS=true".into(),
            });
        }
        if hsts_preload && !hsts_include_subdomains {
            return Err(ConfigError::Invalid {
                var: "REVERIE_HSTS_PRELOAD".into(),
                reason: "requires REVERIE_HSTS_INCLUDE_SUBDOMAINS=true".into(),
            });
        }

        let csp_report_endpoint = match get("REVERIE_CSP_REPORT_ENDPOINT").filter(|s| !s.is_empty())
        {
            Some(s) => {
                // Header-injection guard: this URL flows into a response header
                // (Reporting-Endpoints / report-to / report-uri). Reject quote
                // and CR/LF/semicolon which would split or escape values.
                if s.chars().any(|c| matches!(c, '"' | ';' | '\r' | '\n')) {
                    return Err(ConfigError::Invalid {
                        var: "REVERIE_CSP_REPORT_ENDPOINT".into(),
                        reason: "must not contain \" ; CR or LF".into(),
                    });
                }
                let parsed = url::Url::parse(&s).map_err(|e| ConfigError::Invalid {
                    var: "REVERIE_CSP_REPORT_ENDPOINT".into(),
                    reason: e.to_string(),
                })?;
                if !matches!(parsed.scheme(), "http" | "https") {
                    return Err(ConfigError::Invalid {
                        var: "REVERIE_CSP_REPORT_ENDPOINT".into(),
                        reason: format!("scheme must be http or https, got '{}'", parsed.scheme()),
                    });
                }
                Some(parsed)
            }
            None => None,
        };

        let frontend_dist_path = get("REVERIE_FRONTEND_DIST_PATH")
            .filter(|s| !s.is_empty())
            .map(std::path::PathBuf::from);

        Ok(Self {
            behind_https,
            hsts_include_subdomains,
            hsts_preload,
            csp_report_endpoint,
            frontend_dist_path,
            csp_html_header: None,
            csp_api_header: None,
        })
    }

    /// HSTS response-header value. `None` when `behind_https=false` — the
    /// middleware must not emit HSTS on plaintext HTTP or the browser would
    /// refuse to talk to the host on its next TLS-less request. The composed
    /// string is static ASCII; `from_str` panics on the impossible case so
    /// any future composition bug surfaces loudly instead of silently
    /// dropping the header.
    pub fn hsts_header_value(&self) -> Option<axum::http::HeaderValue> {
        if !self.behind_https {
            return None;
        }
        let mut v = String::from("max-age=31536000");
        if self.hsts_include_subdomains {
            v.push_str("; includeSubDomains");
        }
        if self.hsts_preload {
            v.push_str("; preload");
        }
        Some(axum::http::HeaderValue::from_str(&v).unwrap_or_else(|e| {
            panic!("HSTS string is not a valid HTTP header value ({e}): {v:?}")
        }))
    }

    /// `Reporting-Endpoints: csp-endpoint="<url>"`. `None` when
    /// `csp_report_endpoint` is unset. The URL was validated by
    /// [`Self::from_env`] (no `"` `;` CR or LF; valid `url::Url`); `as_str()`
    /// returns the canonical percent-encoded form. `from_str` panics on the
    /// impossible case rather than silently dropping the header.
    pub fn reporting_endpoints_header_value(&self) -> Option<axum::http::HeaderValue> {
        let url = self.csp_report_endpoint.as_ref()?;
        let v = format!("csp-endpoint=\"{}\"", url.as_str());
        Some(axum::http::HeaderValue::from_str(&v).unwrap_or_else(|e| {
            panic!("Reporting-Endpoints value is not a valid HTTP header value ({e}): {v:?}")
        }))
    }
}

fn parse_bool(get: &EnvGet<'_>, var: &str, default: bool) -> Result<bool, ConfigError> {
    // Strict: accept only lowercase "true"/"false" (exact match). The previous
    // lenient form accepted "1"/"0"/"yes"/"no" with case-insensitivity; it was
    // tightened in UNK-106 so operator-facing values have a single canonical
    // form. Pre-MVP: no operators to migrate.
    get(var).map_or(Ok(default), |v| match v.as_str() {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(ConfigError::Invalid {
            var: var.into(),
            reason: format!("expected 'true' or 'false', got '{v}'"),
        }),
    })
}

fn parse_u32(get: &EnvGet<'_>, var: &str, default: u32) -> Result<u32, ConfigError> {
    get(var).map_or(Ok(default), |v| {
        v.parse::<u32>().map_err(|e| ConfigError::Invalid {
            var: var.into(),
            reason: e.to_string(),
        })
    })
}

fn parse_u64(get: &EnvGet<'_>, var: &str, default: u64) -> Result<u64, ConfigError> {
    get(var).map_or(Ok(default), |v| {
        v.parse::<u64>().map_err(|e| ConfigError::Invalid {
            var: var.into(),
            reason: e.to_string(),
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// Build an `EnvGet` closure backed by an in-memory map. Tests inject
    /// env via this rather than mutating process env (UNK-100 — eliminates
    /// the `sqlx::test` race that `ENV_LOCK` + `unsafe { env::set_var }` was
    /// working around).
    fn env_for(vars: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> + use<> {
        let map: HashMap<String, String> = vars
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect();
        move |k| map.get(k).cloned()
    }

    const BASE_VARS: &[(&str, &str)] = &[
        ("DATABASE_URL", "postgres://test@localhost/reverie_dev"),
        (
            "DATABASE_URL_MIGRATION",
            "postgres://test@localhost/reverie_dev",
        ),
        ("OIDC_ISSUER_URL", "https://auth.example.com"),
        ("OIDC_CLIENT_ID", "test"),
        ("OIDC_CLIENT_SECRET", "secret"),
        ("OIDC_REDIRECT_URI", "http://localhost:3000/auth/callback"),
        // OPDS: default enabled=true requires PUBLIC_URL. Tests that don't
        // care about OPDS disable it here.
        ("REVERIE_OPDS_ENABLED", "false"),
    ];

    fn with_overrides(extra: &[(&str, &str)]) -> Vec<(String, String)> {
        let mut out: Vec<(String, String)> = BASE_VARS
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect();
        for (k, v) in extra {
            if let Some(slot) = out.iter_mut().find(|(kk, _)| kk == k) {
                slot.1 = (*v).to_string();
            } else {
                out.push(((*k).to_string(), (*v).to_string()));
            }
        }
        out
    }

    fn without_keys(keys: &[&str]) -> Vec<(String, String)> {
        BASE_VARS
            .iter()
            .filter(|(k, _)| !keys.contains(k))
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect()
    }

    /// `env_for` variant taking owned-string slices — for callers that build
    /// the var list via `with_overrides` / `without_keys`.
    fn env_for_owned(vars: &[(String, String)]) -> impl Fn(&str) -> Option<String> + use<'_> {
        let map: HashMap<&str, &str> = vars.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        move |k| map.get(k).map(|s| (*s).to_string())
    }

    /// Every `KEY=` line in `docker/staging.env.runtime.example` must name a
    /// variable that `Config::from_source` actually reads via `get("...")`.
    ///
    /// Guards against the operator-facing failure class in UNK-250: an example
    /// var whose name diverges from the code either hard-fails startup with a
    /// misleading `MissingVar` (loud) or is silently ignored while a fallback
    /// takes over (silent — e.g. ingestion DSN falling back to the app role,
    /// collapsing the documented role-separation threat model). The example
    /// file is an intentional *subset* of all knobs, so the check is one-way:
    /// example keys ⊆ names read by `from_source`, not the reverse.
    #[test]
    fn staging_runtime_example_keys_are_read_by_config() {
        // Compile-time embed: a missing file fails the build rather than
        // silently skipping the guard.
        let config_src = include_str!("config.rs");
        let example = include_str!("../../docker/staging.env.runtime.example");

        // Direct reads are `get("KEY")`; the subsystem configs read through
        // `parse_bool(get, "KEY", default)` / `parse_u32` / `parse_u64`, whose
        // call sites spell `get, "KEY"`. Scan for both so a helper-read key in
        // the example file is not mistaken for a divergence.
        let mut read_names: std::collections::HashSet<&str> = std::collections::HashSet::new();
        for needle in ["get(\"", "get, \""] {
            for (i, m) in config_src.match_indices(needle) {
                let rest = &config_src[i + m.len()..];
                if let Some(end) = rest.find('"') {
                    read_names.insert(&rest[..end]);
                }
            }
        }

        let mut violations: Vec<&str> = example
            .lines()
            .map(str::trim)
            .filter(|l| !l.starts_with('#') && !l.is_empty())
            .filter_map(|l| l.split_once('='))
            .map(|(k, _)| k.trim())
            .filter(|k| !read_names.contains(*k))
            .collect();
        violations.sort_unstable();

        assert!(
            violations.is_empty(),
            "staging.env.runtime.example contains keys not read by config.rs: {violations:?}. \
             Rename them to match the `get(\"...\")` call in config.rs, or drop them."
        );
    }

    #[test]
    fn from_env_with_defaults() {
        let config = Config::from_source(&env_for(BASE_VARS)).unwrap();
        assert_eq!(config.port, 3000);
        assert_eq!(config.database_url, "postgres://test@localhost/reverie_dev");
        assert_eq!(config.library_path, "./library");
        assert_eq!(config.ingestion_path, "./ingestion");
        assert_eq!(config.quarantine_path, "./quarantine");
        assert_eq!(
            config.migration_database_url,
            "postgres://test@localhost/reverie_dev"
        );
        // Falls back to DATABASE_URL when DATABASE_URL_INGESTION is unset
        assert_eq!(
            config.ingestion_database_url,
            "postgres://test@localhost/reverie_dev"
        );
        assert_eq!(
            config.format_priority,
            vec![
                ManifestationFormat::Epub,
                ManifestationFormat::Pdf,
                ManifestationFormat::Mobi,
                ManifestationFormat::Azw3,
                ManifestationFormat::Cbz,
                ManifestationFormat::Cbr,
            ]
        );
        assert_eq!(config.cleanup_mode, CleanupMode::All);
        // Enrichment defaults
        assert!(config.enrichment.enabled);
        assert_eq!(config.enrichment.concurrency, 2);
        assert_eq!(config.enrichment.max_attempts, 10);
        assert_eq!(config.cover.max_bytes, 10_485_760);
        assert_eq!(config.cover.min_long_edge_px, 1000);
        assert_eq!(config.cover.redirect_limit, 3);
        // Writeback defaults
        assert!(config.writeback.enabled);
        assert_eq!(config.writeback.concurrency, 2);
        assert_eq!(config.writeback.poll_idle_secs, 5);
        assert_eq!(config.writeback.max_attempts, 10);
        assert_eq!(config.openlibrary_base_url, "https://openlibrary.org");
        assert!(config.googlebooks_api_key.is_none());
        assert!(config.hardcover_api_token.is_none());
        assert!(config.operator_contact.is_none());
    }

    #[test]
    fn user_agent_without_contact_reports_unidentified() {
        let config = Config::from_source(&env_for(BASE_VARS)).unwrap();
        let ua = config.user_agent();
        assert!(ua.starts_with("Reverie/"), "missing Reverie/ prefix: {ua}");
        assert!(ua.ends_with("(unidentified)"), "unexpected suffix: {ua}");
    }

    #[test]
    fn user_agent_with_contact_embeds_identifier() {
        let vars = with_overrides(&[("REVERIE_OPERATOR_CONTACT", "ops@example.com")]);
        let config = Config::from_source(&env_for_owned(&vars)).unwrap();
        assert_eq!(config.operator_contact.as_deref(), Some("ops@example.com"));
        let ua = config.user_agent();
        assert!(ua.contains("(ops@example.com)"), "missing contact: {ua}");
        assert!(ua.starts_with("Reverie/"), "missing Reverie/ prefix: {ua}");
    }

    #[test]
    fn from_env_rejects_concurrency_out_of_range() {
        let vars = with_overrides(&[("REVERIE_ENRICHMENT_CONCURRENCY", "11")]);
        let err = Config::from_source(&env_for_owned(&vars)).unwrap_err();
        assert!(err.to_string().contains("REVERIE_ENRICHMENT_CONCURRENCY"));
    }

    #[test]
    fn from_env_all_vars() {
        let vars = with_overrides(&[
            ("DATABASE_URL", "postgres://custom@localhost/reverie_dev"),
            ("REVERIE_PORT", "8080"),
            ("REVERIE_LIBRARY_PATH", "/data/library"),
            ("REVERIE_INGESTION_PATH", "/data/ingestion"),
            ("REVERIE_QUARANTINE_PATH", "/data/quarantine"),
            ("RUST_LOG", "debug"),
        ]);
        let config = Config::from_source(&env_for_owned(&vars)).unwrap();
        assert_eq!(config.port, 8080);
        assert_eq!(
            config.database_url,
            "postgres://custom@localhost/reverie_dev"
        );
        assert_eq!(config.library_path, "/data/library");
        assert_eq!(config.log_level, "debug");
    }

    #[test]
    fn from_env_prefers_reverie_log_level_over_rust_log() {
        let vars = with_overrides(&[("REVERIE_LOG_LEVEL", "debug"), ("RUST_LOG", "trace")]);
        let config = Config::from_source(&env_for_owned(&vars)).unwrap();
        assert_eq!(
            config.log_level, "debug",
            "REVERIE_LOG_LEVEL should win when both env vars are set"
        );
    }

    #[test]
    fn from_env_uses_reverie_log_level_when_rust_log_unset() {
        let vars = with_overrides(&[("REVERIE_LOG_LEVEL", "warn")]);
        let config = Config::from_source(&env_for_owned(&vars)).unwrap();
        assert_eq!(config.log_level, "warn");
    }

    #[test]
    fn from_env_defaults_log_level_to_info_when_neither_var_set() {
        let config = Config::from_source(&env_for(BASE_VARS)).unwrap();
        assert_eq!(config.log_level, "info");
    }

    #[test]
    fn from_env_missing_database_url() {
        let vars = without_keys(&["DATABASE_URL"]);
        let err = Config::from_source(&env_for_owned(&vars)).unwrap_err();
        assert!(err.to_string().contains("DATABASE_URL"));
    }

    #[test]
    fn from_env_missing_migration_url() {
        let vars = without_keys(&["DATABASE_URL_MIGRATION"]);
        let err = Config::from_source(&env_for_owned(&vars)).unwrap_err();
        assert!(
            err.to_string().contains("DATABASE_URL_MIGRATION"),
            "expected var name in error: {err}"
        );
    }

    #[test]
    fn from_env_empty_migration_url_rejected() {
        let vars = with_overrides(&[("DATABASE_URL_MIGRATION", "")]);
        let err = Config::from_source(&env_for_owned(&vars)).unwrap_err();
        assert!(
            err.to_string().contains("DATABASE_URL_MIGRATION"),
            "expected var name in error: {err}"
        );
    }

    #[test]
    fn from_env_custom_migration_url() {
        let vars = with_overrides(&[(
            "DATABASE_URL_MIGRATION",
            "postgres://schema_owner@localhost/reverie_dev",
        )]);
        let config = Config::from_source(&env_for_owned(&vars)).unwrap();
        assert_eq!(
            config.migration_database_url,
            "postgres://schema_owner@localhost/reverie_dev"
        );
    }

    #[test]
    fn from_env_custom_ingestion_url_and_format_priority() {
        let vars = with_overrides(&[
            (
                "DATABASE_URL_INGESTION",
                "postgres://ingestion@localhost/reverie_dev",
            ),
            ("REVERIE_FORMAT_PRIORITY", "pdf, EPUB , mobi"),
        ]);
        let config = Config::from_source(&env_for_owned(&vars)).unwrap();
        assert_eq!(
            config.ingestion_database_url,
            "postgres://ingestion@localhost/reverie_dev"
        );
        assert_eq!(
            config.format_priority,
            vec![
                ManifestationFormat::Pdf,
                ManifestationFormat::Epub,
                ManifestationFormat::Mobi,
            ]
        );
    }

    #[test]
    fn from_env_rejects_unsupported_format_priority() {
        let vars = with_overrides(&[("REVERIE_FORMAT_PRIORITY", "epub,djvu")]);
        let err = Config::from_source(&env_for_owned(&vars)).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("djvu"), "expected djvu in error: {msg}");
        assert!(
            msg.contains("REVERIE_FORMAT_PRIORITY"),
            "expected var name in error: {msg}"
        );
    }

    #[test]
    fn opds_enabled_without_public_url_errors() {
        let vars = with_overrides(&[("REVERIE_OPDS_ENABLED", "true")]);
        let err = Config::from_source(&env_for_owned(&vars)).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("REVERIE_PUBLIC_URL"),
            "unexpected error: {msg}"
        );
    }

    #[test]
    fn opds_page_size_out_of_range_errors() {
        for bad in ["0", "501"] {
            let vars = with_overrides(&[("REVERIE_OPDS_PAGE_SIZE", bad)]);
            let err = Config::from_source(&env_for_owned(&vars)).unwrap_err();
            let msg = err.to_string();
            assert!(
                msg.contains("REVERIE_OPDS_PAGE_SIZE"),
                "page_size={bad} did not surface var name: {msg}"
            );
        }
    }

    #[test]
    fn opds_realm_with_double_quote_errors() {
        let vars = with_overrides(&[("REVERIE_OPDS_REALM", "bad\"quote")]);
        let err = Config::from_source(&env_for_owned(&vars)).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("REVERIE_OPDS_REALM"),
            "expected realm error: {msg}"
        );
    }

    #[test]
    fn opds_enabled_with_valid_public_url_parses() {
        let vars = with_overrides(&[
            ("REVERIE_OPDS_ENABLED", "true"),
            ("REVERIE_PUBLIC_URL", "https://reverie.example.com/"),
        ]);
        let config = Config::from_source(&env_for_owned(&vars)).unwrap();
        assert!(config.opds.enabled);
        assert_eq!(
            config.opds.public_url.as_ref().map(url::Url::as_str),
            Some("https://reverie.example.com/")
        );
    }

    #[test]
    fn security_defaults_all_off() {
        let cfg = SecurityConfig::from_source(&env_for(&[])).unwrap();
        assert!(!cfg.behind_https);
        assert!(!cfg.hsts_include_subdomains);
        assert!(!cfg.hsts_preload);
        assert!(cfg.csp_report_endpoint.is_none());
        assert!(cfg.frontend_dist_path.is_none());
    }

    #[test]
    fn security_hsts_subdomains_without_https_errors() {
        let err =
            SecurityConfig::from_source(&env_for(&[("REVERIE_HSTS_INCLUDE_SUBDOMAINS", "true")]))
                .unwrap_err();
        assert!(
            err.to_string().contains("REVERIE_HSTS_INCLUDE_SUBDOMAINS"),
            "unexpected: {err}"
        );
    }

    #[test]
    fn security_hsts_preload_without_subdomains_errors() {
        let err = SecurityConfig::from_source(&env_for(&[
            ("REVERIE_BEHIND_HTTPS", "true"),
            ("REVERIE_HSTS_PRELOAD", "true"),
        ]))
        .unwrap_err();
        assert!(
            err.to_string().contains("REVERIE_HSTS_PRELOAD"),
            "unexpected: {err}"
        );
    }

    #[test]
    fn security_hsts_full_stack_ok() {
        let cfg = SecurityConfig::from_source(&env_for(&[
            ("REVERIE_BEHIND_HTTPS", "true"),
            ("REVERIE_HSTS_INCLUDE_SUBDOMAINS", "true"),
            ("REVERIE_HSTS_PRELOAD", "true"),
        ]))
        .unwrap();
        assert!(cfg.behind_https);
        assert!(cfg.hsts_include_subdomains);
        assert!(cfg.hsts_preload);
        let v = cfg.hsts_header_value().unwrap();
        assert_eq!(
            v.to_str().unwrap(),
            "max-age=31536000; includeSubDomains; preload"
        );
    }

    #[test]
    fn security_hsts_header_absent_when_plaintext() {
        let cfg = SecurityConfig::from_source(&env_for(&[])).unwrap();
        assert!(cfg.hsts_header_value().is_none());
    }

    #[test]
    fn security_report_endpoint_bad_scheme_errors() {
        let err = SecurityConfig::from_source(&env_for(&[(
            "REVERIE_CSP_REPORT_ENDPOINT",
            "ftp://bad.example",
        )]))
        .unwrap_err();
        assert!(err.to_string().contains("scheme"), "unexpected: {err}");
    }

    #[test]
    fn security_report_endpoint_malformed_url_errors() {
        let err =
            SecurityConfig::from_source(&env_for(&[("REVERIE_CSP_REPORT_ENDPOINT", "not a url")]))
                .unwrap_err();
        assert!(
            err.to_string().contains("REVERIE_CSP_REPORT_ENDPOINT"),
            "unexpected: {err}"
        );
    }

    #[test]
    fn security_report_endpoint_injection_chars_errors() {
        for bad in [
            "https://ok.example/\";x=y",
            "https://ok.example/;evil",
            "https://ok.example/\r\nX-Injected: 1",
        ] {
            let err =
                SecurityConfig::from_source(&env_for(&[("REVERIE_CSP_REPORT_ENDPOINT", bad)]))
                    .unwrap_err();
            assert!(
                err.to_string().contains("must not contain"),
                "unexpected: {err}"
            );
        }
    }

    #[test]
    fn security_report_endpoint_happy_path() {
        let cfg = SecurityConfig::from_source(&env_for(&[(
            "REVERIE_CSP_REPORT_ENDPOINT",
            "https://log.example/csp",
        )]))
        .unwrap();
        let url = cfg.csp_report_endpoint.as_ref().unwrap();
        assert_eq!(url.as_str(), "https://log.example/csp");
        let hv = cfg.reporting_endpoints_header_value().unwrap();
        assert_eq!(
            hv.to_str().unwrap(),
            r#"csp-endpoint="https://log.example/csp""#
        );
    }

    #[test]
    fn security_parse_bool_rejects_legacy_truthy() {
        // UNK-110: strict form rejects the old "1"/"yes" spellings.
        let err =
            SecurityConfig::from_source(&env_for(&[("REVERIE_BEHIND_HTTPS", "yes")])).unwrap_err();
        assert!(err.to_string().contains("REVERIE_BEHIND_HTTPS"));
    }

    #[test]
    fn from_env_invalid_port() {
        let vars = with_overrides(&[("REVERIE_PORT", "not_a_number")]);
        let err = Config::from_source(&env_for_owned(&vars)).unwrap_err();
        assert!(err.to_string().contains("REVERIE_PORT"));
    }

    #[test]
    fn from_env_invalid_cleanup_mode() {
        let vars = with_overrides(&[("REVERIE_CLEANUP_MODE", "archive")]);
        let err = Config::from_source(&env_for_owned(&vars)).unwrap_err();
        assert!(
            err.to_string().contains("REVERIE_CLEANUP_MODE"),
            "unexpected: {err}"
        );
    }

    #[test]
    fn opds_page_size_boundary_values_accepted() {
        for boundary in ["1", "500"] {
            let vars = with_overrides(&[("REVERIE_OPDS_PAGE_SIZE", boundary)]);
            let cfg = Config::from_source(&env_for_owned(&vars))
                .unwrap_or_else(|e| panic!("page_size={boundary} should be accepted: {e}"));
            assert_eq!(cfg.opds.page_size, boundary.parse::<u32>().unwrap());
        }
    }

    #[test]
    fn from_env_rejects_zero_enrichment_concurrency() {
        let vars = with_overrides(&[("REVERIE_ENRICHMENT_CONCURRENCY", "0")]);
        let err = Config::from_source(&env_for_owned(&vars)).unwrap_err();
        assert!(err.to_string().contains("REVERIE_ENRICHMENT_CONCURRENCY"));
    }

    #[test]
    fn from_env_rejects_zero_writeback_concurrency() {
        let vars = with_overrides(&[("REVERIE_WRITEBACK_CONCURRENCY", "0")]);
        let err = Config::from_source(&env_for_owned(&vars)).unwrap_err();
        assert!(err.to_string().contains("REVERIE_WRITEBACK_CONCURRENCY"));
    }

    // Cover the production wiring `&process_env_get`. CARGO_PKG_NAME is set by
    // cargo for every test run; UNSET_REVERIE_TEST_VAR is reserved nowhere.
    #[test]
    fn process_env_get_reads_process_env_for_set_var() {
        let v = super::process_env_get("CARGO_PKG_NAME");
        assert_eq!(v.as_deref(), Some("reverie-api"));
    }

    #[test]
    fn process_env_get_returns_none_for_unset_var() {
        assert!(super::process_env_get("UNSET_REVERIE_TEST_VAR").is_none());
    }
}
