use super::{
    Issue, IssueKind, Layer, Severity,
    opf_layer::OpfData,
    zip_layer::{ZipHandle, read_entry},
};

/// Validate the cover image (JPEG or PNG only).
pub fn validate(handle: &ZipHandle, opf_data: Option<&OpfData>, issues: &mut Vec<Issue>) {
    let Some(opf) = opf_data else { return };

    let cover_href = find_cover_href(opf);
    let Some(href) = cover_href else {
        return; // No cover declared — not an error
    };

    let opf_dir = opf
        .opf_path
        .rfind('/')
        .map(|i| &opf.opf_path[..i])
        .unwrap_or("");
    let entry_path = if opf_dir.is_empty() {
        href.clone()
    } else {
        format!("{opf_dir}/{href}")
    };

    let Some(bytes) = read_entry(handle, &entry_path) else {
        issues.push(Issue {
            layer: Layer::Cover,
            severity: Severity::Degraded,
            kind: IssueKind::MissingCover { href: href.clone() },
        });
        return;
    };

    // Attempt to decode as JPEG or PNG. Other formats → Degraded.
    // image crate compiled with default-features = false, features = ["jpeg", "png"] only.
    match image::load_from_memory(&bytes) {
        Ok(_) => {} // decodable — no issue
        Err(_) => {
            issues.push(Issue {
                layer: Layer::Cover,
                severity: Severity::Degraded,
                kind: IssueKind::UndecodableCover { href: href.clone() },
            });
        }
    }
}

/// Find the cover image href from OPF manifest/metadata.
/// Checks manifest item with `id="cover-image"`, `id="cover"`, etc.
fn find_cover_href(opf: &OpfData) -> Option<String> {
    for id in &["cover-image", "cover", "Cover", "Cover-Image"] {
        if let Some(href) = opf.manifest.get(*id) {
            return Some(href.clone());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::epub::{opf_layer::OpfData, zip_layer::ZipHandle};
    use std::collections::HashMap;

    fn make_handle_with_cover(cover_bytes: &[u8]) -> ZipHandle {
        use std::io::Write;
        let buf = std::io::Cursor::new(Vec::new());
        let mut w = zip::ZipWriter::new(buf);
        let opts: zip::write::FileOptions<zip::write::ExtendedFileOptions> =
            zip::write::FileOptions::default();
        w.start_file("OEBPS/cover.jpg", opts).unwrap();
        w.write_all(cover_bytes).unwrap();
        let bytes = w.finish().unwrap().into_inner();
        ZipHandle {
            bytes,
            entries: vec!["OEBPS/cover.jpg".to_string()],
        }
    }

    fn make_opf_data(manifest_id: &str, href: &str) -> OpfData {
        let mut manifest = HashMap::new();
        manifest.insert(manifest_id.to_string(), href.to_string());
        OpfData {
            manifest,
            spine_idrefs: vec![],
            opf_path: "OEBPS/content.opf".to_string(),
            accessibility_metadata: None,
        }
    }

    #[test]
    fn valid_cover_image_emits_no_issues() {
        // P3: a decodable cover image must produce zero issues.
        // Generate a minimal 1×1 PNG using the image crate (already a dependency).
        let mut png_bytes: Vec<u8> = Vec::new();
        let img = image::DynamicImage::new_rgb8(1, 1);
        img.write_to(
            &mut std::io::Cursor::new(&mut png_bytes),
            image::ImageFormat::Png,
        )
        .expect("png encode should succeed with png feature enabled");

        let handle = make_handle_with_cover(&png_bytes);
        let opf = make_opf_data("cover", "cover.jpg");
        let mut issues = Vec::new();
        validate(&handle, Some(&opf), &mut issues);
        assert!(
            issues.is_empty(),
            "expected no issues for valid cover: {:?}",
            issues
        );
    }

    #[test]
    fn missing_cover_file_emits_degraded() {
        let handle = ZipHandle {
            bytes: {
                use std::io::Write;
                let buf = std::io::Cursor::new(Vec::new());
                let mut w = zip::ZipWriter::new(buf);
                let opts: zip::write::FileOptions<zip::write::ExtendedFileOptions> =
                    zip::write::FileOptions::default();
                w.start_file("OEBPS/content.opf", opts).unwrap();
                w.write_all(b"<package/>").unwrap();
                w.finish().unwrap().into_inner()
            },
            entries: vec!["OEBPS/content.opf".to_string()],
        };
        let opf = make_opf_data("cover", "cover.jpg");
        let mut issues = Vec::new();
        validate(&handle, Some(&opf), &mut issues);
        assert!(issues.iter().any(|i| {
            i.severity == Severity::Degraded && matches!(&i.kind, IssueKind::MissingCover { .. })
        }));
    }

    #[test]
    fn undecodable_cover_emits_degraded() {
        let handle = make_handle_with_cover(b"not an image");
        let opf = make_opf_data("cover", "cover.jpg");
        let mut issues = Vec::new();
        validate(&handle, Some(&opf), &mut issues);
        assert!(issues.iter().any(|i| {
            i.severity == Severity::Degraded
                && matches!(&i.kind, IssueKind::UndecodableCover { .. })
        }));
    }
}
