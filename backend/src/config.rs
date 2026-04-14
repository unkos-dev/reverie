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
}

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
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // Env vars are process-global, so serialize config tests.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

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
            &[("DATABASE_URL", "postgres://test@localhost/test")],
            &[
                "TOME_PORT",
                "TOME_LIBRARY_PATH",
                "TOME_INGESTION_PATH",
                "TOME_QUARANTINE_PATH",
            ],
            || {
                let config = Config::from_env().unwrap();
                assert_eq!(config.port, 3000);
                assert_eq!(config.database_url, "postgres://test@localhost/test");
                assert_eq!(config.library_path, "./library");
                assert_eq!(config.ingestion_path, "./ingestion");
                assert_eq!(config.quarantine_path, "./quarantine");
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
        with_env(&[], &["DATABASE_URL"], || {
            let err = Config::from_env().unwrap_err();
            assert!(err.to_string().contains("DATABASE_URL"));
        });
    }

    #[test]
    fn from_env_invalid_port() {
        with_env(
            &[
                ("DATABASE_URL", "postgres://x@localhost/x"),
                ("TOME_PORT", "not_a_number"),
            ],
            &[],
            || {
                let err = Config::from_env().unwrap_err();
                assert!(err.to_string().contains("TOME_PORT"));
            },
        );
    }
}
