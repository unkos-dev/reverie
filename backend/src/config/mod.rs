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
mod opds;
mod path;
mod provider;
mod reference;
mod security;
mod writeback;

pub use path::AbsoluteRootPath;

pub use cover::CoverConfig;
pub use enrichment::EnrichmentConfig;
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
use validator::{Validate, ValidationErrors, ValidationErrorsKind};

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

/// Resource-server fields that become required *together* once resource-server
/// JWT validation is configured (the issuer URL is present). Mirrors
/// [`OIDC_FIELDS`]'s "required together iff trigger set" pattern, scoped to
/// just `issuer` + `audience`; `resource_server_jwks_url` and
/// `resource_server_require_at_jwt` are independently optional with safe
/// defaults (OIDC discovery, relaxed `typ` policy) and are never required.
const RESOURCE_SERVER_FIELDS: &[(&str, RequiredFieldAccessor)] = &[
    ("REVERIE_RESOURCE_SERVER_ISSUER", |c| {
        c.resource_server_issuer.as_str()
    }),
    ("REVERIE_RESOURCE_SERVER_AUDIENCE", |c| {
        c.resource_server_audience.as_str()
    }),
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
    /// Per-source login rate limit, attempts per minute per client IP
    /// (`REVERIE_LOGIN_RATE_PER_MIN`, default `10`). The hard blocker against
    /// credential-stuffing; must be at least 1 (a zero quota locks everyone out).
    #[validate(range(min = 1, message = "must be at least 1"))]
    pub login_rate_per_min: u32,
    /// Per-account backoff base, seconds (`REVERIE_LOGIN_THROTTLE_BASE_SECS`,
    /// default `2`). The first failed login for an account waits the lesser of
    /// base and cap; each further failure doubles the base up to the cap.
    #[validate(range(min = 1, message = "must be at least 1"))]
    pub login_throttle_base_secs: i32,
    /// Per-account backoff cap, seconds (`REVERIE_LOGIN_THROTTLE_CAP_SECS`,
    /// default `900`). Upper bound on the escalating per-account delay.
    #[validate(range(min = 1, message = "must be at least 1"))]
    pub login_throttle_cap_secs: i32,
    /// Minimum length for a local-account password
    /// (`REVERIE_PASSWORD_MIN_LENGTH`, default `15`, the single-factor
    /// floor). The length floor, the zxcvbn strength floor
    /// ([`Self::password_min_zxcvbn_score`]), and the HIBP breach check
    /// ([`Self::password_breach_check_enabled`]) together form the password
    /// policy applied at bootstrap, registration, recovery, admin create/reset,
    /// and self-service change. Existing credentials remain valid. Must not
    /// exceed [`Self::password_max_length`].
    #[validate(range(min = 15, message = "must be at least 15"))]
    pub password_min_length: usize,
    /// Maximum length for a local-account password, in characters
    /// (`REVERIE_PASSWORD_MAX_LENGTH`, default `256`). A denial-of-service cap,
    /// not a composition rule: NIST SP 800-63B forbids composition rules but
    /// permits a length cap, and an unbounded password is a CPU-exhaustion
    /// vector on the unauthenticated registration path (zxcvbn and Argon2 cost
    /// both grow with length). Checked before any strength or breach work.
    #[validate(range(min = 64, message = "must be at least 64 (NIST SP 800-63B)"))]
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
    /// Lifetime of a forgot-password recovery PIN, seconds
    /// (`REVERIE_RECOVERY_PIN_TTL_SECS`, default `900` = 15 minutes). Short by
    /// design: the PIN is single-use and rate-limited.
    #[validate(range(min = 60, message = "must be at least 60"))]
    pub recovery_pin_ttl_secs: i64,
    /// Directory the clear recovery PIN is written into, one file per user at
    /// `<dir>/<user_id>.pin` (file mode 0600, directory mode 0700), for an
    /// operator to read and relay (`REVERIE_RECOVERY_PIN_DIR`, default
    /// `/data/recovery-pins`). Per-user files keep concurrent recoveries from
    /// colliding. MUST be outside any web-served directory; the database stores
    /// only an Argon2id hash of the PIN.
    pub recovery_pin_dir: String,
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
    /// (Gate 3); must match the value registered with the issuer.
    pub oidc_redirect_uri: String,
    /// Whether local email+password authentication is enabled
    /// (`REVERIE_LOCAL_AUTH_ENABLED`, default `true`). Reverie is local-first out
    /// of the box (`docs/adr/0029-unified-identity-with-pluggable-authentication-providers.md`), so this
    /// defaults on. Setting it `false` without configuring OIDC is rejected at
    /// startup (Gate 3): at least one auth provider must remain usable or the
    /// instance locks everyone out.
    pub local_auth_enabled: bool,
    /// OIDC issuer URL for resource-server JWT validation
    /// (`REVERIE_RESOURCE_SERVER_ISSUER`). Distinct from `oidc_issuer_url`
    /// (interactive login): a resource-server JWT authenticates an API
    /// caller through the `CurrentUser` extractor and never establishes a
    /// session. Empty (default) disables JWT Bearer authentication
    /// entirely, so a Bearer credential that is not an `rvpat_` personal
    /// token then 401s at the extractor dispatch.
    ///
    /// Prerequisite: a JWT only ever authenticates an identity that already
    /// exists in Reverie. The token's `(iss, sub)` pair must already be
    /// linked to a user through a prior interactive OIDC login (that login
    /// is the only path that creates the link); a correctly signed token
    /// for an issuer/subject Reverie has never seen is rejected, not
    /// auto-provisioned. Interactive login must therefore be configured and
    /// exercised at least once for a given user before resource-server JWTs
    /// for that user will authenticate.
    ///
    /// THREAT: this is the trust anchor for API-caller signatures. The
    /// JWKS URL used to verify tokens derives from this issuer (via OIDC
    /// discovery, or the explicit `resource_server_jwks_url` override),
    /// never from a claim inside an incoming token. An operator pointing
    /// this at a malicious or compromised issuer can induce Reverie to
    /// trust attacker-controlled JWKS, enabling access-token forgery (the
    /// same operator-level threat documented on `oidc_issuer_url`). It carries
    /// the same transport constraints: `https`, no query, no fragment, checked
    /// at startup even when an explicit JWKS override is supplied.
    pub resource_server_issuer: String,
    /// Expected `aud` claim for resource-server JWT validation
    /// (`REVERIE_RESOURCE_SERVER_AUDIENCE`). Required together with
    /// `resource_server_issuer` (both set, or neither).
    ///
    /// Use a DEDICATED `IdP` client/audience for API access, distinct from
    /// the interactive-login `oidc_client_id`: if the same audience were
    /// accepted for both, an ID token minted for interactive login could
    /// be replayed as an API access token (cross-JWT confusion).
    ///
    /// Not for machine-to-machine callers: a service account, CI job, or
    /// script that can never complete an interactive login has no
    /// `(iss, sub)` link to resolve against and is rejected outright (see
    /// the prerequisite documented on `resource_server_issuer`). Mint it a
    /// personal access token instead (`rvpat_`-prefixed, `POST
    /// /api/v1/tokens` or the token-management screen): that credential is
    /// exactly what this config is not for.
    pub resource_server_audience: String,
    /// Explicit JWKS endpoint override for resource-server JWT validation
    /// (`REVERIE_RESOURCE_SERVER_JWKS_URL`). Empty (default) derives the
    /// JWKS URL from `resource_server_issuer` via OIDC discovery at
    /// startup. Set this only for an `IdP` that does not publish
    /// `.well-known/openid-configuration` (or exposes a JWKS endpoint
    /// outside its discovery document); the resolved URL is fixed for the
    /// process lifetime and is never read from an incoming token (RFC 8725
    /// §3.9/§3.10: `jku`/`x5u` header values are never followed).
    ///
    /// The resolved endpoint must use `https`, whether it came from this
    /// override or from discovery; startup fails otherwise.
    ///
    /// Fetches against it run over the shared OIDC transport, so they carry
    /// explicit connect and request timeouts and never follow redirects (the
    /// configured URL must be the final endpoint); resolved keys are cached
    /// in-process. A Bearer credential naming an unknown `kid` triggers a
    /// refetch, but fetches are at least 30 seconds apart and concurrent
    /// misses share one, so a flood of such credentials costs the `IdP` at
    /// most one request per window. A fetch that fails keeps the cached
    /// keys and still starts the window. A key the `IdP` rotates in is
    /// therefore rejected for up to 30 seconds after the last fetch.
    pub resource_server_jwks_url: String,
    /// Whether the `typ` header of a resource-server JWT must be `at+jwt` /
    /// `application/at+jwt` per RFC 9068 §4
    /// (`REVERIE_RESOURCE_SERVER_REQUIRE_AT_JWT`, default `false`).
    ///
    /// `true` (strict): only `at+jwt` / `application/at+jwt` is accepted.
    /// `false` (relaxed, default): a bare `JWT` or an absent `typ` is
    /// additionally accepted. In both modes any OTHER explicit `typ`
    /// (e.g. `logout+jwt`, `dpop+jwt`) is rejected, closing the replay of a
    /// same-key token declared for another purpose (Authentik, for one,
    /// signs logout tokens with the same key as access tokens). Set `true`
    /// for a conforming `IdP` (Authelia, Kanidm, Keycloak 26.2+ with the
    /// `at+jwt` toggle enabled); most self-hosted `IdPs` surveyed
    /// 2026-07-02 (Authentik, Zitadel, Casdoor, Hydra/fosite) emit a bare
    /// `typ: JWT` or omit it entirely, hence the relaxed default. Revisit
    /// once upstream `IdPs` converge (tracked upstream:
    /// goauthentik/authentik#22070).
    pub resource_server_require_at_jwt: bool,
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
    /// default `epub`). Seeds settings once; saved values win. An empty list suspends acquisition.
    #[serde(deserialize_with = "de_accepted_formats")]
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
    /// OPDS catalogue settings (mount enable, page size, realm,
    /// `public_url`).
    #[validate(nested)]
    pub opds: OpdsConfig,
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
        if !cfg.local_auth_enabled && !cfg.oidc_configured() {
            return Err(ConfigError::Invalid {
                var: "REVERIE_LOCAL_AUTH_ENABLED".into(),
                reason: "at least one auth provider must be enabled: set \
                         REVERIE_LOCAL_AUTH_ENABLED=true or configure OIDC (OIDC_ISSUER_URL)"
                    .into(),
            });
        }

        // Gate 4: resource-server fields required together. Deliberately
        // NOT folded into the interactive-provider guard above: JWTs
        // cannot establish a session, so a resource-server-only config
        // (no local auth, no OIDC login) still refuses to start there.
        if cfg.resource_server_configured() {
            for &(var, field) in RESOURCE_SERVER_FIELDS {
                if field(&cfg).trim().is_empty() {
                    return Err(ConfigError::MissingVar(var.into()));
                }
            }
        }

        // Gate 5: the operator contact is embedded verbatim in the outbound
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

    /// Whether resource-server JWT Bearer authentication is enabled,
    /// signalled by a non-blank issuer URL; mirrors [`Self::oidc_configured`].
    /// Gate 4 in [`Self::from_figment`] guarantees that when this is `true`,
    /// `resource_server_audience` is also present. Does NOT count toward the
    /// interactive-provider guard: a resource-server-only instance still
    /// requires local auth or OIDC login to be usable at all.
    pub fn resource_server_configured(&self) -> bool {
        !self.resource_server_issuer.trim().is_empty()
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
    if raw.is_empty() {
        return Ok(Vec::new());
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
            login_throttle_base_secs: 2,
            login_throttle_cap_secs: 900,
            password_min_length: 15,
            password_max_length: 256,
            password_min_zxcvbn_score: 2,
            password_breach_check_enabled: true,
            self_registration_enabled: false,
            recovery_pin_ttl_secs: 900,
            recovery_pin_dir: "/data/recovery-pins".into(),
            trusted_client_ip_header: None,
            // REQUIRED — empty sentinels.
            oidc_issuer_url: String::new(),
            oidc_client_id: String::new(),
            oidc_client_secret: String::new().into(),
            oidc_redirect_uri: String::new(),
            // Local-first default; Gate 3 guards the lock-out case.
            local_auth_enabled: true,
            // REQUIRED-TOGETHER — empty sentinels (Gate 4).
            resource_server_issuer: String::new(),
            resource_server_audience: String::new(),
            resource_server_jwks_url: String::new(),
            resource_server_require_at_jwt: false,
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
        }
    }
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

    #[test]
    fn library_storage_config_unavailable_absolute_root_parses() {
        let tmp = tempfile::tempdir().unwrap();
        let unavailable = tmp.path().join("not-provisioned");
        let config = cfg_from_owned(&with_overrides(&[(
            "REVERIE_LIBRARY_PATH",
            unavailable.to_str().unwrap(),
        )]))
        .unwrap();
        assert_eq!(config.library_path.as_path(), unavailable);
        assert!(!unavailable.exists());
        let schema = serde_json::to_value(schemars::schema_for!(Config)).unwrap();
        for (field, default) in [
            ("library_path", "/data/library"),
            ("ingestion_path", "/data/ingestion"),
        ] {
            assert_eq!(schema["properties"][field]["type"], "string");
            assert_eq!(schema["properties"][field]["default"], default);
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
        assert_eq!(
            config.database_url.expose_secret(),
            "postgres://test@localhost/reverie_dev"
        );
        assert_eq!(config.library_path.as_str(), "/data/library");
        assert_eq!(config.ingestion_path.as_str(), "/data/ingestion");
        assert_eq!(config.recovery_pin_dir, "/data/recovery-pins");
        // BASE_VARS exports DATABASE_URL_MIGRATION but leaves REVERIE_AUTO_MIGRATE
        // unset (off), so the DSN is intentionally NOT carried into Config.
        assert!(config.migration_database_url.is_none());
        assert!(!config.auto_migrate);
        assert_eq!(config.ingestion_database_url.expose_secret(), "");
        assert_eq!(config.accepted_formats, vec![ManifestationFormat::Epub]);
        assert!(config.cleanup_imported);
        assert!(!config.cleanup_duplicates);
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
            ("RUST_LOG", "debug"),
        ]);
        let config = cfg_from_owned(&vars).unwrap();
        assert_eq!(config.port, 8080);
        assert_eq!(
            config.database_url.expose_secret(),
            "postgres://custom@localhost/reverie_dev"
        );
        assert_eq!(config.library_path.as_str(), "/data/library");
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
        assert!(config.migration_database_url.is_none());
        assert!(!config.auto_migrate);
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
            config
                .migration_database_url
                .as_ref()
                .map(ExposeSecret::expose_secret),
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
            config
                .migration_database_url
                .as_ref()
                .map(ExposeSecret::expose_secret),
            Some("postgres://schema_owner@localhost/reverie_dev")
        );
    }

    #[test]
    fn from_env_custom_ingestion_url_and_accepted_formats() {
        let vars = with_overrides(&[
            (
                "DATABASE_URL_INGESTION",
                "postgres://ingestion@localhost/reverie_dev",
            ),
            ("REVERIE_ACCEPTED_FORMATS", " EPUB "),
        ]);
        let config = cfg_from_owned(&vars).unwrap();
        assert_eq!(
            config.ingestion_database_url.expose_secret(),
            "postgres://ingestion@localhost/reverie_dev"
        );
        assert_eq!(config.accepted_formats, vec![ManifestationFormat::Epub]);
    }

    #[test]
    fn from_env_rejects_unsupported_accepted_formats() {
        let vars = with_overrides(&[("REVERIE_ACCEPTED_FORMATS", "epub,djvu")]);
        let err = cfg_from_owned(&vars).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("djvu"), "expected djvu in error: {msg}");
        assert!(
            msg.contains("REVERIE_ACCEPTED_FORMATS"),
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
        // Strict form rejects the old "1"/"yes" spellings natively now
        // (EnvProvider parses "yes" to a `Str`, which a `bool` field
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
        assert!(cfg.migration_database_url.is_none());
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
    fn every_required_field_omitted_yields_missing_var() {
        // Drives Gate 2 across all of REQUIRED_FIELDS: omitting any one required
        // variable must surface MissingVar naming that exact variable. Proves the
        // name<->accessor pairing is correct for every entry, not just the two
        // with bespoke tests above, and guards against a future entry whose
        // accessor reads the wrong field.
        for &(var, _) in REQUIRED_FIELDS {
            let vars = without_keys(&[var]);
            let err = cfg_from_owned(&vars).unwrap_err();
            assert!(
                matches!(&err, ConfigError::MissingVar(v) if v == var),
                "omitting {var} should yield MissingVar({var}), got: {err:?}"
            );
        }
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
            let property = props.get(*field).expect("credential property exists");
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
    fn required_fields_are_known_and_mapped() {
        // REQUIRED_FIELDS is the shared source of required-ness for both the
        // Gate 2 startup check and the generated config reference. Every entry
        // must be a real ENV_MAP var name, or the reference would mark a
        // non-existent variable required.
        assert_ne!(REQUIRED_FIELDS, []);
        let mapped: std::collections::HashSet<&str> =
            ENV_MAP.iter().map(|(name, _)| *name).collect();
        for (var, _) in REQUIRED_FIELDS {
            assert!(
                mapped.contains(var),
                "required var {var} absent from ENV_MAP"
            );
        }
    }

    #[test]
    fn oidc_unconfigured_loads_as_local_only() {
        // OIDC fully unset is valid: local auth defaults on, so a
        // fresh instance is usable without an external IdP.
        let vars = without_keys(&[
            "OIDC_ISSUER_URL",
            "OIDC_CLIENT_ID",
            "OIDC_CLIENT_SECRET",
            "OIDC_REDIRECT_URI",
        ]);
        let config = cfg_from_owned(&vars).expect("local-only config loads");
        assert!(
            !config.oidc_configured(),
            "OIDC is disabled when its issuer is unset"
        );
        assert!(config.local_auth_enabled, "local auth defaults on");
    }

    #[test]
    fn local_disabled_without_oidc_is_rejected() {
        // Both providers off would lock everyone out: refuse boot.
        let mut vars = without_keys(&[
            "OIDC_ISSUER_URL",
            "OIDC_CLIENT_ID",
            "OIDC_CLIENT_SECRET",
            "OIDC_REDIRECT_URI",
        ]);
        vars.push(("REVERIE_LOCAL_AUTH_ENABLED".into(), "false".into()));
        let err = cfg_from_owned(&vars).unwrap_err();
        assert!(
            matches!(&err, ConfigError::Invalid { var, .. } if var == "REVERIE_LOCAL_AUTH_ENABLED"),
            "expected Invalid(REVERIE_LOCAL_AUTH_ENABLED), got: {err:?}"
        );
    }

    #[test]
    fn partial_oidc_block_is_rejected_when_configured() {
        // Once the issuer is set OIDC is "configured", so each remaining OIDC
        // field becomes required together (Gate 3): a half-configured block is a
        // MissingVar for the absent field, never a silent half-enable.
        for var in ["OIDC_CLIENT_ID", "OIDC_CLIENT_SECRET", "OIDC_REDIRECT_URI"] {
            let vars = without_keys(&[var]); // issuer stays set via BASE_VARS
            let err = cfg_from_owned(&vars).unwrap_err();
            assert!(
                matches!(&err, ConfigError::MissingVar(v) if v == var),
                "omitting {var} with OIDC configured should yield MissingVar({var}), got: {err:?}"
            );
        }
    }

    #[test]
    fn local_disabled_with_oidc_configured_is_ok() {
        // Disabling local auth is fine while OIDC remains usable (one provider).
        let vars = with_overrides(&[("REVERIE_LOCAL_AUTH_ENABLED", "false")]);
        let config = cfg_from_owned(&vars).expect("OIDC-only config loads");
        assert!(!config.local_auth_enabled);
        assert!(config.oidc_configured());
    }

    #[test]
    fn resource_server_unconfigured_by_default() {
        let config = cfg_from(BASE_VARS).unwrap();
        assert!(!config.resource_server_configured());
        assert!(!config.resource_server_require_at_jwt);
    }

    #[test]
    fn resource_server_both_fields_set_is_ok() {
        let vars = with_overrides(&[
            ("REVERIE_RESOURCE_SERVER_ISSUER", "https://idp.example.com"),
            ("REVERIE_RESOURCE_SERVER_AUDIENCE", "reverie-api"),
        ]);
        let config = cfg_from_owned(&vars).expect("resource-server config loads");
        assert!(config.resource_server_configured());
        assert_eq!(config.resource_server_audience, "reverie-api");
    }

    #[test]
    fn resource_server_audience_missing_is_rejected_when_issuer_set() {
        // Gate 4: once the issuer is set, resource-server JWT validation is
        // "configured", so audience becomes required together — mirrors
        // partial_oidc_block_is_rejected_when_configured (Gate 3).
        let vars = with_overrides(&[("REVERIE_RESOURCE_SERVER_ISSUER", "https://idp.example.com")]);
        let err = cfg_from_owned(&vars).unwrap_err();
        assert!(
            matches!(&err, ConfigError::MissingVar(v) if v == "REVERIE_RESOURCE_SERVER_AUDIENCE"),
            "expected MissingVar(REVERIE_RESOURCE_SERVER_AUDIENCE), got: {err:?}"
        );
    }

    #[test]
    fn resource_server_only_config_does_not_satisfy_interactive_provider_guard() {
        // A JWT resource server cannot establish a session, so it must not
        // count toward the "at least one auth provider" guard: local auth
        // off + no OIDC login + resource-server-only still refuses to start.
        let mut vars = without_keys(&[
            "OIDC_ISSUER_URL",
            "OIDC_CLIENT_ID",
            "OIDC_CLIENT_SECRET",
            "OIDC_REDIRECT_URI",
        ]);
        vars.push(("REVERIE_LOCAL_AUTH_ENABLED".into(), "false".into()));
        vars.push((
            "REVERIE_RESOURCE_SERVER_ISSUER".into(),
            "https://idp.example.com".into(),
        ));
        vars.push((
            "REVERIE_RESOURCE_SERVER_AUDIENCE".into(),
            "reverie-api".into(),
        ));
        let err = cfg_from_owned(&vars).unwrap_err();
        assert!(
            matches!(&err, ConfigError::Invalid { var, .. } if var == "REVERIE_LOCAL_AUTH_ENABLED"),
            "resource-server-only config must still trip the interactive-provider guard, got: {err:?}"
        );
    }

    #[test]
    fn resource_server_require_at_jwt_defaults_false_and_parses_strict_bool() {
        let vars = with_overrides(&[
            ("REVERIE_RESOURCE_SERVER_ISSUER", "https://idp.example.com"),
            ("REVERIE_RESOURCE_SERVER_AUDIENCE", "reverie-api"),
            ("REVERIE_RESOURCE_SERVER_REQUIRE_AT_JWT", "true"),
        ]);
        let config = cfg_from_owned(&vars).expect("resource-server config loads");
        assert!(config.resource_server_require_at_jwt);
    }

    #[test]
    fn from_env_rejects_operator_contact_that_is_not_a_header_value() {
        let vars = with_overrides(&[(
            "REVERIE_OPERATOR_CONTACT",
            "ops@example.com\r\nX-Injected: 1",
        )]);
        let err = cfg_from_owned(&vars).unwrap_err();
        assert!(
            err.to_string().contains("REVERIE_OPERATOR_CONTACT"),
            "expected var name in error: {err}"
        );
    }

    #[test]
    fn from_env_invalid_port() {
        let vars = with_overrides(&[("REVERIE_PORT", "not_a_number")]);
        let err = cfg_from_owned(&vars).unwrap_err();
        assert!(err.to_string().contains("REVERIE_PORT"));
    }

    #[test]
    fn from_env_invalid_cleanup_imported() {
        let vars = with_overrides(&[("REVERIE_CLEANUP_IMPORTED", "archive")]);
        let err = cfg_from_owned(&vars).unwrap_err();
        assert!(
            err.to_string().contains("REVERIE_CLEANUP_IMPORTED"),
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
    fn accepted_formats_comma_only_is_rejected() {
        // A non-empty value that splits to zero formats must error, not boot
        // with an empty priority list that silently skips every file.
        let vars = with_overrides(&[("REVERIE_ACCEPTED_FORMATS", ",")]);
        let err = cfg_from_owned(&vars).unwrap_err();
        assert!(
            err.to_string().contains("REVERIE_ACCEPTED_FORMATS"),
            "{err}"
        );
        assert!(err.to_string().contains("unsupported format"), "{err}");
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
        for (minimum, maximum) in [("256", "256"), ("300", "300"), ("24", "64")] {
            let vars = with_overrides(&[
                ("REVERIE_PASSWORD_MIN_LENGTH", minimum),
                ("REVERIE_PASSWORD_MAX_LENGTH", maximum),
            ]);
            assert!(cfg_from_owned(&vars).is_ok());
        }
    }

    #[test]
    fn ingestion_dsn_configured_role_passes_server_validation() {
        let vars = with_overrides(&[(
            "DATABASE_URL_INGESTION",
            "postgres://reverie_ingestion@localhost/reverie_dev",
        )]);
        let cfg = cfg_from_owned(&vars).unwrap();
        assert_eq!(
            cfg.ingestion_database_url.expose_secret(),
            "postgres://reverie_ingestion@localhost/reverie_dev"
        );
        cfg.validate_server().unwrap();
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
