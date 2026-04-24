use std::env;

#[derive(Debug, Clone)]
pub struct Config {
    pub port: u16,
    pub database_url: String,
    pub library_path: String,
    pub ingestion_path: String,
    pub quarantine_path: String,
    pub log_level: String,
    pub db_max_connections: u32,
    pub oidc_issuer_url: String,
    pub oidc_client_id: String,
    pub oidc_client_secret: String,
    pub oidc_redirect_uri: String,
    pub ingestion_database_url: String,
    pub format_priority: Vec<String>,
    pub cleanup_mode: CleanupMode,
    pub enrichment: EnrichmentConfig,
    pub cover: CoverConfig,
    pub writeback: WritebackConfig,
    pub opds: OpdsConfig,
    pub openlibrary_base_url: String,
    pub googlebooks_base_url: String,
    pub googlebooks_api_key: Option<String>,
    pub hardcover_base_url: String,
    pub hardcover_api_token: Option<String>,
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
    pub enabled: bool,
    pub page_size: u32,
    pub realm: String,
    pub public_url: Option<url::Url>,
}

#[derive(Debug, Clone)]
pub struct EnrichmentConfig {
    pub enabled: bool,
    pub concurrency: u32,
    pub poll_idle_secs: u64,
    pub fetch_budget_secs: u64,
    pub http_timeout_secs: u64,
    pub max_attempts: u32,
    pub cache_ttl_hit_days: u32,
    pub cache_ttl_miss_days: u32,
    pub cache_ttl_error_mins: u32,
}

#[derive(Debug, Clone)]
pub struct CoverConfig {
    pub max_bytes: u64,
    pub download_timeout_secs: u64,
    pub min_long_edge_px: u32,
    pub redirect_limit: usize,
}

#[derive(Debug, Clone)]
pub struct WritebackConfig {
    pub enabled: bool,
    pub concurrency: u32,
    pub poll_idle_secs: u64,
    pub max_attempts: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CleanupMode {
    /// Delete all files in the ingestion directory after a successful batch
    All,
    /// Delete only files that were actually ingested (selected by format priority)
    Ingested,
    /// Never delete source files — user handles cleanup manually
    None,
}

/// Formats supported by the manifestation_format DB enum.
pub const SUPPORTED_FORMATS: &[&str] = &["epub", "pdf", "mobi", "azw3", "cbz", "cbr"];

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("missing required environment variable: {0}")]
    MissingVar(String),
    #[error("invalid value for {var}: {reason}")]
    Invalid { var: String, reason: String },
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        dotenvy::dotenv().ok();

        let database_url =
            env::var("DATABASE_URL").map_err(|_| ConfigError::MissingVar("DATABASE_URL".into()))?;

        let port = env::var("REVERIE_PORT")
            .unwrap_or_else(|_| "3000".into())
            .parse::<u16>()
            .map_err(|e| ConfigError::Invalid {
                var: "REVERIE_PORT".into(),
                reason: e.to_string(),
            })?;

        let oidc_issuer_url = env::var("OIDC_ISSUER_URL")
            .map_err(|_| ConfigError::MissingVar("OIDC_ISSUER_URL".into()))?;
        let oidc_client_id = env::var("OIDC_CLIENT_ID")
            .map_err(|_| ConfigError::MissingVar("OIDC_CLIENT_ID".into()))?;
        let oidc_client_secret = env::var("OIDC_CLIENT_SECRET")
            .map_err(|_| ConfigError::MissingVar("OIDC_CLIENT_SECRET".into()))?;
        let oidc_redirect_uri = env::var("OIDC_REDIRECT_URI")
            .map_err(|_| ConfigError::MissingVar("OIDC_REDIRECT_URI".into()))?;

        let ingestion_database_url =
            env::var("DATABASE_URL_INGESTION").unwrap_or_else(|_| database_url.clone());

        let format_priority: Vec<String> = env::var("REVERIE_FORMAT_PRIORITY")
            .unwrap_or_else(|_| "epub,pdf,mobi,azw3,cbz,cbr".into())
            .split(',')
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty())
            .collect();

        for fmt in &format_priority {
            if !SUPPORTED_FORMATS.contains(&fmt.as_str()) {
                return Err(ConfigError::Invalid {
                    var: "REVERIE_FORMAT_PRIORITY".into(),
                    reason: format!(
                        "unsupported format '{fmt}'. Supported: {}",
                        SUPPORTED_FORMATS.join(", ")
                    ),
                });
            }
        }

        let cleanup_mode = match env::var("REVERIE_CLEANUP_MODE")
            .unwrap_or_else(|_| "all".into())
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

        let enrichment = EnrichmentConfig::from_env()?;
        let cover = CoverConfig::from_env()?;
        let writeback = WritebackConfig::from_env()?;
        let opds = OpdsConfig::from_env()?;

        let openlibrary_base_url = env::var("REVERIE_OPENLIBRARY_BASE_URL")
            .unwrap_or_else(|_| "https://openlibrary.org".into());
        let googlebooks_base_url = env::var("REVERIE_GOOGLEBOOKS_BASE_URL")
            .unwrap_or_else(|_| "https://www.googleapis.com/books/v1".into());
        let googlebooks_api_key = env::var("REVERIE_GOOGLEBOOKS_API_KEY")
            .ok()
            .filter(|s| !s.is_empty());
        let hardcover_base_url = env::var("REVERIE_HARDCOVER_BASE_URL")
            .unwrap_or_else(|_| "https://api.hardcover.app/v1/graphql".into());
        let hardcover_api_token = env::var("REVERIE_HARDCOVER_API_TOKEN")
            .ok()
            .filter(|s| !s.is_empty());
        let operator_contact = env::var("REVERIE_OPERATOR_CONTACT")
            .ok()
            .filter(|s| !s.is_empty());

        Ok(Self {
            port,
            database_url,
            library_path: env::var("REVERIE_LIBRARY_PATH").unwrap_or_else(|_| "./library".into()),
            ingestion_path: env::var("REVERIE_INGESTION_PATH")
                .unwrap_or_else(|_| "./ingestion".into()),
            quarantine_path: env::var("REVERIE_QUARANTINE_PATH")
                .unwrap_or_else(|_| "./quarantine".into()),
            log_level: env::var("RUST_LOG").unwrap_or_else(|_| "info".into()),
            db_max_connections: env::var("REVERIE_DB_MAX_CONNECTIONS")
                .unwrap_or_else(|_| "10".into())
                .parse::<u32>()
                .map_err(|e| ConfigError::Invalid {
                    var: "REVERIE_DB_MAX_CONNECTIONS".into(),
                    reason: e.to_string(),
                })?,
            oidc_issuer_url,
            oidc_client_id,
            oidc_client_secret,
            oidc_redirect_uri,
            ingestion_database_url,
            format_priority,
            cleanup_mode,
            enrichment,
            cover,
            writeback,
            opds,
            openlibrary_base_url,
            googlebooks_base_url,
            googlebooks_api_key,
            hardcover_base_url,
            hardcover_api_token,
            operator_contact,
        })
    }

    /// `User-Agent` string for outbound metadata API requests.  OpenLibrary
    /// grants identified requests a 3 req/s rate-limit tier (vs. 1 req/s
    /// anonymous) when a contact email or URL is present in the UA.
    pub fn user_agent(&self) -> String {
        match self.operator_contact.as_deref() {
            Some(contact) => format!("Reverie/{} ({contact})", env!("CARGO_PKG_VERSION")),
            None => format!("Reverie/{} (unidentified)", env!("CARGO_PKG_VERSION")),
        }
    }
}

impl EnrichmentConfig {
    fn from_env() -> Result<Self, ConfigError> {
        let enabled = parse_bool("REVERIE_ENRICHMENT_ENABLED", true)?;
        let concurrency = parse_u32("REVERIE_ENRICHMENT_CONCURRENCY", 2)?;
        if !(1..=10).contains(&concurrency) {
            return Err(ConfigError::Invalid {
                var: "REVERIE_ENRICHMENT_CONCURRENCY".into(),
                reason: format!("must be 1-10, got {concurrency}"),
            });
        }
        let poll_idle_secs = parse_u64("REVERIE_ENRICHMENT_POLL_IDLE_SECS", 30)?;
        let fetch_budget_secs = parse_u64("REVERIE_ENRICHMENT_FETCH_BUDGET_SECS", 15)?;
        let http_timeout_secs = parse_u64("REVERIE_ENRICHMENT_HTTP_TIMEOUT_SECS", 10)?;
        let max_attempts = parse_u32("REVERIE_ENRICHMENT_MAX_ATTEMPTS", 10)?;
        let cache_ttl_hit_days = parse_u32("REVERIE_ENRICHMENT_CACHE_TTL_HIT_DAYS", 30)?;
        let cache_ttl_miss_days = parse_u32("REVERIE_ENRICHMENT_CACHE_TTL_MISS_DAYS", 7)?;
        let cache_ttl_error_mins = parse_u32("REVERIE_ENRICHMENT_CACHE_TTL_ERROR_MINS", 15)?;

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
    fn from_env() -> Result<Self, ConfigError> {
        let enabled = parse_bool("REVERIE_WRITEBACK_ENABLED", true)?;
        let concurrency = parse_u32("REVERIE_WRITEBACK_CONCURRENCY", 2)?;
        if !(1..=10).contains(&concurrency) {
            return Err(ConfigError::Invalid {
                var: "REVERIE_WRITEBACK_CONCURRENCY".into(),
                reason: format!("must be 1-10, got {concurrency}"),
            });
        }
        let poll_idle_secs = parse_u64("REVERIE_WRITEBACK_POLL_IDLE_SECS", 5)?;
        let max_attempts = parse_u32("REVERIE_WRITEBACK_MAX_ATTEMPTS", 10)?;
        Ok(Self {
            enabled,
            concurrency,
            poll_idle_secs,
            max_attempts,
        })
    }
}

impl CoverConfig {
    fn from_env() -> Result<Self, ConfigError> {
        let max_bytes = parse_u64("REVERIE_COVER_MAX_BYTES", 10_485_760)?;
        let download_timeout_secs = parse_u64("REVERIE_COVER_DOWNLOAD_TIMEOUT_SECS", 30)?;
        let min_long_edge_px = parse_u32("REVERIE_COVER_MIN_LONG_EDGE_PX", 1000)?;
        let redirect_limit = parse_u32("REVERIE_COVER_REDIRECT_LIMIT", 3)? as usize;

        Ok(Self {
            max_bytes,
            download_timeout_secs,
            min_long_edge_px,
            redirect_limit,
        })
    }
}

impl OpdsConfig {
    fn from_env() -> Result<Self, ConfigError> {
        let enabled = parse_bool("REVERIE_OPDS_ENABLED", true)?;
        let page_size = parse_u32("REVERIE_OPDS_PAGE_SIZE", 50)?;
        if !(1..=500).contains(&page_size) {
            return Err(ConfigError::Invalid {
                var: "REVERIE_OPDS_PAGE_SIZE".into(),
                reason: format!("must be 1-500, got {page_size}"),
            });
        }
        let realm = env::var("REVERIE_OPDS_REALM").unwrap_or_else(|_| "Reverie OPDS".into());
        if realm.contains('"') {
            return Err(ConfigError::Invalid {
                var: "REVERIE_OPDS_REALM".into(),
                reason: "must not contain '\"'".into(),
            });
        }
        let public_url = match env::var("REVERIE_PUBLIC_URL")
            .ok()
            .filter(|s| !s.is_empty())
        {
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

fn parse_bool(var: &str, default: bool) -> Result<bool, ConfigError> {
    match env::var(var) {
        Ok(v) => match v.to_lowercase().as_str() {
            "true" | "1" | "yes" => Ok(true),
            "false" | "0" | "no" => Ok(false),
            _ => Err(ConfigError::Invalid {
                var: var.into(),
                reason: format!("expected boolean, got '{v}'"),
            }),
        },
        Err(_) => Ok(default),
    }
}

fn parse_u32(var: &str, default: u32) -> Result<u32, ConfigError> {
    match env::var(var) {
        Ok(v) => v.parse::<u32>().map_err(|e| ConfigError::Invalid {
            var: var.into(),
            reason: e.to_string(),
        }),
        Err(_) => Ok(default),
    }
}

fn parse_u64(var: &str, default: u64) -> Result<u64, ConfigError> {
    match env::var(var) {
        Ok(v) => v.parse::<u64>().map_err(|e| ConfigError::Invalid {
            var: var.into(),
            reason: e.to_string(),
        }),
        Err(_) => Ok(default),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Env vars are process-global. Use the crate-wide ENV_LOCK from test_support
    // so that db tests reading DATABASE_URL are also serialized against these tests.
    use crate::test_support::ENV_LOCK;

    #[allow(unsafe_code)]
    fn with_env<F: FnOnce()>(vars: &[(&str, &str)], clear: &[&str], f: F) {
        let _guard = ENV_LOCK.lock().unwrap();
        let saved: Vec<(String, Option<String>)> = vars
            .iter()
            .map(|(k, _)| (k.to_string(), env::var(k).ok()))
            .chain(clear.iter().map(|k| (k.to_string(), env::var(k).ok())))
            .collect();
        // SAFETY: tests using with_env are serialized by ENV_LOCK, so no
        // concurrent access to environment variables occurs.
        unsafe {
            for (k, v) in vars {
                env::set_var(k, v);
            }
            for k in clear {
                env::remove_var(k);
            }
        }
        f();
        // SAFETY: same ENV_LOCK held for the whole function — this block
        // restores the pre-test env snapshot captured above.
        unsafe {
            for (k, v) in saved {
                match v {
                    Some(val) => env::set_var(&k, val),
                    None => env::remove_var(&k),
                }
            }
        }
    }

    #[test]
    fn from_env_with_defaults() {
        with_env(
            &[
                ("DATABASE_URL", "postgres://test@localhost/reverie_dev"),
                ("OIDC_ISSUER_URL", "https://auth.example.com"),
                ("OIDC_CLIENT_ID", "test"),
                ("OIDC_CLIENT_SECRET", "secret"),
                ("OIDC_REDIRECT_URI", "http://localhost:3000/auth/callback"),
                // OPDS: default enabled=true requires PUBLIC_URL. Existing tests
                // don't care about OPDS, so disable it here.
                ("REVERIE_OPDS_ENABLED", "false"),
            ],
            &[
                "REVERIE_PORT",
                "REVERIE_LIBRARY_PATH",
                "REVERIE_INGESTION_PATH",
                "REVERIE_QUARANTINE_PATH",
                "DATABASE_URL_INGESTION",
                "REVERIE_FORMAT_PRIORITY",
                "REVERIE_CLEANUP_MODE",
                "REVERIE_ENRICHMENT_ENABLED",
                "REVERIE_ENRICHMENT_CONCURRENCY",
                "REVERIE_ENRICHMENT_POLL_IDLE_SECS",
                "REVERIE_ENRICHMENT_FETCH_BUDGET_SECS",
                "REVERIE_ENRICHMENT_HTTP_TIMEOUT_SECS",
                "REVERIE_ENRICHMENT_MAX_ATTEMPTS",
                "REVERIE_ENRICHMENT_CACHE_TTL_HIT_DAYS",
                "REVERIE_ENRICHMENT_CACHE_TTL_MISS_DAYS",
                "REVERIE_ENRICHMENT_CACHE_TTL_ERROR_MINS",
                "REVERIE_COVER_MAX_BYTES",
                "REVERIE_COVER_DOWNLOAD_TIMEOUT_SECS",
                "REVERIE_COVER_MIN_LONG_EDGE_PX",
                "REVERIE_COVER_REDIRECT_LIMIT",
                "REVERIE_WRITEBACK_ENABLED",
                "REVERIE_WRITEBACK_CONCURRENCY",
                "REVERIE_WRITEBACK_POLL_IDLE_SECS",
                "REVERIE_WRITEBACK_MAX_ATTEMPTS",
                "REVERIE_OPENLIBRARY_BASE_URL",
                "REVERIE_GOOGLEBOOKS_BASE_URL",
                "REVERIE_GOOGLEBOOKS_API_KEY",
                "REVERIE_HARDCOVER_BASE_URL",
                "REVERIE_HARDCOVER_API_TOKEN",
                "REVERIE_OPERATOR_CONTACT",
            ],
            || {
                let config = Config::from_env().unwrap();
                assert_eq!(config.port, 3000);
                assert_eq!(config.database_url, "postgres://test@localhost/reverie_dev");
                assert_eq!(config.library_path, "./library");
                assert_eq!(config.ingestion_path, "./ingestion");
                assert_eq!(config.quarantine_path, "./quarantine");
                // Falls back to DATABASE_URL when DATABASE_URL_INGESTION is unset
                assert_eq!(
                    config.ingestion_database_url,
                    "postgres://test@localhost/reverie_dev"
                );
                assert_eq!(
                    config.format_priority,
                    vec!["epub", "pdf", "mobi", "azw3", "cbz", "cbr"]
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
            },
        );
    }

    #[test]
    fn user_agent_without_contact_reports_unidentified() {
        with_env(
            &[
                ("DATABASE_URL", "postgres://test@localhost/reverie_dev"),
                ("OIDC_ISSUER_URL", "https://auth.example.com"),
                ("OIDC_CLIENT_ID", "test"),
                ("OIDC_CLIENT_SECRET", "secret"),
                ("OIDC_REDIRECT_URI", "http://localhost:3000/auth/callback"),
                // OPDS: default enabled=true requires PUBLIC_URL. Existing tests
                // don't care about OPDS, so disable it here.
                ("REVERIE_OPDS_ENABLED", "false"),
            ],
            &["REVERIE_OPERATOR_CONTACT"],
            || {
                let config = Config::from_env().unwrap();
                let ua = config.user_agent();
                assert!(ua.starts_with("Reverie/"), "missing Reverie/ prefix: {ua}");
                assert!(ua.ends_with("(unidentified)"), "unexpected suffix: {ua}");
            },
        );
    }

    #[test]
    fn user_agent_with_contact_embeds_identifier() {
        with_env(
            &[
                ("DATABASE_URL", "postgres://test@localhost/reverie_dev"),
                ("OIDC_ISSUER_URL", "https://auth.example.com"),
                ("OIDC_CLIENT_ID", "test"),
                ("OIDC_CLIENT_SECRET", "secret"),
                ("OIDC_REDIRECT_URI", "http://localhost:3000/auth/callback"),
                ("REVERIE_OPERATOR_CONTACT", "ops@example.com"),
                ("REVERIE_OPDS_ENABLED", "false"),
            ],
            &[],
            || {
                let config = Config::from_env().unwrap();
                assert_eq!(config.operator_contact.as_deref(), Some("ops@example.com"));
                let ua = config.user_agent();
                assert!(ua.contains("(ops@example.com)"), "missing contact: {ua}");
                assert!(ua.starts_with("Reverie/"), "missing Reverie/ prefix: {ua}");
            },
        );
    }

    #[test]
    fn from_env_rejects_concurrency_out_of_range() {
        with_env(
            &[
                ("DATABASE_URL", "postgres://x@localhost/reverie_dev"),
                ("OIDC_ISSUER_URL", "https://auth.example.com"),
                ("OIDC_CLIENT_ID", "test"),
                ("OIDC_CLIENT_SECRET", "secret"),
                ("OIDC_REDIRECT_URI", "http://localhost:3000/auth/callback"),
                ("REVERIE_ENRICHMENT_CONCURRENCY", "11"),
            ],
            &[],
            || {
                let err = Config::from_env().unwrap_err();
                assert!(err.to_string().contains("REVERIE_ENRICHMENT_CONCURRENCY"));
            },
        );
    }

    #[test]
    fn from_env_all_vars() {
        with_env(
            &[
                ("DATABASE_URL", "postgres://custom@localhost/reverie_dev"),
                ("REVERIE_PORT", "8080"),
                ("REVERIE_LIBRARY_PATH", "/data/library"),
                ("REVERIE_INGESTION_PATH", "/data/ingestion"),
                ("REVERIE_QUARANTINE_PATH", "/data/quarantine"),
                ("RUST_LOG", "debug"),
                ("OIDC_ISSUER_URL", "https://auth.example.com"),
                ("OIDC_CLIENT_ID", "test"),
                ("OIDC_CLIENT_SECRET", "secret"),
                ("OIDC_REDIRECT_URI", "http://localhost:3000/auth/callback"),
                // OPDS: default enabled=true requires PUBLIC_URL. Existing tests
                // don't care about OPDS, so disable it here.
                ("REVERIE_OPDS_ENABLED", "false"),
            ],
            &[],
            || {
                let config = Config::from_env().unwrap();
                assert_eq!(config.port, 8080);
                assert_eq!(
                    config.database_url,
                    "postgres://custom@localhost/reverie_dev"
                );
                assert_eq!(config.library_path, "/data/library");
                assert_eq!(config.log_level, "debug");
            },
        );
    }

    #[test]
    fn from_env_missing_database_url() {
        with_env(
            &[
                ("OIDC_ISSUER_URL", "https://auth.example.com"),
                ("OIDC_CLIENT_ID", "test"),
                ("OIDC_CLIENT_SECRET", "secret"),
                ("OIDC_REDIRECT_URI", "http://localhost:3000/auth/callback"),
                // OPDS: default enabled=true requires PUBLIC_URL. Existing tests
                // don't care about OPDS, so disable it here.
                ("REVERIE_OPDS_ENABLED", "false"),
            ],
            &["DATABASE_URL"],
            || {
                let err = Config::from_env().unwrap_err();
                assert!(err.to_string().contains("DATABASE_URL"));
            },
        );
    }

    #[test]
    fn from_env_custom_ingestion_url_and_format_priority() {
        with_env(
            &[
                ("DATABASE_URL", "postgres://test@localhost/reverie_dev"),
                (
                    "DATABASE_URL_INGESTION",
                    "postgres://ingestion@localhost/reverie_dev",
                ),
                ("REVERIE_FORMAT_PRIORITY", "pdf, EPUB , mobi"),
                ("OIDC_ISSUER_URL", "https://auth.example.com"),
                ("OIDC_CLIENT_ID", "test"),
                ("OIDC_CLIENT_SECRET", "secret"),
                ("OIDC_REDIRECT_URI", "http://localhost:3000/auth/callback"),
                // OPDS: default enabled=true requires PUBLIC_URL. Existing tests
                // don't care about OPDS, so disable it here.
                ("REVERIE_OPDS_ENABLED", "false"),
            ],
            &[],
            || {
                let config = Config::from_env().unwrap();
                assert_eq!(
                    config.ingestion_database_url,
                    "postgres://ingestion@localhost/reverie_dev"
                );
                assert_eq!(config.format_priority, vec!["pdf", "epub", "mobi"]);
            },
        );
    }

    #[test]
    fn from_env_rejects_unsupported_format_priority() {
        with_env(
            &[
                ("DATABASE_URL", "postgres://test@localhost/reverie_dev"),
                ("REVERIE_FORMAT_PRIORITY", "epub,djvu"),
                ("OIDC_ISSUER_URL", "https://auth.example.com"),
                ("OIDC_CLIENT_ID", "test"),
                ("OIDC_CLIENT_SECRET", "secret"),
                ("OIDC_REDIRECT_URI", "http://localhost:3000/auth/callback"),
                // OPDS: default enabled=true requires PUBLIC_URL. Existing tests
                // don't care about OPDS, so disable it here.
                ("REVERIE_OPDS_ENABLED", "false"),
            ],
            &[],
            || {
                let err = Config::from_env().unwrap_err();
                let msg = err.to_string();
                assert!(msg.contains("djvu"), "expected djvu in error: {msg}");
                assert!(
                    msg.contains("REVERIE_FORMAT_PRIORITY"),
                    "expected var name in error: {msg}"
                );
            },
        );
    }

    #[test]
    fn opds_enabled_without_public_url_errors() {
        with_env(
            &[
                ("DATABASE_URL", "postgres://test@localhost/reverie_dev"),
                ("OIDC_ISSUER_URL", "https://auth.example.com"),
                ("OIDC_CLIENT_ID", "test"),
                ("OIDC_CLIENT_SECRET", "secret"),
                ("OIDC_REDIRECT_URI", "http://localhost:3000/auth/callback"),
                ("REVERIE_OPDS_ENABLED", "true"),
            ],
            &["REVERIE_PUBLIC_URL"],
            || {
                let err = Config::from_env().unwrap_err();
                let msg = err.to_string();
                assert!(
                    msg.contains("REVERIE_PUBLIC_URL"),
                    "unexpected error: {msg}"
                );
            },
        );
    }

    #[test]
    fn opds_page_size_out_of_range_errors() {
        for bad in ["0", "501"] {
            with_env(
                &[
                    ("DATABASE_URL", "postgres://test@localhost/reverie_dev"),
                    ("OIDC_ISSUER_URL", "https://auth.example.com"),
                    ("OIDC_CLIENT_ID", "test"),
                    ("OIDC_CLIENT_SECRET", "secret"),
                    ("OIDC_REDIRECT_URI", "http://localhost:3000/auth/callback"),
                    ("REVERIE_OPDS_ENABLED", "false"),
                    ("REVERIE_OPDS_PAGE_SIZE", bad),
                ],
                &[],
                || {
                    let err = Config::from_env().unwrap_err();
                    let msg = err.to_string();
                    assert!(
                        msg.contains("REVERIE_OPDS_PAGE_SIZE"),
                        "page_size={bad} did not surface var name: {msg}"
                    );
                },
            );
        }
    }

    #[test]
    fn opds_realm_with_double_quote_errors() {
        with_env(
            &[
                ("DATABASE_URL", "postgres://test@localhost/reverie_dev"),
                ("OIDC_ISSUER_URL", "https://auth.example.com"),
                ("OIDC_CLIENT_ID", "test"),
                ("OIDC_CLIENT_SECRET", "secret"),
                ("OIDC_REDIRECT_URI", "http://localhost:3000/auth/callback"),
                ("REVERIE_OPDS_ENABLED", "false"),
                ("REVERIE_OPDS_REALM", "bad\"quote"),
            ],
            &[],
            || {
                let err = Config::from_env().unwrap_err();
                let msg = err.to_string();
                assert!(
                    msg.contains("REVERIE_OPDS_REALM"),
                    "expected realm error: {msg}"
                );
            },
        );
    }

    #[test]
    fn opds_enabled_with_valid_public_url_parses() {
        with_env(
            &[
                ("DATABASE_URL", "postgres://test@localhost/reverie_dev"),
                ("OIDC_ISSUER_URL", "https://auth.example.com"),
                ("OIDC_CLIENT_ID", "test"),
                ("OIDC_CLIENT_SECRET", "secret"),
                ("OIDC_REDIRECT_URI", "http://localhost:3000/auth/callback"),
                ("REVERIE_OPDS_ENABLED", "true"),
                ("REVERIE_PUBLIC_URL", "https://reverie.example.com/"),
            ],
            &[],
            || {
                let config = Config::from_env().unwrap();
                assert!(config.opds.enabled);
                assert_eq!(
                    config.opds.public_url.as_ref().map(|u| u.as_str()),
                    Some("https://reverie.example.com/")
                );
            },
        );
    }

    #[test]
    fn from_env_invalid_port() {
        with_env(
            &[
                ("DATABASE_URL", "postgres://x@localhost/reverie_dev"),
                ("REVERIE_PORT", "not_a_number"),
                ("OIDC_ISSUER_URL", "https://auth.example.com"),
                ("OIDC_CLIENT_ID", "test"),
                ("OIDC_CLIENT_SECRET", "secret"),
                ("OIDC_REDIRECT_URI", "http://localhost:3000/auth/callback"),
                // OPDS: default enabled=true requires PUBLIC_URL. Existing tests
                // don't care about OPDS, so disable it here.
                ("REVERIE_OPDS_ENABLED", "false"),
            ],
            &[],
            || {
                let err = Config::from_env().unwrap_err();
                assert!(err.to_string().contains("REVERIE_PORT"));
            },
        );
    }
}
