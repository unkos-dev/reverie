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

fn recovery_command(pool: &PgPool, email: &str) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_reverie-api"));
    command
        .args(["reset-password", email])
        .env_clear()
        .env(
            "DATABASE_URL",
            pool.connect_options().to_url_lossy().as_str(),
        )
        .env("REVERIE_PUBLIC_URL", "https://library.example.test/")
        .kill_on_drop(true);
    command
}

#[sqlx::test(migrations = "./migrations")]
async fn smtp_cli_host_links_are_short_lived_and_choose_the_password_route(pool: PgPool) {
    use reverie_api::models::{account_email_proof, credential_link, role::Role, user};

    for has_password in [false, true] {
        let email = format!("host-{has_password}@example.test");
        let hash = reverie_api::auth::password::hash_password(b"existing password").unwrap();
        let account = user::create_local(
            &pool,
            &email,
            "Holder",
            Role::Adult,
            has_password.then_some(hash.as_str()),
        )
        .await
        .unwrap();
        let output = recovery_command(&pool, &email)
            .env("REVERIE_SMTP_ENABLED", "true")
            .env("REVERIE_SMTP_SECURITY", "invalid")
            .env("REVERIE_SMTP_PASSWORD_FILE", "/unreadable-mail-credential")
            .env("REVERIE_MAIL_QUEUE_KEY_FILE", "/unreadable-queue-key")
            .env("REVERIE_LOCAL_AUTH_ENABLED", "false")
            .output()
            .await
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8(output.stdout).unwrap();
        let action = if has_password { "reset" } else { "setup" };
        let mut lines = stdout.lines();
        assert_eq!(
            lines.next(),
            Some(format!("Password {action} link for {email} (expires in 10 minutes):").as_str())
        );
        let url = url::Url::parse(lines.next().unwrap()).unwrap();
        assert!(lines.next().is_none());
        assert_eq!(
            url.origin().ascii_serialization(),
            "https://library.example.test"
        );
        assert_eq!(
            url.path(),
            if has_password {
                "/reset-password"
            } else {
                "/set-password"
            }
        );
        let token = url.fragment().unwrap();
        assert_eq!(token.len(), 43);
        assert!(!String::from_utf8(output.stderr).unwrap().contains(token));
        let mut tx = pool.begin().await.unwrap();
        account_email_proof::lock_account(&mut tx, account.id)
            .await
            .unwrap()
            .unwrap();
        let expiry = credential_link::live_expiry(&mut tx, account.id, token)
            .await
            .unwrap()
            .unwrap();
        let seconds = (expiry - chrono::Utc::now()).num_seconds();
        assert!((590..=600).contains(&seconds), "{seconds}");
        assert!(
            credential_link::consume_replacement(&mut tx, account.id, token)
                .await
                .unwrap()
        );
        tx.commit().await.unwrap();
        assert!(
            credential_link::lookup(&pool, token)
                .await
                .unwrap()
                .is_none()
        );
        let after = user::find_by_id(&pool, account.id).await.unwrap().unwrap();
        assert_eq!(after.session_version, account.session_version);
        assert!(after.disabled_at.is_none());
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn smtp_cli_refuses_missing_or_invalid_origin_before_replacing_a_link(pool: PgPool) {
    use reverie_api::models::{credential_link, role::Role, user};

    let account = user::create_local(&pool, "host@example.test", "Holder", Role::Adult, None)
        .await
        .unwrap();
    let (original, _) = credential_link::issue(
        &mut pool.acquire().await.unwrap(),
        account.id,
        reverie_api::models::credential_link::CREDENTIAL_LINK_TTL,
    )
    .await
    .unwrap();
    for origin in [
        None,
        Some(""),
        Some("https://private:credential@example.test/"),
        Some("https://library.example.test/path"),
    ] {
        let mut command = recovery_command(&pool, "host@example.test");
        command.env_remove("REVERIE_PUBLIC_URL");
        if let Some(origin) = origin {
            command.env("REVERIE_PUBLIC_URL", origin);
        }
        let output = command.output().await.unwrap();
        assert!(!output.status.success());
        assert_eq!(output.stdout, [] as [u8; 0]);
        let error = String::from_utf8(output.stderr).unwrap();
        assert!(error.contains("REVERIE_PUBLIC_URL"), "{error}");
        assert!(!error.contains("credential@example.test"));
        assert!(
            credential_link::lookup(&pool, &original)
                .await
                .unwrap()
                .is_some()
        );
    }
    for email in ["unknown@example.test", "host@example.test"] {
        if email == "host@example.test" {
            user::disable_account(&pool, account.id).await.unwrap();
        }
        let output = recovery_command(&pool, email).output().await.unwrap();
        assert!(!output.status.success());
        assert_eq!(output.stdout, [] as [u8; 0]);
    }
}

#[tokio::test]
async fn smtp_cli_test_email_uses_only_transport_and_reports_acceptance_or_failure() {
    use tokio::io::{AsyncBufReadExt as _, AsyncWriteExt as _, BufReader};

    for accepted in [true, false] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let relay = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            let (reader, mut writer) = socket.into_split();
            writer.write_all(b"220 test relay\r\n").await.unwrap();
            let mut lines = BufReader::new(reader).lines();
            let mut body = String::new();
            let mut data = false;
            while let Some(line) = lines.next_line().await.unwrap() {
                if data {
                    if line == "." {
                        data = false;
                        writer
                            .write_all(if accepted {
                                b"250 accepted\r\n"
                            } else {
                                b"550 private-relay-error\r\n"
                            })
                            .await
                            .unwrap();
                    } else {
                        body.push_str(&line);
                        body.push('\n');
                    }
                } else if line.starts_with("EHLO") {
                    writer.write_all(b"250 test relay\r\n").await.unwrap();
                } else if line == "DATA" {
                    data = true;
                    writer.write_all(b"354 continue\r\n").await.unwrap();
                } else if line == "QUIT" {
                    writer.write_all(b"221 bye\r\n").await.unwrap();
                    break;
                } else {
                    writer.write_all(b"250 ok\r\n").await.unwrap();
                }
            }
            body
        });
        let output = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            Command::new(env!("CARGO_BIN_EXE_reverie-api"))
                .args(["send-test-email", "recipient@example.test"])
                .env_clear()
                .env("REVERIE_SMTP_HOST", "127.0.0.1")
                .env("REVERIE_SMTP_PORT", port.to_string())
                .env("REVERIE_SMTP_SECURITY", "plaintext")
                .env("REVERIE_SMTP_FROM", "sender@example.test")
                .env("REVERIE_SMTP_REPLY_TO", "support@example.test")
                .env("REVERIE_MAIL_QUEUE_KEY_FILE", "/unreadable-queue-key")
                .env("DATABASE_URL", "invalid-private-dsn")
                .env("OIDC_ISSUER_URL", "invalid")
                .env("REVERIE_LIBRARY_PATH", "invalid")
                .kill_on_drop(true)
                .output(),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(output.status.success(), accepted);
        if accepted {
            assert_eq!(
                String::from_utf8(output.stdout).unwrap(),
                "Accepted by mail server.\n"
            );
        } else {
            assert_eq!(output.stdout, [] as [u8; 0]);
            let error = String::from_utf8(output.stderr).unwrap();
            assert!(
                error
                    .contains("The mail server did not confirm the test email: permanent_refusal."),
                "{error}"
            );
            assert!(!error.contains("private-relay-error"));
        }
        let body = tokio::time::timeout(std::time::Duration::from_secs(5), relay)
            .await
            .unwrap()
            .unwrap();
        assert!(body.contains("Subject: Reverie test email"));
        assert!(body.contains("This is a test email from Reverie."));
        assert!(!body.contains("/reset-password"));
    }
}
