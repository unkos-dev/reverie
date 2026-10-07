use std::process::{Command, Output};

fn invoke(args: &[&str], ingestion_url: Option<&str>) -> std::io::Result<Output> {
    let mut command = Command::new(env!("CARGO_BIN_EXE_reverie-api"));
    command
        .args(args)
        .env_clear()
        .env("DATABASE_URL", "invalid-app-dsn-private-marker")
        .env("REVERIE_OPDS_ENABLED", "false");
    if let Some(url) = ingestion_url {
        command.env("DATABASE_URL_INGESTION", url);
    }
    command.output()
}

fn assert_missing_ingestion(ingestion_url: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
    let output = invoke(&[], ingestion_url)?;
    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr)?;
    assert!(
        stderr.contains("missing required environment variable: DATABASE_URL_INGESTION"),
        "{stderr}"
    );
    assert!(!stderr.contains("invalid-app-dsn-private-marker"));
    assert_eq!(output.stdout, [] as [u8; 0]);
    Ok(())
}

#[test]
fn ingestion_startup_missing_refuses_before_runtime_setup() {
    assert_missing_ingestion(None).unwrap();
}

#[test]
fn ingestion_startup_empty_refuses_before_runtime_setup() {
    assert_missing_ingestion(Some("")).unwrap();
}

#[test]
fn ingestion_startup_whitespace_refuses_before_runtime_setup() {
    for value in ["   ", "\t\r\n", "\u{2003}"] {
        assert_missing_ingestion(Some(value)).unwrap();
    }
}

#[test]
fn ingestion_startup_configured_role_reaches_database_setup() {
    let output = invoke(
        &[],
        Some("postgres://reverie_ingestion@localhost/reverie_dev"),
    )
    .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("failed to connect to database"), "{stderr}");
    assert!(!stderr.contains("DATABASE_URL_INGESTION"));
}

#[test]
fn ingestion_startup_admin_commands_do_not_require_ingestion_credentials() {
    for args in [
        vec!["bootstrap"],
        vec!["reset-password", "operator@example.com"],
        vec!["unlock-account", "operator@example.com"],
    ] {
        for value in [None, Some(""), Some(" \t\n")] {
            let output = invoke(&args, value).unwrap();
            assert!(!output.status.success());
            let stderr = String::from_utf8(output.stderr).unwrap();
            assert!(
                stderr.contains("connect to the database"),
                "{args:?}: {stderr}"
            );
            assert!(!stderr.contains("DATABASE_URL_INGESTION"));
        }
    }
}

#[test]
fn ingestion_startup_migration_requires_only_its_own_credentials() {
    let output = invoke(&["migrate"], None).unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("DATABASE_URL_MIGRATION"), "{stderr}");
    assert!(!stderr.contains("DATABASE_URL_INGESTION"));
}

#[test]
fn ingestion_startup_schema_printing_requires_no_credentials() {
    let output = Command::new(env!("CARGO_BIN_EXE_reverie-api"))
        .arg("print-config-schema")
        .env_clear()
        .output()
        .unwrap();
    assert!(output.status.success());
    let schema: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        schema["properties"]["ingestion_database_url"]["default"],
        ""
    );
}

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
            .env(var, "[\"credential-startup-marker\"]")
            .output()
            .unwrap();
        assert!(!output.status.success());
        let stderr = String::from_utf8(output.stderr).unwrap();
        let stdout = String::from_utf8(output.stdout).unwrap();
        assert!(!stderr.contains("credential-startup-marker"));
        assert!(!stdout.contains("credential-startup-marker"));
        assert!(!stderr.contains("invalid-app-dsn-private-marker"));
        assert!(stderr.contains(var));
    }
    let output = "connection postgres://role:private-marker@localhost/db failed: private-marker";
    assert_eq!(
        redact_startup_output(
            output,
            &[
                "postgres://role:private-marker@localhost/db",
                "private-marker",
                ""
            ]
        ),
        "connection [redacted] failed: [redacted]"
    );
    assert_eq!(
        redact_startup_output("server exited", &[""]),
        "server exited"
    );
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

#[tokio::test]
async fn ingestion_startup_readiness_reports_early_exit() {
    let log = tempfile::NamedTempFile::new().unwrap();
    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_reverie-api"))
        .env_clear()
        .env("DATABASE_URL", "invalid-app-dsn-private-marker")
        .env("REVERIE_OPDS_ENABLED", "false")
        .stdout(log.as_file().try_clone().unwrap())
        .stderr(log.as_file().try_clone().unwrap())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(60),
        wait_for_readiness(&mut child, log.path()),
    )
    .await;
    assert!(matches!(result, Ok(Err(_))), "{result:?}");
    assert!(!child.wait().await.unwrap().success());
    let output = std::fs::read_to_string(log.path()).unwrap();
    assert!(output.contains("missing required environment variable: DATABASE_URL_INGESTION"));
    assert!(!output.contains("invalid-app-dsn-private-marker"));
}

#[sqlx::test(migrations = "./migrations")]
async fn ingestion_startup_configured_roles_serve_ready(pool: sqlx::PgPool) {
    use sqlx::ConnectOptions as _;
    use std::time::Duration;

    let library = tempfile::tempdir().unwrap();
    let ingestion = tempfile::tempdir().unwrap();
    let log = tempfile::NamedTempFile::new().unwrap();
    let app_password =
        std::env::var("REVERIE_APP_PASSWORD").unwrap_or_else(|_| "reverie_app".into());
    let ingestion_password =
        std::env::var("REVERIE_INGESTION_PASSWORD").unwrap_or_else(|_| "reverie_ingestion".into());
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
