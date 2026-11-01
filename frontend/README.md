# Frontend

This directory contains the Vite React frontend.

For UI work, install the Impeccable skill at user scope for Claude Code and Codex using the
[upstream setup instructions](https://impeccable.style/tutorials/getting-started/), then reload the coding tool. The
skill is a separate prerequisite: `just install` installs the detector dependency, not the skill. Reverie's design
artifacts and design-authority rules govern its use.

## Testing

Run `just js::test` from the repository root to execute all frontend test projects, running Vitest first and then the
same application navigation journeys used by CI. After the lockfile-backed `just install`, provision the browser from
`frontend/`:

```bash
vp exec node ./node_modules/playwright/cli.js install chromium
vp exec node ./node_modules/@playwright/test/cli.js install chromium firefox
```

Repeat these commands after updating Playwright. The first uses the directly declared `playwright` package consumed by
the Browser Mode provider; the second uses the journey runner's installed version. Chromium also needs the system
libraries listed in [Playwright's browser setup](https://playwright.dev/docs/browsers). If launch reports missing
libraries, stop and arrange their installation; the test gate does not install browsers or system packages. CI
provisions component Chromium plus journey Chromium/Firefox with their system dependencies. Its browser cache key
includes OS, architecture and both installed Playwright versions.

Choose the environment by the assertion:

- Browser-dependent component behaviour belongs in `tests/browser/**/*.test.tsx`, in the `components-chromium` project.
  Render with `vitest-browser-react`, query with locators and assert DOM state with `await expect.element(...)`. Use
  `userEvent` from `vite-plus/test/browser` for native keyboard and pointer interaction, with real focus, layout,
  scrolling and ResizeObserver behaviour. The renderer owns component cleanup before each test. Do not fabricate
  geometry, focus or browser APIs.
- Browser-independent source logic belongs in `frontend-node`. The `vite-plugins`, `radix-gen` and `a11y` projects cover
  their respective tooling.
- Actual document navigation belongs in standalone browser journeys under `tests/journeys/`. Component tests run inside
  an embedded Vitest frame; navigation can destroy that frame. They do not establish the final logout destination or
  session invalidation.

From `frontend/`, run `vp test run --project components-chromium` for focused browser work or
`vp test --project components-chromium --browser.headless=false` for interactive debugging. Browser matcher types live
in `tsconfig.browser.json`; Node and application declarations remain separate. The frontend lint gate checks browser
files along with the rest of the package.

The component project owns an isolated Vite test host with application styles. It does not inherit application CSP
plugins, response headers, backend proxies or HTML processing, and it does not prove those controls work in the shipped
application. Keep the CSP plugin tests, production build and accessibility gate intact. Browser failures include runner
output and automatic screenshots under `frontend/test-results/browser/` when capture is possible; CI uploads that
directory alongside `test-results/library/` journey traces as `browser-failures`. A browser launch failure may have no
screenshot.

The `frontend-node` project includes pure Library projection and summary tests. Native date input binding lives in
`tests/browser/editors.test.tsx`; the Library browser suites cover query, cache and selection behaviour.
`just js::library-journeys` runs `tests/journeys/*.spec.ts` through `playwright.library.config.ts` in Chromium and
patched Firefox. Each invocation builds through `just js::build`, then serves the production output with `vp preview` to
exclude development tooling. It owns a strict loopback server on port 5174 with reuse disabled and intercepts synthetic
APIs through the real application bootstrap and Router. It needs no database or real login credentials. It proves
navigation and client request/result integration, not actual authentication or database predicates. The WCAG scan runs
separately through `just js::a11y`. Missing browser system libraries stop local verification; local gates never install
them.

For Library visit persistence and local-day projection, see the
[Library state Design](../docs/specs/library/design/0001-library-filter-and-sort-state.md) and
[Navigation-entry decision](../docs/adr/0066-library-visit-state-and-local-day-filter-boundaries.md).

## Application behaviour

The [Accessible application controls Design](../docs/specs/accessibility/design/0030-accessible-application-controls.md)
records field composition, form actions, pending and failure states, selection controls, overlays and command-palette
navigation. The [Library state Design](../docs/specs/library/design/0001-library-filter-and-sort-state.md) covers filter
ownership, vocabulary selection, visit restoration and table-load recovery.

Implementation rules live in [AGENTS.md](AGENTS.md). Visual and interaction authority remains in
[DESIGN.md](DESIGN.md).

## Environment Variables

Configure the development environment using these variables.

**REVERIE_DEV_HOSTS** Vite rejects unknown non-loopback hostnames to prevent DNS rebinding. Loopback hosts work without
configuration. Set `REVERIE_DEV_HOSTS` to a comma-separated list of hostnames when using cloud development environments
like Coder or Codespaces. The `vite-plugins/allowed-hosts.ts` script parses this value. The `vite-plugins/dev-csp.ts`
script adds each hostname to the `connect-src` policy, allowing the HMR websocket to connect through TLS. Format
hostnames without schemes, paths, whitespace, or semicolons.

**REVERIE_DEV_HMR_CLIENT_PORT** The HMR websocket client defaults to port 5173. Set `REVERIE_DEV_HMR_CLIENT_PORT` to 443
when a reverse proxy fronts the dev server. The `vite-plugins/hmr-config.ts` script reads this value.
