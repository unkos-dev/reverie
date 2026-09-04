//! Reverie API server — library crate.
//!
//! Hosts the HTTP service, authentication, ingestion pipeline, metadata
//! enrichment, and OPDS catalogue. The accompanying `reverie-api` binary is a
//! thin entry that calls [`run`] under a `#[tokio::main]` runtime; the split
//! exists so that `missing_docs` and clippy pedantic doc lints fire on
//! externally-reachable items (a binary-only crate has no external API and
//! the lints are silent; see `docs/adr/0004-tiered-comment-policy-for-an-open-source-codebase.md`).
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
        reason = "test code exercises panics and stdout freely; production denies hold outside cfg(test)"
    )
)]

pub mod auth;
#[cfg(test)]
mod authz_matrix;
pub mod config;
pub mod db;
pub mod error;
pub mod models;
pub mod openapi;
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
/// OIDC `nonce` set by `/auth/oidc/login` that the callback test needs to
/// embed in a matching mock-issued ID token. External-crate tests under
/// `backend/tests/` cannot reach this function; intentional, since the
/// shared-store seam is only required by tests that exercise routing
/// internals (which already need crate-private access for fixtures).
/// Production builds use `PostgresStore` via [`build_router`].
#[expect(
    clippy::disallowed_types,
    reason = "tower_sessions::Expiry::OnInactivity takes a time::Duration and accepts no chrono equivalent; see adr/2026-08-05-first-party-datetime-crate.md"
)]
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
        // OpenAPI-documented modules (every unconditionally mounted route
        // module, incl. auth and the always-on /api/v1 cover mount): the
        // documented router is split into its runtime half here and its
        // spec half in `openapi::spec_json`.
        .merge(openapi::router());
    if let Some(opds) = routes::opds::router_enabled(&state.config.opds) {
        api_like = api_like.merge(opds);
    }
    let api_like = api_like
        // Must stay after every merge into api_like above: it only patches
        // routes already registered on this router at call time, so an
        // earlier placement would leave later-merged routes on axum's bare
        // 405. Scoped to api_like, so the SPA merge below is untouched.
        .method_not_allowed_fallback(security::headers::api_method_not_allowed_fallback)
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            security::headers::api_csp_layer,
        ))
        // CSRF synchronizer-token enforcement. Self-exempts safe
        // methods and non-session-authenticated callers, so pre-auth `/auth/*`
        // POSTs and Basic-auth OPDS callers pass through; it gates
        // session-authenticated mutations on `/api/v1/*`. Session state is
        // provided by the `session_layer` wrapping the composite below.
        .layer(axum::middleware::from_fn(security::csrf::csrf_required));

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
        // stack below) can emit a populated RFC 9457 `instance`
        // field. Applies to all routes including the composite
        // fallback so 404s on `/api/v1/*` carry the instance too.
        .layer(axum::middleware::from_fn(
            error::instance::problem_instance_layer,
        ))
        .layer(session_layer)
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .with_state(state)
}

fn resolve_log_filter(configured_level: &str) -> (EnvFilter, Option<String>) {
    // Single source of truth: `configured_level` already encodes the
    // REVERIE_LOG_LEVEL > RUST_LOG > "info" cascade resolved by the
    // EnvProvider. Re-reading RUST_LOG here would invert the
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
#[expect(
    clippy::too_many_lines,
    reason = "This body was verbatim moved from the pre-split `main.rs` and lightly extended (3 lines for try_init error propagation + the `# Errors` docstring section). a typed `StartupError` will reshape startup error handling and is the natural place to extract phase helpers (`setup_tracing`, `init_csp_headers`, `spawn_workers`)."
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

    if config.ingestion_dsn_defaulted {
        tracing::warn!(
            "DATABASE_URL_INGESTION unset — the ingestion pipeline will run as the application \
             role (DATABASE_URL) instead of the scoped reverie_ingestion role. Role separation is \
             inactive. Set DATABASE_URL_INGESTION=<reverie_ingestion DSN> to enforce it."
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

    // First-run env seed: create the first administrator from REVERIE_BOOTSTRAP_*
    // if configured and none exists. Idempotent; honours the single-admin gate.
    seed_admin_if_configured(&pool, config.password_min_length)
        .await
        .context("bootstrap the first administrator from the environment seed")?;

    // OIDC is optional: discover the client only when configured.
    // A local-only instance carries no OIDC client and the initiate/callback
    // handlers 404. Gate 4 has already guaranteed at least one provider is usable.
    let oidc_client = if config.oidc_configured() {
        Some(
            auth::oidc::init_oidc_client(&config)
                .await
                .map_err(|e| anyhow::anyhow!("failed to initialize OIDC client: {e}"))?,
        )
    } else {
        None
    };

    // Resource-server JWT validation is likewise optional: a local-only or
    // OIDC-only instance carries no validator, and the extractor's JWT path
    // 401s inertly (see `auth::middleware::verify_bearer`).
    let jwt_validator = if config.resource_server_configured() {
        Some(std::sync::Arc::new(
            auth::jwt::init_jwt_validator(&config)
                .await
                .map_err(|e| anyhow::anyhow!("failed to initialize JWT validator: {e}"))?,
        ))
    } else {
        None
    };

    let ingestion_pool = db::init_pool(&config.ingestion_database_url, config.db_max_connections)
        .await
        .map_err(|e| anyhow::anyhow!("failed to connect ingestion pool: {e}"))?;

    let initial_settings = services::settings::load(&pool)
        .await
        .map_err(|e| anyhow::anyhow!("failed to load settings from database: {e}"))?;
    let settings = std::sync::Arc::new(tokio::sync::RwLock::new(initial_settings));
    let last_settings_reload = std::sync::Arc::new(tokio::sync::RwLock::new(None));

    // Per-IP login limiter. `login_rate_per_min` is validated `>= 1` at config
    // load, so the NonZeroU32 conversion cannot fail here; surface a startup
    // error rather than panic on the validated-impossible case.
    let login_limiter = auth::rate_limit::build_login_limiter(
        std::num::NonZeroU32::new(config.login_rate_per_min)
            .ok_or_else(|| anyhow::anyhow!("login_rate_per_min must be >= 1"))?,
    );

    let state = AppState {
        pool,
        ingestion_pool,
        config: config.clone(),
        oidc_client,
        jwt_validator,
        login_limiter,
        settings,
        last_settings_reload,
    };
    let app = build_router(state.clone());

    // All fallible setup happens BEFORE any worker is spawned, so an early
    // `?` return cannot leak already-running background tasks (they would
    // be neither cancelled nor drained). The writeback pool sets
    // `app.system_context = 'writeback'` per-connection; the
    // `manifestations_*_system` RLS policies match only when that GUC is
    // set, so user-facing handlers (which never set it) cannot reach the
    // system policies even if they forget `SET LOCAL app.current_user_id`.
    let writeback_pool = db::init_writeback_pool(&config.database_url, config.db_max_connections)
        .await
        .map_err(|e| anyhow::anyhow!("failed to build writeback pool: {e}"))?;

    let addr = format!("0.0.0.0:{}", config.port);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .map_err(|e| anyhow::anyhow!("failed to bind to {addr}: {e}"))?;

    tracing::info!("listening on {}", listener.local_addr()?);

    // Spawn ingestion watcher with a cancellation token for graceful shutdown
    let cancel_token = tokio_util::sync::CancellationToken::new();

    // Settings LISTEN/NOTIFY listener (refreshes Arc<RwLock<Settings>>)
    let settings_token = cancel_token.clone();
    let settings_pool = state.pool.clone();
    let settings_handle = state.settings.clone();
    let settings_reload = state.last_settings_reload.clone();
    let settings_worker = tokio::spawn(async move {
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
    let watcher_worker = tokio::spawn(async move {
        if let Err(e) =
            services::ingestion::run_watcher(watcher_config, watcher_pool, watcher_token).await
        {
            tracing::error!(error = %e, "ingestion watcher exited with error");
        }
    });

    let enrich_token = cancel_token.clone();
    let enrich_config = config.clone();
    let enrich_pool = state.ingestion_pool.clone();
    let enrich_worker = tokio::spawn(async move {
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
    let sweep_worker = tokio::spawn(services::session_sweep::run_sweep(sweep_store, sweep_token));

    // Writeback worker runs on the dedicated system-context pool built
    // above, before any spawn.
    let writeback_token = cancel_token.clone();
    let writeback_config = config.clone();
    let writeback_worker = tokio::spawn(async move {
        if let Err(e) =
            services::writeback::spawn_worker(writeback_pool, writeback_config, writeback_token)
                .await
        {
            tracing::error!(error = %e, "writeback worker exited with error");
        }
    });

    // Defer the `?` on the serve result until after the drain: on a serve
    // error the graceful-shutdown future never fires, so the token must be
    // cancelled here (idempotent on the clean path, where shutdown_signal
    // already cancelled it) and the workers drained before run() returns —
    // otherwise the error path leaks live tasks for the runtime to abort
    // mid-IO, the exact unclean exit graceful shutdown closes.
    // `into_make_service_with_connect_info` exposes the TCP peer as
    // `ConnectInfo<SocketAddr>` so the login rate limiter can key on the client
    // IP. The auth handlers extract it as `Option<ConnectInfo<..>>` and fall
    // back to the per-account backoff when absent (the test harness supplies no
    // peer).
    let serve_result = axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal(cancel_token.clone()))
    .await
    .map_err(|e| anyhow::anyhow!("server error: {e}"));

    cancel_token.cancel();
    drain_workers(
        vec![
            ("settings-listener", settings_worker),
            ("ingestion-watcher", watcher_worker),
            ("enrichment-queue", enrich_worker),
            ("session-sweep", sweep_worker),
            ("writeback-worker", writeback_worker),
        ],
        WORKER_DRAIN_TIMEOUT,
    )
    .await;

    serve_result
}

/// Total wall-clock budget for draining all background workers after the
/// HTTP server stops. Shared across workers (one deadline, not per-worker)
/// so shutdown latency stays bounded regardless of worker count; well under
/// systemd's default 90s stop budget.
const WORKER_DRAIN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// Await every spawned worker after the server stops, bounded by one shared
/// `budget` deadline. A worker that does not exit in time is aborted (a
/// dropped `JoinHandle` would merely detach it, leaving the runtime to
/// abort it at an arbitrary point); a worker that panicked surfaces here as
/// a `JoinError` and is logged at `error` level.
async fn drain_workers(
    workers: Vec<(&'static str, tokio::task::JoinHandle<()>)>,
    budget: std::time::Duration,
) {
    let deadline = tokio::time::Instant::now() + budget;
    for (name, mut handle) in workers {
        // `&mut handle` keeps ownership so the timeout arm can abort();
        // tokio::time::timeout would consume (and on expiry detach) it.
        match tokio::time::timeout_at(deadline, &mut handle).await {
            Ok(Ok(())) => tracing::info!(worker = name, "background worker drained"),
            Ok(Err(join_err)) => {
                tracing::error!(worker = name, error = %join_err, "background task join failed");
            }
            Err(_elapsed) => {
                handle.abort();
                tracing::error!(
                    worker = name,
                    "background worker did not exit within the drain budget; aborted"
                );
                // Confirm the cancellation actually lands before returning —
                // abort() only takes effect at the task's next await point,
                // and returning earlier would hand the still-unwinding task
                // back to the runtime drop. Bounded by a short fixed grace:
                // a task stuck in a blocking section never reaches an await
                // point, and shutdown must not hang on it.
                if tokio::time::timeout(ABORT_GRACE, &mut handle)
                    .await
                    .is_err()
                {
                    tracing::error!(
                        worker = name,
                        "aborted worker still running after grace period; leaving to runtime teardown"
                    );
                }
            }
        }
    }
}

/// Post-`abort()` wait for the cancellation to take effect (see
/// [`drain_workers`]). Short and fixed: it only covers the gap between
/// `abort()` and the task's next await point.
const ABORT_GRACE: std::time::Duration = std::time::Duration::from_secs(1);

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
///   [`Config::from_figment`] already rejects this).
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
    /// `reverie print-config-schema` — emit the config JSON Schema to stdout,
    /// then exit. A build/docs utility (regenerates the committed
    /// `config.schema.json`); reads no environment and touches no database.
    PrintConfigSchema,
    /// `reverie bootstrap`: create the first administrator from the
    /// `REVERIE_BOOTSTRAP_*` environment seed, then exit. No-op if an
    /// administrator already exists (honours the same single-admin gate as HTTP
    /// setup). No positional arguments.
    Bootstrap,
    /// `reverie reset-password <email>`: issue a recovery PIN for an account to
    /// the operator host file, then exit.
    ResetPassword {
        /// Email of the account to issue a recovery PIN for.
        email: String,
    },
    /// `reverie unlock-account <email>`: clear the per-account login throttle
    /// for an account, then exit.
    UnlockAccount {
        /// Email of the account to unlock.
        email: String,
    },
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
        [cmd] if cmd == "print-config-schema" => Ok(Command::PrintConfigSchema),
        [cmd] if cmd == "bootstrap" => Ok(Command::Bootstrap),
        [cmd, email] if cmd == "reset-password" => Ok(Command::ResetPassword {
            email: email.clone(),
        }),
        [cmd, email] if cmd == "unlock-account" => Ok(Command::UnlockAccount {
            email: email.clone(),
        }),
        [cmd] => Err(anyhow::anyhow!(
            "unknown or incomplete subcommand: {cmd:?}; valid subcommands: migrate, \
             print-config-schema, bootstrap, reset-password <email>, unlock-account <email>"
        )),
        [cmd, ..] => Err(anyhow::anyhow!(
            "unexpected arguments after {cmd:?}; usage: reverie [migrate | print-config-schema | \
             bootstrap | reset-password <email> | unlock-account <email>]"
        )),
    }
}

/// Emit the [`config::Config`] JSON Schema to stdout — the
/// `print-config-schema` subcommand. The output is the committed
/// `backend/config.schema.json` artifact (CI drift-checks a fresh emit against
/// it), and the source the configuration reference renders from.
///
/// Reads no environment and opens no database — it is `schema_for!` over the
/// config structs, so it runs in any context. Deterministic: `schemars`
/// orders definitions, and the trailing newline keeps `diff` POSIX-clean.
///
/// `schemars` emits each field's default into the schema (from the `Default`
/// impls — e.g. `port`'s default `3000` is present). The artifact is safe to
/// publish (hard rule 7) because every secret-bearing field defaults to
/// `String::new()` / `None`, so it renders as `""` / `null` — never real
/// credential material. The absence of a `Serialize` derive is a
/// secret-leak-prevention measure for the *serialize* path, not the mechanism
/// that keeps secrets out of the *schema*. Gated by the
/// `config_schema_has_no_secret_default_values` test.
///
/// `println!` is forbidden, so this writes through
/// [`std::io::Write`] directly.
///
/// # Errors
///
/// Returns an error if schema serialization or the stdout write fails.
pub fn print_config_schema() -> anyhow::Result<()> {
    use std::io::Write as _;
    let json = config_schema_json()?;
    std::io::stdout()
        .write_all(json.as_bytes())
        .context("write config schema to stdout")?;
    Ok(())
}

/// Render the config JSON Schema exactly as [`print_config_schema`] emits it
/// (pretty-printed, trailing newline included). The single rendering seam
/// shared by the CLI subcommand and the `gen_config_schema` drift test, so
/// the committed `config.schema.json` is compared against the same bytes the
/// binary would print.
///
/// # Errors
///
/// Returns an error if schema serialization fails.
pub fn config_schema_json() -> anyhow::Result<String> {
    let schema = schemars::schema_for!(config::Config);
    let mut json = serde_json::to_string_pretty(&schema).context("serialize config schema")?;
    json.push('\n');
    Ok(json)
}

/// Resolve the migration DSN from the raw `DATABASE_URL_MIGRATION` value,
/// treating empty/whitespace as unset.
///
/// `std::env::var` returns `Ok("")` for an exported-empty var, which would
/// otherwise reach `db::run_migrations` as a cryptic `Connection` parse error;
/// this mirrors the migration-DSN blank-guard in [`Config::from_figment`].
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
///   is treated as unset, mirroring [`Config::from_figment`]).
/// - If the migration run fails (see [`db::MigrationError`]).
pub async fn run_migrate() -> anyhow::Result<()> {
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

/// Read the first-administrator seed from the environment. Returns
/// `(email, display_name, password)` only when all three are present and
/// non-blank; a partial seed yields `None`.
///
/// Read directly from the environment rather than through the config pipeline:
/// the bootstrap password is a one-shot startup credential, so it is never
/// retained on the long-lived [`Config`].
fn read_bootstrap_seed() -> Option<(String, String, String)> {
    let get = |k: &str| std::env::var(k).ok().filter(|v| !v.trim().is_empty());
    Some((
        get("REVERIE_BOOTSTRAP_EMAIL")?,
        get("REVERIE_BOOTSTRAP_DISPLAY_NAME")?,
        get("REVERIE_BOOTSTRAP_PASSWORD")?,
    ))
}

/// Create the first administrator from the `REVERIE_BOOTSTRAP_*` environment
/// seed when one is configured and no administrator exists. Returns whether an
/// administrator was created. Idempotent: a no-op when an admin already exists or
/// no seed is set. Shared by the `reverie bootstrap` CLI and server startup;
/// both honour the DB-enforced single-admin gate in
/// [`models::user::create_first_admin`].
///
/// THREAT: the seed password is never logged (hard rule 3); only the resulting
/// account email is.
///
/// # Errors
///
/// Returns an error on an invalid seed email, a too-short password, hashing
/// failure, or a database error other than the benign already-bootstrapped race.
async fn seed_admin_if_configured(
    pool: &sqlx::PgPool,
    password_min_length: usize,
) -> anyhow::Result<bool> {
    if models::user::admin_exists(pool)
        .await
        .context("check for an existing administrator")?
    {
        return Ok(false);
    }
    let Some((email, display_name, password)) = read_bootstrap_seed() else {
        return Ok(false);
    };
    if !models::user::is_addr_spec(&email) {
        anyhow::bail!("REVERIE_BOOTSTRAP_EMAIL is not a valid email address");
    }
    if password.chars().count() < password_min_length {
        anyhow::bail!(
            "REVERIE_BOOTSTRAP_PASSWORD must be at least {password_min_length} characters"
        );
    }
    let phc = auth::password::hash_password(password.as_bytes())
        .map_err(|e| anyhow::anyhow!("failed to hash the bootstrap password: {e}"))?;
    match models::user::create_first_admin(pool, &email, &display_name, &phc).await {
        Ok(_) => {
            tracing::info!(email = %email, "created the first administrator from the environment seed");
            Ok(true)
        }
        Err(models::user::BootstrapError::AlreadyBootstrapped) => Ok(false),
        Err(models::user::BootstrapError::EmailTaken) => {
            anyhow::bail!("REVERIE_BOOTSTRAP_EMAIL is already in use")
        }
        Err(models::user::BootstrapError::Db(e)) => Err(anyhow::anyhow!("bootstrap failed: {e}")),
    }
}

/// `reverie bootstrap`: create the first administrator from the
/// `REVERIE_BOOTSTRAP_*` environment seed, then exit. A no-op if one already
/// exists; an error if no seed is configured and none exists.
///
/// # Errors
///
/// Returns an error on configuration load, database connection, or seed failure.
pub async fn run_bootstrap() -> anyhow::Result<()> {
    tracing_subscriber::fmt().try_init().ok();
    let config = config::Config::from_env().context("load configuration")?;
    let pool = db::init_pool(&config.database_url, 1)
        .await
        .context("connect to the database")?;
    if seed_admin_if_configured(&pool, config.password_min_length).await? {
        return Ok(());
    }
    if models::user::admin_exists(&pool).await? {
        tracing::info!("an administrator already exists; bootstrap is a no-op");
        return Ok(());
    }
    anyhow::bail!(
        "set REVERIE_BOOTSTRAP_EMAIL, REVERIE_BOOTSTRAP_DISPLAY_NAME, and \
         REVERIE_BOOTSTRAP_PASSWORD to create the first administrator"
    )
}

/// `reverie reset-password <email>`: issue a recovery PIN for an account to a
/// per-user operator file (mode 0600), then exit. The operator relays the PIN to
/// the user, who completes the reset at `/auth/reset-password`.
///
/// # Errors
///
/// Returns an error when the account is unknown, or on configuration, database,
/// hashing, or file-write failure.
pub async fn run_reset_password(email: &str) -> anyhow::Result<()> {
    tracing_subscriber::fmt().try_init().ok();
    let config = config::Config::from_env().context("load configuration")?;
    let pool = db::init_pool(&config.database_url, 1)
        .await
        .context("connect to the database")?;
    let user = models::user::find_by_email(&pool, email)
        .await
        .context("look up the account")?
        .ok_or_else(|| anyhow::anyhow!("no account with email {email:?}"))?;
    let pin = auth::recovery::generate_pin();
    let pin_hash = auth::password::hash_password(pin.as_bytes())
        .map_err(|e| anyhow::anyhow!("failed to hash the recovery PIN: {e}"))?;
    let expires_at = chrono::Utc::now() + chrono::TimeDelta::seconds(config.recovery_pin_ttl_secs);
    // Supersede prior PINs, persist the new one, and publish it as one
    // serialized step, so a concurrent forgot-password request in the server
    // process cannot leave the operator file and the stored hash describing
    // different PINs. Hashing stays outside that section.
    let issuance = auth::recovery::PinIssuance {
        user_id: user.id,
        email: email.to_owned(),
        pin,
        pin_hash,
        expires_at,
    };
    let outcome = auth::recovery::issue_pin(
        &pool,
        std::path::Path::new(&config.recovery_pin_dir),
        issuance,
    )
    .await
    .context("issue the recovery PIN")?;
    // A withheld PIN was neither stored nor published: a concurrent issuance
    // owns the account's recovery slot and its PIN is the live one. Report that
    // instead of implying an operator can relay a code that does not exist.
    if outcome == auth::recovery::PinIssueOutcome::Withheld {
        anyhow::bail!(
            "a concurrent recovery-PIN issuance owns the active slot; re-run to issue a fresh PIN"
        );
    }
    tracing::info!(
        email = %email,
        path = %config.recovery_pin_dir,
        "wrote a recovery PIN; share it with the user to reset their password"
    );
    Ok(())
}

/// `reverie unlock-account <email>`: clear the per-account login throttle for an
/// account (DB-backed, so this out-of-band process can clear it), then exit.
///
/// # Errors
///
/// Returns an error on configuration, database connection, or update failure.
pub async fn run_unlock_account(email: &str) -> anyhow::Result<()> {
    tracing_subscriber::fmt().try_init().ok();
    let config = config::Config::from_env().context("load configuration")?;
    let pool = db::init_pool(&config.database_url, 1)
        .await
        .context("connect to the database")?;
    models::login_throttle::reset(&pool, email)
        .await
        .context("clear the login throttle")?;
    tracing::info!(email = %email, "cleared the per-account login throttle");
    Ok(())
}

async fn shutdown_signal(cancel_token: tokio_util::sync::CancellationToken) {
    let ctrl_c = tokio::signal::ctrl_c();
    #[expect(
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
        Command, apply_or_verify_schema, drain_workers, parse_command, resolve_log_filter,
        resolve_migration_dsn,
    };
    use crate::test_support;

    #[tokio::test]
    async fn drain_workers_awaits_clean_exit() {
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        let worker = tokio::spawn(async move {
            rx.await.expect("release signal");
        });
        tx.send(()).unwrap();
        // Returns once the worker has actually unwound — well inside budget.
        drain_workers(vec![("clean", worker)], std::time::Duration::from_secs(5)).await;
    }

    #[tokio::test]
    async fn drain_workers_aborts_stuck_worker_at_deadline() {
        let stuck = tokio::spawn(std::future::pending::<()>());
        let started = std::time::Instant::now();
        drain_workers(
            vec![("stuck", stuck)],
            std::time::Duration::from_millis(100),
        )
        .await;
        assert!(
            started.elapsed() >= std::time::Duration::from_millis(100),
            "drain must actually wait out the budget before giving up: {:?}",
            started.elapsed()
        );
        assert!(
            started.elapsed() < std::time::Duration::from_secs(5),
            "drain must give up at the deadline, not hang on a stuck worker"
        );
    }

    #[tokio::test]
    async fn drain_workers_survives_panicked_worker() {
        let panicker = tokio::spawn(async {
            panic!("worker panic must surface as a logged JoinError, not propagate");
        });
        let after = tokio::spawn(async {});
        // Must not propagate the panic, and must still drain later workers.
        drain_workers(
            vec![("panicker", panicker), ("after", after)],
            std::time::Duration::from_secs(5),
        )
        .await;
    }

    #[tokio::test]
    async fn drain_workers_shares_one_deadline_across_workers() {
        let stuck_a = tokio::spawn(std::future::pending::<()>());
        let stuck_b = tokio::spawn(std::future::pending::<()>());
        let started = std::time::Instant::now();
        drain_workers(
            vec![("stuck-a", stuck_a), ("stuck-b", stuck_b)],
            std::time::Duration::from_millis(200),
        )
        .await;
        assert!(
            started.elapsed() < std::time::Duration::from_secs(2),
            "budget is one shared deadline, not per-worker: {:?}",
            started.elapsed()
        );
    }

    #[test]
    fn parse_command_maps_args_rejects_unknown_and_trailing() {
        let migrate = vec!["migrate".to_string()];
        assert_eq!(parse_command(&migrate).unwrap(), Command::Migrate);
        assert_eq!(parse_command(&[]).unwrap(), Command::Serve);
        let schema = vec!["print-config-schema".to_string()];
        assert_eq!(parse_command(&schema).unwrap(), Command::PrintConfigSchema);

        let unknown = vec!["migration".to_string()];
        let err = parse_command(&unknown).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("unknown"), "got: {msg}");
        assert!(
            msg.contains("migration"),
            "should echo the bad token: {msg}"
        );

        // A write-capable subcommand must not silently ignore extra tokens.
        let trailing = vec!["migrate".to_string(), "typo".to_string()];
        let err = parse_command(&trailing).unwrap_err();
        assert!(
            err.to_string().contains("arguments"),
            "trailing args must be rejected, got: {err}"
        );
    }

    #[test]
    fn parse_command_accepts_auth_subcommands() {
        assert_eq!(
            parse_command(&["bootstrap".to_string()]).unwrap(),
            Command::Bootstrap
        );
        assert_eq!(
            parse_command(&["reset-password".to_string(), "a@b.com".to_string()]).unwrap(),
            Command::ResetPassword {
                email: "a@b.com".to_string()
            }
        );
        assert_eq!(
            parse_command(&["unlock-account".to_string(), "a@b.com".to_string()]).unwrap(),
            Command::UnlockAccount {
                email: "a@b.com".to_string()
            }
        );

        // bootstrap takes no positional args.
        assert!(parse_command(&["bootstrap".to_string(), "x".to_string()]).is_err());
        // The arg-bearing subcommands require their email.
        assert!(parse_command(&["reset-password".to_string()]).is_err());
        assert!(parse_command(&["unlock-account".to_string()]).is_err());
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
    /// middleware would silently lose the RFC 9457 `instance` field;
    /// this test fails before that change ships.
    #[tokio::test]
    async fn unmatched_api_route_returns_problem_with_instance() {
        let server = test_support::test_server();
        let r = server.get("/api/v1/__definitely_not_a_route__").await;
        let body = test_support::assert_problem(
            &r,
            crate::error::problems::NOT_FOUND,
            axum::http::StatusCode::NOT_FOUND,
        );
        assert_eq!(
            body["instance"].as_str(),
            Some("/api/v1/__definitely_not_a_route__"),
            "instance must be populated by problem_instance_layer, got: {body}",
        );
    }

    // resolve_log_filter parses `configured_level` directly — env precedence
    // (REVERIE_LOG_LEVEL > RUST_LOG > "info") is resolved upstream by the
    // EnvProvider, so these tests are insensitive to whatever env
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
    #[expect(
        clippy::disallowed_types,
        reason = "tower_sessions::session::Record::expiry_date is a time::OffsetDateTime; see adr/2026-08-05-first-party-datetime-crate.md"
    )]
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
    #[expect(
        clippy::disallowed_types,
        reason = "tower_sessions::session::Record::expiry_date is a time::OffsetDateTime; see adr/2026-08-05-first-party-datetime-crate.md"
    )]
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
    #[expect(
        clippy::disallowed_types,
        reason = "tower_sessions::session::Record::expiry_date is a time::OffsetDateTime; see adr/2026-08-05-first-party-datetime-crate.md"
    )]
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
    #[expect(
        clippy::disallowed_types,
        reason = "tower_sessions::session::Record::expiry_date is a time::OffsetDateTime; see adr/2026-08-05-first-party-datetime-crate.md"
    )]
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
