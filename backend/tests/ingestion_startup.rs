use std::process::{Command, Output};

fn invoke(args: &[&str], ingestion_url: Option<&str>) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_reverie-api"));
    command
        .args(args)
        .env_clear()
        .env("DATABASE_URL", "invalid-app-dsn-private-marker")
        .env("REVERIE_OPDS_ENABLED", "false");
    if let Some(url) = ingestion_url {
        command.env("DATABASE_URL_INGESTION", url);
    }
    command.output().unwrap()
}

fn assert_missing_ingestion(ingestion_url: Option<&str>) {
    let output = invoke(&[], ingestion_url);
    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains("missing required environment variable: DATABASE_URL_INGESTION"),
        "{stderr}"
    );
    assert!(!stderr.contains("invalid-app-dsn-private-marker"));
    assert!(output.stdout.is_empty());
}

#[test]
fn ingestion_startup_missing_refuses_before_runtime_setup() {
    assert_missing_ingestion(None);
}

#[test]
fn ingestion_startup_empty_refuses_before_runtime_setup() {
    assert_missing_ingestion(Some(""));
}

#[test]
fn ingestion_startup_whitespace_refuses_before_runtime_setup() {
    for value in ["   ", "\t\r\n", "\u{2003}"] {
        assert_missing_ingestion(Some(value));
    }
}

#[test]
fn ingestion_startup_configured_role_reaches_database_setup() {
    let output = invoke(
        &[],
        Some("postgres://reverie_ingestion@localhost/reverie_dev"),
    );
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
            let output = invoke(&args, value);
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
    let output = invoke(&["migrate"], None);
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

#[sqlx::test(migrations = "./migrations")]
async fn ingestion_startup_configured_roles_serve_ready(pool: sqlx::PgPool) {
    use sqlx::ConnectOptions as _;
    use std::process::Stdio;
    use std::time::Duration;

    let library = tempfile::tempdir().unwrap();
    let ingestion = tempfile::tempdir().unwrap();
    let socket = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = socket.local_addr().unwrap().port();
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
    drop(socket);
    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_reverie-api"))
        .env_clear()
        .env("DATABASE_URL", app_options.to_url_lossy().as_str())
        .env(
            "DATABASE_URL_INGESTION",
            ingestion_options.to_url_lossy().as_str(),
        )
        .env("REVERIE_PORT", port.to_string())
        .env("REVERIE_OPDS_ENABLED", "false")
        .env("REVERIE_LIBRARY_PATH", library.path())
        .env("REVERIE_INGESTION_PATH", ingestion.path())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(1))
        .build()
        .unwrap();
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            assert!(
                child.try_wait().unwrap().is_none(),
                "server exited during startup"
            );
            if let Ok(response) = client
                .get(format!("http://127.0.0.1:{port}/health/ready"))
                .send()
                .await
                && response.status().is_success()
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("configured runtime roles must reach readiness");
    child.kill().await.unwrap();
}
