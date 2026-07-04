//! Startup validation of the frontend dist directory and its
//! `csp-hashes.json` sidecar produced by the Vite `reverie-csp-hash` plugin.
//!
//! Called from `reverie_api::run` when `config.security.frontend_dist_path`
//! is `Some`. Any failure is propagated as an `anyhow::Error` from `run`,
//! which exits with a non-zero status before `tracing_subscriber` binds the
//! subscriber — an operator who points the backend at a missing or
//! malformed dist must see the failure in stderr.
//!
//! # Tier 2 — security-critical
//!
//! The hash-sidecar shape is the load-bearing input to the HTML CSP. A
//! relaxed sidecar (empty array, base64url-encoded hashes browsers silently
//! drop, embedded CRLF) downgrades CSP enforcement at runtime without
//! emitting any error at the Reverie boundary. Validation here is
//! defense-in-depth on top of the Vite plugin's own emit step.

use std::fs;
use std::path::Path;
use std::sync::OnceLock;

use regex::Regex;

/// Output of a successful validation.
///
/// Hashes are passed to [`crate::security::csp::build_html_csp`] to construct
/// the HTML CSP `script-src` directive.
#[derive(Debug, Clone)]
pub struct ValidatedFrontendDist {
    /// CSP `script-src` source-expression hashes for inline scripts allowed
    /// to execute on Reverie HTML responses. Each element is pre-formatted
    /// (`sha256-<base64>` etc.) and validated against the anchored regex
    /// `^sha(256|384|512)-[A-Za-z0-9+/]+={0,2}$` before reaching this struct.
    pub script_src_hashes: Vec<String>,
}

/// Failure modes for [`validate_frontend_dist`].
///
/// All variants are operator-visible at startup; the binary exits before
/// `tracing` is initialised so the message goes to stderr in plain form.
#[derive(Debug, thiserror::Error)]
pub enum DistValidationError {
    /// Configured `frontend_dist_path` resolves to a path that does not
    /// exist on disk.
    #[error("frontend dist directory does not exist: {path}")]
    DirNotFound {
        /// Display form of the missing path.
        path: String,
    },

    /// Configured `frontend_dist_path` resolves to a non-directory (file,
    /// symlink to a non-directory, etc.).
    #[error("frontend dist path is not a directory: {path}")]
    NotADirectory {
        /// Display form of the offending path.
        path: String,
    },

    /// The dist directory exists but does not contain `index.html`.
    #[error("frontend dist index.html missing: {path}")]
    IndexHtmlMissing {
        /// Display form of the expected `index.html` path.
        path: String,
    },

    /// I/O failure reading either the dist directory metadata or the
    /// `csp-hashes.json` sidecar (permissions, transient I/O). Returned
    /// for any non-`NotFound` error from `fs::metadata` on the dist
    /// directory and for any `fs::read` failure on the sidecar file.
    #[error("csp-hashes.json: unable to read {path}: {source}")]
    SidecarRead {
        /// Display form of the path that failed to read — the sidecar
        /// path for sidecar I/O failures, or the dist directory path
        /// when the directory itself was unreadable.
        path: String,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },

    /// `csp-hashes.json` is not parseable as JSON.
    #[error("csp-hashes.json: malformed JSON: {source}")]
    SidecarParse {
        /// Underlying `serde_json` parse error.
        #[source]
        source: serde_json::Error,
    },

    /// `csp-hashes.json` parses as JSON but does not match the expected
    /// `{"script-src-hashes": [string, ...]}` shape.
    #[error(
        "csp-hashes.json: expected {{\"script-src-hashes\": [\"sha256-...\", ...]}}, got a shape that does not match"
    )]
    SidecarShape,

    /// `csp-hashes.json` parses correctly but the `script-src-hashes`
    /// array is empty. With no hashes [`crate::security::csp::build_html_csp`]
    /// emits `script-src 'self'` with no hash sources, which the browser
    /// blocks the FOUC bootstrap (and any other validated inline script)
    /// from executing — surfacing as a runtime JS error / blank screen
    /// rather than a startup failure. Refused at startup so the operator
    /// learns the Vite hash-extraction step produced an empty file
    /// before deploy.
    #[error("csp-hashes.json: 'script-src-hashes' array is empty")]
    EmptyHashes,

    /// One of the hash strings does not match
    /// `^sha(256|384|512)-<standard base64>$` (regex anchored). Browsers
    /// silently drop base64url-encoded hashes; embedded CRLF would split
    /// the CSP header. Both are blocked here.
    #[error("csp-hashes.json: invalid hash '{hash}' — expected sha(256|384|512)-<standard base64>")]
    InvalidHash {
        /// The first hash that failed regex validation.
        hash: String,
    },
}

fn hash_regex() -> &'static Regex {
    // THREAT: anchored regex (^...$) rejects embedded CRLF that would split
    // the CSP header, and rejects base64url characters (`-` `_`) that
    // browsers silently drop — accepting them would silently weaken CSP at
    // runtime without surfacing an error here. The character class is
    // restricted to RFC 4648 §4 standard base64.
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        #[expect(
            clippy::expect_used,
            reason = "hard-coded regex literal is a compile-time constant; test coverage guarantees it is valid"
        )]
        Regex::new(r"^sha(256|384|512)-[A-Za-z0-9+/]+={0,2}$")
            .expect("static CSP hash regex must compile")
    })
}

/// Validate a frontend dist directory laid out by `vite build` with the
/// `reverie-csp-hash` plugin.
///
/// Validates four invariants in order: the path exists, the path is a
/// directory, `index.html` is present, and `csp-hashes.json` parses to a
/// non-empty array of regex-conformant hash strings. Returns
/// [`ValidatedFrontendDist`] only when every invariant holds.
///
/// Threat: relaxing any of these checks downgrades CSP enforcement
/// silently at runtime — the browser would receive a header with
/// missing or browser-rejected hash sources.
///
/// # Errors
///
/// Returns [`DistValidationError::DirNotFound`] when the path does not
/// exist; [`DistValidationError::SidecarRead`] when `fs::metadata` on
/// the dist directory fails with any non-`NotFound` error (e.g.
/// permission denied) or when reading `csp-hashes.json` fails;
/// [`DistValidationError::NotADirectory`] when the path resolves to a
/// non-directory; [`DistValidationError::IndexHtmlMissing`] when
/// `index.html` is missing;
/// [`DistValidationError::SidecarParse`] when the sidecar is malformed
/// JSON; [`DistValidationError::SidecarShape`] when the JSON is
/// well-formed but the expected key is missing or not an array of
/// strings; [`DistValidationError::EmptyHashes`] when the array is
/// empty; [`DistValidationError::InvalidHash`] when any hash string
/// fails the anchored regex.
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
