---
status: accepted
date: 2026-05-22
decision-makers: john
---

# Frontend data-layer dependencies for Step 11

## Context and Problem Statement

Step 11 of the Reverie blueprint (UNK-80) introduces the
production browser UI on top of three new JSON endpoints
(`/api/books`, `/api/books/{id}`, `/api/works/{id}`) in
Sub-phase 11a, followed by search (11b), metadata curation
(11c), shelves and series (11d), admin (11e), and persisted
settings (11f). The frontend today is a 18-file Vite + React 19

- Tailwind v4 + shadcn (radix-nova) tree with no data-fetching
  library: the only network calls are theme persistence and OPDS
  testing helpers, all written as one-off `fetch()` calls. There
  is no centralised cache, no request deduplication, no loading
  state machine, no mutation-invalidation framework.

CLAUDE.md's proactive ADR trigger requires this document
("write ADR before new npm package"). Sub-phase 11a Task 9 adds
`@tanstack/react-query` + `@tanstack/react-query-devtools`;
Sub-phase 11b Task 6 adds `@tanstack/react-table` (data-table
list view, optional); Sub-phase 11d Task 6 adds
`@dnd-kit/sortable` (shelf-item drag-to-reorder). All four are
covered in this ADR because their adoption decision is the same
shape (TanStack-ecosystem npm packages with overlapping
maintainership and identical supply-chain audit posture); a
shared ADR is more useful than four micro-ADRs.

The decision before us is: **adopt each of these four packages,
versioned at the latest major, with documented alternatives that
were considered and rejected**.

## Decision

### `@tanstack/react-query` (Sub-phase 11a, REQUIRED)

Adopt `@tanstack/react-query@^5`. Pinned under
`dependencies` in `frontend/package.json`. Registered via a
`QueryClient` singleton in `frontend/src/lib/query/client.ts`;
mounted via `QueryClientProvider` in `frontend/src/main.tsx`.

Why this and not vanilla `fetch()` + `useEffect`:

- Request deduplication across components. The Library page and
  the global Cmd-K palette (11b) both call `/api/books`; one
  network round-trip serves both.
- Cache invalidation on mutation. 11c's accept/reject metadata
  mutation invalidates `['books', 'detail', id]` and `['books',
'list']`; the next render of either component refetches
  automatically. Hand-rolling this state machine is the dominant
  cost of fetch-on-mount + manual refetch.
- Suspense integration. `useSuspenseQuery` works with React 19
  Suspense for skeleton-fallback loading without rolling our own
  loading-state machine in every component.
- Route loader prefetch. React Router v7 data mode loaders call
  `queryClient.prefetchQuery` so the cache is hot when the
  component mounts. This is the
  [official Tkdodo pattern documented by react-router upstream](https://reactrouter.com/start/data/data-loading),
  not a community workaround.

Why v5 specifically: v4 is in maintenance mode (v5 shipped
October 2023); the `QueryCache({ onError })` 401-handler pattern
the plan adopts requires v5 (v4 placed the handler on
`defaultOptions.queries.onError`, which v5 removed). React 19
compatibility is documented for v5.51+.

### `@tanstack/react-query-devtools` (Sub-phase 11a, REQUIRED)

Adopt `@tanstack/react-query-devtools@^5` (matching the
runtime version) under `devDependencies`. Mount via a
dynamic `import.meta.env.DEV`-gated import in `main.tsx`,
mirroring the existing `designRoutes` pattern. Cost: zero bytes
in production bundle (Vite tree-shakes the gated import); ~120KB
on the dev bundle; one extra devDependency.

Why this is worth the dev-bundle cost: react-query's cache state
is the most cross-cutting state in the app (every page reads from
it, every mutation writes to it). Inspecting cache entries by key
at runtime is the difference between a 5-minute debug and a
30-minute one when invalidation is wrong. Devtools is the
TanStack-blessed inspector; no third-party tool covers the same
surface.

### `@tanstack/react-table` (Sub-phase 11b, OPTIONAL)

Adopt `@tanstack/react-table@^8` under `dependencies` **only**
if Sub-phase 11a's `LibraryPage` ships the list view
(`?view=list`) at parity with the dev hero. If the list view
defers to 11b, the dependency lands then; if Sub-phase 11a
implements `?view=list` with a hand-rolled `<table>`, the
dependency may never land at all.

Why react-table when the list view is built: server-side
pagination semantics in shadcn's data-table component require
`manualPagination: true` + skipping `getPaginationRowModel`;
react-table is the only library with first-class shadcn
integration documentation. Hand-rolling a sortable + paginated
table in TypeScript with shadcn primitives costs more LOC than
the dependency.

Conditional adoption resolved at 11a's `LibraryPage`
implementation review. If adopted, this ADR's verification
checklist gates the package landing.

### `@dnd-kit/sortable` (Sub-phase 11d, REQUIRED)

Adopt `@dnd-kit/sortable@^8` under `dependencies`, alongside
`@dnd-kit/core@^6` (transitive but pinned directly so version
bumps surface in `npm install`) and `@dnd-kit/utilities@^3`
(CSS-transform helpers used by `ShelfDetailPage`'s
`SortableItem`). Used by 11d's shelf-item reorder UI: drag
rows to reorder, server-side `PUT /api/shelves/{id}/items`
with optimistic update plus rollback on RFC 9110 `If-Match`
precondition failure (412).

Why dnd-kit and not react-dnd: dnd-kit is the actively
maintained successor (react-dnd's last meaningful release was
2022; dnd-kit ships monthly), supports React 19, has zero
external deps beyond peer React, and has built-in keyboard
accessibility (sortable rows must be operable from keyboard for
WCAG 2.2 AA — Reverie's accessibility skill explicitly requires
this). React-dnd needs an HTML5/Touch backend wrapper that does
not ship keyboard support by default.

## Consequences

- **Good** — react-query is the de-facto standard for
  TypeScript+React data layers in 2024+. New contributors arrive
  pre-trained; no Reverie-specific patterns to learn for cache
  shape, key conventions, or mutation invalidation.
- **Good** — the four packages share maintainership (Tanner
  Linsley + the TanStack maintainers cover react-query,
  react-query-devtools, and react-table; dnd-kit has a separate
  but actively-maintained team). Supply-chain audit posture is
  consistent across the surface.
- **Good** — devtools is dev-bundle-only; the production bundle
  cost is bounded by react-query alone (~10KB gzipped) plus
  react-table when adopted (~8KB gzipped) plus dnd-kit
  (~12KB gzipped including sortable).
- **Bad** — four new top-level dependencies. Dependabot /
  Renovate surface grows by four. Trade-off accepted vs. the
  hand-rolled-state-machine cost outlined per package above.
- **Bad** — react-query has a learning curve for contributors
  coming from Redux or SWR. Mitigation: query-key factory in
  `frontend/src/lib/query/keys.ts` collapses key shape into one
  authoritative place; route loader pattern is documented in
  `library-ui.plan.md` and copied into the codebase as a
  comment-block exemplar.
- **Bad** — `manualPagination: true` is an easy footgun for
  contributors who follow the shadcn data-table docs verbatim
  (the default example is client-side pagination, which would
  bypass server cursors entirely). Mitigation: the first
  data-table consumer carries a Tier 1 JSDoc on its options
  spelling out the manual-pagination requirement. Reviewers
  treat any new client-side-pagination data-table as a blocker.
- **Neutral** — dnd-kit + react-table together do not overlap
  with any existing dependency. No removal triggered.

## Alternatives Considered

### SWR (Vercel) instead of react-query

SWR is the lighter-weight alternative: smaller bundle (~4KB vs.
react-query's ~10KB), simpler API. Vercel-maintained, actively
released, React 19 compatible.

Rejected for two structural reasons:

1. No `QueryCache({ onError })` global error handler. SWR
   exposes `onError` only per-`useSWR()` call (or via a
   `SWRConfig` provider, but the provider's `onError` runs
   per-request, not as a cache-level handler). 401-redirect
   wiring would have to be threaded through every hook.
2. No first-class react-router loader integration. SWR's
   prefetch pattern exists but is less documented and less
   stable across react-router data-mode updates. The Tkdodo
   `prefetchQuery` pattern with react-query is officially
   blessed by both projects.

Trade-off: ~6KB bundle delta in exchange for global error
handling + supported loader integration. Worth it.

### Apollo Client / urql for GraphQL

Reverie's backend is a JSON REST API per the json-api-conventions
ADR. No GraphQL surface exists or is planned. Apollo/urql cost
their full weight even without GraphQL, and bring GraphQL-shaped
ergonomics that mismatch REST cursors.

Rejected on shape grounds.

### Hand-roll fetch + useState + useEffect

Cheapest in dependencies. Most expensive in implementation.
Every Step 11 page would need:

- An ad-hoc loading flag
- An ad-hoc error state
- An ad-hoc abort-on-unmount
- An ad-hoc cache (or no cache at all → N round-trips per page
  for shared data)
- An ad-hoc mutation → invalidation pattern

The argument against the hand-roll is the maintenance cost over
five sub-phases (11a–11f). React-query consolidates all five
into a known-good library. Rejected.

### react-dnd (legacy)

react-dnd is the older drag-and-drop library, last meaningful
release 2022. Larger surface (HTML5 + Touch backends are
separate packages), no first-class keyboard accessibility, less
actively maintained than dnd-kit.

Rejected on maintenance + accessibility grounds. Reverie's
accessibility-skill carve-out (WCAG 2.2 AA) treats keyboard
operability as a hard requirement for any interactive
component; dnd-kit ships it built-in, react-dnd requires a
hand-rolled wrapper.

### `@hello-pangea/dnd` (atlassian's react-beautiful-dnd fork)

Maintained successor to the archived `react-beautiful-dnd`.
Smaller API surface than dnd-kit, focused on vertical lists.

Rejected because Sub-phase 11d's reorder UI is one use of many
DnD interactions Reverie may need over time (admin user-role
drag, shelf-grid card sort if 11d+ scope expands). Dnd-kit's
broader primitive set is the more durable bet for a
self-hosted OSS UI; we adopt it once and reuse it forever.

### TanStack Table v9 / future major

v8 is the current stable. v9 is not released. No reason to wait.

## Implementation Plan

**Sub-phase 11a Task 9** — install react-query + devtools.

- `npm --prefix frontend install @tanstack/react-query@^5`
- `npm --prefix frontend install --save-dev @tanstack/react-query-devtools@^5`
- Per
  [`feedback_reverie_frontend_is_npm`](../.claude/projects/-home-coder-reverie/memory/feedback_reverie_frontend_is_npm.md)
  this is `npm` only — never `pnpm` / `yarn` / `bun`.

**Sub-phase 11a Task 12** — wire `QueryClient` singleton + dev-only
devtools mount per the
[`REACT_QUERY_SETUP`](../.claude/PRPs/plans/library-ui.plan.md)
pattern in the plan.

**Sub-phase 11b** — decide on react-table; if `LibraryPage` in
11a shipped a working list view, defer landing the package.
Otherwise:
`npm --prefix frontend install @tanstack/react-table@^8`. First
consumer is `src/pages/library/BookList.tsx`.

**Sub-phase 11d** —
`npm --prefix frontend install @dnd-kit/sortable@^8`. First
consumer is the shelf-item reorder component (TBD path; the 11d
plan carves it).

## Verification

- [ ] ADR status → `accepted`; entry added to `adr/README.md`.
- [ ] `@tanstack/react-query@^5` pinned in
      `frontend/package.json` `dependencies`.
- [ ] `@tanstack/react-query-devtools@^5` pinned in
      `frontend/package.json` `devDependencies`.
- [ ] `frontend/src/lib/query/client.ts` exports a
      `queryClient` singleton plus `setUnauthenticatedHandler`.
- [ ] `frontend/src/main.tsx` wraps the router in
      `<QueryClientProvider client={queryClient}>` and gates a
      `import.meta.env.DEV` dynamic import of the devtools
      `ReactQueryDevtools` component.
- [ ] On 11b adoption gate: `@tanstack/react-table@^8` lands
      with a Tier 1 JSDoc on the first consumer's table options
      object spelling out `manualPagination: true`.
- [ ] On 11d adoption: `@dnd-kit/sortable@^8` lands with
      keyboard operability tested end-to-end (project memory
      `feedback_use_browser_for_design_critique` — use
      agent-browser to keyboard-drive the reorder).
- [ ] `npm audit` passes for each addition (project memory
      `feedback_audit_ignores`: never use `--ignore` lightly).
- [ ] No `pnpm-lock.yaml` / `yarn.lock` / `bun.lockb` produced;
      `frontend/package-lock.json` is the only lockfile.

## More Information

- [`feedback_industry_standard_default`](../.claude/projects/-home-coder-reverie/memory/feedback_industry_standard_default.md)
  — defaults to ecosystem-standard packages over custom plumbing.
- [`feedback_reverie_frontend_is_npm`](../.claude/projects/-home-coder-reverie/memory/feedback_reverie_frontend_is_npm.md)
  — npm only.
- [`feedback_audit_ignores`](../.claude/projects/-home-coder-reverie/memory/feedback_audit_ignores.md)
  — handling of `npm audit` findings on these new packages.
- Implementation plan: `.claude/PRPs/plans/library-ui.plan.md`
  (Sub-phase 11a Tasks 9 + 12; 11b Task 6; 11d Task 6).
- Linear: [UNK-80](https://linear.app/unkos/issue/UNK-80).
- Sibling ADR: this PR's
  `2026-05-22-json-api-conventions.md` and
  `2026-05-22-backend-aux-crates.md`.
