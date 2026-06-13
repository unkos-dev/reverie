---
title: Cover images
description: Which embedded cover formats Reverie renders, and how SVG covers are handled.
---

Reverie serves the cover image embedded in each EPUB (the file declared as the
cover in the book's manifest). Covers are decoded, resized to a thumbnail and a
full-size tier, and cached on disk; the browser only ever receives a raster
image.

## Supported embedded cover formats

| Declared format | How it is served                                                   |
| --------------- | ------------------------------------------------------------------ |
| JPEG, PNG, WebP | Decoded and resized directly.                                      |
| SVG             | Rendered to PNG on the server, then resized like any other raster. |

SVG support exists because [Standard Ebooks](https://standardebooks.org/) — the
canonical public-domain EPUB source — ship their cover as an SVG
(`images/cover.svg`).

## How SVG covers are handled

SVG covers are rasterized to PNG **on the server**, at request time, before
anything is sent to the browser. The original SVG bytes are never served. This
is deliberate: an SVG is an executable document (it can carry script), so
serving it directly would be a cross-site-scripting risk. Rendering it to a flat
PNG removes that risk entirely and keeps the cover route raster-only.

### Limitations

- **Text must be outlined.** Reverie renders SVG covers without a font engine,
  so any cover that relies on live `<text>` elements renders without that text.
  Standard Ebooks convert cover text to vector paths at build time, so their
  covers are unaffected. If you author your own SVG cover, convert text to paths.
- **Gzip-compressed `.svgz` is not supported.** Only uncompressed SVG is
  recognised as a cover.
- A cover that fails to render (malformed SVG, or one whose only content is an
  image that cannot be resolved) falls back to the generated typographic spine,
  exactly as a missing cover does.
