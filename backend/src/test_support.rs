use axum::Router;
use axum_test::TestServer;

/// Serialize tests that mutate or read environment variables so they don't
/// race with each other across modules. Import this wherever `std::env::set_var`
/// or `std::env::var("DATABASE_URL")` is used in test code.
pub static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

use crate::auth::backend::AuthBackend;
use crate::auth::oidc::OidcClient;
use crate::config::{
    CleanupMode, Config, CoverConfig, EnrichmentConfig, OpdsConfig, SecurityConfig, WritebackConfig,
};
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

/// Real-DB helpers for tests that exercise the live schema + RLS policies.
///
/// Tests use `#[sqlx::test(migrations = "./migrations")]`, which provisions
/// an isolated database per test and injects a `PgPool` owned by the
/// schema owner (`reverie` — bypasses RLS). Tests that need to exercise
/// the runtime roles (`reverie_app` / `reverie_ingestion`) build secondary
/// pools against the same per-test DB via [`app_pool_for`] / [`ingestion_pool_for`].
pub mod db {
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
        let user = crate::models::user::upsert_from_oidc_and_maybe_promote(
            app_pool,
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
        use base64ct::Encoding;
        let basic =
            base64ct::Base64::encode_string(format!("{}:{}", user.id, plaintext).as_bytes());
        (user.id, format!("Basic {basic}"))
    }

    /// Build the full router with both pools wired through AppState.
    /// AppState.pool comes from `app_pool` (reverie_app — for the route
    /// handlers' acquire_with_rls); AppState.ingestion_pool comes from
    /// `ingestion_pool` (reverie_ingestion — matches the queue + dry_run).
    pub fn server_with_real_pools(
        app_pool: &PgPool,
        ingestion_pool: &PgPool,
    ) -> axum_test::TestServer {
        use crate::auth::backend::AuthBackend;
        use crate::state::AppState;
        let state = AppState {
            pool: app_pool.clone(),
            ingestion_pool: ingestion_pool.clone(),
            config: super::test_config(),
            oidc_client: super::test_oidc_client(),
        };
        let auth_backend = AuthBackend {
            pool: app_pool.clone(),
        };
        let app = crate::build_router(state, auth_backend);
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
        use crate::auth::backend::AuthBackend;
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
        };
        let auth_backend = AuthBackend {
            pool: app_pool.clone(),
        };
        let app = crate::build_router(state, auth_backend);
        axum_test::TestServer::new(app)
    }

    /// Insert (work, manifestation) via `reverie_ingestion` for use as
    /// fixture data in route tests.  Returns `(work_id, manifestation_id)`.
    pub async fn insert_work_and_manifestation(
        ingestion_pool: &PgPool,
        marker: &str,
    ) -> (Uuid, Uuid) {
        let work_id: Uuid = sqlx::query_scalar(
            "INSERT INTO works (title, sort_title) VALUES ('', '') RETURNING id",
        )
        .fetch_one(ingestion_pool)
        .await
        .expect("insert work");
        let m_id: Uuid = sqlx::query_scalar(
            "INSERT INTO manifestations \
                (work_id, format, file_path, ingestion_file_hash, current_file_hash, \
                 file_size_bytes, ingestion_status, validation_status) \
             VALUES ($1, 'epub'::manifestation_format, $2, $3, $3, 1000, \
                     'complete'::ingestion_status, 'valid'::validation_status) \
             RETURNING id",
        )
        .bind(work_id)
        .bind(format!("/tmp/admin-test-{marker}.epub"))
        .bind(format!("admin-test-hash-{marker}"))
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
        let user =
            crate::models::user::upsert_from_oidc_and_maybe_promote(app_pool, &subject, name, None)
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
        use base64ct::Encoding;
        let basic =
            base64ct::Base64::encode_string(format!("{}:{}", user.id, plaintext).as_bytes());
        (user.id, format!("Basic {basic}"))
    }

    /// Insert an `adult`-role user via `reverie_app` (keeps default role),
    /// mint a device token, return `(user_id, "Basic …")`.
    pub async fn create_adult_and_basic_auth(app_pool: &PgPool, name: &str) -> (Uuid, String) {
        let subject = format!("adult-test-{}-{}", name, Uuid::new_v4());
        let user =
            crate::models::user::upsert_from_oidc_and_maybe_promote(app_pool, &subject, name, None)
                .await
                .expect("upsert user");
        // If upsert_from_oidc_and_maybe_promote promoted this user to admin
        // (it does so for the first user in the DB), forcibly downgrade.
        sqlx::query("UPDATE users SET role = 'adult'::user_role WHERE id = $1")
            .bind(user.id)
            .execute(app_pool)
            .await
            .expect("demote to adult");
        let (plaintext, hash) = crate::auth::token::generate_device_token();
        crate::models::device_token::create(app_pool, user.id, "adult-test", &hash)
            .await
            .expect("create token");
        use base64ct::Encoding;
        let basic =
            base64ct::Base64::encode_string(format!("{}:{}", user.id, plaintext).as_bytes());
        (user.id, format!("Basic {basic}"))
    }

    pub async fn create_shelf(app_pool: &PgPool, user_id: Uuid, name: &str) -> Uuid {
        sqlx::query_scalar("INSERT INTO shelves (user_id, name) VALUES ($1, $2) RETURNING id")
            .bind(user_id)
            .bind(name)
            .fetch_one(app_pool)
            .await
            .expect("create shelf")
    }

    pub async fn add_to_shelf(app_pool: &PgPool, shelf_id: Uuid, manifestation_id: Uuid) {
        sqlx::query(
            "INSERT INTO shelf_items (shelf_id, manifestation_id) \
             VALUES ($1, $2) ON CONFLICT DO NOTHING",
        )
        .bind(shelf_id)
        .bind(manifestation_id)
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
}
