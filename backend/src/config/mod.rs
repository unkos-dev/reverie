//! Environment-driven configuration loaded once at startup.
//!
//! Loading is a declarative figment pipeline (figment + serde + validator +
//! schemars; `docs/adr/0023-declarative-configuration-stack-figment-validator-schemars.md`): the custom
//! [`EnvProvider`] maps env-var names to dotted struct paths, `serde`
//! deserializes into the config structs (per-field defaults from the
//! `Default` impls), [`Config::from_figment`] applies the post-deserialize
//! security gates, and `validator` runs range + cross-field checks.
//! [`Config::from_env`] is the production entry point; tests inject env as
//! in-memory pairs via [`EnvProvider::from_pairs`] so test setup never
//! mutates the process environment. Subsystem configs
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
mod mail;
mod opds;
mod path;
mod provider;
mod reference;
mod security;
mod writeback;

pub use path::AbsoluteRootPath;

pub use cover::CoverConfig;
pub use enrichment::EnrichmentConfig;
pub use mail::{MailConfig, MailTransportConfig, RelayCredentials, SmtpSecurity};
pub use opds::OpdsConfig;
pub use provider::EnvProvider;
pub(crate) use provider::{CredentialValue, resolve_credential};
pub use reference::reference_markdown;
pub use security::SecurityConfig;
pub use writeback::WritebackConfig;

use crate::models::manifestation_format::ManifestationFormat;
use figment::Figment;
use provider::ENV_MAP;
use secrecy::{ExposeSecret, SecretString};
use validator::{Validate, ValidationError, ValidationErrors, ValidationErrorsKind};

/// Accessor returning a required field's resolved value, paired with its
/// env-var name in [`REQUIRED_FIELDS`].
type RequiredFieldAccessor = fn(&Config) -> &str;

/// Environment variables that must be present and non-blank for the server to
/// start (the Gate 2 check in [`Config::from_figment`]). Single source of truth
/// shared with the generated config reference ([`reference_markdown`]) so the
/// "Required" column can never drift from the startup contract — the schema's
/// own `required` array is empty because every config struct is
/// `#[serde(default)]`, so it cannot serve as that source.
///
/// Each entry pairs the env-var name (the reference's "Required" column reads
/// these) with an accessor for the resolved field value (Gate 2 rejects a blank
/// one). Pairing name and accessor in one entry makes them structurally
/// impossible to misalign — adding a required variable is a single edit here,
/// with no parallel list to keep in lockstep.
///
/// `DATABASE_URL_MIGRATION` is deliberately absent: it is *conditionally*
/// required (only when `REVERIE_AUTO_MIGRATE=true`, enforced by Gate 1) and is
/// documented as such by the reference rather than listed here.
pub(crate) const REQUIRED_FIELDS: &[(&str, RequiredFieldAccessor)] =
    &[("DATABASE_URL", |c| c.database_url.expose_secret())];

/// Fields required by normal server startup, shared with the configuration reference.
pub(crate) const SERVER_REQUIRED_FIELDS: &[(&str, RequiredFieldAccessor)] =
    &[("DATABASE_URL_INGESTION", |c| {
        c.ingestion_database_url.expose_secret()
    })];

/// OIDC fields that become required *together* once OIDC is configured (the
/// issuer URL is present). OIDC is enabled iff configured; there is no separate
/// `oidc_enabled` flag, so these are conditionally, not
/// unconditionally, required: a fully-unset OIDC block is valid (local-only
/// instance), but a partially-configured one (issuer set, secret missing) is a
/// `MissingVar` for the absent field. Gate 3 in [`Config::from_figment`] enforces
/// this; the issuer accessor is listed first so an issuer-only block still
/// reports the next missing field rather than itself.
const OIDC_FIELDS: &[(&str, RequiredFieldAccessor)] = &[
    ("OIDC_ISSUER_URL", |c| c.oidc_issuer_url.as_str()),
    ("OIDC_CLIENT_ID", |c| c.oidc_client_id.as_str()),
    ("OIDC_CLIENT_SECRET", |c| {
        c.oidc_client_secret.expose_secret()
    }),
    ("OIDC_REDIRECT_URI", |c| c.oidc_redirect_uri.as_str()),
];

/// Resolved process-wide configuration.
///
/// Fields reflect the settled view of
/// the environment after defaults, parsing, and validation; subsystem
/// configs (OPDS, enrichment, cover, writeback, security) are nested as
/// owned values so callers do not pass the entire `Config` into helpers
/// that only need one slice.
#[derive(Debug, Clone, serde::Deserialize, schemars::JsonSchema, Validate)]
#[serde(default)]
#[validate(schema(function = "validate_opds_public_url"))]
#[validate(schema(function = "validate_mail_public_url"))]
#[expect(
    clippy::struct_excessive_bools,
    reason = "this is the flat env-sourced configuration root; its boolean fields are independent operator toggles (local auth, auto-migrate, self-registration, breach check, ...) that map one-to-one to env vars, not a state machine that should be modelled as an enum"
)]
pub struct Config {
    /// HTTP listen port (`REVERIE_PORT`, default `3000`).
    pub port: u16,
    /// Primary database DSN (`DATABASE_URL`, required). Connections opened
    /// against this DSN run as `reverie_app`; user-facing queries acquire
    /// transactions through [`crate::db::acquire_with_rls`].
    #[schemars(with = "String", default = "String::new")]
    pub database_url: SecretString,
    /// Absolute root for managed manifestation files (`REVERIE_LIBRARY_PATH`,
    /// default `/data/library`). Must exist before server startup; OPDS opens
    /// recorded relative locations through its pinned directory capability.
    pub library_path: AbsoluteRootPath,
    /// Absolute ingestion drop directory (`REVERIE_INGESTION_PATH`,
    /// default `/data/ingestion`). Must exist before server startup.
    pub ingestion_path: AbsoluteRootPath,
    /// Log-filter directive resolved from the environment with cascading
    /// precedence: `REVERIE_LOG_LEVEL` > `RUST_LOG` > `"info"`. The
    /// `REVERIE_*` operator namespace wins on conflict so staging docs
    /// stay coherent; `RUST_LOG` is honoured as the ecosystem default for
    /// developer convenience. The subscriber filter in [`crate::run`]
    /// parses this string directly (no further env re-read), so the
    /// precedence resolved here is the single source of truth for the
    /// process lifetime.
    pub log_level: String,
    /// Per-pool connection cap (`REVERIE_DB_MAX_CONNECTIONS`, default
    /// `10`); applied identically to the primary, ingestion, and
    /// writeback pools. Must be ≥ 1: a zero cap yields a pool that can
    /// never hand out a connection (`PoolTimedOut` on the first query).
    #[validate(range(min = 1, message = "must be at least 1"))]
    pub db_max_connections: u32,
    /// Attempts per minute per client IP, independently for password/setup/registration,
    /// recovery and OIDC start (`REVERIE_LOGIN_RATE_PER_MIN`, default `10`).
    /// Account admission uses ten completed failures in a rolling 15-minute window
    /// and a fixed 15-minute block. Rejections do not extend the block.
    /// Must be at least 1.
    #[validate(range(min = 1, message = "must be at least 1"))]
    pub login_rate_per_min: u32,
    /// Request-path password hashing limit per process
    /// (`REVERIE_AUTH_HASH_CONCURRENCY`, default `1`). Each permit allows one more
    /// parallel Argon2 operation of about 19 MiB; login holds one database
    /// connection per permit.
    #[validate(range(min = 1, message = "must be at least 1"))]
    pub auth_hash_concurrency: u32,
    /// Minimum length for a local-account password
    /// (`REVERIE_PASSWORD_MIN_LENGTH`, default `15`, the single-factor
    /// floor). The length floor, the zxcvbn strength floor
    /// ([`Self::password_min_zxcvbn_score`]), and the HIBP breach check
    /// ([`Self::password_breach_check_enabled`]) together form the password
    /// policy applied at bootstrap, registration, recovery, credential-link redemption
    /// and self-service change. Must not exceed [`Self::password_max_length`].
    #[validate(range(min = 15, message = "must be at least 15"))]
    pub password_min_length: usize,
    /// Maximum new-password length in Unicode code points
    /// (`REVERIE_PASSWORD_MAX_LENGTH`, default `256`, range `64..=256`).
    /// A denial-of-service cap,
    /// not a composition rule: NIST SP 800-63B forbids composition rules but
    /// permits a length cap, and an unbounded password is a CPU-exhaustion
    /// vector on the unauthenticated registration path (zxcvbn and Argon2 cost
    /// both grow with length). Checked before any strength or breach work.
    #[validate(range(min = 64, max = 256, message = "must be between 64 and 256"))]
    pub password_max_length: usize,
    /// Minimum acceptable zxcvbn strength score, 0..=4
    /// (`REVERIE_PASSWORD_MIN_ZXCVBN_SCORE`, default `2`). A candidate scoring
    /// below this is rejected as too weak, with the estimator's feedback
    /// returned in the 422 response.
    #[validate(range(max = 4, message = "must be between 0 and 4"))]
    pub password_min_zxcvbn_score: u8,
    /// Whether the HIBP Pwned Passwords breach check runs at credential-setting
    /// time (`REVERIE_PASSWORD_BREACH_CHECK_ENABLED`, default `true`). The check
    /// fails open: a HIBP outage never blocks account creation or login. Set
    /// `false` to skip the outbound request entirely (e.g. fully offline
    /// instances).
    pub password_breach_check_enabled: bool,
    /// Whether self-service account registration is enabled
    /// (`REVERIE_SELF_REGISTRATION_ENABLED`, default `false`). When off, the
    /// `/auth/register` endpoint returns 404. A self-registered account is
    /// always a non-admin, non-child Adult; admin and child accounts are created
    /// only by an existing administrator.
    pub self_registration_enabled: bool,
    /// Optional forwarded-for header to trust for the client IP behind a reverse
    /// proxy (`REVERIE_TRUSTED_CLIENT_IP_HEADER`, e.g. `X-Forwarded-For`). Unset
    /// by default: the TCP peer is used. An unauthenticated forwarded header is
    /// attacker-spoofable, so it is honoured only when an operator names it.
    pub trusted_client_ip_header: Option<String>,
    /// OIDC issuer URL (`OIDC_ISSUER_URL`). Setting it requires the other three
    /// `OIDC_*` fields together. Every outbound OIDC request uses a shared client
    /// with 5-second connect and 10-second total timeouts, no redirects, and HTTPS
    /// enforced by the transport. Issuers permit neither queries nor fragments;
    /// authorization, token and JWKS URLs permit queries but reject fragments.
    /// Private HTTPS providers are supported. TLS uses the platform trust store
    /// through `rustls-platform-verifier`; a private CA must be installed in the
    /// container or host trust store.
    ///
    /// THREAT: a malicious or compromised issuer can supply attacker-controlled
    /// signing keys used to verify ID tokens, enabling identity forgery.
    pub oidc_issuer_url: String,
    /// OIDC client id (`OIDC_CLIENT_ID`, required when OIDC is configured).
    pub oidc_client_id: String,
    /// OIDC client secret (`OIDC_CLIENT_SECRET`, required when OIDC is
    /// configured). Treated as secret material; never logged.
    ///
    /// NOTE: any new secret-bearing field must also be added to the
    /// `SECRET_FIELDS` list so parsing and validation errors never echo its value.
    #[schemars(with = "String", default = "String::new")]
    pub oidc_client_secret: SecretString,
    /// OIDC redirect URI (`OIDC_REDIRECT_URI`). Required when OIDC is configured
    /// (Gate 3); must match the value registered with the issuer. Its path must
    /// be exactly `/auth/callback`; a trailing slash or another path refuses startup.
    pub oidc_redirect_uri: String,
    /// Whether local email+password authentication is enabled
    /// (`REVERIE_LOCAL_AUTH_ENABLED`, default `true`). Reverie is local-first out
    /// of the box (`docs/adr/0029-unified-identity-with-pluggable-authentication-providers.md`), so this
    /// defaults on. Setting it `false` without configuring OIDC is rejected at
    /// startup (Gate 3): at least one auth provider must remain usable or the
    /// instance locks everyone out.
    pub local_auth_enabled: bool,
    /// Migration DSN (`DATABASE_URL_MIGRATION`). `reverie_migrator`
    /// credentials for the ephemeral migration pool. `None` on the default
    /// server path: the application process holds no migration credential
    /// unless [`Self::auto_migrate`] is set. Required (else
    /// [`ConfigError::MissingVar`]) only when `auto_migrate` is true.
    #[schemars(with = "Option<String>", default = "Option::<String>::default")]
    pub migration_database_url: Option<SecretString>,
    /// Run pending migrations in-process at startup
    /// (`REVERIE_AUTO_MIGRATE`, default `false`). The shipped default is
    /// out-of-band migration via `reverie migrate`; when this is `true` the
    /// long-lived server process carries the migration credential for its
    /// whole lifetime, so it is an opt-in escape hatch only. Requires
    /// [`Self::migration_database_url`] to be set.
    pub auto_migrate: bool,
    /// Ingestion-pipeline DSN (`DATABASE_URL_INGESTION`), required for normal
    /// server startup; missing, empty or whitespace-only values refuse startup.
    /// Use the dedicated `reverie_ingestion` role for the
    /// `*_ingestion_full_access` RLS policies. One-shot administrative commands
    /// do not require this credential.
    #[schemars(with = "String", default = "String::new")]
    pub ingestion_database_url: SecretString,
    /// Accepted ingestion formats (`REVERIE_ACCEPTED_FORMATS`, comma-separated;
    /// default `epub`). Seeds settings once; saved values win. At least one format is required.
    #[serde(deserialize_with = "de_accepted_formats")]
    #[schemars(schema_with = "accepted_formats_schema")]
    pub accepted_formats: Vec<ManifestationFormat>,
    /// Initial imported-source cleanup setting (`REVERIE_CLEANUP_IMPORTED`, default `true`); saved values win.
    pub cleanup_imported: bool,
    /// Initial duplicate-source cleanup setting (`REVERIE_CLEANUP_DUPLICATES`, default `false`); saved values win.
    pub cleanup_duplicates: bool,
    /// Metadata enrichment knobs (concurrency, cache TTLs, etc.).
    #[validate(nested)]
    pub enrichment: EnrichmentConfig,
    /// Cover-image acquisition limits (max bytes, redirect cap, etc.).
    #[validate(nested)]
    pub cover: CoverConfig,
    /// Writeback worker knobs (concurrency, retry cap).
    #[validate(nested)]
    pub writeback: WritebackConfig,
    /// OPDS catalogue settings (mount enable, page size, realm).
    #[validate(nested)]
    pub opds: OpdsConfig,
    /// Optional SMTP delivery for authentication mail (relay, sender,
    /// support address and queue key).
    pub mail: MailConfig,
    /// Response-header policy (CSP, HSTS, reporting endpoint, dist
    /// path). `csp_*_header` fields are finalised by [`crate::run`]
    /// after construction.
    #[validate(nested)]
    pub security: SecurityConfig,
    /// Optional Google Books API key
    /// (`REVERIE_GOOGLEBOOKS_API_KEY`); when set, requests bypass the
    /// public anonymous quota.
    #[schemars(with = "Option<String>", default = "Option::<String>::default")]
    pub googlebooks_api_key: Option<SecretString>,
    /// Optional Hardcover bearer token
    /// (`REVERIE_HARDCOVER_API_TOKEN`); requests are skipped when
    /// unset.
    #[schemars(with = "Option<String>", default = "Option::<String>::default")]
    pub hardcover_api_token: Option<SecretString>,
    /// Operator contact (`REVERIE_OPERATOR_CONTACT`); embedded into
    /// the outbound `User-Agent` to claim `OpenLibrary`'s identified
    /// 3 req/s rate-limit tier (vs. 1 req/s anonymous).
    pub operator_contact: Option<String>,
    /// Externally-visible origin of this instance (`REVERIE_PUBLIC_URL`): an
    /// HTTP(S) origin without credentials, path, query or fragment; a trailing
    /// root slash is accepted. Required when OPDS or SMTP delivery is enabled.
    /// OPDS feeds and emailed links are rooted here; it is never derived from
    /// request headers.
    #[validate(custom(function = "validate_public_origin"))]
    pub public_url: Option<url::Url>,
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
    /// Load only the database credential and public origin for host recovery.
    ///
    /// # Errors
    /// Returns a setting-named error for missing, conflicting or invalid inputs.
    pub fn host_recovery_from_env() -> Result<(SecretString, url::Url), ConfigError> {
        #[derive(Default, serde::Deserialize)]
        #[serde(default)]
        struct Settings {
            database_url: SecretString,
            public_url: Option<url::Url>,
        }
        let settings: Settings =
            Figment::from(EnvProvider::from_process_env().host_recovery_only())
                .extract()
                .map_err(|error| map_figment_error(&error))?;
        let origin = settings
            .public_url
            .ok_or_else(|| ConfigError::MissingVar("REVERIE_PUBLIC_URL".into()))?;
        validate_public_origin(&origin).map_err(|_| ConfigError::Invalid {
            var: "REVERIE_PUBLIC_URL".into(),
            reason: "must be an http or https origin without credentials, path, query or fragment"
                .into(),
        })?;
        if settings.database_url.expose_secret().trim().is_empty() {
            return Err(ConfigError::MissingVar("DATABASE_URL".into()));
        }
        Ok((settings.database_url, origin))
    }

    /// Public entry point for production: reads from the process environment
    /// through the figment pipeline. The binary discovers no env file itself;
    /// populating the process environment is the caller's responsibility (a
    /// container's declared environment, or a sourced dev env file).
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::MissingVar`] when a required variable is
    /// unset (`DATABASE_URL`, `OIDC_*`); returns [`ConfigError::Invalid`]
    /// when an optional variable is set but fails parse or validation
    /// (out-of-range numerics, unsupported `accepted_formats` entries,
    /// malformed URLs, header-injection-prone characters in
    /// `REVERIE_CSP_REPORT_ENDPOINT`, etc.); returns [`ConfigError::Multiple`]
    /// when more than one declarative validation fails together. The
    /// variant carries the offending variable name so the surfaced
    /// operator-facing message is actionable.
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_figment(&Figment::from(EnvProvider::from_process_env()))
    }

    /// Check the credentials required by normal server startup.
    ///
    /// One-shot administrative commands use the shared loader without this check.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::MissingVar`] when `DATABASE_URL_INGESTION` is
    /// absent, empty or whitespace-only.
    pub fn validate_server(&self) -> Result<(), ConfigError> {
        // THREAT: Missing ingestion credentials must not fall back to application-role access.
        for &(var, field) in SERVER_REQUIRED_FIELDS {
            if field(self).trim().is_empty() {
                return Err(ConfigError::MissingVar(var.into()));
            }
        }
        Ok(())
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
    ///    `docs/adr/0014-migration-model-hybrid-entrypoints-and-a-least-privilege-role.md`.
    /// 2. **Required-field check**: a blank required field (`DATABASE_URL`,
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
                .as_ref()
                .is_none_or(|s| s.expose_secret().trim().is_empty())
            {
                return Err(ConfigError::MissingVar("DATABASE_URL_MIGRATION".into()));
            }
        } else {
            cfg.migration_database_url = None;
        }

        // Gate 1b — mail secrets. A disabled mail block carries no key or relay
        // password, and an enabled one must be complete before startup.
        if cfg.mail.enabled {
            cfg.mail.validate_enabled()?;
        } else {
            cfg.mail.password = None;
            cfg.mail.queue_key = None;
        }

        // Gate 2 — required fields blank => MissingVar (NOT Invalid). Name and
        // field accessor are paired in REQUIRED_FIELDS (shared with the config
        // reference), so the two can never drift out of alignment.
        for &(var, field) in REQUIRED_FIELDS {
            if field(&cfg).trim().is_empty() {
                return Err(ConfigError::MissingVar(var.into()));
            }
        }

        // Gate 3: provider availability. OIDC is enabled iff
        // configured (its issuer is set); a partially-configured OIDC block is a
        // MissingVar for the absent field, so an instance never half-enables a
        // provider. At least one provider (local or OIDC) must remain usable, or
        // the instance would refuse every login.
        if cfg.oidc_configured() {
            for &(var, field) in OIDC_FIELDS {
                if field(&cfg).trim().is_empty() {
                    return Err(ConfigError::MissingVar(var.into()));
                }
            }
        }
        if cfg.oidc_configured() {
            let redirect =
                url::Url::parse(&cfg.oidc_redirect_uri).map_err(|_| ConfigError::Invalid {
                    var: "OIDC_REDIRECT_URI".into(),
                    reason: "must be a URL with path exactly /auth/callback".into(),
                })?;
            if redirect.path() != "/auth/callback" {
                return Err(ConfigError::Invalid {
                    var: "OIDC_REDIRECT_URI".into(),
                    reason: "path must be exactly /auth/callback".into(),
                });
            }
        }
        if !cfg.local_auth_enabled && !cfg.oidc_configured() {
            return Err(ConfigError::Invalid {
                var: "REVERIE_LOCAL_AUTH_ENABLED".into(),
                reason: "at least one auth provider must be enabled: set \
                         REVERIE_LOCAL_AUTH_ENABLED=true or configure OIDC (OIDC_ISSUER_URL)"
                    .into(),
            });
        }

        // Gate 4: the operator contact is embedded verbatim in the outbound
        // `User-Agent`, and reqwest refuses to build a client whose UA is not
        // a valid header value. Rejecting it here turns a per-request panic in
        // the UA-setting constructors into a startup error.
        if axum::http::HeaderValue::from_str(&cfg.user_agent()).is_err() {
            return Err(ConfigError::Invalid {
                var: "REVERIE_OPERATOR_CONTACT".into(),
                reason: "must be a valid HTTP header value (visible ASCII, no control characters)"
                    .into(),
            });
        }

        // Declarative validation (range + cross-field). Aggregated.
        cfg.validate().map_err(|e| map_validation_errors(&e))?;
        if cfg.password_min_length > cfg.password_max_length {
            return Err(ConfigError::Invalid {
                var: "REVERIE_PASSWORD_MIN_LENGTH".into(),
                reason: "must not exceed REVERIE_PASSWORD_MAX_LENGTH".into(),
            });
        }

        Ok(cfg)
    }

    /// Whether the OIDC authentication path is enabled. OIDC is enabled iff it is
    /// configured, signalled by a non-blank issuer URL; there is no
    /// separate `oidc_enabled` flag that could disagree with the actual config.
    /// Gate 3 guarantees that when this is `true`, all four `OIDC_*` fields are
    /// present, so callers can treat a configured instance as fully usable.
    pub fn oidc_configured(&self) -> bool {
        !self.oidc_issuer_url.trim().is_empty()
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

/// Deserialize the comma-separated `REVERIE_ACCEPTED_FORMATS` surface
/// (`epub,pdf,mobi`) into the ranked `Vec<ManifestationFormat>`.
///
/// The env contract is bare CSV in a single variable — NOT figment array
/// syntax (`[a,b]`) — so the split lives here rather than relying on figment's
/// array parsing. Each token is trimmed, lowercased, and parsed via
/// [`ManifestationFormat`]'s `FromStr`; an unsupported token rejects the whole
/// value.
fn de_accepted_formats<'de, D>(de: D) -> Result<Vec<ManifestationFormat>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw = <String as serde::Deserialize>::deserialize(de)?;
    if raw.trim().is_empty() {
        return Err(serde::de::Error::custom(
            "at least one format is required; supported: epub",
        ));
    }
    let formats = raw
        .split(',')
        .map(|token| {
            let token = token.trim().to_lowercase();
            if token != "epub" {
                return Err(serde::de::Error::custom(format!(
                    "unsupported format '{token}'; supported: epub"
                )));
            }
            Ok(ManifestationFormat::Epub)
        })
        .collect::<Result<Vec<_>, D::Error>>()?;
    if formats.len() > 1 {
        return Err(serde::de::Error::custom(
            "accepted_formats contains duplicate format: epub",
        ));
    }
    Ok(formats)
}

/// Schema for `accepted_formats`: the formats ingestion supports (a subset of
/// [`ManifestationFormat`]), with at least one required.
fn accepted_formats_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({
        "type": "array",
        "default": ["epub"],
        "minItems": 1,
        "items": {
            "oneOf": [
                {
                    "description": "EPUB 2 / 3 reflowable e-book.",
                    "type": "string",
                    "const": "epub"
                }
            ]
        }
    })
}

// ---------------------------------------------------------------------------
// Error mapping: figment / validator errors → var-named `ConfigError`.
// ---------------------------------------------------------------------------

/// Credential paths scrubbed before parsing or validation errors become diagnostics.
const SECRET_FIELDS: &[&str] = &[
    "database_url",
    "migration_database_url",
    "ingestion_database_url",
    "oidc_client_secret",
    "googlebooks_api_key",
    "hardcover_api_token",
    "mail.password",
    "mail.queue_key",
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
        if let figment::error::Kind::Message(reason) = &e.kind
            && (reason == "credential file could not be read as UTF-8"
                || reason == &format!("conflicting sources: {var} and {var}_FILE"))
        {
            return ConfigError::Invalid {
                var,
                reason: reason.clone(),
            };
        }
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
                    let dotted = fe.params.get("var").and_then(|v| v.as_str()).map_or_else(
                        || join_path(prefix, field),
                        |var| {
                            ENV_MAP
                                .iter()
                                .find(|(name, _)| *name == var)
                                .map_or(var, |(_, path)| *path)
                                .to_owned()
                        },
                    );
                    let var =
                        env_name_for(&dotted).map_or_else(|| dotted.clone(), ToString::to_string);
                    // THREAT: Validator messages and codes can contain credentials even after secret wrapping.
                    let reason = if SECRET_FIELDS.contains(&dotted.as_str()) {
                        "invalid value (omitted — secret-bearing field)".into()
                    } else {
                        fe.message
                            .as_ref()
                            .map_or_else(|| fe.code.to_string(), ToString::to_string)
                    };
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
        let [library_path, ingestion_path] = AbsoluteRootPath::defaults();
        Self {
            port: 3000,
            // REQUIRED — empty sentinel; reviewer handles MissingVar (GOTCHA-REQUIRED).
            database_url: String::new().into(),
            library_path,
            ingestion_path,
            log_level: "info".into(),
            db_max_connections: 10,
            login_rate_per_min: 10,
            auth_hash_concurrency: 1,
            password_min_length: 15,
            password_max_length: 256,
            password_min_zxcvbn_score: 2,
            password_breach_check_enabled: true,
            self_registration_enabled: false,
            trusted_client_ip_header: None,
            // REQUIRED — empty sentinels.
            oidc_issuer_url: String::new(),
            oidc_client_id: String::new(),
            oidc_client_secret: String::new().into(),
            oidc_redirect_uri: String::new(),
            // Local-first default; Gate 3 guards the lock-out case.
            local_auth_enabled: true,
            migration_database_url: None,
            auto_migrate: false,
            ingestion_database_url: String::new().into(),
            accepted_formats: vec![ManifestationFormat::Epub],
            cleanup_imported: true,
            cleanup_duplicates: false,
            enrichment: EnrichmentConfig::default(),
            cover: CoverConfig::default(),
            writeback: WritebackConfig::default(),
            opds: OpdsConfig::default(),
            security: SecurityConfig::default(),
            googlebooks_api_key: None,
            hardcover_api_token: None,
            operator_contact: None,
            public_url: None,
            mail: MailConfig::default(),
        }
    }
}

/// `REVERIE_PUBLIC_URL` must be a bare HTTP(S) origin. The value is never
/// echoed because an operator may have pasted credentials into it.
fn validate_public_origin(url: &url::Url) -> Result<(), ValidationError> {
    let origin_only = matches!(url.scheme(), "http" | "https")
        && url.has_host()
        && url.username().is_empty()
        && url.password().is_none()
        && url.path() == "/"
        && url.query().is_none()
        && url.fragment().is_none();
    if origin_only {
        return Ok(());
    }
    let mut e = ValidationError::new("public_url_origin");
    e.message =
        Some("must be an http or https origin without credentials, path, query or fragment".into());
    Err(e)
}

/// OPDS emits absolute URLs rooted at the public origin, so it is required
/// whenever the catalogue is enabled.
fn validate_opds_public_url(config: &Config) -> Result<(), ValidationError> {
    if config.opds.enabled && config.public_url.is_none() {
        let mut e = ValidationError::new("public_url_required");
        e.add_param("var".into(), &"REVERIE_PUBLIC_URL");
        e.message = Some("required when REVERIE_OPDS_ENABLED=true".into());
        return Err(e);
    }
    Ok(())
}

/// Emailed links are rooted at the public origin, so it is required whenever
/// SMTP delivery is enabled.
fn validate_mail_public_url(config: &Config) -> Result<(), ValidationError> {
    if config.mail.enabled && config.public_url.is_none() {
        let mut e = ValidationError::new("public_url_required");
        e.add_param("var".into(), &"REVERIE_PUBLIC_URL");
        e.message = Some("required when REVERIE_SMTP_ENABLED=true".into());
        return Err(e);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn library_storage_config_absolute_defaults() {
        let config = cfg_from(BASE_VARS).unwrap();
        assert_eq!(config.library_path.as_str(), "/data/library");
        assert_eq!(config.ingestion_path.as_str(), "/data/ingestion");
    }

    #[test]
    fn library_storage_config_empty_and_relative_rejected() {
        for var in ["REVERIE_LIBRARY_PATH", "REVERIE_INGESTION_PATH"] {
            for value in ["", "library", "./library", "../library"] {
                let error = cfg_from_owned(&with_overrides(&[(var, value)])).unwrap_err();
                assert!(matches!(error, ConfigError::Invalid { var: ref name, .. } if name == var));
            }
        }
    }

    /// Build a `Config` through the figment pipeline from in-memory env pairs:
    /// the process-env-free, parallel-safe test seam (GOTCHA-TESTSEAM); never
    /// mutates global env.
    /// Strings flow through `EnvProvider`'s parse/coerce path, exercising the
    /// real production deserialization (GOTCHA-TESTFIDELITY): no pre-typed
    /// `Serialized(struct)` shortcut that would bypass where the bugs live.
    fn cfg_from(vars: &[(&str, &str)]) -> Result<Config, ConfigError> {
        Config::from_figment(&Figment::from(EnvProvider::from_pairs(vars)))
    }

    /// `cfg_from` variant for owned-string var lists built via
    /// `with_overrides`.
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

    #[test]
    fn from_env_empty_migration_url_treated_as_none_when_auto_migrate_off() {
        // An exported-empty DSN is indistinguishable from unset for the
        // default path: both yield None, no error.
        let vars = with_overrides(&[("DATABASE_URL_MIGRATION", "")]);
        let config = cfg_from_owned(&vars).unwrap();
        assert!(config.migration_database_url.is_none());
    }

    #[test]
    fn migration_url_nulled_when_auto_migrate_off() {
        // GOTCHA-MIGGATE (null-out): the long-lived server must not carry the
        // migrator credential when auto_migrate is off, even if the DSN is set.
        let vars = with_overrides(&[("DATABASE_URL_MIGRATION", "postgres://m@localhost/d")]);
        let cfg = cfg_from_owned(&vars).unwrap();
        assert!(cfg.migration_database_url.is_none());
    }

    #[test]
    fn config_debug_omits_all_credentials() {
        let overrides: Vec<_> = SECRET_FIELDS
            .iter()
            .enumerate()
            .map(|(index, field)| {
                (
                    env_name_for(field).unwrap(),
                    format!("credential-debug-marker-{index}"),
                )
            })
            .collect();
        let mut vars = with_overrides(&[("REVERIE_AUTO_MIGRATE", "true")]);
        vars.extend(
            overrides
                .iter()
                .map(|(var, value)| ((*var).to_owned(), value.clone())),
        );
        let config = cfg_from_owned(&vars).unwrap();
        let debug = format!("{config:?}");
        assert!(debug.contains("port: 3000"));
        assert!(debug.contains("log_level: \"info\""));
        for (_, marker) in overrides {
            assert!(
                !debug.contains(&marker),
                "credential marker appeared in Config Debug"
            );
        }
    }

    #[test]
    fn secret_field_deser_error_has_no_value() {
        for field in SECRET_FIELDS {
            let var = env_name_for(field).unwrap();
            let marker = "987654321";
            let vars = with_overrides(&[(var, marker)]);
            let error = cfg_from_owned(&vars).unwrap_err();
            for output in [error.to_string(), format!("{error:?}")] {
                assert!(output.contains(var));
                assert!(
                    !output.contains(marker),
                    "credential marker appeared in parse error"
                );
                assert!(output.contains("invalid value (omitted — secret-bearing field)"));
            }
        }
    }

    #[test]
    fn secret_field_validation_error_has_no_value() {
        for field in SECRET_FIELDS {
            let var = env_name_for(field).unwrap();
            for named_var in [None, Some(var), Some(*field)] {
                for message in [None, Some("credential-message-marker")] {
                    let mut error = validator::ValidationError::new("credential-code-marker");
                    error.message = message.map(Into::into);
                    error.add_param("value".into(), &"credential-parameter-marker");
                    if let Some(named_var) = named_var {
                        error.add_param("var".into(), &named_var);
                    }
                    let mut errors = ValidationErrors::new();
                    errors.add(
                        if named_var.is_some() {
                            "__all__"
                        } else {
                            field
                        },
                        error,
                    );
                    let mapped = map_validation_errors(&errors);
                    for output in [mapped.to_string(), format!("{mapped:?}")] {
                        assert!(output.contains(var));
                        for marker in [
                            "credential-code-marker",
                            "credential-message-marker",
                            "credential-parameter-marker",
                        ] {
                            assert!(
                                !output.contains(marker),
                                "credential marker appeared in validation error"
                            );
                        }
                        assert!(output.contains("invalid value (omitted — secret-bearing field)"));
                    }
                }
            }
        }
    }

    #[test]
    fn non_secret_validation_error_retains_reason() {
        let mut error = validator::ValidationError::new("range");
        error.message = Some("must be at least 15".into());
        let mut errors = ValidationErrors::new();
        errors.add("password_min_length", error);
        let mapped = map_validation_errors(&errors);
        for output in [mapped.to_string(), format!("{mapped:?}")] {
            assert!(output.contains("REVERIE_PASSWORD_MIN_LENGTH"));
            assert!(output.contains("must be at least 15"));
        }
    }

    #[test]
    fn nested_validation_errors_preserve_multiple_and_non_secret_reasons() {
        let mut secret = validator::ValidationError::new("credential-code-marker");
        secret.message = Some("credential-message-marker".into());
        secret.add_param("var".into(), &"DATABASE_URL");
        secret.add_param("value".into(), &"credential-parameter-marker");
        let mut child = ValidationErrors::new();
        child.add("__all__", secret);
        let mut plain = validator::ValidationError::new("range");
        plain.message = Some("must be at least 15".into());
        let mut errors = ValidationErrors::new();
        errors.add("password_min_length", plain);
        errors.errors_mut().insert(
            "nested".into(),
            ValidationErrorsKind::Struct(Box::new(child.clone())),
        );
        errors.errors_mut().insert(
            "items".into(),
            ValidationErrorsKind::List(std::collections::BTreeMap::from([(0, Box::new(child))])),
        );
        let mapped = map_validation_errors(&errors);
        assert!(matches!(&mapped, ConfigError::Multiple(errors) if errors.len() == 3));
        for output in [mapped.to_string(), format!("{mapped:?}")] {
            assert!(output.contains("DATABASE_URL"));
            assert!(output.contains("REVERIE_PASSWORD_MIN_LENGTH"));
            assert!(output.contains("must be at least 15"));
            assert!(!output.contains("credential-"));
        }
    }

    #[test]
    fn config_schema_has_no_secret_default_values() {
        let schema = serde_json::to_value(schemars::schema_for!(Config)).unwrap();
        let props = schema["properties"].as_object().expect("properties object");
        for field in SECRET_FIELDS {
            let property = field.split_once('.').map_or_else(
                || props.get(*field).expect("credential property exists"),
                |(parent, leaf)| {
                    let reference = props[parent]["$ref"]
                        .as_str()
                        .expect("sub-struct reference");
                    let definition = &schema["$defs"][reference.trim_start_matches("#/$defs/")];
                    &definition["properties"][leaf]
                },
            );
            let default = property.get("default").expect("credential default exists");
            let expected = if matches!(
                *field,
                "database_url" | "ingestion_database_url" | "oidc_client_secret"
            ) {
                serde_json::json!("")
            } else {
                serde_json::Value::Null
            };
            assert_eq!(*default, expected, "credential default for {field}");
        }
        assert_eq!(props["port"]["default"], serde_json::json!(3000));
    }

    #[test]
    fn ingestion_dsn_blank_is_preserved_for_admin_configuration() {
        for vars in [
            BASE_VARS
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            with_overrides(&[("DATABASE_URL_INGESTION", "")]),
            with_overrides(&[("DATABASE_URL_INGESTION", " \t\n")]),
        ] {
            let cfg = cfg_from_owned(&vars).unwrap();
            assert_eq!(cfg.ingestion_database_url.expose_secret().trim(), "");
            assert_ne!(
                cfg.ingestion_database_url.expose_secret(),
                cfg.database_url.expose_secret()
            );
            let error = cfg.validate_server().unwrap_err();
            assert!(
                matches!(&error, ConfigError::MissingVar(var) if var == "DATABASE_URL_INGESTION")
            );
            assert_eq!(
                error.to_string(),
                "missing required environment variable: DATABASE_URL_INGESTION"
            );
        }
    }

    #[test]
    fn oidc_redirect_uri_requires_exact_callback_path() {
        for uri in [
            "https://example.com/auth/callback/",
            "https://example.com/other",
            "https://example.com/",
            "not-a-url",
        ] {
            let vars = with_overrides(&[("OIDC_REDIRECT_URI", uri)]);
            let error = cfg_from_owned(&vars).unwrap_err();
            assert!(
                matches!(&error, ConfigError::Invalid { var, .. } if var == "OIDC_REDIRECT_URI")
            );
            assert!(!error.to_string().contains(uri));
        }
        for uri in [
            "https://example.com/auth/callback",
            "http://localhost:3000/auth/callback",
        ] {
            let vars = with_overrides(&[("OIDC_REDIRECT_URI", uri)]);
            let config = cfg_from_owned(&vars).expect("exact callback path accepted");
            assert_eq!(config.oidc_redirect_uri, uri);
        }
    }

    #[test]
    fn password_policy_default_and_configurable_floor() {
        assert_eq!(Config::default().password_min_length, 15);
        for minimum in ["0", "8", "14"] {
            let vars = with_overrides(&[("REVERIE_PASSWORD_MIN_LENGTH", minimum)]);
            let error = cfg_from_owned(&vars).unwrap_err();
            assert!(error.to_string().contains("REVERIE_PASSWORD_MIN_LENGTH"));
            assert!(error.to_string().contains("at least 15"));
        }
        for minimum in ["15", "24"] {
            let vars = with_overrides(&[("REVERIE_PASSWORD_MIN_LENGTH", minimum)]);
            let config = cfg_from_owned(&vars).unwrap();
            assert_eq!(
                config.password_min_length,
                minimum.parse::<usize>().unwrap()
            );
        }
        let vars = with_overrides(&[("REVERIE_PASSWORD_MIN_LENGTH", "300")]);
        let error = cfg_from_owned(&vars).unwrap_err();
        assert!(error.to_string().contains("REVERIE_PASSWORD_MIN_LENGTH"));
        assert!(error.to_string().contains("REVERIE_PASSWORD_MAX_LENGTH"));
        for maximum in ["63", "257", "300"] {
            let vars = with_overrides(&[("REVERIE_PASSWORD_MAX_LENGTH", maximum)]);
            let error = cfg_from_owned(&vars).unwrap_err();
            assert!(error.to_string().contains("REVERIE_PASSWORD_MAX_LENGTH"));
        }
        for (minimum, maximum) in [("256", "256"), ("24", "64")] {
            let vars = with_overrides(&[
                ("REVERIE_PASSWORD_MIN_LENGTH", minimum),
                ("REVERIE_PASSWORD_MAX_LENGTH", maximum),
            ]);
            assert!(cfg_from_owned(&vars).is_ok());
        }
    }
}
