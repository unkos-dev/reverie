use std::process::Command;

fn redact_startup_output(output: &str, credentials: &[&str]) -> String {
    credentials
        .iter()
        .filter(|value| !value.is_empty())
        .fold(output.to_owned(), |text, value| {
            text.replace(value, "[redacted]")
        })
}

#[test]
fn ingestion_startup_diagnostics_redact_credentials() {
    for var in [
        "DATABASE_URL",
        "DATABASE_URL_MIGRATION",
        "DATABASE_URL_INGESTION",
        "OIDC_CLIENT_SECRET",
        "REVERIE_GOOGLEBOOKS_API_KEY",
        "REVERIE_HARDCOVER_API_TOKEN",
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_reverie-api"))
            .env_clear()
            .env("DATABASE_URL", "invalid-app-dsn-private-marker")
            .env("REVERIE_OPDS_ENABLED", "false")
            .env(var, "987654321")
            .output()
            .unwrap();
        assert!(!output.status.success());
        let stderr = String::from_utf8(output.stderr).unwrap();
        let stdout = String::from_utf8(output.stdout).unwrap();
        assert!(!stderr.contains("987654321"));
        assert!(!stdout.contains("987654321"));
        assert!(stderr.contains(var));
        assert!(stderr.contains("invalid value (omitted — secret-bearing field)"));
    }
}

async fn wait_for_readiness(
    child: &mut tokio::process::Child,
    log_path: &std::path::Path,
) -> std::io::Result<()> {
    use std::time::Duration;
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

    let mut port = None;
    loop {
        if let Some(status) = child.try_wait()? {
            return Err(std::io::Error::other(format!(
                "server exited during startup: {status}"
            )));
        }
        if port.is_none() {
            let output = std::fs::read_to_string(log_path)?;
            for line in output.lines() {
                if let Some((_, address)) = line.split_once("listening on ") {
                    let address =
                        address
                            .trim()
                            .parse::<std::net::SocketAddr>()
                            .map_err(|error| {
                                std::io::Error::new(std::io::ErrorKind::InvalidData, error)
                            })?;
                    port = Some(address.port());
                    break;
                }
            }
        }
        if let Some(port) = port {
            let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port)).await?;
            stream.write_all(b"GET /health/ready HTTP/1.1\r\nHost: localhost\r\nUser-Agent: Reverie-startup-test\r\nConnection: close\r\n\r\n").await?;
            let mut status = [0; b"HTTP/1.1 200 ".len()];
            stream.read_exact(&mut status).await?;
            if &status == b"HTTP/1.1 200 " {
                return Ok(());
            }
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

fn password_input(value: Option<&str>) -> Result<&str, &'static str> {
    value
        .filter(|value| !value.is_empty())
        .ok_or("a nonempty role password is required")
}

fn required_password(name: &str) -> String {
    let value = std::env::var(name).ok();
    password_input(value.as_deref())
        .unwrap_or_else(|_| panic!("{name} requires a nonempty password"))
        .to_owned()
}

#[sqlx::test(migrations = false)]
async fn credential_file_migrate_child(pool: sqlx::PgPool) {
    use sqlx::ConnectOptions as _;
    use std::io::Write as _;

    let url = pool.connect_options().to_url_lossy();
    let mut file = tempfile::NamedTempFile::new().unwrap();
    writeln!(file, "{}", url.as_str()).unwrap();
    let output = tokio::process::Command::new(env!("CARGO_BIN_EXE_reverie-api"))
        .arg("migrate")
        .env_clear()
        .env("DATABASE_URL_MIGRATION_FILE", file.path())
        .env("DATABASE_URL_FILE", "/unreadable-app")
        .env("DATABASE_URL_INGESTION_FILE", "/unreadable-ingestion")
        .env("OIDC_CLIENT_SECRET_FILE", "/unreadable-oidc")
        .output()
        .await
        .unwrap();
    assert!(!String::from_utf8_lossy(&output.stderr).contains(url.as_str()));
    assert!(!String::from_utf8_lossy(&output.stdout).contains(url.as_str()));
    let diagnostics = redact_startup_output(
        &String::from_utf8_lossy(&output.stderr),
        &[
            url.as_str(),
            &required_password("REVERIE_MIGRATOR_PASSWORD"),
        ],
    );
    assert!(
        output.status.success(),
        "migrate child failed: {diagnostics}"
    );
    reverie_api::db::verify_schema_current(&pool).await.unwrap();
}

#[sqlx::test(migrations = "./migrations")]
async fn ingestion_startup_configured_roles_serve_ready(pool: sqlx::PgPool) {
    use sqlx::ConnectOptions as _;
    use std::time::Duration;

    let library = tempfile::tempdir().unwrap();
    let ingestion = tempfile::tempdir().unwrap();
    let log = tempfile::NamedTempFile::new().unwrap();
    let app_password = required_password("REVERIE_APP_PASSWORD");
    let ingestion_password = required_password("REVERIE_INGESTION_PASSWORD");
    let app_options = (*pool.connect_options())
        .clone()
        .username("reverie_app")
        .password(&app_password);
    let ingestion_options = (*pool.connect_options())
        .clone()
        .username("reverie_ingestion")
        .password(&ingestion_password);
    let app_url = app_options.to_url_lossy();
    let ingestion_url = ingestion_options.to_url_lossy();
    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_reverie-api"))
        .env_clear()
        .env("DATABASE_URL", app_url.as_str())
        .env("DATABASE_URL_INGESTION", ingestion_url.as_str())
        .env("REVERIE_PORT", "0")
        .env("REVERIE_OPDS_ENABLED", "false")
        .env("REVERIE_LIBRARY_PATH", library.path())
        .env("REVERIE_INGESTION_PATH", ingestion.path())
        .stdout(log.as_file().try_clone().unwrap())
        .stderr(log.as_file().try_clone().unwrap())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let result = tokio::time::timeout(
        Duration::from_secs(60),
        wait_for_readiness(&mut child, log.path()),
    )
    .await;
    if child.try_wait().unwrap().is_none() {
        child.kill().await.unwrap();
    }
    let status = child.wait().await.unwrap();
    let output = std::fs::read_to_string(log.path()).unwrap();
    for credential in [
        app_url.as_str(),
        ingestion_url.as_str(),
        app_password.as_str(),
        ingestion_password.as_str(),
    ] {
        assert!(
            !output.contains(credential),
            "credential appeared in raw startup output"
        );
    }
    let diagnostics = redact_startup_output(
        &output,
        &[
            app_url.as_str(),
            ingestion_url.as_str(),
            &app_password,
            &ingestion_password,
        ],
    );
    assert!(
        matches!(result, Ok(Ok(()))),
        "readiness failed: {result:?}; child status: {status}; output:\n{diagnostics}"
    );
}
