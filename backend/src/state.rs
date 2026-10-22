//! Process-wide state shared across handlers, middleware, and background
//! workers.
//!
//! [`AppState`] is `Clone`. Each field is cheaply cloneable: `PgPool`
//! is `Arc`-backed internally; [`Config`] is owned data (`Clone`-derived,
//! mostly `String` / `Vec` / nested config structs); [`OidcRuntime`] is
//! `Arc`-wrapped so every clone shares provider snapshots and one HTTP
//! connection pool. It is built once during [`crate::run`] and threaded
//! through Axum's `with_state`, the auth/session layers, the ingestion
//! watcher, the enrichment queue, and (read-only) the writeback worker.

use std::sync::Arc;

use sqlx::PgPool;
use tokio::sync::RwLock;

use chrono::{DateTime, Utc};

use crate::auth::oidc::OidcRuntime;
use crate::auth::rate_limit::LoginLimiter;
use crate::config::Config;
use crate::models::settings::Settings;
use crate::services::files::LibraryFiles;

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
    /// Shared discovery command handle for the ingestion owner.
    pub ingestion: crate::services::ingestion::CoordinatorHandle,
    /// Resolved configuration loaded once at startup. Includes finalised
    /// CSP `HeaderValue`s on `config.security` (built in `run` before this
    /// state is constructed).
    pub config: Config,
    /// Immutable library and staging authority acquired before serving.
    pub library_files: LibraryFiles,
    /// Configured interactive OIDC runtime, including its optional current provider
    /// snapshot and shared transport. `None` means OIDC is unconfigured; an empty
    /// snapshot means discovery is pending. Requests share key-refresh work.
    pub oidc: Option<Arc<OidcRuntime>>,
    /// Source allowance for password login, setup and registration.
    pub login_limiter: Arc<LoginLimiter>,
    /// Independent source allowance for password recovery.
    pub recovery_limiter: Arc<LoginLimiter>,
    /// Independent source allowance for OIDC start navigations.
    pub oidc_limiter: Arc<LoginLimiter>,
    /// Shared request-path password and recovery-PIN hashing capacity.
    pub hash_limiter: crate::auth::password::HashLimiter,
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
    /// Authentication mail runtime. `None` when SMTP is disabled: nothing is
    /// queued and no submission worker runs.
    pub mail: Option<Arc<crate::services::mail::queue::MailRuntime>>,
}
