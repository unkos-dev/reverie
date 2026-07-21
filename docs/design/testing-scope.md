# Testing Scope for the Design System

The Step 10 design system is tested at two distinct bars.

## What ships with deterministic unit tests

- **Today (D0–D2):**
  - `Lockup` brand component: React Testing Library coverage of the
    wordmark variants and accessibility contract
    (`frontend/src/components/Lockup.test.tsx`).
  - The CSP-hash Vite plugin: node-environment unit tests under
    `frontend/vite-plugins/__tests__/csp-hash.test.ts`.
  - The `axum_extra` `CookieJar` tuple-response contract that the backend
    design-system sliver (theme PATCH, OIDC callback cookie seed) depends
    on: `backend/tests/cookie_jar_sanity.rs`.
  - Vitest harness validation: `frontend/src/App.test.tsx` asserts the
    application renders under the jsdom test project (the earlier
    `smoke.test.ts` placeholder it replaced is gone).
- **D3–D5 acceptance bar (added before the relevant feature lands):**
  - Theme provider: initial resolution from cookie/DB/`prefers-color-scheme`,
    persistence, and API sync.
  - Theme cookie helpers (read, write, expiry).
  - Route-gating production-build structural assertion (the `/design/*`
    explore tree is excluded from production bundles).

## What is exempt from unit tests

Visual and composition work is verified by:

- Playwright with `@axe-core/playwright` against `/design/system`
  (`npm run a11y`; targets default in
  `frontend/scripts/a11y/allowlist.mjs`).
- Manual Dark/Light toggle.
- The `/crosscheck` dual-model review gate at D5.

Applied-page Lighthouse gates are Step 11's responsibility.

This split is deliberate. Snapshotting visual output in per-component unit
tests locks styling into a pixel-brittle contract that Step 11 cannot move.
The design system's acceptance bar is the axe-core hard gate plus a human
review; per-component visual regression tooling is not adopted.
