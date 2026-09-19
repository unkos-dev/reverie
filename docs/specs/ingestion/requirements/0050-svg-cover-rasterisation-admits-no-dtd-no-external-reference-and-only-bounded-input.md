---
type: REQ
profile-version: 1
id: "REV-REQ-0050"
title: "SVG cover rasterization admits no DTD, no external reference and only bounded input"
governed-by:
  - "REV-ADR-0024"
---

# SVG cover rasterization admits no DTD, no external reference and only bounded input

## Statement

WHEN the cover service rasterizes an SVG cover, it MUST refuse a document that carries a DOCTYPE, MUST resolve an
embedded-image reference only to a path-checked sibling entry within the same archive or to a data URI carried in the
document itself and MUST refuse any other reference, MUST refuse a document using a filter primitive anywhere in the
render tree, and MUST refuse a document whose raw size, element nesting depth, embedded-raster byte size or decoded
pixel count, cumulative decoded pixels, resolved-reference count, or render-tree segment or node count exceeds a fixed
bound; a rasterization whose output carries no visible pixel MUST also be refused.

## Rationale

An EPUB's declared cover is attacker-supplied XML: anyone can hand a self-hoster an EPUB to add to their library, and
the cover service parses and renders that document with no separate sandbox. A document that fetches an external
resource, expands past what its input size implies, or costs unbounded render time can read files the server process can
see, abort the process outright, or exhaust its CPU and memory on a single request. Every reader sharing that server
depends on cover rendering staying confined to the file it was given and remaining available for everyone else's
requests.

## Acceptance criteria

- A cover SVG carrying a DOCTYPE is refused. Checked by `rejects_svg_with_doctype` and `dtd_entities_are_inert` in
  `backend/src/services/covers/svg.rs`; the latter confirms a DOCTYPE-declared external entity is never fetched.
- An `<image>` reference to an absolute path, or to a path that traverses outside the archive, is refused before any
  resolver is consulted. Checked by `blocks_external_path_href` and `blocks_traversal_href` in
  `backend/src/services/covers/svg.rs`.
- An `<image>` reference to a path-checked sibling entry inside the same archive, or to a data URI carried in the
  document, is admitted. Checked by `resolves_in_zip_relative_image_href` and `accepts_data_uri_image` in
  `backend/src/services/covers/svg.rs`.
- A document using a filter primitive is refused, wherever in the render tree the filter is reachable, including through
  a `<pattern>` paint server, or through a clip-path or mask chain more than one level deep. Checked by
  `rejects_svg_with_filter`, `rejects_filter_hidden_in_mask`, `rejects_filter_in_pattern` and
  `rejects_filter_in_chained_mask` in `backend/src/services/covers/svg.rs`.
- A rasterization whose rendered output carries no visible (non-zero-alpha) pixel is refused. Checked by
  `blank_svg_yields_decode_error` in `backend/src/services/covers/svg.rs`.
- Each bound below is enforced at exactly the stated limit:

  | Bound | Limit | Checked by (`backend/src/services/covers/svg.rs`) |
  | ----- | ----- | ------------------------------------------------- |
  | Raw SVG input size | 4,194,304 bytes (4 MiB) | `rejects_oversized_svg_input`; `parses_as_svg_rejects_malformed_and_oversized` |
  | Element nesting depth | 48 levels | `rejects_deeply_nested_svg_without_crashing`; `accepts_moderately_nested_svg` confirms 45 levels is accepted |
  | Byte size of one embedded raster | 8,388,608 bytes (8 MiB) | `rejects_oversized_sibling_image` |
  | Decoded pixel count of one embedded raster | 16,777,216 pixels (16 megapixels) | `rejects_huge_pixel_sibling`; `rejects_huge_pixel_data_uri` |
  | Cumulative decoded pixels admitted per rasterization | 67,108,864 pixels (64 megapixels) | `caps_cumulative_decoded_pixels`; `decoded_pixel_budget_is_shared_across_resolvers` |
  | Sibling-image resolutions per rasterization | 8 resolutions | `caps_sibling_resolution_count` (resolutions past the cap are refused; the document still renders with the admitted images rather than being rejected outright) |
  | Cumulative bytes fetched across sibling resolutions | 33,554,432 bytes (32 MiB) | `caps_cumulative_sibling_bytes` |
  | Rendered output long edge | 1,200 pixels | `caps_huge_viewbox` (a huge declared `viewBox` is scaled down to this edge rather than rendered at its declared size) |
  | Vector path segments across the render tree | 50,000 segments | `rejects_excessive_path_segments`; `rejects_segments_in_chained_clip`; `accepts_vector_cover_under_segment_budget` confirms 1,000 segments is accepted |
  | Nodes (groups, paths, images) across the render tree | 50,000 nodes | `rejects_excessive_node_count` |
