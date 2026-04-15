use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Select the highest-priority format for each filename stem.
///
/// Groups files by `(parent_directory, lowercase_stem)` so that files with the
/// same name in different subdirectories are treated as distinct books. For each
/// group, picks the file whose extension matches the earliest entry in `priority`.
/// Files with unknown or missing extensions are silently ignored.
pub fn select_by_priority(files: &[PathBuf], priority: &[String]) -> Vec<PathBuf> {
    // Group files by (parent dir, lowercase stem) — not stem alone.
    // Stem-only grouping incorrectly collapses files from different directories
    // (e.g., Fantasy/Foundation.epub and SciFi/Foundation.pdf) into one group.
    let mut groups: HashMap<(PathBuf, String), Vec<&PathBuf>> = HashMap::new();
    for file in files {
        let parent = file.parent().unwrap_or(Path::new("")).to_path_buf();
        if let Some(stem) = file.file_stem().and_then(|s| s.to_str()) {
            groups
                .entry((parent, stem.to_lowercase()))
                .or_default()
                .push(file);
        }
    }

    let mut selected = Vec::new();
    for candidates in groups.values() {
        let mut best: Option<(usize, &PathBuf)> = None;
        for candidate in candidates {
            let ext = candidate
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.to_lowercase());
            if let Some(ext) = ext
                && let Some(pos) = priority.iter().position(|p| *p == ext)
                && (best.is_none() || pos < best.unwrap().0)
            {
                best = Some((pos, candidate));
            }
        }
        if let Some((_, path)) = best {
            selected.push(path.clone());
        }
    }

    // Sort for deterministic output
    selected.sort();
    selected
}

#[cfg(test)]
mod tests {
    use super::*;

    fn priority() -> Vec<String> {
        vec![
            "epub".into(),
            "pdf".into(),
            "mobi".into(),
            "azw3".into(),
            "cbz".into(),
            "cbr".into(),
        ]
    }

    #[test]
    fn single_epub() {
        let files = vec![PathBuf::from("a.epub")];
        let result = select_by_priority(&files, &priority());
        assert_eq!(result, vec![PathBuf::from("a.epub")]);
    }

    #[test]
    fn epub_beats_pdf_same_stem() {
        let files = vec![PathBuf::from("a.pdf"), PathBuf::from("a.epub")];
        let result = select_by_priority(&files, &priority());
        assert_eq!(result, vec![PathBuf::from("a.epub")]);
    }

    #[test]
    fn no_matching_format() {
        let files = vec![PathBuf::from("a.docx"), PathBuf::from("b.txt")];
        let result = select_by_priority(&files, &priority());
        assert!(result.is_empty());
    }

    #[test]
    fn case_insensitive_extension() {
        let files = vec![PathBuf::from("a.EPUB")];
        let result = select_by_priority(&files, &priority());
        assert_eq!(result, vec![PathBuf::from("a.EPUB")]);
    }

    #[test]
    fn multiple_titles() {
        let files = vec![PathBuf::from("a.epub"), PathBuf::from("b.pdf")];
        let result = select_by_priority(&files, &priority());
        assert_eq!(
            result,
            vec![PathBuf::from("a.epub"), PathBuf::from("b.pdf")]
        );
    }

    #[test]
    fn files_with_no_extension_ignored() {
        let files = vec![PathBuf::from("noext"), PathBuf::from("a.epub")];
        let result = select_by_priority(&files, &priority());
        assert_eq!(result, vec![PathBuf::from("a.epub")]);
    }

    #[test]
    fn custom_priority_pdf_first() {
        let priority = vec!["pdf".into(), "epub".into()];
        let files = vec![PathBuf::from("a.epub"), PathBuf::from("a.pdf")];
        let result = select_by_priority(&files, &priority);
        assert_eq!(result, vec![PathBuf::from("a.pdf")]);
    }

    #[test]
    fn same_stem_different_dirs_not_grouped() {
        // Two books with the same title in different subdirectories are distinct.
        let files = vec![
            PathBuf::from("Fantasy/Foundation.epub"),
            PathBuf::from("SciFi/Foundation.pdf"),
        ];
        let result = select_by_priority(&files, &priority());
        // Both are selected — they live in different directories
        assert_eq!(result.len(), 2);
        assert!(result.contains(&PathBuf::from("Fantasy/Foundation.epub")));
        assert!(result.contains(&PathBuf::from("SciFi/Foundation.pdf")));
    }

    #[test]
    fn same_stem_same_dir_grouped() {
        // Two formats of the same book in the same directory — pick highest priority.
        let files = vec![
            PathBuf::from("Fantasy/Foundation.pdf"),
            PathBuf::from("Fantasy/Foundation.epub"),
        ];
        let result = select_by_priority(&files, &priority());
        assert_eq!(result, vec![PathBuf::from("Fantasy/Foundation.epub")]);
    }
}
