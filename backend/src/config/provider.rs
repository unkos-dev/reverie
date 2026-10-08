//! The custom [`figment::Provider`] keystone ([`EnvProvider`]) and the
//! env-var → dotted-field-path registry ([`ENV_MAP`]).

use figment::{
    Metadata, Profile, Provider,
    value::{Dict, Map, Value},
};

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
pub const ENV_MAP: &[(&str, &str)] = &[
    // --- top-level flat fields ---
    ("DATABASE_URL", "database_url"),
    ("DATABASE_URL_MIGRATION", "migration_database_url"),
    ("DATABASE_URL_INGESTION", "ingestion_database_url"),
    ("OIDC_ISSUER_URL", "oidc_issuer_url"),
    ("OIDC_CLIENT_ID", "oidc_client_id"),
    ("OIDC_CLIENT_SECRET", "oidc_client_secret"),
    ("OIDC_REDIRECT_URI", "oidc_redirect_uri"),
    ("REVERIE_LOCAL_AUTH_ENABLED", "local_auth_enabled"),
    ("REVERIE_RESOURCE_SERVER_ISSUER", "resource_server_issuer"),
    (
        "REVERIE_RESOURCE_SERVER_AUDIENCE",
        "resource_server_audience",
    ),
    (
        "REVERIE_RESOURCE_SERVER_JWKS_URL",
        "resource_server_jwks_url",
    ),
    (
        "REVERIE_RESOURCE_SERVER_REQUIRE_AT_JWT",
        "resource_server_require_at_jwt",
    ),
    ("REVERIE_PORT", "port"),
    ("REVERIE_LIBRARY_PATH", "library_path"),
    ("REVERIE_INGESTION_PATH", "ingestion_path"),
    // Cascade resolved in `EnvProvider::data` (GOTCHA-CASCADE): both map to
    // `log_level`; `REVERIE_LOG_LEVEL` wins when both are set.
    ("REVERIE_LOG_LEVEL", "log_level"),
    ("RUST_LOG", "log_level"),
    ("REVERIE_DB_MAX_CONNECTIONS", "db_max_connections"),
    ("REVERIE_LOGIN_RATE_PER_MIN", "login_rate_per_min"),
    (
        "REVERIE_LOGIN_THROTTLE_BASE_SECS",
        "login_throttle_base_secs",
    ),
    ("REVERIE_LOGIN_THROTTLE_CAP_SECS", "login_throttle_cap_secs"),
    ("REVERIE_PASSWORD_MIN_LENGTH", "password_min_length"),
    ("REVERIE_PASSWORD_MAX_LENGTH", "password_max_length"),
    (
        "REVERIE_PASSWORD_MIN_ZXCVBN_SCORE",
        "password_min_zxcvbn_score",
    ),
    (
        "REVERIE_PASSWORD_BREACH_CHECK_ENABLED",
        "password_breach_check_enabled",
    ),
    (
        "REVERIE_SELF_REGISTRATION_ENABLED",
        "self_registration_enabled",
    ),
    ("REVERIE_RECOVERY_PIN_TTL_SECS", "recovery_pin_ttl_secs"),
    ("REVERIE_RECOVERY_PIN_DIR", "recovery_pin_dir"),
    (
        "REVERIE_TRUSTED_CLIENT_IP_HEADER",
        "trusted_client_ip_header",
    ),
    ("REVERIE_AUTO_MIGRATE", "auto_migrate"),
    ("REVERIE_ACCEPTED_FORMATS", "accepted_formats"),
    ("REVERIE_CLEANUP_IMPORTED", "cleanup_imported"),
    ("REVERIE_CLEANUP_DUPLICATES", "cleanup_duplicates"),
    ("REVERIE_GOOGLEBOOKS_API_KEY", "googlebooks_api_key"),
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
/// variables.
///
/// Maps each known env-var name to its dotted field path via
/// `ENV_MAP` and parses values into typed figment `Value`s. Empty storage roots
/// reach their checked parser; other empty values are unset. Unmapped vars are ignored.
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
///    ([`Self::from_process_env`]), the `getenv`/`setenv` data race
///    that makes `set_var` `unsafe`. `from_pairs` touches no process env.
///    Production
///    ([`Self::from_process_env`]) runs through the same code so tests exercise
///    the real parse path.
/// 2. **A frozen, irregular var→field contract.** The operator surface mixes
///    bare ecosystem names (`DATABASE_URL`, `OIDC_*`, `RUST_LOG`) with
///    `REVERIE_`-namespaced knobs, and several map to a nested path the var
///    name doesn't spell (`REVERIE_PUBLIC_URL` → `opds.public_url`). No
///    uniform separator rule derives that, so `ENV_MAP` is the explicit
///    registry — which also doubles as the introspectable var↔field source the
///    config-reference generator consumes.
///
/// Value parsing mirrors stock `Env` exactly (see [`Self::data`]); the custom
/// surface is only the two facts above. The pipeline is built in
/// [`crate::config::Config::from_figment`].
///
/// GOTCHA-SPLIT (secondary): the explicit map also sidesteps
/// `Env::split("_")`, which would wrongly split `snake_case` flat fields
/// (`db_max_connections` → `db.max.connections`).
pub struct EnvProvider<R = fn(&str) -> std::io::Result<String>> {
    pairs: Vec<(String, String)>,
    reader: R,
}

impl EnvProvider<fn(&str) -> std::io::Result<String>> {
    /// Collect all current process environment variables.
    #[must_use]
    pub fn from_process_env() -> Self {
        Self {
            pairs: std::env::vars().collect(),
            reader: |path| std::fs::read_to_string(path),
        }
    }

    /// Build from an explicit slice of `(key, value)` string pairs.
    /// Used in tests as an in-memory seam (no process-env mutation, no
    /// `figment::Jail` — parallel-safe, GOTCHA-TESTSEAM).
    #[must_use]
    pub fn from_pairs(pairs: &[(&str, &str)]) -> Self {
        Self {
            pairs: pairs
                .iter()
                .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
                .collect(),
            reader: |path| std::fs::read_to_string(path),
        }
    }
}

impl<R> EnvProvider<R> {
    /// Inject credential reads without changing process state.
    #[must_use]
    pub fn with_reader<T>(self, reader: T) -> EnvProvider<T> {
        EnvProvider {
            pairs: self.pairs,
            reader,
        }
    }
}

impl<R: Fn(&str) -> std::io::Result<String>> Provider for EnvProvider<R> {
    fn metadata(&self) -> Metadata {
        Metadata::named("EnvProvider")
    }

    fn data(&self) -> Result<Map<Profile, Dict>, figment::Error> {
        // Build a lookup map from ENV_MAP for O(1) access.
        let lookup: std::collections::HashMap<&str, &str> = ENV_MAP.iter().copied().collect();

        let mut dict = Dict::new();

        let auto_migrate = self
            .pairs
            .iter()
            .any(|(key, value)| key == "REVERIE_AUTO_MIGRATE" && value == "true");
        for (name, field) in credential_settings() {
            let resolved = resolve_credential(
                &self.pairs,
                name,
                field != "migration_database_url" || auto_migrate,
                &self.reader,
            )
            .map_err(|error| match error {
                super::ConfigError::Invalid { reason, .. } => {
                    figment::Error::from(reason).with_path(field)
                }
                error => figment::Error::from(error.to_string()).with_path(field),
            })?;
            if let Some(value) = resolved {
                let leaf = match value {
                    CredentialValue::Direct(value) => value
                        .parse::<Value>()
                        .unwrap_or_else(|never: std::convert::Infallible| match never {}),
                    CredentialValue::File(value) => Value::from(value),
                };
                if let Value::Dict(_, inner) = figment::util::nest(field, leaf) {
                    merge_dict(&mut dict, inner);
                }
            }
        }

        for (key, val) in &self.pairs {
            // Empty roots reach their checked type; other empty values are unset.
            if val.is_empty()
                && !matches!(
                    key.as_str(),
                    "REVERIE_LIBRARY_PATH" | "REVERIE_INGESTION_PATH" | "REVERIE_ACCEPTED_FORMATS"
                )
            {
                continue;
            }
            // Only process keys we know about; ignore PATH, HOME, etc.
            let Some(&dotted) = lookup.get(key.as_str()) else {
                continue;
            };
            if super::SECRET_FIELDS.contains(&dotted) {
                continue;
            }
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

pub fn credential_settings() -> impl Iterator<Item = (&'static str, &'static str)> {
    ENV_MAP
        .iter()
        .copied()
        .filter(|(_, field)| super::SECRET_FIELDS.contains(field))
}

pub enum CredentialValue {
    Direct(String),
    File(String),
}

impl CredentialValue {
    pub(crate) fn into_string(self) -> String {
        match self {
            Self::Direct(value) | Self::File(value) => value,
        }
    }
}

pub fn resolve_credential<R: Fn(&str) -> std::io::Result<String>>(
    pairs: &[(String, String)],
    name: &str,
    read_file: bool,
    reader: &R,
) -> Result<Option<CredentialValue>, super::ConfigError> {
    let alias = format!("{name}_FILE");
    let direct = pairs
        .iter()
        .find(|(key, value)| key == name && !value.is_empty());
    let file = pairs
        .iter()
        .find(|(key, value)| key == &alias && !value.is_empty());
    match (direct, file) {
        (Some(_), Some(_)) => Err(super::ConfigError::Invalid {
            var: name.into(),
            reason: format!("conflicting sources: {name} and {alias}"),
        }),
        (Some((_, value)), None) => Ok(Some(CredentialValue::Direct(value.clone()))),
        (None, Some((_, path))) if read_file => {
            // THREAT: Raw I/O errors may disclose credential paths or contents.
            let mut value = reader(path).map_err(|_| super::ConfigError::Invalid {
                var: name.into(),
                reason: "credential file could not be read as UTF-8".into(),
            })?;
            while value.ends_with('\n') {
                value.pop();
                if value.ends_with('\r') {
                    value.pop();
                }
            }
            Ok(Some(CredentialValue::File(value)))
        }
        _ => Ok(None),
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

    #[test]
    fn credential_file_resolver_presence_and_contents() {
        use std::cell::RefCell;
        let reads = RefCell::new(Vec::new());
        let reader = |path: &str| {
            reads.borrow_mut().push(path.to_owned());
            Ok("file-value".to_owned())
        };
        for (direct, file, expected) in [
            (None, None, None),
            (Some(""), Some(""), None),
            (Some("direct"), Some(""), Some("direct")),
            (Some(" "), Some(""), Some(" ")),
            (Some(""), Some("file-path"), Some("file-value")),
        ] {
            let mut pairs = Vec::new();
            if let Some(value) = direct {
                pairs.push(("DATABASE_URL".into(), value.into()));
            }
            if let Some(value) = file {
                pairs.push(("DATABASE_URL_FILE".into(), value.into()));
            }
            for _ in 0..2 {
                reads.borrow_mut().clear();
                let value = resolve_credential(&pairs, "DATABASE_URL", true, &reader)
                    .unwrap()
                    .map(CredentialValue::into_string);
                assert_eq!(value.as_deref(), expected);
                assert_eq!(
                    reads.borrow().as_slice(),
                    if file == Some("file-path") {
                        &["file-path"][..]
                    } else {
                        &[]
                    }
                );
                pairs.reverse();
            }
        }
        for (contents, expected) in [
            ("value\n", "value"),
            ("value\r\n", "value"),
            ("value", "value"),
            ("value\n\n", "value"),
            ("value\r\n\r\n", "value"),
            ("value\n\r\n\n", "value"),
            ("value\r", "value\r"),
            (" value ", " value "),
            ("one\ntwo", "one\ntwo"),
            ("\"quoted\"", "\"quoted\""),
            ("秘密", "秘密"),
            ("123", "123"),
            ("true", "true"),
            ("", ""),
            ("\n\r\n", ""),
        ] {
            let provider = EnvProvider::from_pairs(&[("DATABASE_URL_FILE", "content-path")])
                .with_reader(|path: &str| {
                    assert_eq!(path, "content-path");
                    Ok(contents.to_owned())
                });
            let data = provider.data().unwrap();
            let Some(Value::String(_, value)) = data[&Profile::Default].get("database_url") else {
                panic!("file content must remain a string");
            };
            assert_eq!(value, expected);
        }
    }

    #[test]
    fn credential_file_alias_wiring() {
        for (name, field) in credential_settings() {
            let alias = format!("{name}_FILE");
            let provider = EnvProvider::from_pairs(&[
                (alias.as_str(), "path-marker"),
                ("REVERIE_AUTO_MIGRATE", "true"),
            ])
            .with_reader(|path: &str| {
                assert_eq!(path, "path-marker");
                Ok("file-marker".into())
            });
            let data = provider.data().unwrap();
            assert!(
                matches!(data[&Profile::Default].get(field), Some(Value::String(_, value)) if value == "file-marker")
            );
        }
    }

    #[test]
    fn credential_file_safe_errors() {
        let name = "DATABASE_URL";
        let alias = "DATABASE_URL_FILE";
        for kind in [
            std::io::ErrorKind::NotFound,
            std::io::ErrorKind::PermissionDenied,
            std::io::ErrorKind::InvalidData,
        ] {
            let provider = EnvProvider::from_pairs(&[
                (alias, "path-marker"),
                ("REVERIE_AUTO_MIGRATE", "true"),
            ])
            .with_reader(move |_: &str| Err(std::io::Error::new(kind, "content-and-io-marker")));
            let error =
                super::super::Config::from_figment(&figment::Figment::from(provider)).unwrap_err();
            let mut diagnostic = format!("{error} {error:?}");
            let mut source = std::error::Error::source(&error);
            while let Some(error) = source {
                use std::fmt::Write as _;
                write!(diagnostic, "{error} {error:?}").unwrap();
                source = error.source();
            }
            assert!(diagnostic.contains(name));
            assert!(diagnostic.contains("credential file could not be read as UTF-8"));
            assert!(!diagnostic.contains("path-marker"));
            assert!(!diagnostic.contains("content-and-io-marker"));
        }
    }

    #[test]
    fn credential_file_existing_blank_and_migration_guards() {
        for contents in ["", "\n\r\n"] {
            let provider = EnvProvider::from_pairs(&[
                ("DATABASE_URL_FILE", "path"),
                ("REVERIE_OPDS_ENABLED", "false"),
            ])
            .with_reader(|_: &str| Ok(contents.into()));
            assert!(
                matches!(super::super::Config::from_figment(&figment::Figment::from(provider)),
                Err(super::super::ConfigError::MissingVar(name)) if name == "DATABASE_URL")
            );
            let provider = EnvProvider::from_pairs(&[
                ("DATABASE_URL", "app"),
                ("REVERIE_GOOGLEBOOKS_API_KEY_FILE", "path"),
                ("REVERIE_OPDS_ENABLED", "false"),
            ])
            .with_reader(|_: &str| Ok(contents.into()));
            let config =
                super::super::Config::from_figment(&figment::Figment::from(provider)).unwrap();
            assert!(config.googlebooks_api_key.is_some());
        }
        for (file, contents, succeeds) in [
            (None, "valid", false),
            (Some("path"), " \n", false),
            (Some("path"), "valid\n", true),
        ] {
            let mut pairs = vec![
                ("DATABASE_URL", "app"),
                ("REVERIE_OPDS_ENABLED", "false"),
                ("REVERIE_AUTO_MIGRATE", "true"),
            ];
            if let Some(path) = file {
                pairs.push(("DATABASE_URL_MIGRATION_FILE", path));
            }
            let provider =
                EnvProvider::from_pairs(&pairs).with_reader(|_: &str| Ok(contents.into()));
            assert_eq!(
                super::super::Config::from_figment(&figment::Figment::from(provider)).is_ok(),
                succeeds
            );
        }
        let provider = EnvProvider::from_pairs(&[
            ("DATABASE_URL", "app"),
            ("REVERIE_OPDS_ENABLED", "false"),
            ("REVERIE_AUTO_MIGRATE", "true"),
            ("DATABASE_URL_MIGRATION_FILE", "path-marker"),
        ])
        .with_reader(|_: &str| {
            Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "io-marker",
            ))
        });
        let error =
            super::super::Config::from_figment(&figment::Figment::from(provider)).unwrap_err();
        assert!(format!("{error}").contains("DATABASE_URL_MIGRATION"));
        assert!(!format!("{error:?}").contains("io-marker"));
    }

    #[test]
    fn credential_file_inactive_migration_never_reads() {
        let provider = EnvProvider::from_pairs(&[
            ("DATABASE_URL", "app"),
            ("REVERIE_OPDS_ENABLED", "false"),
            ("DATABASE_URL_MIGRATION_FILE", "path-marker"),
            ("REVERIE_BOOTSTRAP_PASSWORD_FILE", "unknown-path"),
        ])
        .with_reader(|_: &str| -> std::io::Result<String> {
            panic!("inactive migration must not read")
        });
        let config = super::super::Config::from_figment(&figment::Figment::from(provider)).unwrap();
        assert!(config.migration_database_url.is_none());
    }

    #[test]
    fn credential_file_conflicts_fail_without_reading() {
        for &(name, field) in ENV_MAP {
            if !super::super::SECRET_FIELDS.contains(&field) {
                continue;
            }
            let alias = format!("{name}_FILE");
            for pairs in [
                vec![
                    (name, "direct-marker"),
                    (alias.as_str(), "/unreadable-path-marker"),
                ],
                vec![
                    (alias.as_str(), "/unreadable-path-marker"),
                    (name, "direct-marker"),
                ],
            ] {
                let error = EnvProvider::from_pairs(&pairs)
                    .with_reader(|_: &str| -> std::io::Result<String> {
                        panic!("conflict must not read")
                    })
                    .data()
                    .unwrap_err();
                let diagnostic = format!("{error} {error:?}");
                assert!(diagnostic.contains(name));
                assert!(diagnostic.contains(&alias));
                assert!(!diagnostic.contains("direct-marker"));
                assert!(!diagnostic.contains("/unreadable-path-marker"));
            }
        }
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
    fn env_provider_from_process_env_reads_real_env() {
        // CARGO_PKG_NAME is set by cargo for every test run; it is unmapped in
        // ENV_MAP (ignored by `data`) but must be collected into the raw pairs.
        let p = EnvProvider::from_process_env();
        assert!(p.pairs.iter().any(|(k, _)| k == "CARGO_PKG_NAME"));
    }

    #[test]
    fn cascade_rust_log_wins_when_reverie_log_level_absent() {
        // GOTCHA-CASCADE leg: only RUST_LOG set → it provides log_level.
        let p = EnvProvider::from_pairs(&[("RUST_LOG", "warn")]);
        let data = p.data().unwrap();
        let dict = data.get(&Profile::Default).unwrap();
        let Some(Value::String(_, s)) = dict.get("log_level") else {
            panic!("log_level should be present");
        };
        assert_eq!(s.as_str(), "warn");
    }

    #[test]
    fn cascade_reverie_log_level_wins_over_rust_log() {
        // GOTCHA-CASCADE leg: both set → REVERIE_LOG_LEVEL wins regardless of
        // pair ordering (the skip reads the full pair list, not insertion order).
        let p = EnvProvider::from_pairs(&[("REVERIE_LOG_LEVEL", "error"), ("RUST_LOG", "debug")]);
        let data = p.data().unwrap();
        let dict = data.get(&Profile::Default).unwrap();
        let Some(Value::String(_, s)) = dict.get("log_level") else {
            panic!("log_level should be present");
        };
        assert_eq!(s.as_str(), "error");
    }

    #[test]
    fn unmapped_vars_dropped_and_numeric_coerced() {
        // Unmapped vars (PATH/HOME/…) never reach the Dict; a mapped numeric
        // string coerces to `Value::Num` (not a `Str` the u16 field would
        // reject as `InvalidType`).
        let p = EnvProvider::from_pairs(&[("PATH", "/usr/bin"), ("REVERIE_PORT", "3000")]);
        let data = p.data().unwrap();
        let dict = data.get(&Profile::Default).unwrap();
        assert!(dict.get("PATH").is_none(), "unmapped PATH must be dropped");
        assert!(matches!(dict.get("port"), Some(Value::Num(..))));
    }
}
