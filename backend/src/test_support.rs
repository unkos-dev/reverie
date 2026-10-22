use axum::Router;
use axum_test::TestServer;

use crate::config::{
    Config, CoverConfig, EnrichmentConfig, OpdsConfig, SecurityConfig, WritebackConfig,
};
use crate::models::manifestation_format::ManifestationFormat;
use crate::state::AppState;

pub fn test_library_files_at(
    library: &crate::config::AbsoluteRootPath,
    id: crate::models::storage_library::LibraryId,
) -> crate::services::files::LibraryFiles {
    test_storage_config(Some(library.as_path()), id).1
}

pub fn test_storage_config(
    library: Option<&std::path::Path>,
    id: crate::models::storage_library::LibraryId,
) -> (Config, crate::services::files::LibraryFiles) {
    let staging = tempfile::tempdir().unwrap();
    let ingestion = staging.path().join("ingestion");
    std::fs::create_dir(&ingestion).unwrap();
    let library = library.map_or_else(
        || {
            let path = staging.path().join("library");
            std::fs::create_dir(&path).unwrap();
            path
        },
        std::path::Path::to_owned,
    );
    let mut config = test_config();
    config.library_path = library.to_str().unwrap().parse().unwrap();
    config.ingestion_path = ingestion.to_str().unwrap().parse().unwrap();
    let files = crate::services::files::LibraryFiles::open(
        [(id, config.library_path.clone())],
        &config.ingestion_path,
    )
    .unwrap()
    .with_fixtures(vec![staging]);
    (config, files)
}

pub fn test_config() -> Config {
    Config {
        port: 3000,
        database_url: String::new().into(),
        library_path: crate::config::Config::default().library_path,
        ingestion_path: crate::config::Config::default().ingestion_path,
        log_level: "info".into(),
        db_max_connections: 10,
        oidc_issuer_url: String::new(),
        oidc_client_id: String::new(),
        oidc_client_secret: String::new().into(),
        oidc_redirect_uri: String::new(),
        local_auth_enabled: true,
        login_rate_per_min: 10,
        auth_hash_concurrency: 10,
        password_min_length: 15,
        password_max_length: 256,
        password_min_zxcvbn_score: 2,
        // Off in tests so endpoint tests exercise length + zxcvbn deterministically
        // without an outbound HIBP call; the breach path is unit-tested against a
        // wiremock loopback in auth::password_policy.
        password_breach_check_enabled: false,
        self_registration_enabled: false,
        recovery_pin_ttl_secs: 900,
        recovery_pin_dir: "./reverie-recovery".into(),
        trusted_client_ip_header: None,
        migration_database_url: None,
        auto_migrate: false,
        ingestion_database_url: String::new().into(),
        accepted_formats: vec![ManifestationFormat::Epub],
        cleanup_imported: true,
        cleanup_duplicates: false,
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
        writeback: WritebackConfig {
            enabled: false,
            concurrency: 1,
            poll_idle_secs: 5,
            max_attempts: 3,
        },
        opds: OpdsConfig {
            enabled: false,
            page_size: 50,
            realm: "Reverie OPDS".into(),
        },
        security: SecurityConfig {
            behind_https: false,
            hsts_include_subdomains: false,
            hsts_preload: false,
            csp_report_endpoint: None,
            frontend_dist_path: None,
            csp_html_header: None,
            csp_api_header: Some(axum::http::HeaderValue::from_static(
                "default-src 'none'; frame-ancestors 'none'; base-uri 'none'",
            )),
        },
        googlebooks_api_key: None,
        hardcover_api_token: None,
        operator_contact: None,
        public_url: Some(url::Url::parse("http://localhost:3000").unwrap()),
        mail: crate::config::MailConfig::default(),
    }
}

pub fn test_oidc_runtime() -> crate::auth::oidc::OidcRuntime {
    use openidconnect::core::{CoreProviderMetadata, CoreResponseType, CoreSubjectIdentifierType};
    use openidconnect::{
        AuthUrl, EmptyAdditionalProviderMetadata, IssuerUrl, JsonWebKeySetUrl, ResponseTypes,
        TokenUrl,
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

    let mut config = test_config();
    config.oidc_issuer_url = "https://fake-issuer.example.com".into();
    config.oidc_client_id = "test-client".into();
    config.oidc_client_secret = "test-secret".into();
    config.oidc_redirect_uri = "http://localhost:3000/auth/callback".into();
    let transport = crate::auth::oidc::OidcTransport::for_tests();
    crate::auth::oidc::OidcRuntime::new(
        crate::auth::oidc::OidcConfiguration::parse(&config, &transport).unwrap(),
        provider,
        transport,
    )
}

pub fn test_login_limiter() -> std::sync::Arc<crate::auth::rate_limit::LoginLimiter> {
    // A high per-minute quota so fixtures are never throttled by the limiter.
    crate::auth::rate_limit::build_login_limiter(std::num::NonZeroU32::new(1000).expect("nonzero"))
}

pub fn test_settings() -> std::sync::Arc<tokio::sync::RwLock<crate::models::settings::Settings>> {
    use crate::models::settings::Settings;
    std::sync::Arc::new(tokio::sync::RwLock::new(Settings {
        enrichment_enabled: true,
        enrichment_concurrency: 2,
        enrichment_poll_idle_secs: 30,
        enrichment_fetch_budget_secs: 15,
        cover_max_bytes: 10_485_760,
        cover_download_timeout_secs: 30,
        cover_min_long_edge_px: 1000,
        cover_redirect_limit: 3,
        writeback_enabled: true,
        writeback_concurrency: 2,
        writeback_poll_idle_secs: 5,
        writeback_max_attempts: 3,
        opds_enabled: true,
        opds_page_size: 50,
        ingestion: crate::models::settings::IngestionSettings {
            ingestion_enabled: true,
            accepted_formats: vec!["epub".into()],
            cleanup_imported: true,
            cleanup_duplicates: false,
        },
        provider_visibility: serde_json::json!({}),
        revision: 0,
        updated_at: chrono::Utc::now(),
    }))
}

pub fn test_state() -> AppState {
    let (config, library_files) = test_storage_config(
        None,
        crate::models::storage_library::LibraryId::from_uuid(uuid::Uuid::new_v4()),
    );
    AppState {
        pool: sqlx::PgPool::connect_lazy("postgres://invalid").unwrap(),
        ingestion_pool: sqlx::PgPool::connect_lazy("postgres://invalid").unwrap(),
        ingestion: crate::services::ingestion::coordinator_channel().0,
        library_files,
        hash_limiter: crate::auth::password::HashLimiter::new(
            usize::try_from(config.auth_hash_concurrency).unwrap(),
        ),
        config,
        oidc: Some(std::sync::Arc::new(test_oidc_runtime())),
        login_limiter: test_login_limiter(),
        recovery_limiter: test_login_limiter(),
        oidc_limiter: test_login_limiter(),
        settings: test_settings(),
        last_settings_reload: std::sync::Arc::new(tokio::sync::RwLock::new(None)),
        mail: None,
    }
}

#[tokio::test]
async fn test_state_storage_roots_match_config_and_survive_cloning() {
    let state = test_state();
    state
        .library_files
        .ingestion()
        .write("marker", b"fixture bytes")
        .unwrap();
    assert_eq!(
        std::fs::read(state.config.ingestion_path.as_path().join("marker")).unwrap(),
        b"fixture bytes"
    );
    let roots = [
        state.config.library_path.as_path().to_owned(),
        state.config.ingestion_path.as_path().to_owned(),
    ];
    let clone = state.clone();
    drop(state);
    assert!(roots.iter().all(|root| root.is_dir()));
    drop(clone);
    assert!(roots.iter().all(|root| !root.exists()));
}

#[test]
fn test_storage_config_preserves_caller_owned_library() {
    let library = tempfile::tempdir().unwrap();
    let id = crate::models::storage_library::LibraryId::from_uuid(uuid::Uuid::new_v4());
    let (config, files) = test_storage_config(Some(library.path()), id);
    files
        .library(id)
        .unwrap()
        .write("marker", b"library bytes")
        .unwrap();
    assert_eq!(
        std::fs::read(config.library_path.as_path().join("marker")).unwrap(),
        b"library bytes"
    );
    let staging = config.ingestion_path.as_path().to_owned();
    drop(files);
    assert!(!staging.exists());
    assert_eq!(
        std::fs::read(library.path().join("marker")).unwrap(),
        b"library bytes"
    );
}

/// Build the full application router with auth layer (for route integration tests).
///
/// Uses `MemoryStore` for sessions even though production uses
/// `PostgresStore` — `test_state` deliberately wires a no-DB pool
/// (`postgres://invalid` via `connect_lazy`), so a session backend that
/// touches the pool would 500 every request that issues a session.
/// Tests that need the live `PostgresStore` wiring use `#[sqlx::test]`
/// and `build_router_with_session_store` with a real pool.
pub fn test_server() -> TestServer {
    let state = test_state();
    let app: Router =
        crate::build_router_with_session_store(state, tower_sessions::MemoryStore::default());
    TestServer::new(app)
}

/// Assert that an `axum-test` response carries an RFC 9457 Problem
/// Details body with the expected status, problem-type slug, and
/// `application/problem+json` Content-Type.
///
/// Use in tests that previously asserted `body["error"] == "..."`
/// against the pre-Problem-Details envelope. The helper checks the four
/// must-be-present fields (`type`, `title`, `status`, `detail`) and
/// returns the parsed body so callers can drill into the
/// caller-supplied `detail` when needed.
///
/// ```ignore
/// let r = server.get("/api/v1/__nope__").await;
/// let problem = test_support::assert_problem(
///     &r,
///     crate::error::problems::NOT_FOUND,
///     axum::http::StatusCode::NOT_FOUND,
/// );
/// assert_eq!(problem["title"], "Not Found");
/// ```
///
/// # Panics
///
/// Panics when the response does not match the expected RFC 9457
/// contract: HTTP status differs from `status`; Content-Type missing
/// or not `application/problem+json`; body fails to parse as JSON;
/// required fields (`type`, `title`, `status`, `detail`) absent; or
/// `type` URI does not end in `/{type_slug}`. Intended for test code
/// only — the panic shape is the assertion-failure surface.
pub fn assert_problem(
    response: &axum_test::TestResponse,
    type_slug: &str,
    status: axum::http::StatusCode,
) -> serde_json::Value {
    assert_eq!(
        response.status_code(),
        status,
        "expected HTTP {status}, got {}",
        response.status_code(),
    );
    let content_type = response
        .headers()
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_owned();
    assert!(
        content_type.contains("application/problem+json"),
        "expected application/problem+json Content-Type, got: {content_type}",
    );
    let body: serde_json::Value = response.json();
    let typ = body["type"]
        .as_str()
        .expect("RFC 9457 `type` field present");
    assert!(
        typ.ends_with(&format!("/{type_slug}")),
        "expected `type` ending in /{type_slug}, got {typ}",
    );
    assert_eq!(
        body["status"]
            .as_u64()
            .expect("RFC 9457 `status` field present"),
        u64::from(status.as_u16()),
        "RFC 9457 `status` field must match HTTP status",
    );
    assert!(
        body["title"].as_str().is_some(),
        "RFC 9457 `title` field present, got: {body}",
    );
    assert!(
        body["detail"].as_str().is_some(),
        "RFC 9457 `detail` field present, got: {body}",
    );
    body
}

/// Assert a JSON field carries the published timestamp contract, a
/// `Z`-terminated RFC 3339 string, and return it parsed.
///
/// The parse is load-bearing rather than decorative. A JSON string is
/// not by itself the contract: a space-separated or offset-suffixed
/// rendering is still a string, so `is_string()` or `!is_null()`
/// cannot tell a conforming response from a broken one. The `Z` check
/// and the RFC 3339 parse together are what pin the published shape.
///
/// # Panics
///
/// Panics when `field` is absent, is not a JSON string, does not carry
/// the UTC `Z` designator, or does not parse as RFC 3339. Intended for
/// test code only, where the panic is the assertion-failure surface.
pub fn assert_rfc3339(body: &serde_json::Value, field: &str) -> chrono::DateTime<chrono::Utc> {
    let raw = body[field]
        .as_str()
        .unwrap_or_else(|| panic!("`{field}` must be a JSON string, got: {}", body[field]));
    assert!(
        raw.ends_with('Z'),
        "`{field}` must be UTC and `Z`-terminated, got: {raw}",
    );
    chrono::DateTime::parse_from_rfc3339(raw)
        .unwrap_or_else(|e| panic!("`{field}` must be RFC 3339, got {raw}: {e}"))
        .with_timezone(&chrono::Utc)
}

/// Wait for matching requests to reach a database lock in the isolated test database.
pub async fn wait_for_blocked_query(pool: &sqlx::PgPool, query_pattern: &str, count: i64) {
    tokio::time::timeout(std::time::Duration::from_secs(30), async {
        loop {
            let blocked = sqlx::query_scalar!(
                r#"SELECT COUNT(*) AS "blocked!" FROM pg_stat_activity
                 WHERE datname = current_database() AND query LIKE $1
                 AND cardinality(pg_blocking_pids(pid)) > 0"#,
                query_pattern,
            )
            .fetch_one(pool)
            .await
            .unwrap();
            if blocked >= count {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("matching requests must reach their database lock");
}

/// Real-DB helpers for tests that exercise the live schema + RLS policies.
///
/// Tests use `#[sqlx::test(migrations = "./migrations")]`, which provisions
/// an isolated database per test and injects a `PgPool` owned by the
/// schema owner (`reverie` — bypasses RLS). Tests that need to exercise
/// the runtime roles (`reverie_app` / `reverie_ingestion`) build secondary
/// pools against the same per-test DB via [`app_pool_for`] / [`ingestion_pool_for`].
pub mod db {
    use base64ct::Encoding as _;
    use sqlx::PgPool;
    use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
    use uuid::Uuid;

    fn password_input(value: Option<&str>) -> Result<&str, &'static str> {
        value
            .filter(|value| !value.is_empty())
            .ok_or("a nonempty role password is required")
    }

    pub fn required_password(name: &str) -> String {
        let value = std::env::var(name).ok();
        password_input(value.as_deref())
            .unwrap_or_else(|_| panic!("{name} requires a nonempty password"))
            .to_owned()
    }

    /// Build a `reverie_app` pool against the same DB as the given pool.
    /// Use this when a test needs RLS-enforced access (the runtime web role).
    pub async fn app_pool_for(pool: &PgPool) -> PgPool {
        let password = required_password("REVERIE_APP_PASSWORD");
        pool_as_role(pool, "reverie_app", &password, false).await
    }

    /// Build a writeback-worker pool against the same DB as the given pool.
    /// Connects as `reverie_app` with `app.system_context = 'writeback'` set
    /// session-scoped on every connection — mirrors `db::init_writeback_pool`.
    /// Use this for tests that exercise writeback orchestrator/queue code
    /// paths against `manifestations` (which has system-context RLS policies).
    pub async fn writeback_pool_for(pool: &PgPool) -> PgPool {
        let password = required_password("REVERIE_APP_PASSWORD");
        pool_as_role(pool, "reverie_app", &password, true).await
    }

    /// Build a `reverie_ingestion` pool against the same DB as the given pool.
    /// Use this for fixture inserts on pipeline tables (manifestations, works)
    /// where the `*_ingestion_full_access` RLS policies apply.
    pub async fn ingestion_pool_for(pool: &PgPool) -> PgPool {
        let password = required_password("REVERIE_INGESTION_PASSWORD");
        pool_as_role(pool, "reverie_ingestion", &password, false).await
    }

    /// Build a `reverie_readonly` pool against the same DB as the given pool.
    /// Use this to prove read-only RLS access and write denial on tables the
    /// readonly reporting role only holds `SELECT` on.
    pub async fn readonly_pool_for(pool: &PgPool) -> PgPool {
        let password = required_password("REVERIE_READONLY_PASSWORD");
        pool_as_role(pool, "reverie_readonly", &password, false).await
    }

    /// Rebuild connection options against the same database as `opts`, as a
    /// different role. Carries the transport across: a socket-backed source
    /// yields socket-backed options, because rebuilding from host and port
    /// alone would silently aim a socket-DSN test run at TCP.
    fn role_opts_from(opts: &PgConnectOptions, username: &str, password: &str) -> PgConnectOptions {
        let mut new_opts = PgConnectOptions::new()
            .host(opts.get_host())
            .port(opts.get_port())
            .database(
                opts.get_database()
                    .expect("injected pool has database name"),
            )
            .username(username)
            .password(password);
        if let Some(socket) = opts.get_socket() {
            new_opts = new_opts.socket(socket);
        }
        new_opts
    }

    async fn pool_as_role(
        pool: &PgPool,
        username: &str,
        password: &str,
        writeback_context: bool,
    ) -> PgPool {
        let new_opts = {
            let opts = pool.connect_options();
            role_opts_from(&opts, username, password)
        };
        let mut builder = PgPoolOptions::new().max_connections(5);
        if writeback_context {
            builder = builder.after_connect(|conn, _meta| {
                Box::pin(async move {
                    sqlx::query("SELECT set_config('app.system_context', 'writeback', false)")
                        .execute(conn)
                        .await?;
                    Ok(())
                })
            });
        }
        builder
            .connect_with(new_opts)
            .await
            .unwrap_or_else(|e| panic!("connect as role failed: {e}"))
    }

    /// Insert an author row and link it to `work_id` with the given role and
    /// position, bypassing the PATCH surface and the sort-column refresh.
    /// Returns the author id.
    pub async fn insert_contributor(
        pool: &PgPool,
        work_id: Uuid,
        name: &str,
        role: &str,
        position: i32,
    ) -> Uuid {
        let author_id: Uuid = sqlx::query_scalar!(
            "INSERT INTO authors (name, sort_name) VALUES ($1, $1) RETURNING id",
            name,
        )
        .fetch_one(pool)
        .await
        .expect("insert author");
        sqlx::query!(
            "INSERT INTO work_authors (work_id, author_id, role, position) \
             VALUES ($1, $2, ($3::text)::author_role, $4)",
            work_id,
            author_id,
            role,
            position,
        )
        .execute(pool)
        .await
        .expect("insert work_authors");
        author_id
    }

    /// Insert an admin-role user via `reverie_app` (the only role with grants
    /// on `users`), mint a device token, and return
    /// `(user_id, "Basic ...")` ready for use as an `Authorization` header.
    pub async fn create_admin_and_basic_auth(app_pool: &PgPool) -> (Uuid, String) {
        let subject = format!("admin-test-{}", Uuid::new_v4());
        // Auto-promotion is retired; grant admin explicitly. `admin` is not a
        // child role, so this respects chk_child_role_sync.
        let user = crate::models::user::upsert_from_oidc(
            app_pool,
            "https://test-issuer.example.com",
            &subject,
            "Admin Test",
            None,
        )
        .await
        .expect("upsert user");
        sqlx::query!(
            "UPDATE users SET role = 'admin'::user_role WHERE id = $1",
            user.id,
        )
        .execute(app_pool)
        .await
        .expect("promote to admin");
        let (plaintext, hash) = crate::auth::token::generate_device_token();
        // Mint with the full role-derived scope set (not the create() default
        // `{read}`) so this token authenticates like the user's own session —
        // scope-gated mutation tests would otherwise 403 on every write.
        let token = crate::models::device_token::create_with_limit(
            app_pool,
            user.id,
            "admin-test",
            &hash,
            crate::auth::scope::Scope::for_role(crate::models::role::Role::Admin),
            None,
        )
        .await
        .expect("create token");

        let basic = base64ct::Base64::encode_string(
            format!(
                "{}{}:{}",
                crate::auth::token::TOKEN_PREFIX,
                token.id,
                plaintext
            )
            .as_bytes(),
        );
        (user.id, format!("Basic {basic}"))
    }

    /// Build the full router with both pools wired through `AppState`.
    /// AppState.pool comes from `app_pool` (`reverie_app` — for the route
    /// handlers' `acquire_with_rls`); `AppState.ingestion_pool` comes from
    /// `ingestion_pool` (`reverie_ingestion` — matches the queue + `dry_run`).
    ///
    /// This routes through `crate::build_router`, so sessions are backed by
    /// `PostgresStore` against `app_pool`. The `tower_sessions.session` table
    /// must therefore exist in the per-test DB — `#[sqlx::test(migrations =
    /// "./migrations")]` ensures that. Callers needing in-process session
    /// state (OIDC nonce readback) should build the router via
    /// `crate::build_router_with_session_store(..., MemoryStore)` instead.
    pub fn server_with_real_pools(
        app_pool: &PgPool,
        ingestion_pool: &PgPool,
    ) -> axum_test::TestServer {
        server_with_config(app_pool, ingestion_pool, super::test_config())
    }

    pub fn server_with_config(
        app_pool: &PgPool,
        ingestion_pool: &PgPool,
        config: crate::config::Config,
    ) -> axum_test::TestServer {
        server_with_reload_state(app_pool, ingestion_pool, config, None, None)
    }

    /// Same as [`server_with_config`] with the authentication mail runtime wired in.
    pub fn server_with_mail(
        app_pool: &PgPool,
        ingestion_pool: &PgPool,
        config: crate::config::Config,
        mail: std::sync::Arc<crate::services::mail::queue::MailRuntime>,
    ) -> axum_test::TestServer {
        server_with_reload_state(app_pool, ingestion_pool, config, None, Some(mail))
    }

    /// Same as [`server_with_real_pools`] with the live-reload task's last
    /// successful reload already recorded as `at`.
    pub fn server_with_last_reload(
        app_pool: &PgPool,
        ingestion_pool: &PgPool,
        at: chrono::DateTime<chrono::Utc>,
    ) -> axum_test::TestServer {
        server_with_reload_state(
            app_pool,
            ingestion_pool,
            super::test_config(),
            Some(at),
            None,
        )
    }

    fn server_with_reload_state(
        app_pool: &PgPool,
        ingestion_pool: &PgPool,
        mut config: crate::config::Config,
        last_reload: Option<chrono::DateTime<chrono::Utc>>,
        mail: Option<std::sync::Arc<crate::services::mail::queue::MailRuntime>>,
    ) -> axum_test::TestServer {
        use crate::state::AppState;
        let (storage_config, library_files) = crate::test_support::test_storage_config(
            None,
            crate::models::storage_library::LibraryId::from_uuid(uuid::Uuid::new_v4()),
        );
        config.library_path = storage_config.library_path;
        config.ingestion_path = storage_config.ingestion_path;
        let state = AppState {
            pool: app_pool.clone(),
            ingestion_pool: ingestion_pool.clone(),
            ingestion: crate::services::ingestion::coordinator_channel().0,
            library_files,
            hash_limiter: crate::auth::password::HashLimiter::new(
                usize::try_from(config.auth_hash_concurrency).unwrap(),
            ),
            config,
            oidc: Some(std::sync::Arc::new(super::test_oidc_runtime())),
            login_limiter: super::test_login_limiter(),
            recovery_limiter: super::test_login_limiter(),
            oidc_limiter: super::test_login_limiter(),
            settings: super::test_settings(),
            last_settings_reload: std::sync::Arc::new(tokio::sync::RwLock::new(last_reload)),
            mail,
        };
        let app = crate::build_router(state);
        axum_test::TestServer::new(app)
    }

    /// Same as [`server_with_real_pools`] but with self-registration enabled, for
    /// `/auth/register` tests (the base `test_config()` has it off to match the
    /// shipped default).
    pub fn server_with_self_registration(
        app_pool: &PgPool,
        ingestion_pool: &PgPool,
    ) -> axum_test::TestServer {
        use crate::state::AppState;
        let (mut config, library_files) = super::test_storage_config(
            None,
            crate::models::storage_library::LibraryId::from_uuid(uuid::Uuid::new_v4()),
        );
        config.self_registration_enabled = true;
        let state = AppState {
            pool: app_pool.clone(),
            ingestion_pool: ingestion_pool.clone(),
            ingestion: crate::services::ingestion::coordinator_channel().0,
            library_files,
            hash_limiter: crate::auth::password::HashLimiter::new(
                usize::try_from(config.auth_hash_concurrency).unwrap(),
            ),
            config,
            oidc: Some(std::sync::Arc::new(super::test_oidc_runtime())),
            login_limiter: super::test_login_limiter(),
            recovery_limiter: super::test_login_limiter(),
            oidc_limiter: super::test_login_limiter(),
            settings: super::test_settings(),
            last_settings_reload: std::sync::Arc::new(tokio::sync::RwLock::new(None)),
            mail: None,
        };
        let app = crate::build_router(state);
        axum_test::TestServer::new(app)
    }

    /// Same as [`server_with_real_pools`] but with a per-source limit of
    /// `per_min` and `X-Forwarded-For` trusted for the client IP.
    ///
    /// Both are needed to exercise a 429 through the router: the base
    /// `test_config()` trusts no header, and `PeerAddr` is empty under the test
    /// harness, so `client_ip` resolves to `None` and the limiter is never
    /// consulted. Send an `X-Forwarded-For` header from the test to key it.
    pub fn server_with_source_rate_limit(
        app_pool: &PgPool,
        ingestion_pool: &PgPool,
        per_min: u32,
    ) -> axum_test::TestServer {
        use crate::state::AppState;
        let (mut config, library_files) = super::test_storage_config(
            None,
            crate::models::storage_library::LibraryId::from_uuid(uuid::Uuid::new_v4()),
        );
        config.trusted_client_ip_header = Some("x-forwarded-for".to_owned());
        let state = AppState {
            pool: app_pool.clone(),
            ingestion_pool: ingestion_pool.clone(),
            ingestion: crate::services::ingestion::coordinator_channel().0,
            library_files,
            hash_limiter: crate::auth::password::HashLimiter::new(
                usize::try_from(config.auth_hash_concurrency).unwrap(),
            ),
            config,
            oidc: Some(std::sync::Arc::new(super::test_oidc_runtime())),
            login_limiter: crate::auth::rate_limit::build_login_limiter(
                std::num::NonZeroU32::new(per_min).expect("per_min must be non-zero"),
            ),
            recovery_limiter: crate::auth::rate_limit::build_login_limiter(
                std::num::NonZeroU32::new(per_min).expect("per_min must be non-zero"),
            ),
            oidc_limiter: crate::auth::rate_limit::build_login_limiter(
                std::num::NonZeroU32::new(per_min).expect("per_min must be non-zero"),
            ),
            settings: super::test_settings(),
            last_settings_reload: std::sync::Arc::new(tokio::sync::RwLock::new(None)),
            mail: None,
        };
        let app = crate::build_router(state);
        axum_test::TestServer::new(app)
    }

    /// Same as [`server_with_real_pools`] but with OPDS enabled. Tests that
    /// exercise `/opds/*` routes need this — the base `test_config()` has
    /// `opds.enabled = false` to match ordinary route tests.
    ///
    /// `library_path` is the absolute path to a real directory (usually a
    /// `tempfile::TempDir`) — the download handler opens
    /// `file_path` through this root's library capability.
    pub async fn server_with_opds_enabled(
        app_pool: &PgPool,
        ingestion_pool: &PgPool,
        library_path: &std::path::Path,
    ) -> axum_test::TestServer {
        use crate::config::OpdsConfig;
        use crate::state::AppState;

        let (mut config, library_files) = super::test_storage_config(
            Some(library_path),
            crate::models::storage_library::default_library_id(app_pool)
                .await
                .unwrap(),
        );
        config.opds = OpdsConfig {
            enabled: true,
            page_size: 50,
            realm: "Reverie OPDS".into(),
        };
        config.public_url = Some(url::Url::parse("http://host.example/").unwrap());
        let state = AppState {
            pool: app_pool.clone(),
            ingestion_pool: ingestion_pool.clone(),
            ingestion: crate::services::ingestion::coordinator_channel().0,
            library_files,
            hash_limiter: crate::auth::password::HashLimiter::new(
                usize::try_from(config.auth_hash_concurrency).unwrap(),
            ),
            config,
            oidc: Some(std::sync::Arc::new(super::test_oidc_runtime())),
            login_limiter: super::test_login_limiter(),
            recovery_limiter: super::test_login_limiter(),
            oidc_limiter: super::test_login_limiter(),
            settings: super::test_settings(),
            last_settings_reload: std::sync::Arc::new(tokio::sync::RwLock::new(None)),
            mail: None,
        };
        let app = crate::build_router(state);
        axum_test::TestServer::new(app)
    }

    /// Same as [`server_with_real_pools`] but with a caller-chosen
    /// `opds.page_size` (shared page-size knob for every paginated list).
    /// Pagination-walk tests use a tiny page size so two-page walks need
    /// only a handful of fixture rows.
    pub fn server_with_real_pools_page_size(
        app_pool: &PgPool,
        ingestion_pool: &PgPool,
        page_size: u32,
    ) -> axum_test::TestServer {
        use crate::state::AppState;
        let (mut config, library_files) = super::test_storage_config(
            None,
            crate::models::storage_library::LibraryId::from_uuid(uuid::Uuid::new_v4()),
        );
        config.opds.page_size = page_size;
        let state = AppState {
            pool: app_pool.clone(),
            ingestion_pool: ingestion_pool.clone(),
            ingestion: crate::services::ingestion::coordinator_channel().0,
            library_files,
            hash_limiter: crate::auth::password::HashLimiter::new(
                usize::try_from(config.auth_hash_concurrency).unwrap(),
            ),
            config,
            oidc: Some(std::sync::Arc::new(super::test_oidc_runtime())),
            login_limiter: super::test_login_limiter(),
            recovery_limiter: super::test_login_limiter(),
            oidc_limiter: super::test_login_limiter(),
            settings: super::test_settings(),
            last_settings_reload: std::sync::Arc::new(tokio::sync::RwLock::new(None)),
            mail: None,
        };
        let app = crate::build_router(state);
        axum_test::TestServer::new(app)
    }

    /// Same as [`server_with_opds_enabled`] but with a caller-chosen
    /// `opds.page_size` for navigation-feed pagination-walk tests.
    pub async fn server_with_opds_page_size(
        app_pool: &PgPool,
        ingestion_pool: &PgPool,
        library_path: &std::path::Path,
        page_size: u32,
    ) -> axum_test::TestServer {
        use crate::config::OpdsConfig;
        use crate::state::AppState;

        let (mut config, library_files) = super::test_storage_config(
            Some(library_path),
            crate::models::storage_library::default_library_id(app_pool)
                .await
                .unwrap(),
        );
        config.opds = OpdsConfig {
            enabled: true,
            page_size,
            realm: "Reverie OPDS".into(),
        };
        config.public_url = Some(url::Url::parse("http://host.example/").unwrap());
        let state = AppState {
            pool: app_pool.clone(),
            ingestion_pool: ingestion_pool.clone(),
            ingestion: crate::services::ingestion::coordinator_channel().0,
            library_files,
            hash_limiter: crate::auth::password::HashLimiter::new(
                usize::try_from(config.auth_hash_concurrency).unwrap(),
            ),
            config,
            oidc: Some(std::sync::Arc::new(super::test_oidc_runtime())),
            login_limiter: super::test_login_limiter(),
            recovery_limiter: super::test_login_limiter(),
            oidc_limiter: super::test_login_limiter(),
            settings: super::test_settings(),
            last_settings_reload: std::sync::Arc::new(tokio::sync::RwLock::new(None)),
            mail: None,
        };
        let app = crate::build_router(state);
        axum_test::TestServer::new(app)
    }

    /// Insert (work, manifestation) via `reverie_ingestion` for use as
    /// fixture data in route tests.  Returns `(work_id, manifestation_id)`.
    pub async fn insert_work_and_manifestation(
        ingestion_pool: &PgPool,
        marker: &str,
    ) -> (Uuid, Uuid) {
        let work_id: Uuid = sqlx::query_scalar!(
            "INSERT INTO works (title, sort_title) VALUES ('', '') RETURNING id",
        )
        .fetch_one(ingestion_pool)
        .await
        .expect("insert work");
        let file_path = format!("fixtures/admin-test-{marker}.epub");
        let file_hash = format!("admin-test-hash-{marker}");
        let m_id: Uuid = sqlx::query_scalar!(
            "WITH inserted AS (INSERT INTO manifestations \
                (library_id, work_id, format, file_path, ingestion_file_hash, current_file_hash, \
                 file_size_bytes, ingestion_status, validation_status) \
             VALUES ((SELECT id FROM libraries WHERE configuration_key = 'default'), $1, 'epub'::manifestation_format, $2, $3, $3, 1000, \
                     'complete'::ingestion_status, 'clean'::validation_status) \
             RETURNING *), claimed AS (INSERT INTO library_path_claims (library_id, path, manifestation_id) SELECT library_id, file_path, id FROM inserted) SELECT id AS \"id!\" FROM inserted",
            work_id,
            file_path,
            file_hash,
        )
        .fetch_one(ingestion_pool)
        .await
        .expect("insert manifestation");
        (work_id, m_id)
    }

    /// Insert a `role='child', is_child=TRUE` user via `reverie_app`, mint a
    /// device token, and return `(user_id, "Basic …")` ready for use as an
    /// `Authorization` header.
    pub async fn create_child_user_and_basic_auth(app_pool: &PgPool, name: &str) -> (Uuid, String) {
        let subject = format!("child-test-{}-{}", name, Uuid::new_v4());
        let user = crate::models::user::upsert_from_oidc(
            app_pool,
            "https://test-issuer.example.com",
            &subject,
            name,
            None,
        )
        .await
        .expect("upsert user");
        sqlx::query!(
            "UPDATE users SET role = 'child'::user_role, is_child = TRUE WHERE id = $1",
            user.id,
        )
        .execute(app_pool)
        .await
        .expect("demote to child");
        let (plaintext, hash) = crate::auth::token::generate_device_token();
        // Role-derived scope set, not create()'s `{read}` default — see the
        // admin helper above for why.
        let token = crate::models::device_token::create_with_limit(
            app_pool,
            user.id,
            "child-test",
            &hash,
            crate::auth::scope::Scope::for_role(crate::models::role::Role::Child),
            None,
        )
        .await
        .expect("create token");

        let basic = base64ct::Base64::encode_string(
            format!(
                "{}{}:{}",
                crate::auth::token::TOKEN_PREFIX,
                token.id,
                plaintext
            )
            .as_bytes(),
        );
        (user.id, format!("Basic {basic}"))
    }

    /// Insert an `adult`-role user via `reverie_app` (keeps default role),
    /// mint a device token, return `(user_id, "Basic …")`.
    pub async fn create_adult_and_basic_auth(app_pool: &PgPool, name: &str) -> (Uuid, String) {
        let subject = format!("adult-test-{}-{}", name, Uuid::new_v4());
        // No auto-promotion to undo: a fresh upsert defaults to the adult role.
        let user = crate::models::user::upsert_from_oidc(
            app_pool,
            "https://test-issuer.example.com",
            &subject,
            name,
            None,
        )
        .await
        .expect("upsert user");
        let (plaintext, hash) = crate::auth::token::generate_device_token();
        // Role-derived scope set, not create()'s `{read}` default — see the
        // admin helper above for why.
        let token = crate::models::device_token::create_with_limit(
            app_pool,
            user.id,
            "adult-test",
            &hash,
            crate::auth::scope::Scope::for_role(crate::models::role::Role::Adult),
            None,
        )
        .await
        .expect("create token");

        let basic = base64ct::Base64::encode_string(
            format!(
                "{}{}:{}",
                crate::auth::token::TOKEN_PREFIX,
                token.id,
                plaintext
            )
            .as_bytes(),
        );
        (user.id, format!("Basic {basic}"))
    }

    /// Insert an `adult`-role user with an email and a local password
    /// credential (Argon2id hash of `password`). Returns the user id. For
    /// `/auth/local/login` tests.
    pub async fn create_adult_with_password(
        app_pool: &PgPool,
        name: &str,
        email: &str,
        password: &str,
    ) -> Uuid {
        let subject = format!("local-test-{}-{}", name, Uuid::new_v4());
        let user = crate::models::user::upsert_from_oidc(
            app_pool,
            "https://test-issuer.example.com",
            &subject,
            name,
            Some(email),
        )
        .await
        .expect("upsert user");
        let phc = crate::auth::password::hash_password(password.as_bytes()).expect("hash password");
        crate::models::local_credentials::set_password(app_pool, user.id, &phc)
            .await
            .expect("set password");
        user.id
    }

    pub async fn create_shelf(app_pool: &PgPool, user_id: Uuid, name: &str) -> Uuid {
        sqlx::query_scalar!(
            "INSERT INTO shelves (user_id, name) VALUES ($1, $2) RETURNING id",
            user_id,
            name,
        )
        .fetch_one(app_pool)
        .await
        .expect("create shelf")
    }

    pub async fn add_to_shelf(app_pool: &PgPool, shelf_id: Uuid, manifestation_id: Uuid) {
        sqlx::query!(
            "INSERT INTO shelf_items (shelf_id, manifestation_id) \
             VALUES ($1, $2) ON CONFLICT DO NOTHING",
            shelf_id,
            manifestation_id,
        )
        .execute(app_pool)
        .await
        .expect("shelf_items insert");
    }

    /// Build a minimal valid EPUB with a 2×2 JPEG cover manifested at
    /// `OEBPS/cover.jpg` with manifest id `cover-image`. Mirrors the cover
    /// extraction path.
    ///
    /// Tests that create multiple fixtures in the same DB must pass a unique
    /// `marker` — it's embedded as a ZIP entry so the resulting SHA-256 is
    /// unique and doesn't collide with `manifestations.file_hash_unique`.
    pub fn make_minimal_epub_with_cover_tagged(marker: &str) -> Vec<u8> {
        use std::io::Write as _;
        use zip::write::{ExtendedFileOptions, FileOptions};

        // 2x2 JPEG bytes via the image crate.
        let cover_bytes = {
            let img = image::DynamicImage::new_rgb8(2, 2);
            let mut buf = Vec::new();
            img.write_to(
                &mut std::io::Cursor::new(&mut buf),
                image::ImageFormat::Jpeg,
            )
            .expect("encode jpeg");
            buf
        };

        let buf = std::io::Cursor::new(Vec::new());
        let mut w = zip::ZipWriter::new(buf);

        let stored: FileOptions<ExtendedFileOptions> =
            FileOptions::default().compression_method(zip::CompressionMethod::Stored);
        w.start_file("mimetype", stored).unwrap();
        w.write_all(b"application/epub+zip").unwrap();

        let default: FileOptions<ExtendedFileOptions> = FileOptions::default();

        w.start_file("META-INF/container.xml", default.clone())
            .unwrap();
        w.write_all(
            br#"<?xml version="1.0" encoding="UTF-8"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles>
    <rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/>
  </rootfiles>
</container>"#,
        )
        .unwrap();

        w.start_file("OEBPS/content.opf", default.clone()).unwrap();
        w.write_all(
            br#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0">
  <metadata/>
  <manifest>
    <item id="cover-image" href="cover.jpg" media-type="image/jpeg"/>
  </manifest>
  <spine/>
</package>"#,
        )
        .unwrap();

        w.start_file("OEBPS/cover.jpg", default.clone()).unwrap();
        w.write_all(&cover_bytes).unwrap();

        // Uniqueness tag — lets the same helper produce distinct bytes (and
        // thus distinct SHA-256 hashes) per call site.
        w.start_file("META-INF/reverie-marker.txt", default)
            .unwrap();
        w.write_all(marker.as_bytes()).unwrap();

        w.finish().unwrap().into_inner()
    }

    /// Build a minimal EPUB whose cover is an SVG at `OEBPS/cover.svg`
    /// (manifest id `cover-image`, `media-type="image/svg+xml"`,
    /// `properties="cover-image"`) — the Standard Ebooks shape. `svg_body` is
    /// the raw SVG bytes; when `sibling_jpeg` is set, an `OEBPS/cover.jpg` entry
    /// is added so an `<image href="cover.jpg">` reference resolves. `marker`
    /// makes the archive bytes (and thus the SHA-256) unique per call site.
    fn build_svg_cover_epub(marker: &str, svg_body: &[u8], sibling_jpeg: bool) -> Vec<u8> {
        use std::io::Write as _;
        use zip::write::{ExtendedFileOptions, FileOptions};

        let buf = std::io::Cursor::new(Vec::new());
        let mut w = zip::ZipWriter::new(buf);

        let stored: FileOptions<ExtendedFileOptions> =
            FileOptions::default().compression_method(zip::CompressionMethod::Stored);
        w.start_file("mimetype", stored).unwrap();
        w.write_all(b"application/epub+zip").unwrap();

        let default: FileOptions<ExtendedFileOptions> = FileOptions::default();

        w.start_file("META-INF/container.xml", default.clone())
            .unwrap();
        w.write_all(
            br#"<?xml version="1.0" encoding="UTF-8"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles>
    <rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/>
  </rootfiles>
</container>"#,
        )
        .unwrap();

        w.start_file("OEBPS/content.opf", default.clone()).unwrap();
        w.write_all(
            br#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0">
  <metadata/>
  <manifest>
    <item id="cover.svg" href="cover.svg" media-type="image/svg+xml" properties="cover-image"/>
  </manifest>
  <spine/>
</package>"#,
        )
        .unwrap();

        w.start_file("OEBPS/cover.svg", default.clone()).unwrap();
        w.write_all(svg_body).unwrap();

        if sibling_jpeg {
            let jpeg = {
                let img = image::DynamicImage::new_rgb8(2, 2);
                let mut b = Vec::new();
                img.write_to(&mut std::io::Cursor::new(&mut b), image::ImageFormat::Jpeg)
                    .expect("encode jpeg");
                b
            };
            w.start_file("OEBPS/cover.jpg", default.clone()).unwrap();
            w.write_all(&jpeg).unwrap();
        }

        w.start_file("META-INF/reverie-marker.txt", default)
            .unwrap();
        w.write_all(marker.as_bytes()).unwrap();

        w.finish().unwrap().into_inner()
    }

    /// SE-realistic SVG cover referencing a sibling raster via
    /// `<image href="cover.jpg">`; the sibling 2×2 JPEG is included so the
    /// rasterizer's in-ZIP href resolver has something to resolve.
    pub fn make_minimal_epub_with_svg_cover_sibling_ref(marker: &str) -> Vec<u8> {
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="150" viewBox="0 0 100 150"><image href="cover.jpg" width="100" height="150"/></svg>"#;
        build_svg_cover_epub(marker, svg, true)
    }

    /// Malformed (truncated) SVG cover — exercises the negative serve path.
    pub fn make_minimal_epub_with_malformed_svg_cover(marker: &str) -> Vec<u8> {
        build_svg_cover_epub(marker, b"<svg><broken", false)
    }

    #[cfg(test)]
    mod tests {
        use super::role_opts_from;
        use sqlx::postgres::PgConnectOptions;

        #[test]
        fn role_password_requires_nonempty_input() {
            assert_eq!(
                super::password_input(Some("supplied-password")),
                Ok("supplied-password")
            );
            assert!(super::password_input(None).is_err());
            assert!(super::password_input(Some("")).is_err());
        }

        #[test]
        fn role_opts_carry_a_socket_source() {
            let source = PgConnectOptions::new()
                .socket("/tmp/pgsock")
                .database("per_test_db");
            let derived = role_opts_from(&source, "reverie_app", "pw");
            assert_eq!(
                derived.get_socket(),
                Some(&std::path::PathBuf::from("/tmp/pgsock"))
            );
            assert_eq!(derived.get_database(), Some("per_test_db"));
            assert_eq!(derived.get_username(), "reverie_app");
        }

        #[test]
        fn role_opts_stay_tcp_for_a_tcp_source() {
            let source = PgConnectOptions::new()
                .host("db.internal")
                .port(6432)
                .database("per_test_db");
            let derived = role_opts_from(&source, "reverie_ingestion", "pw");
            assert_eq!(derived.get_socket(), None);
            assert_eq!(derived.get_host(), "db.internal");
            assert_eq!(derived.get_port(), 6432);
        }
    }
}

/// Mock OIDC provider scaffolding for end-to-end auth-flow tests.
///
/// Spins up a `wiremock::MockServer` with `/jwks` and `/token` endpoints,
/// generates a real RSA keypair per test, and exposes an `OidcClient`
/// configured to point at the mock. Lets tests drive the full callback
/// flow (PKCE/CSRF/nonce validation → token exchange → ID token signature
/// verification → user upsert → session login) without going through a
/// real `IdP`.
pub mod oidc_mock {
    use openidconnect::core::{
        CoreIdToken, CoreIdTokenClaims, CoreJsonWebKey, CoreJsonWebKeySet, CoreJwsSigningAlgorithm,
        CoreProviderMetadata, CoreResponseType, CoreRsaPrivateSigningKey,
        CoreSubjectIdentifierType,
    };
    use openidconnect::{
        AccessToken, Audience, AuthUrl, EmptyAdditionalClaims, EmptyAdditionalProviderMetadata,
        EndUserEmail, EndUserName, IssuerUrl, JsonWebKeyId, JsonWebKeySetUrl, LocalizedClaim,
        Nonce as OidcNonce, ResponseTypes, StandardClaims, SubjectIdentifier, TokenUrl,
    };
    use rsa::RsaPrivateKey;
    use rsa::pkcs1::EncodeRsaPrivateKey;
    use rsa::pkcs1::LineEnding;
    use rsa::traits::PublicKeyParts;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use crate::auth::oidc::{OidcClient, OidcConfiguration, OidcRuntime, OidcTransport};

    /// One-shot mock OIDC provider. Construct with [`Self::start`], wire an
    /// `OidcClient` into your `AppState` via [`Self::client`], then call
    /// [`Self::mount_token_endpoint`] before driving `/auth/callback`.
    pub struct MockOidcProvider {
        server: MockServer,
        signing_key_pem: String,
        kid: String,
        issuer: String,
        client_id: String,
        jwks: CoreJsonWebKeySet,
        jwks_guard: Option<wiremock::MockGuard>,
        token_guard: tokio::sync::Mutex<Option<wiremock::MockGuard>>,
    }

    impl MockOidcProvider {
        /// Boot a `MockServer` and serve a freshly-generated JWKS at `/jwks`.
        /// `client_id` is the OIDC `aud` claim — must match `OIDC_CLIENT_ID`
        /// in the `AppState`'s config (`test_config()` uses an empty string;
        /// pass `""` here to match).
        pub async fn start(client_id: &str) -> Self {
            let kid = "test-kid".to_owned();
            let (signing_key_pem, jwks) = Self::signing_material(&kid);
            let jwks_body = serde_json::to_value(&jwks).unwrap();

            let server = MockServer::start().await;
            let jwks_guard = Mock::given(method("GET"))
                .and(path("/jwks"))
                .respond_with(ResponseTemplate::new(200).set_body_json(jwks_body))
                .mount_as_scoped(&server)
                .await;

            let issuer = server.uri();
            Self {
                server,
                signing_key_pem,
                kid,
                issuer,
                client_id: client_id.to_string(),
                jwks,
                jwks_guard: Some(jwks_guard),
                token_guard: tokio::sync::Mutex::new(None),
            }
        }

        fn signing_material(kid: &str) -> (String, CoreJsonWebKeySet) {
            let private_key = RsaPrivateKey::new(&mut rand_core_06::OsRng, 2048).unwrap();
            let pem = private_key
                .to_pkcs1_pem(LineEnding::LF)
                .unwrap()
                .to_string();
            let key = CoreJsonWebKey::new_rsa(
                private_key.n().to_bytes_be(),
                private_key.e().to_bytes_be(),
                Some(JsonWebKeyId::new(kid.to_owned())),
            );
            (pem, CoreJsonWebKeySet::new(vec![key]))
        }

        pub async fn rotate(&mut self) {
            self.kid.push('b');
            (self.signing_key_pem, self.jwks) = Self::signing_material(&self.kid);
            self.set_jwks_response(ResponseTemplate::new(200).set_body_json(&self.jwks), 0)
                .await;
        }

        pub fn jwks(&self) -> CoreJsonWebKeySet {
            self.jwks.clone()
        }

        pub async fn set_jwks_response(&mut self, response: ResponseTemplate, minimum: u64) {
            drop(self.jwks_guard.take());
            self.jwks_guard = Some(
                Mock::given(method("GET"))
                    .and(path("/jwks"))
                    .respond_with(response)
                    .expect(minimum..)
                    .mount_as_scoped(&self.server)
                    .await,
            );
        }

        pub async fn hold_jwks(&mut self) -> std::sync::mpsc::SyncSender<()> {
            drop(self.jwks_guard.take());
            let (release, wait) = std::sync::mpsc::sync_channel(1);
            let wait = std::sync::Mutex::new(wait);
            let response = ResponseTemplate::new(200).set_body_json(self.jwks());
            self.jwks_guard = Some(
                Mock::given(method("GET"))
                    .and(path("/jwks"))
                    .respond_with(move |_: &wiremock::Request| {
                        wait.lock()
                            .unwrap()
                            .recv_timeout(std::time::Duration::from_secs(5))
                            .unwrap();
                        response.clone()
                    })
                    .expect(1)
                    .mount_as_scoped(&self.server)
                    .await,
            );
            release
        }

        pub async fn wait_for_jwks(&self) {
            tokio::time::timeout(
                std::time::Duration::from_secs(5),
                self.jwks_guard.as_ref().unwrap().wait_until_satisfied(),
            )
            .await
            .unwrap();
        }

        pub async fn request_count(&self, endpoint: &str) -> usize {
            self.server
                .received_requests()
                .await
                .unwrap()
                .iter()
                .filter(|request| request.url.path() == endpoint)
                .count()
        }

        /// The issuer (`iss`) this mock signs into its ID tokens, which the
        /// callback persists to `user_identities.issuer`. Dynamic (the mock
        /// server's URI), so tests must read it here rather than hard-code it.
        pub fn issuer(&self) -> &str {
            &self.issuer
        }

        pub async fn assert_no_requests(&self) {
            assert!(
                self.server
                    .received_requests()
                    .await
                    .expect("request recording enabled")
                    .is_empty()
            );
        }

        pub(crate) fn metadata(&self) -> CoreProviderMetadata {
            let issuer_url = IssuerUrl::new(self.issuer.clone()).expect("valid issuer url");
            let auth_url = AuthUrl::new(format!("{}/auth", self.issuer)).expect("auth url");
            let jwks_url =
                JsonWebKeySetUrl::new(format!("{}/jwks", self.issuer)).expect("jwks url");
            let token_url = TokenUrl::new(format!("{}/token", self.issuer)).expect("token url");

            CoreProviderMetadata::new(
                issuer_url,
                auth_url,
                jwks_url,
                vec![ResponseTypes::new(vec![CoreResponseType::Code])],
                vec![CoreSubjectIdentifierType::Public],
                vec![CoreJwsSigningAlgorithm::RsaSsaPkcs1V15Sha256],
                EmptyAdditionalProviderMetadata {},
            )
            .set_token_endpoint(Some(token_url))
            .set_jwks(self.jwks.clone())
        }

        pub fn config(&self, redirect_uri: &str) -> crate::config::Config {
            let mut config = super::test_config();
            config.oidc_issuer_url = self.issuer.clone();
            config.oidc_client_id = self.client_id.clone();
            config.oidc_client_secret = "test-secret".into();
            config.oidc_redirect_uri = redirect_uri.to_owned();
            config
        }

        pub fn configuration(&self, redirect_uri: &str) -> OidcConfiguration {
            OidcConfiguration::parse(&self.config(redirect_uri), &OidcTransport::for_tests())
                .unwrap()
        }

        pub fn client(&self, redirect_uri: &str) -> OidcClient {
            self.configuration(redirect_uri).client(self.metadata())
        }

        /// Install a `/token` responder that returns a signed ID token whose
        /// claims include the given `nonce`. Call after reading the
        /// server-stored nonce out of the shared session store but before
        /// driving `/auth/callback`.
        pub fn token(
            &self,
            subject: &str,
            email: Option<&str>,
            name: Option<&str>,
            nonce: &str,
        ) -> CoreIdToken {
            // CoreIdTokenClaims::new takes chrono::DateTime<Utc> for the
            // expiration and issued_at parameters, which is what first-party
            // code uses anyway.
            use chrono::{TimeDelta, Utc};
            let issuer_url = IssuerUrl::new(self.issuer.clone()).expect("valid issuer url");

            let mut standard_claims =
                StandardClaims::new(SubjectIdentifier::new(subject.to_string()));
            if let Some(e) = email {
                standard_claims = standard_claims
                    .set_email(Some(EndUserEmail::new(e.to_string())))
                    .set_email_verified(Some(true));
            }
            if let Some(n) = name {
                let mut localized: LocalizedClaim<EndUserName> = LocalizedClaim::new();
                localized.insert(None, EndUserName::new(n.to_string()));
                standard_claims = standard_claims.set_name(Some(localized));
            }

            let claims = CoreIdTokenClaims::new(
                issuer_url,
                vec![Audience::new(self.client_id.clone())],
                Utc::now() + TimeDelta::seconds(300),
                Utc::now(),
                standard_claims,
                EmptyAdditionalClaims {},
            )
            .set_nonce(Some(OidcNonce::new(nonce.to_string())));

            self.token_with_claims(claims)
        }

        pub fn token_with_claims(&self, claims: CoreIdTokenClaims) -> CoreIdToken {
            let access_token = AccessToken::new("test-access-token".to_owned());
            let signing_key = CoreRsaPrivateSigningKey::from_pem(
                &self.signing_key_pem,
                Some(JsonWebKeyId::new(self.kid.clone())),
            )
            .expect("parse signing key");

            CoreIdToken::new(
                claims,
                &signing_key,
                CoreJwsSigningAlgorithm::RsaSsaPkcs1V15Sha256,
                Some(&access_token),
                None,
            )
            .expect("sign id token")
        }

        pub async fn mount_token_endpoint(
            &self,
            subject: &str,
            email: Option<&str>,
            name: Option<&str>,
            nonce: &str,
        ) {
            let id_token = self.token(subject, email, name, nonce);
            self.mount_id_token(id_token).await;
        }

        pub async fn mount_id_token(&self, id_token: CoreIdToken) {
            let access_token = AccessToken::new("test-access-token".to_owned());
            let token_response = serde_json::json!({
                "access_token": access_token.secret(),
                "token_type": "Bearer",
                "expires_in": 300,
                "id_token": id_token.to_string(),
            });

            let mut guard = self.token_guard.lock().await;
            drop(guard.take());
            *guard = Some(
                Mock::given(method("POST"))
                    .and(path("/token"))
                    .respond_with(ResponseTemplate::new(200).set_body_json(token_response))
                    .mount_as_scoped(&self.server)
                    .await,
            );
        }

        /// Pair the mock client with a loopback test transport.
        pub fn runtime(&self, redirect_uri: &str) -> crate::auth::oidc::OidcRuntime {
            OidcRuntime::new(
                self.configuration(redirect_uri),
                self.metadata(),
                OidcTransport::for_tests(),
            )
        }

        /// Serve discovery metadata for the mock provider.
        pub async fn mount_discovery(&self) {
            self.mount_discovery_with_token_endpoint(&format!("{}/token", self.issuer))
                .await;
        }

        /// Serve discovery metadata with a chosen token endpoint.
        pub async fn mount_discovery_with_token_endpoint(&self, token_endpoint: &str) {
            self.mount_discovery_with_jwks_uri(token_endpoint, &format!("{}/jwks", self.issuer))
                .await;
        }

        /// Serve discovery with a chosen token endpoint and signing-key URL.
        pub async fn mount_discovery_with_jwks_uri(&self, token_endpoint: &str, jwks_uri: &str) {
            let document = serde_json::json!({
                "issuer": self.issuer,
                "authorization_endpoint": format!("{}/auth", self.issuer),
                "token_endpoint": token_endpoint,
                "jwks_uri": jwks_uri,
                "response_types_supported": ["code"],
                "subject_types_supported": ["public"],
                "id_token_signing_alg_values_supported": ["RS256"],
            });
            Mock::given(method("GET"))
                .and(path("/.well-known/openid-configuration"))
                .respond_with(ResponseTemplate::new(200).set_body_json(document))
                .mount(&self.server)
                .await;
        }
    }
}

#[derive(Clone, Default)]
pub struct CapturedLogs(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);

impl std::io::Write for CapturedLogs {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl CapturedLogs {
    pub(crate) fn subscriber(&self) -> impl tracing::Subscriber + Send + Sync + use<> {
        let writer = self.clone();
        tracing_subscriber::fmt()
            .without_time()
            .with_ansi(false)
            .with_writer(move || writer.clone())
            .finish()
    }

    pub(crate) fn trace_subscriber(&self) -> impl tracing::Subscriber + Send + Sync + use<> {
        let writer = self.clone();
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::TRACE)
            .without_time()
            .with_ansi(false)
            .with_writer(move || writer.clone())
            .finish()
    }

    pub(crate) fn text(&self) -> String {
        String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
    }
}

/// An in-process SMTP relay and mail runtime builders for queue tests.
pub mod mail {
    use std::sync::{Arc, Mutex};

    use chrono::{DateTime, TimeDelta, Utc};
    use lettre::message::Mailbox;
    use sqlx::PgPool;
    use tokio::{
        io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
        net::{TcpListener, TcpStream},
    };
    use uuid::Uuid;

    use crate::config::{MailTransportConfig, SmtpSecurity};
    use crate::models::account_email_proof::lock_account;
    use crate::models::mail_job::{MailPayload, MailPurpose, NewJob, enqueue};
    use crate::services::mail::{crypto::QueueKey, queue::MailRuntime};

    /// How the relay answers the end of a message.
    #[derive(Clone, Copy, PartialEq, Eq)]
    pub enum Behaviour {
        Accept,
        Transient,
        Permanent,
        Hang,
    }

    /// A plaintext SMTP listener that records what it was sent.
    pub struct FakeRelay {
        pub port: u16,
        behaviour: Arc<Mutex<Behaviour>>,
        messages: Arc<Mutex<Vec<String>>>,
        recipients: Arc<Mutex<Vec<String>>>,
    }

    impl FakeRelay {
        pub async fn start(behaviour: Behaviour) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let relay = Self {
                port: listener.local_addr().unwrap().port(),
                behaviour: Arc::new(Mutex::new(behaviour)),
                messages: Arc::default(),
                recipients: Arc::default(),
            };
            let behaviour = Arc::clone(&relay.behaviour);
            let messages = Arc::clone(&relay.messages);
            let recipients = Arc::clone(&relay.recipients);
            tokio::spawn(async move {
                while let Ok((stream, _)) = listener.accept().await {
                    tokio::spawn(serve(
                        stream,
                        Arc::clone(&behaviour),
                        Arc::clone(&messages),
                        Arc::clone(&recipients),
                    ));
                }
            });
            relay
        }

        pub fn set(&self, behaviour: Behaviour) {
            *self.behaviour.lock().unwrap() = behaviour;
        }

        /// Every message body the relay received, headers included.
        pub fn messages(&self) -> Vec<String> {
            self.messages.lock().unwrap().clone()
        }

        pub fn message_count(&self) -> usize {
            self.messages.lock().unwrap().len()
        }

        /// Every envelope recipient the relay was offered.
        pub fn recipients(&self) -> Vec<String> {
            self.recipients.lock().unwrap().clone()
        }
    }

    fn say(text: &str) -> Vec<u8> {
        format!("{text}\r\n").into_bytes()
    }

    async fn serve(
        stream: TcpStream,
        behaviour: Arc<Mutex<Behaviour>>,
        messages: Arc<Mutex<Vec<String>>>,
        recipients: Arc<Mutex<Vec<String>>>,
    ) {
        let (reader, mut writer) = stream.into_split();
        let mut reader = BufReader::new(reader);
        let mut line = String::new();
        writer
            .write_all(&say("220 fake.example ESMTP"))
            .await
            .unwrap();
        loop {
            line.clear();
            if reader.read_line(&mut line).await.unwrap_or(0) == 0 {
                return;
            }
            let command = line.trim_end().to_owned();
            let upper = command.to_ascii_uppercase();
            let reply = if upper.starts_with("EHLO") {
                "250 fake.example".to_owned()
            } else if upper.starts_with("RCPT TO") {
                recipients.lock().unwrap().push(command.clone());
                "250 recipient ok".to_owned()
            } else if upper.starts_with("MAIL FROM") {
                "250 sender ok".to_owned()
            } else if upper.starts_with("DATA") {
                writer
                    .write_all(&say("354 end with <CRLF>.<CRLF>"))
                    .await
                    .unwrap();
                let mut data = String::new();
                loop {
                    line.clear();
                    if reader.read_line(&mut line).await.unwrap_or(0) == 0 {
                        return;
                    }
                    if line == ".\r\n" {
                        break;
                    }
                    data.push_str(&line);
                }
                messages.lock().unwrap().push(data);
                let current = *behaviour.lock().unwrap();
                match current {
                    Behaviour::Accept => "250 queued".to_owned(),
                    Behaviour::Transient => "451 try again later".to_owned(),
                    Behaviour::Permanent => "550 refused".to_owned(),
                    Behaviour::Hang => std::future::pending().await,
                }
            } else if upper.starts_with("QUIT") {
                writer.write_all(&say("221 bye")).await.unwrap();
                return;
            } else {
                "502 not implemented".to_owned()
            };
            writer.write_all(&say(&reply)).await.unwrap();
        }
    }

    pub async fn account(pool: &PgPool, email: &str) -> Uuid {
        sqlx::query_scalar!(
            "INSERT INTO users (display_name, email) VALUES ('Mail User', $1) RETURNING id",
            email
        )
        .fetch_one(pool)
        .await
        .unwrap()
    }

    pub fn payload(recipient: &str) -> MailPayload {
        MailPayload {
            recipient: recipient.into(),
            subject: "Subject".into(),
            body: "Body-marker https://example.test/link".into(),
        }
    }

    pub fn digest(seed: u8) -> Vec<u8> {
        vec![seed; 32]
    }

    pub async fn enqueue_notice(
        app: &PgPool,
        user: Uuid,
        purpose: MailPurpose,
        recipient: &str,
    ) -> Uuid {
        let mut tx = app.begin().await.unwrap();
        lock_account(&mut tx, user).await.unwrap().unwrap();
        let id = enqueue(
            &mut tx,
            &key(),
            &NewJob {
                user_id: user,
                purpose,
                proof_digest: None,
                expires_at: None,
                payload: &payload(recipient),
            },
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        id
    }

    pub async fn enqueue_bearer(
        app: &PgPool,
        user: Uuid,
        purpose: MailPurpose,
        proof_digest: &[u8],
        lifetime: TimeDelta,
        recipient: &str,
    ) -> Uuid {
        let mut tx = app.begin().await.unwrap();
        lock_account(&mut tx, user).await.unwrap().unwrap();
        let id = enqueue(
            &mut tx,
            &key(),
            &NewJob {
                user_id: user,
                purpose,
                proof_digest: Some(proof_digest),
                expires_at: Some(Utc::now() + lifetime),
                payload: &payload(recipient),
            },
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        id
    }

    pub async fn state_of(pool: &PgPool, id: Uuid) -> String {
        sqlx::query_scalar!("SELECT state FROM mail_jobs WHERE id = $1", id)
            .fetch_one(pool)
            .await
            .unwrap()
    }

    pub async fn payload_present(pool: &PgPool, id: Uuid) -> bool {
        sqlx::query_scalar!(
            r#"SELECT (payload IS NOT NULL) AS "present!" FROM mail_jobs WHERE id = $1"#,
            id
        )
        .fetch_one(pool)
        .await
        .unwrap()
    }

    pub async fn backdate(pool: &PgPool, id: Uuid, enqueued_at: DateTime<Utc>) {
        sqlx::query!(
            "UPDATE mail_jobs SET enqueued_at = $2::timestamptz, next_attempt_at = $2::timestamptz, \
                    expires_at = $2::timestamptz + interval '30 days' WHERE id = $1",
            id,
            enqueued_at
        )
        .execute(pool)
        .await
        .unwrap();
    }

    pub fn key() -> QueueKey {
        QueueKey::from_bytes([9; 32])
    }

    pub fn other_key() -> QueueKey {
        QueueKey::from_bytes([10; 32])
    }

    fn mailbox(address: &str) -> Mailbox {
        Mailbox::new(None, address.parse().unwrap())
    }

    pub fn runtime_with(port: u16, key: QueueKey, local_auth_enabled: bool) -> MailRuntime {
        MailRuntime::new(
            key,
            MailTransportConfig {
                host: "127.0.0.1".into(),
                port,
                security: SmtpSecurity::Plaintext,
                credentials: None,
                from: mailbox("reverie@example.com"),
                reply_to: mailbox("support@example.com"),
            },
            local_auth_enabled,
        )
    }

    pub fn runtime(port: u16) -> MailRuntime {
        runtime_with(port, key(), true)
    }

    pub const PASSWORD: &str = "lithe cobalt meadow drift";
    pub const SUPPORT: &str = "support@example.com";
    pub const ORIGIN: &str = "http://localhost:3000";

    /// A mail runtime that submits to a relay on `port`.
    pub fn shared_runtime(port: u16) -> Arc<MailRuntime> {
        Arc::new(runtime(port))
    }

    /// A server with the mail runtime wired in and cookies saved.
    pub fn server(
        app: &PgPool,
        ingestion: &PgPool,
        mail: &Arc<MailRuntime>,
    ) -> axum_test::TestServer {
        let mut server =
            super::db::server_with_mail(app, ingestion, super::test_config(), Arc::clone(mail));
        server.save_cookies();
        server
    }

    /// Sign in over the saved cookies and return the CSRF token.
    pub async fn login(server: &axum_test::TestServer, email: &str, password: &str) -> String {
        let response = server
            .post("/auth/local/login")
            .json(&serde_json::json!({"email": email, "password": password}))
            .await;
        assert_eq!(response.status_code(), axum::http::StatusCode::NO_CONTENT);
        server.get("/auth/me").await.json::<serde_json::Value>()["csrf_token"]
            .as_str()
            .unwrap()
            .to_owned()
    }

    pub fn csrf(token: &str) -> (axum::http::HeaderName, axum::http::HeaderValue) {
        (
            axum::http::HeaderName::from_static("x-csrf-token"),
            axum::http::HeaderValue::from_str(token).unwrap(),
        )
    }

    /// A stored job with its sealed payload opened.
    #[derive(Debug)]
    pub struct Stored {
        pub id: Uuid,
        pub purpose: String,
        pub state: String,
        pub recipient: String,
        pub subject: String,
        pub body: String,
    }

    /// Every job of `user` that still holds a payload, oldest first.
    pub async fn messages(pool: &PgPool, user: Uuid) -> Vec<Stored> {
        let rows = sqlx::query!(
            "SELECT id, purpose, state, nonce, payload FROM mail_jobs \
              WHERE user_id = $1 AND payload IS NOT NULL ORDER BY enqueued_at, id",
            user
        )
        .fetch_all(pool)
        .await
        .unwrap();
        rows.into_iter()
            .map(|row| {
                let plaintext = crate::services::mail::crypto::open(
                    &key(),
                    row.id,
                    &row.purpose,
                    row.nonce.as_deref().unwrap(),
                    row.payload.as_deref().unwrap(),
                )
                .unwrap();
                let payload: MailPayload = serde_json::from_slice(&plaintext).unwrap();
                Stored {
                    id: row.id,
                    purpose: row.purpose,
                    state: row.state,
                    recipient: payload.recipient,
                    subject: payload.subject,
                    body: payload.body,
                }
            })
            .collect()
    }

    /// Job count for one account, any state, optionally one purpose.
    pub async fn job_count(pool: &PgPool, user: Uuid, purpose: Option<&str>) -> i64 {
        sqlx::query_scalar!(
            r#"SELECT count(*) AS "count!" FROM mail_jobs
                WHERE user_id = $1 AND ($2::text IS NULL OR purpose = $2)"#,
            user,
            purpose
        )
        .fetch_one(pool)
        .await
        .unwrap()
    }

    /// Wait for the first job of `purpose` for `user`; panics after five seconds.
    pub async fn wait_for_job(pool: &PgPool, user: Uuid, purpose: &str) -> Uuid {
        for _ in 0..500 {
            if let Some(id) = sqlx::query_scalar!(
                "SELECT id FROM mail_jobs WHERE user_id = $1 AND purpose = $2 ORDER BY enqueued_at LIMIT 1",
                user,
                purpose
            )
            .fetch_optional(pool)
            .await
            .unwrap()
            {
                return id;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        panic!("no {purpose} job appeared");
    }

    /// The token after `#` in the first link of a body.
    pub fn token_in(body: &str) -> String {
        let fragment = body.split_once('#').unwrap().1;
        fragment.lines().next().unwrap().trim().to_owned()
    }

    /// Mark an account's address verified without going through confirmation.
    pub async fn verify_address(pool: &PgPool, user: Uuid) {
        sqlx::query!(
            "UPDATE users SET email_verified_at = now() WHERE id = $1",
            user
        )
        .execute(pool)
        .await
        .unwrap();
    }

    /// Whether the account holds an unexpired proof of `purpose`.
    pub async fn proof_live(pool: &PgPool, user: Uuid, purpose: &str) -> bool {
        sqlx::query_scalar!(
            r#"SELECT EXISTS (SELECT 1 FROM account_email_proofs
                               WHERE user_id = $1 AND purpose = $2
                                 AND expires_at > clock_timestamp()) AS "live!""#,
            user,
            purpose
        )
        .fetch_one(pool)
        .await
        .unwrap()
    }

    /// Mark a delivery accepted and old enough that the 60-second interval has passed.
    pub async fn settle_delivery(pool: &PgPool, id: Uuid) {
        sqlx::query!(
            "UPDATE mail_jobs SET state = 'accepted', terminal_at = clock_timestamp(), \
                    enqueued_at = clock_timestamp() - interval '2 minutes', \
                    next_attempt_at = clock_timestamp() - interval '2 minutes' WHERE id = $1",
            id
        )
        .execute(pool)
        .await
        .unwrap();
    }

    /// Wait until `user` has at least `count` jobs of `purpose`.
    pub async fn wait_for_count(pool: &PgPool, user: Uuid, purpose: &str, count: i64) {
        for _ in 0..500 {
            if job_count(pool, user, Some(purpose)).await >= count {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        panic!("fewer than {count} {purpose} jobs appeared");
    }

    /// Fail every later insert into `mail_jobs`.
    pub async fn break_enqueue(pool: &PgPool) {
        sqlx::raw_sql(
            "CREATE FUNCTION fail_mail_insert() RETURNS trigger LANGUAGE plpgsql AS \
             $$ BEGIN RAISE EXCEPTION 'mail insert blocked'; END $$; \
             CREATE TRIGGER fail_mail_insert BEFORE INSERT ON mail_jobs \
             FOR EACH ROW EXECUTE FUNCTION fail_mail_insert();",
        )
        .execute(pool)
        .await
        .unwrap();
    }
}
