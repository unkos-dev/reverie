//! SVG cover rasterization (Tier 2 — parses untrusted XML from user EPUBs).
//!
//! Standard Ebooks EPUBs declare their cover as `images/cover.svg`
//! (`media-type="image/svg+xml"`). The raster pipeline cannot decode SVG, so
//! this module sniffs SVG at the extraction boundary and rasterizes it to PNG
//! via `resvg`/`usvg`/`tiny-skia`. The PNG then flows through the existing
//! resize/cache/serve path unchanged — **no SVG bytes ever leave the server**,
//! so there is no stored-XSS surface on the cover route.
//!
//! THREAT: input is attacker-controlled XML pulled from an uploaded EPUB.
//! Defenses, all enforced here:
//! - **XXE / entity expansion**: we parse with `allow_dtd: false`, so any cover
//!   carrying a DOCTYPE is rejected before entity processing — no external-entity
//!   fetch, no billion-laughs, and no entity-inflated nesting that could slip
//!   past the byte-scan depth guard. (`roxmltree` performs zero IO regardless.)
//! - **Local file read via `<image href>`**: `usvg`'s default `resolve_string`
//!   reads local files. We override it to resolve only sibling ZIP entries
//!   (path-traversal-checked) and override `resolve_data` to bound base64
//!   data-URI payloads. No `resources_dir` is set.
//! - **Decode bombs**: `resvg`'s raster decoders have no built-in limits
//!   (resvg#647). Every embedded raster (sibling *and* data-URI) is bounded by
//!   byte size (`MAX_EMBEDDED_IMAGE_BYTES`) and header-sniffed megapixels
//!   (`MAX_EMBEDDED_PIXELS`) before decode, and a cumulative decoded-pixel
//!   budget (`MAX_TOTAL_DECODED_PIXELS`) bounds the aggregate across every
//!   image one rasterization admits, so many small payloads cannot multiply
//!   the per-image cap.
//! - **Resolution amplification**: usvg consults the sibling resolver once per
//!   `<image href>` element while building the tree — before the node budget
//!   can reject it — and each consultation decompresses a ZIP entry, so many
//!   references to one entry multiply that cost. A cumulative per-rasterization
//!   budget (`MAX_SIBLING_RESOLUTIONS`, `MAX_TOTAL_SIBLING_BYTES`) refuses
//!   further resolutions once spent.
//! - **Output-size bomb**: render output is capped at `MAX_RENDER_LONG_EDGE`;
//!   `Pixmap::new` returning `None` (zero/oversize) maps to a decode error.
//! - **Parser stack overflow**: both `roxmltree`'s parser and `usvg`'s tree
//!   conversion recurse on element nesting with no depth guard and *abort the
//!   process* on deeply nested SVG. `check_nesting_depth` bounds nesting with a
//!   flat scan of the raw bytes (no parse, no recursion — cannot itself overflow)
//!   *before* either touches the input.
//! - **Render-cost bomb**: filters and vector tessellation escape the output
//!   cap — resvg allocates filter buffers to the filter region, not the canvas,
//!   and tessellation scales with segment count. `check_render_complexity`
//!   rejects covers using filter primitives or exceeding segment/node budgets
//!   *before* `resvg::render`. Both this and the depth bound are applied at
//!   ingestion (Layer 5) too, so acceptance and serve-time agree.
//! - **Silent transparent cover**: `usvg` drops unresolvable `<image>` nodes
//!   and "succeeds" with a blank pixmap. We reject all-transparent renders so
//!   the existing `<img onError>` spine fallback is preserved.
//!
//! See ADR `docs/adr/0024-rasterize-svg-declared-epub-covers-to-png-via-resvg.md`.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use resvg::{tiny_skia, usvg};

use super::error::CoverError;

/// Hard cap on raw SVG input. Bounds DTD entity-expansion input and base64
/// data-URI payloads (which live inside the SVG bytes).
const MAX_SVG_INPUT_BYTES: usize = 4 * 1024 * 1024;
/// Hard cap on a single embedded raster (sibling entry or decoded data-URI).
const MAX_EMBEDDED_IMAGE_BYTES: usize = 8 * 1024 * 1024;
/// Hard cap on an embedded raster's pixel count (decode-bomb guard).
const MAX_EMBEDDED_PIXELS: u64 = 16 * 1024 * 1024;
/// Cumulative cap on decoded pixels admitted across one rasterization, both
/// resolvers combined. The per-image megapixel cap bounds each decode and the
/// input cap bounds encoded bytes, but neither bounds aggregate decode work:
/// a few-KB payload can decode to the per-image cap, and the node budget
/// admits thousands of images. Accounted from header-sniffed dimensions, so
/// refusal costs no decode. 4x the per-image cap covers any real cover (one
/// artwork image) with wide headroom.
const MAX_TOTAL_DECODED_PIXELS: u64 = 64 * 1024 * 1024;
/// Cap on `<image href>` sibling resolutions in one rasterization. usvg
/// consults the resolver once per `<image>` element while building the tree,
/// before `check_render_complexity` can reject it, and every consultation
/// costs a ZIP-entry decompression, so the per-image caps alone let a single
/// cover reference one entry thousands of times. Real covers use one sibling.
const MAX_SIBLING_RESOLUTIONS: usize = 8;
/// Cumulative byte cap across all sibling fetches in one rasterization; belt
/// over [`MAX_SIBLING_RESOLUTIONS`] so total fetched bytes stay bounded even
/// when every fetch is under the per-image cap. Once exceeded, remaining
/// resolutions return `None` without touching the archive.
const MAX_TOTAL_SIBLING_BYTES: usize = 32 * 1024 * 1024;
/// Long-edge cap of the rasterized output, in pixels. Matches `CoverSize::Full`
/// so the downstream resize step is a near-no-op.
const MAX_RENDER_LONG_EDGE: u32 = 1200;
/// Hard cap on total vector path segments across the whole render tree.
/// Tessellation cost scales with segment count independent of the output-size
/// cap, so this bounds render CPU. Real covers are sub-1k; this leaves wide
/// headroom while rejecting geometry-spam bombs.
const MAX_SVG_PATH_SEGMENTS: usize = 50_000;
/// Hard cap on total nodes (groups/paths/images) across the render tree.
/// Backstops element-explosion bombs (e.g. millions of empty groups) that
/// carry no path segments yet still cost per-node render work.
const MAX_SVG_NODES: usize = 50_000;
/// Hard cap on element-nesting depth, enforced on the raw bytes *before* any
/// parse. Both `roxmltree`'s parser and usvg's tree conversion recurse on nesting
/// with no guard and abort the process on deep input; depth-50 parses + renders
/// fine in tests while depth-1000 overflows, so this cap sits comfortably inside
/// the verified-safe range with ~10× headroom over real covers (a few levels
/// deep). Also reused as the belt bound on the complexity walk's own recursion.
const MAX_SVG_DEPTH: u32 = 48;

/// Cheap byte-prefix sniff for SVG. Skips a UTF-8 BOM and leading ASCII
/// whitespace, then matches `<?xml` or `<svg` (case-insensitive for the tag).
///
/// Deliberately rejects gzip-compressed `.svgz` (magic `1f 8b`): keeping it out
/// of the rasterize path preserves today's behaviour for those inputs.
pub(crate) fn looks_like_svg(bytes: &[u8]) -> bool {
    let mut b = bytes;
    if let Some(rest) = b.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        b = rest;
    }
    let start = b
        .iter()
        .position(|c| !c.is_ascii_whitespace())
        .unwrap_or(b.len());
    let b = &b[start..];
    starts_with_ci(b, b"<?xml") || starts_with_ci(b, b"<svg")
}

fn starts_with_ci(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.len() >= needle.len() && haystack[..needle.len()].eq_ignore_ascii_case(needle)
}

/// Rasterize `svg_bytes` to PNG, resolving any `<image href>` siblings via
/// `resolve_sibling` (returns the raw bytes of a sibling entry, or `None`).
///
/// # Errors
///
/// Returns [`CoverError::Decode`] for anything [`parse_and_gate`] rejects
/// (oversized input, non-UTF-8, unparsable SVG, excessive nesting depth, or
/// unbounded render cost), plus non-positive or unallocatable render dimensions,
/// PNG-encode failure, or an all-transparent render (so the caller's spine
/// fallback still fires).
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "render dimensions are non-negative and bounded to MAX_RENDER_LONG_EDGE before the cast"
)]
pub(crate) fn rasterize_svg<F>(svg_bytes: &[u8], resolve_sibling: F) -> Result<Vec<u8>, CoverError>
where
    F: Fn(&str) -> Option<Vec<u8>> + Send + Sync,
{
    let tree = parse_and_gate(svg_bytes, resolve_sibling)?;

    let size = tree.size();
    let long = size.width().max(size.height());
    if !long.is_finite() || long <= 0.0 {
        return Err(CoverError::Decode("svg has non-positive size".to_owned()));
    }

    // Render at the long-edge cap regardless of the SVG's native size — SVG is
    // resolution-independent, and this bounds output allocation no matter how
    // large the declared viewBox is.
    let scale = f32::from(u16::try_from(MAX_RENDER_LONG_EDGE).unwrap_or(u16::MAX)) / long;
    let width = ((size.width() * scale).round() as u32).max(1);
    let height = ((size.height() * scale).round() as u32).max(1);

    let mut pixmap = tiny_skia::Pixmap::new(width, height)
        .ok_or_else(|| CoverError::Decode(format!("pixmap alloc failed for {width}x{height}")))?;

    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );

    // THREAT: usvg silently drops unresolvable <image> nodes and "succeeds"
    // with a transparent pixmap. Serving that as 200 would kill the spine
    // fallback. Reject all-transparent output.
    if !pixmap.data().as_chunks::<4>().0.iter().any(|px| px[3] != 0) {
        return Err(CoverError::Decode(
            "svg rendered no visible content".to_owned(),
        ));
    }

    pixmap
        .encode_png()
        .map_err(|e| CoverError::Decode(format!("png encode: {e}")))
}

/// Parse/gate-only check reused by the ingestion validator (Layer 5) to
/// classify *why* a full rasterization attempt failed. Does not rasterize and
/// resolves no siblings: it answers "does this parse and clear the
/// render-cost gate?". Shares [`parse_and_gate`] with serve-time
/// [`rasterize_svg`], so the two cannot disagree on the parse/gate verdict: a
/// cover that clears those gates here will not be rejected by them at serve.
///
/// A `true` result is not sufficient on its own for ingestion to call a cover
/// usable: an SVG can clear the parse/gate check yet still resolve to nothing
/// visible (an `<image>` whose sibling is absent, or an empty `<svg>`), which
/// is exactly what [`rasterize_svg`]'s all-transparent and non-positive-size
/// checks reject. The validator calls [`rasterize_svg`] first, with the same
/// sibling resolution serving uses, and falls back to this function only to
/// tell a genuine parse/gate failure (worth an `UndecodableCover` issue) apart
/// from a visibility-only one (not an error, just not usable).
pub(crate) fn parses_as_svg(svg_bytes: &[u8]) -> bool {
    parse_and_gate(svg_bytes, |_: &str| None).is_ok()
}

/// Parse `svg_bytes` into a hardened usvg tree, enforcing every pre-render
/// guard. The single source of truth shared by serve-time [`rasterize_svg`] and
/// ingestion [`parses_as_svg`], so the two cannot disagree on the parse-and-gate
/// verdict. (The serve path additionally rasterizes and rejects a non-positive
/// declared size or an all-transparent render — the size check keys on the
/// declared viewport, the transparency check on sibling resolution — so both
/// live only there; see [`parses_as_svg`].)
///
/// THREAT pipeline, in order: input-size cap → **raw-byte nesting-depth bound** →
/// UTF-8 → `roxmltree` parse → usvg conversion → render-cost gate. The depth
/// bound is load-bearing and must run on the raw bytes *before any parse*:
/// **both** `roxmltree`'s parser **and** usvg's tree conversion recurse on
/// element nesting with no guard and stack-overflow (process abort) on deeply
/// nested SVG. The byte scan ([`check_nesting_depth`]) is a flat loop, so it
/// cannot itself overflow.
///
/// # Errors
///
/// [`CoverError::Decode`] on input over [`MAX_SVG_INPUT_BYTES`], nesting past
/// [`MAX_SVG_DEPTH`] (see [`check_nesting_depth`]), non-UTF-8 bytes, an XML parse
/// failure, a usvg conversion failure, or a render-cost breach (see
/// [`check_render_complexity`]).
fn parse_and_gate<F>(svg_bytes: &[u8], resolve_sibling: F) -> Result<usvg::Tree, CoverError>
where
    F: Fn(&str) -> Option<Vec<u8>> + Send + Sync,
{
    if svg_bytes.len() > MAX_SVG_INPUT_BYTES {
        return Err(CoverError::Decode(format!(
            "svg input {} exceeds {MAX_SVG_INPUT_BYTES} byte cap",
            svg_bytes.len()
        )));
    }

    // THREAT: bound nesting depth on the raw bytes, before roxmltree or usvg
    // touch it — both recurse on nesting and abort the process on deep input.
    check_nesting_depth(svg_bytes)?;

    let svg_str = std::str::from_utf8(svg_bytes)
        .map_err(|_| CoverError::Decode("svg is not valid UTF-8".to_owned()))?;

    // THREAT: disable DTD processing. With `allow_dtd: true`, roxmltree expands
    // internal `<!ENTITY>` definitions at parse time, inflating nesting depth past
    // what check_nesting_depth measured on the raw bytes (entity *bodies* live in
    // the doctype, which the scanner skips) — a stack-overflow bypass. Disabling
    // it also closes XXE and billion-laughs outright. Real covers carry no DTD; a
    // cover that does is rejected (→ spine fallback).
    let xml_opt = usvg::roxmltree::ParsingOptions {
        allow_dtd: false,
        ..Default::default()
    };
    let doc = usvg::roxmltree::Document::parse_with_options(svg_str, xml_opt)
        .map_err(|e| CoverError::Decode(format!("svg parse: {e}")))?;

    let opt = usvg::Options {
        image_href_resolver: hardened_resolver(resolve_sibling),
        ..Default::default()
    };
    let tree = usvg::Tree::from_xmltree(&doc, &opt)
        .map_err(|e| CoverError::Decode(format!("svg parse: {e}")))?;

    // THREAT: bound render cost before resvg::render (parse is cheap + input-
    // capped; render is the bomb). Rejects filters and geometry-spam.
    check_render_complexity(&tree)?;
    Ok(tree)
}

/// Reject SVG whose element nesting exceeds [`MAX_SVG_DEPTH`], scanned from the
/// raw bytes **before any XML parse**.
///
/// THREAT: both `roxmltree`'s parser and usvg's tree conversion recurse on
/// element nesting with no depth guard and abort the process (stack overflow) on
/// deep input — so depth must be bounded before either runs. This is a flat byte
/// scan (no parse, no recursion): it cannot itself overflow. Comments, CDATA,
/// processing instructions and the doctype are skipped; element opens increment
/// depth, close and self-closing tags do not. Conservative — it never
/// undercounts genuine nesting.
fn check_nesting_depth(bytes: &[u8]) -> Result<(), CoverError> {
    let mut depth: u32 = 0;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'<' {
            i += 1;
            continue;
        }
        let rest = &bytes[i..];
        // Skip non-element constructs whose bodies may contain stray `<`/`>`.
        if let Some(skip) = skip_delimited(rest, b"<!--", b"-->")
            .or_else(|| skip_delimited(rest, b"<![CDATA[", b"]]>"))
            .or_else(|| skip_delimited(rest, b"<?", b"?>"))
            .or_else(|| skip_delimited(rest, b"<!", b">"))
        {
            i += skip;
            continue;
        }
        if rest.starts_with(b"</") {
            depth = depth.saturating_sub(1);
            i += find_after(rest, b">").unwrap_or(rest.len());
            continue;
        }
        // Element open: `<name ...>` or `<name .../>`.
        let (tag_len, self_closing) = scan_open_tag(rest);
        if !self_closing {
            depth += 1;
            if depth > MAX_SVG_DEPTH {
                return Err(CoverError::Decode(format!(
                    "svg nesting exceeds depth {MAX_SVG_DEPTH}"
                )));
            }
        }
        i += tag_len;
    }
    Ok(())
}

/// If `rest` begins with `open`, return the byte length up to and including the
/// first following `close` (or to end-of-input if unterminated). Otherwise `None`.
fn skip_delimited(rest: &[u8], open: &[u8], close: &[u8]) -> Option<usize> {
    rest.starts_with(open)
        .then(|| find_after(rest, close).unwrap_or(rest.len()))
}

/// Byte offset just past the first occurrence of `needle` in `hay`, or `None`.
fn find_after(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len())
        .position(|w| w == needle)
        .map(|p| p + needle.len())
}

/// From a slice starting at the `<` of an element-open tag, return the length
/// consumed (up to and including `>`) and whether the tag self-closes. Quotes are
/// respected so a `>` inside an attribute value does not end the tag early.
const fn scan_open_tag(rest: &[u8]) -> (usize, bool) {
    let mut quote = 0u8;
    let mut j = 1; // past '<'
    while j < rest.len() {
        let c = rest[j];
        if quote != 0 {
            if c == quote {
                quote = 0;
            }
        } else if c == b'"' || c == b'\'' {
            quote = c;
        } else if c == b'>' {
            let self_closing = rest[j - 1] == b'/';
            return (j + 1, self_closing);
        }
        j += 1;
    }
    (rest.len(), false) // unterminated tag; roxmltree will reject it downstream
}

/// Reject SVG covers whose render cost is unbounded relative to the output-size
/// cap, *before* `resvg::render`. Two axes escape `MAX_RENDER_LONG_EDGE`:
///
/// - **Filters**: resvg allocates the filter buffer to the (transformed) filter
///   region, **not** clamped to the output canvas (resvg 0.47 `filter/mod.rs`),
///   so a crafted `filterUnits="userSpaceOnUse"` region + large `feGaussianBlur`
///   is a CPU/memory bomb. Covers don't need filters → reject any.
/// - **Vector tessellation**: cost scales with path-segment count independent of
///   output size → cap total segments, plus total nodes (element-explosion
///   backstop) and nesting depth (recursion/stack-overflow guard).
///
/// THREAT: input is attacker-controlled XML from an uploaded EPUB; this is the
/// render-cost analogue of the pre-decode raster guards in [`safe_image_kind`].
/// The walk descends `children`, the `clip-path`/`mask` sub-trees, **and** every
/// node's paint-server / layout sub-trees (via [`usvg::Node::subroots`]) — a
/// filter can hide inside a mask or a `<pattern>` fill, neither reachable via
/// `children` alone.
///
/// # Errors
///
/// Returns [`CoverError::Decode`] on the first breach (filter present, or a
/// segment/node/depth budget exceeded) so the caller's spine fallback fires.
fn check_render_complexity(tree: &usvg::Tree) -> Result<(), CoverError> {
    let mut budget = ComplexityBudget {
        segments: 0,
        nodes: 0,
    };
    check_group(tree.root(), 0, &mut budget)
}

/// Running totals for [`check_render_complexity`], threaded through the walk.
struct ComplexityBudget {
    segments: usize,
    nodes: usize,
}

impl ComplexityBudget {
    /// Count one node; error once the node budget is exceeded.
    fn bump_node(&mut self) -> Result<(), CoverError> {
        self.nodes = self.nodes.saturating_add(1);
        if self.nodes > MAX_SVG_NODES {
            return Err(CoverError::Decode(format!(
                "svg cover exceeds {MAX_SVG_NODES} node budget"
            )));
        }
        Ok(())
    }
}

/// Recursively gate one group: reject on filters, descend its clip-path/mask
/// sub-trees, then walk children.
fn check_group(
    group: &usvg::Group,
    depth: u32,
    budget: &mut ComplexityBudget,
) -> Result<(), CoverError> {
    if depth > MAX_SVG_DEPTH {
        return Err(CoverError::Decode("svg cover nesting too deep".to_owned()));
    }
    budget.bump_node()?;
    if !group.filters().is_empty() {
        return Err(CoverError::Decode(
            "svg cover uses filter primitives (unsupported)".to_owned(),
        ));
    }
    // Follow the *full* clip-path / mask chains, not just the first link. usvg
    // models `clip-path="url(#c2)"` on a `<clipPath>` (and the `<mask>` analogue)
    // as a recursive sibling, and a filter or geometry bomb can hide past level 1.
    // Each iteration costs a budget node, so a pathological chain is bounded.
    let mut clip = group.clip_path();
    while let Some(cp) = clip {
        check_group(cp.root(), depth + 1, budget)?;
        clip = cp.clip_path();
    }
    let mut mask = group.mask();
    while let Some(mk) = mask {
        check_group(mk.root(), depth + 1, budget)?;
        mask = mk.mask();
    }
    for node in group.children() {
        check_node(node, depth + 1, budget)?;
    }
    Ok(())
}

/// Gate one node by type, then descend its paint-server / layout sub-trees.
///
/// THREAT: a filter can hide inside a `<pattern>` fill/stroke (path) or a text
/// layout sub-tree — neither is a `child`. [`usvg::Node::subroots`] is the public
/// uniform accessor for those roots; without descending them a patterned filter
/// reaches `resvg::render` uncounted.
fn check_node(
    node: &usvg::Node,
    depth: u32,
    budget: &mut ComplexityBudget,
) -> Result<(), CoverError> {
    match node {
        // Groups carry their own filter/clip/mask handling and children.
        usvg::Node::Group(child) => return check_group(child, depth, budget),
        usvg::Node::Path(path) => {
            budget.bump_node()?;
            budget.segments = budget.segments.saturating_add(path.data().verbs().len());
            if budget.segments > MAX_SVG_PATH_SEGMENTS {
                return Err(CoverError::Decode(format!(
                    "svg cover exceeds {MAX_SVG_PATH_SEGMENTS} path-segment budget"
                )));
            }
        }
        usvg::Node::Image(_) | usvg::Node::Text(_) => budget.bump_node()?,
    }
    // `subroots` takes an `FnMut(&Group)` with no return channel, so capture the
    // first error and surface it after the walk.
    let mut subroot_err: Option<CoverError> = None;
    node.subroots(|subroot| {
        if subroot_err.is_some() {
            return;
        }
        if let Err(e) = check_group(subroot, depth + 1, budget) {
            subroot_err = Some(e);
        }
    });
    if let Some(e) = subroot_err {
        return Err(e);
    }
    Ok(())
}

/// Build a hardened [`usvg::ImageHrefResolver`]: the string resolver restricts
/// `<image href>` to path-checked sibling entries via `resolve_sibling`; the
/// data resolver bounds base64 data-URI payloads. Both reject rasters over the
/// byte/megapixel caps. Replaces usvg's default, which reads local files.
///
/// The string resolver additionally carries a cumulative budget: every call
/// counts against [`MAX_SIBLING_RESOLUTIONS`] and every fetched sibling
/// against [`MAX_TOTAL_SIBLING_BYTES`]; past either, it returns `None` before
/// consulting `resolve_sibling`, and usvg drops the image node. The counters
/// live in the resolver, which is built fresh per parse, so the budget is
/// per-rasterization.
fn hardened_resolver<'a, F>(resolve_sibling: F) -> usvg::ImageHrefResolver<'a>
where
    F: Fn(&str) -> Option<Vec<u8>> + Send + Sync + 'a,
{
    let resolutions = AtomicUsize::new(0);
    let fetched_bytes = AtomicUsize::new(0);
    // Shared across BOTH closures: aggregate decode cost is one axis no matter
    // which resolver admits the image.
    let decoded_pixels = Arc::new(AtomicU64::new(0));
    let data_pixels = Arc::clone(&decoded_pixels);
    usvg::ImageHrefResolver {
        // THREAT: default data resolver wraps base64 payloads without bounding
        // decode size. Apply the same byte+megapixel guard. Data-URI payloads
        // live inside the input-capped SVG bytes, so total encoded bytes are
        // bounded without a fetch budget here; aggregate decode cost is
        // bounded by the decoded-pixel budget shared with the string resolver.
        resolve_data: Box::new(move |_mime, data, _opts| safe_image_kind(data, &data_pixels)),
        // THREAT: default string resolver reads local files. Restrict to
        // path-checked siblings inside the same EPUB ZIP, within the
        // cumulative budget above.
        resolve_string: Box::new(move |href, _opts| {
            if resolutions.fetch_add(1, Ordering::Relaxed) >= MAX_SIBLING_RESOLUTIONS
                || fetched_bytes.load(Ordering::Relaxed) > MAX_TOTAL_SIBLING_BYTES
            {
                tracing::debug!(
                    resolution_cap = MAX_SIBLING_RESOLUTIONS,
                    byte_cap = MAX_TOTAL_SIBLING_BYTES,
                    "svg cover: sibling resolution refused (cumulative budget spent)"
                );
                return None;
            }
            if !crate::services::epub::is_safe_path(href) {
                return None;
            }
            let bytes = resolve_sibling(href)?;
            if fetched_bytes.fetch_add(bytes.len(), Ordering::Relaxed) + bytes.len()
                > MAX_TOTAL_SIBLING_BYTES
            {
                tracing::debug!(
                    bytes = bytes.len(),
                    byte_cap = MAX_TOTAL_SIBLING_BYTES,
                    "svg cover: sibling image rejected (over cumulative byte cap)"
                );
                return None;
            }
            safe_image_kind(Arc::new(bytes), &decoded_pixels)
        }),
    }
}

/// Wrap embedded raster bytes in an [`usvg::ImageKind`] only if they pass the
/// per-image byte and megapixel caps and the rasterization's shared
/// cumulative decoded-pixel budget, and decode to a supported format.
/// Dimension sniffing is header-only, and the cumulative budget is accounted
/// from the sniffed dimensions, so no full decode happens here either way.
/// GIF is intentionally excluded — the `image` crate is built without the
/// `gif` feature, so its dimensions cannot be sniffed and it is not a cover
/// format we accept.
fn safe_image_kind(data: Arc<Vec<u8>>, decoded_pixels: &AtomicU64) -> Option<usvg::ImageKind> {
    // Cap rejections log shape only (size/dims/format), never payload bytes, at
    // `debug` — input is attacker-controlled and a single SVG may embed many
    // images, so a louder level would let a crafted cover amplify log volume.
    if data.len() > MAX_EMBEDDED_IMAGE_BYTES {
        tracing::debug!(
            bytes = data.len(),
            cap = MAX_EMBEDDED_IMAGE_BYTES,
            "svg cover: embedded image rejected (over byte cap)"
        );
        return None;
    }
    let Ok(fmt) = image::guess_format(&data) else {
        tracing::debug!(
            bytes = data.len(),
            "svg cover: embedded image rejected (unrecognized format)"
        );
        return None;
    };
    let Ok((width, height)) =
        image::ImageReader::with_format(std::io::Cursor::new(data.as_slice()), fmt)
            .into_dimensions()
    else {
        tracing::debug!(
            ?fmt,
            "svg cover: embedded image rejected (undecodable header)"
        );
        return None;
    };
    let pixels = u64::from(width).saturating_mul(u64::from(height));
    if pixels > MAX_EMBEDDED_PIXELS {
        tracing::debug!(
            width,
            height,
            cap = MAX_EMBEDDED_PIXELS,
            "svg cover: embedded image rejected (over pixel cap)"
        );
        return None;
    }
    if decoded_pixels
        .fetch_add(pixels, Ordering::Relaxed)
        .saturating_add(pixels)
        > MAX_TOTAL_DECODED_PIXELS
    {
        tracing::debug!(
            width,
            height,
            cap = MAX_TOTAL_DECODED_PIXELS,
            "svg cover: embedded image refused (cumulative decoded-pixel budget spent)"
        );
        return None;
    }
    match fmt {
        image::ImageFormat::Jpeg => Some(usvg::ImageKind::JPEG(data)),
        image::ImageFormat::Png => Some(usvg::ImageKind::PNG(data)),
        image::ImageFormat::WebP => Some(usvg::ImageKind::WEBP(data)),
        other => {
            tracing::debug!(
                ?other,
                "svg cover: embedded image rejected (unsupported format)"
            );
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raster(width: u32, height: u32, fmt: image::ImageFormat) -> Vec<u8> {
        let img = image::DynamicImage::new_rgb8(width, height);
        let mut buf = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut buf), fmt)
            .expect("encode raster");
        buf
    }

    fn vector_svg(width: u32, height: u32) -> String {
        format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}"><rect width="{width}" height="{height}" fill="black"/></svg>"#
        )
    }

    /// SVG whose ONLY paintable content is a single `<image>` — so a blocked or
    /// rejected href yields a fully-transparent render. Airtight for the
    /// security guards (no background to mask a failed resolve).
    fn image_only_svg(href: &str) -> String {
        format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="150" viewBox="0 0 100 150"><image href="{href}" width="100" height="150"/></svg>"#
        )
    }

    fn long_edge(png: &[u8]) -> u32 {
        let img =
            image::load_from_memory_with_format(png, image::ImageFormat::Png).expect("decode png");
        img.width().max(img.height())
    }

    // PNG signature, an IHDR declaring `width`×`height`, an empty IDAT, and
    // IEND. The empty IDAT matters: dimension sniffing stops at the first
    // IDAT, so without one the sniff fails and the guards under test reject
    // for "undecodable header" instead of the dimensions. Carries no pixel
    // data, so a later full decode at render still fails.
    fn png_header_only(width: u32, height: u32) -> Vec<u8> {
        fn crc32(data: &[u8]) -> u32 {
            let mut crc = 0xFFFF_FFFFu32;
            for &byte in data {
                crc ^= u32::from(byte);
                for _ in 0..8 {
                    crc = if crc & 1 != 0 {
                        (crc >> 1) ^ 0xEDB8_8320
                    } else {
                        crc >> 1
                    };
                }
            }
            !crc
        }
        fn chunk(out: &mut Vec<u8>, tag: [u8; 4], data: &[u8]) {
            out.extend_from_slice(&u32::try_from(data.len()).unwrap().to_be_bytes());
            let mut body = tag.to_vec();
            body.extend_from_slice(data);
            out.extend_from_slice(&body);
            out.extend_from_slice(&crc32(&body).to_be_bytes());
        }
        let mut out = vec![137, 80, 78, 71, 13, 10, 26, 10];
        let mut ihdr = Vec::new();
        ihdr.extend_from_slice(&width.to_be_bytes());
        ihdr.extend_from_slice(&height.to_be_bytes());
        ihdr.extend_from_slice(&[8, 2, 0, 0, 0]); // 8-bit, RGB, no interlace
        chunk(&mut out, *b"IHDR", &ihdr);
        chunk(&mut out, *b"IDAT", &[]);
        chunk(&mut out, *b"IEND", &[]);
        out
    }

    #[test]
    fn rasterizes_minimal_svg_to_png() {
        let out = rasterize_svg(vector_svg(100, 150).as_bytes(), |_| None).unwrap();
        assert_eq!(image::guess_format(&out).unwrap(), image::ImageFormat::Png);
        assert!(long_edge(&out) <= MAX_RENDER_LONG_EDGE);
    }

    #[test]
    fn resolves_in_zip_relative_image_href() {
        let sibling = raster(2, 2, image::ImageFormat::Png);
        let out = rasterize_svg(image_only_svg("cover.png").as_bytes(), move |href| {
            (href == "cover.png").then(|| sibling.clone())
        })
        .unwrap();
        assert_eq!(image::guess_format(&out).unwrap(), image::ImageFormat::Png);
    }

    #[test]
    fn blocks_external_path_href() {
        // Resolver would happily return a real image for ANY href; the absolute
        // path must be blocked by is_safe_path BEFORE the resolver is consulted,
        // leaving a transparent render → Decode error.
        let err = rasterize_svg(image_only_svg("/etc/hostname").as_bytes(), |_| {
            Some(raster(2, 2, image::ImageFormat::Png))
        })
        .unwrap_err();
        assert!(matches!(err, CoverError::Decode(_)));
    }

    #[test]
    fn caps_sibling_resolution_count() {
        let images: String = std::iter::repeat_n(
            r#"<image href="cover.png" width="10" height="10"/>"#,
            MAX_SIBLING_RESOLUTIONS * 3,
        )
        .collect();
        let svg = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="150" viewBox="0 0 100 150">{images}</svg>"#
        );
        let sibling = raster(2, 2, image::ImageFormat::Png);
        let fetches = AtomicUsize::new(0);
        // The admitted resolutions still render, so the cap degrades rather
        // than rejects.
        rasterize_svg(svg.as_bytes(), |href| {
            fetches.fetch_add(1, Ordering::Relaxed);
            (href == "cover.png").then(|| sibling.clone())
        })
        .unwrap();
        assert_eq!(fetches.load(Ordering::Relaxed), MAX_SIBLING_RESOLUTIONS);
    }

    #[test]
    fn caps_cumulative_sibling_bytes() {
        // Valid PNG header padded to exactly the per-image cap: each fetch
        // passes the single-image guards, so only the cumulative byte cap can
        // stop the sequence. It admits floor(total/per-image) fetches, trips on
        // the next one, and refuses the rest before consulting the resolver.
        let mut padded = png_header_only(2, 2);
        padded.resize(MAX_EMBEDDED_IMAGE_BYTES, 0);
        let images: String = std::iter::repeat_n(
            r#"<image href="big.png" width="10" height="10"/>"#,
            MAX_SIBLING_RESOLUTIONS,
        )
        .collect();
        let svg = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="150" viewBox="0 0 100 150">{images}</svg>"#
        );
        let fetches = AtomicUsize::new(0);
        // Header-only padding never decodes at render, so every node drops and
        // the blank-render rejection fires.
        rasterize_svg(svg.as_bytes(), |_| {
            fetches.fetch_add(1, Ordering::Relaxed);
            Some(padded.clone())
        })
        .unwrap_err();
        let admitted = MAX_TOTAL_SIBLING_BYTES / MAX_EMBEDDED_IMAGE_BYTES;
        assert!(admitted + 1 < MAX_SIBLING_RESOLUTIONS);
        assert_eq!(fetches.load(Ordering::Relaxed), admitted + 1);
    }

    #[test]
    fn caps_cumulative_decoded_pixels() {
        // Header declares exactly the per-image pixel cap, so only the
        // cumulative budget can refuse: it admits floor(total/per-image)
        // images and refuses every one after.
        let img = Arc::new(png_header_only(4096, 4096));
        let budget = AtomicU64::new(0);
        let admitted = MAX_TOTAL_DECODED_PIXELS / MAX_EMBEDDED_PIXELS;
        for _ in 0..admitted {
            assert!(safe_image_kind(Arc::clone(&img), &budget).is_some());
        }
        assert!(safe_image_kind(Arc::clone(&img), &budget).is_none());
    }

    #[test]
    fn decoded_pixel_budget_is_shared_across_resolvers() {
        use base64ct::Encoding as _;
        // Four data-URI images at the per-image pixel cap spend the whole
        // decoded-pixel budget, so the sibling image that would render the
        // only visible content is refused and the blank-render rejection
        // fires. A per-resolver budget would admit the sibling and succeed.
        let big = base64ct::Base64::encode_string(&png_header_only(4096, 4096));
        let element = format!(r#"<image href="data:image/png;base64,{big}"/>"#);
        let mut svg = String::from(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="150" viewBox="0 0 100 150">"#,
        );
        for _ in 0..(MAX_TOTAL_DECODED_PIXELS / MAX_EMBEDDED_PIXELS) {
            svg.push_str(&element);
        }
        svg.push_str(r#"<image href="cover.png" width="100" height="150"/></svg>"#);
        let sibling = raster(2, 2, image::ImageFormat::Png);
        let err = rasterize_svg(svg.as_bytes(), move |href| {
            (href == "cover.png").then(|| sibling.clone())
        })
        .unwrap_err();
        assert!(matches!(err, CoverError::Decode(_)));
    }

    #[test]
    fn blocks_traversal_href() {
        let err = rasterize_svg(image_only_svg("../secret.png").as_bytes(), |_| {
            Some(raster(2, 2, image::ImageFormat::Png))
        })
        .unwrap_err();
        assert!(matches!(err, CoverError::Decode(_)));
    }

    /// SVG whose only visible content is a filled `<rect>` carrying a blur
    /// filter. Renders fine without the gate; must be rejected with it.
    fn filter_svg() -> String {
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="150" viewBox="0 0 100 150"><filter id="b"><feGaussianBlur stdDeviation="8"/></filter><rect width="100" height="150" fill="black" filter="url(#b)"/></svg>"#.to_owned()
    }

    /// A blur filter buried inside a `<mask>` — not reachable via `children()`,
    /// only via the mask sub-tree. Guards the mask descent in the walk.
    fn mask_filter_svg() -> String {
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="150" viewBox="0 0 100 150"><filter id="b"><feGaussianBlur stdDeviation="8"/></filter><mask id="m"><rect width="100" height="150" fill="white" filter="url(#b)"/></mask><rect width="100" height="150" fill="black" mask="url(#m)"/></svg>"#.to_owned()
    }

    /// Filled path with `segments` line commands inside the viewBox — visible
    /// content (renders without the gate), bounded well under the input cap.
    fn many_segments_svg(segments: usize) -> String {
        let mut d = String::with_capacity(segments * 9 + 16);
        d.push_str("M0 0");
        for i in 0..segments {
            use std::fmt::Write;
            write!(d, " L{} {}", i % 100, i % 150).expect("write path data");
        }
        d.push_str(" Z");
        format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="150" viewBox="0 0 100 150"><path d="{d}" fill="black"/></svg>"#
        )
    }

    #[test]
    fn rejects_svg_with_filter() {
        let err = rasterize_svg(filter_svg().as_bytes(), |_| None).unwrap_err();
        assert!(matches!(err, CoverError::Decode(_)));
        // Ingestion (Layer 5) must agree: a filtered cover is not acceptable.
        assert!(!parses_as_svg(filter_svg().as_bytes()));
    }

    #[test]
    fn rejects_filter_hidden_in_mask() {
        let err = rasterize_svg(mask_filter_svg().as_bytes(), |_| None).unwrap_err();
        assert!(matches!(err, CoverError::Decode(_)));
        assert!(!parses_as_svg(mask_filter_svg().as_bytes()));
    }

    #[test]
    fn rejects_filter_in_pattern() {
        // A filter hidden inside a <pattern> paint server reaches resvg::render
        // via the path's fill — not reachable through children/clip/mask, only
        // through the path's paint-server subroots. The walk must descend them.
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100"><defs><filter id="b"><feGaussianBlur stdDeviation="200"/></filter><pattern id="p" width="10" height="10" patternUnits="userSpaceOnUse"><rect width="10" height="10" fill="black" filter="url(#b)"/></pattern></defs><rect width="100" height="100" fill="url(#p)"/></svg>"#;
        let err = rasterize_svg(svg.as_bytes(), |_| None).unwrap_err();
        assert!(matches!(err, CoverError::Decode(_)));
        assert!(!parses_as_svg(svg.as_bytes()));
    }

    #[test]
    fn rejects_filter_in_chained_mask() {
        // Filter hidden in a SECOND-level mask: m1 references m2 via mask=…, and
        // m2's content carries the filter. The walk must follow the mask.mask
        // chain, not just the first level.
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100"><filter id="f" filterUnits="userSpaceOnUse" x="-5000" y="-5000" width="10000" height="10000"><feGaussianBlur stdDeviation="500"/></filter><mask id="m2"><rect width="100" height="100" fill="white" filter="url(#f)"/></mask><mask id="m1" mask="url(#m2)"><rect width="100" height="100" fill="white"/></mask><rect width="100" height="100" fill="black" mask="url(#m1)"/></svg>"#;
        let err = rasterize_svg(svg.as_bytes(), |_| None).unwrap_err();
        assert!(matches!(err, CoverError::Decode(_)));
        assert!(!parses_as_svg(svg.as_bytes()));
    }

    #[test]
    fn rejects_segments_in_chained_clip() {
        // 60k path segments hidden in a SECOND-level clipPath: c1 references c2
        // via clip-path=…. The walk must follow the clip_path.clip_path chain.
        // The leading filled rect makes the clip visible (so it renders without
        // the fix); the trailing dense subpath is the segment bomb.
        let mut d = String::from("M0 0 L100 0 L100 100 L0 100 Z M10 10");
        for i in 0..(MAX_SVG_PATH_SEGMENTS + 10_000) {
            use std::fmt::Write;
            write!(d, " L{} {}", i % 80 + 10, i % 80 + 10).expect("write path data");
        }
        let svg = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100"><clipPath id="c2"><path d="{d}"/></clipPath><clipPath id="c1" clip-path="url(#c2)"><rect width="100" height="100"/></clipPath><rect width="100" height="100" clip-path="url(#c1)" fill="black"/></svg>"#
        );
        assert!(
            svg.len() <= MAX_SVG_INPUT_BYTES,
            "fixture must fit input cap"
        );
        let err = rasterize_svg(svg.as_bytes(), |_| None).unwrap_err();
        assert!(matches!(err, CoverError::Decode(_)));
    }

    #[test]
    fn rejects_svg_with_doctype() {
        // DTDs are disabled (allow_dtd: false). A DOCTYPE is the prerequisite for
        // entity-expansion attacks that hide deep nesting from the byte-scan depth
        // guard (roxmltree expands &entity; into deep DOM the scanner never saw),
        // so any cover carrying one is rejected outright. Real covers have none.
        let svg = r#"<!DOCTYPE svg><svg xmlns="http://www.w3.org/2000/svg" width="100" height="150" viewBox="0 0 100 150"><rect width="100" height="150" fill="black"/></svg>"#;
        let err = rasterize_svg(svg.as_bytes(), |_| None).unwrap_err();
        assert!(matches!(err, CoverError::Decode(_)));
        assert!(!parses_as_svg(svg.as_bytes()));
    }

    #[test]
    fn rejects_excessive_path_segments() {
        let svg = many_segments_svg(MAX_SVG_PATH_SEGMENTS + 10_000);
        assert!(
            svg.len() <= MAX_SVG_INPUT_BYTES,
            "fixture must fit input cap"
        );
        let err = rasterize_svg(svg.as_bytes(), |_| None).unwrap_err();
        assert!(matches!(err, CoverError::Decode(_)));
        assert!(!parses_as_svg(svg.as_bytes()));
    }

    #[test]
    fn accepts_vector_cover_under_segment_budget() {
        // Well under budget, no filter — the gate must not false-reject it.
        let svg = many_segments_svg(1_000);
        let out = rasterize_svg(svg.as_bytes(), |_| None).unwrap();
        assert_eq!(image::guess_format(&out).unwrap(), image::ImageFormat::Png);
        assert!(parses_as_svg(svg.as_bytes()));
    }

    /// Real Standard Ebooks `cover.svg` (Pride and Prejudice). SE releases all
    /// its work to the public domain (CC0). This is the canonical input the SVG cover
    /// pipeline exists to render, so the gate must accept it — a false-reject here would
    /// regress every SE cover inside the PR that ships SVG support.
    const STANDARD_EBOOKS_COVER: &str = include_str!("testdata/standard_ebooks_cover.svg");

    #[test]
    fn accepts_real_standard_ebooks_cover() {
        // The cover references a sibling `cover.jpg` (full-bleed artwork); serve-
        // time resolves it and renders non-blank. Verified real SE covers use no
        // filters and a handful of path segments — far under every budget.
        let artwork = raster(2, 2, image::ImageFormat::Jpeg);
        let out = rasterize_svg(STANDARD_EBOOKS_COVER.as_bytes(), move |href| {
            (href == "cover.jpg").then(|| artwork.clone())
        })
        .unwrap();
        assert_eq!(image::guess_format(&out).unwrap(), image::ImageFormat::Png);
        // Ingestion (no sibling resolver) must accept it too.
        assert!(parses_as_svg(STANDARD_EBOOKS_COVER.as_bytes()));
    }

    /// Many zero-segment nodes: one data-URI `<image>` in `defs`, cloned by
    /// `count` `<use>` references, which usvg expands into per-reference
    /// nodes. Drives the node-count budget without path segments, without the
    /// sibling resolver (whose cumulative budget would drop the nodes first),
    /// and within the input byte cap (a `<use>` is far smaller than an inline
    /// data URI).
    fn many_images_svg(count: usize) -> String {
        use base64ct::Encoding as _;
        let uri = base64ct::Base64::encode_string(&png_header_only(1, 1));
        let mut s = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="150" viewBox="0 0 100 150"><defs><image id="i" href="data:image/png;base64,{uri}"/></defs>"#
        );
        for _ in 0..count {
            s.push_str(r##"<use href="#i"/>"##);
        }
        s.push_str("</svg>");
        s
    }

    #[test]
    fn rejects_excessive_node_count() {
        let svg = many_images_svg(MAX_SVG_NODES + 5);
        assert!(
            svg.len() <= MAX_SVG_INPUT_BYTES,
            "fixture must fit input cap"
        );
        let err = rasterize_svg(svg.as_bytes(), |_| None).unwrap_err();
        // Pin the message: a fixture usvg quietly prunes would still fail on
        // the blank render, and this test would stop guarding the budget.
        let CoverError::Decode(msg) = err else {
            panic!("unexpected error class: {err}");
        };
        assert!(msg.contains("node budget"), "unexpected rejection: {msg}");
    }

    /// `depth` nested `<g>` wrapping a single rect. Plain groups — the pre-parse
    /// depth check measures raw DOM nesting, before any usvg optimization.
    fn nested_groups_svg(depth: usize) -> String {
        let mut s = String::from(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="150" viewBox="0 0 100 150">"#,
        );
        for _ in 0..depth {
            s.push_str("<g>");
        }
        s.push_str(r#"<rect width="100" height="150" fill="black"/>"#);
        for _ in 0..depth {
            s.push_str("</g>");
        }
        s.push_str("</svg>");
        s
    }

    #[test]
    fn rejects_deeply_nested_svg_without_crashing() {
        // usvg's tree conversion has no recursion guard and aborts the process on
        // deep nesting (~266 levels). The pre-parse depth check (roxmltree DOM,
        // iterative) must reject this cleanly — well past usvg's overflow point —
        // before usvg ever converts it. The test passing (not SIGABRT) is the
        // assertion.
        let svg = nested_groups_svg(1_000);
        let err = rasterize_svg(svg.as_bytes(), |_| None).unwrap_err();
        assert!(matches!(err, CoverError::Decode(_)));
        assert!(!parses_as_svg(svg.as_bytes()));
    }

    #[test]
    fn accepts_moderately_nested_svg() {
        // Just under MAX_SVG_DEPTH — documents the boundary: the depth check must
        // not false-reject legitimately (if unusually) nested covers, and the
        // deepest the cap permits still parses + renders without overflowing.
        let svg = nested_groups_svg(45);
        let out = rasterize_svg(svg.as_bytes(), |_| None).unwrap();
        assert_eq!(image::guess_format(&out).unwrap(), image::ImageFormat::Png);
        assert!(parses_as_svg(svg.as_bytes()));
    }

    #[test]
    fn rejects_malformed_svg() {
        let err = rasterize_svg(b"<svg><broken", |_| None).unwrap_err();
        assert!(matches!(err, CoverError::Decode(_)));
    }

    #[test]
    fn rejects_zero_size_svg() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="0" height="0"></svg>"#;
        let err = rasterize_svg(svg.as_bytes(), |_| None).unwrap_err();
        assert!(matches!(err, CoverError::Decode(_)));
    }

    #[test]
    fn caps_huge_viewbox() {
        let out = rasterize_svg(vector_svg(999_999, 999_999).as_bytes(), |_| None).unwrap();
        assert!(long_edge(&out) <= MAX_RENDER_LONG_EDGE);
    }

    #[test]
    fn dtd_entities_are_inert() {
        // DTDs are disabled (allow_dtd: false), so this DOCTYPE-declared external
        // SYSTEM entity is rejected at parse — never expanded, never fetched, no
        // file content read, no hang. Result is `Decode`.
        let svg = r#"<?xml version="1.0"?><!DOCTYPE svg [<!ENTITY xxe SYSTEM "file:///etc/hostname">]><svg xmlns="http://www.w3.org/2000/svg" width="100" height="100"><text>&xxe;</text></svg>"#;
        let result = rasterize_svg(svg.as_bytes(), |_| None);
        assert!(result.is_err(), "external entity must not yield a cover");
    }

    #[test]
    fn rejects_oversized_svg_input() {
        let mut svg = b"<svg xmlns=\"http://www.w3.org/2000/svg\">".to_vec();
        svg.resize(MAX_SVG_INPUT_BYTES + 1, b' ');
        let err = rasterize_svg(&svg, |_| None).unwrap_err();
        assert!(matches!(err, CoverError::Decode(_)));
    }

    #[test]
    fn rejects_oversized_sibling_image() {
        // Resolver returns more than the byte cap → rejected before decode →
        // transparent render → Decode.
        let mut huge = raster(2, 2, image::ImageFormat::Png);
        huge.resize(MAX_EMBEDDED_IMAGE_BYTES + 1, 0);
        let err = rasterize_svg(image_only_svg("cover.png").as_bytes(), move |_| {
            Some(huge.clone())
        })
        .unwrap_err();
        assert!(matches!(err, CoverError::Decode(_)));
    }

    #[test]
    fn rejects_huge_pixel_sibling() {
        // Small bytes, but the PNG header declares 30000×30000 (900 MP) → the
        // megapixel guard rejects it → transparent render → Decode.
        let bomb = png_header_only(30_000, 30_000);
        let err = rasterize_svg(image_only_svg("cover.png").as_bytes(), move |_| {
            Some(bomb.clone())
        })
        .unwrap_err();
        assert!(matches!(err, CoverError::Decode(_)));
    }

    // Standard base64 (RFC 4648, with padding) — enough to embed a raster as a
    // `data:` URI so usvg's data-URI path hands the decoded bytes to the
    // overridden `resolve_data` resolver. Self-contained to avoid a test-only dep.
    fn base64_encode(data: &[u8]) -> String {
        const ALPHABET: &[u8; 64] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
        for chunk in data.chunks(3) {
            let n = (u32::from(chunk[0]) << 16)
                | (u32::from(chunk.get(1).copied().unwrap_or(0)) << 8)
                | u32::from(chunk.get(2).copied().unwrap_or(0));
            out.push(ALPHABET[((n >> 18) & 63) as usize] as char);
            out.push(ALPHABET[((n >> 12) & 63) as usize] as char);
            out.push(if chunk.len() > 1 {
                ALPHABET[((n >> 6) & 63) as usize] as char
            } else {
                '='
            });
            out.push(if chunk.len() > 2 {
                ALPHABET[(n & 63) as usize] as char
            } else {
                '='
            });
        }
        out
    }

    /// SVG whose only paintable content is a single `<image>` carrying a base64
    /// `data:` URI — so the raster reaches the `resolve_data` resolver, and a
    /// rejected payload yields a fully-transparent render (Decode).
    fn data_uri_image_svg(b64: &str) -> String {
        format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="150" viewBox="0 0 100 150"><image href="data:image/png;base64,{b64}" width="100" height="150"/></svg>"#
        )
    }

    #[test]
    fn accepts_data_uri_image() {
        // resolve_data path (base64 data-URI): a valid small PNG must resolve
        // through safe_image_kind and render non-blank — the SE data-URI cover
        // shape, distinct from the in-ZIP sibling path.
        let b64 = base64_encode(&raster(2, 2, image::ImageFormat::Png));
        let out = rasterize_svg(data_uri_image_svg(&b64).as_bytes(), |_| None).unwrap();
        assert_eq!(image::guess_format(&out).unwrap(), image::ImageFormat::Png);
    }

    #[test]
    fn rejects_huge_pixel_data_uri() {
        // The megapixel guard must fire on the resolve_data (base64 data-URI)
        // path, not only resolve_string siblings: a header-only PNG declaring
        // 30000×30000 (900 MP) is dropped → transparent render → Decode. (The
        // byte cap can't be exercised here — MAX_SVG_INPUT_BYTES bounds the whole
        // SVG, including the base64 payload, below MAX_EMBEDDED_IMAGE_BYTES — so
        // the megapixel guard is the reachable data-URI cap.)
        let b64 = base64_encode(&png_header_only(30_000, 30_000));
        let err = rasterize_svg(data_uri_image_svg(&b64).as_bytes(), |_| None).unwrap_err();
        assert!(matches!(err, CoverError::Decode(_)));
    }

    #[test]
    fn blank_svg_yields_decode_error() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100" viewBox="0 0 100 100"></svg>"#;
        let err = rasterize_svg(svg.as_bytes(), |_| None).unwrap_err();
        assert!(matches!(err, CoverError::Decode(_)));
    }

    #[test]
    fn looks_like_svg_accepts_variants() {
        assert!(looks_like_svg(b"<svg xmlns=\"...\">"));
        assert!(looks_like_svg(b"<?xml version=\"1.0\"?><svg/>"));
        assert!(looks_like_svg(b"   \n\t<svg/>"));
        assert!(looks_like_svg(b"\xEF\xBB\xBF<svg/>"));
        assert!(looks_like_svg(b"<SVG/>"));
    }

    #[test]
    fn looks_like_svg_rejects_rasters() {
        assert!(!looks_like_svg(&raster(2, 2, image::ImageFormat::Png)));
        assert!(!looks_like_svg(&raster(2, 2, image::ImageFormat::Jpeg)));
        assert!(!looks_like_svg(&[0x1f, 0x8b, 0x08, 0x00])); // svgz/gzip
        assert!(!looks_like_svg(b""));
    }

    #[test]
    fn parses_as_svg_accepts_valid_and_image_ref() {
        assert!(parses_as_svg(vector_svg(100, 150).as_bytes()));
        // SE-shape: image ref is unresolved at validation time but the SVG
        // still parses.
        assert!(parses_as_svg(image_only_svg("cover.jpg").as_bytes()));
    }

    #[test]
    fn parses_as_svg_rejects_malformed_and_oversized() {
        assert!(!parses_as_svg(b"<svg><broken"));
        assert!(!parses_as_svg(&raster(2, 2, image::ImageFormat::Png)));
        let mut svg = b"<svg>".to_vec();
        svg.resize(MAX_SVG_INPUT_BYTES + 1, b' ');
        assert!(!parses_as_svg(&svg));
    }
}
