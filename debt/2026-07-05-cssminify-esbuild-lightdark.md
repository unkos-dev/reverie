---
severity: low
surfaces: [developer, end-user]
adopted: 2026-07-05
adopted-because: react-data-grid's color tokens are all light-dark() functions; rolldown-vite's default lightningcss minifier miscompiles light-dark() (parcel-bundler/lightningcss#873), silently corrupting the grid's dark palette in production builds
lift-when-class: dep-unblocks
lift-when: a lightningcss release containing the light-dark() minification fix ships inside the pinned vite-plus, and a production build with the default minifier retains functional light-dark() in the emitted CSS
---

# cssMinify forced to esbuild while lightningcss corrupts light-dark()

`frontend/vite.config.ts` sets `build.cssMinify: "esbuild"`, overriding
rolldown-vite's default CSS minifier for the whole app. The default
(lightningcss) miscompiles the `light-dark()` color function, and every
react-data-grid color token uses it, so the grid's dark palette breaks
silently in production output under the default. react-data-grid's own
vite config works around the same bug, through a narrower mechanism
(excluding the light-dark feature from the lightningcss transform).

The override is app-wide, not grid-scoped: one minifier setting covers
all emitted CSS. esbuild's CSS minification is safe but less aggressive
than lightningcss, so the cost is a slightly larger stylesheet, not a
correctness risk.

Lift: drop the `cssMinify` line once the pinned vite-plus ships a
lightningcss with the upstream fix, and verify by grepping the built
`dist/assets/*.css` for surviving functional `light-dark(` under the
default minifier (the same check used when this was adopted).
