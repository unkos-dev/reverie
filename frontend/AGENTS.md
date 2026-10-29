# Frontend Agent Operating Manual

_(Operator detail: see `./README.md`)_

## Foundations

Vite+, React 19, React Aria Components, Tailwind v4, Vite+'s bundled Vitest and Playwright are Reverie's chosen tools.
Use their documented defaults. An alternative implementation needs evidence that the default is deficient or incorrect
for Reverie, with approval governed by the root rules. Existing code follows the same standard: an existing workaround
does not justify keeping it when the chosen tool meets the requirement.

- **Vendor docs first:** Before proposing a React Aria composition, prop or workaround, read the relevant component page
  at `https://react-aria.adobe.com/` and quote the guidance that supports it. Append `.md` to a page URL for Markdown
  (for example, `https://react-aria.adobe.com/Button.md`); the page's Copy for LLM action provides the same format. For
  forms, also read `https://react-aria.adobe.com/forms.md`. Types show what exists; docs show intended use. Read Vite+
  guidance at `https://viteplus.dev/guide/`. Prefer vendor Markdown pages when available.
- **Recipes, not tools:** Use `just js::test`, `just js::check`, `just js::a11y` and `just js::detect`. `vp test`,
  `vp build` and `vp lint` are Vite+ built-ins and ignore a `package.json` script of the same name; `vp run <script>`
  runs the script.

## Interaction

- **React Aria owns interaction:** Build interactive UI from the React Aria Components wrappers in `src/components/ui/`,
  styled with Reverie tokens and Tailwind data-attribute modifiers (`data-selected:`). Use the `ui/button` wrapper with
  `onPress`. Never run the shadcn CLI, overwrite existing wrappers with starter output, or reintroduce Radix UI or
  `cmdk`.
- **No stand-in controls:** Never make a `<div>` or `<span>` act as a control, and never rebuild focus, keyboard or
  dismissal behaviour that a React Aria component already provides.

## React

- **Function components:** Write components as functions. Component-level error boundaries use `react-error-boundary`;
  routes use React Router's `errorElement`.
- **Server state:** API calls live in `src/api/`, and lint bans `fetch` elsewhere. Fetching, caching and mutations go
  through React Query, never `useState` or `useEffect`.
- **Effects:** Keep dependency arrays complete and never suppress the rule. Use `useEffectEvent` for non-reactive event
  logic an Effect calls, never to drop a dependency that should resynchronise the Effect. Return a cleanup when the
  Effect starts something that must be stopped or undone. Never pass an async function directly. An async Effect for
  non-server state cancels obsolete work from its cleanup (`AbortController` where the API supports it, a staleness flag
  otherwise), suppresses only the expected cancellation error, and lets every other failure propagate. Give each Effect
  one lifecycle concern.
- **Error boundaries:** Every `<Suspense>` needs an error boundary above it. The route `errorElement`
  (`RootErrorBoundary`) counts. Add a local boundary only when a failure should stay inside part of the page, as
  `TableChunkBoundary` does.
- **Forms:** Use React Aria `Form` with uncontrolled fields, submitted through the `action` prop. Validation uses React
  Aria constraints, `validate` and `FieldError`. React resets uncontrolled fields once the action succeeds, so await the
  request inside the action and handle expected failures there: return the submitted values and server errors through
  `useActionState`, and feed them back as `defaultValue` and `validationErrors`. A thrown error reaches the nearest
  error boundary. Keep pending state until the request settles and preserve entered values and feedback after expected
  failures. Each existing form migration verifies pending state, failure feedback and retained values. Control a field
  only when its value drives other UI or needs realtime validation.
- **Client state:** Start with `useState` in the owning component, then composition, then lifting to the nearest common
  ancestor. Use Context only for low-frequency values such as theme and auth.
- **Memoisation:** The React Compiler lint rules are on and the compiler transform is off. Add `memo`, `useMemo` or
  `useCallback` only for a measured performance need, reference stability or an API contract. Leave existing memoisation
  in place unless you test the removal.
- **Logging:** No production `console.log` or `console.debug`. `console.error` belongs to error boundaries and degraded
  paths.

## TypeScript invariants

- **Lint is the authority:** The root `vite.config.ts` enforces type-aware strict rules, including no `any`, no non-null
  assertions, no unsafe member access, no object-literal type assertions and no array-index keys. Narrow `unknown` with
  type guards. Never suppress a rule to land a change.
- **Type definitions:** Use type aliases, as enforced by `typescript/consistent-type-definitions`. Derive vendor
  component props from their exported types.
- **Explicit returns on exports:** Exported functions declare their return type (`ReactElement` for components). Local
  helpers may rely on inference.
- **No enums:** `erasableSyntaxOnly` rejects them. Use `as const` objects and union types.
- **Type assertions:** Assert only to narrow at a boundary the type system cannot see, with the reason beside it.
- **Zod at boundaries:** Parse API response bodies, URL parameters and submitted `FormData` through a Zod schema before
  use.
- **Validate formats, not just types:** A response field with a declared format gets a validator for that format, not
  `z.string()`. Timestamps use bare `z.iso.datetime()`: the API emits `Z`-terminated RFC 3339, and `{ offset: true }`
  would also accept `+00:00`, which nothing produces. A malformed wire format once reached production through a
  `z.string()` field. Request schemas keep matching what the endpoint accepts, which can be looser.

## State ownership

- **One Writer Per Shared State:** Client state with more than one writer gets a single owner; components dispatch to
  it, never independently read-modify-write the same URL params or store slice.
- **Library visit state:** The Library page calls `useLibraryFilters` once, and that owner holds the published filters
  and view override. Editing surfaces receive its state and dispatch functions and never call the hook themselves. Build
  each patch from the value the dispatch passes in, not from rendered state, so sibling edits compose. The visit
  persists on the current Navigation entry (ADR-0066): never persist filters or the visit's view override anywhere else,
  and never add a filter timer or draft buffer. The account view preference is separate and keeps its own storage.
  Native range controls own unfinished input. Range endpoints are independent, and a complete inverted range stays
  entered and requests an empty result. `docs/specs/library/design/0001-library-filter-and-sort-state.md` records the
  full lifecycle, including how selected local days become API instants. Added-date bounds use local calendar
  arithmetic; never substitute 24 elapsed hours for the next calendar day.
- **Library Sort Is Not URL State:** Sort is a one-layer per-user preference
  (`docs/adr/0047-library-sort-is-a-per-user-preference-never-url-state.md`): every sort gesture writes `sort_stack`
  through the page's one intent handler, every surface reads the one resolved stack, and the list request carries the
  override explicitly. Never add a `?sort=` reader or writer to the library surface, never send the installation default
  sort in a request, and never resolve sort independently in a component: a second resolution is the read split that
  made an inherited descending sort unclearable.
- **Retained rows keep their source:** While previous results stay on screen, row edits, undo and conflict reload use
  the displayed rows' source query key, not the current one.

## Styling architecture

- **Tailwind v4 utilities and Reverie tokens:** Class names are complete static strings. Merge conditional and consumer
  classes with `cn` (`src/lib/utils.ts`), which resolves conflicts through `tailwind-merge`.
- **Inline styles for runtime values only:** Set a CSS variable inline and consume it with a utility (`w-(--x)`), or use
  `style` directly for a value computed at runtime. Theme-following colour comes from tokens; a raw colour value in TS
  or TSX belongs only to fixed brand artwork such as `Lockup`. Stylelint bans hex in CSS outside the generated and
  atmosphere tiers.
- **Theme tree** (`src/styles/themes/`):
  - `src/styles/themes/primitives.generated.css`: Generated from the brand anchors in
    `scripts/radix-gen/emit-primitives.ts`; regenerate with `vp run primitives:gen`. Never hand-edit it: a drift test
    pins the committed artifact to the emitter's output, so anchor or override changes happen in the emitter and ship
    with the regenerated file.
  - `src/styles/themes/index.css`: Semantic roles and compatibility aliases. Stylelint bans hex here.
  - `src/styles/themes/atmosphere.css`: Sealed art-directed tier.

## Design

`DESIGN.md` and `PRODUCT.md` are this package's design artifacts under the root design-authority rules.

- **Impeccable:** Use the `impeccable` skill for UI work within the design scope those rules authorise. Run
  `just js::detect` on UI changes; it is advisory and checks the source against `DESIGN.md`.
- **Rendered verification:** Inspect and screenshot the rendered page with the `playwright-cli` skill and compare it
  with the artifact. It is inspection tooling: Browser Mode tests, the Playwright journeys and the accessibility gate
  remain the automated test surfaces.
  - Run it as `vp exec --fail-if-no-match -- playwright-cli` from the repository root, wherever the skill writes
    `playwright-cli`. The skill is generated by `playwright-cli install --skills=agents`; refresh it that way, never by
    hand.
  - Open a named session (`-s=<name>`) and close only that session; never `close-all` or `kill-all`.
  - Keep sessions in memory. State that must be saved goes only under the ignored `.playwright-cli/`.
  - Never print cookie, token or storage-state values; check for presence only.

## Testing standards

- **Choose the environment by the assertion:** Browser-dependent component behaviour belongs in `tests/browser/`
  (`components-chromium`), browser-independent logic in a Node project, and document navigation in Playwright journeys
  under `tests/journeys/`. Never fabricate geometry, focus or browser APIs.
- **Browser Mode idiom:** Render with `vitest-browser-react`, query through locators, assert DOM state with
  `await expect.element(...)`, and interact through `userEvent` from `vite-plus/test/browser`.
- **Behaviour, not implementation:** Assert what a user can observe.
- **Accessibility gate:** CI runs Playwright and axe-core (WCAG 2.2 AA) over the shipped pre-auth routes in
  `scripts/a11y/allowlist.mjs`'s `DEFAULT_TARGETS`. Reproduce locally with `just js::a11y`, which reuses any server
  already on port 5173. Never narrow the WCAG tag ladder. An accepted violation needs a carve-out with its rationale in
  the same file. New scannable routes join `DEFAULT_TARGETS`; `A11Y_TARGETS` is a per-run override that CI does not set.
  A route qualifies only when it renders a single top-level `<main>` without an API, because the lane boots Vite alone:
  a mount-time query renders an error branch the scan would certify as passing.
