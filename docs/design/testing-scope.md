# Testing Scope for the Design System

Browser-dependent component behaviour runs in Vitest Browser Mode with `vitest-browser-react`. The suites under
`frontend/tests/browser/` cover native controls, focus, keyboard interaction and pending states. Browser-independent
logic and build plugins run in Node. Application navigation runs against production builds in Chromium and Firefox
through `just js::library-journeys`.

The CSP-hash plugin tests remain under `frontend/vite-plugins/__tests__/csp-hash.test.ts`. The backend's cookie response
contract is covered by `backend/tests/cookie_jar_sanity.rs`.

Visual acceptance requires rendered inspection against `frontend/DESIGN.md`, including light and dark themes and the
applicable viewport sizes. Component assertions do not replace that inspection. Playwright and axe-core check the
shipped routes listed in `frontend/scripts/a11y/allowlist.mjs` through `just js::a11y`.

See the [frontend testing guide](../../frontend/README.md#testing) for setup and environment ownership, and the
[Browser Mode decision](../adr/0065-browser-mode-for-browser-dependent-component-tests.md) for the testing boundaries.
