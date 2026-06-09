//! Environment-driven configuration loaded once at startup.
//!
//! Loading is a declarative figment pipeline (figment + serde + validator +
//! schemars; `adr/2026-06-09-declarative-config-stack.md`): the custom
//! [`EnvProvider`] maps env-var names to dotted struct paths, `serde`
//! deserializes into the config structs (per-field defaults from the
//! `Default` impls), [`Config::from_figment`] applies the post-deserialize
//! security gates, and `validator` runs range + cross-field checks.
//! [`Config::from_env`] is the production entry point; tests inject env as
//! in-memory pairs via [`EnvProvider::from_pairs`] so test setup never
//! mutates the process environment (UNK-100). Subsystem configs
//! ([`OpdsConfig`], [`EnrichmentConfig`], [`CoverConfig`],
//! [`WritebackConfig`], [`SecurityConfig`]) nest as owned sub-structs.
//!
//! [`SecurityConfig`] is a partial value after `from_env` — the
//! `csp_html_header` / `csp_api_header` fields stay `None` until
//! [`crate::run`] precomputes them from the FOUC-script hash and the
//! configured report endpoint. Responses emit no
//! `Content-Security-Policy` header while those fields remain `None`
//! (see the `if let Some(v)` guards in [`crate::security::headers`]),
//! so embedders bypassing `run` must perform the finalisation pass
//! themselves via [`crate::security::csp`].

use figment::{
    Figment, Metadata, Profile, Provider,
    value::{Dict, Map, Value},
};
use validator::{Validate, ValidationError, ValidationErrors, ValidationErrorsKind};

use crate::models::manifestation_format::ManifestationFormat;

/// Resolved process-wide configuration. Fields reflect the settled view of
/// the environment after defaults, parsing, and validation; subsystem
/// configs (OPDS, enrichment, cover, writeback, security) are nested as
/// owned values so callers do not pass the entire `Config` into helpers
/// that only need one slice.
#[derive(Debug, Clone, serde::Deserialize, schemars::JsonSchema, Validate)]
#[serde(default)]
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
    /// Migration DSN (`DATABASE_URL_MIGRATION`). `reverie_migrator`
    /// credentials for the ephemeral migration pool. `None` on the default
    /// server path — the application process holds no migration credential
    /// unless [`Self::auto_migrate`] is set. Required (else
    /// [`ConfigError::MissingVar`]) only when `auto_migrate` is true.
    pub migration_database_url: Option<String>,
    /// Run pending migrations in-process at startup
    /// (`REVERIE_AUTO_MIGRATE`, default `false`). The shipped default is
    /// out-of-band migration via `reverie migrate`; when this is `true` the
    /// long-lived server process carries the migration credential for its
    /// whole lifetime, so it is an opt-in escape hatch only. Requires
    /// [`Self::migration_database_url`] to be set.
    pub auto_migrate: bool,
    /// Ingestion-pipeline DSN (`DATABASE_URL_INGESTION`); falls back to
    /// `database_url` when unset. Connections run as
    /// `reverie_ingestion` against the `*_ingestion_full_access` RLS
    /// policies.
    pub ingestion_database_url: String,
    /// Ranked acceptable formats (`REVERIE_FORMAT_PRIORITY`,
    /// comma-separated; default `epub,pdf,mobi,azw3,cbz,cbr`). The
    /// ingestion pipeline picks the highest-ranked file when an
    /// incoming work has multiple candidates.
    #[serde(deserialize_with = "de_format_priority")]
    pub format_priority: Vec<ManifestationFormat>,
    /// Post-ingestion cleanup behaviour (`REVERIE_CLEANUP_MODE`,
    /// default `all`). See [`CleanupMode`] for variant semantics.
    pub cleanup_mode: CleanupMode,
    /// Metadata enrichment knobs (concurrency, cache TTLs, etc.).
    #[validate(nested)]
    pub enrichment: EnrichmentConfig,
    /// Cover-image acquisition limits (max bytes, redirect cap, etc.).
    #[validate(nested)]
    pub cover: CoverConfig,
    /// Writeback worker knobs (concurrency, retry cap).
    #[validate(nested)]
    pub writeback: WritebackConfig,
    /// OPDS catalogue settings (mount enable, page size, realm,
    /// `public_url`).
    #[validate(nested)]
    pub opds: OpdsConfig,
    /// Response-header policy (CSP, HSTS, reporting endpoint, dist
    /// path). `csp_*_header` fields are finalised by [`crate::run`]
    /// after construction.
    #[validate(nested)]
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
#[derive(Debug, Clone, serde::Deserialize, schemars::JsonSchema, Validate)]
#[serde(default)]
#[validate(schema(function = "validate_opds_config"))]
pub struct OpdsConfig {
    /// Whether the `/opds/*` routes are mounted
    /// (`REVERIE_OPDS_ENABLED`, default `true`).
    pub enabled: bool,
    /// Default page size for paginated feeds (`REVERIE_OPDS_PAGE_SIZE`,
    /// default `50`; valid range 1-500).
    #[validate(range(min = 1, max = 500, message = "must be between 1 and 500"))]
    pub page_size: u32,
    /// `WWW-Authenticate: Basic realm=...` value emitted on 401
    /// responses from `/opds/*` (`REVERIE_OPDS_REALM`, default
    /// `"Reverie OPDS"`). Validated to exclude `"` to keep the header
    /// well-formed.
    #[validate(custom(function = "validate_realm"))]
    pub realm: String,
    /// Externally-visible base URL the catalogue emits absolute links
    /// rooted at (`REVERIE_PUBLIC_URL`). Required when `enabled=true`;
    /// optional otherwise.
    pub public_url: Option<url::Url>,
}

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
    pub max_attempts: u32,
}

/// Response-header policy.
///
/// CSP values are stored as precomputed `HeaderValue`s. They depend on
/// `validate_frontend_dist` reading the on-disk FOUC script to derive its
/// hash, so `csp_api_header` and `csp_html_header` are left as `None` after
/// deserialization and finalised by `main()` via
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
/// A `SecurityConfig` obtained directly from the config pipeline — without the
/// CSP-finalisation pass — emits no `Content-Security-Policy` on either
/// route class (both fields stay `None`); HSTS and Reporting-Endpoints
/// are still applied because they are derived on demand.
#[derive(Debug, Clone, Default, serde::Deserialize, schemars::JsonSchema, Validate)]
#[serde(default)]
#[validate(schema(function = "validate_security_config"))]
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
    #[serde(default, deserialize_with = "de_csp_endpoint")]
    pub csp_report_endpoint: Option<url::Url>,
    /// Optional path to the built frontend dist directory
    /// (`REVERIE_FRONTEND_DIST_PATH`). When set, the SPA assets router
    /// is mounted and the FOUC-script hash is read at startup to seed
    /// the HTML CSP.
    pub frontend_dist_path: Option<std::path::PathBuf>,
    /// Precomputed `Content-Security-Policy` header for HTML
    /// responses. `None` after [`Config::from_env`]; finalised by
    /// [`crate::run`] from the FOUC-script hash + reporting endpoint.
    #[serde(skip)]
    #[schemars(skip)]
    pub csp_html_header: Option<axum::http::HeaderValue>,
    /// Precomputed `Content-Security-Policy` header for API
    /// responses. `None` after [`Config::from_env`]; finalised by
    /// [`crate::run`] from the reporting endpoint
    /// (`default-src 'none'`-rooted, no script-src hashes).
    #[serde(skip)]
    #[schemars(skip)]
    pub csp_api_header: Option<axum::http::HeaderValue>,
}

/// Post-ingestion cleanup behaviour selector for the watcher's
/// "after a successful batch" hook.
///
/// Wire format (JSON, DB `text` column): lowercase string —
/// `"all"` | `"ingested"` | `"none"`.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
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
    /// Two or more validation failures surfaced together. Only the
    /// declarative `validate()` phase aggregates; deserialize-phase
    /// (figment `extract`) errors remain fail-fast, one at a time.
    #[error("{} configuration error(s):\n{}", .0.len(), .0.iter().map(ToString::to_string).collect::<Vec<_>>().join("\n"))]
    Multiple(Vec<Self>),
}

impl Config {
    /// Public entry point for production: loads `.env` (best-effort) then
    /// reads from the process environment through the figment pipeline.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::MissingVar`] when a required variable is
    /// unset (`DATABASE_URL`, `OIDC_*`); returns [`ConfigError::Invalid`]
    /// when an optional variable is set but fails parse or validation
    /// (out-of-range numerics, unsupported `format_priority` entries,
    /// malformed URLs, header-injection-prone characters in
    /// `REVERIE_CSP_REPORT_ENDPOINT`, etc.); returns [`ConfigError::Multiple`]
    /// when more than one declarative validation fails together. The
    /// variant carries the offending variable name so the surfaced
    /// operator-facing message is actionable.
    pub fn from_env() -> Result<Self, ConfigError> {
        dotenvy::dotenv().ok();
        Self::from_figment(&Figment::from(EnvProvider::from_process_env()))
    }

    /// Load configuration from a prepared [`Figment`].
    ///
    /// The pipeline is: figment `extract` (typed deserialization, with
    /// per-field defaults supplied by the `#[serde(default)]` `Default`
    /// impls — no separate `Serialized::defaults` layer is needed, which
    /// also keeps secret-bearing fields out of any `Serialize` path) →
    /// post-deserialize security gates → declarative `validate()`.
    ///
    /// Post-deserialize gates, in order:
    ///
    /// 1. **Migration-credential gate** (security-load-bearing): figment
    ///    deserializes `migration_database_url` from `DATABASE_URL_MIGRATION`
    ///    unconditionally. When `auto_migrate` is off the field is forced
    ///    back to `None` so the long-lived server never carries the migrator
    ///    credential; when on, an absent/blank DSN is a `MissingVar`. See
    ///    `adr/2026-06-02-hybrid-migration-entrypoints-and-role.md`.
    /// 2. **Ingestion-DSN fallback**: a blank `ingestion_database_url` clones
    ///    `database_url` (role-scoped DSN, defaults to the app DSN).
    /// 3. **Required-field check**: a blank required field (`DATABASE_URL`,
    ///    `OIDC_*`) is a `MissingVar` — distinct from `Invalid` so the
    ///    operator message says "set the var", not "fix the value".
    ///
    /// # Errors
    ///
    /// [`ConfigError::MissingVar`] for unset required vars,
    /// [`ConfigError::Invalid`] for a single parse/validation failure, and
    /// [`ConfigError::Multiple`] for aggregated `validate()` failures.
    pub fn from_figment(figment: &Figment) -> Result<Self, ConfigError> {
        let mut cfg: Self = figment.extract().map_err(|e| map_figment_error(&e))?;

        // Gate 1 — migration credential (GOTCHA-MIGGATE). The blank check is
        // trimmed: `DATABASE_URL_MIGRATION="   "` must refuse start cleanly
        // rather than boot carrying a garbage credential.
        if cfg.auto_migrate {
            if cfg
                .migration_database_url
                .as_deref()
                .is_none_or(|s| s.trim().is_empty())
            {
                return Err(ConfigError::MissingVar("DATABASE_URL_MIGRATION".into()));
            }
        } else {
            cfg.migration_database_url = None;
        }

        // Gate 2 — ingestion DSN falls back to the app DSN when blank.
        if cfg.ingestion_database_url.trim().is_empty() {
            cfg.ingestion_database_url = cfg.database_url.clone();
        }

        // Gate 3 — required fields blank => MissingVar (NOT Invalid).
        for (value, var) in [
            (&cfg.database_url, "DATABASE_URL"),
            (&cfg.oidc_issuer_url, "OIDC_ISSUER_URL"),
            (&cfg.oidc_client_id, "OIDC_CLIENT_ID"),
            (&cfg.oidc_client_secret, "OIDC_CLIENT_SECRET"),
            (&cfg.oidc_redirect_uri, "OIDC_REDIRECT_URI"),
        ] {
            if value.trim().is_empty() {
                return Err(ConfigError::MissingVar(var.into()));
            }
        }

        // Declarative validation (range + cross-field). Aggregated.
        cfg.validate().map_err(|e| map_validation_errors(&e))?;

        Ok(cfg)
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

impl SecurityConfig {
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
    /// `csp_report_endpoint` is unset. The URL was validated at deserialize
    /// time by `de_csp_endpoint` (no `"` `;` CR or LF; valid `url::Url`); `as_str()`
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

// ---------------------------------------------------------------------------
// Declarative validators (validator 0.20). Range checks are field attributes;
// cross-field checks are struct-level `schema` functions. Cross-field errors
// carry the offending env-var name as a `"var"` param because the validator
// error tree keys struct-level failures under the opaque `"__all__"` key,
// which the reverse field→env-name map cannot resolve. Single-field checks
// (`validate_realm`) need no `"var"` param — their tree path reverse-maps
// natively (`opds.realm` → `REVERIE_OPDS_REALM`).
// ---------------------------------------------------------------------------

/// Reject a `"` in the OPDS `realm` — it flows into a `WWW-Authenticate: Basic
/// realm="…"` header and an embedded quote would split the value.
fn validate_realm(realm: &str) -> Result<(), ValidationError> {
    if realm.contains('"') {
        let mut e = ValidationError::new("realm_quote");
        e.message = Some("must not contain '\"'".into());
        return Err(e);
    }
    Ok(())
}

/// OPDS cross-field rule: when the catalogue is enabled it emits absolute URLs
/// rooted at `public_url`, so `public_url` is required.
fn validate_opds_config(opds: &OpdsConfig) -> Result<(), ValidationError> {
    if opds.enabled && opds.public_url.is_none() {
        let mut e = ValidationError::new("public_url_required");
        e.add_param("var".into(), &"REVERIE_PUBLIC_URL");
        e.message = Some("required when REVERIE_OPDS_ENABLED=true".into());
        return Err(e);
    }
    Ok(())
}

/// HSTS precondition ladder (chrome.com / hstspreload.org rules): subdomains
/// requires HTTPS; preload requires subdomains. Never emit HSTS on plaintext.
fn validate_security_config(sec: &SecurityConfig) -> Result<(), ValidationError> {
    if sec.hsts_include_subdomains && !sec.behind_https {
        let mut e = ValidationError::new("hsts_subdomains_requires_https");
        e.add_param("var".into(), &"REVERIE_HSTS_INCLUDE_SUBDOMAINS");
        e.message = Some("requires REVERIE_BEHIND_HTTPS=true".into());
        return Err(e);
    }
    if sec.hsts_preload && !sec.hsts_include_subdomains {
        let mut e = ValidationError::new("hsts_preload_requires_subdomains");
        e.add_param("var".into(), &"REVERIE_HSTS_PRELOAD");
        e.message = Some("requires REVERIE_HSTS_INCLUDE_SUBDOMAINS=true".into());
        return Err(e);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Error mapping: figment / validator errors → var-named `ConfigError`.
// ---------------------------------------------------------------------------

/// Dotted field paths whose value is secret material. A deserialize error on
/// one of these must never echo the offending value into a `ConfigError`
/// (hard rule 7). This is reachable, not theoretical: `EnvProvider` parses
/// `OIDC_CLIENT_SECRET=true` / `=123` into a `Value::Bool` / `Value::Num`,
/// which fails to deserialize into the `String` field with an `invalid type:
/// found bool true` message — figment echoes the value. The scrub replaces
/// that reason with a value-free one for any secret-bearing key path.
const SECRET_FIELDS: &[&str] = &[
    "oidc_client_secret",
    "googlebooks_api_key",
    "hardcover_api_token",
];

/// Reverse the [`ENV_MAP`]: dotted field path → operator-facing env-var name.
/// On the `log_level` collision (`REVERIE_LOG_LEVEL` and `RUST_LOG` both map
/// there) the `REVERIE_*` name is preferred; `log_level` never fails
/// deserialize/validation so the choice is academic.
fn env_name_for(dotted: &str) -> Option<&'static str> {
    ENV_MAP
        .iter()
        .filter(|(_, d)| *d == dotted)
        .map(|(name, _)| *name)
        .max_by_key(|name| usize::from(name.starts_with("REVERIE_")))
}

/// Map a figment `extract` error (deserialize phase, fail-fast) to a
/// var-named [`ConfigError::Invalid`]. The error's key path
/// (`["security", "csp_report_endpoint"]`) reverse-maps to the env-var name;
/// the message is taken up to figment's ` for key …` suffix. Secret-bearing
/// fields surface a value-free reason.
fn map_figment_error(e: &figment::Error) -> ConfigError {
    let dotted = e.path.join(".");
    let var = env_name_for(&dotted).map_or_else(|| dotted.clone(), ToString::to_string);
    if SECRET_FIELDS.contains(&dotted.as_str()) {
        return ConfigError::Invalid {
            var,
            reason: "invalid value (omitted — secret-bearing field)".into(),
        };
    }
    // figment's Display is "<message> for key \"<profile.path>\" in <source>";
    // keep only the message so the reason is clean and value-faithful.
    let full = e.to_string();
    let reason = full
        .split_once(" for key ")
        .map_or(full.as_str(), |(msg, _)| msg)
        .to_string();
    ConfigError::Invalid { var, reason }
}

/// Walk the nested `validate()` error tree into a flat list of var-named
/// [`ConfigError::Invalid`], then collapse to a single error or
/// [`ConfigError::Multiple`]. Field errors reverse-map by their tree path;
/// struct-level (`__all__`) errors carry the var name as a `"var"` param.
fn map_validation_errors(errs: &ValidationErrors) -> ConfigError {
    let mut out: Vec<ConfigError> = Vec::new();
    collect_validation_errors(errs, "", &mut out);
    if out.len() == 1 {
        // `swap_remove(0)` avoids cloning; the vec is dropped right after.
        out.swap_remove(0)
    } else {
        ConfigError::Multiple(out)
    }
}

/// Recursive helper for [`map_validation_errors`]. `prefix` is the dotted path
/// accumulated from enclosing structs.
fn collect_validation_errors(errs: &ValidationErrors, prefix: &str, out: &mut Vec<ConfigError>) {
    for (field, kind) in errs.errors() {
        match kind {
            ValidationErrorsKind::Field(field_errors) => {
                for fe in field_errors {
                    // Struct-level (`schema`) errors land under "__all__" and
                    // name their var explicitly; field errors reverse-map by
                    // the accumulated dotted path.
                    let var = fe
                        .params
                        .get("var")
                        .and_then(|v| v.as_str())
                        .map(ToString::to_string)
                        .or_else(|| {
                            let dotted = join_path(prefix, field);
                            env_name_for(&dotted).map(ToString::to_string)
                        })
                        .unwrap_or_else(|| join_path(prefix, field));
                    let reason = fe
                        .message
                        .as_ref()
                        .map_or_else(|| fe.code.to_string(), ToString::to_string);
                    out.push(ConfigError::Invalid { var, reason });
                }
            }
            ValidationErrorsKind::Struct(inner) => {
                collect_validation_errors(inner, &join_path(prefix, field), out);
            }
            ValidationErrorsKind::List(items) => {
                for (idx, inner) in items {
                    let path = format!("{}[{idx}]", join_path(prefix, field));
                    collect_validation_errors(inner, &path, out);
                }
            }
        }
    }
}

/// Join a dotted-path prefix with a child key, skipping the synthetic
/// `__all__` struct-level key (which is not a real field segment).
fn join_path(prefix: &str, field: &str) -> String {
    if field == "__all__" {
        return prefix.to_string();
    }
    if prefix.is_empty() {
        field.to_string()
    } else {
        format!("{prefix}.{field}")
    }
}

/// Deserialize the comma-separated `REVERIE_FORMAT_PRIORITY` surface
/// (`epub,pdf,mobi`) into the ranked `Vec<ManifestationFormat>`.
///
/// The env contract is bare CSV in a single variable — NOT figment array
/// syntax (`[a,b]`) — so the split lives here rather than relying on figment's
/// array parsing. Each token is trimmed, lowercased, and parsed via
/// [`ManifestationFormat`]'s `FromStr`; an unsupported token rejects the whole
/// value.
fn de_format_priority<'de, D>(de: D) -> Result<Vec<ManifestationFormat>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw = <String as serde::Deserialize>::deserialize(de)?;
    raw.split(',')
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
        .map(|s| {
            s.parse::<ManifestationFormat>().map_err(|_| {
                serde::de::Error::custom(format!(
                    "unsupported format '{s}'. Supported: epub, pdf, mobi, azw3, cbz, cbr"
                ))
            })
        })
        .collect()
}

/// Deserialize `REVERIE_CSP_REPORT_ENDPOINT` into `Option<url::Url>` with the
/// header-injection guard applied to the RAW string BEFORE `url::Url::parse`.
///
/// THREAT: this URL is emitted into the `Reporting-Endpoints` response header.
/// `url::Url::parse` percent-encodes (`"` → `%22`) and rejects CR/LF with a
/// generic message, so a guard placed on the parsed `Url` (or a
/// `#[validate(custom)]` on the typed value) would silently pass quote
/// injection and surface the wrong error for CR/LF. The raw-string char guard
/// therefore runs first, then the scheme allowlist — order mirrors the former
/// imperative path exactly. See `adr/2026-06-09-declarative-config-stack.md`
/// (GOTCHA-CSPRAW) and the `security_report_endpoint_injection_chars_errors`
/// test (3 forms).
fn de_csp_endpoint<'de, D>(de: D) -> Result<Option<url::Url>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = <String as serde::Deserialize>::deserialize(de)?;
    if s.is_empty() {
        return Ok(None);
    }
    // THREAT: reject quote / semicolon / CR / LF on the raw string — these
    // would split or escape the response header value (header injection).
    if s.chars().any(|c| matches!(c, '"' | ';' | '\r' | '\n')) {
        return Err(serde::de::Error::custom("must not contain \" ; CR or LF"));
    }
    let parsed = url::Url::parse(&s).map_err(serde::de::Error::custom)?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(serde::de::Error::custom(format!(
            "scheme must be http or https, got '{}'",
            parsed.scheme()
        )));
    }
    Ok(Some(parsed))
}

// ---------------------------------------------------------------------------
// Default impls — the single source for every optional field's default value,
// consumed by serde's container `#[serde(default)]` during figment extract.
// Required fields (database_url, oidc_*) default to empty and are caught by
// the post-extract required-check as `MissingVar` (GOTCHA-REQUIRED).
// ---------------------------------------------------------------------------

impl Default for Config {
    fn default() -> Self {
        Self {
            port: 3000,
            // REQUIRED — empty sentinel; reviewer handles MissingVar (GOTCHA-REQUIRED).
            database_url: String::new(),
            library_path: "./library".into(),
            ingestion_path: "./ingestion".into(),
            quarantine_path: "./quarantine".into(),
            log_level: "info".into(),
            db_max_connections: 10,
            // REQUIRED — empty sentinels.
            oidc_issuer_url: String::new(),
            oidc_client_id: String::new(),
            oidc_client_secret: String::new(),
            oidc_redirect_uri: String::new(),
            migration_database_url: None,
            auto_migrate: false,
            // Falls back to database_url at post-deserialize time (Task 6).
            ingestion_database_url: String::new(),
            format_priority: vec![
                ManifestationFormat::Epub,
                ManifestationFormat::Pdf,
                ManifestationFormat::Mobi,
                ManifestationFormat::Azw3,
                ManifestationFormat::Cbz,
                ManifestationFormat::Cbr,
            ],
            cleanup_mode: CleanupMode::All,
            enrichment: EnrichmentConfig::default(),
            cover: CoverConfig::default(),
            writeback: WritebackConfig::default(),
            opds: OpdsConfig::default(),
            security: SecurityConfig::default(),
            openlibrary_base_url: "https://openlibrary.org".into(),
            googlebooks_base_url: "https://www.googleapis.com/books/v1".into(),
            googlebooks_api_key: None,
            hardcover_base_url: "https://api.hardcover.app/v1/graphql".into(),
            hardcover_api_token: None,
            operator_contact: None,
        }
    }
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

impl Default for OpdsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            page_size: 50,
            realm: "Reverie OPDS".into(),
            public_url: None,
        }
    }
}

// ---------------------------------------------------------------------------
// EnvProvider — the custom figment::Provider keystone (env-var → field map,
// empty-as-unset filter, REVERIE_LOG_LEVEL > RUST_LOG cascade, value parse).
// ---------------------------------------------------------------------------

/// The env-var name → dotted-field-path map.
///
/// Convention:
///   flat top-level fields: `REVERIE_PORT` → `"port"`
///   sub-struct fields:     `REVERIE_ENRICHMENT_CONCURRENCY` → `"enrichment.concurrency"`
///
/// Non-`REVERIE_` vars (`DATABASE_URL`, `OIDC_*`) are included explicitly.
/// `REVERIE_LOG_LEVEL` and `RUST_LOG` both map to `"log_level"`; their
/// precedence cascade is resolved in [`EnvProvider::data`] (GOTCHA-CASCADE).
const ENV_MAP: &[(&str, &str)] = &[
    // --- top-level flat fields ---
    ("DATABASE_URL", "database_url"),
    ("DATABASE_URL_MIGRATION", "migration_database_url"),
    ("DATABASE_URL_INGESTION", "ingestion_database_url"),
    ("OIDC_ISSUER_URL", "oidc_issuer_url"),
    ("OIDC_CLIENT_ID", "oidc_client_id"),
    ("OIDC_CLIENT_SECRET", "oidc_client_secret"),
    ("OIDC_REDIRECT_URI", "oidc_redirect_uri"),
    ("REVERIE_PORT", "port"),
    ("REVERIE_LIBRARY_PATH", "library_path"),
    ("REVERIE_INGESTION_PATH", "ingestion_path"),
    ("REVERIE_QUARANTINE_PATH", "quarantine_path"),
    // Cascade resolved in `EnvProvider::data` (GOTCHA-CASCADE): both map to
    // `log_level`; `REVERIE_LOG_LEVEL` wins when both are set.
    ("REVERIE_LOG_LEVEL", "log_level"),
    ("RUST_LOG", "log_level"),
    ("REVERIE_DB_MAX_CONNECTIONS", "db_max_connections"),
    ("REVERIE_AUTO_MIGRATE", "auto_migrate"),
    ("REVERIE_FORMAT_PRIORITY", "format_priority"),
    ("REVERIE_CLEANUP_MODE", "cleanup_mode"),
    ("REVERIE_OPENLIBRARY_BASE_URL", "openlibrary_base_url"),
    ("REVERIE_GOOGLEBOOKS_BASE_URL", "googlebooks_base_url"),
    ("REVERIE_GOOGLEBOOKS_API_KEY", "googlebooks_api_key"),
    ("REVERIE_HARDCOVER_BASE_URL", "hardcover_base_url"),
    ("REVERIE_HARDCOVER_API_TOKEN", "hardcover_api_token"),
    ("REVERIE_OPERATOR_CONTACT", "operator_contact"),
    // --- enrichment sub-struct ---
    ("REVERIE_ENRICHMENT_ENABLED", "enrichment.enabled"),
    ("REVERIE_ENRICHMENT_CONCURRENCY", "enrichment.concurrency"),
    (
        "REVERIE_ENRICHMENT_POLL_IDLE_SECS",
        "enrichment.poll_idle_secs",
    ),
    (
        "REVERIE_ENRICHMENT_FETCH_BUDGET_SECS",
        "enrichment.fetch_budget_secs",
    ),
    (
        "REVERIE_ENRICHMENT_HTTP_TIMEOUT_SECS",
        "enrichment.http_timeout_secs",
    ),
    ("REVERIE_ENRICHMENT_MAX_ATTEMPTS", "enrichment.max_attempts"),
    (
        "REVERIE_ENRICHMENT_CACHE_TTL_HIT_DAYS",
        "enrichment.cache_ttl_hit_days",
    ),
    (
        "REVERIE_ENRICHMENT_CACHE_TTL_MISS_DAYS",
        "enrichment.cache_ttl_miss_days",
    ),
    (
        "REVERIE_ENRICHMENT_CACHE_TTL_ERROR_MINS",
        "enrichment.cache_ttl_error_mins",
    ),
    // --- cover sub-struct ---
    ("REVERIE_COVER_MAX_BYTES", "cover.max_bytes"),
    (
        "REVERIE_COVER_DOWNLOAD_TIMEOUT_SECS",
        "cover.download_timeout_secs",
    ),
    ("REVERIE_COVER_MIN_LONG_EDGE_PX", "cover.min_long_edge_px"),
    ("REVERIE_COVER_REDIRECT_LIMIT", "cover.redirect_limit"),
    // --- writeback sub-struct ---
    ("REVERIE_WRITEBACK_ENABLED", "writeback.enabled"),
    ("REVERIE_WRITEBACK_CONCURRENCY", "writeback.concurrency"),
    (
        "REVERIE_WRITEBACK_POLL_IDLE_SECS",
        "writeback.poll_idle_secs",
    ),
    ("REVERIE_WRITEBACK_MAX_ATTEMPTS", "writeback.max_attempts"),
    // --- opds sub-struct ---
    ("REVERIE_OPDS_ENABLED", "opds.enabled"),
    ("REVERIE_OPDS_PAGE_SIZE", "opds.page_size"),
    ("REVERIE_OPDS_REALM", "opds.realm"),
    ("REVERIE_PUBLIC_URL", "opds.public_url"),
    // --- security sub-struct ---
    ("REVERIE_BEHIND_HTTPS", "security.behind_https"),
    (
        "REVERIE_HSTS_INCLUDE_SUBDOMAINS",
        "security.hsts_include_subdomains",
    ),
    ("REVERIE_HSTS_PRELOAD", "security.hsts_preload"),
    (
        "REVERIE_CSP_REPORT_ENDPOINT",
        "security.csp_report_endpoint",
    ),
    ("REVERIE_FRONTEND_DIST_PATH", "security.frontend_dist_path"),
];

/// Custom [`figment::Provider`] feeding the config pipeline from environment
/// variables. Maps each known env-var name to its dotted field path via
/// `ENV_MAP`, parses values into typed figment `Value`s, and drops empties
/// (empty-as-unset). Unmapped vars (`PATH`, `HOME`, …) are ignored.
///
/// # Why a custom provider rather than stock [`figment::providers::Env`]
///
/// Two reasons, in order of load-bearing-ness:
///
/// 1. **A race-free, parallel-safe test seam.** [`Self::from_pairs`] injects
///    env as in-memory string pairs, so the config-parsing tests run
///    concurrently (each `sqlx::test` owns its DB) without mutating process
///    env. Stock `Env` reads only [`std::env::vars`]; testing it means
///    `Jail`/`temp-env`/`set_var`, all of which mutate global env under a lock
///    — serializing those tests and racing the suite's other env readers
///    (`dotenvy`, [`Self::from_process_env`]), the `getenv`/`setenv` data race
///    that makes `set_var` `unsafe`. `from_pairs` touches no process env.
///    Production
///    ([`Self::from_process_env`]) runs through the same code so tests exercise
///    the real parse path (UNK-100).
/// 2. **A frozen, irregular var→field contract.** The operator surface mixes
///    bare ecosystem names (`DATABASE_URL`, `OIDC_*`, `RUST_LOG`) with
///    `REVERIE_`-namespaced knobs, and several map to a nested path the var
///    name doesn't spell (`REVERIE_PUBLIC_URL` → `opds.public_url`). No
///    uniform separator rule derives that, so `ENV_MAP` is the explicit
///    registry — which also doubles as the introspectable var↔field source the
///    config-reference generator consumes (UNK-370).
///
/// Value parsing mirrors stock `Env` exactly (see [`Self::data`]); the custom
/// surface is only the two facts above. The pipeline is built in
/// [`Config::from_figment`].
///
/// GOTCHA-SPLIT (secondary): the explicit map also sidesteps
/// `Env::split("_")`, which would wrongly split `snake_case` flat fields
/// (`db_max_connections` → `db.max.connections`).
pub struct EnvProvider {
    pairs: Vec<(String, String)>,
}

impl EnvProvider {
    /// Collect all current process environment variables.
    pub fn from_process_env() -> Self {
        Self {
            pairs: std::env::vars().collect(),
        }
    }

    /// Build from an explicit slice of `(key, value)` string pairs.
    /// Used in tests as an in-memory seam (no process-env mutation, no
    /// `figment::Jail` — parallel-safe, GOTCHA-TESTSEAM).
    pub fn from_pairs(pairs: &[(&str, &str)]) -> Self {
        Self {
            pairs: pairs
                .iter()
                .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
                .collect(),
        }
    }
}

impl Provider for EnvProvider {
    fn metadata(&self) -> Metadata {
        Metadata::named("EnvProvider")
    }

    fn data(&self) -> Result<Map<Profile, Dict>, figment::Error> {
        // Build a lookup map from ENV_MAP for O(1) access.
        let lookup: std::collections::HashMap<&str, &str> = ENV_MAP.iter().copied().collect();

        let mut dict = Dict::new();

        for (key, val) in &self.pairs {
            // Empty string == unset (GOTCHA-EMPTY).
            if val.is_empty() {
                continue;
            }
            // Only process keys we know about; ignore PATH, HOME, etc.
            let Some(&dotted) = lookup.get(key.as_str()) else {
                continue;
            };
            // Log cascade (GOTCHA-CASCADE): `REVERIE_LOG_LEVEL` > `RUST_LOG` >
            // `"info"` (the `Default`). Both vars map to `log_level` in
            // ENV_MAP, so skip `RUST_LOG` when the operator-namespace var is
            // present — otherwise `pairs` ordering would decide the winner.
            if key == "RUST_LOG"
                && self
                    .pairs
                    .iter()
                    .any(|(k, v)| k == "REVERIE_LOG_LEVEL" && !v.is_empty())
            {
                continue;
            }
            // Parse the raw string into a typed figment `Value` (numeric →
            // `Num`, `true`/`false` → `Bool`, else `Str`) exactly as
            // `figment::providers::Env` does internally (env.rs: `v.parse()`).
            // `Value::from(String)` would force `Value::Str` for everything,
            // which the deserializer then refuses to coerce into `u16`/`bool`
            // fields (`InvalidType(Str, "u16")`). The parse keeps the strict
            // bool contract intact: only lowercase `true`/`false` become `Bool`;
            // `1`/`yes`/`True` parse to `Num`/`Str` and are rejected by a `bool`
            // field. `Value`'s `FromStr` error is `Infallible`.
            let leaf = val
                .parse::<Value>()
                .unwrap_or_else(|never: std::convert::Infallible| match never {});
            let nested = figment::util::nest(dotted, leaf);
            // Merge nested into our accumulating dict.
            // `nested` is a Value::Dict; extract its inner map and extend.
            if let figment::value::Value::Dict(_, inner) = nested {
                merge_dict(&mut dict, inner);
            }
        }

        let mut map = Map::new();
        map.insert(Profile::Default, dict);
        Ok(map)
    }
}

/// Recursively merge `src` into `dst`, with `src` winning on conflict.
fn merge_dict(dst: &mut Dict, src: Dict) {
    for (k, v) in src {
        // Check if dst already has this key as a Dict so we can recurse.
        // We use a separate `contains_key` check to avoid holding multiple
        // mutable borrows simultaneously (borrow checker limitation with
        // match on get_mut + entry in the same arm).
        let existing_is_dict = matches!(dst.get(&k), Some(figment::value::Value::Dict(_, _)));
        if existing_is_dict {
            if let figment::value::Value::Dict(_, src_inner) = v {
                if let Some(figment::value::Value::Dict(_, dst_inner)) = dst.get_mut(&k) {
                    merge_dict(dst_inner, src_inner);
                }
            } else {
                dst.insert(k, v);
            }
        } else {
            dst.insert(k, v);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a `Config` through the figment pipeline from in-memory env pairs —
    /// the process-env-free, parallel-safe test seam (UNK-100, GOTCHA-TESTSEAM).
    /// Strings flow through `EnvProvider`'s parse/coerce path, exercising the
    /// real production deserialization (GOTCHA-TESTFIDELITY): no pre-typed
    /// `Serialized(struct)` shortcut that would bypass where the bugs live.
    fn cfg_from(vars: &[(&str, &str)]) -> Result<Config, ConfigError> {
        Config::from_figment(&Figment::from(EnvProvider::from_pairs(vars)))
    }

    /// `cfg_from` variant for owned-string var lists built via
    /// `with_overrides` / `without_keys`.
    fn cfg_from_owned(vars: &[(String, String)]) -> Result<Config, ConfigError> {
        let refs: Vec<(&str, &str)> = vars.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        cfg_from(&refs)
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

    /// Every `KEY=` line in `docker/staging.env.runtime.example` must be a
    /// variable the loader actually reads — now expressed as a key in the
    /// declarative [`ENV_MAP`] (the structured replacement for the former
    /// textual `get("KEY")` source scan).
    ///
    /// Guards the operator-facing failure class in UNK-250: an example var
    /// whose name diverges from the loader either hard-fails startup with a
    /// misleading `MissingVar` (loud) or is silently ignored while a fallback
    /// takes over (silent — e.g. ingestion DSN falling back to the app role,
    /// collapsing the documented role-separation threat model). The example
    /// file is an intentional *subset* of all knobs, so the check is one-way:
    /// example keys ⊆ [`ENV_MAP`] keys, not the reverse.
    #[test]
    fn staging_runtime_example_keys_are_in_env_map() {
        // Compile-time embed: a missing file fails the build rather than
        // silently skipping the guard.
        let example = include_str!("../../docker/staging.env.runtime.example");

        let map_keys: std::collections::HashSet<&str> =
            ENV_MAP.iter().map(|(name, _)| *name).collect();

        let mut violations: Vec<&str> = example
            .lines()
            .map(str::trim)
            .filter(|l| !l.starts_with('#') && !l.is_empty())
            .filter_map(|l| l.split_once('='))
            .map(|(k, _)| k.trim())
            .filter(|k| !map_keys.contains(*k))
            .collect();
        violations.sort_unstable();

        assert!(
            violations.is_empty(),
            "staging.env.runtime.example contains keys absent from ENV_MAP: {violations:?}. \
             Add them to ENV_MAP (with their dotted field path), or drop them from the example."
        );
    }

    #[test]
    fn from_env_with_defaults() {
        let config = cfg_from(BASE_VARS).unwrap();
        assert_eq!(config.port, 3000);
        assert_eq!(config.database_url, "postgres://test@localhost/reverie_dev");
        assert_eq!(config.library_path, "./library");
        assert_eq!(config.ingestion_path, "./ingestion");
        assert_eq!(config.quarantine_path, "./quarantine");
        // BASE_VARS exports DATABASE_URL_MIGRATION but leaves REVERIE_AUTO_MIGRATE
        // unset (off), so the DSN is intentionally NOT carried into Config.
        assert_eq!(config.migration_database_url, None);
        assert!(!config.auto_migrate);
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
        let config = cfg_from(BASE_VARS).unwrap();
        let ua = config.user_agent();
        assert!(ua.starts_with("Reverie/"), "missing Reverie/ prefix: {ua}");
        assert!(ua.ends_with("(unidentified)"), "unexpected suffix: {ua}");
    }

    #[test]
    fn user_agent_with_contact_embeds_identifier() {
        let vars = with_overrides(&[("REVERIE_OPERATOR_CONTACT", "ops@example.com")]);
        let config = cfg_from_owned(&vars).unwrap();
        assert_eq!(config.operator_contact.as_deref(), Some("ops@example.com"));
        let ua = config.user_agent();
        assert!(ua.contains("(ops@example.com)"), "missing contact: {ua}");
        assert!(ua.starts_with("Reverie/"), "missing Reverie/ prefix: {ua}");
    }

    #[test]
    fn from_env_rejects_concurrency_out_of_range() {
        let vars = with_overrides(&[("REVERIE_ENRICHMENT_CONCURRENCY", "11")]);
        let err = cfg_from_owned(&vars).unwrap_err();
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
        let config = cfg_from_owned(&vars).unwrap();
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
        let config = cfg_from_owned(&vars).unwrap();
        assert_eq!(
            config.log_level, "debug",
            "REVERIE_LOG_LEVEL should win when both env vars are set"
        );
    }

    #[test]
    fn from_env_uses_reverie_log_level_when_rust_log_unset() {
        let vars = with_overrides(&[("REVERIE_LOG_LEVEL", "warn")]);
        let config = cfg_from_owned(&vars).unwrap();
        assert_eq!(config.log_level, "warn");
    }

    #[test]
    fn from_env_defaults_log_level_to_info_when_neither_var_set() {
        let config = cfg_from(BASE_VARS).unwrap();
        assert_eq!(config.log_level, "info");
    }

    #[test]
    fn from_env_missing_database_url() {
        let vars = without_keys(&["DATABASE_URL"]);
        let err = cfg_from_owned(&vars).unwrap_err();
        assert!(err.to_string().contains("DATABASE_URL"));
    }

    #[test]
    fn from_env_missing_migration_url_ok_when_auto_migrate_off() {
        // New contract: with REVERIE_AUTO_MIGRATE off (default), the migration
        // DSN is not required — the default server path only verifies the
        // schema via the app pool and never holds a migration credential.
        let vars = without_keys(&["DATABASE_URL_MIGRATION"]);
        let config = cfg_from_owned(&vars).unwrap();
        assert_eq!(config.migration_database_url, None);
        assert!(!config.auto_migrate);
    }

    #[test]
    fn from_env_empty_migration_url_treated_as_none_when_auto_migrate_off() {
        // An exported-empty DSN is indistinguishable from unset for the
        // default path: both yield None, no error.
        let vars = with_overrides(&[("DATABASE_URL_MIGRATION", "")]);
        let config = cfg_from_owned(&vars).unwrap();
        assert_eq!(config.migration_database_url, None);
    }

    #[test]
    fn from_env_auto_migrate_true_requires_migration_url() {
        let vars = with_overrides(&[("REVERIE_AUTO_MIGRATE", "true")]);
        let vars = vars
            .into_iter()
            .filter(|(k, _)| k != "DATABASE_URL_MIGRATION")
            .collect::<Vec<_>>();
        let err = cfg_from_owned(&vars).unwrap_err();
        assert!(
            err.to_string().contains("DATABASE_URL_MIGRATION"),
            "expected var name in error: {err}"
        );
    }

    #[test]
    fn from_env_auto_migrate_true_with_url_ok() {
        let vars = with_overrides(&[
            ("REVERIE_AUTO_MIGRATE", "true"),
            (
                "DATABASE_URL_MIGRATION",
                "postgres://reverie_migrator@localhost/reverie_dev",
            ),
        ]);
        let config = cfg_from_owned(&vars).unwrap();
        assert!(config.auto_migrate);
        assert_eq!(
            config.migration_database_url.as_deref(),
            Some("postgres://reverie_migrator@localhost/reverie_dev")
        );
    }

    #[test]
    fn from_env_auto_migrate_invalid_value_rejected() {
        let vars = with_overrides(&[("REVERIE_AUTO_MIGRATE", "yes")]);
        let err = cfg_from_owned(&vars).unwrap_err();
        assert!(
            err.to_string().contains("REVERIE_AUTO_MIGRATE"),
            "expected var name in error: {err}"
        );
    }

    #[test]
    fn from_env_custom_migration_url() {
        // The DSN is only stored when auto-migrate is on; set the flag so the
        // custom value is retained (off would yield None regardless).
        let vars = with_overrides(&[
            ("REVERIE_AUTO_MIGRATE", "true"),
            (
                "DATABASE_URL_MIGRATION",
                "postgres://schema_owner@localhost/reverie_dev",
            ),
        ]);
        let config = cfg_from_owned(&vars).unwrap();
        assert_eq!(
            config.migration_database_url.as_deref(),
            Some("postgres://schema_owner@localhost/reverie_dev")
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
        let config = cfg_from_owned(&vars).unwrap();
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
        let err = cfg_from_owned(&vars).unwrap_err();
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
        let err = cfg_from_owned(&vars).unwrap_err();
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
            let err = cfg_from_owned(&vars).unwrap_err();
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
        let err = cfg_from_owned(&vars).unwrap_err();
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
        let config = cfg_from_owned(&vars).unwrap();
        assert!(config.opds.enabled);
        assert_eq!(
            config.opds.public_url.as_ref().map(url::Url::as_str),
            Some("https://reverie.example.com/")
        );
    }

    /// Build just the `SecurityConfig` slice through the full pipeline.
    /// `BASE_VARS` satisfy the unrelated required fields (OPDS disabled there)
    /// so only the security knobs under test drive the outcome.
    fn security_from(extra: &[(&str, &str)]) -> Result<SecurityConfig, ConfigError> {
        cfg_from_owned(&with_overrides(extra)).map(|c| c.security)
    }

    #[test]
    fn security_defaults_all_off() {
        let cfg = security_from(&[]).unwrap();
        assert!(!cfg.behind_https);
        assert!(!cfg.hsts_include_subdomains);
        assert!(!cfg.hsts_preload);
        assert!(cfg.csp_report_endpoint.is_none());
        assert!(cfg.frontend_dist_path.is_none());
    }

    #[test]
    fn security_hsts_subdomains_without_https_errors() {
        let err = security_from(&[("REVERIE_HSTS_INCLUDE_SUBDOMAINS", "true")]).unwrap_err();
        assert!(
            err.to_string().contains("REVERIE_HSTS_INCLUDE_SUBDOMAINS"),
            "unexpected: {err}"
        );
    }

    #[test]
    fn security_hsts_preload_without_subdomains_errors() {
        let err = security_from(&[
            ("REVERIE_BEHIND_HTTPS", "true"),
            ("REVERIE_HSTS_PRELOAD", "true"),
        ])
        .unwrap_err();
        assert!(
            err.to_string().contains("REVERIE_HSTS_PRELOAD"),
            "unexpected: {err}"
        );
    }

    #[test]
    fn security_hsts_full_stack_ok() {
        let cfg = security_from(&[
            ("REVERIE_BEHIND_HTTPS", "true"),
            ("REVERIE_HSTS_INCLUDE_SUBDOMAINS", "true"),
            ("REVERIE_HSTS_PRELOAD", "true"),
        ])
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
        let cfg = security_from(&[]).unwrap();
        assert!(cfg.hsts_header_value().is_none());
    }

    #[test]
    fn security_report_endpoint_bad_scheme_errors() {
        let err =
            security_from(&[("REVERIE_CSP_REPORT_ENDPOINT", "ftp://bad.example")]).unwrap_err();
        assert!(err.to_string().contains("scheme"), "unexpected: {err}");
    }

    #[test]
    fn security_report_endpoint_malformed_url_errors() {
        let err = security_from(&[("REVERIE_CSP_REPORT_ENDPOINT", "not a url")]).unwrap_err();
        assert!(
            err.to_string().contains("REVERIE_CSP_REPORT_ENDPOINT"),
            "unexpected: {err}"
        );
    }

    #[test]
    fn security_report_endpoint_injection_chars_errors() {
        // The raw-string guard in `de_csp_endpoint` runs at deserialize time,
        // BEFORE `url::Url::parse` percent-encodes the quote — verifying the 3
        // injection forms route through the serde path, not removed code.
        for bad in [
            "https://ok.example/\";x=y",
            "https://ok.example/;evil",
            "https://ok.example/\r\nX-Injected: 1",
        ] {
            let err = security_from(&[("REVERIE_CSP_REPORT_ENDPOINT", bad)]).unwrap_err();
            assert!(
                err.to_string().contains("must not contain"),
                "unexpected: {err}"
            );
        }
    }

    #[test]
    fn security_report_endpoint_happy_path() {
        let cfg =
            security_from(&[("REVERIE_CSP_REPORT_ENDPOINT", "https://log.example/csp")]).unwrap();
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
        // UNK-110: strict form rejects the old "1"/"yes" spellings — native
        // now (EnvProvider parses "yes" to a `Str`, which a `bool` field
        // refuses), no custom bool deserializer (GOTCHA-BOOL, Task 9).
        let err = security_from(&[("REVERIE_BEHIND_HTTPS", "yes")]).unwrap_err();
        assert!(err.to_string().contains("REVERIE_BEHIND_HTTPS"));
    }

    // --- Tasks 6-9 gate tests (the new pipeline's security/correctness gates;
    //     the pre-existing substring asserts above do NOT distinguish the
    //     variants or the var-name source, so these are load-bearing). ---

    #[test]
    fn missing_database_url_is_missing_var_variant() {
        // GOTCHA-REQUIRED: unset required var must be the `MissingVar`
        // VARIANT (recovery: set the var), never `Invalid` (fix the value).
        let vars = without_keys(&["DATABASE_URL"]);
        let err = cfg_from_owned(&vars).unwrap_err();
        assert!(
            matches!(&err, ConfigError::MissingVar(v) if v == "DATABASE_URL"),
            "expected MissingVar(DATABASE_URL), got: {err:?}"
        );
    }

    #[test]
    fn nested_validate_error_names_env_var() {
        // GOTCHA-ERRNAME: a sub-struct range failure must surface the ENV VAR
        // (tree traversal + reverse map), not the Rust dotted field path.
        let vars = with_overrides(&[("REVERIE_ENRICHMENT_CONCURRENCY", "11")]);
        let err = cfg_from_owned(&vars).unwrap_err();
        let s = err.to_string();
        assert!(
            s.contains("REVERIE_ENRICHMENT_CONCURRENCY"),
            "expected env var name, got: {s}"
        );
        assert!(
            !s.contains("enrichment.concurrency"),
            "leaked dotted field path: {s}"
        );
    }

    #[test]
    fn multi_violation_yields_multiple() {
        // GOTCHA-AGG: two range violations aggregate into `Multiple`.
        let vars = with_overrides(&[
            ("REVERIE_ENRICHMENT_CONCURRENCY", "11"),
            ("REVERIE_WRITEBACK_CONCURRENCY", "0"),
        ]);
        let err = cfg_from_owned(&vars).unwrap_err();
        let ConfigError::Multiple(inner) = &err else {
            panic!("expected Multiple, got: {err:?}");
        };
        assert!(inner.len() >= 2, "expected >=2 errors, got {}", inner.len());
        let s = err.to_string();
        assert!(s.contains("REVERIE_ENRICHMENT_CONCURRENCY"), "{s}");
        assert!(s.contains("REVERIE_WRITEBACK_CONCURRENCY"), "{s}");
    }

    #[test]
    fn auto_migrate_blank_migration_url_is_missing_var() {
        // GOTCHA-MIGGATE (trim fidelity): a whitespace-only DSN must refuse
        // start cleanly, not boot carrying a garbage migration credential.
        let vars = with_overrides(&[
            ("REVERIE_AUTO_MIGRATE", "true"),
            ("DATABASE_URL_MIGRATION", "   "),
        ]);
        let err = cfg_from_owned(&vars).unwrap_err();
        assert!(
            matches!(&err, ConfigError::MissingVar(v) if v == "DATABASE_URL_MIGRATION"),
            "expected MissingVar(DATABASE_URL_MIGRATION), got: {err:?}"
        );
    }

    #[test]
    fn migration_url_nulled_when_auto_migrate_off() {
        // GOTCHA-MIGGATE (null-out): the long-lived server must not carry the
        // migrator credential when auto_migrate is off, even if the DSN is set.
        let vars = with_overrides(&[("DATABASE_URL_MIGRATION", "postgres://m@localhost/d")]);
        let cfg = cfg_from_owned(&vars).unwrap();
        assert_eq!(cfg.migration_database_url, None);
    }

    #[test]
    fn missing_oidc_secret_error_names_var_not_value() {
        // Hard rule 7: a missing secret surfaces only the var NAME via
        // MissingVar (a distinct code path from the deserialize-error scrub
        // below — here the secret was never set, so there is no value).
        let vars = without_keys(&["OIDC_CLIENT_SECRET"]);
        let err = cfg_from_owned(&vars).unwrap_err();
        assert!(
            matches!(&err, ConfigError::MissingVar(v) if v == "OIDC_CLIENT_SECRET"),
            "expected MissingVar(OIDC_CLIENT_SECRET), got: {err:?}"
        );
    }

    #[test]
    fn secret_field_deser_error_has_no_value() {
        // GOTCHA-SECRET-ERR (hard rule 7): a non-string-shaped secret value
        // (`true` parses to Value::Bool) fails String deserialization at
        // `extract()`, and figment's raw message would echo the value. The
        // SECRET_FIELDS scrub must replace the reason with a value-free one
        // while still naming the var.
        let vars = with_overrides(&[("OIDC_CLIENT_SECRET", "true")]);
        let err = cfg_from_owned(&vars).unwrap_err();
        let s = err.to_string();
        assert!(s.contains("OIDC_CLIENT_SECRET"), "must name the var: {s}");
        assert!(s.contains("omitted"), "expected scrubbed reason, got: {s}");
        assert!(!s.contains("true"), "secret value leaked into error: {s}");
    }

    #[test]
    fn env_provider_maps_flat_and_nested_key() {
        // GOTCHA-SPLIT: flat snake_case stays flat; only genuinely nested vars
        // nest. `db_max_connections` must NOT become `db.max.connections`.
        let p = EnvProvider::from_pairs(&[
            ("REVERIE_DB_MAX_CONNECTIONS", "20"),
            ("REVERIE_ENRICHMENT_CONCURRENCY", "3"),
        ]);
        let data = p.data().unwrap();
        let dict = data.get(&Profile::Default).unwrap();
        assert!(
            matches!(dict.get("db_max_connections"), Some(Value::Num(..))),
            "db_max_connections should be a flat numeric leaf"
        );
        assert!(
            dict.get("db").is_none(),
            "must not split into a `db` sub-dict"
        );
        let Some(Value::Dict(_, enr)) = dict.get("enrichment") else {
            panic!("enrichment should nest into a sub-dict");
        };
        assert!(enr.contains_key("concurrency"));
    }

    #[test]
    fn env_provider_drops_empty_as_unset() {
        // GOTCHA-EMPTY: an exported-empty var equals unset.
        let p = EnvProvider::from_pairs(&[("REVERIE_GOOGLEBOOKS_API_KEY", "")]);
        let data = p.data().unwrap();
        let dict = data.get(&Profile::Default).unwrap();
        assert!(dict.get("googlebooks_api_key").is_none());
    }

    #[test]
    fn config_schema_has_no_secret_default_values() {
        // Hard rule 7 / GOTCHA-SECRET: the emitted JSON Schema must carry no
        // secret VALUE. schemars renders each field's default, so secret-
        // bearing fields appear with `""` (required `String`) or `null`
        // (optional) — both non-values; a non-empty string default would be a
        // leak of real credential material.
        let schema = serde_json::to_value(schemars::schema_for!(Config)).unwrap();
        let props = schema["properties"].as_object().expect("properties object");
        for field in [
            "oidc_client_secret",
            "googlebooks_api_key",
            "hardcover_api_token",
        ] {
            let default = &props[field]["default"];
            let safe = default.is_null() || default.as_str() == Some("");
            assert!(
                safe,
                "secret field {field} carries a non-empty default in the schema: {default}"
            );
        }
        // Non-vacuity: a non-secret scalar still carries its real default, so
        // the assertion above is meaningful (the schema does emit defaults).
        assert_eq!(props["port"]["default"], serde_json::json!(3000));
    }

    #[test]
    fn env_provider_from_process_env_reads_real_env() {
        // CARGO_PKG_NAME is set by cargo for every test run; it is unmapped in
        // ENV_MAP (ignored by `data`) but must be collected into the raw pairs.
        let p = EnvProvider::from_process_env();
        assert!(p.pairs.iter().any(|(k, _)| k == "CARGO_PKG_NAME"));
    }

    #[test]
    fn from_env_invalid_port() {
        let vars = with_overrides(&[("REVERIE_PORT", "not_a_number")]);
        let err = cfg_from_owned(&vars).unwrap_err();
        assert!(err.to_string().contains("REVERIE_PORT"));
    }

    #[test]
    fn from_env_invalid_cleanup_mode() {
        let vars = with_overrides(&[("REVERIE_CLEANUP_MODE", "archive")]);
        let err = cfg_from_owned(&vars).unwrap_err();
        assert!(
            err.to_string().contains("REVERIE_CLEANUP_MODE"),
            "unexpected: {err}"
        );
    }

    #[test]
    fn opds_page_size_boundary_values_accepted() {
        for boundary in ["1", "500"] {
            let vars = with_overrides(&[("REVERIE_OPDS_PAGE_SIZE", boundary)]);
            let cfg = cfg_from_owned(&vars)
                .unwrap_or_else(|e| panic!("page_size={boundary} should be accepted: {e}"));
            assert_eq!(cfg.opds.page_size, boundary.parse::<u32>().unwrap());
        }
    }

    #[test]
    fn from_env_rejects_zero_enrichment_concurrency() {
        let vars = with_overrides(&[("REVERIE_ENRICHMENT_CONCURRENCY", "0")]);
        let err = cfg_from_owned(&vars).unwrap_err();
        assert!(err.to_string().contains("REVERIE_ENRICHMENT_CONCURRENCY"));
    }

    #[test]
    fn from_env_rejects_zero_writeback_concurrency() {
        let vars = with_overrides(&[("REVERIE_WRITEBACK_CONCURRENCY", "0")]);
        let err = cfg_from_owned(&vars).unwrap_err();
        assert!(err.to_string().contains("REVERIE_WRITEBACK_CONCURRENCY"));
    }
}
