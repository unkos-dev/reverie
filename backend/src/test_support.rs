use axum::Router;
use axum_test::TestServer;

/// Serialize tests that mutate or read environment variables so they don't
/// race with each other across modules. Import this wherever `std::env::set_var`
/// or `std::env::var("DATABASE_URL")` is used in test code.
pub static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

use crate::auth::backend::AuthBackend;
use crate::auth::oidc::OidcClient;
use crate::config::{CleanupMode, Config, CoverConfig, EnrichmentConfig};
use crate::state::AppState;

pub fn test_config() -> Config {
    Config {
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
        ingestion_database_url: String::new(),
        format_priority: vec![
            "epub".into(),
            "pdf".into(),
            "mobi".into(),
            "azw3".into(),
            "cbz".into(),
            "cbr".into(),
        ],
        cleanup_mode: CleanupMode::All,
        enrichment: EnrichmentConfig {
            enabled: false,
            concurrency: 1,
            poll_idle_secs: 30,
            fetch_budget_secs: 15,
            http_timeout_secs: 10,
            max_attempts: 3,
            cache_ttl_hit_days: 1,
            cache_ttl_miss_days: 1,
            cache_ttl_error_mins: 1,
        },
        cover: CoverConfig {
            max_bytes: 10_485_760,
            download_timeout_secs: 30,
            min_long_edge_px: 1000,
            redirect_limit: 3,
        },
        openlibrary_base_url: "https://openlibrary.org".into(),
        googlebooks_base_url: "https://www.googleapis.com/books/v1".into(),
        googlebooks_api_key: None,
        hardcover_base_url: "https://api.hardcover.app/v1/graphql".into(),
        hardcover_api_token: None,
    }
}

pub fn test_oidc_client() -> OidcClient {
    use openidconnect::core::{CoreProviderMetadata, CoreResponseType, CoreSubjectIdentifierType};
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

pub fn test_state() -> AppState {
    AppState {
        pool: sqlx::PgPool::connect_lazy("postgres://invalid").unwrap(),
        ingestion_pool: sqlx::PgPool::connect_lazy("postgres://invalid").unwrap(),
        config: test_config(),
        oidc_client: test_oidc_client(),
    }
}

/// Build the full application router with auth layer (for route integration tests).
pub fn test_server() -> TestServer {
    let state = test_state();
    let auth_backend = AuthBackend {
        pool: state.pool.clone(),
    };
    let app: Router = crate::build_router(state, auth_backend);
    TestServer::new(app)
}
