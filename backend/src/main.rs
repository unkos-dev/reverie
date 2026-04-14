mod config;
mod db;
mod error;
mod models;
mod routes;
mod services;
mod state;

use axum::Router;
use tracing_subscriber::EnvFilter;

use crate::config::Config;
use crate::state::AppState;

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .merge(routes::health::router())
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

    let pool = db::init_pool(&config.database_url, config.db_max_connections)
        .await
        .expect("failed to connect to database");

    let state = AppState {
        pool,
        config: config.clone(),
    };
    let app = build_router(state);

    let addr = format!("0.0.0.0:{}", config.port);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("failed to bind");

    tracing::info!("listening on {}", listener.local_addr().unwrap());

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .expect("server error");
}

async fn shutdown_signal() {
    let ctrl_c = tokio::signal::ctrl_c();
    let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .expect("failed to register SIGTERM handler");
    tokio::select! {
        _ = ctrl_c => {},
        _ = sigterm.recv() => {},
    }
    tracing::info!("shutdown signal received");
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum_test::TestServer;

    fn test_router() -> Router {
        // Liveness endpoint only — no DB needed for unit tests.
        Router::new()
            .merge(routes::health::router())
            .with_state(AppState {
                pool: sqlx::PgPool::connect_lazy("postgres://invalid").unwrap(),
                config: Config {
                    port: 3000,
                    database_url: String::new(),
                    library_path: String::new(),
                    ingestion_path: String::new(),
                    quarantine_path: String::new(),
                    log_level: "info".into(),
                    db_max_connections: 10,
                },
            })
    }

    #[tokio::test]
    async fn health_returns_ok() {
        let server = TestServer::new(test_router());
        let response: axum_test::TestResponse = server.get("/health").await;
        response.assert_status_ok();
        response.assert_text("ok");
    }
}
