---
type: ADR
profile-version: 1
id: "REV-ADR-0010"
title: "Frontend data-layer dependencies: React Query and dnd-kit"
status: "accepted"
recorded-on: "2026-09-04"
decided-on: "2026-05-22"
decision-makers:
  - "John Unkovich"
informed:
  - "Reverie contributors"
---

# Frontend data-layer dependencies: React Query and dnd-kit

## Context and problem statement

The JSON REST API conventions work put a production browser UI on top of new JSON endpoints. The frontend was a
Vite + React 19 + Tailwind v4 + shadcn (radix-nova) tree with no data-fetching library: the only network calls were
theme persistence and OPDS testing helpers, each a one-off `fetch()` call. There was no centralised cache, no request
deduplication, no loading state machine, and no mutation-invalidation framework. The work also needed drag-to-reorder
for shelf items, operable from the keyboard. Which packages should serve server-state caching and mutation handling,
a devtools surface for that cache, and accessible drag-and-drop?

## Decision drivers

- Server-state caching needs request deduplication across components that read the same resource, and
  mutation-driven cache invalidation, since the existing tree hand-rolled neither.
- The cache's state is the most cross-cutting state in the app, so a runtime inspector for it turns a long debugging
  session into a short one.
- Shelf-item reordering must be operable from the keyboard: sortable rows are an interactive component, and WCAG 2.2
  AA requires keyboard operability for those.
- A global auth-failure handler and prefetch-on-route-load both need first-class support from the caching library,
  not a pattern threaded by hand through every call site.
- Packages that share maintainership and an active release cadence keep the supply-chain audit posture consistent.

## Considered options

- React Query with its devtools, and dnd-kit
- SWR instead of React Query
- Apollo Client or urql for GraphQL
- Hand-rolled `fetch` with `useState` and `useEffect`
- react-dnd (legacy)
- `@hello-pangea/dnd` (Atlassian's react-beautiful-dnd fork)

## Decision outcome

Chosen option: **React Query with its devtools, and dnd-kit**, because together they give the frontend request
deduplication, mutation-driven cache invalidation, a runtime cache inspector, and keyboard-accessible drag-to-reorder,
without hand-rolling any of the three.

`@tanstack/react-query@^5` is pinned under `dependencies` in `frontend/package.json`, registered via a `QueryClient`
singleton in `frontend/src/lib/query/client.ts`, and mounted via `QueryClientProvider` in `frontend/src/main.tsx`.
It was chosen over hand-rolled `fetch` plus `useState` plus `useEffect` because it gives request deduplication across
components that read the same resource, because a mutation can invalidate a query key and every reader of that key
refetches automatically, because `useSuspenseQuery` integrates with React 19 Suspense without a bespoke loading-state
machine in every component, and because React Router v7 data-mode loaders can call `queryClient.prefetchQuery` so the
cache is already warm when a component mounts, an officially documented react-router pattern rather than a community
workaround. Version 5 specifically, because v4 is in maintenance mode and the global error-handler pattern the
auth-redirect wiring needs, an `onError` callback on `QueryCache`, requires v5: v4 placed the handler on
`defaultOptions.queries.onError`, which v5 removed.

`@tanstack/react-query-devtools@^5`, matching the runtime version, is pinned under `devDependencies` and mounted via
a dynamic, `import.meta.env.DEV`-gated import in `main.tsx`. The gated import costs zero bytes in the production
bundle, since Vite tree-shakes it, and around 120KB on the dev bundle. Because query-cache state is the most
cross-cutting state in the app, being able to inspect cache entries by key at runtime is the difference between a
short debug and a long one when invalidation is wrong; devtools is the TanStack-maintained inspector and no
third-party tool covers the same surface.

`@dnd-kit/sortable` is pinned under `dependencies` alongside `@dnd-kit/core` (a transitive dependency pinned
directly so version bumps surface at install) and `@dnd-kit/utilities` (CSS-transform helpers). It serves
the shelf-item reorder UI: dragging rows to reorder, backed by a server-side `PUT` to a shelf's item-order endpoint
with an optimistic update and rollback on an RFC 9110 `If-Match` precondition failure (412). Chosen over react-dnd
because dnd-kit is the actively maintained successor, supports React 19, has zero external dependencies beyond peer
React, and has built-in keyboard accessibility; react-dnd needs an HTML5 or Touch backend wrapper that does not ship
keyboard support by default.

### Consequences

- Positive: React Query is the de-facto standard for TypeScript and React data layers; new contributors arrive
  pre-trained, with no Reverie-specific patterns to learn for cache shape, key conventions, or mutation invalidation.
- Positive: React Query and its devtools share maintainership under the TanStack umbrella; dnd-kit has a separate but
  actively maintained team, so supply-chain audit posture stays consistent across the surface.
- Positive: devtools is dev-bundle-only, so the production bundle cost is bounded by `@tanstack/react-query` alone
  (around 10KB gzipped) plus dnd-kit (around 12KB gzipped including sortable).
- Positive: dnd-kit does not overlap with any existing dependency, so adopting it triggers no removal.
- Negative: three new top-level dependencies (`@tanstack/react-query`, `@tanstack/react-query-devtools`, and the
  `@dnd-kit` family) grow the Dependabot and Renovate surface. Accepted against the cost of hand-rolling the same
  state machines per page.
- Negative: React Query has a learning curve for contributors coming from Redux or SWR. Mitigated by a query-key
  factory in `frontend/src/lib/query/keys.ts` that collapses key shape into one authoritative place.

### Confirmation

`@tanstack/react-query`, `@tanstack/react-query-devtools`, `@dnd-kit/sortable`, and `@dnd-kit/core` are pinned under
`frontend/package.json`. `frontend/src/lib/query/client.test.ts` and `frontend/src/lib/query/devtools.test.tsx`
exercise the query client and the gated devtools import; `frontend/src/pages/shelves/ShelfDetailPage.tsx` is the
`DndContext`/`useSortable` consumer for shelf reordering. `just js::build` runs the frontend's `tsgo` typecheck,
which fails if any of these imports breaks.

## Pros and cons of the options

### React Query with its devtools, and dnd-kit

- Positive: request deduplication, mutation-driven invalidation, Suspense integration, and route-loader prefetch
  come from one actively maintained library with an official react-router integration pattern.
- Positive: dnd-kit ships keyboard accessibility built in, satisfying WCAG 2.2 AA without a hand-rolled wrapper.
- Negative: three new top-level dependencies to track for updates and audits.

### SWR instead of React Query

SWR is the lighter-weight alternative: a smaller bundle (around 4KB versus React Query's 10KB) and a simpler API,
Vercel-maintained, actively released, and React 19 compatible.

- Positive: smaller bundle, simpler API surface.
- Negative: no `onError` callback on a `QueryCache`-level global error handler; SWR exposes `onError` only per
  `useSWR()` call or via an `SWRConfig` provider whose `onError` runs per request rather than as a cache-level
  handler, so 401-redirect wiring would have to be threaded through every hook.
- Negative: no first-class react-router loader integration; SWR's prefetch pattern is less documented and less
  stable across react-router data-mode updates than React Query's officially blessed `prefetchQuery` pattern.

### Apollo Client or urql for GraphQL

Reverie's backend is a JSON REST API. No GraphQL surface exists or is planned.

- Negative: Apollo and urql cost their full weight even without GraphQL, and bring GraphQL-shaped ergonomics that
  mismatch REST cursors.

### Hand-rolled `fetch` with `useState` and `useEffect`

Cheapest in dependencies, most expensive in implementation.

- Negative: every page needs an ad hoc loading flag, an ad hoc error state, an ad hoc abort-on-unmount, an ad hoc
  cache (or no cache at all, meaning repeated round-trips per page for shared data), and an ad hoc
  mutation-to-invalidation pattern; the maintenance cost compounds across every page that fetches data.

### react-dnd (legacy)

react-dnd is the older drag-and-drop library, with its last meaningful release in 2022.

- Negative: a larger surface, since the HTML5 and Touch backends are separate packages.
- Negative: no first-class keyboard accessibility; a hand-rolled wrapper would be needed to meet WCAG 2.2 AA
  keyboard operability, which dnd-kit ships built in.

### `@hello-pangea/dnd` (Atlassian's react-beautiful-dnd fork)

The maintained successor to the archived `react-beautiful-dnd`, with a smaller API surface focused on vertical
lists.

- Negative: shelf-item reordering is one of several drag-and-drop interactions Reverie may need over time (for
  example, admin user-role drag or shelf-grid card sort); dnd-kit's broader primitive set is the more durable choice
  to adopt once and reuse.

## More information

This decision was recorded alongside the
[JSON REST API conventions](../../adr/2026-05-22-json-api-conventions.md) decision and the
[backend auxiliary crates](./0009-backend-auxiliary-crates-axum-extra-serde-with-and-subtle.md) decision, for the
same body of work.
