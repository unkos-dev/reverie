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

mod cover;
mod enrichment;
mod opds;
mod provider;
mod reference;
mod security;
mod writeback;

pub use cover::CoverConfig;
pub use enrichment::EnrichmentConfig;
pub use opds::OpdsConfig;
pub use provider::EnvProvider;
pub use reference::reference_markdown;
pub use security::SecurityConfig;
pub use writeback::WritebackConfig;

use crate::models::manifestation_format::ManifestationFormat;
use figment::Figment;
use provider::ENV_MAP;
use validator::{Validate, ValidationErrors, ValidationErrorsKind};

/// Environment variables that must be present and non-blank for the server to
/// start (the Gate 3 check in [`Config::from_figment`]). Single source of truth
/// shared with the generated config reference ([`reference_markdown`]) so the
/// "Required" column can never drift from the startup contract — the schema's
/// own `required` array is empty because every config struct is
/// `#[serde(default)]`, so it cannot serve as that source.
///
/// `DATABASE_URL_MIGRATION` is deliberately absent: it is *conditionally*
/// required (only when `REVERIE_AUTO_MIGRATE=true`, enforced by Gate 1) and is
/// documented as such by the reference rather than listed here.
pub(crate) const REQUIRED_ENV_VARS: &[&str] = &[
    "DATABASE_URL",
    "OIDC_ISSUER_URL",
    "OIDC_CLIENT_ID",
    "OIDC_CLIENT_SECRET",
    "OIDC_REDIRECT_URI",
];

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
    /// writeback pools. Must be ≥ 1 — a zero cap yields a pool that can
    /// never hand out a connection (`PoolTimedOut` on the first query).
    #[validate(range(min = 1, message = "must be at least 1"))]
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
    ///
    /// NOTE: any new secret-bearing field must also be added to the
    /// `SECRET_FIELDS` list so a deserialize error never echoes its value
    /// (hard rule 7).
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
    /// `true` when `DATABASE_URL_INGESTION` was unset/blank and
    /// `ingestion_database_url` was defaulted to `database_url` by Gate 2
    /// — i.e. the ingestion pipeline runs under the application role
    /// (`reverie_app`) instead of the scoped `reverie_ingestion` role.
    /// Not env-sourced; set by [`Config::from_figment`] and surfaced as a
    /// startup `tracing::warn!` in [`crate::run`] (tracing is not yet live
    /// when the fallback fires), so the role-separation collapse is
    /// auditable rather than silent.
    #[serde(skip)]
    #[schemars(skip)]
    pub ingestion_dsn_defaulted: bool,
}

/// Post-ingestion cleanup behaviour selector for the watcher's
/// "after a successful batch" hook.
///
/// Wire format (JSON, DB `text` column): lowercase string —
/// `"all"` | `"ingested"` | `"none"`.
///
/// Deliberately NOT `#[non_exhaustive]`: the ingestion watcher matches it
/// exhaustively, so adding a variant is a compile error at the match site
/// rather than a silent fall-through — the same property the `Command` enum
/// relies on.
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
///
/// Deliberately NOT `#[non_exhaustive]`: `reverie_api` is a single-crate
/// application with no downstream consumers, and the call sites match the
/// variants exhaustively, so a new variant surfaces as a compile error rather
/// than being silently absorbed.
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

        // Gate 2 — ingestion DSN falls back to the app DSN when blank. Record
        // the fallback so `run()` can warn once tracing is live: this collapses
        // the reverie_ingestion/reverie_app role separation and must be
        // auditable, not silent.
        if cfg.ingestion_database_url.trim().is_empty() {
            cfg.ingestion_database_url = cfg.database_url.clone();
            cfg.ingestion_dsn_defaulted = true;
        }

        // Gate 3 — required fields blank => MissingVar (NOT Invalid). Var names
        // come from REQUIRED_ENV_VARS (shared with the config reference); the
        // field-accessor list below MUST stay aligned with that order.
        for (value, var) in [
            &cfg.database_url,
            &cfg.oidc_issuer_url,
            &cfg.oidc_client_id,
            &cfg.oidc_client_secret,
            &cfg.oidc_redirect_uri,
        ]
        .into_iter()
        .zip(REQUIRED_ENV_VARS)
        {
            if value.trim().is_empty() {
                return Err(ConfigError::MissingVar((*var).into()));
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
    let formats: Vec<ManifestationFormat> = raw
        .split(',')
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
        .map(|s| {
            s.parse::<ManifestationFormat>().map_err(|_| {
                serde::de::Error::custom(format!(
                    "unsupported format '{s}'. Supported: epub, pdf, mobi, azw3, cbz, cbr"
                ))
            })
        })
        .collect::<Result<_, _>>()?;
    // A non-empty raw value that yields zero formats (e.g. `","` or `" , "`)
    // is rejected rather than silently producing an empty priority list — an
    // empty list makes the ingestion pipeline skip every candidate file with
    // no operator-visible cause.
    if formats.is_empty() {
        return Err(serde::de::Error::custom(
            "must list at least one format. Supported: epub, pdf, mobi, azw3, cbz, cbr",
        ));
    }
    Ok(formats)
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
///
/// The DSN fields embed credentials in their `user:password@host` form, so
/// they are included defensively: a `String`-typed DSN does not currently
/// reach a value-echoing coercion error (strings coerce trivially), but a
/// future type change (e.g. a `url::Url` DSN field) would reopen that path.
const SECRET_FIELDS: &[&str] = &[
    "database_url",
    "migration_database_url",
    "ingestion_database_url",
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
            ingestion_dsn_defaulted: false,
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
        let example = include_str!("../../../docker/staging.env.runtime.example");

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
    fn required_env_vars_are_known_and_mapped() {
        // REQUIRED_ENV_VARS is the shared source of required-ness for both the
        // Gate 3 startup check and the generated config reference. Every entry
        // must be a real ENV_MAP var name, or the reference would mark a
        // non-existent variable required.
        assert!(!REQUIRED_ENV_VARS.is_empty());
        let mapped: std::collections::HashSet<&str> =
            ENV_MAP.iter().map(|(name, _)| *name).collect();
        for var in REQUIRED_ENV_VARS {
            assert!(mapped.contains(var), "required var {var} absent from ENV_MAP");
        }
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

    #[test]
    fn format_priority_comma_only_is_rejected() {
        // A non-empty value that splits to zero formats must error, not boot
        // with an empty priority list that silently skips every file.
        let vars = with_overrides(&[("REVERIE_FORMAT_PRIORITY", ",")]);
        let err = cfg_from_owned(&vars).unwrap_err();
        assert!(err.to_string().contains("REVERIE_FORMAT_PRIORITY"), "{err}");
        assert!(err.to_string().contains("at least one format"), "{err}");
    }

    #[test]
    fn db_max_connections_round_trips_as_flat_field() {
        // GOTCHA-SPLIT end-to-end: the flat snake_case var deserializes onto
        // the top-level u32 field (not a `db.max.connections` sub-dict).
        let vars = with_overrides(&[("REVERIE_DB_MAX_CONNECTIONS", "20")]);
        let cfg = cfg_from_owned(&vars).unwrap();
        assert_eq!(cfg.db_max_connections, 20);
    }

    #[test]
    fn db_max_connections_zero_is_rejected() {
        let vars = with_overrides(&[("REVERIE_DB_MAX_CONNECTIONS", "0")]);
        let err = cfg_from_owned(&vars).unwrap_err();
        assert!(
            err.to_string().contains("REVERIE_DB_MAX_CONNECTIONS"),
            "{err}"
        );
    }

    #[test]
    fn enrichment_max_attempts_zero_is_rejected() {
        let vars = with_overrides(&[("REVERIE_ENRICHMENT_MAX_ATTEMPTS", "0")]);
        let err = cfg_from_owned(&vars).unwrap_err();
        assert!(
            err.to_string().contains("REVERIE_ENRICHMENT_MAX_ATTEMPTS"),
            "{err}"
        );
    }

    #[test]
    fn writeback_max_attempts_zero_is_rejected() {
        let vars = with_overrides(&[("REVERIE_WRITEBACK_MAX_ATTEMPTS", "0")]);
        let err = cfg_from_owned(&vars).unwrap_err();
        assert!(
            err.to_string().contains("REVERIE_WRITEBACK_MAX_ATTEMPTS"),
            "{err}"
        );
    }

    #[test]
    fn ingestion_dsn_blank_flags_defaulted_fallback() {
        // BASE_VARS omits DATABASE_URL_INGESTION → Gate 2 falls back to the app
        // DSN and flags it so `run()` can warn about the role-separation collapse.
        let cfg = cfg_from(BASE_VARS).unwrap();
        assert!(cfg.ingestion_dsn_defaulted);
        assert_eq!(cfg.ingestion_database_url, cfg.database_url);
    }

    #[test]
    fn ingestion_dsn_explicit_clears_defaulted_flag() {
        let vars = with_overrides(&[(
            "DATABASE_URL_INGESTION",
            "postgres://reverie_ingestion@localhost/reverie_dev",
        )]);
        let cfg = cfg_from_owned(&vars).unwrap();
        assert!(!cfg.ingestion_dsn_defaulted);
        assert_eq!(
            cfg.ingestion_database_url,
            "postgres://reverie_ingestion@localhost/reverie_dev"
        );
    }

    #[test]
    fn security_parse_bool_rejects_numeric_and_capitalized_truthy() {
        // Only lowercase `true`/`false` are booleans; legacy-truthy spellings
        // parse to Num (`1`) or Str (`True`/`YES`/`on`) and a `bool` field
        // rejects them. `1` exercises a different parse branch than `yes`.
        for bad in ["1", "True", "YES", "on"] {
            let err = security_from(&[("REVERIE_BEHIND_HTTPS", bad)]).unwrap_err();
            assert!(
                err.to_string().contains("REVERIE_BEHIND_HTTPS"),
                "expected '{bad}' rejected: {err}"
            );
        }
    }

    #[test]
    fn security_hsts_https_only_emits_max_age_only() {
        // behind_https without subdomains/preload is a valid production config:
        // a bare max-age with no suffixes.
        let cfg = security_from(&[("REVERIE_BEHIND_HTTPS", "true")]).unwrap();
        let v = cfg.hsts_header_value().unwrap();
        assert_eq!(v.to_str().unwrap(), "max-age=31536000");
    }
}
