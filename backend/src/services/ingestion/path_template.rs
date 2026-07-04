//! Library path template rendering and filename-heuristic extraction.
//!
//! Paths written into the library are derived from a template (`{Author}/{Title}.{ext}`
//! by default). All user-supplied components pass through `sanitize_path_component`
//! before substitution, which strips filesystem-unsafe characters and prevents
//! path-traversal via embedded `/` or `\` sequences. The rendered path is always
//! relative; callers join it to the absolute library root before any I/O.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Default path template: `{Author}/{Title}.{ext}`
pub const DEFAULT_TEMPLATE: &str = "{Author}/{Title}.{ext}";

/// Render a path template by replacing `{key}` placeholders with values from `vars`.
/// Unknown keys are replaced with "Unknown".
///
/// Uses a single forward pass so that substituted values are never re-scanned.
/// This prevents infinite loops when a value itself contains `{...}` text.
pub fn render(template: &str, vars: &HashMap<String, String>) -> PathBuf {
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

/// Parse author and title from a filename using the `Author - Title.ext` convention.
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
#[expect(
    clippy::case_sensitive_file_extension_comparisons,
    reason = "test code: all test extensions are lowercase literals, case-insensitive comparison is unnecessary ceremony"
)]
mod tests {
    use super::*;

    #[test]
    fn render_basic_template() {
        let mut vars = HashMap::new();
        vars.insert("Author".into(), "J.K. Rowling".into());
        vars.insert("Title".into(), "Harry Potter".into());
        vars.insert("ext".into(), "epub".into());
        let result = render(DEFAULT_TEMPLATE, &vars);
        assert_eq!(result, PathBuf::from("J.K. Rowling/Harry Potter.epub"));
    }

    #[test]
    fn render_missing_var_uses_unknown() {
        let mut vars = HashMap::new();
        vars.insert("Author".into(), "Someone".into());
        vars.insert("ext".into(), "pdf".into());
        let result = render("{Author}/{Series}/{Title}.{ext}", &vars);
        assert_eq!(result, PathBuf::from("Someone/Unknown/Unknown.pdf"));
    }

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

    #[test]
    fn collision_no_conflict() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("book.epub");
        let result = resolve_collision(&path).unwrap();
        assert_eq!(result, path);
    }

    #[test]
    fn collision_with_existing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("book.epub");
        std::fs::write(&path, b"existing").unwrap();
        let result = resolve_collision(&path).unwrap();
        assert_eq!(result, dir.path().join("book (2).epub"));
    }

    #[test]
    fn collision_multiple() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("book.epub");
        std::fs::write(&path, b"1").unwrap();
        std::fs::write(dir.path().join("book (2).epub"), b"2").unwrap();
        let result = resolve_collision(&path).unwrap();
        assert_eq!(result, dir.path().join("book (3).epub"));
    }

    #[test]
    fn heuristic_author_title() {
        let vars = heuristic_vars_from_filename("J.K. Rowling - Harry Potter.epub");
        assert_eq!(vars["Author"], "J.K. Rowling");
        assert_eq!(vars["Title"], "Harry Potter");
        assert_eq!(vars["ext"], "epub");
    }

    #[test]
    fn heuristic_no_separator() {
        let vars = heuristic_vars_from_filename("JustATitle.pdf");
        assert_eq!(vars["Author"], "Unknown");
        assert_eq!(vars["Title"], "JustATitle");
        assert_eq!(vars["ext"], "pdf");
    }

    #[test]
    fn heuristic_no_extension() {
        let vars = heuristic_vars_from_filename("Some File");
        assert_eq!(vars["Author"], "Unknown");
        assert_eq!(vars["Title"], "Some File");
        assert_eq!(vars["ext"], "");
    }

    #[test]
    fn render_value_containing_braces_does_not_loop() {
        // A filename like "{Author} - Book.epub" produces Author = "{Author}".
        // The render function must not re-process the substituted value.
        let vars = heuristic_vars_from_filename("{Author} - Book.epub");
        // The brace chars in the Author value are not path-unsafe, so they pass
        // through sanitize_path_component unchanged. render must still terminate.
        let result = render(DEFAULT_TEMPLATE, &vars);
        // Just verifying it terminates and produces a path with the ext
        assert!(result.to_str().unwrap().ends_with(".epub"));
    }
}
