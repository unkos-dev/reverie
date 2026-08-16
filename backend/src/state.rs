//! Process-wide state shared across handlers, middleware, and background
//! workers.
//!
//! [`AppState`] is `Clone`. Each field is cheaply cloneable: `PgPool`
//! is `Arc`-backed internally; [`Config`] is owned data (`Clone`-derived,
//! mostly `String` / `Vec` / nested config structs); [`OidcRuntime`] is
//! `Arc`-wrapped so every clone shares one discovered client and one HTTP
//! connection pool. It is built once during [`crate::run`] and threaded
//! through Axum's `with_state`, the auth/session layers, the ingestion
//! watcher, the enrichment queue, and (read-only) the writeback worker.

use std::sync::Arc;

use sqlx::PgPool;
use tokio::sync::RwLock;

use chrono::{DateTime, Utc};

use crate::auth::jwt::JwtValidator;
use crate::auth::oidc::OidcRuntime;
use crate::auth::rate_limit::LoginLimiter;
use crate::config::Config;
use crate::models::settings::Settings;

/// Cloneable handle to every dependency a request handler or background
/// task needs. Constructed once at startup; threaded through Axum via
/// `with_state` and into spawned tasks via per-task clones.
#[derive(Clone)]
pub struct AppState {
    /// Primary application pool. Connections run as `reverie_app`; every
    /// user-facing query MUST acquire via [`crate::db::acquire_with_rls`]
    /// so RLS policies see `app.current_user_id` set transaction-locally.
    pub pool: PgPool,
    /// Ingestion-pipeline pool. Connections run as `reverie_ingestion`
    /// and exercise the `*_ingestion_full_access` RLS policies. Used by
    /// the watcher, dry-run handlers, and metadata fetchers.
    pub ingestion_pool: PgPool,
    /// Resolved configuration loaded once at startup. Includes finalised
    /// CSP `HeaderValue`s on `config.security` (built in `run` before this
    /// state is constructed).
    pub config: Config,
    /// Interactive OIDC runtime: the pre-discovered client (issuer metadata +
    /// JWKS) paired with the bounded transport that discovered it, or `None`
    /// when OIDC is not configured (local-only instance). Discovery happens
    /// once at startup in [`crate::auth::oidc::init_oidc_client`] and only when
    /// [`crate::config::Config::oidc_configured`] is true. The OIDC
    /// initiate/callback handlers guard on `Some` and 404 when this is `None`;
    /// the callback's token exchange reuses this runtime's transport instead of
    /// building a client per request.
    pub oidc: Option<Arc<OidcRuntime>>,
    /// Resource-server JWT validator for IdP-issued Bearer access tokens, or
    /// `None` when resource-server JWT validation is not configured
    /// ([`crate::config::Config::resource_server_configured`]). Built once
    /// at startup in [`crate::auth::jwt::init_jwt_validator`]; `Arc`-wrapped
    /// because [`JwtValidator`] itself holds no cheap-clone guarantee.
    pub jwt_validator: Option<Arc<JwtValidator>>,
    /// Per-source (per-IP) login rate limiter, shared across the login,
    /// forgot-password, and reset-password handlers. Built once at startup from
    /// `config.login_rate_per_min`. The IP-independent per-account backoff lives
    /// in the `local_login_throttle` table, not here.
    pub login_limiter: Arc<LoginLimiter>,
    /// Operator-tunable settings loaded from the `settings` table.
    /// Refreshed via LISTEN/NOTIFY + 60s fallback poll. Handlers read
    /// via `state.settings.read().await`.
    pub settings: Arc<RwLock<Settings>>,
    /// Timestamp of last successful background reload of the settings
    /// cache. `None` until the first LISTEN/NOTIFY refresh completes;
    /// the initial `load()` at startup does not count. Exposed in
    /// `GET /api/v1/settings` so operators can verify the live-reload
    /// mechanism is healthy.
    pub last_settings_reload: Arc<RwLock<Option<DateTime<Utc>>>>,
}
