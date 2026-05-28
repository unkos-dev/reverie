---
name: Reverie
description: Your library, catalogued.
colors:
  # Brand-canonical (theme-independent identity colors)
  parchment: "#E8DCC2"
  ink: "#0E0D0A"
  cream: "#E8E0D0"
  reverie-gold: "#C9A961"
  reverie-gold-light: "#8E6F38"
  reverie-alarm: "#F26C7B"
  reverie-alarm-light: "#BC2937"

  # Light theme (Parchment-anchored) — surface ramp
  canvas-light: "#E8DCC2"
  canvas-2-light: "#DFD2B4"
  surface-light: "#F0E6CF"
  surface-2-light: "#E5D8BC"
  border-light: "#C7B894"
  border-strong-light: "#B0A07C"
  fg-light: "#0E0D0A"
  fg-muted-light: "#5A5244"
  fg-faint-light: "#8A8170"
  accent-light: "#8E6F38"
  accent-soft-light: "#DCC890"
  accent-strong-light: "#6E5424"
  fg-on-accent-light: "#E8DCC2"

  # Dark theme (Ink-anchored) — surface ramp
  canvas-dark: "#14120E"
  canvas-2-dark: "#1A1812"
  surface-dark: "#221F18"
  surface-2-dark: "#2A261D"
  border-dark: "#2E2A22"
  border-strong-dark: "#3A3528"
  fg-dark: "#E8E0D0"
  fg-muted-dark: "#A8A090"
  fg-faint-dark: "#6E6858"
  accent-dark: "#C9A961"
  accent-soft-dark: "#4A3C24"
  accent-strong-dark: "#D4B070"
  fg-on-accent-dark: "#0E0D0A"
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
    backgroundColor: "{colors.reverie-alarm}"
    textColor: "{colors.ink}"
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

Reverie's surfaces feel like a museum after hours — the collection is the point, the lighting is considered, the chrome stays out of the reader's way. Cinematic-boutique is the locked visual register: motion is felt in localised heroes (parallax cover backdrops, slow auto-shifts, gold-accented micro-interactions), typography does the structural work, and information hierarchy is disciplined. The library and detail views carry the identity; the reader recedes into a calm long-form surface that inherits palette and motion language but withdraws nearly all chrome.

Restraint is the rule. The brand has exactly one accent (Reverie Gold) and exactly one alarm (Reverie Alarm) — every other state, hierarchy, and emphasis signal is carried by weight, opacity, type scale, density, and motion. No severity ladder. No third hue earns its screen. The category-reflex check has two altitudes the system answers no to: it does not look like a self-hosted media manager (Calibre, Plex), and it does not look like an AI workflow tool (SaaS-cream chrome, Linear-clone gradients).

**Key Characteristics:**

- Editorial pairing: Author (display) + Satoshi (workhorse) + JetBrains Mono (metadata).
- Two anchored canvases: Parchment for light, Ink for dark. Both surface ramps tint warmly toward the brand hue. No neutral greys.
- Single gold accent (≤10% of any screen), single alarm (fill-only, two carve-outs only).
- Flat by default; signature shadows reserved for localised hero treatments (home hero, book-detail key art).
- Cinematic-boutique motion budget: 180–320 ms ease-out, no bounce, no spring; `prefers-reduced-motion` respected as a first-class state.

## 2. Colors

A warm, considered palette of parchment, ink, cream, and gold. No greys. The dark theme is Ink-anchored; the light theme is Parchment-anchored. Both tint surfaces warmly toward the brand hue rather than collapsing toward neutrals.

### Primary

- **Reverie Gold** (`#C9A961` on dark; `#8E6F38` on light): the accent. Reserved for focus rings, primary CTAs, recovery actions out of error states, and the Slot mark fill. The single source of glow. On light surfaces, the gold darkens to `#8E6F38` because Reverie Gold at `#C9A961` does not meet WCAG AA contrast against Parchment; the same hue, the legibility-required value shift.

### Secondary

- **Reverie Alarm** (`#F26C7B` on dark; `#BC2937` on light): the alarm. Carve-out from the single-accent rule. Reinforcement only — destructive intent and unrecoverable error are communicated through copy, friction, and iconography first; the alarm makes those signals louder for users who can perceive it. Fill, border, or icon only — **never body text**. Both hues sit at 354° deliberately cool of pure red so they cannot be misread as warm cousins of gold.

### Neutral — surface ramp (Light theme · Parchment-anchored)

- **canvas-light** (`#E8DCC2`): Parchment. The default page background. Warm, slightly aged.
- **canvas-2-light** (`#DFD2B4`): the layer beneath canvas; nav backdrops, sidebars, less-active surfaces.
- **surface-light** (`#F0E6CF`): the layer above canvas; cards, dialogs, dropdowns.
- **surface-2-light** (`#E5D8BC`): the layer above surface; hover state for primitives, popovers over surfaces.
- **border-light** (`#C7B894`) / **border-strong-light** (`#B0A07C`): hairline and structural borders.
- **fg-light** (`#0E0D0A` Ink) / **fg-muted-light** (`#5A5244`) / **fg-faint-light** (`#8A8170`): text hierarchy.

### Neutral — surface ramp (Dark theme · Ink-anchored)

- **canvas-dark** (`#14120E`): Ink-leaning page background. Slightly warmer than absolute black.
- **canvas-2-dark** (`#1A1812`): beneath-canvas layer.
- **surface-dark** (`#221F18`) / **surface-2-dark** (`#2A261D`): card and hover-elevation layers.
- **border-dark** (`#2E2A22`) / **border-strong-dark** (`#3A3528`): borders.
- **fg-dark** (`#E8E0D0` Cream) / **fg-muted-dark** (`#A8A090`) / **fg-faint-dark** (`#6E6858`): text hierarchy.

### Named Rules

**The One Accent Rule.** Reverie Gold is the only accent. State, hierarchy, and emphasis below the level of "this matters" are communicated through weight, opacity, type scale, density, and motion — never through additional hues. There is no severity ladder. No third hue earns its screen.

**The Alarm Carve-Out Rule.** Reverie Alarm appears in exactly two contexts: (1) irreversible destructive confirmation (alongside typed-name confirmation and an undo window where feasible), and (2) unrecoverable system errors that would otherwise survive as a wrong "it's working" mental model. Nowhere else. The carve-out exists because some moments cannot be communicated by weight alone without compromising safety.

**The Light-Gold Restriction Rule.** On Parchment, `#8E6F38` passes WCAG 1.4.11 (UI 3:1) and 1.4.3 large-text, but not 1.4.3 normal text (4.5:1). Therefore on light surfaces, gold is used only for focus rings, large CTAs (the primary action on a surface), and recovery actions out of error states. Not body text, not links, not inline emphasis. axe-core contrast violations on small-text gold are the right signal — the surface is misusing the accent.

**The No-Black-No-White Rule.** `#000` and `#fff` are prohibited. Every neutral tints warmly toward the brand hue: Ink is `#0E0D0A`, Cream is `#E8E0D0`, Parchment is `#E8DCC2`. The doctrine is OKLCH thinking with hex implementation; if a token ever needs to be added, derive it in OKLCH first, then commit the hex.

## 3. Typography

**Display Font:** Author Variable (with system-ui, -apple-system, Segoe UI, sans-serif fallback).
**Body Font:** Satoshi Variable (with system-ui, -apple-system, Segoe UI, sans-serif fallback).
**Mono Font:** JetBrains Mono Regular (with ui-monospace, SFMono-Regular, Menlo fallback). Used for metadata surfaces (ISBN, IDs, format codes).

All three are self-hosted variable woff2 files at `public/fonts/fontshare/files/` with `font-display: swap`. The Fontshare CDN is broken in Chromium under the production CSP (Opaque Response Blocking trips on cookie-bearing CSS responses); self-hosting bypasses that and matches `font-src 'self'`.

**Character:** Author and Satoshi are a deliberate Fontshare pairing — Author does the editorial work (display, book titles in detail, italic accent moments), Satoshi does the structural work (body, navigation, controls, wordmark). The wordmark uses Satoshi rather than Author because the wide-tracked 0.32em uppercase stamp is itself doing identity work; a neutral grotesque lets that treatment carry without competing with display character. JetBrains Mono carries metadata where mono affordance is needed (technical fields, debug surfaces).

### Hierarchy

- **Display** (Author 500, `clamp(2rem, 4vw, 3.25rem)`, line-height 1.05, tracking -0.012em): hero headlines, book detail titles, marquee surfaces.
- **Headline** (Author 500, 1.875rem / 30px, line-height 1.15, tracking -0.012em): section headers in the library and detail views.
- **Title** (Satoshi 500, 1.125rem / 18px, line-height 1.35): card titles, primary affordance labels.
- **Body** (Satoshi 400, 0.9375rem / 15px, line-height 1.55): paragraph text, descriptions, control text. Cap line length at 65–75ch for prose.
- **Label** (JetBrains Mono 400, 0.75rem / 12px, tracking 0.04em, uppercase small): metadata fields (ISBN, file size, format codes), debug surfaces. Use sparingly; mono is signal, not chrome.
- **Wordmark** (Satoshi 700, 1.75rem reference / 28px, tracking 0.32em, uppercase, padding-left 0.32em for optical balance): the canonical Lockup component only. Not a heading style.

### Named Rules

**The Sans-Only Rule.** Reverie deliberately rejected the chrome-sans / content-serif convention. Author and Satoshi both sit in the sans family — Author's wide-letterform editorial energy carries the display register that a content serif would normally fill. There is no content serif in the system.

**The 65-75ch Body Rule.** Body text caps at 65–75ch for prose surfaces (book descriptions, blog-shape content). Library cards and detail panels are not prose and may exceed this; reader text is user-configurable and overrides the system default.

**The Italic-as-Editorial-Accent Rule.** Italics use Author italic (the editorial counterweight), not Satoshi italic. Reserved for book titles, ship-names, internal-monologue, and the tagline (which uses Author 400 regular, not italic). Not for general emphasis.

**The No-Black-Weight Rule.** Author and Satoshi black weights (900) are not loaded. Both fight the boutique register; their absence is deliberate. If a display moment needs more weight, increase size or use tracking; do not reach for 900.

## 4. Elevation

Flat by default, tonal layering for everyday depth, with one signature shadow reserved for localised hero treatments only.

Everyday depth comes from the surface ramp: canvas → surface → surface-2 + border / border-strong does the work for cards, dialogs, dropdowns, and primitives. Shadcn primitives currently use `ring-1 ring-foreground/10` on cards rather than box-shadow; that ring is the rest state. Hover and focus lift use opacity and the `--surface-2` hover token, not shadow.

The cinematic register earns exactly one shadow vocabulary: the **hero glow** on the home hero, book-detail key-art region, and other carved-out signature surfaces where the philosophy spec already endorses parallax. Outside those surfaces, shadows are absent. SaaS-cream chrome (ambient drop-shadows on every card) is anti-ref'd in PRODUCT.md — flat by default is how that line gets held.

### Shadow Vocabulary

- **hero-glow-rest** (`box-shadow: 0 16px 48px -16px rgba(201, 169, 97, 0.22)`): a low-opacity gold-tinted glow under hero-class surfaces only. Resting state on the home hero and book-detail key-art region. Never on cards, never on dialogs.
- **hero-glow-hover** (`box-shadow: 0 24px 64px -20px rgba(201, 169, 97, 0.32)`): the hover lift on the same surfaces. The transition uses `--ease-emphasised` over `--duration-base` (240 ms).
- **focus-ring** (`ring-3` via the `--accent` token): not a shadow per se, but the only chrome that "elevates" the focused element. The brand's "this matters" gesture made visible.

### Named Rules

**The Flat-By-Default Rule.** Surfaces are flat at rest. Depth comes from tonal layering (canvas → surface → surface-2). Shadows appear only on the hero-class surfaces carved out by the philosophy spec, or as a response to state (the focus ring, the hero hover lift). If a card's resting state needs a drop shadow to look complete, the design is wrong.

**The No-Ambient-Drift Rule.** No canvas-wide ambient drift. No background gradient that runs across the whole surface. The philosophy spec rejected this at D2 — it pulled focus from content and dated the surfaces against contemporary cinematic boutique software. Atmospheric interest comes from cover art, gold accent, typography, and the localised hero treatments. The hero glow is the only ambient element, and it lives inside a carved-out hero region.

## 5. Components

The component philosophy is **considered, inscribed**: every primitive feels like it was placed deliberately and cannot be moved by accident. Each affordance is a carved gesture, not a frictionless tap. Weight + restraint + intent.

Reverie ships shadcn/ui primitives (button, card, input, dialog, alert-dialog, dropdown, etc.) rebound to the canonical brand palette via a shadcn-compatibility alias layer in `styles/themes/index.css`. New code prefers the canonical `--color-canvas/--color-fg/--color-accent/…` token names; the alias layer means stock primitives drop in without per-file rewrites.

### Buttons

- **Shape:** rounded-lg (0.75 rem / 12 px) by default. Smaller variants reach for rounded-md (0.5 rem) only when the button itself is small enough (sm/xs sizes use `min(var(--radius-md), 12px)` and `min(var(--radius-md), 10px)` respectively).
- **Primary:** bg-accent (`#C9A961` dark / `#8E6F38` light), text-fg-on-accent, no border. The single "this matters" gesture on the surface. Reserved for the primary action — there is at most one Primary per surface.
- **Outline:** transparent bg, border-input (border token), text-fg. The everyday button; appears multiple times per surface.
- **Ghost:** transparent rest, hover-bg surface-2, text-fg. Tertiary affordance, denser surfaces (toolbars, dropdown headers).
- **Destructive:** bg-destructive (aliased to fg — no native red token; the surface enforces the no-hue-coded-state rule). The Alarm Carve-Out applies: real destructive intent uses the dedicated alert-dialog with typed-name confirmation and Reverie Alarm fill on the confirm action, not this default destructive variant.
- **Hover / Focus:** primary hovers to `bg-primary/80` (opacity step). Outline and ghost hover to `bg-muted` → fg-foreground. Focus shows the `focus-visible` ring at 3 px in `--accent` with 50 % opacity. No movement on hover; `active` translates 1 px down (`active:translate-y-px`) for the carved-in-press affordance.
- **Sizes:** default h-8 (32 px), xs h-6, sm h-7, lg h-9. Icon-only variants are square at the matching height.

### Cards

- **Corner Style:** rounded-xl (0.875 rem / 14 px).
- **Background:** bg-card (alias to surface). Slight warm tint above canvas.
- **Border / Shadow Strategy:** `ring-1 ring-foreground/10` at rest. No drop shadow. Flat-By-Default applies.
- **Internal Padding:** py-4 default (16 px vertical, full-bleed images extend edge-to-edge); px-4 on header / content / footer. Size `sm` collapses to gap-3 / py-3.
- **Composition:** Header (with optional title + description + action grid), Content, Footer. Cards do not nest. Cards do not stack inside cards.

### Inputs

- **Style:** h-8 (32 px), rounded-lg (0.75 rem), border-input, transparent bg in light theme (`bg-input/30` in dark).
- **Focus:** `focus-visible:border-ring` + `ring-3` at `ring-ring/50` (gold accent with 50 % opacity, 3 px ring). The focus ring is the inscription gesture — small but unmistakable.
- **Disabled:** `cursor-not-allowed`, `opacity-50`, `bg-input/50`.
- **Invalid:** `aria-invalid` swaps border and ring to `destructive`. In practice the destructive alias maps to fg (no red), so visible invalid state is enforced by the surface (helper text in fg-muted, weight bump, and where warranted, Reverie Alarm fill on the recovery CTA).

### Dialogs

- **Alert-Dialog (destructive confirmation):** the canonical Alarm Carve-Out surface. Typed-name confirmation, Reverie Alarm fill on the confirm button, plain Outline button on the cancel. Cancel is the safer default and gets focus.
- **Dialog (general):** bg-surface, rounded-xl, no shadow. Backdrop is a low-opacity neutral overlay — never `bg-black/50` (currently flagged in impeccable detect; fix in next dialog-touching PR). Use sparingly: the absolute bans include "Modal as first thought".

### Navigation

- **Style:** chrome typography is Satoshi 500; nav items are Title scale (1.125 rem). Default state is fg-muted; hover lifts to fg; active is fg + a 1 px gold underline (the only place a gold line lives — focus rings are 3 px and behave differently).
- **Mobile:** sheet-based, not bottom-nav. The library is the identity surface and earns horizontal scroll affordances; nav stays vertical and quiet.

### Signature: Lockup

The canonical brand expression — glyph (the Slot) + wordmark, inline. Lives at `src/components/Lockup.tsx`. The Lockup is **brand-locked**, not theme-flexible: glyph fill is always `#C9A961` Reverie Gold, never recolored; the slot sits at 53 % from top (not centered — centered reads mechanical / mail-slot, below-center reads inscribed); wordmark uses inline-literal `#0E0D0A` / `#E8E0D0` rather than design tokens so it renders correctly even before the theme tree resolves.

Three forms ranked by canonicality: (1) lockup with glyph — canonical; (2) glyph alone — favicons, very compressed contexts; (3) wordmark alone — only when neither fits (footer microtext, plain-text email signatures). Never use the wordmark alone as a primary mark — wide-tracked uppercase grotesques drift toward DTC luxury minimalism without the glyph anchor.

### Named Rules

**The One-Primary-Per-Surface Rule.** Each surface has at most one Primary button. If two actions seem to need primary weight, one of them is actually destructive (route to alert-dialog) or the surface is doing two jobs (split it).

**The No-Bottom-Nav Rule.** Mobile nav is a sheet, not a bottom bar. Bottom-nav is a phone-app reflex and reads consumer-app; Reverie's mobile presence is a complement to the desktop archive, not a standalone mobile app.

**The Lockup-As-Brand-Carrier Rule.** The Lockup component is invariant. It uses inline hex values, not theme tokens. It must render correctly even before the theme tree resolves (philosophy §11C). Do not theme it; do not re-style its parts; do not use its glyph or wordmark separately as decorative elements.

## 6. Do's and Don'ts

### Do:

- **Do** anchor every screen on Parchment (`#E8DCC2`) or the Ink-anchored dark canvas (`#14120E`); the two canvases are the only legitimate page backgrounds. Canonical Ink (`#0E0D0A`) is the foreground colour on light surfaces, not a canvas value.
- **Do** use Reverie Gold (`#C9A961` dark / `#8E6F38` light) for focus rings, primary CTAs, recovery actions out of error states, and the Slot mark fill. Nothing else.
- **Do** restrict Reverie Alarm to fill, border, or icon — never body text. Use it in exactly the two carve-out contexts (irreversible destructive confirm, unrecoverable system error).
- **Do** carry hierarchy through weight, opacity, type scale, density, and motion. Author or Satoshi at different weights is the system's primary expressiveness.
- **Do** cap body text at 65–75ch on prose surfaces; cards and detail panels are not prose and may run wider.
- **Do** keep cards flat at rest (`ring-1 ring-foreground/10`, no shadow). Tonal layering — canvas → surface → surface-2 — does the depth work.
- **Do** reserve shadows for hero-class surfaces only (home hero, book-detail key art). The signature `hero-glow` token is the only legitimate shadow in the system.
- **Do** respect `prefers-reduced-motion`: status pulses become static, hover lift becomes opacity, cursor parallax disables. The cinematic register survives in palette and typography when motion withdraws.
- **Do** use the Lockup component for the brand mark, never the glyph or wordmark alone (except in the carved-out edge cases: favicons, footer microtext).

### Don't:

- **Don't** introduce a severity colour ladder. No red/amber/green pills. No info-blue callouts. No "did you know" banners. Gold says "this matters"; Reverie Alarm says "stop"; everything else is weight + density + motion. This is the One Accent Rule, hard.
- **Don't** ship surfaces that look like **SaaS-cream AI-workflow chrome** (Linear, Stripe, vercel-cream marketing). It is the first-order category trap for self-hosted media managers in 2026. Reverie is not a workflow tool.
- **Don't** ship surfaces that look like **generic ebook UI** (Calibre, Plex-clone shelves, default-Tailwind grids of identical title + author + cover cards). The category cliché. Reverie is in the category but must not look like it shipped from the category.
- **Don't** ship surfaces that look like a **cozy reading nook** (lamp, paper, warm-domestic, hand-drawn book stacks). The philosophy spec rejected this lane. The library is an archive, not a corner of a living room. The reader recedes; it does not invite.
- **Don't** ship surfaces that look like a **severity-coloured dashboard** (red/amber/green pills, info-blue callouts, status banners, hero-metric templates). Repeat of the One Accent Rule, but as an anti-pattern audit.
- **Don't** use `#000` or `#fff`. Every neutral tints warmly toward the brand hue. Ink is `#0E0D0A`, Cream is `#E8E0D0`, Parchment is `#E8DCC2`.
- **Don't** put Reverie Gold on Parchment normal-size body text. The light-theme accent at `#8E6F38` passes WCAG 1.4.11 and 1.4.3 large-text only — never 1.4.3 normal text. axe-core contrast violations on small-text gold are the right signal: the surface is misusing the accent.
- **Don't** use `bg-black/50` as a dialog backdrop. Currently flagged by impeccable detect in `alert-dialog.tsx`, `dialog.tsx`, `sheet.tsx`; fix in the next modal-touching PR. Use a low-opacity warm-neutral overlay instead (a tint of canvas-2 + opacity).
- **Don't** load Author or Satoshi black weights (900). Both fight the boutique register. If a display moment needs more weight, reach for size or tracking.
- **Don't** use side-stripe borders (`border-left` greater than 1px as a coloured accent). Absolute ban. If a call-out needs visual weight, use a full border, background tint, leading numeral or icon, or nothing.
- **Don't** use gradient text (`background-clip: text` with a gradient). Decorative, never meaningful. Use a single solid colour; emphasis comes from weight and size.
- **Don't** reach for glassmorphism, hero-metric templates, or identical card grids by default. All three are anti-pattern reflexes for this category.
- **Don't** use em dashes (`—` or `--`) in UI copy. Commas, colons, semicolons, periods, or parentheses do the work.
- **Don't** centre the Slot in the glyph. It sits at 53 % from top. Centred reads mail-slot; below-centre reads inscribed.
- **Don't** recolour the glyph block, change the wordmark tracking, stack the Lockup vertically, or apply effects (shadow, glow, gradient, bevel, outline) to either.
- **Don't** invent additional accents or alarms. Gold is the only accent; Reverie Alarm is the only alarm. Identity.md §4 logs every accent already considered and rejected (warning amber, success green, info blue, red severity tints, branded burgundy/oxblood alarm). Do not relitigate.
