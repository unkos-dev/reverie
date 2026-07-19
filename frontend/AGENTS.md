# Frontend Agent Operating Manual

_(Operator detail: see `./README.md`)_

<cardinal_rules>
These rules define the React and TypeScript architecture. Do not deviate.

1. **No Any:** `any` is forbidden. Use `unknown` and narrow with type guards.
2. **No Inline Styles or Hex:** Do not use inline style objects (except dynamic calcs) or arbitrary hex values. Use Tailwind utility classes and the established theme variables.
3. **shadcn via CLI only:** Do not manually create or paste shadcn/ui components. Run `npx shadcn@latest add <component>`.
4. **No console.log:** Do not leave `console.log` statements in production code.
   </cardinal_rules>

<accessibility>
- **No Div-Buttons:** You are forbidden from adding `onClick` to non-interactive elements like `<div>` or `<span>`. Use semantic `<button>` tags; if you must use a non-interactive element, provide `role="button"`, `tabIndex={0}`, and keyboard handlers for both Enter and Space.
</accessibility>

<react_and_components>

- **Component Boundaries:** Keep data fetching in React Query hooks or owning components and API calls in `src/api/`. Split rendering from orchestration when it improves reuse, testing, or clarity; small cohesive components may own both local coordination and presentation.
- **Functional Only:** No class components.
- **Strict Effects:** `useEffect` must have a complete dependency array. Never suppress the linter. Never pass an async function directly; use the `AbortController` pattern:

  ```tsx
  useEffect(() => {
    const controller = new AbortController();
    const load = async () => {
      try {
        const data = await fetchData({ signal: controller.signal });
        setData(data);
      } catch (error) {
        if (!controller.signal.aborted) setError(error);
      }
    };
    load();
    return () => controller.abort();
  }, []);
  ```

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
- **Library URL State:** All library filter, sort, and view URL writes flow through the page-owned write authority (`useLiveSearchParams`). Calling `setSearchParams` directly for library state from a component is a defect.
- **Debounced Writes Re-Validate at Fire Time:** A due timer fires before the render that would cancel it (React renders ride scheduler tasks that lose to due timers), so a debounced write into shared state must check it is still valid when it fires, not only when scheduled.
  </state_ownership>

<styling_architecture>

- **Tailwind v4:** Use utility classes.
- **Theme Tree:**
  - `primitives.generated.css`: Generated from brand anchors. NEVER HAND-EDIT.
  - `index.css`: Semantic roles and shadcn aliases. Hex values are banned here by stylelint.
  - `atmosphere.css`: Sealed art-directed tier.
- **Class Merging:** Use `clsx` or `cn` for conditional classes.
  </styling_architecture>

<testing_standards>

- **Behavioral Testing:** Use Vitest + React Testing Library. Test behavior, not implementation details.
  </testing_standards>
