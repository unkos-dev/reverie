//! Library path template rendering and filename-heuristic extraction.
//!
//! Paths written into the library are derived from a template (`{Author}/{Title}.{ext}`
//! by default). All user-supplied components pass through `sanitize_path_component`
//! before substitution, which strips filesystem-unsafe characters and prevents
//! path-traversal via embedded `/` or `\` sequences. The rendered path is always
//! relative; callers join it to the absolute library root before any I/O.

use std::collections::HashMap;
use std::hash::BuildHasher;
use std::path::{Path, PathBuf};

/// Default path template: `{Author}/{Title}.{ext}`
pub const DEFAULT_TEMPLATE: &str = "{Author}/{Title}.{ext}";

/// Render a path template by replacing `{key}` placeholders with values from `vars`.
/// Unknown keys are replaced with "Unknown".
///
/// Uses a single forward pass so that substituted values are never re-scanned.
/// This prevents infinite loops when a value itself contains `{...}` text.
#[must_use]
pub fn render<S: BuildHasher>(template: &str, vars: &HashMap<String, String, S>) -> PathBuf {
    let mut output = String::with_capacity(template.len() + 64);
    let mut remaining = template;

    while let Some(start) = remaining.find('{') {
        output.push_str(&remaining[..start]);
        remaining = &remaining[start..];
        if let Some(end) = remaining.find('}') {
            let key = &remaining[1..end];
            let value = vars
                .get(key)
                .cloned()
                .unwrap_or_else(|| "Unknown".to_string());
            output.push_str(&sanitize_path_component(&value));
            remaining = &remaining[end + 1..];
        } else {
            // Unmatched '{' — emit literally and stop scanning
            output.push_str(remaining);
            remaining = "";
        }
    }
    output.push_str(remaining);
    PathBuf::from(output)
}

/// Sanitize a single path component by replacing filesystem-unsafe characters with `_`.
///
/// Replaces `/`, `\`, `:`, `*`, `?`, `"`, `<`, `>`, `|`, and `\0` — the union of
/// characters forbidden on POSIX, Windows `NTFS`, and common network filesystems.
/// Leading/trailing whitespace and dots are trimmed; consecutive underscores are
/// collapsed to one. An empty result is replaced with `"Unknown"`.
///
/// This function is the path-traversal guard for the library write path: because
/// `/` and `\` are replaced, a metadata field containing `../../etc/passwd` cannot
/// escape the library root when its sanitized value is joined to an absolute base.
#[must_use]
pub fn sanitize_path_component(s: &str) -> String {
    let mut result: String = s
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '\0' => '_',
            _ => c,
        })
        .collect();

    // Trim leading/trailing whitespace and dots
    result = result.trim().trim_matches('.').to_string();

    // Collapse consecutive underscores
    while result.contains("__") {
        result = result.replace("__", "_");
    }

    if result.is_empty() {
        result = "Unknown".to_string();
    }

    result
}

/// If `path` already exists, append ` (2)`, ` (3)`, etc. until a free name is found.
///
/// Checks for the existence of the path and up to 998 suffixed candidates.
/// The returned path is guaranteed not to exist at the moment of the check,
/// though a subsequent create may still race on highly concurrent systems.
///
/// # Errors
///
/// Returns `std::io::Error` with kind `AlreadyExists` if all 998 suffixed
/// candidates are occupied and no free slot can be found.
pub fn resolve_collision(path: &Path) -> std::io::Result<PathBuf> {
    if !path.exists() {
        return Ok(path.to_path_buf());
    }

    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("file");
    let ext = path.extension().and_then(|e| e.to_str());
    let parent = path.parent().unwrap_or_else(|| Path::new("."));

    for i in 2..=999 {
        let new_name = ext.map_or_else(|| format!("{stem} ({i})"), |e| format!("{stem} ({i}).{e}"));
        let candidate = parent.join(new_name);
        if !candidate.exists() {
            return Ok(candidate);
        }
    }

    Err(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        format!("could not resolve collision for {}", path.display()),
    ))
}

/// Render one candidate using the existing collision suffix convention.
///
/// # Errors
/// Returns an invalid or non-UTF8 relative path.
pub fn collision_candidate(
    candidate: &crate::services::files::RelativeFilePath,
    suffix: u64,
) -> std::io::Result<crate::services::files::RelativeFilePath> {
    if suffix == 1 {
        return Ok(candidate.clone());
    }
    let mut path = candidate.as_path().to_owned();
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .ok_or_else(|| std::io::Error::other("missing collision stem"))?;
    let name = path
        .extension()
        .and_then(|extension| extension.to_str())
        .map_or_else(
            || format!("{stem} ({suffix})"),
            |extension| format!("{stem} ({suffix}).{extension}"),
        );
    path.set_file_name(name);
    path.to_str()
        .ok_or_else(|| std::io::Error::other("non-UTF8 collision path"))?
        .parse()
        .map_err(std::io::Error::other)
}

/// Parse author and title from a filename using the `Author - Title.ext` convention.
#[must_use]
pub fn heuristic_vars_from_filename(filename: &str) -> HashMap<String, String> {
    let mut vars = HashMap::new();

    // Strip extension
    let stem = Path::new(filename)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(filename);

    let ext = Path::new(filename)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");

    if let Some((author, title)) = stem.split_once(" - ") {
        vars.insert("Author".into(), author.trim().to_string());
        vars.insert("Title".into(), title.trim().to_string());
    } else {
        vars.insert("Author".into(), "Unknown".into());
        vars.insert("Title".into(), stem.trim().to_string());
    }

    vars.insert("ext".into(), ext.to_lowercase());
    vars
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_unsafe_chars() {
        assert_eq!(sanitize_path_component("Author: Name"), "Author_ Name");
        assert_eq!(sanitize_path_component("A/B\\C"), "A_B_C");
        assert_eq!(sanitize_path_component("***"), "_");
    }

    #[test]
    fn sanitize_dots_and_whitespace() {
        assert_eq!(sanitize_path_component("...hidden"), "hidden");
        assert_eq!(sanitize_path_component("  spaced  "), "spaced");
        assert_eq!(sanitize_path_component(""), "Unknown");
    }
}
