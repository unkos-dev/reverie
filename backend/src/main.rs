mod auth;
mod config;
mod db;
mod error;
mod models;
mod routes;
mod services;
mod state;

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

    let pool = db::init_pool(&config.database_url, config.db_max_connections)
        .await
        .expect("failed to connect to database");

    let oidc_client = auth::oidc::init_oidc_client(&config)
        .await
        .expect("failed to initialize OIDC client");

    let auth_backend = AuthBackend { pool: pool.clone() };
    let state = AppState {
        pool,
        config: config.clone(),
        oidc_client,
    };
    let app = build_router(state, auth_backend);

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
        // Liveness endpoint only — no DB or OIDC needed for unit tests.
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
                    oidc_issuer_url: String::new(),
                    oidc_client_id: String::new(),
                    oidc_client_secret: String::new(),
                    oidc_redirect_uri: String::new(),
                },
                oidc_client: test_oidc_client(),
            })
    }

    /// Create a minimal OidcClient for tests (no real OIDC provider).
    fn test_oidc_client() -> crate::auth::oidc::OidcClient {
        use openidconnect::core::{
            CoreProviderMetadata, CoreResponseType, CoreSubjectIdentifierType,
        };
        use openidconnect::{
            AuthUrl, ClientId, EmptyAdditionalProviderMetadata, IssuerUrl, JsonWebKeySetUrl,
            RedirectUrl, ResponseTypes, TokenUrl,
        };

        let issuer = IssuerUrl::new("https://fake-issuer.example.com".into()).unwrap();
        let provider = CoreProviderMetadata::new(
            issuer,
            AuthUrl::new("https://fake-issuer.example.com/auth".into()).unwrap(),
            JsonWebKeySetUrl::new("https://fake-issuer.example.com/jwks".into()).unwrap(),
            vec![ResponseTypes::new(vec![CoreResponseType::Code])],
            vec![CoreSubjectIdentifierType::Public],
            vec![],
            EmptyAdditionalProviderMetadata {},
        )
        .set_token_endpoint(Some(
            TokenUrl::new("https://fake-issuer.example.com/token".into()).unwrap(),
        ));

        openidconnect::core::CoreClient::from_provider_metadata(
            provider,
            ClientId::new("test-client".into()),
            Some(openidconnect::ClientSecret::new("test-secret".into())),
        )
        .set_redirect_uri(RedirectUrl::new("http://localhost:3000/auth/callback".into()).unwrap())
    }

    #[tokio::test]
    async fn health_returns_ok() {
        let server = TestServer::new(test_router());
        let response: axum_test::TestResponse = server.get("/health").await;
        response.assert_status_ok();
        response.assert_text("ok");
    }
}
