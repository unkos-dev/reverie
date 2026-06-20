---
name: Reverie
description: Your library, catalogued.
colors:
  # Brand-canonical anchors (theme-independent identity values)
  parchment: "#E8DCC2"
  ink: "#0E0D0A"
  cream: "#E8E0D0"
  reverie-gold: "#C9A961"
  danger: "#B91C1C"

  # Light theme (Parchment-anchored) — surface ramp
  canvas-light: "#E8DCC2"
  canvas-2-light: "#D3D1CF"
  surface-light: "#C8C6C2"
  surface-2-light: "#BFBCB6"
  surface-3-light: "#B6B2AB"
  border-light: "#ADA89F"
  border-strong-light: "#A29C90"
  border-control-light: "#251F14"
  fg-light: "#251F14"
  fg-muted-light: "#3D362A"
  fg-faint-light: "#120A00AA"
  accent-light: "#A77C00"
  accent-soft-light: "#D6C8A8"
  accent-strong-light: "#967100"
  accent-text-light: "#5A3E00"
  fg-on-accent-light: "#0E0D0A"
  danger-light: "#B91C1C"
  danger-text-light: "#900000"
  danger-soft-light: "#D4C2BF"
  fg-on-danger-light: "#FFFFFF"

  # Dark theme (Ink-anchored) — surface ramp
  canvas-dark: "#0E0D0A"
  canvas-2-dark: "#191815"
  surface-dark: "#24221E"
  surface-2-dark: "#2B2924"
  surface-3-dark: "#34302A"
  border-dark: "#3E3A32"
  border-strong-dark: "#4C473D"
  border-control-dark: "#F0EEE9"
  fg-dark: "#F0EEE9"
  fg-muted-dark: "#BAB2A3"
  fg-faint-dark: "#FFEFCF7B"
  accent-dark: "#C9A961"
  accent-soft-dark: "#262217"
  accent-strong-dark: "#BE9E56"
  accent-text-dark: "#D9B970"
  fg-on-accent-dark: "#0E0D0A"
  danger-dark: "#B91C1C"
  danger-text-dark: "#FF9082"
  danger-soft-dark: "#410A08"
  fg-on-danger-dark: "#FFFFFF"

  # Overlay scrim (ink, theme-fixed) — modal / dialog / sheet / popover backdrop
  overlay: "#0E0D0AE0"
typography:
  display:
    fontFamily: '"Author Variable", system-ui, -apple-system, "Segoe UI", sans-serif'
    fontSize: "clamp(2rem, 4vw, 3.25rem)"
    fontWeight: 500
    lineHeight: 1.05
    letterSpacing: "-0.012em"
  headline:
    fontFamily: '"Author Variable", system-ui, -apple-system, "Segoe UI", sans-serif'
    fontSize: "1.875rem"
    fontWeight: 500
    lineHeight: 1.15
    letterSpacing: "-0.012em"
  title:
    fontFamily: '"Satoshi Variable", system-ui, -apple-system, "Segoe UI", sans-serif'
    fontSize: "1.125rem"
    fontWeight: 500
    lineHeight: 1.35
    letterSpacing: "0"
  body:
    fontFamily: '"Satoshi Variable", system-ui, -apple-system, "Segoe UI", sans-serif'
    fontSize: "0.9375rem"
    fontWeight: 400
    lineHeight: 1.55
    letterSpacing: "0"
  label:
    fontFamily: '"JetBrains Mono", ui-monospace, SFMono-Regular, Menlo, monospace'
    fontSize: "0.75rem"
    fontWeight: 400
    lineHeight: 1.4
    letterSpacing: "0.04em"
  wordmark:
    fontFamily: '"Satoshi Variable", system-ui, -apple-system, "Segoe UI", sans-serif'
    fontSize: "1.75rem"
    fontWeight: 700
    lineHeight: 1
    letterSpacing: "0.32em"
rounded:
  sm: "0.25rem"
  md: "0.5rem"
  lg: "0.75rem"
  xl: "0.875rem"
components:
  # Buttons — shadcn variant set rebound to the canonical brand palette
  button-primary:
    backgroundColor: "{colors.accent-dark}"
    textColor: "{colors.fg-on-accent-dark}"
    rounded: "{rounded.lg}"
    padding: "0 0.625rem"
    height: "2rem"
  button-outline:
    backgroundColor: "{colors.canvas-dark}"
    textColor: "{colors.fg-dark}"
    rounded: "{rounded.lg}"
    padding: "0 0.625rem"
    height: "2rem"
  button-ghost:
    backgroundColor: "transparent"
    textColor: "{colors.fg-dark}"
    rounded: "{rounded.lg}"
    padding: "0 0.625rem"
    height: "2rem"
  button-destructive:
    backgroundColor: "#B91C1C33" # --color-destructive (danger) @ 20% — the dark soft fill
    textColor: "{colors.danger-dark}"
    rounded: "{rounded.lg}"
    padding: "0 0.625rem"
    height: "2rem"

  # Surfaces
  card:
    backgroundColor: "{colors.surface-dark}"
    textColor: "{colors.fg-dark}"
    rounded: "{rounded.xl}"
    padding: "1rem"
  input:
    backgroundColor: "{colors.canvas-dark}"
    textColor: "{colors.fg-dark}"
    rounded: "{rounded.lg}"
    padding: "0.25rem 0.625rem"
    height: "2rem"

  # Signature components — the Lockup is brand-locked (not theme-flexible)
  lockup-dark:
    backgroundColor: "{colors.canvas-dark}"
    textColor: "{colors.cream}"
    height: "1.75rem"
  lockup-light:
    backgroundColor: "{colors.canvas-light}"
    textColor: "{colors.ink}"
    height: "1.75rem"
---

<!-- markdownlint-disable MD024 MD025 MD026 MD036 -->
<!-- Stitch DESIGN.md format conflicts: MD024 repeated '### Named Rules',
     MD025 implicit h1 from frontmatter `name:` field,
     MD026 '### Do:' and '### Don't:' colons,
     MD036 '**Creative North Star: "..."**' bolded line. -->

# Design System: Reverie

## 1. Overview

**Creative North Star: "The Curator's Archive"**

Reverie's surfaces feel like a museum after hours — the collection is the point, the lighting is considered, the chrome stays out of the reader's way. Cinematic-boutique is the locked visual register: the Library is the identity surface and carries the atmosphere (a slow breathing ember field, film grain, a photographic hero band, a gilt-foil masthead H1, hover-lift on covers, and a press-F cinematic mode that dissolves the chrome). Typography does the structural work and information hierarchy is disciplined. The reader recedes into a calm long-form surface that inherits palette and motion language but withdraws nearly all chrome; utility and admin screens stay plainest of all.

Colour is governed through a three-tier token tree: generated Radix primitives (`--sand` / `--gold` / `--danger` ramps), then semantic roles (`--canvas` / `--fg` / `--accent` / …) with a shadcn-compatibility alias layer, then a sealed art-directed atmosphere tier (`--atm-*` / `--cover-*`) that chrome must never consume. The brand has exactly one accent (Reverie Gold) and exactly one state hue (the danger red) — every other state, hierarchy, and emphasis signal is carried by weight, opacity, type scale, density, and motion. There is no severity ladder; success, warning, and info stay hue-less. The category-reflex check has two altitudes the system answers no to: it does not look like a self-hosted media manager (Calibre, Plex), and it does not look like an AI workflow tool (SaaS-cream chrome, Linear-clone gradients).

**Key Characteristics:**

- Editorial pairing: Author (display) + Satoshi (workhorse) + JetBrains Mono (metadata). Sans-only; no content serif.
- Two anchored canvases: Parchment (`#E8DCC2`) for light, Ink (`#0E0D0A`) for dark. Both surface ramps tint warmly toward the brand hue. No neutral greys.
- Single gold accent (≤10% of any screen), single danger hue (bounded exception, destructive/error only, fill/border/icon — never body text).
- Flat by default; the one signature shadow is the accent-glow cover-lift on the Library grid. Atmospheric depth on the Library page comes from the sealed `--atm-*` tier.
- Motion budget: 180–320 ms ease-out (`--ease-standard` / `--ease-emphasised`), no bounce, no spring; route crossfades via the View Transitions API; `prefers-reduced-motion` respected as a first-class state.

## 2. Colors

A warm, considered palette of parchment, ink, cream, and gold, generated as Radix ramps from four anchors (Ink `#0E0D0A`, Parchment `#E8DCC2`, Reverie Gold `#C9A961`, Danger `#B91C1C`). No greys. The dark theme is Ink-anchored; the light theme is Parchment-anchored. Both tint surfaces warmly toward the brand hue rather than collapsing toward neutrals. sRGB hex is the floor; a P3 wide-gamut layer (OKLCH) progressively enhances where the display supports it.

### Primary

- **Reverie Gold** (accent: `#C9A961` dark / `#A77C00` light): the accent (`--accent`, the `gold-9` step). Reserved for primary CTAs, recovery actions out of error states, the active-nav slot bar (a 16×2px gold mark echoing the glyph's slot), and the Slot mark fill. The single source of glow. On light surfaces the accent darkens to `#A77C00` because gold at `#C9A961` does not meet WCAG AA against Parchment.
- **Gold accent-text** (`#D9B970` dark / `#5A3E00` light): a darker companion step (`--accent-text`, `gold-11`) used where gold must read as text or as a hairline against the page — most visibly the focus ring (`--focus-ring`). On light it clears the 1.4.11 non-text boundary on its own, which the accent fill does not.

### Secondary

- **Danger** (`#B91C1C` solid, both themes): the one sanctioned state hue (`--danger`, `danger-9`), a generated AA-correct warm red. A bounded exception to the no-hue-states rule, reserved strictly for irreversible-destructive confirmation and unrecoverable system errors. Appears as fill, border, or icon — **never body text** — and per WCAG 1.4.1 always pairs with an icon, weight, or text label. `--fg-on-danger` is white (`#FFFFFF`), the one sanctioned use of pure white, clearing 6.47:1 on the solid. Text-weight companions: `--danger-text` (`#FF9082` dark / `#900000` light); soft fill `--danger-soft` (`#410A08` dark / `#D4C2BF` light).

### Neutral — surface ramp (Light theme · Parchment-anchored)

- **canvas-light** (`#E8DCC2`): Parchment. The default page background. Warm, slightly aged.
- **canvas-2-light** (`#D3D1CF`): the layer beneath canvas; nav backdrops, sidebars, less-active surfaces.
- **surface-light** (`#C8C6C2`): the layer above canvas; cards, dialogs, dropdowns.
- **surface-2-light** (`#BFBCB6`) / **surface-3-light** (`#B6B2AB`): hover-elevation and popover layers above surface.
- **border-light** (`#ADA89F`) / **border-strong-light** (`#A29C90`): hairline and structural borders. **border-control-light** (`#251F14`): the sole boundary of interactive controls.
- **fg-light** (`#251F14`) / **fg-muted-light** (`#3D362A`) / **fg-faint-light** (`#120A00AA`): text hierarchy. fg-faint is sub-AA — placeholder and non-essential text only.

### Neutral — surface ramp (Dark theme · Ink-anchored)

- **canvas-dark** (`#0E0D0A`): Ink page background.
- **canvas-2-dark** (`#191815`): beneath-canvas layer.
- **surface-dark** (`#24221E`) / **surface-2-dark** (`#2B2924`) / **surface-3-dark** (`#34302A`): card, hover-elevation, and popover layers.
- **border-dark** (`#3E3A32`) / **border-strong-dark** (`#4C473D`): borders. **border-control-dark** (`#F0EEE9`): control boundary.
- **fg-dark** (`#F0EEE9`) / **fg-muted-dark** (`#BAB2A3`) / **fg-faint-dark** (`#FFEFCF7B`): text hierarchy.

### Atmosphere (sealed Tier-3 — art-directed, chrome must not consume)

The atmosphere tier is theme-fixed editorial constants behind a review gate. No `bg-atm-*` utilities are generated; these are read via raw `var(--atm-*)` only, on the Library page and the cover artwork.

- **Cover palette** (`--cover-ink #0E0D0A`, `--cover-cream #E8E0D0`, `--cover-parchment #E8DCC2`, `--cover-gold #C9A961`): theme-fixed, because a publisher's spine does not shift with the reader's theme.
- **Gilt foil** (`--atm-gilt-0…4`: `#F8E8B8`, `#E8CC85`, `#C9A961`, `#9A7E40`, `#5E4520`): the five warm-metal stops of the Library masthead H1.
- **Cover cloth** (`--atm-cloth-*`, six warm-dark tones — bordeaux, oxblood, midnight, charcoal, sepia, terracotta — each with a `-c` body and `-e` embossed edge): book spines as physical objects, theme-fixed.
- **Ember / vellum / sheen** (`--atm-ember`, `--atm-vellum`, `--atm-sheen-*`): the breathing ambient field and editorial sheen, retuned per theme (softer terracotta on parchment, warm ember on ink).

### Named Rules

**The One Accent Rule.** Reverie Gold is the only accent. State, hierarchy, and emphasis below the level of "this matters" are communicated through weight, opacity, type scale, density, and motion — never through additional hues. There is no severity ladder.

**The Danger Carve-Out Rule.** The danger hue is the single exception to no-hue-states. It appears in exactly two contexts: (1) irreversible destructive confirmation (alongside typed-name confirmation and an undo window where feasible), and (2) unrecoverable system errors that would otherwise survive as a wrong "it's working" mental model. Nowhere else, never decorative, always paired with a non-colour signal. Success, warning, and info remain hue-less.

**The Light-Gold Restriction Rule.** On Parchment, the gold accent fill (`#A77C00`) passes WCAG 1.4.11 and 1.4.3 large-text, but not 1.4.3 normal text. The accent fill is therefore restricted on light surfaces to large CTAs and recovery actions. Gold that must read as text or a hairline (e.g. the focus ring) uses the darker `accent-text` step (`#5A3E00`) instead. axe-core contrast violations on small-text gold are the right signal — the surface is misusing the accent.

**The No-Black-No-White Rule.** `#000` is prohibited and `#fff` is reserved for exactly one role: `--fg-on-danger`, where the danger solid needs maximum contrast. Every other neutral tints warmly toward the brand hue. The doctrine is OKLCH thinking with sRGB-hex floor and a P3 enhancement layer; primitives are generated, not hand-tuned.

## 3. Typography

**Display Font:** Author Variable (with system-ui, -apple-system, Segoe UI, sans-serif fallback).
**Body Font:** Satoshi Variable (with system-ui, -apple-system, Segoe UI, sans-serif fallback).
**Mono Font:** JetBrains Mono Regular (with ui-monospace, SFMono-Regular, Menlo fallback). Used for metadata surfaces (ISBN, IDs, format codes).

All three are self-hosted variable woff2 files at `public/fonts/fontshare/files/` with `font-display: swap`. The Fontshare CDN is broken in Chromium under the production CSP (Opaque Response Blocking trips on cookie-bearing CSS responses); self-hosting bypasses that and matches `font-src 'self'`.

**Character:** Author and Satoshi are a deliberate Fontshare pairing — Author does the editorial work (display, book titles in detail, italic accent moments), Satoshi does the structural work (body, navigation, controls, wordmark). The wordmark uses Satoshi rather than Author because the wide-tracked 0.32em uppercase stamp is itself doing identity work; a neutral grotesque lets that treatment carry without competing with display character. JetBrains Mono carries metadata where mono affordance is needed.

### Hierarchy

- **Display** (Author 500, `clamp(2rem, 4vw, 3.25rem)`, line-height 1.05, tracking -0.012em): hero headlines, book detail titles, the Library masthead H1 (gilt-foil treated).
- **Headline** (Author 500, 1.875rem / 30px, line-height 1.15, tracking -0.012em): section headers in the library and detail views.
- **Title** (Satoshi 500, 1.125rem / 18px, line-height 1.35): card titles, primary affordance labels, nav items.
- **Body** (Satoshi 400, 0.9375rem / 15px, line-height 1.55): paragraph text, descriptions, control text. Cap line length at 65–75ch for prose.
- **Label** (JetBrains Mono 400, 0.75rem / 12px, tracking 0.04em, uppercase small): metadata fields (ISBN, file size, format codes), debug surfaces. Use sparingly; mono is signal, not chrome.
- **Wordmark** (Satoshi 700, 1.75rem reference / 28px, tracking 0.32em, uppercase, padding-left 0.32em for optical balance): the canonical Lockup component only. Not a heading style.

### Named Rules

**The Sans-Only Rule.** Reverie deliberately rejected the chrome-sans / content-serif convention. Author and Satoshi both sit in the sans family — Author's wide-letterform editorial energy carries the display register that a content serif would normally fill. There is no content serif in the system.

**The 65-75ch Body Rule.** Body text caps at 65–75ch for prose surfaces (book descriptions, blog-shape content). Library cards and detail panels are not prose and may exceed this; reader text is user-configurable and overrides the system default.

**The Italic-as-Editorial-Accent Rule.** Italics use Author italic (the editorial counterweight), not Satoshi italic. Reserved for book titles, ship-names, internal-monologue, and the tagline (which uses Author 400 regular, not italic). Not for general emphasis.

**The No-Black-Weight Rule.** Author and Satoshi black weights (900) are not loaded — the variable axes run 400–700. Both fight the boutique register; their absence is deliberate. If a display moment needs more weight, increase size or use tracking; do not reach for 900.

## 4. Elevation

Flat by default, tonal layering for everyday depth, with one signature shadow and an art-directed atmosphere reserved for the Library identity surface.

Everyday depth comes from the surface ramp: canvas → surface → surface-2 → surface-3 + border / border-strong does the work for cards, dialogs, dropdowns, and primitives. Shadcn primitives use `ring-1 ring-foreground/10` on cards rather than box-shadow; that ring is the rest state. Hover and focus lift use opacity and the `--surface-2` hover token, not shadow.

The cinematic register earns exactly one chrome shadow: the **accent-glow cover-lift** on the Library grid, where a cover gains `box-shadow: 0 14px 32px -16px var(--accent-glow)` on hover or focus-within (`--accent-glow` is the accent at 45% via `color-mix`). Outside that, chrome shadows are absent. The Library page additionally carries the sealed atmosphere — a fixed breathing ember radial (`.lib-atm`, 12s `breathe`), film grain, and a photographic hero band that fades into the canvas — but these live in the art-directed Tier-3 layer, scoped to the Library route only, never on utility, admin, or reader chrome.

### Shadow Vocabulary

- **cover-lift** (`box-shadow: 0 14px 32px -16px var(--accent-glow)`): the gold-tinted glow a Library cover gains on hover / focus-within, over `--duration-base` (240 ms). The single chrome shadow in the system.
- **cover pedestal**: on Light, dark cloth covers carry a hairline + subtle pedestal shadow so books read as objects sitting on Parchment rather than dissolving into it.
- **focus ring** (`outline: 2px solid var(--focus-ring)`, offset 2px): not a shadow, but the only chrome that "elevates" the focused element. `--focus-ring` is the gold `accent-text` step; a single ring, no halo.

### Named Rules

**The Flat-By-Default Rule.** Surfaces are flat at rest. Depth comes from tonal layering (canvas → surface → surface-2 → surface-3). The one chrome shadow is the Library cover-lift on hover / focus; otherwise depth is a response to state (the focus ring). If a card's resting state needs a drop shadow to look complete, the design is wrong.

**The Atmosphere-Is-Library-Only Rule.** The ambient ember field, film grain, photographic hero, and gilt-foil masthead are mounted inside the Library page, never in the app shell. The identity surface carries the atmosphere; the reader recedes and utility / admin screens stay plain. Ambient drift is never app-wide and never bleeds into chrome.

**The Single-Focus-Indicator Rule.** A global `:focus-visible` rule paints one 2px `accent-text` outline at a 2px offset and forces `box-shadow: none`. The `box-shadow: none` is load-bearing: it suppresses the per-primitive `focus-visible:ring-*` box-shadow rings so the app never paints two concentric indicators. Verify focus by render in both themes, not on the computed claim alone.

## 5. Components

The component philosophy is **considered, inscribed**: every primitive feels like it was placed deliberately and cannot be moved by accident. Each affordance is a carved gesture, not a frictionless tap. Weight + restraint + intent.

Reverie ships shadcn/ui primitives (button, card, input, dialog, alert-dialog, dropdown, sheet, etc.) rebound to the canonical brand palette via a shadcn-compatibility alias layer in `styles/themes/index.css`. New code prefers the canonical `--color-canvas` / `--color-fg` / `--color-accent` token names; the alias layer means stock primitives drop in without per-file rewrites.

### Buttons

- **Shape:** rounded-lg (0.75 rem / 12 px) by default. Smaller variants reach for `min(var(--radius-md), 12px)` (sm / icon-sm) and `min(var(--radius-md), 10px)` (xs / icon-xs).
- **Primary (default):** bg-primary (the accent, `#C9A961` dark / `#A77C00` light), text-fg-on-accent, no border. The single "this matters" gesture on the surface — at most one Primary per surface. (Link-form primaries dim to `bg-primary/80` on hover; the press affordance is `active:translate-y-px`.)
- **Outline:** transparent / canvas bg, border-border, text-fg, hover-bg muted. The everyday button; appears multiple times per surface. (Dark: `border-input`, `bg-input/30`.)
- **Secondary:** bg-secondary (surface-2), text-fg, hover `bg-secondary/80`.
- **Ghost:** transparent rest, hover-bg muted, text-fg. Tertiary affordance, denser surfaces (toolbars, dropdown headers).
- **Destructive:** `bg-destructive/10 text-destructive` — a soft danger-tinted fill with danger-hue text, hover to `bg-destructive/20`. The dedicated alert-dialog (typed-name confirmation, **solid** danger fill on the confirm action) is the real destructive surface; this variant is the inline soft form.
- **States:** focus shows the global 2px `accent-text` outline; `active:translate-y-px` gives the carved-in-press affordance (suppressed on menu triggers). `aria-invalid` shifts border + ring to the danger hue.
- **Sizes:** default h-8 (32 px), xs h-6, sm h-7, lg h-9. Icon variants are square at the matching height (size-6 / 7 / 8 / 9).

### Cards

- **Corner Style:** rounded-xl (0.875 rem / 14 px).
- **Background:** bg-card (alias to surface). Slight warm tint above canvas.
- **Border / Shadow Strategy:** `ring-1 ring-foreground/10` at rest. No drop shadow. Flat-By-Default applies.
- **Internal Padding:** py-4 default (16 px vertical; full-bleed images extend edge-to-edge and round the matching corners); px-4 on header / content; footer is `border-t bg-muted/50`. Size `sm` collapses to gap-3 / py-3 / px-3.
- **Composition:** Header (optional title + description + action grid), Content, Footer. Cards do not nest. Cards do not stack inside cards.

### Inputs

- **Style:** h-8 (32 px), rounded-lg (0.75 rem), border-input, transparent bg in light theme (`bg-input/30` in dark).
- **Focus:** the global 2px `accent-text` outline (no halo). The focus ring is the inscription gesture — small but unmistakable.
- **Disabled:** `cursor-not-allowed`, `opacity-50`.
- **Invalid:** `aria-invalid` shifts border and ring to the danger hue. Visible invalid state still pairs with helper text and weight so colour is never the sole signal (WCAG 1.4.1).

### Dialogs

- **Backdrop:** every overlay (dialog, alert-dialog, sheet, popover) uses the `bg-overlay` scrim token — a near-opaque warm-ink veil (`rgb(14 13 10 / 88%)`), theme-fixed on both themes, with an optional `backdrop-blur-xs` where supported. The earlier `bg-black/50` backdrops have been removed.
- **Alert-Dialog (destructive confirmation):** the canonical Danger Carve-Out surface. Typed-name confirmation, **solid** danger fill on the confirm button, plain Outline button on the cancel. Cancel is the safer default and gets focus.
- **Dialog (general):** bg-surface, rounded-xl, no shadow. Use sparingly; the modal is not a first thought.

### Navigation

- **Style:** the global nav is a sticky left rail (`canvas-2`, `border-r`), not a top bar. Items are Satoshi 500 at text-sm (0.875 rem); default state is fg-muted on transparent; hover and active both lift to a `surface` pill with fg text. The active row additionally carries a 16×2px gold slot bar (`bg-accent`) at its leading edge — the glyph's slot reused as the active marker, the one place the accent appears in the rail. Disabled / planned entries render as fg-faint placeholders, `aria-disabled`, out of tab order, with a "planned" tooltip.
- **Mobile:** sheet-based, not bottom-nav. The library is the identity surface and earns horizontal scroll affordances; nav stays vertical and quiet.

### Signature: Lockup

The canonical brand expression — glyph (the Slot) + wordmark, inline, at `src/components/Lockup.tsx`. The Lockup is **brand-locked**, not theme-flexible: glyph fill is always `#C9A961` Reverie Gold, never recoloured; the slot sits at 53 % from top (not centred — centred reads mechanical / mail-slot, below-centre reads inscribed); the wordmark uses inline-literal `#0E0D0A` / `#E8E0D0` rather than design tokens so it renders correctly even before the theme tree resolves.

Three forms ranked by canonicality: (1) lockup with glyph — canonical; (2) glyph alone — favicons, very compressed contexts; (3) wordmark alone — only when neither fits (footer microtext, plain-text email signatures). Never use the wordmark alone as a primary mark.

### Signature: CoverArtwork

When a book has no cover art (or it fails to load), `src/components/CoverArtwork.tsx` renders a typographic spine — compositions × cloth colourways assigned deterministically from the book id, drawn from the `--cover-*` and `--atm-cloth-*` tokens. Covers are brand-fixed and theme-independent: a publisher's spine does not switch when the reader toggles Light↔Dark. On Light the dark cloth carries a pedestal (hairline + shadow) so books read as objects on Parchment. The visible gilt title text is capped to three lines; the SVG `<title>` keeps the whole string for assistive tech.

### Named Rules

**The One-Primary-Per-Surface Rule.** Each surface has at most one Primary button. If two actions seem to need primary weight, one of them is actually destructive (route to alert-dialog) or the surface is doing two jobs (split it).

**The No-Bottom-Nav Rule.** Mobile nav is a sheet, not a bottom bar. Bottom-nav reads consumer-app; Reverie's mobile presence is a complement to the desktop archive, not a standalone mobile app.

**The Lockup-As-Brand-Carrier Rule.** The Lockup component is invariant. It uses inline hex values, not theme tokens, and must render correctly before the theme tree resolves. Do not theme it; do not re-style its parts; do not use its glyph or wordmark separately as decorative elements.

## 6. Do's and Don'ts

### Do:

- **Do** anchor every screen on Parchment (`#E8DCC2`) or the Ink dark canvas (`#0E0D0A`); the two canvases are the only legitimate page backgrounds.
- **Do** use Reverie Gold for primary CTAs, recovery actions, the active-nav slot bar, and the Slot mark fill. Use the darker `accent-text` step where gold must read as text or a hairline (e.g. the focus ring); nothing else.
- **Do** restrict the danger hue (`#B91C1C`) to fill, border, or icon — never body text — in exactly the two carve-out contexts (irreversible destructive confirm, unrecoverable system error), always paired with a non-colour signal.
- **Do** carry hierarchy through weight, opacity, type scale, density, and motion. Author or Satoshi at different weights is the system's primary expressiveness.
- **Do** cap body text at 65–75ch on prose surfaces; cards and detail panels are not prose and may run wider.
- **Do** keep cards flat at rest (`ring-1 ring-foreground/10`, no shadow). Tonal layering — canvas → surface → surface-2 → surface-3 — does the depth work.
- **Do** keep the ambient atmosphere (ember field, film grain, photographic hero, gilt-foil masthead) on the Library page only; the app shell, reader, and admin surfaces stay plain.
- **Do** use the `bg-overlay` scrim token for every modal / dialog / sheet / popover backdrop.
- **Do** respect `prefers-reduced-motion`: tile stagger and dot pulse stop, the ember field holds a still frame, route crossfades become instant swaps, cinematic transitions snap. The cinematic register survives in palette and typography when motion withdraws.
- **Do** use the Lockup component for the brand mark, never the glyph or wordmark alone (except favicons / footer microtext).

### Don't:

- **Don't** introduce a severity colour ladder. No red/amber/green pills. No info-blue callouts. No "did you know" banners. Gold says "this matters"; the danger hue says "stop"; everything else is weight + density + motion. This is the One Accent Rule, hard.
- **Don't** ship surfaces that look like **SaaS-cream AI-workflow chrome** (Linear, Stripe, vercel-cream marketing). The first-order category trap for self-hosted media managers in 2026. Reverie is not a workflow tool.
- **Don't** ship surfaces that look like **generic ebook UI** (Calibre, Plex-clone shelves, default-Tailwind grids of identical title + author + cover cards). The category cliché. Reverie is in the category but must not look like it shipped from the category.
- **Don't** ship surfaces that look like a **cozy reading nook** (lamp, paper, warm-domestic, hand-drawn book stacks). The library is an archive, not a corner of a living room. The reader recedes; it does not invite.
- **Don't** ship surfaces that look like a **severity-coloured dashboard** (red/amber/green pills, info-blue callouts, status banners, hero-metric templates).
- **Don't** use `#000`. `#fff` is reserved for `--fg-on-danger` only; every other neutral tints warmly toward the brand hue.
- **Don't** put the gold accent fill on Parchment normal-size body text — it passes WCAG large-text only. Use the darker `accent-text` step for gold that must read as text. axe-core contrast violations on small-text gold are the right signal.
- **Don't** consume the sealed `--atm-*` atmosphere tokens from chrome. They are art-directed editorial constants for the Library masthead and ambient field only; no `bg-atm-*` utilities exist by design.
- **Don't** use `background-clip: text` with a gradient — with one carved-out exception: the gilt-foil Library masthead H1 (`.lib-h1-gilt`), an art-directed once-per-page treatment that falls back to a solid system colour under `forced-colors` (WCAG 1.4.1). Do not extend gilt text to any other surface.
- **Don't** load Author or Satoshi black weights (900). Both fight the boutique register; reach for size or tracking instead.
- **Don't** use side-stripe borders (`border-left` greater than 1px as a coloured accent). Absolute ban. Use a full border, background tint, leading numeral or icon, or nothing.
- **Don't** paint two focus indicators. The global 2px `accent-text` outline (with `box-shadow: none`) is the single indicator; don't reintroduce a competing box-shadow ring.
- **Don't** use em dashes (`—` or `--`) in UI copy. Commas, colons, semicolons, periods, or parentheses do the work.
- **Don't** centre the Slot in the glyph (it sits at 53 % from top), recolour the glyph, change the wordmark tracking, stack the Lockup vertically, or apply effects (shadow, glow, gradient, bevel, outline) to either part.
- **Don't** invent additional accents or state hues. Gold is the only accent; the danger red is the only state hue. Do not relitigate success / warning / info colours — they stay hue-less.
