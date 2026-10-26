# Frontend Agent Operating Manual

_(Operator detail: see `./README.md`)_

<cardinal_rules>
These rules define the React and TypeScript architecture. Do not deviate.

1. **No Any:** `any` is forbidden. Use `unknown` and narrow with type guards.
2. **No Inline Styles or Hex:** Do not use inline style objects (except dynamic calcs) or arbitrary hex values. Use
   Tailwind utility classes and the established theme variables.
3. **React Aria foundation:** Compose new interactive primitives with React Aria Components and semantic HTML, styled
   with Reverie's Tailwind utilities and tokens. Preserve existing shadcn-derived components until their planned
   migration. Do not run the shadcn generator or overwrite React Aria wrappers with Radix or `cmdk` components. Radix
   Colors and the generated colour primitives remain independent of this interaction-library migration.
4. **No console.log:** Do not leave `console.log` statements in production code.
   </cardinal_rules>

<accessibility>
- **No Div-Buttons:** You are forbidden from adding `onClick` to non-interactive elements like `<div>` or `<span>`. Use
  semantic `<button>` tags; if you must use a non-interactive element, provide `role="button"`, `tabIndex={0}`, and
  keyboard handlers for both Enter and Space.
</accessibility>

<react_and_components>

- **Component Boundaries:** Keep data fetching in React Query hooks or owning components and API calls in `src/api/`.
  Split rendering from orchestration when it improves reuse, testing, or clarity; small cohesive components may own both
  local coordination and presentation.
- **Functional Only:** No class components.
- **Strict Effects:** `useEffect` must have a complete dependency array and return a cleanup function. Never suppress
  the linter or pass an async function directly. Server-state fetching belongs in React Query, not an Effect. A rare
  async Effect for non-server-state synchronization must cancel obsolete work from its cleanup (`AbortController` when
  the API supports it, a staleness flag otherwise), suppress only the expected cancellation error, and let every other
  failure propagate.
- **Parallel Async:** Run independent async work concurrently when ordering, rate, and resource constraints permit. Use
  `Promise.all()` to fail fast or `Promise.allSettled()` when one failure should not abort the others.
- **Cohesive Effects:** Keep each `useEffect` focused on one lifecycle concern. Closely coupled setup and cleanup may
  share an effect; unrelated work should not be combined merely to reduce line count.
- **Suspense & Error Boundaries:** Every `<Suspense>` boundary MUST have an `<ErrorBoundary>` above it. The pair handles
  both states.
- **Forms (Uncontrolled Default):** Prefer uncontrolled inputs using `FormData` when the form has a clear submit step.
  Only use controlled inputs (`useState`) when the value drives other UI or requires real-time validation.
- **No Complex JSX:** Extract complex inline JSX expressions to variables or helper functions for readability.
- **Routing:** This is a Vite SPA using React Router. Do not use Next.js imports (e.g., `next/navigation`).
  </react_and_components>

<typescript_invariants>

- **Strict Mode:** No `!` non-null assertions. Use optional chaining (`?.`) or explicit null checks.
- **Props Definition:** Prefer `type Props = {}` for closed component prop shapes. Use `interface` ONLY when the prop
  type is explicitly meant to be extended.
- **No Enums:** Prefer `as const` objects and union types (`type X = typeof X[keyof typeof X]`).
- **Explicit Returns:** Public functions must have explicit return types (`ReactElement`, not `JSX.Element` which is
  deprecated in React 19).
- **No `as` Casts:** Type assertions are forbidden unless narrowing from `unknown` with a documented reason.
- **Zod Boundaries:** All API response bodies, URL parameters, and form inputs must be parsed through a Zod schema
  before use.
- **Validate Formats, Not Just Types:** A response field with a declared format gets a validator for that format, not
  `z.string()`. Timestamps use bare `z.iso.datetime()`: the API emits `Z`-terminated RFC 3339, and the
  `{ offset: true }` variant would also accept `+00:00`, which nothing produces. `z.string()` accepts any serialization
  regression that happens to be a string, which is how a malformed wire format reached production once already. This
  applies to response schemas; a request field must keep matching what the endpoint accepts, which can be looser.
  </typescript_invariants>

<state_and_data>

- **Server State (React Query):** You MUST use `@tanstack/react-query` for all server state (fetching, caching,
  mutations). NEVER use `useState` or `useEffect` for data fetching. API calls live in `src/api/`.
- **Client State Decision Tree:**
  1. Used by one component -> `useState` inside it.
  2. Used by parent + children -> First, attempt Component Composition (passing UI as `children` or `slots`). If that
     fails, lift state to the nearest common ancestor.
  3. Distant branches -> Context, but for **low-frequency reads only** (theme, auth). Do not use Context for
     high-frequency updates, as it triggers massive re-renders.
- **Performance:** Use `React.memo`, `useMemo`, or `useCallback` when profiling, reference stability, or an API contract
  justifies it; remove cargo-cult memoization. List `key` values must be stable and unique (never use array index).
  </state_and_data>

<state_ownership>

- **One Writer Per Shared State:** Client state with more than one writer gets a single owner; components dispatch to
  it, never independently read-modify-write the same URL params or store slice.
- **Library Visit State:** One page-level `useLibraryFilters` owner holds published filters and the optional view
  override after Router setup. Persist with `navigation.updateCurrentEntry`, preserving unrelated entry data. Reload and
  traversal restore the destination; fresh entries start clean even when the page remains mounted. Filter edits do not
  navigate. All editing surfaces receive state and dispatch functions from the page.
- **Library Sort Is Not URL State:** Sort is a one-layer per-user preference
  (`docs/adr/0047-library-sort-is-a-per-user-preference-never-url-state.md`): every sort gesture writes `sort_stack`
  through the page's one intent handler, every surface reads the one resolved stack, and the list request carries the
  override explicitly. Never add a `?sort=` reader or writer to the library surface, never send the installation default
  sort in a request, and never resolve sort independently in a component: a second resolution is the read split that
  made an inherited descending sort unclearable.
- **Compose Current Intent:** Apply edits against the current visit snapshot so sibling edits and blur/Clear compose.
  Clear removes filters while preserving view and sort. No Library filter timer, custom draft buffer, compatibility URL
  parser or second persistence store is permitted.
- **Native Input Defaults:** Use supported React Aria composition and defaults. NumberFields use integer steps and
  intrinsic domains, without peer constraints. Optional DatePickers retain segments and calendars. Native unfinished
  input stays with its control; complete inverted intervals remain entered and request a true empty result.
- **Local Added Days:** Persist calendar dates. Project the lower local midnight and the upper day's following local
  midnight into inclusive `created_at_gte` and exclusive `created_at_lt` instants. Add a calendar day before timezone
  conversion; never add 24 elapsed hours.
- **Displayed Cache Provenance:** Retained rows use their source query key for edits, undo and conflict reload. Current
  intent clears selection, and placeholder pages cannot load more. </state_ownership>

<styling_architecture>

- **Tailwind v4:** Use utility classes.
- **Theme Tree:**
  - `primitives.generated.css`: Generated from the brand anchors in `scripts/radix-gen/emit-primitives.ts`; regenerate
    with `vp run primitives:gen`. NEVER HAND-EDIT: a drift test pins the committed artifact to the emitter's output, so
    anchor or override changes happen in the emitter and ship with the regenerated file.
  - `index.css`: Semantic roles and shadcn aliases. Hex values are banned here by stylelint.
  - `atmosphere.css`: Sealed art-directed tier.
- **Class Merging:** Use `clsx` or `cn` for conditional classes.
  </styling_architecture>

<testing_standards>

- **Behavioural Testing:** Use Vitest Browser Mode with `vitest-browser-react` for browser-dependent components. Keep
  React Testing Library for remaining jsdom coverage. Test behaviour, not implementation details.
- **Accessibility Gate:** CI runs Playwright + axe-core (WCAG 2.2 AA) over the shipped pre-auth routes in
  `scripts/a11y/allowlist.mjs`'s `DEFAULT_TARGETS`; reproduce locally with `vp run a11y` (Playwright owns the dev-server
  lifecycle). Never narrow the WCAG tag ladder. An accepted violation requires a rationale-bearing carve-out in the same
  file; new scannable routes join `DEFAULT_TARGETS` (the `A11Y_TARGETS` env var is a per-run override that CI does not
  set). A route qualifies only when it renders a single top-level `<main>` without an API, since the lane boots Vite
  alone: a mount-time query renders an error branch the scan would certify as passing. </testing_standards>
