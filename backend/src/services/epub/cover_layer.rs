//! Cover-image decodability validation layer (Layer 5).
//!
//! Checks that the cover image declared in the `OPF` manifest exists in the
//! archive and is usable: either it decodes as a raster (`JPEG`/`PNG`/`WebP`
//! via the `image` crate) or it parses as `SVG` (Standard Ebooks ship
//! `cover.svg`). SVG covers are only *parsed* here — rasterization to `PNG`
//! happens lazily at serve time in [`crate::services::covers`]. An undecodable,
//! unparsable, or missing cover produces a `Degraded` issue rather than an
//! `Irrecoverable` one — the book is still readable without a valid cover.

use super::{
    Issue, IssueKind, Layer, Severity,
    opf_layer::OpfData,
    zip_layer::{ZipHandle, read_entry},
};

/// Validate the cover image declared in the `OPF` manifest (`JPEG` or `PNG` only).
///
/// Resolves the cover href relative to the `OPF` directory, reads the entry from
/// `handle`, and attempts to decode it with the `image` crate. A missing or
/// undecodable cover appends a `Degraded` issue; no cover declared is not an error.
pub fn validate(handle: &ZipHandle, opf_data: Option<&OpfData>, issues: &mut Vec<Issue>) {
    let Some(opf) = opf_data else { return };

    let cover_href = find_cover_href(opf);
    let Some(href) = cover_href else {
        return; // No cover declared — not an error
    };

    let opf_dir = opf.opf_path.rfind('/').map_or("", |i| &opf.opf_path[..i]);
    let entry_path = if opf_dir.is_empty() {
        href.clone()
    } else {
        format!("{opf_dir}/{href}")
    };

    let Some(bytes) = read_entry(handle, &entry_path) else {
        issues.push(Issue {
            layer: Layer::Cover,
            severity: Severity::Degraded,
            kind: IssueKind::MissingCover { href },
        });
        return;
    };

    // Attempt to decode as a raster first. SVG-declared covers aren't
    // raster-decodable — accept them when they parse as SVG (parse only; no
    // rasterization at ingestion). Anything else → Degraded.
    match image::load_from_memory(&bytes) {
        Ok(_) => {} // decodable raster — no issue
        Err(_)
            if crate::services::covers::svg::looks_like_svg(&bytes)
                && crate::services::covers::svg::parses_as_svg(&bytes) => {}
        Err(_) => {
            issues.push(Issue {
                layer: Layer::Cover,
                severity: Severity::Degraded,
                kind: IssueKind::UndecodableCover { href },
            });
        }
    }
}

/// Find the cover image href from `OPF` manifest/metadata.
/// Checks manifest item with `id="cover-image"`, `id="cover"`, etc.
///
/// Exported so `services::covers::extract` can mirror Step 5 detection
/// semantics exactly — any divergence between the validation pass and the
/// OPDS cover serve would be a silent correctness hazard.
pub fn find_cover_href(opf: &OpfData) -> Option<String> {
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
            title: None,
            creators: vec![],
            description: None,
            publisher: None,
            date: None,
            language: None,
            identifiers: vec![],
            subjects: vec![],
            series_meta: None,
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
            "expected no issues for valid cover: {issues:?}"
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

    fn make_handle_with_svg_cover(svg_bytes: &[u8]) -> ZipHandle {
        use std::io::Write;
        let buf = std::io::Cursor::new(Vec::new());
        let mut w = zip::ZipWriter::new(buf);
        let opts: zip::write::FileOptions<zip::write::ExtendedFileOptions> =
            zip::write::FileOptions::default();
        w.start_file("OEBPS/cover.svg", opts).unwrap();
        w.write_all(svg_bytes).unwrap();
        let bytes = w.finish().unwrap().into_inner();
        ZipHandle {
            bytes,
            entries: vec!["OEBPS/cover.svg".to_string()],
        }
    }

    #[test]
    fn svg_cover_emits_no_issues() {
        // Standard Ebooks ship cover.svg; a parseable SVG must not be flagged.
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="150"><rect width="100" height="150"/></svg>"#;
        let handle = make_handle_with_svg_cover(svg);
        let opf = make_opf_data("cover-image", "cover.svg");
        let mut issues = Vec::new();
        validate(&handle, Some(&opf), &mut issues);
        assert!(
            issues.is_empty(),
            "expected no issues for SVG cover: {issues:?}"
        );
    }

    #[test]
    fn malformed_svg_cover_emits_degraded() {
        let handle = make_handle_with_svg_cover(b"<svg><broken");
        let opf = make_opf_data("cover-image", "cover.svg");
        let mut issues = Vec::new();
        validate(&handle, Some(&opf), &mut issues);
        assert!(issues.iter().any(|i| {
            i.severity == Severity::Degraded
                && matches!(&i.kind, IssueKind::UndecodableCover { .. })
        }));
    }
}
