# Reverie brand assets — runtime delivery copies

The files in this directory are **deployment artifacts**, not the source of
truth. They are copies of a subset of the canonical brand asset set, served
by the application at `/brand/...` so the browser can fetch them at runtime
(favicons, Open Graph card, in-app lockups).

## Source of truth

The full brand identity — including reference variants, construction
diagrams, and the canonical `identity.md` document — lives at:

**[unkos-dev/reverie-branding](https://github.com/unkos-dev/reverie-branding)**

Treat that repository as authoritative. If a brand decision needs to change,
update it there first, then sync the affected ship files into this directory.

## What's here

| Path                                | Purpose                                        |
| ----------------------------------- | ---------------------------------------------- |
| `glyph/slot.svg`                    | Canonical mark — used in app chrome.           |
| `glyph/slot-favicon.svg`            | Thicker-slot variant for sub-24px raster.      |
| `lockup/lockup-on-dark.svg`         | Static lockup asset (e.g. share cards, README). The runtime lockup is rendered by `frontend/src/components/Lockup.tsx`. |
| `lockup/lockup-on-light.svg`        | Static lockup asset (light variant). See `Lockup.tsx` note above. |
| `raster/favicon-16.png`             | Browser-tab favicon, legacy fallback.          |
| `raster/favicon-32.png`             | Browser-tab favicon, high-DPI.                 |
| `raster/favicon-48.png`             | Windows shortcut icon.                         |
| `raster/apple-touch-icon-180.png`   | iOS home-screen icon (framed).                 |
| `raster/og-card-1200x630.png`       | Open Graph / Twitter share image.              |

Reference variants (mono, framed, high-res masters, construction diagrams)
are deliberately excluded from this directory — they're not served at
runtime, and bundling them would bloat both the production build and the
repo's git history. Pull them from the branding repo when needed.

## Editing

Don't edit these files in place. Changes to brand identity belong in
`unkos-dev/reverie-branding`. Once merged there, copy the updated ship files
into this directory in a separate Reverie PR. This keeps the brand history
in one repo and the deployment history coupled to product changes.
