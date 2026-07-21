---
title: Visual Identity
description: Tokens, type scale, motion, theme architecture, and the theme cookie lifecycle.
---

This page is the canonical reference for Reverie's visual surface. The
[brand identity](https://github.com/unkos-dev/reverie-branding/blob/main/identity.md)
remains the source of truth for colour, typography, mark, lockup, and
tagline; this page embeds the load-bearing parts and adds the runtime
detail (cookie lifecycle, FOUC mechanics, cross-stack contracts).

## Brand identity reference

- Mark: **Slot**: a single rectangular slot, the negative space of a
  bookshelf. See `identity.md` §1.
- Lockup: Slot + wordmark in horizontal lockup. The Lockup component at
  `frontend/src/components/Lockup.tsx` is the canonical render; see
  `identity.md` §6 for proportions.
- Tagline: **"Your library, catalogued."**
- Colours: Reverie Gold `#C9A961`, Ink `#0E0D0A`, Cream `#E8E0D0`,
  Parchment `#E8DCC2`. Gold, Ink, and Parchment are generator anchors
  (`ANCHORS` in `frontend/scripts/radix-gen/emit-primitives.ts`, alongside
  the danger and neutral seeds), so most runtime token values are derived
  ramp steps rather than the anchors themselves. Cream is not an anchor
  and is not a ramp step: it survives as a hand-placed constant in the
  Lockup and as `--cover-cream` in the atmosphere tier. The nearest ramp
  value, Dark's `--fg`, is `#F0EEE9`. See the token table below.
- Typography: Author Variable (display), Satoshi Variable (body),
  JetBrains Mono Regular (mono, loaded conditionally).

## Tokens

Tokens exist at two names. The Tier 2 semantic role (`--canvas`, `--fg`,
`--accent`) is what carries the runtime value in
`frontend/src/styles/themes/index.css`; an `@theme inline` block in the
same file maps each one to a `--color-*` name so Tailwind emits the
matching utility (`bg-canvas`, `text-fg`, `border-border-strong`). Write
utilities in components; read the semantic name when tracing a value.
The three-tier contract itself is documented in
[Color Tokens](/reverie/design/color-tokens/).

Every value below is a step in a generated Radix ramp. The primitive
column is the traceable source; the hex is what that step currently
resolves to in `frontend/src/styles/themes/primitives.generated.css`.
Regenerating the ramps changes the hex, so treat the primitive as
canonical and the hex as informative.

| Token              | Primitive         | Light       | Dark        | Purpose                                                |
| ------------------ | ----------------- | ----------- | ----------- | ------------------------------------------------------ |
| `--canvas`         | `--bg`            | `#E8DCC2`   | `#0E0D0A`   | Page canvas                                            |
| `--canvas-2`       | `sand-2`          | `#D3D1CF`   | `#191815`   | Slightly recessed canvas                               |
| `--surface`        | `sand-3`          | `#C8C6C2`   | `#24221E`   | Card / panel surface                                   |
| `--surface-2`      | `sand-4`          | `#BFBCB6`   | `#2B2924`   | Hover / elevated surface                               |
| `--surface-3`      | `sand-5`          | `#B6B2AB`   | `#34302A`   | Active / selected surface                              |
| `--border`         | `sand-6`          | `#ADA89F`   | `#3E3A32`   | Decorative separator (1.4.11-exempt)                   |
| `--border-strong`  | `sand-7`          | `#A29C90`   | `#4C473D`   | Hover / focus border                                   |
| `--border-control` | `sand-12`         | `#251F14`   | `#F0EEE9`   | Sole boundary of interactive controls                  |
| `--fg`             | `sand-12`         | `#251F14`   | `#F0EEE9`   | Primary text                                           |
| `--fg-muted`       | `sand-11`         | `#3D362A`   | `#BAB2A3`   | Secondary text                                         |
| `--fg-faint`       | `sand-a10`        | `#120A00AA` | `#FFEFCF7B` | Tertiary, sub-AA: decorative and placeholder only      |
| `--accent`         | `gold-9`          | `#A77C00`   | `#C9A961`   | Primary affordance fill and pressed state, never hover |
| `--accent-soft`    | `gold-3`          | `#D6C8A8`   | `#262217`   | Selected background; pair with `text-fg`               |
| `--accent-strong`  | `gold-10`         | `#967100`   | `#BE9E56`   | Pressed accent                                         |
| `--accent-text`    | `gold-11`         | `#5A3E00`   | `#D9B970`   | Text-grade gold and the focus ring                     |
| `--fg-on-accent`   | `gold-contrast`   | `#0E0D0A`   | `#0E0D0A`   | Text on the saturated `bg-accent` fill only            |
| `--danger`         | `danger-9`        | `#B91C1C`   | `#B91C1C`   | Destructive and unrecoverable error; never decorative  |
| `--danger-text`    | `danger-11`       | `#900000`   | `#FF9082`   | Text-grade danger                                      |
| `--danger-soft`    | `danger-3`        | `#D4C2BF`   | `#410A08`   | Danger background wash                                 |
| `--fg-on-danger`   | `danger-contrast` | `#FFFFFF`   | `#FFFFFF`   | Text on the `bg-danger` fill (6.47:1)                  |

Two tokens sit outside that table. `--overlay` is the modal, sheet, and
popover scrim, a named exception to the primitive contract: it is
`rgb(14 13 10 / 88%)` on both themes, because a Parchment-tinted scrim
would defeat the modal.

`--color-hover` has no Tier 2 runtime name; it is defined in the
`@theme inline` block as an alias of `--surface-2`. It decouples shadcn
primitives' hover and focus treatment from the gold register: dropdown
and select items light up at `--color-hover` on focus instead of
saturating gold, so `--accent` stays the unambiguous signature for
primary actions, focus rings, and recovery actions.

**Danger is the one state hue.** `--success`, `--warning`, `--info`, and
`--neutral` are deliberately absent, and adding one requires a
brand-aligned decision rather than a convenience commit. `--danger` is
the single bounded exception, recorded in
`adr/2026-06-18-single-danger-hue-amends-no-hue-philosophy.md`. Per WCAG
1.4.1 it never carries meaning alone: it always pairs with an icon,
weight, or text label. See
[Philosophy § State without hue](/reverie/design/philosophy/#state-without-hue)
and [Color Tokens](/reverie/design/color-tokens/).

### Using gold on Light without breaking contrast

On Parchment the Light accent (`gold-9`, `#A77C00`) measures 2.80:1,
which is below the WCAG 2.2 1.4.11 3:1 floor as a line and below 1.4.3
as text of any size. Darkening it far enough to clear 3:1 stops it
reading as gold, so the contrast is carried by _how the accent is used_
rather than by the value:

- As a **fill**, `bg-accent` pairs with `--fg-on-accent` (Ink) at 5.11:1,
  which clears 1.4.3 for normal text. The fill does the work, not the
  gold edge.
- As **text or a line**, use `--accent-text` (`gold-11`), which clears
  7.27:1 on Parchment and 10.29:1 on Ink.
- The **focus ring** uses `--accent-text` for exactly this reason and
  needs no halo.
- On `--accent-soft`, pair `text-fg`, not `text-fg-on-accent`: Dark's
  `--accent-soft` is a near-black gold tint, so Ink text on it is
  unreadable.

axe-core flags 1.4.11 and 1.4.3 violations on any Light surface using a
`gold-9` edge or `gold-9` text outside those mitigated cases. Used as
above, the accent needs no accessibility exception: the design-system
gate carries no contrast carve-out for gold. The one entry left in
`frontend/scripts/a11y/allowlist.mjs` is unrelated to the accent: it
covers cover-spine text whose paired background axe cannot attribute
through the spine's absolutely-positioned stack, a measurement false
positive rather than a tolerated shortfall. Introducing gold as a line,
an icon stroke, or normal-size text on _new_ Light surfaces is a brand
violation, not axe noise, and reviewers should reject it.

## Typography

| Role               | Family                  | Weight  |
| ------------------ | ----------------------- | ------- |
| Wordmark / Lockup  | Satoshi Variable        | 700     |
| Display headings   | Author Variable         | 500–600 |
| Section headings   | Author Variable         | 500     |
| Tagline            | Author Variable Italic  | 400     |
| Body               | Satoshi Variable        | 400     |
| Italic accent      | Satoshi Variable Italic | 400     |
| Mono (conditional) | JetBrains Mono          | 400     |

Variable woff2 are self-hosted at
`frontend/public/fonts/fontshare/files/`. See
`frontend/public/fonts/fontshare/README.md` for the SHA256SUMS
verification + refresh procedure.

## Spacing

4px base scale: 0, 4, 8, 12, 16, 24, 32, 48, 64, 96. Tailwind's default
spacing scale is the runtime; named tokens are not introduced because
the scale is conventional and the cost-of-renaming is high.

## Motion

| Token               | Value                               | Use                                 |
| ------------------- | ----------------------------------- | ----------------------------------- |
| `--duration-fast`   | 180ms                               | Micro-interactions (cursor changes) |
| `--duration-base`   | 240ms                               | Default interaction motion          |
| `--duration-slow`   | 320ms                               | Page-level transitions              |
| `--duration-theme`  | 300ms                               | Light ↔ Dark crossfade              |
| `--ease-standard`   | `cubic-bezier(0.22, 0.61, 0.36, 1)` | Default easing                      |
| `--ease-emphasised` | `cubic-bezier(0.16, 0.78, 0.30, 1)` | Accent-bearing motion               |

The `loading-pulse` keyframe (`opacity: 1 ↔ 0.85`, ~1.6s) carries the
loading-state expression: no `--color-loading` token; the loading
region pulses opacity instead. Reduced-motion preferences disable
ambient pulses.

## State expression

State communicates through typography weight, surface opacity, motion,
and the gold accent, with `--danger` as the single bounded hue exception.
The canonical state-to-expression mapping lives in
[Philosophy § State without hue](/reverie/design/philosophy/#state-without-hue)
and is not repeated here, because two copies of the same table is how
this page drifted from the code once already.

Charts and code blocks are scoped exceptions; when they ship, the
deviation is documented here and constrained to the surface that
requires it.

## Theme architecture

Three preferences: `system`, `light`, `dark`. Three places store the
state:

- The browser `reverie_theme` cookie (the canonical preference).
- `<html data-theme>` (the resolved effective theme, `light` or
  `dark`, never `system`).
- The `users.theme_preference` row in the database (the per-user
  preference that follows the user across devices).

### Cookie name three-place rule

The string `reverie_theme` lives in three places:

- `backend/src/auth/theme_cookie.rs` (`THEME_COOKIE_NAME` const)
- `frontend/src/fouc/fouc.js` (inline FOUC body, CSP-hashed at build)
- `frontend/src/lib/theme/cookie.ts`

All three MUST change together. The backend unit test on
`set_theme_cookie` enforces the backend side; the frontend side has no
automated parity guard yet, so the two must be kept in sync by hand.

### Cookie attribute parity

The cookie attributes are a parity contract between
`set_theme_cookie` (backend) and `writeThemeCookie` (frontend):

- `Path=/`
- `Max-Age=31536000` (one year, matches `Duration::days(365)` exactly)
- `SameSite=Lax`
- **No** `HttpOnly` (JS must read it before hydration)
- `Secure` (always set; Reverie requires HTTPS in production; localhost
  is a browser-recognised secure context, so dev still works)

Drift on either side produces two cookies of the same name with
divergent attributes; FOUC's `document.cookie.split('; ')` then matches
non-deterministically. Both sides have unit tests asserting the
attribute strings verbatim.

### FOUC mechanism

FOUC is a blocking inline `<script>` injected by
`frontend/vite-plugins/csp-hash.ts` at the `<!-- reverie:fouc-hash -->`
marker in `frontend/index.html`; the body lives at
`frontend/src/fouc/fouc.js` (plain ES5, self-invoking, try/catch
fallback to `light`). `vite build` emits `dist/csp-hashes.json`
containing the SHA-256, which `backend/src/security/dist_validation.rs`
reads at startup. CSP is hash-based: no nonce, no backend templating.

## Theme cookie lifecycle

`reverie_theme` survives logout by design. It is **device state**
(visual preference, non-PII, non-session-scoped), not session state.
This matches industry precedent (GitHub `color_mode`, MDN's site
preference, Audiobookshelf, Jellyfin, Kavita) and the shared-device
rationale: a device's user-distinct theme survives a session sign-out
without leaking identity.

A failed `PATCH` on a persisted preference change is handled
by response class. A 401 (anonymous), 403 (expired session or missing
CSRF), or 5xx (backend unavailable) keeps the optimistic value (the theme
must not flip mid-visit on an auth lapse or outage) and the reconcile on
the next successful authenticated load re-syncs to the server's stored
preference, so the server stays canonical across sessions. Only a
validation rejection (a 4xx other than 401/403) rolls back and surfaces a
toast, since that is a genuine client/server disagreement rather than an
auth lapse.

The cookie carries no PII, only the literal string `system`, `light`,
or `dark`. It is not `HttpOnly` because the FOUC script runs before any
module loader and must read it synchronously to avoid a flicker.

**Contrast rule:** any future _session-state_ cookie MUST be
`HttpOnly` and MUST clear on logout. `reverie_theme` is the explicit
counterexample; the contrast is documented at the backend module
header (`backend/src/auth/theme_cookie.rs`) and cross-referenced from
the operator-facing CSP doc.

## Mark, lockup, tagline

The Lockup component at `frontend/src/components/Lockup.tsx` is the
canonical render. Slot proportions and lockup spacing follow
`identity.md` §1 + §6.

```tsx
import { Lockup } from "@/components/Lockup";
<Lockup size={28} theme="dark" />;
```

The Lockup intentionally inlines `#C9A961` / `#0E0D0A` / `#E8E0D0` as
constants (philosophy §11C invariant; the Lockup must render
correctly even before `themes/index.css` resolves, e.g. on the OIDC
error page). It is the documented exemption to the hex-literal ban.
