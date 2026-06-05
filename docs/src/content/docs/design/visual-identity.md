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

- Mark: **Slot** — a single rectangular slot, the negative space of a
  bookshelf. See `identity.md` §1.
- Lockup: Slot + wordmark in horizontal lockup. The Lockup component at
  `frontend/src/components/Lockup.tsx` is the canonical render — see
  `identity.md` §6 for proportions.
- Tagline: **"Your library, catalogued."**
- Colours: Reverie Gold `#C9A961`, Ink `#0E0D0A`, Cream `#E8E0D0`,
  Parchment `#E8DCC2`. See the canonical token table below.
- Typography: Author Variable (display), Satoshi Variable (body),
  JetBrains Mono Regular (mono — conditional, UNK-113).

## Tokens

The canonical palette generates Tailwind utilities via `@theme inline`
in `frontend/src/styles/themes/index.css`. Token names are namespace
`--color-*` so `bg-canvas`, `text-fg`, `border-border-strong`, etc. all
resolve to brand variables.

| Token                   | Light                   | Dark                    | Purpose                                                             |
| ----------------------- | ----------------------- | ----------------------- | ------------------------------------------------------------------- |
| `--color-canvas`        | `#E8DCC2` (Parchment)   | `#14120E`               | Page canvas                                                         |
| `--color-canvas-2`      | `#DFD2B4`               | `#1A1812`               | Slightly recessed canvas                                            |
| `--color-surface`       | `#F0E6CF`               | `#221F18`               | Card / panel surface                                                |
| `--color-surface-2`     | `#E5D8BC`               | `#2A261D`               | Hover / elevated surface                                            |
| `--color-border`        | `#C7B894`               | `#2E2A22`               | Default border                                                      |
| `--color-border-strong` | `#B0A07C`               | `#3A3528`               | Hover / focus border                                                |
| `--color-fg`            | `#0E0D0A` (Ink)         | `#E8E0D0` (Cream)       | Primary text                                                        |
| `--color-fg-muted`      | `#5A5244`               | `#A8A090`               | Secondary text                                                      |
| `--color-fg-faint`      | `#8A8170`               | `#6E6858`               | Tertiary text                                                       |
| `--color-accent`        | `#8E6F38`               | `#C9A961` (Gold)        | Accent / focus / CTA — primary affordances only, never hover        |
| `--color-accent-soft`   | `#DCC890`               | `#4A3C24`               | Selected backgrounds (pair with `text-fg`, not `text-fg-on-accent`) |
| `--color-accent-strong` | `#6E5424`               | `#D4B070`               | Pressed accent                                                      |
| `--color-fg-on-accent`  | `#E8DCC2`               | `#0E0D0A`               | Text on saturated `bg-accent` only — fails AA on `bg-accent-soft`   |
| `--color-hover`         | `#E5D8BC` (= surface-2) | `#2A261D` (= surface-2) | shadcn-primitive hover/focus lift; decoupled from gold              |

**No state-color tokens.** `--color-success`, `--color-warning`,
`--color-danger`, `--color-info`, and `--color-neutral` are deliberately
absent — see [Philosophy § State without hue](/reverie/design/philosophy/#state-without-hue).

The Light-theme accent (`#8E6F38`) is the brand's `#C9A961` darkened to
satisfy WCAG 2.2 1.4.11 (UI component 3:1) and 1.4.3 large-text
contrast against `#E8DCC2`. It does **not** pass 1.4.3 normal-text
4.5:1 — restrict to focus rings, large CTAs, and recovery actions.
axe-core surfaces this as a violation on any Light surface where
`bg-accent` carries normal body text; the design-system axe gate
tolerates this documented violation on the `lg`-size primary button
surface in the `/design/system` gallery, but introducing
`bg-accent` on _new_ normal-size Light surfaces is a brand violation,
not an axe-noise issue.

`--color-hover` decouples shadcn primitives' hover/focus treatment from
the gold register: dropdown items and select items light up at
`--color-hover` (= `--color-surface-2`) on focus instead of saturating
gold, so brand `--color-accent` stays the unambiguous signature for
primary actions, focus rings, and recovery actions.

## Typography

| Role               | Family                  | Weight  |
| ------------------ | ----------------------- | ------- |
| Wordmark / Lockup  | Satoshi Variable        | 700     |
| Display headings   | Author Variable         | 600–700 |
| Section headings   | Author Variable         | 500–600 |
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
loading-state expression — no `--color-loading` token; the loading
region pulses opacity instead. Reduced-motion preferences disable
ambient pulses.

## State expression (no hue)

State communicates through typography weight, surface opacity, motion,
and the gold accent — never a state-coded hue. The canonical mapping:

| State                | Expression                                                                                                   |
| -------------------- | ------------------------------------------------------------------------------------------------------------ |
| Default / idle       | `text-fg`, `bg-surface` (or unchanged)                                                                       |
| Hover (surface lift) | `translate-y-[-1px]` + `border-border-strong`                                                                |
| Hover (in-list item) | `bg-hover` (= `bg-surface-2`)                                                                                |
| Active / pressed     | `bg-accent` or `bg-accent-strong`                                                                            |
| Selected             | `bg-accent-soft` background + `text-fg`                                                                      |
| Disabled             | `opacity-50` + `text-fg-muted` (`text-fg-faint` is decorative-only — opacity-50 × fg-faint drops below AA)   |
| Loading              | opacity pulse 0.85 ↔ 1.0, ~1.6s, on the region                                                               |
| Error                | `text-fg font-semibold` + gold recovery action                                                               |
| Success (explicit)   | gold inline note (`text-fg-on-accent` on full `bg-accent` fill); fades after ~3s                             |
| Link                 | underline + `text-accent` on hover; no permanent colour difference                                           |
| Focus (keyboard)     | 2px gold outline + 2px offset (`focus-visible:ring-2 focus-visible:ring-accent focus-visible:ring-offset-2`) |

Charts and code blocks are scoped exceptions — when they ship, the
deviation is documented here and constrained to the surface that
requires it.

## Theme architecture

Three preferences: `system`, `light`, `dark`. Three places store the
state:

- The browser `reverie_theme` cookie (the canonical preference).
- `<html data-theme>` (the resolved effective theme — `light` or
  `dark`, never `system`).
- The `users.theme_preference` row in the database (the per-user
  preference that follows the user across devices).

### Cookie name three-place rule

The string `reverie_theme` lives in three places:

- `backend/src/auth/theme_cookie.rs` (`THEME_COOKIE_NAME` const)
- `frontend/src/fouc/fouc.js` (inline FOUC body, CSP-hashed at build)
- `frontend/src/lib/theme/cookie.ts`

All three MUST change together. The backend unit test on
`set_theme_cookie` enforces the backend side; cross-stack drift is
tracked under [UNK-105](https://linear.app/unkos/issue/UNK-105).

### Cookie attribute parity

The cookie attributes are a parity contract between
`set_theme_cookie` (backend) and `writeThemeCookie` (frontend):

- `Path=/`
- `Max-Age=31536000` (one year, matches `Duration::days(365)` exactly)
- `SameSite=Lax`
- **No** `HttpOnly` (JS must read it before hydration)
- `Secure` (always set — Reverie requires HTTPS in production; localhost
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
reads at startup. CSP is hash-based — no nonce, no backend templating.

## Theme cookie lifecycle

`reverie_theme` survives logout by design. It is **device state**
(visual preference, non-PII, non-session-scoped), not session state.
This matches industry precedent (GitHub `color_mode`, MDN's site
preference, Audiobookshelf, Jellyfin, Kavita) and the shared-device
rationale: a device's user-distinct theme survives a session sign-out
without leaking identity.

The cookie carries no PII — only the literal string `system`, `light`,
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
constants (philosophy §11C invariant — the Lockup must render
correctly even before `themes/index.css` resolves, e.g. on the OIDC
error page). It is the documented exemption to the hex-literal ban.
