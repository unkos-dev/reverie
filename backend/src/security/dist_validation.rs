//! Startup validation of the frontend dist directory and its
//! `csp-hashes.json` sidecar produced by the Vite `reverie-csp-hash` plugin.
//!
//! Called from `main()` when `config.security.frontend_dist_path` is `Some`.
//! Any failure panics the process before `tracing_subscriber` binds the
//! subscriber (existing main.rs `.expect()` pattern) — an operator who points
//! the backend at a missing or malformed dist must see the failure in stderr.

use std::fs;
use std::path::Path;
use std::sync::OnceLock;

use regex::Regex;

/// Output of a successful validation — hashes are passed to
/// [`crate::security::csp::build_html_csp`] to construct the HTML CSP header.
#[derive(Debug, Clone)]
pub struct ValidatedFrontendDist {
    pub script_src_hashes: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum DistValidationError {
    #[error("frontend dist directory does not exist: {path}")]
    DirNotFound { path: String },
    #[error("frontend dist path is not a directory: {path}")]
    NotADirectory { path: String },
    #[error("frontend dist index.html missing: {path}")]
    IndexHtmlMissing { path: String },
    #[error("csp-hashes.json: unable to read {path}: {source}")]
    SidecarRead {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("csp-hashes.json: malformed JSON: {source}")]
    SidecarParse {
        #[source]
        source: serde_json::Error,
    },
    #[error(
        "csp-hashes.json: expected {{\"script-src-hashes\": [\"sha256-...\", ...]}}, got a shape that does not match"
    )]
    SidecarShape,
    #[error("csp-hashes.json: 'script-src-hashes' array is empty")]
    EmptyHashes,
    #[error("csp-hashes.json: invalid hash '{hash}' — expected sha(256|384|512)-<standard base64>")]
    InvalidHash { hash: String },
}

fn hash_regex() -> &'static Regex {
    // Standard RFC 4648 §4 base64 with padding. Browsers silently drop
    // base64url-encoded hashes, so `-` / `_` are rejected here.
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"^sha(256|384|512)-[A-Za-z0-9+/]+={0,2}$")
            .expect("static CSP hash regex must compile")
    })
}

/// Validate a frontend dist directory laid out by `vite build` with the
/// `reverie-csp-hash` plugin.
pub fn validate_frontend_dist(path: &Path) -> Result<ValidatedFrontendDist, DistValidationError> {
    let path_display = path.display().to_string();
    let metadata = match fs::metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(DistValidationError::DirNotFound { path: path_display });
        }
        Err(e) => {
            return Err(DistValidationError::SidecarRead {
                path: path_display,
                source: e,
            });
        }
    };
    if !metadata.is_dir() {
        return Err(DistValidationError::NotADirectory { path: path_display });
    }

    let index = path.join("index.html");
    let index_ok = fs::metadata(&index).is_ok_and(|m| m.is_file());
    if !index_ok {
        return Err(DistValidationError::IndexHtmlMissing {
            path: index.display().to_string(),
        });
    }

    let sidecar = path.join("csp-hashes.json");
    let bytes = fs::read(&sidecar).map_err(|e| DistValidationError::SidecarRead {
        path: sidecar.display().to_string(),
        source: e,
    })?;
    let v: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|e| DistValidationError::SidecarParse { source: e })?;

    let array = v
        .as_object()
        .and_then(|o| o.get("script-src-hashes"))
        .and_then(|v| v.as_array())
        .ok_or(DistValidationError::SidecarShape)?;

    if array.is_empty() {
        return Err(DistValidationError::EmptyHashes);
    }

    let mut hashes = Vec::with_capacity(array.len());
    let re = hash_regex();
    for item in array {
        let s = item.as_str().ok_or(DistValidationError::SidecarShape)?;
        if !re.is_match(s) {
            return Err(DistValidationError::InvalidHash { hash: s.to_owned() });
        }
        hashes.push(s.to_owned());
    }

    Ok(ValidatedFrontendDist {
        script_src_hashes: hashes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn write_sidecar(dir: &Path, body: &str) {
        fs::write(dir.join("csp-hashes.json"), body).unwrap();
    }

    fn make_valid_dist(body: &str) -> TempDir {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("index.html"), b"<html></html>").unwrap();
        write_sidecar(tmp.path(), body);
        tmp
    }

    #[test]
    fn dir_not_found() {
        let tmp = TempDir::new().unwrap();
        let missing = tmp.path().join("does-not-exist");
        let err = validate_frontend_dist(&missing).unwrap_err();
        assert!(
            matches!(err, DistValidationError::DirNotFound { .. }),
            "{err}"
        );
    }

    #[test]
    fn not_a_directory() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("not-a-dir");
        fs::write(&file, b"").unwrap();
        let err = validate_frontend_dist(&file).unwrap_err();
        assert!(
            matches!(err, DistValidationError::NotADirectory { .. }),
            "{err}"
        );
    }

    #[test]
    fn index_html_missing() {
        let tmp = TempDir::new().unwrap();
        let err = validate_frontend_dist(tmp.path()).unwrap_err();
        assert!(
            matches!(err, DistValidationError::IndexHtmlMissing { .. }),
            "{err}"
        );
    }

    #[test]
    fn sidecar_missing() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("index.html"), b"").unwrap();
        let err = validate_frontend_dist(tmp.path()).unwrap_err();
        assert!(
            matches!(err, DistValidationError::SidecarRead { .. }),
            "{err}"
        );
    }

    #[test]
    fn sidecar_malformed_json() {
        let tmp = make_valid_dist("{not json");
        let err = validate_frontend_dist(tmp.path()).unwrap_err();
        assert!(
            matches!(err, DistValidationError::SidecarParse { .. }),
            "{err}"
        );
    }

    #[test]
    fn sidecar_missing_key() {
        let tmp = make_valid_dist(r#"{"other":["x"]}"#);
        let err = validate_frontend_dist(tmp.path()).unwrap_err();
        assert!(matches!(err, DistValidationError::SidecarShape), "{err}");
    }

    #[test]
    fn sidecar_hashes_not_array() {
        let tmp = make_valid_dist(r#"{"script-src-hashes":"sha256-abc"}"#);
        let err = validate_frontend_dist(tmp.path()).unwrap_err();
        assert!(matches!(err, DistValidationError::SidecarShape), "{err}");
    }

    #[test]
    fn sidecar_empty_array() {
        let tmp = make_valid_dist(r#"{"script-src-hashes":[]}"#);
        let err = validate_frontend_dist(tmp.path()).unwrap_err();
        assert!(matches!(err, DistValidationError::EmptyHashes), "{err}");
    }

    #[test]
    fn sidecar_invalid_hash_base64url_chars() {
        // base64url uses - and _ which CSP browsers silently drop.
        let tmp = make_valid_dist(r#"{"script-src-hashes":["sha256-ab-cd_"]}"#);
        let err = validate_frontend_dist(tmp.path()).unwrap_err();
        assert!(
            matches!(err, DistValidationError::InvalidHash { .. }),
            "{err}"
        );
    }

    #[test]
    fn sidecar_invalid_hash_missing_prefix() {
        let tmp = make_valid_dist(r#"{"script-src-hashes":["abc123="]}"#);
        let err = validate_frontend_dist(tmp.path()).unwrap_err();
        assert!(
            matches!(err, DistValidationError::InvalidHash { .. }),
            "{err}"
        );
    }

    #[test]
    fn sidecar_invalid_hash_wrong_algo() {
        let tmp = make_valid_dist(r#"{"script-src-hashes":["sha1-YWJj"]}"#);
        let err = validate_frontend_dist(tmp.path()).unwrap_err();
        assert!(
            matches!(err, DistValidationError::InvalidHash { .. }),
            "{err}"
        );
    }

    #[test]
    fn sidecar_invalid_hash_crlf_rejected() {
        // Anchored regex ensures embedded CRLF cannot pass validation.
        let tmp = make_valid_dist("{\"script-src-hashes\":[\"sha256-YWJjZA==\\r\\nInjected: x\"]}");
        let err = validate_frontend_dist(tmp.path()).unwrap_err();
        assert!(
            matches!(err, DistValidationError::InvalidHash { .. }),
            "{err}"
        );
    }

    #[test]
    fn happy_path_one_hash() {
        let tmp = make_valid_dist(r#"{"script-src-hashes":["sha256-YWJjZA=="]}"#);
        let ok = validate_frontend_dist(tmp.path()).unwrap();
        assert_eq!(ok.script_src_hashes, vec!["sha256-YWJjZA=="]);
    }

    #[test]
    fn happy_path_two_hashes_all_algos() {
        let body = r#"{"script-src-hashes":["sha384-YWJjZA==","sha512-YWJjZA=="]}"#;
        let tmp = make_valid_dist(body);
        let ok = validate_frontend_dist(tmp.path()).unwrap();
        assert_eq!(ok.script_src_hashes.len(), 2);
    }
}
