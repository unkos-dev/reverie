mod auth;
mod config;
mod db;
mod error;
mod models;
mod routes;
mod security;
mod services;
mod state;
#[cfg(test)]
pub(crate) mod test_support;

use axum::Router;
use axum_login::AuthManagerLayerBuilder;
use tower_sessions::{Expiry, MemoryStore, SessionManagerLayer};
use tracing_subscriber::EnvFilter;

use crate::auth::backend::AuthBackend;
use crate::config::Config;
use crate::state::AppState;

pub fn build_router(state: AppState, auth_backend: AuthBackend) -> Router {
    // NOTE: MemoryStore does not evict expired sessions server-side — the cookie
    // expires client-side but the HashMap entry stays until process restart.
    // Acceptable for single-instance self-hosted deployments; replace with
    // tower-sessions-sqlx-store if memory growth under sustained use becomes an issue.
    build_router_with_session_store(state, auth_backend, MemoryStore::default())
}

/// Same as [`build_router`] but with a caller-provided session store.
///
/// Used by integration tests to share a `MemoryStore` between the test
/// harness and the running server, so the test can read server-written
/// session state — e.g. the OIDC `nonce` set by `/auth/login` that the
/// callback test needs to embed in a matching mock-issued ID token.
pub(crate) fn build_router_with_session_store<S>(
    state: AppState,
    auth_backend: AuthBackend,
    session_store: S,
) -> Router
where
    S: tower_sessions::SessionStore + Clone,
{
    // Secure flag intentionally omitted: backend runs behind a TLS-terminating
    // reverse proxy and sees plain HTTP, so Secure would prevent cookie delivery.
    // Cookies are unsigned — session security relies on the cryptographic randomness
    // of tower-sessions session IDs (ChaCha-seeded via `rand` crate).
    let session_layer = SessionManagerLayer::new(session_store)
        .with_http_only(true)
        .with_same_site(tower_sessions::cookie::SameSite::Lax)
        .with_expiry(Expiry::OnInactivity(time::Duration::hours(24)));

    let auth_layer = AuthManagerLayerBuilder::new(auth_backend, session_layer).build();

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
        .layer(auth_layer)
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .with_state(state)
}

#[tokio::main]
async fn main() {
    let mut config = Config::from_env().expect("invalid configuration");

    // Finalise CSP headers once at startup. API CSP has no dynamic inputs
    // besides the optional report endpoint. HTML CSP consumes the script-src
    // hash list produced by `vite build`'s csp-hash plugin and read back from
    // the committed sidecar. Panicking at startup beats silently dropping
    // the security header on every response.
    let api_csp = security::csp::build_api_csp(config.security.csp_report_endpoint.as_ref());
    config.security.csp_api_header = Some(
        axum::http::HeaderValue::from_str(&api_csp).unwrap_or_else(|e| {
            panic!("API CSP is not a valid HTTP header value ({e}): {api_csp:?}")
        }),
    );
    if let Some(dist_path) = config.security.frontend_dist_path.clone() {
        let validated = security::dist_validation::validate_frontend_dist(&dist_path)
            .expect("frontend dist validation failed — rebuild frontend (vite build)");
        let html_csp = security::csp::build_html_csp(
            &validated.script_src_hashes,
            config.security.csp_report_endpoint.as_ref(),
        );
        config.security.csp_html_header = Some(
            axum::http::HeaderValue::from_str(&html_csp).unwrap_or_else(|e| {
                panic!("HTML CSP is not a valid HTTP header value ({e}): {html_csp:?}")
            }),
        );
    }

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| config.log_level.parse().expect("invalid RUST_LOG value")),
        )
        .init();

    if config.operator_contact.is_none() {
        tracing::warn!(
            "REVERIE_OPERATOR_CONTACT unset — OpenLibrary requests will run at the 1 req/s anonymous tier. \
             Set REVERIE_OPERATOR_CONTACT=<email-or-url> to unlock the identified 3 req/s tier."
        );
    }

    let pool = db::init_pool(&config.database_url, config.db_max_connections)
        .await
        .expect("failed to connect to database");

    let oidc_client = auth::oidc::init_oidc_client(&config)
        .await
        .expect("failed to initialize OIDC client");

    let ingestion_pool = db::init_pool(&config.ingestion_database_url, config.db_max_connections)
        .await
        .expect("failed to connect ingestion pool");

    let auth_backend = AuthBackend { pool: pool.clone() };
    let state = AppState {
        pool,
        ingestion_pool,
        config: config.clone(),
        oidc_client,
    };
    let app = build_router(state.clone(), auth_backend);

    // Spawn ingestion watcher with a cancellation token for graceful shutdown
    let cancel_token = tokio_util::sync::CancellationToken::new();
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

    // Writeback worker runs on a dedicated reverie_app pool that sets
    // `app.system_context = 'writeback'` per-connection.  The
    // `manifestations_*_system` RLS policies match only when that GUC is
    // set, so user-facing handlers (which never set it) cannot reach the
    // system policies even if they forget `SET LOCAL app.current_user_id`.
    let writeback_token = cancel_token.clone();
    let writeback_config = config.clone();
    let writeback_pool = db::init_writeback_pool(&config.database_url, config.db_max_connections)
        .await
        .expect("failed to build writeback pool");
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
        .expect("failed to bind");

    tracing::info!("listening on {}", listener.local_addr().unwrap());

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal(cancel_token))
        .await
        .expect("server error");
}

async fn shutdown_signal(cancel_token: tokio_util::sync::CancellationToken) {
    let ctrl_c = tokio::signal::ctrl_c();
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
    use crate::test_support;

    #[tokio::test]
    async fn health_returns_ok() {
        let server = test_support::test_server();
        let response = server.get("/health").await;
        response.assert_status_ok();
        response.assert_text("ok");
    }
}
