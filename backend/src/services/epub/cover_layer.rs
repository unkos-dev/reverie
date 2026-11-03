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
    if raster_is_decodable(&bytes) {
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

pub(crate) fn raster_is_decodable(bytes: &[u8]) -> bool {
    image::load_from_memory(bytes).is_ok()
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
/// Exported so `services::covers::extract` can mirror this detection
/// semantics exactly — any divergence between the validation pass and the
/// OPDS cover serve would be a silent correctness hazard.
#[must_use]
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
