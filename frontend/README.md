# Frontend

This directory contains the Vite React frontend.

## Testing

Run `just js::test` from the repository root to execute all frontend test projects, including Chromium component tests.
After the lockfile-backed `just install`, provision the browser from `frontend/`:

```bash
vp exec node ./node_modules/playwright/cli.js install chromium
```

Repeat this command after updating Playwright. It uses the directly declared `playwright` package consumed by the
Browser Mode provider, independently of the accessibility runner's version. Chromium also needs the system libraries
listed in [Playwright's browser setup](https://playwright.dev/docs/browsers). If launch reports missing libraries, stop
and arrange their installation; the test gate does not install browsers or system packages. CI installs Chromium and its
system dependencies before running the same test recipe, caching binaries by OS, architecture and installed Playwright
version.

Choose the environment by the assertion:

- Browser-dependent component behaviour belongs in `tests/browser/**/*.test.tsx`, in the `components-chromium` project.
  Use native keyboard, focus, layout, scrolling and ResizeObserver behaviour. The dialog, UserMenu and LibraryTableView
  suites are working examples. Do not import the jsdom setup or fabricate geometry and focus.
- Browser-independent logic belongs in a Node project. The existing `vite-plugins`, `radix-gen` and `a11y` projects
  demonstrate Node configuration; classify source tests before moving them.
- Actual document navigation belongs in standalone browser journeys. Component tests run inside an embedded Vitest
  frame; navigation can destroy that frame. They do not establish the final logout destination or session invalidation.

The `frontend` jsdom project is temporary migration coverage. Keep its required assertions until each has an equivalent
home: browser-dependent outcomes in Browser Mode, pure logic in Node, and actual navigation in standalone journeys.
Remove superseded jsdom cases as those outcomes are verified, then remove jsdom and its geometry, focus and browser API
stubs once no required assertion depends on them. The representative browser suites retain their source counterparts
during this transition; expanding duplicate suites is not the migration strategy. Standalone journeys and complete suite
conversion are separate work.

From `frontend/`, run `vp test run --project components-chromium` for focused browser work or
`vp test --project components-chromium --browser.headless=false` for interactive debugging. Browser matcher types live
in `tsconfig.browser.json`; Node and app/jsdom declarations remain separate. The frontend lint gate checks browser files
along with the rest of the package.

The component project owns an isolated Vite test host with application styles. It does not inherit application CSP
plugins, response headers, backend proxies or HTML processing, and it does not prove those controls work in the shipped
application. Keep the CSP plugin tests, production build and accessibility gate intact. Browser failures include runner
output and automatic screenshots under `frontend/test-results/browser/` when capture is possible; CI uploads that
directory as `component-browser-failures`. A browser launch failure may have no screenshot.

## Interactive components

React Aria Components owns buttons, selection controls, disclosures, tabs, progress, overlays, the command palette and
vocabulary pickers. Wrappers in `src/components/ui/` use Reverie's Tailwind classes and theme tokens. Prefer native
interaction defaults and keep API queries and domain state in their owning components.

Vocabulary pickers supply server suggestions to the `ComboBox` collection and render hydrated selections inside
`ComboBoxValue` separately. Selection order follows the selected keys, not server ranking. Multiple selection keeps the
native popup lifecycle. The command palette uses `MenuItem` links with native routing and closure. Arrow navigation
retains input focus through `Autocomplete`; Tab traverses result links. Scrolling belongs to the `Menu`. Dialog and
select labels belong to their native parent composition.

Active Library filters use `TagGroup` and `TagList` for removal and focus recovery. The collection stays mounted when
empty and shows `No active filters.`; Clear all stays mounted and disabled. View and density use nonempty single
selection in native `ToggleButtonGroup` controls, with changes dispatched to the existing URL and preference owners.

Create-user role and token expiry use named, uncontrolled Selects. Submission reads and validates their FormData values;
failed requests retain the choices, and reopening a closed dialog restores the defaults.

Selection controls compose native fields and buttons with labelled content. Use `isSelected`, `isReadOnly`, `isDisabled`
and `isPending` for their native states. Disclosure retains filter drafts while collapsed; ingestion mounts query
children only while expanded. Book tabs activate from arrow keys and mount only the selected panel. Buttons styled as
navigation links use React Router Link with `buttonVariants`.

Token receipt fields use native `TextField`, `Label` and `Input` composition for their read-only values and label
associations. The supported `render` prop retains the shared field styling.

Refine dismissal retains valid filter edits for Escape, close and outside clicks. Columns Reset closes the popover and
returns focus through native dismissal. Settings actions are disabled when pristine; Save stays in the tab order while
pending.

Radix Colors and `src/styles/themes/primitives.generated.css` supply the colour foundation independently of the
interaction library. The Radix UI runtime is not installed. The grid engine and its editing, sorting and loading state
remain separate from the shared controls.

## Native form validation

React Aria fields own validation messages, their associations and first-invalid-field focus. Domain validators and API
mutations remain in their owning modules. Settings retains its drafts, dirty patches and concurrency handling;
validation appears beside each field. General API failures remain alerts.

## Environment Variables

Configure the development environment using these variables.

**REVERIE_DEV_HOSTS** Vite rejects unknown non-loopback hostnames to prevent DNS rebinding. Loopback hosts work without
configuration. Set `REVERIE_DEV_HOSTS` to a comma-separated list of hostnames when using cloud development environments
like Coder or Codespaces. The `vite-plugins/allowed-hosts.ts` script parses this value. The `vite-plugins/dev-csp.ts`
script adds each hostname to the `connect-src` policy, allowing the HMR websocket to connect through TLS. Format
hostnames without schemes, paths, whitespace, or semicolons.

**REVERIE_DEV_HMR_CLIENT_PORT** The HMR websocket client defaults to port 5173. Set `REVERIE_DEV_HMR_CLIENT_PORT` to 443
when a reverse proxy fronts the dev server. The `vite-plugins/hmr-config.ts` script reads this value.

## Project Structure

```text
frontend/
├── public/              # Static assets
├── src/
│   ├── api/             # API client functions
│   ├── components/      # Reusable UI components
│   │   └── ui/          # Token-styled interaction primitives
│   ├── fouc/            # Pre-paint script hashed into HTML CSP at build
│   ├── hooks/           # Custom React hooks
│   ├── pages/           # Route-level page components
│   ├── routes/          # Lazy route modules (incl. pre-auth /login, /setup, /forgot-password)
│   ├── lib/             # Utilities
│   ├── App.tsx          # Root component
│   └── main.tsx         # Entrypoint
├── vite-plugins/        # Custom Vite plugins
├── tests/               # Vitest setup
├── index.html
├── tsconfig.json
└── vite.config.ts       # Tailwind v4 and Vitest configuration
```
