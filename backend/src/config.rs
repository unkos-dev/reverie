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
    pub openlibrary_base_url: String,
    pub googlebooks_base_url: String,
    pub googlebooks_api_key: Option<String>,
    pub hardcover_base_url: String,
    pub hardcover_api_token: Option<String>,
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

        let port = env::var("TOME_PORT")
            .unwrap_or_else(|_| "3000".into())
            .parse::<u16>()
            .map_err(|e| ConfigError::Invalid {
                var: "TOME_PORT".into(),
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

        let format_priority: Vec<String> = env::var("TOME_FORMAT_PRIORITY")
            .unwrap_or_else(|_| "epub,pdf,mobi,azw3,cbz,cbr".into())
            .split(',')
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty())
            .collect();

        for fmt in &format_priority {
            if !SUPPORTED_FORMATS.contains(&fmt.as_str()) {
                return Err(ConfigError::Invalid {
                    var: "TOME_FORMAT_PRIORITY".into(),
                    reason: format!(
                        "unsupported format '{fmt}'. Supported: {}",
                        SUPPORTED_FORMATS.join(", ")
                    ),
                });
            }
        }

        let cleanup_mode = match env::var("TOME_CLEANUP_MODE")
            .unwrap_or_else(|_| "all".into())
            .to_lowercase()
            .as_str()
        {
            "all" => CleanupMode::All,
            "ingested" => CleanupMode::Ingested,
            "none" => CleanupMode::None,
            other => {
                return Err(ConfigError::Invalid {
                    var: "TOME_CLEANUP_MODE".into(),
                    reason: format!("expected 'all', 'ingested', or 'none', got '{other}'"),
                });
            }
        };

        let enrichment = EnrichmentConfig::from_env()?;
        let cover = CoverConfig::from_env()?;

        let openlibrary_base_url = env::var("TOME_OPENLIBRARY_BASE_URL")
            .unwrap_or_else(|_| "https://openlibrary.org".into());
        let googlebooks_base_url = env::var("TOME_GOOGLEBOOKS_BASE_URL")
            .unwrap_or_else(|_| "https://www.googleapis.com/books/v1".into());
        let googlebooks_api_key = env::var("TOME_GOOGLEBOOKS_API_KEY")
            .ok()
            .filter(|s| !s.is_empty());
        let hardcover_base_url = env::var("TOME_HARDCOVER_BASE_URL")
            .unwrap_or_else(|_| "https://api.hardcover.app/v1/graphql".into());
        let hardcover_api_token = env::var("TOME_HARDCOVER_API_TOKEN")
            .ok()
            .filter(|s| !s.is_empty());

        Ok(Self {
            port,
            database_url,
            library_path: env::var("TOME_LIBRARY_PATH").unwrap_or_else(|_| "./library".into()),
            ingestion_path: env::var("TOME_INGESTION_PATH")
                .unwrap_or_else(|_| "./ingestion".into()),
            quarantine_path: env::var("TOME_QUARANTINE_PATH")
                .unwrap_or_else(|_| "./quarantine".into()),
            log_level: env::var("RUST_LOG").unwrap_or_else(|_| "info".into()),
            db_max_connections: env::var("TOME_DB_MAX_CONNECTIONS")
                .unwrap_or_else(|_| "10".into())
                .parse::<u32>()
                .map_err(|e| ConfigError::Invalid {
                    var: "TOME_DB_MAX_CONNECTIONS".into(),
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
            openlibrary_base_url,
            googlebooks_base_url,
            googlebooks_api_key,
            hardcover_base_url,
            hardcover_api_token,
        })
    }
}

impl EnrichmentConfig {
    fn from_env() -> Result<Self, ConfigError> {
        let enabled = parse_bool("TOME_ENRICHMENT_ENABLED", true)?;
        let concurrency = parse_u32("TOME_ENRICHMENT_CONCURRENCY", 2)?;
        if !(1..=10).contains(&concurrency) {
            return Err(ConfigError::Invalid {
                var: "TOME_ENRICHMENT_CONCURRENCY".into(),
                reason: format!("must be 1-10, got {concurrency}"),
            });
        }
        let poll_idle_secs = parse_u64("TOME_ENRICHMENT_POLL_IDLE_SECS", 30)?;
        let fetch_budget_secs = parse_u64("TOME_ENRICHMENT_FETCH_BUDGET_SECS", 15)?;
        let http_timeout_secs = parse_u64("TOME_ENRICHMENT_HTTP_TIMEOUT_SECS", 10)?;
        let max_attempts = parse_u32("TOME_ENRICHMENT_MAX_ATTEMPTS", 10)?;
        let cache_ttl_hit_days = parse_u32("TOME_ENRICHMENT_CACHE_TTL_HIT_DAYS", 30)?;
        let cache_ttl_miss_days = parse_u32("TOME_ENRICHMENT_CACHE_TTL_MISS_DAYS", 7)?;
        let cache_ttl_error_mins = parse_u32("TOME_ENRICHMENT_CACHE_TTL_ERROR_MINS", 15)?;

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

impl CoverConfig {
    fn from_env() -> Result<Self, ConfigError> {
        let max_bytes = parse_u64("TOME_COVER_MAX_BYTES", 10_485_760)?;
        let download_timeout_secs = parse_u64("TOME_COVER_DOWNLOAD_TIMEOUT_SECS", 30)?;
        let min_long_edge_px = parse_u32("TOME_COVER_MIN_LONG_EDGE_PX", 1000)?;
        let redirect_limit = parse_u32("TOME_COVER_REDIRECT_LIMIT", 3)? as usize;

        Ok(Self {
            max_bytes,
            download_timeout_secs,
            min_long_edge_px,
            redirect_limit,
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
                ("DATABASE_URL", "postgres://test@localhost/test"),
                ("OIDC_ISSUER_URL", "https://auth.example.com"),
                ("OIDC_CLIENT_ID", "test"),
                ("OIDC_CLIENT_SECRET", "secret"),
                ("OIDC_REDIRECT_URI", "http://localhost:3000/auth/callback"),
            ],
            &[
                "TOME_PORT",
                "TOME_LIBRARY_PATH",
                "TOME_INGESTION_PATH",
                "TOME_QUARANTINE_PATH",
                "DATABASE_URL_INGESTION",
                "TOME_FORMAT_PRIORITY",
                "TOME_CLEANUP_MODE",
                "TOME_ENRICHMENT_ENABLED",
                "TOME_ENRICHMENT_CONCURRENCY",
                "TOME_ENRICHMENT_POLL_IDLE_SECS",
                "TOME_ENRICHMENT_FETCH_BUDGET_SECS",
                "TOME_ENRICHMENT_HTTP_TIMEOUT_SECS",
                "TOME_ENRICHMENT_MAX_ATTEMPTS",
                "TOME_ENRICHMENT_CACHE_TTL_HIT_DAYS",
                "TOME_ENRICHMENT_CACHE_TTL_MISS_DAYS",
                "TOME_ENRICHMENT_CACHE_TTL_ERROR_MINS",
                "TOME_COVER_MAX_BYTES",
                "TOME_COVER_DOWNLOAD_TIMEOUT_SECS",
                "TOME_COVER_MIN_LONG_EDGE_PX",
                "TOME_COVER_REDIRECT_LIMIT",
                "TOME_OPENLIBRARY_BASE_URL",
                "TOME_GOOGLEBOOKS_BASE_URL",
                "TOME_GOOGLEBOOKS_API_KEY",
                "TOME_HARDCOVER_BASE_URL",
                "TOME_HARDCOVER_API_TOKEN",
            ],
            || {
                let config = Config::from_env().unwrap();
                assert_eq!(config.port, 3000);
                assert_eq!(config.database_url, "postgres://test@localhost/test");
                assert_eq!(config.library_path, "./library");
                assert_eq!(config.ingestion_path, "./ingestion");
                assert_eq!(config.quarantine_path, "./quarantine");
                // Falls back to DATABASE_URL when DATABASE_URL_INGESTION is unset
                assert_eq!(
                    config.ingestion_database_url,
                    "postgres://test@localhost/test"
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
                assert_eq!(config.openlibrary_base_url, "https://openlibrary.org");
                assert!(config.googlebooks_api_key.is_none());
                assert!(config.hardcover_api_token.is_none());
            },
        );
    }

    #[test]
    fn from_env_rejects_concurrency_out_of_range() {
        with_env(
            &[
                ("DATABASE_URL", "postgres://x@localhost/x"),
                ("OIDC_ISSUER_URL", "https://auth.example.com"),
                ("OIDC_CLIENT_ID", "test"),
                ("OIDC_CLIENT_SECRET", "secret"),
                ("OIDC_REDIRECT_URI", "http://localhost:3000/auth/callback"),
                ("TOME_ENRICHMENT_CONCURRENCY", "11"),
            ],
            &[],
            || {
                let err = Config::from_env().unwrap_err();
                assert!(err.to_string().contains("TOME_ENRICHMENT_CONCURRENCY"));
            },
        );
    }

    #[test]
    fn from_env_all_vars() {
        with_env(
            &[
                ("DATABASE_URL", "postgres://custom@localhost/db"),
                ("TOME_PORT", "8080"),
                ("TOME_LIBRARY_PATH", "/data/library"),
                ("TOME_INGESTION_PATH", "/data/ingestion"),
                ("TOME_QUARANTINE_PATH", "/data/quarantine"),
                ("RUST_LOG", "debug"),
                ("OIDC_ISSUER_URL", "https://auth.example.com"),
                ("OIDC_CLIENT_ID", "test"),
                ("OIDC_CLIENT_SECRET", "secret"),
                ("OIDC_REDIRECT_URI", "http://localhost:3000/auth/callback"),
            ],
            &[],
            || {
                let config = Config::from_env().unwrap();
                assert_eq!(config.port, 8080);
                assert_eq!(config.database_url, "postgres://custom@localhost/db");
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
                ("DATABASE_URL", "postgres://test@localhost/test"),
                (
                    "DATABASE_URL_INGESTION",
                    "postgres://ingestion@localhost/test",
                ),
                ("TOME_FORMAT_PRIORITY", "pdf, EPUB , mobi"),
                ("OIDC_ISSUER_URL", "https://auth.example.com"),
                ("OIDC_CLIENT_ID", "test"),
                ("OIDC_CLIENT_SECRET", "secret"),
                ("OIDC_REDIRECT_URI", "http://localhost:3000/auth/callback"),
            ],
            &[],
            || {
                let config = Config::from_env().unwrap();
                assert_eq!(
                    config.ingestion_database_url,
                    "postgres://ingestion@localhost/test"
                );
                assert_eq!(config.format_priority, vec!["pdf", "epub", "mobi"]);
            },
        );
    }

    #[test]
    fn from_env_rejects_unsupported_format_priority() {
        with_env(
            &[
                ("DATABASE_URL", "postgres://test@localhost/test"),
                ("TOME_FORMAT_PRIORITY", "epub,djvu"),
                ("OIDC_ISSUER_URL", "https://auth.example.com"),
                ("OIDC_CLIENT_ID", "test"),
                ("OIDC_CLIENT_SECRET", "secret"),
                ("OIDC_REDIRECT_URI", "http://localhost:3000/auth/callback"),
            ],
            &[],
            || {
                let err = Config::from_env().unwrap_err();
                let msg = err.to_string();
                assert!(msg.contains("djvu"), "expected djvu in error: {msg}");
                assert!(
                    msg.contains("TOME_FORMAT_PRIORITY"),
                    "expected var name in error: {msg}"
                );
            },
        );
    }

    #[test]
    fn from_env_invalid_port() {
        with_env(
            &[
                ("DATABASE_URL", "postgres://x@localhost/x"),
                ("TOME_PORT", "not_a_number"),
                ("OIDC_ISSUER_URL", "https://auth.example.com"),
                ("OIDC_CLIENT_ID", "test"),
                ("OIDC_CLIENT_SECRET", "secret"),
                ("OIDC_REDIRECT_URI", "http://localhost:3000/auth/callback"),
            ],
            &[],
            || {
                let err = Config::from_env().unwrap_err();
                assert!(err.to_string().contains("TOME_PORT"));
            },
        );
    }
}
