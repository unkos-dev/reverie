mod auth;
mod config;
mod db;
mod error;
mod models;
mod routes;
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
    // Acceptable for single-instance homelab; replace with tower-sessions-sqlx-store
    // if memory growth under sustained use becomes an issue.
    let session_store = MemoryStore::default();
    // Secure flag intentionally omitted: backend runs behind a TLS-terminating
    // reverse proxy and sees plain HTTP, so Secure would prevent cookie delivery.
    // Cookies are unsigned — session security relies on the cryptographic randomness
    // of tower-sessions session IDs (ChaCha-seeded via `rand` crate).
    let session_layer = SessionManagerLayer::new(session_store)
        .with_http_only(true)
        .with_same_site(tower_sessions::cookie::SameSite::Lax)
        .with_expiry(Expiry::OnInactivity(time::Duration::hours(24)));

    let auth_layer = AuthManagerLayerBuilder::new(auth_backend, session_layer).build();

    Router::new()
        .merge(routes::health::router())
        .merge(routes::auth::router())
        .merge(routes::tokens::router())
        .merge(routes::ingestion::router())
        .merge(routes::enrichment::router())
        .merge(routes::metadata::router())
        .layer(auth_layer)
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .with_state(state)
}

#[tokio::main]
async fn main() {
    let config = Config::from_env().expect("invalid configuration");

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
