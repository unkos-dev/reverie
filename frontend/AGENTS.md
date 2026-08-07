# Frontend Agent Operating Manual

_(Operator detail: see `./README.md`)_

<cardinal_rules>
These rules define the React and TypeScript architecture. Do not deviate.

1. **No Any:** `any` is forbidden. Use `unknown` and narrow with type guards.
2. **No Inline Styles or Hex:** Do not use inline style objects (except dynamic calcs) or arbitrary hex values. Use Tailwind utility classes and the established theme variables.
3. **shadcn via CLI only:** Do not manually create or paste shadcn/ui components. Run `npx shadcn@latest add <component>` from `frontend/` so the CLI reads the workspace `components.json`. The CLI is deliberately not a dependency: it is invoked only when adding a primitive, while retaining it adds a persistent package and advisory surface to every development install. Review and commit all generated source.
4. **No console.log:** Do not leave `console.log` statements in production code.
   </cardinal_rules>

<accessibility>
- **No Div-Buttons:** You are forbidden from adding `onClick` to non-interactive elements like `<div>` or `<span>`. Use semantic `<button>` tags; if you must use a non-interactive element, provide `role="button"`, `tabIndex={0}`, and keyboard handlers for both Enter and Space.
</accessibility>

<react_and_components>

- **Component Boundaries:** Keep data fetching in React Query hooks or owning components and API calls in `src/api/`. Split rendering from orchestration when it improves reuse, testing, or clarity; small cohesive components may own both local coordination and presentation.
- **Functional Only:** No class components.
- **Strict Effects:** `useEffect` must have a complete dependency array and return a cleanup function. Never suppress the linter or pass an async function directly. Server-state fetching belongs in React Query, not an Effect. A rare async Effect for non-server-state synchronization must cancel obsolete work from its cleanup (`AbortController` when the API supports it, a staleness flag otherwise), suppress only the expected cancellation error, and let every other failure propagate.

- **Parallel Async:** Run independent async work concurrently when ordering, rate, and resource constraints permit. Use `Promise.all()` to fail fast or `Promise.allSettled()` when one failure should not abort the others.
- **Cohesive Effects:** Keep each `useEffect` focused on one lifecycle concern. Closely coupled setup and cleanup may share an effect; unrelated work should not be combined merely to reduce line count.
- **Suspense & Error Boundaries:** Every `<Suspense>` boundary MUST have an `<ErrorBoundary>` above it. The pair handles both states.
- **Forms (Uncontrolled Default):** Prefer uncontrolled inputs using `FormData` when the form has a clear submit step. Only use controlled inputs (`useState`) when the value drives other UI or requires real-time validation.
- **No Complex JSX:** Extract complex inline JSX expressions to variables or helper functions for readability.
- **Routing:** This is a Vite SPA using React Router. Do not use Next.js imports (e.g., `next/navigation`).
  </react_and_components>

<typescript_invariants>

- **Strict Mode:** No `!` non-null assertions. Use optional chaining (`?.`) or explicit null checks.
- **Props Definition:** Prefer `type Props = {}` for closed component prop shapes. Use `interface` ONLY when the prop type is explicitly meant to be extended.
- **No Enums:** Prefer `as const` objects and union types (`type X = typeof X[keyof typeof X]`).
- **Explicit Returns:** Public functions must have explicit return types (`ReactElement`, not `JSX.Element` which is deprecated in React 19).
- **No `as` Casts:** Type assertions are forbidden unless narrowing from `unknown` with a documented reason.
- **Zod Boundaries:** All API response bodies, URL parameters, and form inputs must be parsed through a Zod schema before use.
- **Validate Formats, Not Just Types:** A response field with a declared format gets a validator for that format, not `z.string()`. Timestamps use bare `z.iso.datetime()`: the API emits `Z`-terminated RFC 3339, and the `{ offset: true }` variant would also accept `+00:00`, which nothing produces. `z.string()` accepts any serialization regression that happens to be a string, which is how a malformed wire format reached production once already. This applies to response schemas; a request field must keep matching what the endpoint accepts, which can be looser.
  </typescript_invariants>

<state_and_data>

- **Server State (React Query):** You MUST use `@tanstack/react-query` for all server state (fetching, caching, mutations). NEVER use `useState` or `useEffect` for data fetching. API calls live in `src/api/`.
- **Client State Decision Tree:**
  1. Used by one component -> `useState` inside it.
  2. Used by parent + children -> First, attempt Component Composition (passing UI as `children` or `slots`). If that fails, lift state to the nearest common ancestor.
  3. Distant branches -> Context, but for **low-frequency reads only** (theme, auth). Do not use Context for high-frequency updates, as it triggers massive re-renders.
- **Performance:** Use `React.memo`, `useMemo`, or `useCallback` when profiling, reference stability, or an API contract justifies it; remove cargo-cult memoization. List `key` values must be stable and unique (never use array index).
  </state_and_data>

<state_ownership>

- **One Writer Per Shared State:** Client state with more than one writer gets a single owner; components dispatch to it, never independently read-modify-write the same URL params or store slice.
- **Library URL State:** The library's filter, sort, and view state lives in typed per-key search params (`lib/hooks/use-library-filters.ts`). Every read and every write on that surface goes through it. React Router's `useSearchParams` is banned there by lint, in both directions: those writes reach the URL through `history.replaceState`, which the router's history does not observe, so a read through it returns filter state that goes stale as soon as anything writes, and a write through it lands where the surface cannot read it.
- **A Gesture Writes Only Its Own Keys:** Each editing surface owns a slice and writes that slice alone. Writing the whole grammar on every commit pushes a sibling slice's current value into that slice's pending write, overwriting an edit still being typed.
- **Clear Affordances Are Undebounced Writes:** A write that does not ask to be debounced cancels the pending debounced write on the same keys before queueing its own. That is what stops a queued keystroke resurrecting a condition a clear just removed, which is why no timer re-checks validity when it fires and no generation counter exists for it to check.
- **Filters Live in the URL for Their Lifetime, Not for Sharing:** They must survive a refresh and in-app navigation, but must not survive into a fresh visit days later, and no other medium has that shape without extra machinery. Link sharing is not the reason; this is a self-hosted household instance.
  </state_ownership>

<styling_architecture>

- **Tailwind v4:** Use utility classes.
- **Theme Tree:**
  - `primitives.generated.css`: Generated from the brand anchors in `scripts/radix-gen/emit-primitives.ts`; regenerate with `npm run primitives:gen`. NEVER HAND-EDIT: a drift test pins the committed artifact to the emitter's output, so anchor or override changes happen in the emitter and ship with the regenerated file.
  - `index.css`: Semantic roles and shadcn aliases. Hex values are banned here by stylelint.
  - `atmosphere.css`: Sealed art-directed tier.
- **Class Merging:** Use `clsx` or `cn` for conditional classes.
  </styling_architecture>

<testing_standards>

- **Behavioral Testing:** Use Vitest + React Testing Library. Test behavior, not implementation details.
- **Accessibility Gate:** CI runs Playwright + axe-core (WCAG 2.2 AA) over the dev-only design showcase; reproduce locally with `npm run a11y` (Playwright owns the dev-server lifecycle). Never narrow the WCAG tag ladder. An accepted violation requires a rationale-bearing carve-out in `scripts/a11y/allowlist.mjs`; new scannable routes join the default target list in the same file's `parseTargets` (the `A11Y_TARGETS` env var is a per-run override that CI does not set).
  </testing_standards>
