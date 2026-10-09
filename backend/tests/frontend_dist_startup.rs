use std::path::Path;
use std::process::{Command, Output};

const VALID_HASH: &str = "sha256-Zm9vYmFyYmF6cXV4Zm9vYmFyYmF6cXV4Zm9vYmFyYmE=";

fn start_with_dist(dist: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_reverie-api"))
        .env_clear()
        .env("DATABASE_URL", "invalid-app-dsn-private-marker")
        .env(
            "DATABASE_URL_INGESTION",
            "postgres://reverie_ingestion@localhost/reverie_dev",
        )
        .env("REVERIE_OPDS_ENABLED", "false")
        .env("REVERIE_FRONTEND_DIST_PATH", dist)
        .output()
        .unwrap()
}

fn dist_with_sidecar(sidecar: Option<&str>) -> tempfile::TempDir {
    let dist = tempfile::tempdir().unwrap();
    std::fs::write(dist.path().join("index.html"), "<html></html>").unwrap();
    if let Some(body) = sidecar {
        std::fs::write(dist.path().join("csp-hashes.json"), body).unwrap();
    }
    dist
}

fn assert_refused_by_dist_validation(dist: &Path, case: &str) {
    let output = start_with_dist(dist);
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(!output.status.success(), "{case}: exited zero\n{stderr}");
    assert!(
        stderr.contains("frontend dist validation failed"),
        "{case}: {stderr}"
    );
    assert!(
        !stderr.contains("failed to connect to database"),
        "{case}: startup got past dist validation\n{stderr}"
    );
    assert_eq!(output.stdout, [] as [u8; 0], "{case}");
}

#[test]
fn a_missing_hash_set_exits_non_zero_before_any_connection() {
    assert_refused_by_dist_validation(dist_with_sidecar(None).path(), "missing sidecar");

    let unreadable = dist_with_sidecar(None);
    std::fs::create_dir(unreadable.path().join("csp-hashes.json")).unwrap();
    assert_refused_by_dist_validation(unreadable.path(), "unreadable sidecar");
}

#[test]
fn a_malformed_hash_set_exits_non_zero_before_any_connection() {
    let empty_array = r#"{"script-src-hashes": []}"#;
    let not_an_array = r#"{"script-src-hashes": "sha256-abc"}"#;
    for (case, body) in [
        ("not JSON", "not json"),
        ("missing field", r#"{"other": []}"#),
        ("field not an array", not_an_array),
        ("empty array", empty_array),
    ] {
        assert_refused_by_dist_validation(dist_with_sidecar(Some(body)).path(), case);
    }
}

#[test]
fn an_invalid_hash_entry_exits_non_zero_before_any_connection() {
    let base64url = format!(r#"{{"script-src-hashes": ["{VALID_HASH}", "sha256-ab_c-d"]}}"#);
    let unsupported = r#"{"script-src-hashes": ["sha1-Zm9vYmFy"]}"#;
    let line_break = format!(r#"{{"script-src-hashes": ["{VALID_HASH}\r\nX-Injected: 1"]}}"#);
    for (case, body) in [
        ("base64url digest", base64url.as_str()),
        ("unsupported algorithm", unsupported),
        ("embedded CR and LF", line_break.as_str()),
    ] {
        assert_refused_by_dist_validation(dist_with_sidecar(Some(body)).path(), case);
    }
}

#[test]
fn a_valid_hash_set_passes_validation_and_startup_continues() {
    let body = format!(r#"{{"script-src-hashes": ["{VALID_HASH}"]}}"#);
    let dist = dist_with_sidecar(Some(&body));
    let output = start_with_dist(dist.path());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(!output.status.success());
    assert!(stderr.contains("failed to connect to database"), "{stderr}");
    assert!(!stderr.contains("frontend dist validation failed"));
}
