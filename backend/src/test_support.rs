use axum::Router;
use axum_test::TestServer;

use crate::auth::oidc::OidcClient;
use crate::config::{
    CleanupMode, Config, CoverConfig, EnrichmentConfig, OpdsConfig, SecurityConfig, WritebackConfig,
};
use crate::models::manifestation_format::ManifestationFormat;
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
        migration_database_url: None,
        auto_migrate: false,
        ingestion_database_url: String::new(),
        format_priority: vec![
            ManifestationFormat::Epub,
            ManifestationFormat::Pdf,
            ManifestationFormat::Mobi,
            ManifestationFormat::Azw3,
            ManifestationFormat::Cbz,
            ManifestationFormat::Cbr,
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
            public_url: Some(url::Url::parse("http://localhost:3000").unwrap()),
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
        openlibrary_base_url: "https://openlibrary.org".into(),
        googlebooks_base_url: "https://www.googleapis.com/books/v1".into(),
        googlebooks_api_key: None,
        hardcover_base_url: "https://api.hardcover.app/v1/graphql".into(),
        hardcover_api_token: None,
        operator_contact: None,
        ingestion_dsn_defaulted: false,
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
        format_priority: vec![
            "epub".into(),
            "pdf".into(),
            "mobi".into(),
            "azw3".into(),
            "cbz".into(),
            "cbr".into(),
        ],
        cleanup_mode: "all".into(),
        openlibrary_base_url: "https://openlibrary.org".into(),
        googlebooks_base_url: "https://www.googleapis.com/books/v1".into(),
        hardcover_base_url: "https://api.hardcover.app/v1/graphql".into(),
        updated_at: time::OffsetDateTime::now_utc(),
    }))
}

pub fn test_state() -> AppState {
    AppState {
        pool: sqlx::PgPool::connect_lazy("postgres://invalid").unwrap(),
        ingestion_pool: sqlx::PgPool::connect_lazy("postgres://invalid").unwrap(),
        config: test_config(),
        oidc_client: test_oidc_client(),
        settings: test_settings(),
        last_settings_reload: std::sync::Arc::new(tokio::sync::RwLock::new(None)),
    }
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

    /// Build a `reverie_app` pool against the same DB as the given pool.
    /// Use this when a test needs RLS-enforced access (the runtime web role).
    /// Password defaults to the role name (matches `docker/init-roles.sql`);
    /// override with `REVERIE_APP_PASSWORD` env var.
    pub async fn app_pool_for(pool: &PgPool) -> PgPool {
        let password =
            std::env::var("REVERIE_APP_PASSWORD").unwrap_or_else(|_| "reverie_app".into());
        pool_as_role(pool, "reverie_app", &password, false).await
    }

    /// Build a writeback-worker pool against the same DB as the given pool.
    /// Connects as `reverie_app` with `app.system_context = 'writeback'` set
    /// session-scoped on every connection — mirrors `db::init_writeback_pool`.
    /// Use this for tests that exercise writeback orchestrator/queue code
    /// paths against `manifestations` (which has system-context RLS policies).
    pub async fn writeback_pool_for(pool: &PgPool) -> PgPool {
        let password =
            std::env::var("REVERIE_APP_PASSWORD").unwrap_or_else(|_| "reverie_app".into());
        pool_as_role(pool, "reverie_app", &password, true).await
    }

    /// Build a `reverie_ingestion` pool against the same DB as the given pool.
    /// Use this for fixture inserts on pipeline tables (manifestations, works)
    /// where the `*_ingestion_full_access` RLS policies apply.
    /// Password defaults to the role name (matches `docker/init-roles.sql`);
    /// override with `REVERIE_INGESTION_PASSWORD` env var.
    pub async fn ingestion_pool_for(pool: &PgPool) -> PgPool {
        let password = std::env::var("REVERIE_INGESTION_PASSWORD")
            .unwrap_or_else(|_| "reverie_ingestion".into());
        pool_as_role(pool, "reverie_ingestion", &password, false).await
    }

    async fn pool_as_role(
        pool: &PgPool,
        username: &str,
        password: &str,
        writeback_context: bool,
    ) -> PgPool {
        let (host, port, database) = {
            let opts = pool.connect_options();
            (
                opts.get_host().to_owned(),
                opts.get_port(),
                opts.get_database()
                    .expect("injected pool has database name")
                    .to_owned(),
            )
        };
        let new_opts = PgConnectOptions::new()
            .host(&host)
            .port(port)
            .database(&database)
            .username(username)
            .password(password);
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
        sqlx::query("UPDATE users SET role = 'admin'::user_role WHERE id = $1")
            .bind(user.id)
            .execute(app_pool)
            .await
            .expect("promote to admin");
        let (plaintext, hash) = crate::auth::token::generate_device_token();
        crate::models::device_token::create(app_pool, user.id, "admin-test", &hash)
            .await
            .expect("create token");

        let basic =
            base64ct::Base64::encode_string(format!("{}:{}", user.id, plaintext).as_bytes());
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
        use crate::state::AppState;
        let state = AppState {
            pool: app_pool.clone(),
            ingestion_pool: ingestion_pool.clone(),
            config: super::test_config(),
            oidc_client: super::test_oidc_client(),
            settings: super::test_settings(),
            last_settings_reload: std::sync::Arc::new(tokio::sync::RwLock::new(None)),
        };
        let app = crate::build_router(state);
        axum_test::TestServer::new(app)
    }

    /// Same as [`server_with_real_pools`] but with OPDS enabled. Tests that
    /// exercise `/opds/*` routes need this — the base `test_config()` has
    /// `opds.enabled = false` to match ordinary route tests.
    ///
    /// `library_path` is the absolute path to a real directory (usually a
    /// `tempfile::TempDir`) — the download handler's canonicalisation guard
    /// resolves `file_path` against this root.
    pub fn server_with_opds_enabled(
        app_pool: &PgPool,
        ingestion_pool: &PgPool,
        library_path: &std::path::Path,
    ) -> axum_test::TestServer {
        use crate::config::OpdsConfig;
        use crate::state::AppState;

        let mut config = super::test_config();
        config.library_path = library_path.to_string_lossy().into_owned();
        config.opds = OpdsConfig {
            enabled: true,
            page_size: 50,
            realm: "Reverie OPDS".into(),
            public_url: Some(url::Url::parse("http://host.example/").unwrap()),
        };
        let state = AppState {
            pool: app_pool.clone(),
            ingestion_pool: ingestion_pool.clone(),
            config,
            oidc_client: super::test_oidc_client(),
            settings: super::test_settings(),
            last_settings_reload: std::sync::Arc::new(tokio::sync::RwLock::new(None)),
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
        let mut config = super::test_config();
        config.opds.page_size = page_size;
        let state = AppState {
            pool: app_pool.clone(),
            ingestion_pool: ingestion_pool.clone(),
            config,
            oidc_client: super::test_oidc_client(),
            settings: super::test_settings(),
            last_settings_reload: std::sync::Arc::new(tokio::sync::RwLock::new(None)),
        };
        let app = crate::build_router(state);
        axum_test::TestServer::new(app)
    }

    /// Same as [`server_with_opds_enabled`] but with a caller-chosen
    /// `opds.page_size` for navigation-feed pagination-walk tests.
    pub fn server_with_opds_page_size(
        app_pool: &PgPool,
        ingestion_pool: &PgPool,
        library_path: &std::path::Path,
        page_size: u32,
    ) -> axum_test::TestServer {
        use crate::config::OpdsConfig;
        use crate::state::AppState;

        let mut config = super::test_config();
        config.library_path = library_path.to_string_lossy().into_owned();
        config.opds = OpdsConfig {
            enabled: true,
            page_size,
            realm: "Reverie OPDS".into(),
            public_url: Some(url::Url::parse("http://host.example/").unwrap()),
        };
        let state = AppState {
            pool: app_pool.clone(),
            ingestion_pool: ingestion_pool.clone(),
            config,
            oidc_client: super::test_oidc_client(),
            settings: super::test_settings(),
            last_settings_reload: std::sync::Arc::new(tokio::sync::RwLock::new(None)),
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
        let file_path = format!("/tmp/admin-test-{marker}.epub");
        let file_hash = format!("admin-test-hash-{marker}");
        let m_id: Uuid = sqlx::query_scalar!(
            "INSERT INTO manifestations \
                (work_id, format, file_path, ingestion_file_hash, current_file_hash, \
                 file_size_bytes, ingestion_status, validation_status) \
             VALUES ($1, 'epub'::manifestation_format, $2, $3, $3, 1000, \
                     'complete'::ingestion_status, 'clean'::validation_status) \
             RETURNING id",
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
        sqlx::query("UPDATE users SET role = 'child'::user_role, is_child = TRUE WHERE id = $1")
            .bind(user.id)
            .execute(app_pool)
            .await
            .expect("demote to child");
        let (plaintext, hash) = crate::auth::token::generate_device_token();
        crate::models::device_token::create(app_pool, user.id, "child-test", &hash)
            .await
            .expect("create token");

        let basic =
            base64ct::Base64::encode_string(format!("{}:{}", user.id, plaintext).as_bytes());
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
        crate::models::device_token::create(app_pool, user.id, "adult-test", &hash)
            .await
            .expect("create token");

        let basic =
            base64ct::Base64::encode_string(format!("{}:{}", user.id, plaintext).as_bytes());
        (user.id, format!("Basic {basic}"))
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
    /// extraction path under Step 5.
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
        CoreClient, CoreIdToken, CoreIdTokenClaims, CoreJsonWebKey, CoreJsonWebKeySet,
        CoreJwsSigningAlgorithm, CoreProviderMetadata, CoreResponseType, CoreRsaPrivateSigningKey,
        CoreSubjectIdentifierType,
    };
    use openidconnect::{
        AccessToken, Audience, AuthUrl, ClientId, ClientSecret, EmptyAdditionalClaims,
        EmptyAdditionalProviderMetadata, EndUserEmail, EndUserName, IssuerUrl, JsonWebKeyId,
        JsonWebKeySetUrl, LocalizedClaim, Nonce as OidcNonce, RedirectUrl, ResponseTypes,
        StandardClaims, SubjectIdentifier, TokenUrl,
    };
    use rsa::RsaPrivateKey;
    use rsa::pkcs1::EncodeRsaPrivateKey;
    use rsa::pkcs1::LineEnding;
    use rsa::traits::PublicKeyParts;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::OidcClient;

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
    }

    impl MockOidcProvider {
        /// Boot a `MockServer` and serve a freshly-generated JWKS at `/jwks`.
        /// `client_id` is the OIDC `aud` claim — must match `OIDC_CLIENT_ID`
        /// in the `AppState`'s config (`test_config()` uses an empty string;
        /// pass `""` here to match).
        pub async fn start(client_id: &str) -> Self {
            // 2048-bit RSA — fast enough for a per-test keygen and matches
            // the bar real IdPs publish (Authentik, Keycloak default to 2048).
            let mut rng = rand_core_06::OsRng;
            let private_key = RsaPrivateKey::new(&mut rng, 2048).expect("rsa keygen");
            let signing_key_pem = private_key
                .to_pkcs1_pem(LineEnding::LF)
                .expect("pkcs1 pem encode")
                .to_string();
            let n = private_key.n().to_bytes_be();
            let e = private_key.e().to_bytes_be();
            let kid = "test-kid".to_string();
            let jwk = CoreJsonWebKey::new_rsa(n, e, Some(JsonWebKeyId::new(kid.clone())));
            let jwks = CoreJsonWebKeySet::new(vec![jwk]);
            let jwks_body = serde_json::to_value(&jwks).expect("serialize jwks");

            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .and(path("/jwks"))
                .respond_with(ResponseTemplate::new(200).set_body_json(jwks_body))
                .mount(&server)
                .await;

            let issuer = server.uri();
            Self {
                server,
                signing_key_pem,
                kid,
                issuer,
                client_id: client_id.to_string(),
                jwks,
            }
        }

        /// Build an `OidcClient` bound to this mock with embedded JWKS, so
        /// `id_token_verifier` does not need network IO.
        pub fn client(&self, redirect_uri: &str) -> OidcClient {
            let issuer_url = IssuerUrl::new(self.issuer.clone()).expect("valid issuer url");
            let auth_url = AuthUrl::new(format!("{}/auth", self.issuer)).expect("auth url");
            let jwks_url =
                JsonWebKeySetUrl::new(format!("{}/jwks", self.issuer)).expect("jwks url");
            let token_url = TokenUrl::new(format!("{}/token", self.issuer)).expect("token url");

            let metadata = CoreProviderMetadata::new(
                issuer_url,
                auth_url,
                jwks_url,
                vec![ResponseTypes::new(vec![CoreResponseType::Code])],
                vec![CoreSubjectIdentifierType::Public],
                vec![CoreJwsSigningAlgorithm::RsaSsaPkcs1V15Sha256],
                EmptyAdditionalProviderMetadata {},
            )
            .set_token_endpoint(Some(token_url))
            .set_jwks(self.jwks.clone());

            CoreClient::from_provider_metadata(
                metadata,
                ClientId::new(self.client_id.clone()),
                Some(ClientSecret::new("test-secret".to_string())),
            )
            .set_redirect_uri(RedirectUrl::new(redirect_uri.to_string()).expect("redirect url"))
        }

        /// Install a `/token` responder that returns a signed ID token whose
        /// claims include the given `nonce`. Call after reading the
        /// server-stored nonce out of the shared session store but before
        /// driving `/auth/callback`.
        pub async fn mount_token_endpoint(
            &self,
            subject: &str,
            email: Option<&str>,
            name: Option<&str>,
            nonce: &str,
        ) {
            // chrono is required by openidconnect v4's public API:
            // CoreIdTokenClaims::new takes chrono::DateTime<Utc> for the
            // expiration and issued_at parameters. Project policy is `time`
            // over chrono (see backend/CLAUDE.md) — this transitive use is
            // the documented exception. Do not "migrate" to time here; it
            // would require forking openidconnect.
            use chrono::{Duration, Utc};
            let issuer_url = IssuerUrl::new(self.issuer.clone()).expect("valid issuer url");
            let access_token = AccessToken::new("test-access-token".to_string());

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
                Utc::now() + Duration::seconds(300),
                Utc::now(),
                standard_claims,
                EmptyAdditionalClaims {},
            )
            .set_nonce(Some(OidcNonce::new(nonce.to_string())));

            let signing_key = CoreRsaPrivateSigningKey::from_pem(
                &self.signing_key_pem,
                Some(JsonWebKeyId::new(self.kid.clone())),
            )
            .expect("parse signing key");

            let id_token = CoreIdToken::new(
                claims,
                &signing_key,
                CoreJwsSigningAlgorithm::RsaSsaPkcs1V15Sha256,
                Some(&access_token),
                None,
            )
            .expect("sign id token");

            let token_response = serde_json::json!({
                "access_token": access_token.secret(),
                "token_type": "Bearer",
                "expires_in": 300,
                "id_token": id_token.to_string(),
            });

            Mock::given(method("POST"))
                .and(path("/token"))
                .respond_with(ResponseTemplate::new(200).set_body_json(token_response))
                .mount(&self.server)
                .await;
        }
    }
}
