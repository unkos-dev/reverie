# Plan: Design System D3 — Codify Canonical Theme (brand-aligned)

> [!IMPORTANT]
> **This plan supersedes the D3 phase of `.claude/PRPs/plans/design-system.plan.md`
> (lines 1187–1492).** That plan was authored before the April 2026 brand
> identity work landed and treats the winning D2 direction's tokens as the
> canonical seed. The brand identity at
> [unkos-dev/reverie-branding](https://github.com/unkos-dev/reverie-branding)
> is now the source of truth for color, typography, mark, lockup, and tagline;
> D3.1, D3.7, D3.9, D3.15, D3.16, D3.18, and D3.20 are materially refreshed,
> and the remaining sub-tasks get a once-over to swap `--mg-*` for `--color-*`
> and drop hue-coded state assumptions.
>
> **Pre-D3 reference still valid:** parent plan §"New Patterns to Establish"
> (lines 246–797) — `USER_MODEL_COLUMN_ADDITION`, `PATCH_HANDLER_SHAPE`,
> `THEME_COOKIE_WRITER`, `THEME_COOKIE_FRONTEND_WRITER`, `SQLX_TEST_HARNESS`,
> `FOUC_INLINE_SCRIPT`, `THEME_PROVIDER`, `SHADCN_COMPONENTS_JSON` — are
> brand-neutral and remain the canonical pattern set. Only
> `TAILWIND_V4_MULTI_THEME` is re-emitted in this plan because token names and
> values change.
>
> **Numbering:** D3.1–D3.20 retained. Most sub-tasks are content refreshes
> within unchanged shape; renumbering would break references in
> `feedback_shared_constants_tracker.md`, the TODO comment in
> `frontend/src/fouc/fouc.js`, and other downstream artefacts.
> **D3.16 keeps its number but reverses approach** — not `@fontsource`
> (no Author/Satoshi packages), but self-hosted variable woff2 fetched
> from Fontshare's per-font download endpoint. Implementers must read
> D3.16's full body, not pattern-match on the original verb.
>
> **Related Linear issues:** [UNK-103](https://linear.app/unkos/issue/UNK-103)
> (Step 10 design-system epic — the parent for this work),
> [UNK-104](https://linear.app/unkos/issue/UNK-104) (OIDC e2e test),
> [UNK-105](https://linear.app/unkos/issue/UNK-105) (shared-constants pipeline),
> [UNK-113](https://linear.app/unkos/issue/UNK-113) (post-0.1.0 JetBrains
> Mono usage review).

## Summary

Codify Reverie's design system against the April 2026 brand identity:
self-hosted variable woff2 fonts (Author + Satoshi + conditional JetBrains
Mono), `--color-*` semantic tokens sourced directly from
`unkos-dev/reverie-branding/identity.md`, no hue-coded state tokens, themed
shadcn primitives bound to the canonical token set, FOUC-free Dark/Light/System
theme switching backed by a per-user DB column, and a dev-only
`/design/system` primitive gallery that becomes the visual contract for
Step 11. The Slot mark and Lockup component already shipped in PR #51; this
phase wires the theme machinery beneath them.

## User Story

As a Reverie user
I want the web UI to render Reverie Gold on warm Ink/Cream/Parchment surfaces,
typeset in Author + Satoshi, with my Dark/Light/System preference remembered
across devices and never flickering on load
So that every subsequent feature step inherits a brand-aligned visual contract
instead of accumulating ad-hoc styling decisions.

## Problem → Solution

**Current state** (post PR #51, on `main` at `4febd8e`):

- Three D2 explore trees still mounted in `frontend/src/main.tsx:6–9` and
  routed at `/design/explore/{midnight-gold,signal,atelier-ink}` — direction
  exploration outcome already decided, trees are dead weight.
- `frontend/src/index.css` is bare `@import "tailwindcss"`; no tokens, no
  `@theme inline`, no `@custom-variant dark`.
- D2 `tokens.css` files use `--mg-*` / direction-prefixed names, include
  `--mg-success/warning/danger/info`, and embed `cdn.fontshare.com` `@font-face`
  blocks (philosophy spec §6 reverses the CDN decision).
- `frontend/src/fouc/fouc.js` is the 5-line placeholder from D0.13.
- `frontend/public/fonts/fontshare/files/` does not exist; the
  `frontend/public/fonts/fontshare/README.md` still describes the
  CDN-as-EULA-compliant rationale (stale; reversed by philosophy §6).
- Backend: `backend/src/auth/theme_cookie.rs` does not exist; `users` table
  has no `theme_preference` column; `/auth/me` does not include the field;
  no `PATCH /auth/me/theme` route.
- `backend/src/security/csp.rs:30,80` and `frontend/vite.config.ts:16` both
  declare `font-src 'self' https://cdn.fontshare.com` — needs to drop the
  CDN allowance once self-hosted woff2 lands.
- `frontend/src/components/Lockup.tsx` and `frontend/index.html` brand head
  are already in place (PR #51); they hard-code `#C9A961`, `#0E0D0A`, `#E8E0D0`
  inline because no token system exists yet.

**Desired state:**

- `App.tsx` boots into a minimal canonical-canvas shell
  (`<main className="bg-canvas text-fg min-h-screen">…</main>`) so brand
  identity is visible at first paint — Vite hero scaffold is replaced.
  The shell intentionally stays bare; Step 11 builds the library view on
  top of it.
- Single canonical theme tree at `frontend/src/styles/themes/{dark,light,index}.css`
  expressing the brand palette via `--color-*` tokens, with no state-color
  tokens (`--color-success/warning/danger/info` deliberately absent).
- Self-hosted variable woff2 at `frontend/public/fonts/fontshare/files/` for
  Author normal/italic, Satoshi normal/italic, plus JetBrains Mono 400 (the
  latter conditional on UNK-113 review post-0.1.0). `@font-face` blocks live
  in the canonical theme tree; CSP returns to `font-src 'self'`.
- Themed shadcn primitives in `frontend/src/components/ui/` whose `cva`
  composition references token utilities (`bg-surface`, `text-fg`,
  `border-border`, `text-accent`) — no shadcn stock visual DNA.
- Theme provider sources `preference` from `reverie_theme` cookie and
  `effective` from `dataset.theme`; cross-tab sync via `BroadcastChannel`;
  optimistic update with rollback on PATCH failure.
- Backend writes `reverie_theme` cookie on OIDC callback and on
  `PATCH /auth/me/theme`; cookie attributes pinned to a contract that the
  frontend's `writeThemeCookie` matches verbatim.
- Dev-only `/design/system` route renders every primitive in every state
  in both themes; production bundle tree-shakes `/design` chunks structurally
  via `manualChunks`. Lockup component (already shipped) re-references
  `Satoshi Variable` font-family to match the canonical naming.
- `docs/src/content/docs/design/visual-identity.md` references the brand
  repo as the SoT for color, typography, mark, lockup, and tagline; pulls
  the rest from the rewritten philosophy spec (§6, §8, §10, §11, §11A,
  §11B, §11C).
- Operator CSP doc updated: `font-src 'self'` only; `Cookies` section lists
  `id` (HttpOnly, session) and `reverie_theme` (non-HttpOnly, 365 days).

## Metadata

| Field | Value |
|---|---|
| Branch | `feat/design-system-d3` (off `main` at `4febd8e`) |
| Parent plan | `.claude/PRPs/plans/design-system.plan.md` (D3 section superseded) |
| Brand SoT | [unkos-dev/reverie-branding](https://github.com/unkos-dev/reverie-branding) `identity.md` |
| Spec | `plans/2026-04-25-design-system-philosophy-design.md` (gitignored) |
| Reconciliation | `plans/2026-04-26-brand-reconciliation.md` (gitignored) |
| Type | NEW_CAPABILITY (foundation for Step 11) |
| Complexity | HIGH (DB + backend + frontend + docs; cross-stack cookie parity; tree-shake gate; 20 sub-tasks; brand alignment) |
| Systems Affected | `frontend/`, `backend/`, `docs/`, CI |
| Estimated tasks | 20 (D3.1–D3.20) |
| Estimated files | ~45 (1 migration up/down, ~3 backend edits, 1 new backend module, 4–5 self-hosted woff2 + checksums, ~22 shadcn primitives, ~6 theme/provider/switcher files, 1 primitive-gallery route file, 1 design-route gate file, 2 docs files, 1 operator-CSP-doc edit, 1 stale README rewrite, 3 CSP file edits) |
| Out of scope | Library grid, book detail page, search UI, hero screens, real `/api/books` integration (all Step 11) |

---

## UX Design

### Before State

```
╔═══════════════════════════════════════════════════════════════════════════╗
║                              BEFORE STATE                                  ║
╠═══════════════════════════════════════════════════════════════════════════╣
║                                                                           ║
║   Visit /                                                                 ║
║   ┌─────────────┐         ┌──────────────┐         ┌──────────────────┐  ║
║   │ Bare Vite   │ ──────► │ Logos render │ ──────► │ No identity, no  │  ║
║   │ scaffold    │         │ count++ btn  │         │ theme awareness  │  ║
║   └─────────────┘         └──────────────┘         └──────────────────┘  ║
║                                                                           ║
║   Visit /design/explore/midnight-gold (etc)                               ║
║   ┌─────────────┐         ┌──────────────┐         ┌──────────────────┐  ║
║   │ Three       │ ──────► │ Each in own  │ ──────► │ Direction was    │  ║
║   │ explore     │         │ scoped CSS   │         │ chosen; trees    │  ║
║   │ trees       │         │ (--mg-*, etc)│         │ are dead weight  │  ║
║   └─────────────┘         └──────────────┘         └──────────────────┘  ║
║                                                                           ║
║   Theme preference                                                        ║
║   ┌─────────────┐         ┌──────────────┐         ┌──────────────────┐  ║
║   │ No DB       │ ──X──►  │ No backend   │ ──X──►  │ No frontend      │  ║
║   │ column      │         │ endpoint     │         │ provider/cookie  │  ║
║   └─────────────┘         └──────────────┘         └──────────────────┘  ║
║                                                                           ║
║   PAIN_POINTS:                                                            ║
║   - No canonical token system; brand values live as inline hex in Lockup  ║
║   - Three competing CSS trees with state-color tokens that brand bans     ║
║   - Theme is unaware → every Step 11 surface would re-derive its own      ║
║   - Fonts ship from cdn.fontshare.com; ORB cookie risk on the CDN CSS API ║
║                                                                           ║
╚═══════════════════════════════════════════════════════════════════════════╝
```

### After State

```
╔═══════════════════════════════════════════════════════════════════════════╗
║                               AFTER STATE                                  ║
╠═══════════════════════════════════════════════════════════════════════════╣
║                                                                           ║
║   Visit /  (FOUC script runs first; sets data-theme synchronously)        ║
║   ┌─────────────┐    ┌──────────────────┐    ┌──────────────────────┐    ║
║   │ Reads       │──► │ <html data-theme=│──► │ ThemeProvider        │    ║
║   │ reverie_    │    │ "dark"|"light">  │    │ reconciles w/ server │    ║
║   │ theme cookie│    │ before hydration │    │ optimistic + rollback│    ║
║   └─────────────┘    └──────────────────┘    └──────────────────────┘    ║
║                                                                           ║
║   Visit /design/system  (DEV only; structurally tree-shaken in prod)      ║
║   ┌─────────────┐    ┌──────────────────┐    ┌──────────────────────┐    ║
║   │ Theme       │──► │ Every primitive  │──► │ Every state in both  │    ║
║   │ switcher    │    │ in 'ui/'         │    │ themes; brand-bound  │    ║
║   └─────────────┘    └──────────────────┘    └──────────────────────┘    ║
║                                                                           ║
║   Change theme  (cross-tab sync via BroadcastChannel)                     ║
║   ┌─────────────┐    ┌──────────────────┐    ┌──────────────────────┐    ║
║   │ Click       │──► │ Optimistic write │──► │ PATCH /auth/me/theme │    ║
║   │ Light/Dark  │    │ to cookie + DOM  │    │ + broadcast to tabs  │    ║
║   └─────────────┘    └──────────────────┘    └──────────────────────┘    ║
║                                                                           ║
║   USER_FLOW: Login → cookie seeded from DB → reload → no flicker.         ║
║              Change theme → mirrors immediately, persists in DB.          ║
║   VALUE_ADD: Brand-coherent surfaces; theme that survives device cycles.  ║
║   DATA_FLOW: cookie ↔ FOUC; cookie ↔ provider; provider ↔ /auth/me ↔ DB.  ║
║                                                                           ║
╚═══════════════════════════════════════════════════════════════════════════╝
```

### Interaction Changes

| Location | Before | After | User Impact |
|---|---|---|---|
| `/` | Default Vite scaffold + `App.css` | Minimal canonical-canvas shell — brand surfaces visible immediately; Step 11 builds the library view on top | Identity visible at first paint |
| `/design/system` | Does not exist | Primitive gallery, dev-only | Visual contract for Step 11 |
| `/design/explore/*` | Three direction prototypes | 404 (routes deleted) | Dead weight removed |
| `<html data-theme>` | Absent | Set synchronously by FOUC before React hydrates | No theme flicker |
| Theme switcher | None | Top-of-page System / Light / Dark | Cross-device preference |
| Logout | (no theme cookie exists yet) | `reverie_theme` survives logout (device state) | Same theme on next login |

---

## Mandatory Reading

**Implementation agent MUST read these before starting any task.** Listed in
read order.

| Priority | File | Lines | Why Read This |
|---|---|---|---|
| P0 | [`unkos-dev/reverie-branding` `identity.md`](https://github.com/unkos-dev/reverie-branding/blob/main/identity.md) | all | Brand SoT — colour, typography table, mark/lockup spec, tagline, dont's |
| P0 | `plans/2026-04-25-design-system-philosophy-design.md` | §6, §8, §10, §11, §11A, §11B, §11C, §18 | Brand-aligned philosophy; state-without-hue mapping |
| P0 | `plans/2026-04-26-brand-reconciliation.md` | all | Per-delta D2→brand reconciliation with file-level impact |
| P0 | `.claude/PRPs/plans/design-system.plan.md` | 246–797 | All 9 backing patterns (USER_MODEL_COLUMN_ADDITION through SHADCN_COMPONENTS_JSON) |
| P0 | `.claude/PRPs/plans/design-system.plan.md` | 1130–1186 | D1 + D2 phases (already shipped) — context for what produced the explore tree |
| P1 | `frontend/src/components/Lockup.tsx` | 1–58 | Already-shipped brand component; `Satoshi Variable` font-family reference must match canonical naming after D3.7 |
| P1 | `frontend/index.html` | 1–35 | Brand head wired; `<!-- reverie:fouc-hash -->` marker is the FOUC injection point |
| P1 | `frontend/src/fouc/fouc.js` | all | Placeholder body that D3.13 replaces |
| P1 | `frontend/vite-plugins/csp-hash.ts` | all | Hashes the FOUC body and emits `dist/csp-hashes.json` |
| P1 | `backend/src/security/csp.rs` | 30, 80 | Production HTML CSP — `font-src` allowlist must drop `cdn.fontshare.com` |
| P1 | `backend/src/routes/auth.rs` | 68–177 | OIDC callback + `/auth/me` — D3.4/D3.5 wire here |
| P1 | `backend/src/models/user.rs` | 1–40 | `UserRow`/`User` struct — D3.3 four-edit pattern target |
| P2 | `backend/migrations/20260414000001_add_session_version.up.sql` | all | Migration template to mirror in D3.2 |
| P2 | `backend/migrations/20260414000001_add_session_version.down.sql` | all | Down template |
| P2 | `frontend/public/fonts/fontshare/README.md` | all | Currently describes CDN approach — D3.16 rewrites to self-host rationale |
| P2 | `frontend/src/design/explore/midnight-gold/tokens.css` | 1–123 | D2 tokens reference; explicit non-input for D3.7 (brand identity is the input) |
| P2 | `docs/security/content-security-policy.md` | all | Operator surface that D3.20 extends |

**External Documentation:**

| Source | Section | Why Needed |
|---|---|---|
| [Tailwind CSS v4 — `@theme` directive](https://tailwindcss.com/docs/theme) | Theme variables and `inline` mode | TAILWIND_V4_MULTI_THEME_BRAND pattern relies on `@theme inline` semantics |
| [Tailwind CSS v4 — `@custom-variant`](https://tailwindcss.com/docs/dark-mode) | Custom dark-mode selector | `dark:` modifier wiring to `[data-theme="dark"]` |
| [shadcn/ui — Combobox](https://ui.shadcn.com/docs/components/combobox) | Composition pattern | Confirms `combobox` is composed from `command` + `popover`, not standalone |
| [shadcn/ui — Configuration](https://ui.shadcn.com/docs/components-json) | `components.json` schema | SHADCN_COMPONENTS_JSON pattern; pre-write to skip prompts |
| [`axum-extra::extract::cookie`](https://docs.rs/axum-extra/latest/axum_extra/extract/cookie/index.html) | `CookieJar`, `Cookie::build`, `SameSite` | THEME_COOKIE_WRITER pattern relies on these |
| [`@axe-core/cli`](https://github.com/dequelabs/axe-core-npm/tree/develop/packages/cli) | `--exit` flag | D3.17 CI gating requires `--exit` (default exit-0 is silent) |
| [Fontshare per-font download endpoint](https://api.fontshare.com/v2/fonts/download/author) | (no static docs) | D3.16 fetches the variable woff2 zip; URL discovered via `frontend/public/fonts/fontshare/README.md` |
| [WCAG 2.2 — 1.4.3 Contrast (Minimum)](https://www.w3.org/WAI/WCAG22/Understanding/contrast-minimum.html) | AA criterion | D3.17 contrast pass standard |

---

## Patterns to Mirror

**One pattern is re-emitted because brand alignment changed token names and
values; the other eight are referenced by line in the parent plan because
they are brand-neutral.**

### Re-emitted: TAILWIND_V4_MULTI_THEME_BRAND (supersedes parent §608–656)

`@theme` declares token → utility mapping; `@custom-variant` teaches Tailwind
what `dark:` means; runtime swap happens via regular CSS variables keyed on
`[data-theme]`. **Token names and values come from the brand identity (§4 of
`identity.md`) and the philosophy spec §10 — not from the D2 explore tree.**

```css
/* frontend/src/styles/themes/index.css — imported from frontend/src/index.css */
@import "tailwindcss";

/* Tell Tailwind: "dark:" variant activates when [data-theme="dark"] is on
   an ancestor (or the element itself). Required because Tailwind v4's default
   dark-mode detection is media-query based; we need [data-theme] driven. */
@custom-variant dark (&:where([data-theme="dark"], [data-theme="dark"] *));

/* Tokens that generate utilities (bg-canvas, text-fg, border-border, etc.).
   `inline` keyword allows referencing runtime var(--…) values. */
@theme inline {
  --color-canvas:        var(--canvas);
  --color-canvas-2:      var(--canvas-2);
  --color-surface:       var(--surface);
  --color-surface-2:     var(--surface-2);
  --color-border:        var(--border);
  --color-border-strong: var(--border-strong);
  --color-fg:            var(--fg);
  --color-fg-muted:      var(--fg-muted);
  --color-fg-faint:      var(--fg-faint);
  --color-accent:        var(--accent);
  --color-accent-soft:   var(--accent-soft);
  --color-accent-strong: var(--accent-strong);
  --color-fg-on-accent:  var(--fg-on-accent);

  /* Typography — Satoshi for body/chrome, Author for display, JBM
     conditional (loaded; usage reviewed at UNK-113 post-0.1.0). */
  --font-display: "Author Variable", system-ui, -apple-system, "Segoe UI", sans-serif;
  --font-body:    "Satoshi Variable", system-ui, -apple-system, "Segoe UI", sans-serif;
  --font-mono:    "JetBrains Mono", ui-monospace, SFMono-Regular, Menlo, monospace;

  /* Radius / spacing scaffolding — sourced from D2 outcomes; brand-neutral.
     Spacing follows a 4px base; radius follows the boutique-discipline curve. */
  --radius-sm: 0.25rem;
  --radius-md: 0.5rem;
  --radius-lg: 0.75rem;
}

/* Default + explicit Light theme (Parchment).
   Runtime values live on regular selectors, NOT inside @theme
   (which can't be nested per Tailwind v4 docs). */
:root,
[data-theme="light"] {
  --canvas:        #E8DCC2; /* Parchment */
  --canvas-2:      #DFD2B4;
  --surface:       #F0E6CF;
  --surface-2:     #E5D8BC;
  --border:        #C7B894;
  --border-strong: #B0A07C;
  --fg:            #0E0D0A; /* Ink */
  --fg-muted:      #5A5244;
  --fg-faint:      #8A8170;
  --accent:        #8E6F38; /* Reverie Gold #C9A961 darkened for Light theme. Sourced from philosophy spec §10, not identity.md (which only locks #C9A961). Passes 1.4.11 (UI 3:1) + 1.4.3 large-text on Parchment; not normal-text 4.5:1 — restrict to focus rings, large CTAs, recovery actions. */
  --accent-soft:   #DCC890;
  --accent-strong: #6E5424;
  --fg-on-accent:  #E8DCC2; /* Parchment */
}

/* Dark theme — Ink-leaning canvas + warm surfaces + Reverie Gold accent. */
[data-theme="dark"] {
  --canvas:        #14120E;
  --canvas-2:      #1A1812;
  --surface:       #221F18;
  --surface-2:     #2A261D;
  --border:        #2E2A22;
  --border-strong: #3A3528;
  --fg:            #E8E0D0; /* Cream */
  --fg-muted:      #A8A090;
  --fg-faint:      #6E6858;
  --accent:        #C9A961; /* Reverie Gold */
  --accent-soft:   #4A3C24;
  --accent-strong: #D4B070;
  --fg-on-accent:  #0E0D0A; /* Ink */
}
```

**Three load-bearing rules:**

1. `@custom-variant dark (...)` — without it, `dark:bg-surface` utilities
   never fire on `[data-theme="dark"]`.
2. `@theme inline` (not plain `@theme`) — `inline` is what allows tokens to
   reference runtime `var(--…)` values.
3. Theme overrides live on regular selectors **outside** `@theme`. `@theme`
   itself cannot be nested under a selector per Tailwind v4 docs.

**Deliberately absent:** no `--color-success`, `--color-warning`,
`--color-danger`, `--color-info`. State communicates through typography
weight, surface opacity, motion, and the gold accent — see §11A of the
philosophy spec. State-color tokens were present in D2's `--mg-*` set;
brand handoff bans them.

**JetBrains Mono is loaded but conditional.** UNK-113 reviews actual usage
post-0.1.0 — if no surface adopts it by then, `--font-mono` declaration and
JBM `@font-face` are removed in a follow-up. Do not retroactively delete
the declaration in this PR; the conditional review happens later.

### Referenced from parent plan (unchanged, brand-neutral)

| Pattern | Parent plan lines | Purpose |
|---|---|---|
| `USER_MODEL_COLUMN_ADDITION` | 246–311 | Four-edit pattern for adding `theme_preference` to `users` |
| `PATCH_HANDLER_SHAPE` | 313–358 | Authenticated PATCH handler shape (extractor list, return type, validation) |
| `THEME_COOKIE_WRITER` | 359–421 | Backend `set_theme_cookie` + module doc + cross-stack const tracking |
| `THEME_COOKIE_FRONTEND_WRITER` | 423–501 | Frontend `readThemeCookie`/`writeThemeCookie` with attribute parity contract |
| `SQLX_TEST_HARNESS` | 503–605 | Migration + integration test scaffolding for D3.5 |
| `FOUC_INLINE_SCRIPT` | 663–696 | The `fouc.js` body content; ES5, self-invoking, try/catch fallback |
| `THEME_PROVIDER` | 698–767 | React provider with cookie/dataset/server reconciliation + BroadcastChannel |
| `SHADCN_COMPONENTS_JSON` | 769–797 | Pre-written `components.json` for zero-prompt `shadcn init` |

---

## Files to Change

> [!NOTE]
> The full Files to Change table for the design-system plan lives at
> `.claude/PRPs/plans/design-system.plan.md` lines ~835–950 (D0–D2 entries
> already shipped). The table below lists only files D3 introduces, modifies,
> or deletes — it is a **delta** on top of the post-PR-#51 working tree.

### Backend

| File | Action | Justification |
|---|---|---|
| `backend/migrations/<TS>_add_theme_preference.up.sql` | CREATE | Add `theme_preference TEXT NOT NULL DEFAULT 'system'` to `users` |
| `backend/migrations/<TS>_add_theme_preference.down.sql` | CREATE | Drop the column |
| `backend/src/models/user.rs` | UPDATE | Four-edit per `USER_MODEL_COLUMN_ADDITION` |
| `backend/src/auth/theme_cookie.rs` | CREATE | `THEME_COOKIE_NAME` + `set_theme_cookie` + module doc |
| `backend/src/auth/mod.rs` | UPDATE | Re-export `theme_cookie` module |
| `backend/src/routes/auth.rs` | UPDATE | `/auth/me` includes `theme_preference`; `update_theme` handler; OIDC callback writes cookie |
| `backend/src/security/csp.rs` | UPDATE | Drop `https://cdn.fontshare.com` from `font-src` (lines 30, 80); update unit tests asserting CSP string |

### Frontend — pruning

| File | Action | Justification |
|---|---|---|
| `frontend/src/design/explore/atelier-ink/` | DELETE (recursive) | Direction eliminated at D5 |
| `frontend/src/design/explore/midnight-gold/` | DELETE (recursive) | D2 explore tree; values come from brand, not the tree |
| `frontend/src/design/explore/signal/` | DELETE (recursive) | Direction eliminated at D5 |
| `frontend/src/pages/design/explore/` | DELETE (recursive) | All three explore page mocks + `_shared/` |
| `frontend/src/main.tsx` | UPDATE | Remove explore-tree imports + routes; mount `<ThemeProvider>` |
| `frontend/src/App.tsx` | UPDATE | Replace Vite hero scaffold with minimal canonical-canvas shell |
| `frontend/src/App.css` | DELETE | Vite default scaffold styles; superseded by token-bound utilities |
| `frontend/src/assets/` | DELETE | Vite default React/Vite logos (`react.svg` etc); orphaned by App.tsx rewrite |

### Frontend — canonical theme + tokens + fonts

| File | Action | Justification |
|---|---|---|
| `frontend/src/index.css` | UPDATE | Import `./styles/themes/index.css` (replaces bare `@import "tailwindcss"`) |
| `frontend/src/styles/themes/index.css` | CREATE | TAILWIND_V4_MULTI_THEME_BRAND — `@import "tailwindcss"`, `@custom-variant`, `@theme inline`, palette overrides |
| `frontend/src/styles/themes/dark.css` | CREATE (optional split file) | Dark palette override (imported from index) |
| `frontend/src/styles/themes/light.css` | CREATE (optional split file) | Light palette override (imported from index) |
| `frontend/src/styles/fonts.css` | CREATE | Self-hosted `@font-face` blocks for Author, Satoshi, JetBrains Mono |
| `frontend/public/fonts/fontshare/files/Author-Variable.woff2` | CREATE (binary) | Variable axis, 400–700 normal |
| `frontend/public/fonts/fontshare/files/Author-VariableItalic.woff2` | CREATE (binary) | Variable axis, 400–700 italic |
| `frontend/public/fonts/fontshare/files/Satoshi-Variable.woff2` | CREATE (binary) | Variable axis, 400–700 normal |
| `frontend/public/fonts/fontshare/files/Satoshi-VariableItalic.woff2` | CREATE (binary) | Variable axis, 400–700 italic |
| `frontend/public/fonts/fontshare/files/JetBrainsMono-Regular.woff2` | CREATE (binary) | 400 weight, conditional (UNK-113) |
| `frontend/public/fonts/fontshare/files/SHA256SUMS` | CREATE | Verifiable checksums (woff2 are large binaries; integrity check) |
| `frontend/public/fonts/fontshare/README.md` | REWRITE | Currently describes CDN approach (stale); rewrite with self-host rationale + ORB context + FFL acceptance |
| `frontend/vite.config.ts` | UPDATE | Drop `https://cdn.fontshare.com` from `DEV_CSP` `font-src`; add `manualChunks` for design tree |

### Frontend — shadcn primitives + theme provider

| File | Action | Justification |
|---|---|---|
| `frontend/components.json` | CREATE | Pre-written per SHADCN_COMPONENTS_JSON; zero-prompt init |
| `frontend/tsconfig.app.json` | UPDATE | Add `baseUrl: "."` and `paths: { "@/*": ["src/*"] }` |
| `frontend/src/lib/utils.ts` | CREATE (via `shadcn init`) | `cn` helper |
| `frontend/src/components/ui/*.tsx` | CREATE (via `shadcn add`) | Restyled against tokens (D3.9) |
| `frontend/src/components/Lockup.tsx` | UPDATE (1 line) | `font-family` references must match canonical `Satoshi Variable` after D3.7; reading the spec: it already says `"Satoshi Variable", "Satoshi", system-ui, sans-serif` so this is verification only |
| `frontend/src/components/theme-switcher.tsx` | CREATE | DropdownMenu with System/Light/Dark |
| `frontend/src/lib/theme/ThemeProvider.tsx` | CREATE | Per THEME_PROVIDER pattern |
| `frontend/src/lib/theme/cookie.ts` | CREATE | Per THEME_COOKIE_FRONTEND_WRITER pattern |
| `frontend/src/lib/theme/api.ts` | CREATE | `fetchMe` + `patchTheme` thin client |

### Frontend — gallery + dev gating

| File | Action | Justification |
|---|---|---|
| `frontend/src/pages/design/system.tsx` | CREATE | Primitive gallery |
| `frontend/src/routes/design.tsx` | CREATE | Dev-only `designRoutes` array |
| `frontend/src/fouc/fouc.js` | UPDATE | Replace placeholder with FOUC_INLINE_SCRIPT body |

### Frontend — tooling

| File | Action | Justification |
|---|---|---|
| `frontend/eslint.config.js` | UPDATE | `no-restricted-syntax` hex-ban rule |
| `frontend/.stylelintrc.json` | CREATE | `color-no-hex` outside theme files; Tailwind v4 at-rules ignore |
| `frontend/src/__tests__/hex-ban.test.ts` | CREATE | In-process `RuleTester` validation |

### Tests

| File | Action | Justification |
|---|---|---|
| `backend/tests/auth_theme.rs` (or extension per `SQLX_TEST_HARNESS` parent §503) | CREATE | `set_theme_cookie` unit; `/auth/me/theme` PATCH happy + invalid; `/auth/me` returns field — follow `SQLX_TEST_HARNESS` for harness shape and helper signatures |
| `frontend/src/lib/theme/cookie.test.ts` | CREATE | Round-trip + attribute string assertions |
| `frontend/src/lib/theme/ThemeProvider.test.tsx` | CREATE | Initial-state matrix; reconciliation; rollback; system-pref change; cross-tab |
| `frontend/vite-plugins/__tests__/csp-hash.test.ts` | UPDATE if needed | Hash regenerates after `fouc.js` body change |

### Docs

| File | Action | Justification |
|---|---|---|
| `docs/src/content/docs/design/visual-identity.md` | CREATE | Tokens, type scale, spacing, motion, state philosophy, theme architecture, theme cookie lifecycle |
| `docs/src/content/docs/design/philosophy.md` | CREATE | Authoritative philosophy doc — folds in `plans/2026-04-25-design-system-philosophy-design.md` content |
| `docs/astro.config.mjs` | UPDATE | Sidebar entry: Design → Philosophy + Visual Identity |
| `docs/security/content-security-policy.md` | UPDATE | `## Cookies` section; `Fonts` subsection; drop `cdn.fontshare.com` row |

### Plan housekeeping

| File | Action | Justification |
|---|---|---|
| `.claude/PRPs/plans/design-system.plan.md` | UPDATE (1 hunk at line 1187) | Insert forwarding note: "D3 superseded by `design-system-d3.plan.md` (2026-04-26 brand alignment)" |

---

## NOT Building (Scope Limits)

Explicit exclusions to prevent scope creep:

- **Library grid, book detail page, search UI, hero screens** — Step 11 builds
  these against real `/api/books`. The primitive gallery at `/design/system`
  is the visual contract D3 ships; applied pages are not.
- **Real `/api/books` integration** — primitives render against the dev
  gallery's hand-written examples; book ingestion / surfacing is Step 11.
- **First-install onboarding flow** — multi-step welcome, ingestion-folder
  setup, theme picker. Step 11 scope (philosophy spec §16).
- **Reader chrome / reading-comfort token scale** — token scaffolding may
  land in D3 if needed by gallery primitives, but the reader view itself
  and its user-configurable typography overrides are Step 11.
- **Smart-shelf filter-preset UI** — Step 11 (philosophy §16).
- **Device-sync UI** — Step 11 (philosophy §16).
- **OIDC e2e test (`Set-Cookie: reverie_theme` on callback)** — UNK-104;
  blocked on `wiremock` + signed-ID-token scaffolding that doesn't exist yet.
- **JetBrains Mono usage decision** — UNK-113, post-0.1.0. JBM is loaded by
  D3 but its retention/removal is not adjudicated here.
- **Removing the `/design/system` route in production builds via runtime
  env-flag** — D3 uses a structural `manualChunks` gate; runtime gating is
  unnecessary because the chunk simply isn't emitted.
- **Gold-on-accent hue in charts/code blocks** — philosophy §11A names these
  as scoped exceptions, but charts and code blocks are not in the gallery.
  Scope deferred to whichever Step actually adds them.

---

## Step-by-Step Tasks

Execute in order. Each task is atomic and independently verifiable. Refresh
notes call out where this plan diverges from the parent's D3 section.

### Task D3.0 — Insert forwarding note in parent plan (FIRST action)

- **STATUS:** **Already complete as of this plan's creation (2026-04-26).**
  Verify presence then mark done; do not re-insert.
- **ACTION (verification):**
  ```bash
  rg -n "D3 superseded" .claude/PRPs/plans/design-system.plan.md
  ```
  Expect a hit at line ~1189.
- **RATIONALE:** Parent plan stays a coherent historical artefact; nobody
  accidentally executes the stale section that still references `--mg-*`,
  state-color tokens, and `@fontsource`.
- **VALIDATE:** The `> [!NOTE]` block appears between the
  `### Phase D3 — Codify Design System` heading and the
  `**Skills:**` line.

### Task D3.1 — Prune all three D2 exploration trees (refresh)

- **REFRESH:** Original D3.1 said "delete all three direction pages and keep
  only the winning direction's tokens as seed." Brand alignment makes the
  tokens come from `identity.md` directly, so **all three trees go**, and no
  seeding step exists. The midnight-gold token file is reference, not input.
- **ACTION:** Delete recursively:
  - `frontend/src/design/explore/atelier-ink/`
  - `frontend/src/design/explore/midnight-gold/`
  - `frontend/src/design/explore/signal/`
  - `frontend/src/pages/design/explore/` (entire directory including `_shared/`)
- **ACTION:** Edit `frontend/src/main.tsx` — remove imports of `ExploreIndex`,
  `MidnightGold`, `Signal`, `AtelierInk`, and the four `/design/explore/...`
  router entries. Leave only the root `/` route until D3.10–D3.12 add the
  design route gate and ThemeProvider mounting.
- **RATIONALE:** Working on top of stale trees muddies every D3 review.
  Brand identity, not the trees, is the input source.
- **VALIDATE:** `rg "design/explore" frontend/src/ frontend/index.html`
  returns nothing.
- **VALIDATE:** `npm run build` and `npm run lint` succeed.

### Task D3.2 — Create the theme-preference migration

- **ACTION:** Generate the migration timestamp at write-time:
  `STAMP=$(date -u +%Y%m%d000001)`. Write
  `backend/migrations/${STAMP}_add_theme_preference.up.sql`:
  ```sql
  ALTER TABLE users ADD COLUMN theme_preference TEXT NOT NULL DEFAULT 'system';
  ```
- **ACTION:** Write `${STAMP}_add_theme_preference.down.sql`:
  ```sql
  ALTER TABLE users DROP COLUMN theme_preference;
  ```
- **MIRROR:** `backend/migrations/20260414000001_add_session_version.{up,down}.sql`
  verbatim — same shape, single-line ALTER.
- **VALIDATE:** `cd backend && sqlx migrate run` succeeds against a fresh DB.
- **VALIDATE:** `sqlx migrate revert` cleanly reverts.
- **GOTCHA:** Schema is freely mutable pre-release (per memory
  `project_schema_evolution.md`); do not gate this behind a feature flag.

### Task D3.3 — Extend the user model

- **ACTION:** Apply the four-edit `USER_MODEL_COLUMN_ADDITION` pattern (parent
  plan §246) to `backend/src/models/user.rs`:
  1. Append `theme_preference` to the `USER_COLUMNS` constant.
  2. Add `theme_preference: String` field to `UserRow`.
  3. Add `pub theme_preference: String` field to `User`.
  4. Add `theme_preference: row.theme_preference,` to the
     `From<UserRow> for User` impl.
- **ACTION:** Add `const ALLOWED_THEMES: &[&str] = &["system", "light", "dark"];`
  in `backend/src/auth/theme_cookie.rs` (created in D3.5) so it sits next to
  `THEME_COOKIE_NAME`. Do **not** define it inline in `routes/auth.rs` to
  avoid duplication.
- **VALIDATE:** `cargo build -p reverie-api` succeeds.
- **VALIDATE:** Existing user tests pass (`cd backend && cargo test -p reverie-api models::user`).
- **GOTCHA:** Missing any of the four edits produces a clear compile error;
  treat that as the test for completeness.

### Task D3.4 — Update `/auth/me` response

- **ACTION:** In `backend/src/routes/auth.rs:162–177` (the `me` handler's
  `Json` body), add `"theme_preference": u.theme_preference`.
- **VALIDATE:** Add `#[sqlx::test]` integration test asserting GET `/auth/me`
  returns 200 with the field present and default `"system"`.
- **VALIDATE:** Existing `/auth/me` tests pass.

### Task D3.5 — Implement `PATCH /auth/me/theme`

- **ACTION:** Create `backend/src/auth/theme_cookie.rs` per the
  `THEME_COOKIE_WRITER` pattern (parent plan §359–421). Module-level `//!`
  doc must include the lifecycle line:
  ```rust
  //! Lifecycle: survives logout by design. See docs/design/visual-identity.md
  //! § Theme Cookie Lifecycle for rationale and the contrast rule for
  //! session-state cookies.
  ```
- **ACTION:** Append `update_theme` handler to
  `backend/src/routes/auth.rs` per the `PATCH_HANDLER_SHAPE` pattern (parent
  plan §313–358). Signature takes `jar: CookieJar`, returns
  `(CookieJar, Json<_>)`, validates input against `ALLOWED_THEMES`, returns
  `AppError::Validation` (HTTP 422) on rejection — **not** `BadRequest`.
- **ACTION:** Register route in `routes::auth::router()`:
  `.route("/auth/me/theme", patch(update_theme))`.
- **ACTION:** Update OIDC `callback` handler
  (`backend/src/routes/auth.rs:68–152`):
  - Add `jar: CookieJar` to the extractor list.
  - Change return type from `impl IntoResponse` to `(CookieJar, Redirect)`.
  - After `auth_session.login(&user)` succeeds, call:
    ```rust
    let jar = set_theme_cookie(jar, &user.theme_preference);
    ```
  - Final return becomes `Ok((jar, Redirect::temporary("/")))`.
- **ACTION:** Re-export the new module in `backend/src/auth/mod.rs`.
- **TESTS (TDD — write FIRST per CLAUDE.md hard rule §5):**
  - Unit test for `set_theme_cookie`: given a fresh `CookieJar` and value
    `"dark"`, assert returned jar contains a cookie with **string-compared**
    name `"reverie_theme"` (so renaming the const fails the test —
    enforces UNK-105 cross-stack drift), `http_only = false`,
    `same_site = Lax`, `path = "/"`, `max_age = Duration::days(365)`.
  - Integration tests per `SQLX_TEST_HARNESS` (parent §503):
    - **Happy path:** PATCH `{"theme_preference":"dark"}` returns 200; row
      updated; response includes `Set-Cookie: reverie_theme=dark`.
    - **Rejection:** PATCH `{"theme_preference":"purple"}` returns **422**;
      no row modified; no `Set-Cookie` header.
- **NOTE:** OIDC callback success-path e2e test ("Set-Cookie includes
  reverie_theme on callback") tracks separately under
  [UNK-104](https://linear.app/unkos/issue/UNK-104) — requires `wiremock` +
  signed-ID-token scaffolding not yet present. **Do not bundle that work.**
- **VALIDATE:** `cd backend && cargo test -p reverie-api auth::theme_cookie auth::routes`
  green.
- **VALIDATE:** `cargo clippy -p reverie-api --all-targets -- -D warnings` clean.

### Task D3.6 — Init shadcn/ui via CLI (zero-prompt)

- **ACTION (path aliases — prerequisite):**
  - `frontend/tsconfig.app.json`: add `"baseUrl": "."` and
    `"paths": { "@/*": ["src/*"] }` to `compilerOptions`.
  - `frontend/vite.config.ts`: add
    `resolve: { alias: { "@": path.resolve(__dirname, "src") } }`
    (import `path` from `"node:path"`).
- **ACTION (pre-write `components.json`):** Write `frontend/components.json`
  per `SHADCN_COMPONENTS_JSON` pattern (parent §769) BEFORE running init.
  Pre-writing makes init zero-prompt.
- **ACTION:** `cd frontend && npx shadcn@latest init --yes` — picks up the
  pre-written config. Generates `src/lib/utils.ts` (`cn` helper) and updates
  `src/index.css`.
- **GOTCHA (Feb 2026 unified package):** Current shadcn CLI generates
  components importing from unified `radix-ui` package, not individual
  `@radix-ui/react-*` modules. One big `radix-ui` dep in `package.json`
  is correct.
- **GOTCHA (`init` overwrites `index.css`):** D3.7 must run AFTER `init`;
  the init step replaces `index.css` content, then D3.7 replaces it with
  the canonical theme tree.
- **FALLBACK:** If `--yes` does not skip all prompts in the installed CLI
  version, run `npx shadcn@latest init --help` first to capture the current
  non-interactive flag set; adjust accordingly. Do **not** run interactive
  init (blocks on stdin in CI/agent contexts).
- **VALIDATE:** `npm run build` succeeds; `npm run lint` passes.
- **VALIDATE:** `@/components/...` and `@/lib/utils` imports resolve in a
  test file.

### Task D3.7 — Commit canonical theme tree (refresh — token rename + brand values + drop state-color)

- **REFRESH:** Original D3.7 sourced tokens from "the winning D2 direction
  (Dark + Light from tweakcn)" with `--mg-*` placeholders. Brand alignment
  reverses both inputs:
  1. Token names become `--color-*` (and the underlying runtime vars
     `--canvas`, `--surface`, `--fg`, `--accent`, etc.).
  2. Values come from `identity.md` §4 + philosophy spec §10 directly.
  3. State-color tokens (`--success`, `--warning`, `--danger`, `--info`,
     `--neutral`) are **dropped** — see philosophy §11A.
  4. JetBrains Mono added (conditional, UNK-113).
  5. `@font-face` blocks live in `styles/fonts.css` (D3.16) referencing
     self-hosted woff2, not the Fontshare CDN.
- **ACTION:** Replace `frontend/src/index.css` with:
  ```css
  @import "./styles/themes/index.css";
  ```
  The `@import "./styles/fonts.css";` line is added in **D3.16** alongside
  the `fonts.css` file itself — keeping the import paired with the file
  creation preserves D3.7's atomicity (`npm run build` must succeed at the
  end of this task). Between D3.7 and D3.16, font-family declarations fall
  through to `system-ui`; this is the intentional intermediate state.
- **ACTION:** Create `frontend/src/styles/themes/index.css` per the
  re-emitted **TAILWIND_V4_MULTI_THEME_BRAND** pattern in this plan's
  Patterns to Mirror section. Include:
  - `@import "tailwindcss";`
  - `@custom-variant dark (...)`
  - `@theme inline { … }` mapping `--color-*` to runtime vars
  - `:root, [data-theme="light"] { … }` Parchment palette (full
    13-token set from philosophy §10)
  - `[data-theme="dark"] { … }` Ink-leaning palette
- **ACTION:** Optionally split palettes into `themes/dark.css` +
  `themes/light.css` and `@import` from `themes/index.css`. Allowed; not
  required.
- **VALIDATE:** `npm run build` succeeds; output contains
  `[data-theme="dark"]` and `[data-theme="light"]` blocks.
- **VALIDATE (after D3.11 lands):** `/design/system` shows visible theme
  swap when `data-theme` flips on `<html>`.
- **GOTCHA:** Stylelint `color-no-hex` (D3.14) must exempt the theme files —
  hex literals legitimately live there. The `.stylelintrc.json` glob
  `!src/styles/themes/**/*.css` covers this.

### Task D3.8 — Add shadcn primitives

- **ACTION:** Install Step 11 primitive set non-interactively. `combobox` is
  composed (`command` + `popover` + `cmdk`), not standalone:
  ```bash
  npx shadcn@latest add --yes \
    button input label select command popover \
    radio-group checkbox switch card dialog alert-dialog sheet table tabs \
    sonner tooltip dropdown-menu form avatar badge separator skeleton \
    scroll-area
  ```
  Notes: `sonner` is the Toast primitive in current shadcn;
  `command` + `popover` compose into Combobox per
  [shadcn Combobox docs](https://ui.shadcn.com/docs/components/combobox).
- **FALLBACK:** If `--yes` does not auto-accept peer-dep prompts
  (`react-hook-form`, `zod`, `@hookform/resolvers` for Form; `cmdk` for
  command), run `npm install react-hook-form zod @hookform/resolvers cmdk`
  first then re-run `add --yes`.
- **VALIDATE:** All files appear under `frontend/src/components/ui/`.
- **VALIDATE:** `npm run build` succeeds.

### Task D3.9 — Restyle every primitive against the token system (refresh — token names)

- **REFRESH:** Original D3.9 referenced `bg-white → bg-surface` etc. against
  `--mg-*`. With `--color-*` namespace, utilities become `bg-canvas`,
  `bg-surface`, `text-fg`, `text-fg-muted`, `border-border`,
  `border-border-strong`, `text-accent`, `bg-accent-soft`, `text-fg-on-accent`.
  Tailwind v4 generates these from the `@theme inline` declarations in D3.7.
- **ACTION:** For every `frontend/src/components/ui/*.tsx`, replace
  default spacing/radius/colour utility classes with token-bound equivalents.
  Examples:
  - `bg-white` / `bg-background` → `bg-canvas`
  - `bg-card` → `bg-surface`
  - `text-foreground` → `text-fg`
  - `text-muted-foreground` → `text-fg-muted`
  - `border` → `border-border`
  - `rounded-md` → `rounded-md` (token-backed via `@theme inline`'s
    `--radius-md`)
  - `bg-primary text-primary-foreground` → `bg-accent text-fg-on-accent`
- **ACTION:** Where ≥3 primitives share a class string group, extract into a
  `cva` composition. shadcn already uses `cva` under the hood — extend, do
  not parallel.
- **GOTCHA:** No state-color utilities. Disabled state expresses through
  `opacity-50 text-fg-faint`; error state expresses through `font-semibold`
  + gold recovery action; loading expresses through opacity pulse on the
  region. Do not introduce `text-destructive` or similar.
- **VALIDATE:** `/design/system` (D3.11) renders every primitive with no
  hardcoded hex (Stylelint + ESLint hex-ban from D3.14 enforce).
- **VALIDATE:** `npm run lint && npx stylelint 'src/**/*.css' --max-warnings 0`
  exits 0.

### Task D3.10 — Theme provider + switcher + API client + canvas shell

**Order within this task:** (1) replace `App.tsx` shell first so the
canvas exists when the provider wraps it; (2) create cookie/api/provider
modules; (3) create switcher; (4) mount provider in `main.tsx`; (5)
write tests last (or first, per TDD — pick one and stick with it).

- **ACTION (canonical-canvas shell — do FIRST):** Replace
  `frontend/src/App.tsx` with a minimal shell so the brand canvas is
  visible at `/`:
  ```tsx
  import type { ReactElement } from "react";

  function App(): ReactElement {
    return (
      <main className="bg-canvas text-fg min-h-screen">
        {/* Step 11 builds the library view here. */}
      </main>
    );
  }

  export default App;
  ```
  Delete `frontend/src/App.css` (Vite scaffold styles, no longer
  referenced). Delete `frontend/src/assets/` (Vite-default logos
  orphaned by the App.tsx rewrite).
  **Why first:** the provider mounts above this shell; if the shell
  doesn't exist when `<ThemeProvider>` wraps `<RouterProvider>`, the
  canvas tokens at `/` paint against browser-default white background
  while `data-theme="dark"` is set on `<html>` — visually wrong even
  though tokens are correct.
- **GOTCHA:** The shell must be `min-h-screen` so canvas paints below
  the fold even when no content occupies it; `bg-canvas text-fg`
  exercises the canonical `--color-*` tokens at `/`.
- **ACTION:** Create the three theme-lib files per the `THEME_PROVIDER`
  (parent §698) and `THEME_COOKIE_FRONTEND_WRITER` (parent §423) patterns:
  - `frontend/src/lib/theme/cookie.ts` — `THEME_COOKIE_NAME`,
    `readThemeCookie`, `writeThemeCookie`. **Attribute parity** with
    `backend/src/auth/theme_cookie.rs::set_theme_cookie` is mandatory:
    `Path=/`, `Max-Age=31536000`, `SameSite=Lax`, no `HttpOnly`, no `Secure`.
  - `frontend/src/lib/theme/api.ts` — `fetchMe`, `patchTheme`.
  - `frontend/src/lib/theme/ThemeProvider.tsx` — sources `preference` from
    `readThemeCookie()` (NOT `dataset.theme` — see THEME_PROVIDER's
    `deriveInitialState` for why); sources `effective` from `dataset.theme`
    with matchMedia fallback; reconciles with `/auth/me` on mount;
    optimistic update with rollback on PATCH failure;
    `BroadcastChannel('reverie-theme')` for cross-tab sync.
- **ACTION:** Create `frontend/src/components/theme-switcher.tsx` —
  `DropdownMenu` primitive with System / Light / Dark options.
- **ACTION:** Mount `<ThemeProvider>` in `frontend/src/main.tsx`, wrapping
  `<RouterProvider>`.
- **TESTS (TDD — write FIRST):**
  - `cookie.test.ts`:
    - Round-trip parse/write.
    - Malformed cookie → null.
    - Attribute string assertions: `Path=/`, `Max-Age=31536000`,
      `SameSite=Lax`, NOT `HttpOnly`, NOT `Secure`.
  - `ThemeProvider.test.tsx` — initial-state derivation matrix:
    - (a) cookie=`system` + `dataset.theme=dark` →
      `preference='system', effective='dark'`.
    - (b) cookie=`light` + `dataset.theme=light` → both `light`.
    - (c) missing cookie + `dataset.theme=dark` →
      `preference='system', effective='dark'`.
    - (d) logged-out visitor (401 from `/auth/me`) → stays on
      cookie-derived preference, no reconciliation.
    - Reconciliation with server when cookie differs.
    - Optimistic update + rollback on PATCH failure.
    - `system` preference reacts to `prefers-color-scheme` media query
      change mid-session.
    - BroadcastChannel message from another tab updates state without
      triggering a PATCH.
- **VALIDATE:** `npm test` all green.
- **VALIDATE (manual, after D3.11 lands):** Navigate to `/design/system`,
  cycle theme switcher; open two tabs, change theme in one, the other
  updates without reload.

### Task D3.11 — Component gallery at `/design/system`

- **ACTION:** Create `frontend/src/pages/design/system.tsx`. For every
  primitive in `components/ui/`, render it in every state (default, hover,
  focus, active, disabled, error, loading) in both themes (theme-switcher
  at top of page).
- **ACTION:** Wire route via dynamic-import pattern (D3.12).
- **VALIDATE:** `npm run dev`, navigate to `/design/system`, manually toggle
  theme, every primitive renders correctly in both.
- **GOTCHA:** "Error" state means the typography-weight + gold-CTA pattern
  from §11A — do not mock a `text-destructive` class. The gallery's job
  includes validating that the no-hue rule actually carries.

### Task D3.12 — Dev-only route tree + dynamic gating + structural bundle gate

- **ACTION:** Create `frontend/src/routes/design.tsx` exporting
  `designRoutes` (array of `RouteObject`).
- **ACTION:** In `main.tsx` gate via:
  ```typescript
  const routes: RouteObject[] = [...prodRoutes];
  if (import.meta.env.DEV) {
    const { designRoutes } = await import('./routes/design');
    routes.push(...designRoutes);
  }
  ```
- **ACTION (structural bundle gate):** In `frontend/vite.config.ts`,
  configure `build.rollupOptions.output.manualChunks` to route design-tree
  modules into a `design` chunk:
  ```typescript
  build: {
    rollupOptions: {
      output: {
        manualChunks(id) {
          if (id.includes('/src/routes/design/') ||
              id.includes('/src/pages/design/')) {
            return 'design';
          }
        },
      },
    },
  },
  ```
  In production, Vite tree-shakes the entire `design-*` chunk because
  `import.meta.env.DEV` is replaced with literal `false`; the if-branch is
  dead code. No `design-*.js` is emitted.
- **VALIDATE (structural):** `npm run build && test -z "$(ls frontend/dist/assets/design-*.js 2>/dev/null)"`
  exits 0. Substring grep against minified output is unreliable (Vite
  mangles names); this check is structural.

### Task D3.13 — Replace placeholder contents of `frontend/src/fouc/fouc.js`

- **ACTION:** Replace the current 5-line placeholder with the
  `FOUC_INLINE_SCRIPT` body (parent §663–696). JS only — no surrounding
  `<script>` tags; the Vite plugin wraps it. Do **not** touch the
  `<!-- reverie:fouc-hash -->` marker or its location in `index.html`.
- **CONSTRAINT:** Script body must not contain the literal `</script>`
  (case-insensitive). `vite-plugins/csp-hash.ts` throws at build time if
  present (a raw `</script>` would escape the element and render as HTML).
- **VALIDATE (build regen):** `npm run build` succeeds; `dist/csp-hashes.json`
  contains a single `sha256-...` entry whose value matches:
  ```bash
  openssl dgst -sha256 -binary frontend/src/fouc/fouc.js | base64
  ```
  Backend's `dist_validation.rs` reads this on next start.
- **VALIDATE (happy path):** Set `reverie_theme=dark` cookie; hard-reload;
  devtools confirms `<html data-theme="dark">` is set before any React
  mount event.
- **VALIDATE (catch-block path):** Set `reverie_theme=` with a control
  character or `reverie_theme=javascript:alert(1)`; hard-reload; confirm
  `<html data-theme="light">` (the try/catch fallback). JS-disabled is out
  of scope — the entire app is React; unstyled no-JS rendering is not a
  supported configuration.

### Task D3.14 — ESLint + Stylelint hex bans

- **ACTION:** Edit `frontend/eslint.config.js` — add to the existing
  `files: ['**/*.{ts,tsx}']` block:
  ```javascript
  rules: {
    'no-restricted-syntax': ['error', {
      selector: "Literal[value=/^#[0-9a-fA-F]{3,8}$/]",
      message: 'No raw hex codes in .tsx. Use semantic tokens (bg-canvas, text-fg, etc.).',
    }],
  },
  ```
- **ACTION (Lockup exemption):** `frontend/src/components/Lockup.tsx` is
  the canonical brand-identifier component and intentionally inlines
  `#C9A961` / `#0E0D0A` / `#E8E0D0` as constants — see philosophy spec
  §11C: "Inline styles to make the component self-contained without
  depending on token CSS being loaded." This is a load-bearing
  invariant (the Lockup must render correctly even before
  `themes/index.css` resolves, e.g. on the OIDC error page). Add an
  `overrides` block to `eslint.config.js` exempting the file:
  ```javascript
  // After the main config block:
  {
    files: ['src/components/Lockup.tsx'],
    rules: {
      'no-restricted-syntax': 'off', // Brand constants by design — see philosophy §11C
    },
  }
  ```
  Do **not** introduce per-line `eslint-disable-next-line` directives;
  the `overrides` block is the documented exemption mechanism and keeps
  the rationale visible at the config level.
- **ACTION (Stylelint):** `npm install -D stylelint stylelint-config-standard`
  if not already present (stylelint already at v17.9.0 per package.json;
  add `stylelint-config-standard` if missing). Do **not** install any
  third-party Tailwind-aware Stylelint config — false positives on Tailwind
  v4 at-rules are resolved via the built-in `at-rule-no-unknown` ignore list.
- **ACTION:** Create `frontend/.stylelintrc.json`:
  ```json
  {
    "extends": ["stylelint-config-standard"],
    "rules": {
      "at-rule-no-unknown": [true, {
        "ignoreAtRules": [
          "theme", "custom-variant", "layer", "utility",
          "apply", "config", "tailwind", "source", "variant"
        ]
      }]
    },
    "overrides": [
      {
        "files": ["src/**/*.css", "!src/styles/themes/**/*.css", "!src/styles/fonts.css"],
        "rules": { "color-no-hex": true }
      }
    ]
  }
  ```
  Negated globs exempt theme files (canonical hex live there) and
  `fonts.css` (no hex but harmless to exempt; future inline `format()`
  hash references shouldn't trip it).
- **ACTION (rule-correctness test, in-process):** Test the hex-ban via
  ESLint's `RuleTester` — no subprocess spawn, no fixture files:
  ```typescript
  // frontend/src/__tests__/hex-ban.test.ts
  import { RuleTester } from 'eslint';
  // import the config rule set or re-create the rule shape inline
  const tester = new RuleTester({ languageOptions: { ecmaVersion: 2022, sourceType: 'module' } });
  tester.run('hex-ban', rule, {
    valid: [
      { code: 'const c = "hello";' },
      { code: 'const c = bgSurface;' },
    ],
    invalid: [
      { code: 'const c = "#abc123";', errors: 1 },
      { code: 'const c = "#fff";', errors: 1 },
    ],
  });
  ```
  In-process, millisecond runtime, deterministic.
- **ACTION:** Tighten CI (D0.9): remove `|| true` on the stylelint step;
  add `npx eslint src --max-warnings 0` if not already covered by
  `npm run lint`.
- **VALIDATE:** `npx stylelint 'src/**/*.css' --max-warnings 0` and
  `npm run lint` both exit 0.
- **VALIDATE:** Deliberately introduce `"#abc123"` literal in a non-theme
  `.tsx` file → both fail; revert. Hex in a `themes/*.css` file → no
  failure.
- **VALIDATE:** `npm test` runs the `RuleTester`-based hex-ban test in
  under 100ms.

### Task D3.15 — Motion tokens + state philosophy (refresh — no ambient layer + no hue-coded states)

- **REFRESH:** Original D3.15 wanted "motion + state tokens" with implicit
  state-color sibling tokens. Philosophy spec §8 drops the always-running
  ambient layer; §11A drops hue-coded states entirely. New scope: motion
  tokens only, plus a documented state-without-hue mapping that
  `visual-identity.md` (D3.18) embeds.
- **ACTION:** Extend `@theme inline` in `frontend/src/styles/themes/index.css`
  with motion tokens:
  ```css
  --duration-fast:   180ms;
  --duration-base:   240ms;
  --duration-slow:   320ms;
  --duration-theme:  300ms;
  --ease-standard:   cubic-bezier(0.22, 0.61, 0.36, 1);
  --ease-emphasised: cubic-bezier(0.16, 0.78, 0.30, 1);
  ```
  These map to the philosophy §8 timing budget (interaction motion 200–300ms;
  page transitions ≤300ms).
- **ACTION (deliberate omission):** Do **not** add `--color-success`,
  `--color-warning`, `--color-danger`, `--color-info`, or `--color-neutral`
  tokens. State expression follows the §11A mapping table:

  | State | Expression |
  |---|---|
  | Default / idle | `text-fg`, `bg-surface` (or unchanged) |
  | Hover | `translate-y-[-1px]` + `border-border-strong` |
  | Active / pressed | `bg-accent` or `bg-accent-strong` |
  | Selected | `bg-accent-soft` background + `text-fg-on-accent` |
  | Disabled | `opacity-50` + `text-fg-faint` |
  | Loading | opacity pulse 0.85 ↔ 1.0, ~1.6s, on the region |
  | Error | `text-fg font-semibold` + gold recovery action |
  | Success (explicit) | gold inline note (`text-fg-on-accent` on `bg-accent-soft`); fades after ~3s |
  | Link | underline + `text-accent` on hover; no permanent color difference |
  | Focus (keyboard) | 2px gold outline + 2px offset (`focus-visible:ring-2 focus-visible:ring-accent focus-visible:ring-offset-2`) |
- **ACTION:** Codify the loading-pulse keyframe as a Tailwind utility (or
  inline rule) named `animate-loading-pulse`:
  ```css
  @keyframes loading-pulse {
    0%, 100% { opacity: 1; }
    50%      { opacity: 0.85; }
  }
  ```
- **ACTION:** Document above table verbatim in
  `docs/src/content/docs/design/visual-identity.md` (D3.18).
- **VALIDATE:** Stylelint passes (state-color tokens absence is enforced
  by the absence of declarations).
- **VALIDATE:** D5 crosscheck (separate phase) reviews state-without-hue
  in primitive gallery.

### Task D3.16 — Self-host Author + Satoshi (+ JBM) variable woff2 (refresh — REVERSED)

- **REFRESH (HARD REVERSAL):** Original D3.16 said "self-hosted fonts via
  `@fontsource`." There is **no `@fontsource/author` or `@fontsource/satoshi`**
  package — Author and Satoshi ship from Fontshare, not Google Fonts.
  Philosophy §6 mandates **self-host the variable woff2 directly**, fetched
  from Fontshare's per-font download endpoint. Brand handoff specified the
  Fontshare CDN, but Chromium ORB blocks the cookie-bearing CSS API
  response (memory `feedback_fontshare_api_quirk.md`); we override that
  delivery decision while keeping brand's "which fonts" call.
- **FFL ACCEPTANCE (load-bearing rationale, document durably):** Fontshare
  Free EULA clause 02 prohibits both "uploading them in a public server"
  and "transmit the Font Software over the Internet in font serving... from
  infrastructure other than Fontshare's." Self-hosting in this open-source
  repo formally violates clause 02. We accept that risk because:
  1. Chromium ORB on the CDN's CSS API breaks the cookie-bearing response
     (verified). Self-hosted variable woff2 is the only viable delivery.
  2. Production CSP `font-src 'self'` is materially stronger than allowing
     `cdn.fontshare.com`.
  3. If ITF objects, fallback is paid commercial license + on-prem mirror.
     The substitution is mechanical (URLs change; `@font-face` does not).
  4. The risk surfaces to a single party (ITF) with a single resolution
     path; not a structural risk to operators.
- **ACTION (fetch + commit woff2):**
  - Pull variable woff2 zips from
    `https://api.fontshare.com/v2/fonts/download/author` and
    `https://api.fontshare.com/v2/fonts/download/satoshi`.
  - Extract `WEB/fonts/Author-Variable.woff2`,
    `Author-VariableItalic.woff2`, `Satoshi-Variable.woff2`,
    `Satoshi-VariableItalic.woff2`.
  - Pull JetBrains Mono 400 (regular, non-italic) variable or static woff2
    from JetBrains' GitHub release or Google Fonts API; self-host one weight.
    Self-hosted JBM has no FFL constraint (OFL-1.1, permissive).
  - Place all five files at
    `frontend/public/fonts/fontshare/files/`.
  - Verify each woff2 by sha256 against the zip's contents and write a
    `SHA256SUMS` file in the same directory listing each filename + hash.
  - Commit the woff2 + SHA256SUMS as binary additions. They are large but
    bounded (≈40KB × 4 + ≈30KB ≈ 200KB total).
- **ACTION (authoritative `@font-face`):** Create
  `frontend/src/styles/fonts.css`:
  ```css
  @font-face {
    font-family: 'Author Variable';
    src: url('/fonts/fontshare/files/Author-Variable.woff2') format('woff2-variations');
    font-weight: 400 700;
    font-style: normal;
    font-display: swap;
  }

  @font-face {
    font-family: 'Author Variable';
    src: url('/fonts/fontshare/files/Author-VariableItalic.woff2') format('woff2-variations');
    font-weight: 400 700;
    font-style: italic;
    font-display: swap;
  }

  @font-face {
    font-family: 'Satoshi Variable';
    src: url('/fonts/fontshare/files/Satoshi-Variable.woff2') format('woff2-variations');
    font-weight: 400 700;
    font-style: normal;
    font-display: swap;
  }

  @font-face {
    font-family: 'Satoshi Variable';
    src: url('/fonts/fontshare/files/Satoshi-VariableItalic.woff2') format('woff2-variations');
    font-weight: 400 700;
    font-style: italic;
    font-display: swap;
  }

  @font-face {
    font-family: 'JetBrains Mono';
    src: url('/fonts/fontshare/files/JetBrainsMono-Regular.woff2') format('woff2');
    font-weight: 400;
    font-style: normal;
    font-display: swap;
  }
  ```
  The `@theme inline` declarations in `themes/index.css` already reference
  these family names. No `--font-display`/`--font-body`/`--font-mono`
  changes needed in D3.7 if the theme tree was authored with these names.
- **ACTION (wire fonts.css from index.css):** Append the fonts import to
  `frontend/src/index.css` so the canonical theme tree picks it up:
  ```css
  @import "./styles/themes/index.css";
  @import "./styles/fonts.css";
  ```
  D3.7 deliberately deferred this import to keep itself atomic (fonts.css
  did not exist between D3.7 and D3.16). With D3.16 creating the file in
  the same task, the import lands here.
- **ACTION (CSP — drop CDN, all three sites):**
  - `frontend/vite.config.ts:16`: change
    `"font-src 'self' https://cdn.fontshare.com"` → `"font-src 'self'"`.
  - `backend/src/security/csp.rs:30`: change
    `"; font-src 'self' https://cdn.fontshare.com"` → `"; font-src 'self'"`.
  - `backend/src/security/csp.rs:80` (unit-test fixture string):
    update to match the new `font-src 'self';` substring.
- **ACTION (rewrite stale README):** Replace
  `frontend/public/fonts/fontshare/README.md` with a self-host-rationale
  README. Cover:
  - Why self-host (ORB blocks cookie-bearing CSS API; CSP wants
    `'self'` only).
  - Why not `@fontsource` (no Author/Satoshi packages exist).
  - FFL clause-02 acceptance — link to this plan's D3.16 for the full
    rationale; note that ITF objection triggers a paid-license + mirror
    fallback.
  - SHA256SUMS verification: how to re-derive after a font refresh.
  - Variable-axis URL discovery procedure (preserved from old README —
    Playwright capture on `fontshare.com/fonts/{author,satoshi}`).
  - Italic constraint note: italic woff2 not on public weight API; pull
    from per-font download endpoint.
- **ACTION (update `Lockup.tsx` reference verification):** No edit needed
  — the existing component already references
  `"Satoshi Variable", "Satoshi", system-ui, sans-serif`, which matches
  the canonical name. Verify with `rg "Satoshi" frontend/src/components/Lockup.tsx`.
- **VALIDATE (build + load):** `npm run build` succeeds. `npm run dev`,
  open devtools network panel, font requests load from
  `/fonts/fontshare/files/...` with status 200; no requests to
  `cdn.fontshare.com`; no CSP violations in console.
- **VALIDATE (CSP parity dev):**
  ```bash
  curl -sI http://localhost:5173/ | tr ';' '\n' | grep -iq "font-src 'self'$\|font-src 'self';"
  ```
  exits 0.
- **VALIDATE (CSP parity prod):** Existing `csp.rs` unit tests pass with
  the updated allowlist string.
- **VALIDATE (integrity):**
  ```bash
  cd frontend/public/fonts/fontshare/files && sha256sum -c SHA256SUMS
  ```
  reports OK for all entries.
- **GOTCHA:** Vite serves `public/` at the root path. `url('/fonts/...')`
  resolves to `<dev-server>/fonts/...` in dev and `<dist>/fonts/...` in
  production. Do not relative-path from `themes/index.css`; absolute
  `/fonts/...` is correct.

### Task D3.17 — Accessibility pass

- **ACTION:** For every primitive in `/design/system`, verify:
  - Visible focus indicator in both themes (gold ring per §11A:
    `focus-visible:ring-2 focus-visible:ring-accent focus-visible:ring-offset-2`).
  - Full keyboard navigation: tab / shift-tab / enter / space / arrow.
  - WCAG 2.2 AA contrast for all text over backgrounds. Reverie Gold
    `#C9A961` against `#0E0D0A` Ink reaches >7:1 (passes 1.4.3 AAA).
    Light-theme darkened gold `#8E6F38` against Parchment `#E8DCC2`
    measures ≈3.47:1 — passes 1.4.11 (UI components, 3:1) and 1.4.3
    large-text but **fails 1.4.3 normal-text 4.5:1**. The accent is
    therefore correct only for focus rings, large CTAs, and recovery
    actions on Light; if any normal-size body text adopts it, axe will
    fail this gate. Verify with axe `--exit`.
- **ACTION:** Run axe with the **mandatory** `--exit` flag (default exit-0
  is silent):
  ```bash
  npx @axe-core/cli http://localhost:5173/design/system --exit
  ```
- **ACTION:** Document the focus-ring style in `visual-identity.md` (D3.18).
- **VALIDATE:** `axe-core` exits 0.
- **GOTCHA:** Focus visibility is non-negotiable for WCAG. The gold ring is
  the canonical pattern; do not substitute browser default outline.

### Task D3.18 — Canonicalise in `docs/design/visual-identity.md` (refresh — defer to brand SoT + rewritten spec)

- **REFRESH:** Original D3.18 had `visual-identity.md` re-derive content.
  Rewritten philosophy spec already canonicalises §6, §8, §10, §11, §11A,
  §11B, §11C, §18; brand identity at
  [unkos-dev/reverie-branding](https://github.com/unkos-dev/reverie-branding)
  is the SoT for color/typography/mark/lockup/tagline. New scope: pull from
  these sources rather than re-derive.
- **D3.18 OPEN-QUESTION RESOLUTION (philosophy spec §17 follow-up):** The
  philosophy spec at `plans/2026-04-25-design-system-philosophy-design.md`
  was originally tagged `will-fold-into:
  docs/src/content/docs/design/philosophy.md (after D2)`. Now post-D2 with
  brand additions, fold-in still applies, **but**
  `visual-identity.md` is the better home for the canonical token list,
  type scale, motion, theme architecture, and theme-cookie lifecycle, while
  `philosophy.md` carries §1–§5, §7, §9, §11–§14, §16, §18 (the
  conceptual content). Both files exist; `visual-identity.md` references
  philosophy where overlap exists.
- **ACTION:** Create `docs/src/content/docs/design/philosophy.md` —
  authoritative philosophy. Fold from
  `plans/2026-04-25-design-system-philosophy-design.md` content excluding
  the §6/§8/§10/§11/§11A/§11B/§11C tables (those move to visual-identity).
  Frontmatter: `title: Reverie Design Philosophy`, `description: ...`.
- **ACTION:** Create `docs/src/content/docs/design/visual-identity.md`
  with these sections:
  - **Brand identity reference.** Link
    [unkos-dev/reverie-branding `identity.md`](https://github.com/unkos-dev/reverie-branding/blob/main/identity.md)
    as SoT for color/typography/mark/lockup/tagline. Embed the §1 mark and
    §6 lockup proportions inline; do not duplicate the whole table.
  - **Tokens.** Full `--color-*` table from philosophy §10 (canvas/canvas-2/
    surface/surface-2/border/border-strong/fg/fg-muted/fg-faint/accent/
    accent-soft/accent-strong/fg-on-accent) for both themes. **No state-color
    tokens.**
  - **Typography.** Embed philosophy §6 role/weight table (Wordmark / Display /
    Section / Tagline / Body / Italic accent / Mono).
  - **Type scale.** Pulled from `themes/index.css` declarations.
  - **Spacing.** 4px base + 8/12/16/24/32/48/64/96 scale.
  - **Motion.** Per philosophy §8 + D3.15 motion tokens; document 200–300ms
    interaction budget; reduced-motion respect.
  - **State philosophy (no hue).** Embed philosophy §11A state-mapping table
    verbatim. Code blocks and charts noted as scoped exceptions.
  - **Theme architecture.** Cookie-name three-place rule:
    - "Cookie name `reverie_theme` is referenced in three places:
      `backend/src/auth/theme_cookie.rs` (`THEME_COOKIE_NAME` const),
      `frontend/src/fouc/fouc.js` (inline FOUC body, CSP-hashed at build),
      `frontend/src/lib/theme/cookie.ts`. All three MUST change together.
      The backend unit test on `set_theme_cookie` enforces the backend
      side; cross-stack drift tracked under
      [UNK-105](https://linear.app/unkos/issue/UNK-105)."
    - "Cookie attributes (`Path=/`, `Max-Age=31536000`, `SameSite=Lax`,
      no `HttpOnly`, no `Secure`) are a parity contract between
      `set_theme_cookie` (backend) and `writeThemeCookie` (frontend).
      Drift produces two cookies of the same name with divergent
      attributes; FOUC's `document.cookie.split('; ')` then matches
      non-deterministically. Both sides have unit tests asserting the
      attribute strings."
    - "FOUC is a blocking inline `<script>` injected by
      `frontend/vite-plugins/csp-hash.ts` at the
      `<!-- reverie:fouc-hash -->` marker; body lives at
      `frontend/src/fouc/fouc.js`. `vite build` emits
      `dist/csp-hashes.json` containing the SHA-256, which
      `backend/src/security/dist_validation.rs` reads at startup. CSP is
      hash-based, no nonce, no backend templating."
  - **Theme cookie lifecycle.** Section per parent §1416–1422 — verbatim
    inclusion of: `reverie_theme` survives logout by design (device state,
    not session state); industry precedent (GitHub `color_mode`, MDN,
    Audiobookshelf/Jellyfin/Kavita); shared-device consideration;
    fingerprinting consideration; **contrast rule:** any future
    *session-state* cookie MUST be `HttpOnly` and MUST clear on logout —
    `reverie_theme` is the explicit counterexample.
  - **Mark, lockup, tagline.** Reference `identity.md` §1, §2, §3, §6, §7;
    embed Slot proportions; show Lockup component usage:
    ```tsx
    import { Lockup } from "@/components/Lockup";
    <Lockup size={28} theme="dark" />
    ```
- **ACTION (cross-reference from backend code):** In
  `backend/src/auth/theme_cookie.rs`, the module-level `//!` comment MUST
  include:
  ```rust
  //! Lifecycle: survives logout by design. See
  //! docs/design/visual-identity.md § Theme Cookie Lifecycle for rationale
  //! and the contrast rule for session-state cookies.
  ```
- **ACTION (sidebar):** Update `docs/astro.config.mjs`:
  ```javascript
  {
    label: 'Design',
    items: [
      { label: 'Philosophy', slug: 'design/philosophy' },
      { label: 'Visual Identity', slug: 'design/visual-identity' },
    ],
  },
  ```
- **VALIDATE:** `cd docs && npm run build` succeeds. Both pages reachable
  in built site. Anchor `#theme-cookie-lifecycle` resolves.

### Task D3.19 — Smoke-test an extra theme

- **ACTION:** Add throwaway third theme file (`frontend/src/styles/themes/sepia.css`)
  with minimally-plausible values and a `[data-theme="sepia"]` selector;
  confirm adding it + a switcher option works end-to-end with no
  architectural change.
- **ACTION:** Delete the throwaway file before commit (or keep as docs
  example in `visual-identity.md`).
- **VALIDATE:** Toggle `data-theme="sepia"` in devtools → tokens apply →
  architecture confirmed theme-unlimited.

### Task D3.20 — Update operator CSP doc for fonts + cookies (refresh — drop CDN row)

- **REFRESH:** Original D3.20 added `font-src 'self'` row referencing
  `cdn.fontshare.com` allowance in some operator deployments. Self-hosting
  removes that allowance entirely. The doc now describes a single
  `font-src 'self'` policy with a "to allowlist a CDN, edit
  `csp.rs::build_html_csp`" escape hatch.
- **ACTION:** Edit `docs/security/content-security-policy.md` (canonical
  operator surface from UNK-106).
- **ACTION (`## Cookies` section, before `## Further reading`):**
  ```markdown
  ## Cookies

  Reverie sets two cookies on authenticated browsers:

  | Name            | HttpOnly | Max-Age     | Path | SameSite | Purpose                                    | Lifecycle                                          |
  | --------------- | -------- | ----------- | ---- | -------- | ------------------------------------------ | -------------------------------------------------- |
  | `id`            | **Yes**  | Session     | `/`  | `Lax`    | tower-sessions session cookie (auth state) | Cleared on logout; short-lived                     |
  | `reverie_theme` | **No**   | 365 days    | `/`  | `Lax`    | Dark/Light/System preference for FOUC      | Survives logout by design (device state, not PII)  |

  `reverie_theme` is intentionally not `HttpOnly` because JavaScript must
  read it synchronously before React hydrates to avoid a theme flicker. It
  carries no PII — only the string `system`, `light`, or `dark`. See
  `docs/design/visual-identity.md` § Theme Cookie Lifecycle for the full
  rationale and the contrast rule: any future *session-state* cookie MUST
  be `HttpOnly` and MUST be cleared on logout; `reverie_theme` is the
  explicit counterexample.

  Neither cookie sets `Secure` in the default deployment because the backend
  speaks plain HTTP behind a TLS-terminating reverse proxy (matches the HSTS
  configuration story above). Operators running Reverie with direct HTTPS
  termination would typically layer the `Secure` attribute at the proxy via
  `Set-Cookie` rewriting — Reverie itself does not attempt to detect TLS
  state.
  ```
- **ACTION (extend "Dev mode vs production" table):** Add immediately
  after the `HSTS` row:
  ```markdown
  | font-src policy   | `'self'` (matches prod)                            | `'self'` (declared in `csp.rs::build_html_csp`)           |
  ```
- **ACTION (`### Fonts` subsection after the table):**
  ```markdown
  ### Fonts

  Reverie self-hosts variable woff2 fonts at
  `frontend/public/fonts/fontshare/files/`; the `font-src 'self'` directive
  is sufficient for the default deployment. Operators who need fonts from
  a CDN (e.g., Google Fonts, custom asset host) must edit
  `backend/src/security/csp.rs::build_html_csp` to allowlist the required
  origin(s) and rebuild. No runtime configuration knob exists for this —
  the policy is intentionally code-declared so every deployment has an
  identical, auditable font policy out of the box.

  The canonical theme tree (`frontend/src/styles/themes/`,
  `frontend/src/styles/fonts.css`) declares Author + Satoshi as variable
  woff2 from Fontshare and JetBrains Mono 400. Author and Satoshi italics
  are pulled from Fontshare's per-font download endpoint (the public
  weight CSS API does not expose italic variable axes).
  ```
- **VALIDATE:** `cd docs && npm run build` succeeds (if doc is part of
  Starlight; otherwise check with project's markdown linter).
- **VALIDATE (cross-reference integrity):** Anchor
  `visual-identity.md § Theme Cookie Lifecycle` exists (added in D3.18).
  If absent, D3.18 did not land; fix D3.18 before marking D3.20 done.

---

## D3 Exit Gate

- Gallery complete; both themes render every primitive in every state.
- Both themes pass WCAG 2.2 AA contrast (axe `--exit` exits 0).
- No primitive shows stock shadcn DNA (every primitive references
  `--color-*` tokens, no hardcoded hex).
- Production bundle free of `/design` code (structural `manualChunks`
  gate: no `design-*.js` in `dist/assets/`).
- Operator CSP doc updated with cookies + fonts coverage; `font-src 'self'`
  only.
- Brand SoT references resolve: `identity.md` linked from
  `visual-identity.md`; Slot mark + Lockup component in use.
- Self-hosted fonts load from `/fonts/fontshare/files/` with no
  `cdn.fontshare.com` requests.
- Cross-stack cookie parity: backend unit test asserts attribute strings;
  frontend `cookie.test.ts` asserts the matching strings.
- Forwarding note in parent plan at line 1187 directs readers to this plan.

---

## Testing Strategy

### Unit Tests

| Test File | Test Cases | Validates |
|---|---|---|
| `backend/src/auth/theme_cookie.rs` (inline `#[cfg(test)]`) | `set_theme_cookie` produces `THEME_COOKIE_NAME = "reverie_theme"` (string compare), `http_only=false`, `same_site=Lax`, `path=/`, `max_age=Duration::days(365)` | Cookie attribute contract; cross-stack const guard |
| `backend/tests/auth_theme.rs` (or extension) | `/auth/me` includes field; PATCH valid → 200; PATCH invalid → 422 (not 400); cookie present in response | Endpoint contract + validation |
| `frontend/src/lib/theme/cookie.test.ts` | Round-trip; malformed → null; written string contains `Path=/`, `Max-Age=31536000`, `SameSite=Lax`; does NOT contain `HttpOnly`, `Secure` | Frontend half of parity contract |
| `frontend/src/lib/theme/ThemeProvider.test.tsx` | 8-row initial-state matrix + reconciliation + rollback + system-pref change + BroadcastChannel | Provider correctness |
| `frontend/src/__tests__/hex-ban.test.ts` | RuleTester valid/invalid cases | Hex-ban rule fires on `#abc123` literals |
| `frontend/vite-plugins/__tests__/csp-hash.test.ts` | Hash regenerates after `fouc.js` body change | FOUC integration intact |

### Integration Tests

| Test | Validates |
|---|---|
| `axe-core` against `/design/system` (both themes) | WCAG 2.2 AA |
| `npm run build && test -z "$(ls frontend/dist/assets/design-*.js 2>/dev/null)"` | Structural design-tree tree-shake |
| `cd docs && npm run build` | Both design docs reachable; sidebar updated |
| `sqlx migrate run` + `sqlx migrate revert` | Migration + rollback |

### Edge Cases Checklist

- [ ] Invalid `theme_preference` value (e.g. `"purple"`) → PATCH 422
- [ ] Logged-out visitor (`/auth/me` 401) → provider stays on
      cookie-derived preference; no PATCH attempt
- [ ] Malformed `reverie_theme` cookie value → FOUC catches → `data-theme="light"`
- [ ] PATCH failure → optimistic update rolls back cookie + DOM
- [ ] System preference set + `prefers-color-scheme` toggles mid-session →
      `effective` updates without page reload
- [ ] Two tabs open, theme changed in tab A → tab B updates without reload
      and without re-PATCHing
- [ ] First-time visitor (no cookie) → `preference='system'`,
      `effective` from `prefers-color-scheme`
- [ ] Reduced-motion preference → loading pulse disabled, status-dot pulse
      disabled, hover lift reduces to opacity change

---

## Validation Commands

### Level 1: STATIC_ANALYSIS

```bash
cd backend && cargo clippy -p reverie-api --all-targets -- -D warnings
```

```bash
cd frontend && npm run lint
```

```bash
cd frontend && npx stylelint 'src/**/*.css' --max-warnings 0
```

**EXPECT:** All exit 0.

### Level 2: UNIT_TESTS

```bash
cd backend && cargo test -p reverie-api auth::theme_cookie
```

```bash
cd backend && cargo test -p reverie-api models::user
```

```bash
cd frontend && npm test
```

**EXPECT:** All tests pass.

### Level 3: BUILD_AND_TYPECHECK

```bash
cd backend && cargo build -p reverie-api
```

```bash
cd frontend && npm run build
```

```bash
cd docs && npm run build
```

**EXPECT:** All builds succeed.

### Level 4: STRUCTURAL_GATE (production tree-shake)

```bash
cd frontend && npm run build && test -z "$(ls dist/assets/design-*.js 2>/dev/null)"
```

**EXPECT:** Exit 0 (no design chunk emitted).

### Level 5: DATABASE_VALIDATION

```bash
cd backend && sqlx migrate run
```

```bash
cd backend && sqlx migrate revert && sqlx migrate run
```

**EXPECT:** Both succeed; column appears and disappears correctly.

### Level 6: BROWSER_VALIDATION

Run `npm run dev` from `frontend/` then verify:

- `/design/system` renders every primitive in both themes.
- Theme-switcher cycles System / Light / Dark.
- Open two tabs; change theme in one; second tab updates.
- Network panel: fonts load from `/fonts/fontshare/files/`; no
  `cdn.fontshare.com` requests; no CSP violations.
- Hard-reload with `reverie_theme=dark` cookie set → `<html data-theme="dark">`
  before any React mount event (devtools Performance trace or React DevTools
  mount delay).

### Level 7: ACCESSIBILITY

```bash
cd frontend && npm run dev &
sleep 2
npx @axe-core/cli http://localhost:5173/design/system --exit
```

**EXPECT:** Exit 0. Both Dark and Light themes pass.

---

## Acceptance Criteria

- [ ] All 20 D3 sub-tasks complete in dependency order.
- [ ] Forwarding note inserted in parent plan (D3.0).
- [ ] Three D2 explore trees deleted; no `design/explore` references in
      working tree.
- [ ] Migration round-trips clean.
- [ ] User model + endpoints + cookie helper + tests landed.
- [ ] Self-hosted woff2 commit + SHA256SUMS verifies.
- [ ] CSP `font-src 'self'` only (dev + prod + unit-test fixture).
- [ ] Canonical `--color-*` token tree at `frontend/src/styles/themes/`;
      no state-color tokens; brand identity values match spec §10.
- [ ] shadcn primitives installed and restyled against tokens.
- [ ] No raw hex literals outside theme files (ESLint + Stylelint enforce).
- [ ] Theme provider + cookie + switcher + cross-tab sync working.
- [ ] FOUC body replaced; build emits matching `dist/csp-hashes.json` hash.
- [ ] `/design/system` gallery renders every primitive in every state in
      both themes.
- [ ] Production bundle structurally tree-shakes design tree.
- [ ] Docs (philosophy + visual-identity) build and reference brand SoT.
- [ ] Operator CSP doc updated with `## Cookies` + `### Fonts`.
- [ ] Axe `--exit` exits 0 against `/design/system` in both themes.

---

## Completion Checklist

- [ ] All Level 1 commands pass (lint + clippy + stylelint)
- [ ] All Level 2 commands pass (unit tests, both stacks)
- [ ] All Level 3 commands pass (cargo build + npm build + docs build)
- [ ] Level 4 structural gate exits 0
- [ ] Level 5 database round-trip clean
- [ ] Level 6 browser checks completed (manual)
- [ ] Level 7 axe `--exit` exits 0
- [ ] PR opened against `main` from `feat/design-system-d3`
- [ ] PR body links the brand SoT, the rewritten philosophy spec, and
      this plan
- [ ] **Security-review affirmation (CLAUDE.md hard rule §6):** PR body
      explicitly answers "will this stand up to a security review?" with
      coverage of (a) the new non-`HttpOnly` `reverie_theme` cookie and
      its PII-free invariant; (b) CSP strengthening (`font-src` drops the
      `cdn.fontshare.com` origin in dev + prod + unit-test fixture);
      (c) the FFL clause-02 acceptance trade-off and ITF-objection
      fallback path. Substance is documented inline in this plan
      (D3.5/D3.16/D3.18/D3.20 + Notes); the affirmation surfaces it as
      an explicit pre-merge gate rather than relying on inline rationale.
- [ ] Adversarial review (`/crosscheck` per `feedback_crosscheck_default.md`)
      passes
- [ ] User reviews and merges (per `feedback_user_does_merge.md`)

---

## Risks and Mitigations

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| Fontshare ITF objects to FFL clause-02 violation (self-hosting in public repo) | LOW | MED | Documented in D3.16 README + plan rationale; fallback is paid commercial license + on-prem mirror. Substitution is mechanical. |
| `shadcn init --yes` non-interactive flag changes between CLI versions | LOW | LOW | D3.6 includes fallback (`--help` capture). |
| Cross-stack cookie attribute drift | MED | MED | Backend + frontend both have unit tests asserting verbatim attribute strings. Drift fails the test in the same PR. |
| FOUC hash regenerates but backend dist-validation lags | LOW | MED | `vite-plugins/csp-hash.ts` emits on every build; `dist_validation.rs` reads on startup. Hash drift fails fast at boot. |
| `prefers-color-scheme` change mid-session not picked up by provider | LOW | LOW | `ThemeProvider.test.tsx` includes the explicit case; matchMedia listener wired. |
| Brand identity drifts from canonical (`identity.md`) before D3 ships | LOW | HIGH | Brand SoT is pinned to a public repo; `identity.md` content embedded in this plan as snapshot. Any drift surfaces as a doc PR before code lands. |
| Brand handoff conflict on font delivery (CDN vs self-host) | RESOLVED | — | Documented as intentional override-of-the-override in philosophy §6 + this plan §D3.16; brand wins on which fonts, not on how served. |
| `manualChunks` config changes don't tree-shake (chunk still emitted) | LOW | MED | Level 4 gate is the test; CI fails if chunk leaks into prod. |
| WCAG AA contrast fails on Light theme + Reverie Gold | LOW | HIGH | Brand pre-darkened light-theme accent to `#8E6F38` for AA; D3.17 axe pass is the gate. |
| Lockup `Satoshi Variable` family-name mismatch with self-hosted name | LOW | LOW | Verified during exploration — already matches. D3.16 grep verifies. |
| Original D3.16 mis-pattern-matched as `@fontsource` install | MED | MED | Refresh note in this plan's preamble + D3.16 body's REVERSAL header. |

---

## Notes

**Why keep D3.1–D3.20 numbering instead of renumbering.** Cross-references
exist outside this plan: the TODO comment in `frontend/src/fouc/fouc.js`
mentions "D3.13"; memory file `feedback_shared_constants_tracker.md`
references the cross-stack guard introduced in D3.10/D3.18. Renumbering
would force edits in those locations for cosmetic reasons. Most tasks here
are content refreshes within an unchanged shape, so the numbering remains
truthful.

**Why D3.16 keeps its number despite reversed approach.** Number and
position-in-sequence are stable; only the implementation verb changed
(was `npm install @fontsource/X`, now `fetch + commit + self-host woff2`).
A future executor reading "D3.16" cross-referenced from elsewhere should
land at the new body — renumbering would silently strand cross-references.
The `**REFRESH (HARD REVERSAL):**` header at the top of D3.16 is the
mitigation for the pattern-match risk.

**FFL clause-02 acceptance.** The Fontshare Free EULA prohibits both
"uploading them in a public server" and "transmit the Font Software... in
font serving... from infrastructure other than Fontshare's." Self-hosting
in the public repo is a formal violation. Accepted in exchange for
Chromium ORB compatibility and `font-src 'self'` CSP strength. ITF objection
fallback is paid commercial license + on-prem mirror; substitution is
mechanical (URLs change; `@font-face` does not). Documented in the rewritten
`frontend/public/fonts/fontshare/README.md` and durably in this plan.

**Why brand wins (mostly) and we override (once).** Brand identity is
authoritative on which fonts (Author + Satoshi + JBM), which colours
(Reverie Gold + Ink + Cream + Parchment), which mark (Slot), which lockup,
which tagline. Self-host vs CDN is a delivery decision; brand specified
the CDN, but Chromium ORB blocks the cookie-bearing CSS API response,
which is a hard technical incompatibility. Override-of-the-override is
narrowly scoped to delivery and documented inline.

**No `--color-success/warning/danger/info` tokens.** This is a load-bearing
brand invariant from §11A — semantic state colour fights the cinematic-
boutique register, and Reverie Gold must remain unambiguous. Adding any
hue-coded state token in this PR or any subsequent PR requires a separate
brand-aligned decision; do not "harmlessly add" them on the assumption
they'll be useful later. Charts and code blocks are scoped exceptions per
§11A; if/when they ship, document the deviation in `visual-identity.md`
and constrain to the surface that requires it.

**JetBrains Mono is loaded but conditional.** UNK-113 reviews adoption
post-0.1.0. If no surface adopts the mono token by then, `--font-mono`
declaration + JBM `@font-face` are removed in a follow-up. Do not
retroactively delete the declaration in this PR.

**Step 11 inheritance.** The library grid, book detail, search, hero
screens, and onboarding flow all inherit this token system, this primitive
gallery, and this theme provider. Any token name change after this PR
ships imposes a churn cost on Step 11. The token namespace (`--color-*`)
and the 13-token palette are committed; if a Step-11 surface needs a
token not listed in §10, add it as a new `--color-*` token (extend the
canonical theme), don't reach for a state-coded substitute.

**Adversarial review.** Per memory `feedback_crosscheck_default.md`, run
`/crosscheck` (Opus + Gemini) before PR ready-for-review. `/santa-method`
(Claude-only) is the fallback if Gemini quota is unavailable; do not use
free-tier-gated CLIs.

---

## Confidence Score

**8/10** for one-pass implementation success.

**Rationale:**

- **+3** All 8 brand-neutral backing patterns (USER_MODEL_COLUMN_ADDITION,
  PATCH_HANDLER_SHAPE, THEME_COOKIE_WRITER, THEME_COOKIE_FRONTEND_WRITER,
  SQLX_TEST_HARNESS, FOUC_INLINE_SCRIPT, THEME_PROVIDER,
  SHADCN_COMPONENTS_JSON) are line-referenced; the 9th pattern
  (TAILWIND_V4_MULTI_THEME_BRAND) is re-emitted in full with brand values
  inline, no placeholders. 9 patterns total: 8 referenced + 1 re-emitted.
- **+2** Brand SoT is canonical and pinned (`unkos-dev/reverie-branding/identity.md`);
  philosophy spec §10 palette table matches identity.md §4 verbatim;
  no token-value ambiguity.
- **+2** Pre-D3 state mapped: D2 explore trees enumerated, font directory
  state confirmed, backend pre-conditions verified (`theme_cookie.rs`
  absent, migration timestamps audited, csp.rs allowlist line numbers
  located).
- **+1** Cross-stack cookie parity pinned via two unit tests on each side
  with verbatim attribute string assertions; FOUC hash auto-regenerates;
  structural tree-shake gate is unambiguous.
- **−1** D3.16 self-hosting requires a one-time manual step (download zips
  from Fontshare, extract, place in repo, write SHA256SUMS). The fetch
  command is documented but not scriptable inside this plan because
  Fontshare's per-font download endpoint requires a browser-issued request
  (cookie-bearing). An executor unfamiliar with the discovery procedure
  could mis-fetch (e.g. pull static-weight files from the public CSS API
  which doesn't expose variable axes). The README rewrite + the
  philosophy spec §6 italic-file constraint note are the mitigations.
- **−1** Original D3.18 ambiguity around philosophy spec fold-into is
  resolved here, but the actual content split (which spec section goes
  where) requires judgment during execution. Plan documents the split;
  reasonable variation acceptable.

**Where this plan would score 10/10:** if D3.16 included a verified
script for fetching + validating the woff2 zips, and if the philosophy →
docs content split were enumerated section-by-section in D3.18 rather
than as a high-level rule. Both are deferred to execution-time judgment
with this plan as guide.
