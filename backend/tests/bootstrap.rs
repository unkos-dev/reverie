use std::process::Stdio;

use sqlx::{ConnectOptions, PgPool};
use tokio::process::Command;

fn bootstrap_command(pool: &PgPool, roots: &std::path::Path, password: &str) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_reverie-api"));
    command
        .env_clear()
        .env(
            "DATABASE_URL",
            pool.connect_options().to_url_lossy().as_str(),
        )
        .env(
            "DATABASE_URL_INGESTION",
            pool.connect_options().to_url_lossy().as_str(),
        )
        .env("REVERIE_OPDS_ENABLED", "false")
        .env("REVERIE_LIBRARY_PATH", roots.join("library"))
        .env("REVERIE_INGESTION_PATH", roots.join("ingestion"))
        .env("REVERIE_PASSWORD_BREACH_CHECK_ENABLED", "false")
        .env("REVERIE_BOOTSTRAP_EMAIL", "first@example.com")
        .env("REVERIE_BOOTSTRAP_DISPLAY_NAME", "First Administrator")
        .env("REVERIE_BOOTSTRAP_PASSWORD", password)
        .env("REVERIE_PORT", "0")
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    command
}

async fn exercise_bootstrap(pool: &PgPool, cli: bool) -> anyhow::Result<()> {
    let roots = tempfile::tempdir()?;
    std::fs::create_dir(roots.path().join("library"))?;
    std::fs::create_dir(roots.path().join("ingestion"))?;
    for password in ["short".to_owned(), "a".repeat(15), "x".repeat(257)] {
        let mut command = bootstrap_command(pool, roots.path(), &password);
        if cli {
            command.arg("bootstrap");
        }
        let output =
            tokio::time::timeout(std::time::Duration::from_secs(15), command.output()).await??;
        assert!(!output.status.success());
        let error = String::from_utf8(output.stderr)?;
        assert!(error.contains("password policy"));
        assert!(!error.contains(&password));
        assert!(!reverie_api::models::user::admin_exists(pool).await?);
    }
    let mut command = bootstrap_command(pool, roots.path(), "correct-horse-battery-staple-7!");
    if cli {
        command.arg("bootstrap");
    }
    let mut child = command.spawn()?;
    if cli {
        assert!(
            tokio::time::timeout(std::time::Duration::from_secs(15), child.wait())
                .await??
                .success()
        );
    } else {
        tokio::time::timeout(std::time::Duration::from_secs(15), async {
            loop {
                if reverie_api::models::user::admin_exists(pool).await? {
                    break;
                }
                assert!(child.try_wait()?.is_none(), "startup must reach bootstrap");
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
            Ok::<(), anyhow::Error>(())
        })
        .await??;
        child.kill().await?;
        child.wait().await?;
    }
    let user = reverie_api::models::user::find_by_email(pool, "first@example.com")
        .await?
        .ok_or_else(|| anyhow::anyhow!("administrator missing"))?;
    let credential = reverie_api::models::local_credentials::find_by_user_id(pool, user.id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("administrator credential missing"))?;
    assert!(
        reverie_api::auth::password::verify_password(
            b"correct-horse-battery-staple-7!",
            &credential.password_hash
        )
        .is_ok()
    );
    Ok(())
}

#[sqlx::test(migrations = "./migrations")]
async fn bootstrap_cli_applies_shared_password_policy(pool: PgPool) -> anyhow::Result<()> {
    exercise_bootstrap(&pool, true).await
}

#[sqlx::test(migrations = "./migrations")]
async fn bootstrap_environment_applies_shared_password_policy(pool: PgPool) -> anyhow::Result<()> {
    exercise_bootstrap(&pool, false).await
}
