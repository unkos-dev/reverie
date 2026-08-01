//! Cover-image decodability validation layer (Layer 5).
//!
//! Checks that the cover image declared in the `OPF` manifest exists in the
//! archive and is usable: either it decodes as a raster (`JPEG`/`PNG`/`WebP`
//! via the `image` crate) or it rasterizes to a visible image as `SVG`
//! (Standard Ebooks ship `cover.svg`), reusing serve time's
//! [`crate::services::covers::svg::rasterize_svg`] with the same sibling
//! resolution so this layer's verdict matches what the serve endpoint will do
//! with the same bytes. An undecodable, unparsable, or missing cover produces
//! a `Degraded` issue rather than an `Irrecoverable` one: the book is still
//! readable without a valid cover. A cover that parses but renders nothing
//! visible (an empty `SVG`, or one whose referenced sibling image is absent)
//! is treated like no cover declared: not an issue, just not usable.

use super::{
    Issue, IssueKind, Layer, Severity,
    opf_layer::OpfData,
    zip_layer::{ZipHandle, read_entry},
};

/// Validate the cover image declared in the `OPF` manifest.
///
/// Resolves the cover href relative to the `OPF` directory, reads the entry from
/// `handle`, and accepts it if it decodes as a raster (`JPEG`/`PNG`/`WebP`) or
/// rasterizes to a visible image as `SVG` via
/// [`crate::services::covers::svg::rasterize_svg`], resolving `<image>`
/// siblings from `handle` the same way serving does. A missing, undecodable,
/// or unparsable cover appends a `Degraded` issue; no cover declared, or an
/// SVG that parses but renders nothing visible, is not an error.
///
/// Returns `true` only when the cover will actually render at serve time
/// (declared, present, and decodable, or an SVG that rasterizes to a visible
/// image) so the ingestion caller can persist that outcome on the
/// manifestation row without re-parsing the archive later. `false` covers
/// every other case: no cover declared, missing, undecodable, or an SVG that
/// resolves to a blank render.
pub fn validate(handle: &ZipHandle, opf_data: Option<&OpfData>, issues: &mut Vec<Issue>) -> bool {
    let Some(opf) = opf_data else { return false };

    let cover_href = find_cover_href(opf);
    let Some(href) = cover_href else {
        return false; // No cover declared — not an error, but nothing embedded
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
        return false;
    };

    // Attempt to decode as a raster first. SVG-declared covers aren't
    // raster-decodable.
    if image::load_from_memory(&bytes).is_ok() {
        return true; // decodable raster, no issue
    }
    if crate::services::covers::svg::looks_like_svg(&bytes) {
        return validate_svg(handle, &entry_path, &bytes, href, issues);
    }
    issues.push(Issue {
        layer: Layer::Cover,
        severity: Severity::Degraded,
        kind: IssueKind::UndecodableCover { href },
    });
    false
}

/// Resolve the usability of an SVG-declared cover by rasterizing it exactly as
/// serving would: same [`crate::services::covers::svg::rasterize_svg`] routine,
/// same sibling resolution scoped to `entry_path`'s directory within `handle`.
///
/// A rasterization failure after
/// [`crate::services::covers::svg::parses_as_svg`] passes is *not* an
/// ingestion error: the two calls do not evaluate the same tree (live sibling
/// resolution adds nodes and trips guards that its no-op resolver never
/// reaches), and rasterization can also fail on a blank render, a
/// non-positive declared size, pixmap allocation, or PNG encoding.
/// `parses_as_svg` is the conservative discriminator: anything it accepts is
/// silently non-usable, mirroring the "no cover declared" case above, rather
/// than gaining an `UndecodableCover` issue. A genuine parse or render-cost
/// failure still appends `UndecodableCover`, unchanged from before.
fn validate_svg(
    handle: &ZipHandle,
    entry_path: &str,
    bytes: &[u8],
    href: String,
    issues: &mut Vec<Issue>,
) -> bool {
    let cover_dir = entry_path.rfind('/').map_or("", |i| &entry_path[..i]);
    let rasterized = crate::services::covers::svg::rasterize_svg(bytes, |sibling_href| {
        let sibling_path =
            crate::services::covers::extract::join_sibling_path(cover_dir, sibling_href)?;
        read_entry(handle, &sibling_path)
    });
    if rasterized.is_ok() {
        return true;
    }
    if crate::services::covers::svg::parses_as_svg(bytes) {
        // rasterize_svg can fail for reasons beyond a genuine parse/gate
        // problem: a blank render, size/allocation/encode failures, or
        // sibling-resolution guards that parses_as_svg's no-op resolver never
        // reaches. This fallback is therefore load-bearing: it keeps every
        // cover the resolver-free gate accepts out of UndecodableCover,
        // matching serve's spine fallback for this shape.
        return false;
    }
    issues.push(Issue {
        layer: Layer::Cover,
        severity: Severity::Degraded,
        kind: IssueKind::UndecodableCover { href },
    });
    false
}

/// Find the cover image href from the `OPF`.
///
/// Prefers the EPUB 3 standard: the manifest item carrying
/// `properties="cover-image"` ([`OpfData::cover_href`]) — this is how Standard
/// Ebooks (and most modern EPUBs) declare the cover, with an arbitrary `id`
/// like `cover.svg`. Falls back to the EPUB 2 `<meta name="cover"
/// content="ID"/>` declaration ([`OpfData::meta_cover_href`]) — an explicit
/// author declaration outranks a guess, so this comes before the id
/// heuristic. Falls back last to the legacy id heuristic (`id="cover-image"`,
/// `id="cover"`, …) for EPUBs that predate both.
///
/// Exported so `services::covers::extract` can mirror Step 5 detection
/// semantics exactly — any divergence between the validation pass and the
/// OPDS cover serve would be a silent correctness hazard.
pub fn find_cover_href(opf: &OpfData) -> Option<String> {
    if let Some(href) = &opf.cover_href {
        return Some(href.clone());
    }
    if let Some(href) = &opf.meta_cover_href {
        return Some(href.clone());
    }
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
            cover_href: None,
            meta_cover_href: None,
            spine_idrefs: vec![],
            opf_path: "OEBPS/content.opf".to_string(),
            accessibility_metadata: None,
            title: None,
            subtitle: None,
            creators: vec![],
            number_of_pages: None,
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
        let has_cover = validate(&handle, Some(&opf), &mut issues);
        assert!(
            issues.is_empty(),
            "expected no issues for valid cover: {issues:?}"
        );
        assert!(has_cover, "a decodable raster cover must report has_cover");
    }

    #[test]
    fn no_cover_declared_reports_no_cover_without_issues() {
        // No manifest entry keyed "cover"/"cover-image"/etc and no cover_href:
        // find_cover_href resolves to None. Not an error, but not a usable
        // embedded cover either.
        let handle = make_handle_with_cover(b"unused");
        let opf = make_opf_data("chapter1", "chapter1.xhtml");
        let mut issues = Vec::new();
        let has_cover = validate(&handle, Some(&opf), &mut issues);
        assert!(
            issues.is_empty(),
            "no cover declared is not an error: {issues:?}"
        );
        assert!(!has_cover, "no cover declared must not report has_cover");
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
        let has_cover = validate(&handle, Some(&opf), &mut issues);
        assert!(issues.iter().any(|i| {
            i.severity == Severity::Degraded && matches!(&i.kind, IssueKind::MissingCover { .. })
        }));
        assert!(!has_cover, "a missing cover file must not report has_cover");
    }

    #[test]
    fn undecodable_cover_emits_degraded() {
        let handle = make_handle_with_cover(b"not an image");
        let opf = make_opf_data("cover", "cover.jpg");
        let mut issues = Vec::new();
        let has_cover = validate(&handle, Some(&opf), &mut issues);
        assert!(issues.iter().any(|i| {
            i.severity == Severity::Degraded
                && matches!(&i.kind, IssueKind::UndecodableCover { .. })
        }));
        assert!(!has_cover, "an undecodable cover must not report has_cover");
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

    // Real Standard Ebooks shape: cover declared via properties="cover-image"
    // with a non-magic id, so detection must come from `cover_href`, not the id
    // heuristic.
    fn make_se_svg_opf(href: &str) -> OpfData {
        let mut opf = make_opf_data("cover.svg", href);
        opf.cover_href = Some(href.to_string());
        opf
    }

    #[test]
    fn svg_cover_emits_no_issues() {
        // Standard Ebooks ship cover.svg; a parseable SVG must not be flagged.
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="150"><rect width="100" height="150"/></svg>"#;
        let handle = make_handle_with_svg_cover(svg);
        let opf = make_se_svg_opf("cover.svg");
        let mut issues = Vec::new();
        let has_cover = validate(&handle, Some(&opf), &mut issues);
        assert!(
            issues.is_empty(),
            "expected no issues for SVG cover: {issues:?}"
        );
        assert!(has_cover, "a parseable SVG cover must report has_cover");
    }

    #[test]
    fn malformed_svg_cover_emits_degraded() {
        let handle = make_handle_with_svg_cover(b"<svg><broken");
        let opf = make_se_svg_opf("cover.svg");
        let mut issues = Vec::new();
        let has_cover = validate(&handle, Some(&opf), &mut issues);
        assert!(issues.iter().any(|i| {
            i.severity == Severity::Degraded
                && matches!(&i.kind, IssueKind::UndecodableCover { .. })
        }));
        assert!(
            !has_cover,
            "a malformed SVG cover must not report has_cover"
        );
    }

    #[test]
    fn filtered_svg_cover_emits_degraded() {
        // A cover that parses but carries a filter primitive is a render-cost
        // bomb (rejected at serve). Ingestion must agree and flag it Degraded
        // up front, not accept it and surprise the serve path with a placeholder.
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="150"><filter id="b"><feGaussianBlur stdDeviation="8"/></filter><rect width="100" height="150" fill="black" filter="url(#b)"/></svg>"#;
        let handle = make_handle_with_svg_cover(svg);
        let opf = make_se_svg_opf("cover.svg");
        let mut issues = Vec::new();
        let has_cover = validate(&handle, Some(&opf), &mut issues);
        assert!(issues.iter().any(|i| {
            i.severity == Severity::Degraded
                && matches!(&i.kind, IssueKind::UndecodableCover { .. })
        }));
        assert!(!has_cover, "a filtered SVG cover must not report has_cover");
    }

    fn make_handle_with_svg_cover_and_sibling(
        svg_bytes: &[u8],
        sibling_name: &str,
        sibling_bytes: &[u8],
    ) -> ZipHandle {
        use std::io::Write;
        let buf = std::io::Cursor::new(Vec::new());
        let mut w = zip::ZipWriter::new(buf);
        let opts: zip::write::FileOptions<zip::write::ExtendedFileOptions> =
            zip::write::FileOptions::default();
        w.start_file("OEBPS/cover.svg", opts.clone()).unwrap();
        w.write_all(svg_bytes).unwrap();
        let sibling_path = format!("OEBPS/{sibling_name}");
        w.start_file(&sibling_path, opts).unwrap();
        w.write_all(sibling_bytes).unwrap();
        let bytes = w.finish().unwrap().into_inner();
        ZipHandle {
            bytes,
            entries: vec!["OEBPS/cover.svg".to_string(), sibling_path],
        }
    }

    #[test]
    fn empty_svg_cover_reports_not_usable_without_issues() {
        // Parses and clears the render-cost gate, but has no paintable content:
        // it rasterizes to a blank pixmap. Serving falls back to the spine for this
        // shape (see covers::svg::rasterize_svg), so ingestion must agree the
        // cover isn't usable, the same way "no cover declared" isn't an error.
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="150" viewBox="0 0 100 150"></svg>"#;
        let handle = make_handle_with_svg_cover(svg);
        let opf = make_se_svg_opf("cover.svg");
        let mut issues = Vec::new();
        let has_cover = validate(&handle, Some(&opf), &mut issues);
        assert!(
            issues.is_empty(),
            "a blank-rendering SVG is not an ingestion error: {issues:?}"
        );
        assert!(!has_cover, "an empty SVG cover must not report has_cover");
    }

    #[test]
    fn svg_cover_with_unresolved_sibling_reports_not_usable_without_issues() {
        // Cover's only content is an <image> whose href has no matching ZIP
        // entry: usvg drops the unresolvable node and the render comes back
        // blank. Serving falls back to the spine for this shape too, so the
        // ingestion-time flag must not claim the cover is usable.
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="150" viewBox="0 0 100 150"><image href="missing.jpg" width="100" height="150"/></svg>"#;
        let handle = make_handle_with_svg_cover(svg);
        let opf = make_se_svg_opf("cover.svg");
        let mut issues = Vec::new();
        let has_cover = validate(&handle, Some(&opf), &mut issues);
        assert!(
            issues.is_empty(),
            "an unresolved sibling is not an ingestion error: {issues:?}"
        );
        assert!(
            !has_cover,
            "an SVG cover with an unresolved sibling must not report has_cover"
        );
    }

    #[test]
    fn svg_cover_with_resolvable_sibling_reports_usable() {
        // Companion positive case: the sibling actually resolves from the same
        // ZipHandle the validator holds, proving sibling resolution (not just
        // the parse/gate check) is exercised and a real image renders.
        let sibling = {
            let img = image::DynamicImage::new_rgb8(2, 2);
            let mut buf = Vec::new();
            img.write_to(
                &mut std::io::Cursor::new(&mut buf),
                image::ImageFormat::Jpeg,
            )
            .unwrap();
            buf
        };
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="150" viewBox="0 0 100 150"><image href="cover.jpg" width="100" height="150"/></svg>"#;
        let handle = make_handle_with_svg_cover_and_sibling(svg, "cover.jpg", &sibling);
        let opf = make_se_svg_opf("cover.svg");
        let mut issues = Vec::new();
        let has_cover = validate(&handle, Some(&opf), &mut issues);
        assert!(
            issues.is_empty(),
            "a resolvable sibling must not produce issues: {issues:?}"
        );
        assert!(
            has_cover,
            "an SVG cover whose sibling resolves must report has_cover"
        );
    }

    #[test]
    fn find_cover_href_prefers_properties_over_id() {
        // cover_href (from properties="cover-image") wins even when the id is
        // not one of the legacy magic ids.
        let opf = make_se_svg_opf("images/cover.svg");
        assert_eq!(find_cover_href(&opf).as_deref(), Some("images/cover.svg"));
    }

    #[test]
    fn find_cover_href_falls_back_to_id() {
        // Legacy EPUBs without the property: the id heuristic still resolves.
        let opf = make_opf_data("cover", "cover.jpg");
        assert_eq!(find_cover_href(&opf).as_deref(), Some("cover.jpg"));
    }

    fn make_epub2_meta_opf(meta_href: &str) -> OpfData {
        // The manifest id ("cvr") is deliberately not one of the legacy magic
        // ids, so only the EPUB 2 <meta name="cover"> resolution can find it.
        let mut opf = make_opf_data("cvr", meta_href);
        opf.meta_cover_href = Some(meta_href.to_string());
        opf
    }

    #[test]
    fn find_cover_href_resolves_epub2_meta_only_cover() {
        // Book declares its cover only via <meta name="cover" content="ID"/>,
        // with no EPUB 3 property and no magic-id manifest item.
        let opf = make_epub2_meta_opf("images/cover.jpg");
        assert_eq!(find_cover_href(&opf).as_deref(), Some("images/cover.jpg"));
    }

    #[test]
    fn find_cover_href_meta_beats_magic_id() {
        // Both a resolved EPUB 2 meta cover and a magic-id manifest entry
        // (a different image) exist -- the explicit meta declaration
        // outranks the id guess.
        let mut opf = make_opf_data("cover-image", "images/wrong-cover.jpg");
        opf.meta_cover_href = Some("images/right-cover.jpg".to_string());
        assert_eq!(
            find_cover_href(&opf).as_deref(),
            Some("images/right-cover.jpg")
        );
    }

    #[test]
    fn find_cover_href_property_wins_over_epub2_meta() {
        // Both an EPUB 3 property="cover-image" and an EPUB 2 meta exist --
        // the explicit modern property still wins over the meta fallback.
        let mut opf = make_epub2_meta_opf("images/legacy-cover.jpg");
        opf.cover_href = Some("images/modern-cover.svg".to_string());
        assert_eq!(
            find_cover_href(&opf).as_deref(),
            Some("images/modern-cover.svg")
        );
    }
}
