//! Reverie API server — library crate.
//!
//! Hosts the HTTP service, authentication, ingestion pipeline, metadata
//! enrichment, and OPDS catalogue. The accompanying `reverie-api` binary is a
//! thin entry that calls [`run`] under a `#[tokio::main]` runtime; the split
//! exists so that `missing_docs` and clippy pedantic doc lints fire on
//! externally-reachable items (a binary-only crate has no external API and
//! the lints are silent — see `adr/2026-05-08-tiered-comment-policy.md`
//! Phase 0).
//!
//! Embedders mounting Reverie under their own server may use
//! [`build_router`] directly with a fully-initialised [`state::AppState`].

#![deny(missing_docs)]
#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::print_stdout,
        clippy::print_stderr,
    )
)]

pub mod auth;
pub mod config;
pub mod db;
pub mod error;
pub mod models;
pub mod routes;
pub mod security;
pub mod services;
pub mod state;
#[cfg(test)]
pub(crate) mod test_support;

use anyhow::Context as _;
use axum::Router;
use tower_sessions::{Expiry, SessionManagerLayer};
use tracing_subscriber::EnvFilter;

use crate::auth::store::PostgresStore;
use crate::config::Config;
use crate::state::AppState;

/// Build the production Axum router with a Postgres-backed session store.
///
/// Wires the unconditional reserved-prefix routes (`/api`, `/auth`,
/// `/health`), the OPDS catalogue at `/opds` when `config.opds.enabled`
/// is set, the SPA assets fallback when `frontend_dist_path` is
/// configured, the CSP middleware stack, and the auth/session layers.
/// Returned router is ready for `axum::serve`.
///
/// Production callers should reach this through [`run`]. Embedders mounting
/// Reverie inside another Axum service can call it directly, supplying a
/// fully-initialised [`AppState`] (DB pools + finalised CSP headers on
/// `state.config.security`).
pub fn build_router(state: AppState) -> Router {
    // Sessions persist to Postgres so a backend restart doesn't log every
    // user out. The first-party `auth::store::PostgresStore` targets
    // `tower_sessions.session` (provisioned by the consolidated initial-schema
    // migration). Expired-session cleanup runs as a scheduled sweep in `run`
    // (see `services::session_sweep`), driving `ExpiredDeletion::delete_expired`
    // hourly under the shared cancellation token. Embedders calling this
    // function directly are responsible for their own reaping.
    let session_store = PostgresStore::new(state.pool.clone());
    build_router_with_session_store(state, session_store)
}

/// Same as [`build_router`] but with a caller-provided session store.
///
/// Used by **in-crate integration tests** (under `src/**/tests` modules
/// gated on `#[cfg(test)]`) to inject a `tower_sessions::MemoryStore` so
/// the test harness can read server-written session state — e.g. the
/// OIDC `nonce` set by `/auth/login` that the callback test needs to
/// embed in a matching mock-issued ID token. External-crate tests under
/// `backend/tests/` cannot reach this function; intentional, since the
/// shared-store seam is only required by tests that exercise routing
/// internals (which already need crate-private access for fixtures).
/// Production builds use `PostgresStore` via [`build_router`].
pub(crate) fn build_router_with_session_store<S>(state: AppState, session_store: S) -> Router
where
    S: tower_sessions::SessionStore + Clone,
{
    // `Secure` is gated on `behind_https`, mirroring the HSTS gate in
    // `security::headers`. The browser evaluates `Secure` against its own leg
    // to the edge (HTTPS when a TLS-terminating proxy fronts us — the
    // proxy→backend hop being plain HTTP is irrelevant), so a TLS-fronted
    // deploy (`REVERIE_BEHIND_HTTPS=true`) gets `Secure=true` and an HTTP-only
    // LAN deploy gets `Secure=false`. Cookies are unsigned — session security
    // relies on the cryptographic randomness of tower-sessions session IDs
    // (CSPRNG `i128` via the `rand` crate) regardless of this flag.
    let session_layer = SessionManagerLayer::new(session_store)
        .with_http_only(true)
        .with_secure(state.config.security.behind_https)
        .with_same_site(tower_sessions::cookie::SameSite::Lax)
        .with_expiry(Expiry::OnInactivity(time::Duration::hours(24)))
        // Save on every request so `OnInactivity` actually tracks inactivity:
        // the expiry is recomputed from the last save, and reads (e.g.
        // `/auth/me`) don't otherwise save, so without this the 24h window would
        // count from the last login rather than the last request. Costs one
        // session-row UPDATE per request that carries a session; the sliding
        // window is worth it. A flushed session (force-logout) is not resurrected
        // — tower-sessions skips the save for a deleted session.
        .with_always_save(true);

    // Reserved-prefix routes — /api, /auth, /health, /opds. API CSP layered on
    // matched responses; unmatched paths flow into the composite fallback
    // below which attaches API CSP manually for reserved-prefix 404s.
    let mut api_like = Router::new()
        .merge(routes::health::router())
        .merge(routes::auth::router())
        .merge(routes::tokens::router())
        .merge(routes::ingestion::router())
        .merge(routes::enrichment::router())
        .merge(routes::metadata::router())
        .merge(routes::library::router())
        .merge(routes::dashboard::router())
        .merge(routes::series::router())
        .merge(routes::shelves::router())
        .merge(routes::users::router())
        .merge(routes::settings::router())
        // /api/books/:id/cover{,/thumb} — always mounted (Step 10 consumes it
        // with a session cookie regardless of OPDS availability).
        .merge(routes::opds::covers_router());
    if let Some(opds) = routes::opds::router_enabled(&state.config.opds) {
        api_like = api_like.merge(opds);
    }
    let api_like = api_like.layer(axum::middleware::from_fn_with_state(
        state.clone(),
        security::headers::api_csp_layer,
    ));

    // SPA assets router (None in API-only dev — Vite owns the HTML).
    let spa =
        routes::spa::router_enabled(state.config.security.frontend_dist_path.as_deref()).map(|r| {
            r.layer(axum::middleware::from_fn_with_state(
                state.clone(),
                security::headers::html_csp_layer,
            ))
        });

    let mut composite = api_like;
    if let Some(spa) = spa {
        composite = composite.merge(spa);
    }

    composite
        // Single composite fallback — Axum 0.8 rejects merging two routers
        // that both carry a fallback, so the SPA router has none and this
        // handler path-dispatches JSON-404 vs SPA index.html itself.
        .fallback(security::headers::composite_fallback)
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            security::headers::security_headers,
        ))
        // Capture the request path into a task-local so
        // `AppError::into_response` (called anywhere in the handler
        // stack below) can emit a populated RFC 7807 `instance`
        // field. Applies to all routes including the composite
        // fallback so 404s on `/api/*` carry the instance too.
        .layer(axum::middleware::from_fn(
            error::instance::problem_instance_layer,
        ))
        .layer(session_layer)
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .with_state(state)
}

fn resolve_log_filter(configured_level: &str) -> (EnvFilter, Option<String>) {
    // Single source of truth: `configured_level` already encodes the
    // REVERIE_LOG_LEVEL > RUST_LOG > "info" cascade resolved by
    // Config::from_source. Re-reading RUST_LOG here would invert the
    // precedence (ecosystem default beats operator namespace) and
    // contradict the documented behaviour on the Config::log_level field.
    match configured_level.parse::<EnvFilter>() {
        Ok(f) => (f, None),
        Err(e) => (
            EnvFilter::new("info"),
            Some(format!("{configured_level:?}: {e}")),
        ),
    }
}

/// Boot and run the Reverie API server until shutdown.
///
/// Loads configuration from the environment, finalises CSP headers, opens
/// the primary and ingestion DB pools, initialises the OIDC client, builds
/// the router, spawns the ingestion watcher, the enrichment queue, and the
/// writeback worker (the last on a dedicated `reverie_app` pool that sets
/// `app.system_context = 'writeback'` per-connection), then binds the
/// listener and serves until SIGINT/SIGTERM. Returns once graceful
/// shutdown completes.
///
/// Caller is responsible for installing a tokio runtime — typically by
/// being invoked from a `#[tokio::main]` `async fn main` in the binary
/// crate. Failures during startup return an error rather than panicking;
/// callers should surface those to operators with a non-zero exit.
///
/// # Errors
///
/// Returns an error when:
/// - configuration cannot be loaded from the environment
///   (missing or invalid env var);
/// - the API or HTML CSP string fails to parse as a valid HTTP header
///   value (a programming-invariant failure that beats silently dropping
///   the header on every response);
/// - frontend dist validation fails when `frontend_dist_path` is set
///   (rebuild the frontend with `vite build`);
/// - the global tracing subscriber cannot be installed (typically because
///   the host process already installed one — embedders should install
///   their subscriber before calling `run`);
/// - any of the primary, ingestion, or writeback DB pools cannot connect;
/// - OIDC discovery against the configured issuer fails;
/// - the TCP listener cannot bind to the configured port;
/// - `axum::serve` returns an error during the serving loop.
#[allow(
    clippy::too_many_lines,
    reason = "Phase 0 of the comment-policy rollout (UNK-191) is structural-only: this body was verbatim moved from the pre-split `main.rs` and lightly extended (3 lines for try_init error propagation + the `# Errors` docstring section). UNK-193 (typed `StartupError`) will reshape startup error handling and is the natural place to extract phase helpers (`setup_tracing`, `init_csp_headers`, `spawn_workers`)."
)]
pub async fn run() -> anyhow::Result<()> {
    let mut config =
        Config::from_env().map_err(|e| anyhow::anyhow!("invalid configuration: {e}"))?;

    // Finalise CSP headers once at startup. API CSP has no dynamic inputs
    // besides the optional report endpoint. HTML CSP consumes the script-src
    // hash list produced by `vite build`'s csp-hash plugin and read back from
    // the committed sidecar. Failing at startup beats silently dropping
    // the security header on every response.
    let api_csp = security::csp::build_api_csp(config.security.csp_report_endpoint.as_ref());
    config.security.csp_api_header =
        Some(axum::http::HeaderValue::from_str(&api_csp).map_err(|e| {
            anyhow::anyhow!("API CSP is not a valid HTTP header value ({e}): {api_csp:?}")
        })?);
    if let Some(dist_path) = config.security.frontend_dist_path.clone() {
        let validated =
            security::dist_validation::validate_frontend_dist(&dist_path).map_err(|e| {
                anyhow::anyhow!(
                    "frontend dist validation failed — rebuild frontend (vite build): {e}"
                )
            })?;
        let html_csp = security::csp::build_html_csp(
            &validated.script_src_hashes,
            config.security.csp_report_endpoint.as_ref(),
        );
        config.security.csp_html_header =
            Some(axum::http::HeaderValue::from_str(&html_csp).map_err(|e| {
                anyhow::anyhow!("HTML CSP is not a valid HTTP header value ({e}): {html_csp:?}")
            })?);
    }

    let (log_filter, log_level_parse_err) = resolve_log_filter(&config.log_level);
    // try_init rather than init: now that run() is a public library entrypoint,
    // a host process that has already installed a global tracing subscriber is a
    // reachable path. init() would panic; try_init returns Err that we surface
    // through run()'s error contract.
    tracing_subscriber::fmt()
        .with_env_filter(log_filter)
        .try_init()
        .map_err(|e| anyhow::anyhow!("failed to initialize tracing subscriber: {e}"))?;
    if let Some(err) = log_level_parse_err {
        tracing::warn!(
            error = %err,
            "configured log level is unparsable; falling back to info. \
             Fix REVERIE_LOG_LEVEL (or RUST_LOG fallback) to silence this warning."
        );
    }

    if config.operator_contact.is_none() {
        tracing::warn!(
            "REVERIE_OPERATOR_CONTACT unset — OpenLibrary requests will run at the 1 req/s anonymous tier. \
             Set REVERIE_OPERATOR_CONTACT=<email-or-url> to unlock the identified 3 req/s tier."
        );
    }

    let pool = db::init_pool(&config.database_url, config.db_max_connections)
        .await
        .map_err(|e| anyhow::anyhow!("failed to connect to database: {e}"))?;

    // Startup schema step — apply in-process (opt-in) or verify (default).
    // Extracted to apply_or_verify_schema so the flag selector (the security
    // contract that the default path carries no migration credential) is
    // pinned by tests. The app pool is created first but performs no
    // schema-dependent query before this runs.
    apply_or_verify_schema(&config, &pool).await?;

    let oidc_client = auth::oidc::init_oidc_client(&config)
        .await
        .map_err(|e| anyhow::anyhow!("failed to initialize OIDC client: {e}"))?;

    let ingestion_pool = db::init_pool(&config.ingestion_database_url, config.db_max_connections)
        .await
        .map_err(|e| anyhow::anyhow!("failed to connect ingestion pool: {e}"))?;

    let initial_settings = services::settings::load(&pool)
        .await
        .map_err(|e| anyhow::anyhow!("failed to load settings from database: {e}"))?;
    let settings = std::sync::Arc::new(tokio::sync::RwLock::new(initial_settings));
    let last_settings_reload = std::sync::Arc::new(tokio::sync::RwLock::new(None));

    let state = AppState {
        pool,
        ingestion_pool,
        config: config.clone(),
        oidc_client,
        settings,
        last_settings_reload,
    };
    let app = build_router(state.clone());

    // Spawn ingestion watcher with a cancellation token for graceful shutdown
    let cancel_token = tokio_util::sync::CancellationToken::new();

    // Settings LISTEN/NOTIFY listener (refreshes Arc<RwLock<Settings>>)
    let settings_token = cancel_token.clone();
    let settings_pool = state.pool.clone();
    let settings_handle = state.settings.clone();
    let settings_reload = state.last_settings_reload.clone();
    tokio::spawn(async move {
        services::settings::spawn_listener(
            settings_pool,
            settings_handle,
            settings_reload,
            settings_token,
        )
        .await;
    });
    let watcher_token = cancel_token.clone();
    let watcher_config = config.clone();
    let watcher_pool = state.ingestion_pool.clone();
    tokio::spawn(async move {
        if let Err(e) =
            services::ingestion::run_watcher(watcher_config, watcher_pool, watcher_token).await
        {
            tracing::error!(error = %e, "ingestion watcher exited with error");
        }
    });

    let enrich_token = cancel_token.clone();
    let enrich_config = config.clone();
    let enrich_pool = state.ingestion_pool.clone();
    tokio::spawn(async move {
        if let Err(e) =
            services::enrichment::spawn_queue(enrich_pool, enrich_config, enrich_token).await
        {
            tracing::error!(error = %e, "enrichment queue exited with error");
        }
    });

    // Session expired-row sweep. Drives the PostgresStore's ExpiredDeletion
    // trait hourly so `tower_sessions.session` stays bounded; shares the
    // cancellation token so SIGTERM drains it like the other workers.
    let sweep_token = cancel_token.clone();
    let sweep_store = PostgresStore::new(state.pool.clone());
    tokio::spawn(services::session_sweep::run_sweep(sweep_store, sweep_token));

    // Writeback worker runs on a dedicated reverie_app pool that sets
    // `app.system_context = 'writeback'` per-connection.  The
    // `manifestations_*_system` RLS policies match only when that GUC is
    // set, so user-facing handlers (which never set it) cannot reach the
    // system policies even if they forget `SET LOCAL app.current_user_id`.
    let writeback_token = cancel_token.clone();
    let writeback_config = config.clone();
    let writeback_pool = db::init_writeback_pool(&config.database_url, config.db_max_connections)
        .await
        .map_err(|e| anyhow::anyhow!("failed to build writeback pool: {e}"))?;
    tokio::spawn(async move {
        if let Err(e) =
            services::writeback::spawn_worker(writeback_pool, writeback_config, writeback_token)
                .await
        {
            tracing::error!(error = %e, "writeback worker exited with error");
        }
    });

    let addr = format!("0.0.0.0:{}", config.port);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .map_err(|e| anyhow::anyhow!("failed to bind to {addr}: {e}"))?;

    tracing::info!("listening on {}", listener.local_addr()?);

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal(cancel_token))
        .await
        .map_err(|e| anyhow::anyhow!("server error: {e}"))?;

    Ok(())
}

/// Startup schema step: apply pending migrations in-process when
/// `config.auto_migrate` is set, otherwise verify the schema is current via
/// the application pool (holding no migration credential).
///
/// This is the seam that carries the PR's security contract — the default
/// path (`auto_migrate == false`) must NOT migrate and must NOT require a
/// migration DSN. Extracted from `run()` so the flag selector is pinned by
/// tests; inverting the branch is otherwise invisible to the suite.
///
/// # Errors
///
/// - `auto_migrate` set but `migration_database_url` is `None` (defensive —
///   `Config::from_source` already rejects this).
/// - The in-process migration run fails (see [`db::run_migrations`]).
/// - Schema verification fails or detects divergence (see
///   [`db::verify_schema_current`]).
async fn apply_or_verify_schema(config: &Config, app_pool: &sqlx::PgPool) -> anyhow::Result<()> {
    if config.auto_migrate {
        let migration_url = config
            .migration_database_url
            .as_deref()
            .context("REVERIE_AUTO_MIGRATE is set but DATABASE_URL_MIGRATION is missing")?;
        let report = db::run_migrations(migration_url)
            .await
            .context("database migration failed")?;
        if report.applied > 0 {
            tracing::info!(
                count = report.applied,
                elapsed_ms = report.elapsed_ms,
                "applied pending migrations"
            );
        } else {
            tracing::debug!("database schema is up to date");
        }
        Ok(())
    } else {
        // Default path: confirm the out-of-band migration ran and the schema
        // matches this binary. Fail-closed on any divergence (ahead OR behind)
        // and on a never-migrated database, so a stale or unmigrated deployment
        // is a legible startup error rather than silent runtime SQL failures.
        db::verify_schema_current(app_pool)
            .await
            .context("database schema verification failed")
    }
}

/// The subcommand selected on the command line.
///
/// Deliberately NOT `#[non_exhaustive]`: `main.rs` matches it exhaustively, so
/// adding a variant is a compile error there rather than a silent fall-through
/// to the server — the same "a typo must not boot the server" property
/// [`parse_command`] enforces at runtime.
#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    /// `reverie migrate` — apply pending migrations out-of-band, then exit.
    Migrate,
    /// No subcommand — run the long-lived HTTP server.
    Serve,
}

/// Map the CLI argument tail (everything after `argv[0]`) to a [`Command`].
///
/// no args → [`Command::Serve`]; exactly `migrate` → [`Command::Migrate`]. Any
/// other token, OR any trailing argument, is an error rather than a silent
/// fall-through: a typo (`reverie migration`, `reverie --help`) must not boot
/// the long-running server, and a write-capable subcommand must not silently
/// ignore extra tokens (`reverie migrate typo` must not run migrations). In a
/// compose `service_completed_successfully` slot either mistake would make a
/// one-shot migrate container run forever as a server.
///
/// # Errors
///
/// Returns an error naming the unknown subcommand, or reporting unexpected
/// trailing arguments.
pub fn parse_command(args: &[String]) -> anyhow::Result<Command> {
    match args {
        [] => Ok(Command::Serve),
        [cmd] if cmd == "migrate" => Ok(Command::Migrate),
        [cmd] => Err(anyhow::anyhow!(
            "unknown subcommand: {cmd:?}; valid subcommands: migrate"
        )),
        [cmd, ..] => Err(anyhow::anyhow!(
            "unexpected trailing arguments after {cmd:?}; usage: reverie [migrate]"
        )),
    }
}

/// Resolve the migration DSN from the raw `DATABASE_URL_MIGRATION` value,
/// treating empty/whitespace as unset.
///
/// `std::env::var` returns `Ok("")` for an exported-empty var, which would
/// otherwise reach `db::run_migrations` as a cryptic `Connection` parse error;
/// this mirrors the `.filter()` guard in [`Config::from_source`].
///
/// # Errors
///
/// Returns an error when the value is `None`, empty, or whitespace-only.
fn resolve_migration_dsn(var: Option<String>) -> anyhow::Result<String> {
    var.filter(|s| !s.trim().is_empty())
        .ok_or_else(|| anyhow::anyhow!("DATABASE_URL_MIGRATION is required for `reverie migrate`"))
}

/// Apply pending database migrations, then exit. This is the `reverie
/// migrate` subcommand entrypoint — the shipped default for schema
/// management, run out-of-band so the long-lived server process never
/// carries the migration credential.
///
/// Deliberately does NOT build the full [`Config`]: a migrate container has
/// no business holding the OIDC secret or the application DSN. It reads only
/// `DATABASE_URL_MIGRATION` (the `reverie_migrator` DSN) and reuses
/// [`db::run_migrations`].
///
/// # Errors
///
/// - If `DATABASE_URL_MIGRATION` is unset or empty (an exported-empty value
///   is treated as unset, mirroring [`Config::from_source`]).
/// - If the migration run fails (see [`db::MigrationError`]).
pub async fn run_migrate() -> anyhow::Result<()> {
    // run_migrate bypasses Config::from_env, which is where dotenv is normally
    // loaded — so load it here too, or `cargo run -- migrate` would ignore a
    // DATABASE_URL_MIGRATION set in the project .env and fail spuriously.
    dotenvy::dotenv().ok();

    // run_migrate bypasses run(), where the tracing subscriber is normally
    // installed. Without one here, every event in this function AND inside
    // db::run_migrations drops silently, leaving the operator with only an
    // exit code. Best-effort: .ok() tolerates a host that already installed
    // a global subscriber.
    tracing_subscriber::fmt().try_init().ok();

    let migration_url = resolve_migration_dsn(std::env::var("DATABASE_URL_MIGRATION").ok())?;

    let report = db::run_migrations(&migration_url)
        .await
        .context("database migration failed")?;
    if report.applied > 0 {
        tracing::info!(
            count = report.applied,
            elapsed_ms = report.elapsed_ms,
            "applied pending migrations"
        );
    } else {
        tracing::info!("database schema is already up to date");
    }
    Ok(())
}

async fn shutdown_signal(cancel_token: tokio_util::sync::CancellationToken) {
    let ctrl_c = tokio::signal::ctrl_c();
    #[allow(
        clippy::expect_used,
        reason = "Signal registration happens once at startup; failure means the OS cannot deliver SIGTERM to this process at all, which is an unrecoverable condition on a Unix host — panicking here is correct"
    )]
    let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .expect("failed to register SIGTERM handler");
    tokio::select! {
        _ = ctrl_c => {},
        _ = sigterm.recv() => {},
    }
    tracing::info!("shutdown signal received");
    cancel_token.cancel();
}

#[cfg(test)]
mod tests {
    use super::{
        Command, apply_or_verify_schema, parse_command, resolve_log_filter, resolve_migration_dsn,
    };
    use crate::test_support;

    #[test]
    fn parse_command_maps_args_rejects_unknown_and_trailing() {
        let migrate = vec!["migrate".to_string()];
        assert_eq!(parse_command(&migrate).unwrap(), Command::Migrate);
        assert_eq!(parse_command(&[]).unwrap(), Command::Serve);

        let unknown = vec!["migration".to_string()];
        let err = parse_command(&unknown).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("unknown subcommand"), "got: {msg}");
        assert!(
            msg.contains("migration"),
            "should echo the bad token: {msg}"
        );

        // A write-capable subcommand must not silently ignore extra tokens.
        let trailing = vec!["migrate".to_string(), "typo".to_string()];
        let err = parse_command(&trailing).unwrap_err();
        assert!(
            err.to_string().contains("trailing"),
            "trailing args must be rejected, got: {err}"
        );
    }

    #[test]
    fn resolve_migration_dsn_rejects_missing_and_empty() {
        assert!(resolve_migration_dsn(None).is_err());
        assert!(resolve_migration_dsn(Some(String::new())).is_err());
        assert!(resolve_migration_dsn(Some("   ".into())).is_err());

        let url = "postgres://reverie_migrator@localhost/reverie_dev";
        assert_eq!(resolve_migration_dsn(Some(url.into())).unwrap(), url);
    }

    // apply_or_verify_schema is the flag selector carrying the security
    // contract: flag off MUST take the verify branch (no migration credential),
    // flag on MUST take the migrate branch. These pin the branch both ways so an
    // inverted condition fails the suite.

    #[sqlx::test(migrations = "./migrations")]
    async fn apply_or_verify_flag_off_verifies_ok(pool: sqlx::PgPool) {
        let cfg = test_support::test_config(); // auto_migrate: false
        let app_pool = test_support::db::app_pool_for(&pool).await;
        apply_or_verify_schema(&cfg, &app_pool)
            .await
            .expect("flag off + current schema must verify Ok via the app pool");
    }

    #[sqlx::test(migrations = false)]
    async fn apply_or_verify_flag_off_takes_verify_branch(pool: sqlx::PgPool) {
        // Fresh, never-migrated DB + flag off. The verify branch returns
        // NotInitialized; the migrate branch would instead APPLY all
        // migrations and succeed. A NotInitialized error therefore proves the
        // selector chose verify — the security contract (no migration
        // credential on the default path). Inverting the branch flips this to
        // Ok and fails the assert.
        let cfg = test_support::test_config(); // auto_migrate: false
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let err = apply_or_verify_schema(&cfg, &app_pool).await.unwrap_err();
        assert!(
            format!("{err:#}").contains("not initialized"),
            "flag off must take the verify branch (NotInitialized on a fresh DB), got: {err:#}"
        );
    }

    #[sqlx::test(migrations = false)]
    async fn apply_or_verify_flag_on_takes_migrate_branch(pool: sqlx::PgPool) {
        // Flag on + DSN absent. Only the migrate branch inspects
        // migration_database_url, so the "DATABASE_URL_MIGRATION is missing"
        // error proves the selector chose migrate; the verify branch would
        // instead read the schema and return NotInitialized. Inverting the
        // branch flips the error message and fails the assert. (The migrate
        // branch's privilege sufficiency is covered by
        // `db::tests::reverie_migrator_can_apply_full_migration_set`.)
        let mut cfg = test_support::test_config();
        cfg.auto_migrate = true;
        cfg.migration_database_url = None;
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let err = apply_or_verify_schema(&cfg, &app_pool).await.unwrap_err();
        assert!(
            format!("{err:#}").contains("DATABASE_URL_MIGRATION is missing"),
            "flag on must take the migrate branch (checks the DSN), got: {err:#}"
        );
    }

    #[tokio::test]
    async fn health_returns_ok() {
        let server = test_support::test_server();
        let response = server.get("/health").await;
        response.assert_status_ok();
        response.assert_text("ok");
    }

    /// Regression coverage for the `problem_instance_layer` wiring on
    /// the composite router (see `build_router_with_session_store`).
    /// A future layer-order edit that drops or repositions the
    /// middleware would silently lose the RFC 7807 `instance` field;
    /// this test fails before that change ships.
    #[tokio::test]
    async fn unmatched_api_route_returns_problem_with_instance() {
        let server = test_support::test_server();
        let r = server.get("/api/__definitely_not_a_route__").await;
        let body = test_support::assert_problem(
            &r,
            crate::error::problems::NOT_FOUND,
            axum::http::StatusCode::NOT_FOUND,
        );
        assert_eq!(
            body["instance"].as_str(),
            Some("/api/__definitely_not_a_route__"),
            "instance must be populated by problem_instance_layer, got: {body}",
        );
    }

    // resolve_log_filter parses `configured_level` directly — env precedence
    // (REVERIE_LOG_LEVEL > RUST_LOG > "info") is resolved upstream by
    // Config::from_source, so these tests are insensitive to whatever env
    // vars happen to be set in the test runner.

    #[test]
    fn resolve_log_filter_returns_no_error_for_valid_configured_level() {
        let (_filter, err) = resolve_log_filter("debug");
        assert!(
            err.is_none(),
            "valid configured level should not produce a parse error, got {err:?}"
        );
    }

    #[test]
    fn resolve_log_filter_surfaces_error_for_invalid_configured_level() {
        // EnvFilter parsing rejects directives where the level segment after `=`
        // is not one of trace/debug/info/warn/error/off (or a numeric verbosity).
        // "info=bogus" is a level-name typo — exactly the operator-error class
        // this test guards against.
        let bad = "info=bogus";
        let (_filter, err) = resolve_log_filter(bad);
        let err = err.expect("invalid configured level should produce a parse error");
        assert!(
            err.contains(bad),
            "error message should name the bad value, got: {err}"
        );
    }

    // PostgresStore replaces MemoryStore in production specifically so a
    // backend restart does not nuke every active session (LXC redeploy =
    // forced re-login is the staging-friction this swap avoids). The test
    // simulates that restart by saving a record through one PostgresStore
    // instance, dropping it, building a fresh PostgresStore against the
    // same DB pool, and asserting the record loads with identical
    // contents.
    #[sqlx::test(migrations = "./migrations")]
    async fn session_record_survives_store_restart(pool: sqlx::PgPool) {
        use std::collections::HashMap;
        use time::OffsetDateTime;
        use tower_sessions::SessionStore;
        use tower_sessions::session::{Id, Record};

        use crate::auth::store::PostgresStore;

        let app_pool = test_support::db::app_pool_for(&pool).await;

        let mut data: HashMap<String, serde_json::Value> = HashMap::new();
        data.insert("user_id".into(), serde_json::json!("user-42"));
        data.insert("nonce".into(), serde_json::json!("abc-123-nonce"));

        let record_id = {
            let store = PostgresStore::new(app_pool.clone());
            let mut record = Record {
                id: Id::default(),
                data: data.clone(),
                expiry_date: OffsetDateTime::now_utc() + time::Duration::hours(1),
            };
            store.create(&mut record).await.expect("create session");
            record.id
        };

        // First store dropped — the bytes live only in tower_sessions.session.
        let store2 = PostgresStore::new(app_pool.clone());
        let loaded = store2
            .load(&record_id)
            .await
            .expect("load session record")
            .expect("session record persists across store recreation");

        assert_eq!(
            loaded.data, data,
            "session payload (incl. csrf nonce shape) survives intact"
        );
    }

    // PostgresStore must not return records whose expiry has passed.
    // The contract `SessionStore::load -> Ok(None)` for an expired id is
    // the load-bearing seam for stale-cookie auth: if it broke, a user
    // holding an expired session cookie would still resolve to an
    // authenticated identity. Asserting it explicitly closes the
    // negative-case gap CR raised on PR #180.
    #[sqlx::test(migrations = "./migrations")]
    async fn expired_session_is_not_returned(pool: sqlx::PgPool) {
        use std::collections::HashMap;
        use time::OffsetDateTime;
        use tower_sessions::SessionStore;
        use tower_sessions::session::{Id, Record};

        use crate::auth::store::PostgresStore;

        let app_pool = test_support::db::app_pool_for(&pool).await;
        let store = PostgresStore::new(app_pool.clone());

        let mut record = Record {
            id: Id::default(),
            data: HashMap::new(),
            expiry_date: OffsetDateTime::now_utc() - time::Duration::seconds(1),
        };
        store
            .create(&mut record)
            .await
            .expect("create expired session");

        let loaded = store
            .load(&record.id)
            .await
            .expect("load should not error on an expired id");
        assert!(
            loaded.is_none(),
            "expired session must not be returned by load"
        );
    }

    // `save` upserts an EXISTING row (the `ON CONFLICT DO UPDATE` branch). The
    // create/restart tests only cover the INSERT branch; this locks the UPDATE
    // branch against the live schema rather than relying on compile-validation
    // of shared `upsert` SQL alone.
    #[sqlx::test(migrations = "./migrations")]
    async fn store_save_updates_existing_record(pool: sqlx::PgPool) {
        use std::collections::HashMap;
        use time::OffsetDateTime;
        use tower_sessions::SessionStore;
        use tower_sessions::session::{Id, Record};

        use crate::auth::store::PostgresStore;

        let app_pool = test_support::db::app_pool_for(&pool).await;
        let store = PostgresStore::new(app_pool.clone());

        let mut record = Record {
            id: Id::default(),
            data: HashMap::new(),
            expiry_date: OffsetDateTime::now_utc() + time::Duration::hours(1),
        };
        store.create(&mut record).await.expect("create session");

        record
            .data
            .insert("user_id".into(), serde_json::json!("user-7"));
        store
            .save(&record)
            .await
            .expect("save upserts the existing row");

        let loaded = store
            .load(&record.id)
            .await
            .expect("load")
            .expect("record present after save");
        assert_eq!(
            loaded.data.get("user_id"),
            Some(&serde_json::json!("user-7")),
            "save must persist the updated payload onto the existing row"
        );
    }

    // `delete` removes the row by id (logout / explicit invalidation path).
    #[sqlx::test(migrations = "./migrations")]
    async fn store_delete_removes_record(pool: sqlx::PgPool) {
        use std::collections::HashMap;
        use time::OffsetDateTime;
        use tower_sessions::SessionStore;
        use tower_sessions::session::{Id, Record};

        use crate::auth::store::PostgresStore;

        let app_pool = test_support::db::app_pool_for(&pool).await;
        let store = PostgresStore::new(app_pool.clone());

        let mut record = Record {
            id: Id::default(),
            data: HashMap::new(),
            expiry_date: OffsetDateTime::now_utc() + time::Duration::hours(1),
        };
        store.create(&mut record).await.expect("create session");
        assert!(
            store.load(&record.id).await.expect("load").is_some(),
            "record must exist before delete"
        );

        store.delete(&record.id).await.expect("delete record");
        assert!(
            store
                .load(&record.id)
                .await
                .expect("load after delete")
                .is_none(),
            "delete must remove the row"
        );
    }
}
