use sqlx::PgPool;

use crate::auth::oidc::OidcClient;
use crate::config::Config;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub ingestion_pool: PgPool,
    pub config: Config,
    pub oidc_client: OidcClient,
}
