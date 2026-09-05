---
type: ADR
profile-version: 1
id: "REV-ADR-0026"
title: "Radix-generated three-tier dual-theme color tokens"
status: "accepted"
recorded-on: "2026-09-05"
decided-on: "2026-06-18"
decision-makers:
  - "John Unkovich"
---

# Radix-generated three-tier dual-theme color tokens

## Context and problem statement

Reverie's frontend renders in both a light and a dark theme, and every text and UI-control role must meet WCAG 2.2 AA
contrast in each. Achieving that with hand-picked per-theme hex values means tuning, and re-tuning, each value by
hand and re-checking its contrast against every surface it lands on; the two themes drift apart as values are
adjusted independently, and the set of one-off color tokens grows without bound. The system also needs raw color
confined to a single origin so the rest of the UI refers to named roles (surface, border, text, accent, danger)
rather than hexes, and it needs art-directed decorative color (the "atmosphere": gilt, cloth, vellum, ember) kept
strictly separate from functional UI color so chrome can never depend on a decorative tone.

How should the frontend's color be structured so that both themes stay AA-correct and in lockstep, raw color lives in
one governable place, and decorative color cannot leak into UI chrome?

## Decision drivers

- Light and dark must derive from one source and stay in lockstep, with no independently hand-maintained second
  palette.
- WCAG 2.2 AA contrast for text and non-text UI roles, guaranteed by construction rather than per-value review.
- A single origin for raw color; everything else references named semantic roles.
- Tailwind v4 with `@theme inline` and no `tailwind.config.ts`.
- No new runtime dependency; the color system must be CSP-clean (no external fetch).
- Exactly one UI accent (gold); decorative or categorical tones must not become a second accent.

## Considered options

- Radix Colors generated scales: generated 12-step perceptual scales, consumed through two semantic tiers, with a
  sealed atmosphere tier.
- Radix Themes: the Radix component library with its own theming.
- Hand-tuned per-theme hex ramps.

## Decision outcome

Chosen option: **Radix Colors generated scales**, consumed through a three-tier contract of generated primitives, a
semantic role layer, and a sealed atmosphere tier, because it satisfies every driver at once. The generator produces
matched light and dark 12-step scales from the same anchors, with per-step roles whose contrast holds by
construction, so AA is structural and the two themes cannot drift, and its output is plain CSS custom properties,
vendored as a static file, so there is no runtime dependency and nothing to fetch.

The three tiers:

1. **Primitives (Tier 1)**: `--sand-*` (neutral), `--gold-*` (accent), and `--danger-*` (state) 12-step scales, plus
   alpha, P3, contrast, and a `--bg` page token, generated for both themes and vendored verbatim as
   `frontend/src/styles/themes/primitives.generated.css`. Raw hex lives only here; regenerated, never hand-edited.
2. **Semantic (Tier 2)**: reverie role names (`--canvas`, `--surface`, `--border`, `--fg`, `--accent`, `--danger`, …)
   plus the shadcn aliases, each resolving to a Tier 1 step via `var()`. No raw color. The mapping is
   theme-constant; the primitive layer does the light/dark switch.
3. **Atmosphere (Tier 3)**: `--atm-*` art-directed constants (gilt, ember, sheen, vellum, cloth). A sealed parallel
   namespace: UI chrome resolves color through Tier 2 only and never reads `--atm-*`.

Components reference Tier 2; Tier 2 references Tier 1; raw color is Tier 1 (plus a small, named set of exceptions
such as the ink modal scrim). The shadcn alias layer re-skins automatically because it already routes through the
semantic tokens.

Stylelint's `color-no-hex` rule confines raw hex to the generated primitive file and the atmosphere file, and bans it
from the semantic file; a contract test asserts every semantic token resolves to an existing primitive and that
role-pair contrasts meet their AA floor.

### Consequences

- Positive: contrast for each role holds by construction in both themes: no per-value AA bookkeeping and no
  light/dark drift.
- Positive: raw color is confined to one generated file, lint-enforceable, and the rest of the UI is `var()`
  references to named roles.
- Positive: there is no runtime dependency: the generated scales are static CSS, CSP-clean, and shadcn components
  re-skin with no per-component work.
- Negative: changing or adding a color requires regenerating the scales rather than a quick one-off hex edit, and the
  generated primitive file is large and not meant to be hand-edited.
- Negative: the system gains a generation step that runs out-of-band, with its output vendored into the repository.

## Pros and cons of the options

### Radix Colors generated scales

- Positive: step number is a fixed UI role with contrast guaranteed by construction, and both themes come from one
  generation.
- Positive: the output is plain CSS custom properties (vendorable, no runtime dependency), framework-agnostic.
- Neutral: it introduces a regeneration step (output vendored).
- Negative: it is color only; focus-ring and other remedies are layered on top in the semantic tier.

### Radix Themes

- Positive: it bundles a complete themed component system.
- Negative: it imposes its own components and styling model on top of the existing shadcn/Tailwind UI, far more than
  a color foundation needs, and a large surface to adopt and override.
- Negative: it is a runtime dependency, against the CSP-clean / vendored-color driver.

### Hand-tuned per-theme hex ramps

- Positive: it is maximally direct: any value can be set by hand.
- Negative: each value's contrast must be checked and re-checked by hand per surface; the two themes drift as values
  change independently; the set of one-off tokens grows without bound.

## More information

Pairs with the single-danger-hue decision
([A single danger hue amends the no-hue-states policy](./0027-a-single-danger-hue-amends-the-no-hue-states-policy.md)),
which governs the `--danger` family this architecture carries. The brand color anchors and the generated palette are
documented in the brand identity reference (`reverie-branding/identity.md`). Reading-surface color tokens and
cover-spine textures are intentionally out of scope here and reserved to their own surfaces. Revisit if Radix's
generator or color package changes its scale semantics, or if a third theme is introduced.
