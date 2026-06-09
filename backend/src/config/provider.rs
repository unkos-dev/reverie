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
/// [`crate::config::Config::from_figment`].
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
}
