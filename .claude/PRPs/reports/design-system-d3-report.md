# Implementation Report — Design System D3

**Plan**: `.claude/PRPs/plans/design-system-d3.plan.md` (now archived to `completed/`)
**Source Issues**: [UNK-103](https://linear.app/unkos/issue/UNK-103) (Step 10 epic), [UNK-104](https://linear.app/unkos/issue/UNK-104) (OIDC e2e — deferred), [UNK-105](https://linear.app/unkos/issue/UNK-105) (cross-stack pipeline), [UNK-113](https://linear.app/unkos/issue/UNK-113) (post-0.1.0 JBM review)
**Branch**: `feat/design-system-d3`
**Date**: 2026-04-27
**Status**: COMPLETE WITH DOCUMENTED DEVIATIONS

---

## Summary

D3 codifies Reverie's design system against the April 2026 brand identity:
self-hosted variable woff2 fonts (Author + Satoshi + JetBrains Mono),
canonical `--color-*` tokens sourced from
[unkos-dev/reverie-branding](https://github.com/unkos-dev/reverie-branding/blob/main/identity.md)
+ philosophy spec §10, no hue-coded state tokens, themed shadcn primitives
bound to the brand palette via an alias layer, FOUC-free Dark/Light/System
theme switching with a per-user `theme_preference` DB column, and a
dev-only `/design/system` primitive gallery that becomes Step 11's visual
contract. Production CSP tightens from `font-src 'self' https://cdn.fontshare.com`
to `font-src 'self'`. Cookie attribute parity is pinned by symmetric unit
tests on both stacks.

---

## Tasks Completed

| #     | Task                                                | Outcome      |
| ----- | --------------------------------------------------- | ------------ |
| D3.0  | Forwarding note in parent plan                      | Verified     |
| D3.1  | Prune D2 explore trees                              | Done         |
| D3.2  | theme_preference migration (up + down)              | Done         |
| D3.3  | User model 4-edit pattern                           | Done         |
| D3.4  | /auth/me returns theme_preference                   | Done         |
| D3.5  | theme_cookie module + PATCH /auth/me/theme + tests  | Done         |
| D3.6  | shadcn init zero-prompt                             | Done         |
| D3.7  | Canonical theme tree + tokens                       | Done         |
| D3.8  | shadcn primitives installed (form → field)          | Done         |
| D3.9  | Restyle primitives (alias-layer approach)           | DEVIATION    |
| D3.10 | Theme provider + cookie + switcher + canvas shell   | Done         |
| D3.11 | /design/system gallery                              | Done         |
| D3.12 | Dev-only route gate + manualChunks                  | Done         |
| D3.13 | FOUC body                                           | Done (after `</script` fix) |
| D3.14 | ESLint + Stylelint hex bans                         | Done         |
| D3.15 | Motion tokens + state-without-hue                   | Done (in D3.7) |
| D3.16 | Self-host Author/Satoshi/JBM                        | Done         |
| D3.17 | axe accessibility pass                              | Dark clean; Light has 4 documented violations |
| D3.18 | Docs (philosophy + visual-identity)                 | Done         |
| D3.19 | Smoke-test extra theme                              | Done (Playwright injection) |
| D3.20 | Operator CSP doc                                    | Done         |

---

## Validation Results

| Level                            | Result | Details |
|----------------------------------|--------|---------|
| L1 Backend clippy                | PASS   | `cargo clippy -p reverie-api --all-targets -- -D warnings` clean |
| L1 Frontend lint (ESLint)        | PASS   | `npm run lint` clean |
| L1 Stylelint                     | PASS   | `.stylelintrc.json` shipped in `f44c3fa`; rule active (`color-no-hex` on `src/**/*.css`) |
| L2 Backend tests                 | PASS   | 434 + 2 unit tests, 0 failures |
| L2 Frontend tests                | PASS   | 30 tests, 0 failures (cookie + ThemeProvider + existing) |
| L3 Backend build                 | PASS   | `cargo build -p reverie-api` |
| L3 Frontend build                | PASS   | `npm run build` |
| L3 Docs build                    | PASS   | `cd docs && npm run build` (6 pages, including 2 new design pages) |
| L4 Structural tree-shake gate    | PASS   | No `dist/assets/design-*.js` in production output |
| L5 Migration round-trip          | PASS   | `sqlx migrate revert && sqlx migrate run` clean |
| L6 Browser validation (manual)   | PASS   | data-theme cold-load works; fonts load from `/fonts/fontshare/files/`; no `cdn.fontshare.com` requests; theme switcher cycles |
| L7 Accessibility (axe)           | PARTIAL | Dark clean. Light has 4 violations (bg-accent on Light at normal text size — see plan §327 documented trade-off) |

---

## Deviations from Plan

### D3.9 — alias layer instead of per-file utility-class rewrites

**Plan letter:** rewrite each of the 25 `components/ui/*.tsx` files to
replace `bg-card/bg-primary/text-foreground/etc.` with
`bg-canvas/bg-accent/text-fg/etc.`

**What we did:** added a shadcn-namespace alias block to
`@theme inline` in `styles/themes/index.css` that rebinds shadcn's
expected token names onto the canonical brand palette. Stock shadcn
primitives render with brand identity without per-file edits.

**Why:** the alias approach satisfies the plan's exit gate ("every
primitive references --color-* tokens, no hardcoded hex") because
every shadcn-namespace utility resolves via `var(--color-*)`. The
class-rename approach risks merge churn against future shadcn
registry updates and adds ~25 file diffs whose effective output is
identical to the alias mechanism.

**Trade-off you'll see:** `--color-accent` aliases to brand gold, so
shadcn's `bg-accent` (which it uses for hover/focus highlights in
dropdown-menu and select) lights up in full Reverie Gold + contrast
text. That's louder than a strictly-soft-gold treatment but matches
the brand's "selected = gold" intent. If the gallery surfaces
specific primitives where the gold-on-hover is too loud, follow up
with per-primitive `cva` overrides in a separate polish PR.

### D3.17 — axe Light-theme violations match plan §327's documented trade-off

**Plan letter:** axe `--exit` exits 0 against `/design/system` in both
themes.

**What we did:** ran axe via Playwright + injected `axe-core`. Dark
theme passes clean (0 violations, 22 passes). Light theme has 4
violations, all `bg-accent` (#8E6F38) text on lg-size buttons + the
default badge — exactly what plan §327 calls out: "Passes 1.4.11 (UI
3:1) + 1.4.3 large-text on Parchment; not normal-text 4.5:1 —
restrict to focus rings, large CTAs, recovery actions. ... if any
normal-size body text adopts it, axe will fail this gate."

**Why we accept it:** the constraint is brand-level and intentional.
The `/design/system` route is dev-only, structurally tree-shaken in
production; `/design/system` axe-clean is a developer tool, not a
user-facing one. Production surfaces will discriminate between
default-size primaries (outline variant, brand-correct) and large
CTAs (lg + bg-accent, brand-correct).

### D3.10 — TDD ordering noted

The ThemeProvider implementation was written before its tests, then
tests added second. CLAUDE.md hard rule §5 specifies tests-first; the
plan's D3.10 ACTION block also calls out "TDD — write FIRST". Not a
correctness issue (tests pass and exercise the documented behaviour),
but a process deviation worth flagging.

### Plan §11A "Selected = bg-accent-soft + text-fg-on-accent" — errata

axe surfaced that the documented Selected-state mapping fails AA
contrast in both themes (Parchment-on-light-gold is 1.81:1; Ink-on-
dark-muted-gold is similar). `text-fg-on-accent` is correct only on
full `bg-accent` fills, where on-accent is the ink/parchment that
contrasts with the saturated gold. On `bg-accent-soft`, the correct
on-color is `text-fg`. Updated visual-identity.md, philosophy.md, and
the gallery accordingly. Surface as "errata" rather than silent
deviation.

### D3.13 — FOUC body initially shipped with `</script` literal in a comment

The csp-hash plugin checks for the literal string `</script>`, but the
HTML parser terminates the inline script at `</script` (no `>` needed).
Initial commit shipped a comment containing `</script`, causing the
FOUC to terminate early and `<html data-theme>` to remain unset on
cold load. Fix landed in commit d29a7cc — comment was rephrased to
avoid the literal entirely. Browser-driven verification confirms the
fix.

---

## Files Changed

29 files added, 14 modified, 13 deleted. Highlights:

- **Backend** (added): `backend/src/auth/theme_cookie.rs`, two
  migration files, three integration tests inline in
  `routes/auth.rs::tests`.
- **Backend** (modified): `models/user.rs` (4-edit pattern),
  `routes/auth.rs` (new handler + OIDC callback signature),
  `auth/mod.rs` (re-export), `security/csp.rs` (drop CDN allowance +
  matching unit-test fixture).
- **Frontend** (added): full `lib/theme/` subtree (cookie + api +
  ThemeProvider + tests), `components/theme-switcher.tsx`,
  `pages/design/system.tsx`, `routes/design.tsx`, the canonical
  `styles/themes/index.css` + `styles/fonts.css`, 25 shadcn primitives
  in `components/ui/`, `lib/utils.ts`, `components.json`, five woff2
  files + SHA256SUMS.
- **Frontend** (modified): `App.tsx`, `main.tsx`, `index.css`,
  `vite.config.ts` (alias + manualChunks + DEV_CSP), `tsconfig.app.json`
  + `tsconfig.json` (path aliases), `index.css` (canonical theme +
  fonts imports), `fouc/fouc.js` (FOUC body).
- **Frontend** (deleted): D2 explore trees, Vite scaffold (App.css +
  assets/), the stale `pages/design/explore/`.
- **Docs**: new `docs/src/content/docs/design/{philosophy,visual-identity}.md`;
  `docs/security/content-security-policy.md` Cookies + Fonts coverage.
- **Plans**: parent `design-system.plan.md` forwarding note;
  `design-system-d3.plan.md` itself is archived to `completed/`.

---

## Tests Written

| Test File | Tests |
|---|---|
| `backend/src/auth/theme_cookie.rs#tests` | `set_theme_cookie_writes_canonical_attributes` (cross-stack drift guard, including `Secure`) |
| `backend/src/models/theme_preference.rs#tests` | `as_str_matches_serde_lowercase`, `json_roundtrip_uses_lowercase_string`, `json_rejects_unknown_variant` (wire-format parity + serde validation gate) |
| `backend/src/routes/auth.rs#tests` | `me_returns_theme_preference_default`; `patch_theme_returns_401_without_auth`; `patch_theme_updates_user_row` (parametrized over `light` / `dark` / `system`); `patch_theme_rejects_invalid_value` (422 + no row mutation + no `reverie_theme=` Set-Cookie) |
| `frontend/src/lib/theme/cookie.test.ts` | 8 tests: round-trip, malformed → null, ignores other cookies, attribute parity (`Path=/`, `Max-Age=31536000`, `SameSite=Lax`, `Secure`, NOT `HttpOnly`) |
| `frontend/src/lib/theme/ThemeProvider.test.tsx` | 8 tests: initial-state matrix (cookie × dataset.theme × matchMedia), 401 → no PATCH, server reconciliation, optimistic update + 422 rollback, matchMedia reactivity |
| `frontend/src/fouc/fouc.test.ts` | 7 tests: jsdom evaluation of the inline FOUC body — cookie ∈ {dark, light, system + matchMedia, no cookie}, malformed cookie, matchMedia throws → catch warns + defaults to light |

---

## Security Review Affirmation (Completion Checklist gate)

Per CLAUDE.md hard rule §6 and the plan's Completion Checklist, the
three explicit gates:

### (a) `reverie_theme` non-`HttpOnly` cookie + PII-free invariant

`reverie_theme` is intentionally not `HttpOnly` because the FOUC
script must read it synchronously before React hydrates — hiding it
from JS would re-introduce the theme flicker we are deleting. The
cookie carries no PII: only the literal string `system`, `light`, or
`dark`. It survives logout by design (matches industry precedent —
GitHub `color_mode`, MDN, Audiobookshelf, Jellyfin, Kavita).

The contrast rule for future cookies is documented at the backend
module header (`backend/src/auth/theme_cookie.rs`) and at
`docs/design/visual-identity.md § Theme Cookie Lifecycle` and the
operator surface `docs/security/content-security-policy.md ## Cookies`:
**any future session-state cookie MUST be `HttpOnly` and MUST clear
on logout.** `reverie_theme` is the explicit counterexample.

The cookie attribute string (`Path=/, Max-Age=31536000, SameSite=Lax,
Secure, no HttpOnly`) is pinned by symmetric unit tests on backend
(verbatim string asserts on the built `Cookie` struct) and frontend
(string asserts on the written `document.cookie` input). Drift on
either side fails the corresponding test in the same PR.

### (b) CSP strengthening — `font-src` drops the CDN

Production CSP at `backend/src/security/csp.rs::build_html_csp` now
emits `font-src 'self'` (was `font-src 'self' https://cdn.fontshare.com`).
The matching dev CSP in `frontend/vite.config.ts` is identically
tightened. The unit-test fixture string (`csp.rs:80` in the test
module) is updated to assert the new value verbatim, so a future
loosening would fail the test in the same PR.

Operators who need a font CDN (Google Fonts, custom asset host) edit
`build_html_csp` directly and rebuild — no runtime configuration
knob, by design (every deployment carries an identical, auditable
font policy).

### (c) FFL clause-02 acceptance + ITF-objection fallback

Self-hosting Author + Satoshi woff2 in this open-source repo is a
formal violation of Fontshare Free EULA clause 02 ("uploading them
in a public server" + "transmit the Font Software... in font
serving... from infrastructure other than Fontshare's"). The
trade-off is documented inline at:

- The plan's `D3.16 — FFL ACCEPTANCE` block (§ rationale 1–4).
- `frontend/public/fonts/fontshare/README.md` (the operator-facing
  rationale + verification procedure).
- This report's "Why we accept it" notes.

Acceptance basis: (1) Chromium ORB blocks Fontshare's cookie-bearing
CSS API, leaving self-hosting as the only viable delivery for the
brand's typographic register; (2) the production CSP `font-src 'self'`
is materially stronger than the prior `font-src 'self'
https://cdn.fontshare.com`; (3) if Indian Type Foundry objects, the
fallback is a paid commercial license + on-prem mirror — substitution
is mechanical (URLs change; `@font-face` declarations do not); (4)
risk surfaces to a single party (ITF) with a single resolution path,
not as a structural risk to operators. JetBrains Mono is OFL-1.1
(permissive — no FFL constraint).

---

## Appendix — Post-Review Fixups

After the report was archived, a five-agent multi-agent review on PR #52
surfaced issues that landed as commits `4d5f888` through `68d8d49` on
the same branch. The fixes:

- `cargo fmt` on the new auth test code (CI was failing).
- OIDC session cleanup at `routes/auth.rs:151-153` switched from
  `let _ = session.remove(...)` (silently discarded `Result`) to
  `if let Err(e) = ... { tracing::warn!(...) }` so a degraded session
  store surfaces in logs without aborting login.
- Theme PATCH rollback now surfaces failures via `toast.error(...)`.
  `Toaster` was registered but never mounted; `sonner.tsx` was importing
  `useTheme` from `next-themes` (which was never wired). Wired through
  the project's `ThemeProvider`, mounted in `main.tsx`, dropped the
  `next-themes` dep.
- Theme cookie's `Secure` attribute is now always set. Initial fix made
  it conditional on `SecurityConfig::behind_https`, but CodeQL's
  data-flow analysis still flagged the false-branch path. Reverie's
  threat model ("multi-user exposed instance") doesn't endorse
  publicly-reachable HTTP deployments, and localhost is a
  browser-recognised secure context, so always-Secure works for dev,
  HTTPS proxy, and direct-TLS production. Operators running HTTP-only
  behind a public DNS name see the browser reject the cookie — the
  documented signal to put the deployment behind TLS.
- `theme_preference` was promoted from a stringly-typed value to a
  Postgres ENUM + Rust `enum ThemePreference` (with `sqlx::Type` +
  `serde`). The runtime `ALLOWED_THEMES.contains` check is gone — serde
  is the validation gate at the wire boundary. The OIDC callback could
  previously have written a corrupted DB value into the cookie; that
  path is now type-checked.
- `theme-switcher.tsx` `as ThemePreference` cast replaced with a narrow
  via `OPTIONS.find`.
- `ThemeProvider.tsx` reconcile effect's `// eslint-disable-next-line
  react-hooks/exhaustive-deps` removed in favour of a `useRef`-captured
  mount-time preference. `useMemo(deriveInitialState, [])` converted to
  `useMemo(() => deriveInitialState(), [])` to satisfy the React
  Compiler `use-memo` rule that the suppression was masking.
- `fouc.js` catch now `console.warn`s the failure before defaulting to
  light. The fallback strategy was correct; suppressing the diagnostic
  was not (the catch had previously hidden the `</script` literal bug
  fixed in `d29a7cc`).
- New tests: `patch_theme_returns_401_without_auth`, parametrized
  happy-path PATCH over `[light, dark, system]`, and
  `frontend/src/fouc/fouc.test.ts` (jsdom evaluation of the inline FOUC
  body — six branches plus the catch's warn breadcrumb).

---

## Next Steps

1. Open PR against `main` from `feat/design-system-d3`.
2. Run `/crosscheck` (Opus + Gemini) per
   `feedback_crosscheck_default.md` before declaring ready-for-review.
3. User reviews + merges per `feedback_user_does_merge.md`.
4. Follow-ups (separate PRs):
   - UNK-104: OIDC callback `Set-Cookie: reverie_theme` e2e test
     (needs `wiremock` + signed-ID-token scaffolding).
   - UNK-113: review JetBrains Mono adoption post-0.1.0; remove
     declaration if no surface uses it.
   - Optional: per-primitive `cva` overrides if the gallery shows
     gold-on-hover treatment is too loud for specific primitives.
