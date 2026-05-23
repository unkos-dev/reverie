# Implementation Report — 11a-A.5 (frontend scaffold)

**Plan**: `.claude/PRPs/plans/library-ui.plan.md` (Tasks 9–16)
**Source Issue**: UNK-80 (sub-phase 11a-A.5)
**Branch**: `feat/unk-80-11a-a5-frontend-scaffold`
**Date**: 2026-05-23
**Status**: COMPLETE

---

## Summary

Frontend foundations for Step 11 of the Reverie blueprint. Wires
`@tanstack/react-query@^5` + devtools, lays down `src/api/` with an
RFC-7807-parsing fetch wrapper and CSRF-token-injecting mutating-verb
support, and converts the router to data mode with production routes
for `/library` and `/b/:id`. Adds a 401 auth boundary that
full-page-redirects to `/auth/login` (the backend OIDC initiator,
since no SPA `/login` route exists). Visual layout mirrors the dev
hero contract from Step 10 D4 (`/design/hero/library`, `/design/hero/book`).

---

## Assessment vs Reality

| Metric     | Predicted | Actual | Reasoning                                                                                                                                     |
| ---------- | --------- | ------ | --------------------------------------------------------------------------------------------------------------------------------------------- |
| Complexity | HIGH      | MEDIUM | Tasks 9–16 lifted cleanly off well-spec'd patterns in the plan; no surprises in the wire shape (backend A.2/A.3/A.4 already landed in main).  |
| Confidence | 9/10      | 9/10   | One advisor-flagged correction during execution (the `/login` SPA route does not exist; switched to `window.location.assign("/auth/login")`). |

---

## Tasks Completed

| #   | Task                                       | File(s)                                                                                                                     | Status |
| --- | ------------------------------------------ | --------------------------------------------------------------------------------------------------------------------------- | ------ |
| 9   | install `@tanstack/react-query` + devtools | `frontend/package.json`, `frontend/package-lock.json`                                                                       | ✅     |
| 10  | RED — `apiFetch` + `listBooks` tests       | `src/api/fetch.test.ts`, `src/api/books.test.ts`                                                                            | ✅     |
| 11  | GREEN — `src/api/` scaffold                | `src/api/errors.ts`, `src/api/fetch.ts`, `src/api/books.ts`, `src/api/index.ts`                                             | ✅     |
| 12  | `QueryClient` + `QueryClientProvider`      | `src/lib/query/client.ts`, `src/lib/query/keys.ts`, `src/lib/query/devtools.tsx`, `src/main.tsx`                            | ✅     |
| 13  | react-router data mode + production routes | `src/main.tsx`, `src/routes/production.ts`, `src/routes/library.tsx`, `src/routes/book.tsx`, `src/routes/library-params.ts` | ✅     |
| 14  | `LibraryPage` + RTL test                   | `src/pages/library/LibraryPage.tsx`, `src/pages/library/LibraryPage.test.tsx`                                               | ✅     |
| 15  | `BookPage` + RTL test                      | `src/pages/book/BookPage.tsx`, `src/pages/book/BookPage.test.tsx`                                                           | ✅     |
| 16  | 401 auth boundary                          | `src/App.tsx`, `src/App.test.tsx`, `src/lib/query/client.test.ts`                                                           | ✅     |

---

## Validation Results

| Check       | Result | Details                                                                       |
| ----------- | ------ | ----------------------------------------------------------------------------- |
| Type check  | ✅     | `tsc -b` clean                                                                |
| Lint        | ✅     | `eslint . --max-warnings 0` — 0 errors, 0 warnings                            |
| Unit tests  | ✅     | 156 passed (16 files); +20 new tests for this sub-phase                       |
| Build       | ✅     | `vite build` — design chunk dead-stripped; library + book lazy chunks emitted |
| Integration | ⏭️     | Browser QA against live backend deferred — see "Outstanding"                  |

---

## Files Changed

| File                                              | Action | Notes                                                                                                    |
| ------------------------------------------------- | ------ | -------------------------------------------------------------------------------------------------------- |
| `frontend/package.json`                           | UPDATE | Added react-query 5.100.13 + devtools                                                                    |
| `frontend/package-lock.json`                      | UPDATE | npm lockfile only (no rogue lockfiles)                                                                   |
| `frontend/src/App.tsx`                            | UPDATE | `<Outlet/>` + 401 redirect effect                                                                        |
| `frontend/src/main.tsx`                           | UPDATE | QueryClientProvider; data-mode children; devtools gated by `import.meta.env.DEV`                         |
| `frontend/src/api/errors.ts`                      | CREATE | `ApiError` (RFC 7807 fields)                                                                             |
| `frontend/src/api/fetch.ts`                       | CREATE | `apiFetch` — CSRF injection on mutating verbs, RFC 7807 parse, csrf-mismatch retry, Content-Type default |
| `frontend/src/api/books.ts`                       | CREATE | `listBooks` / `getBook` / `getWork` + types                                                              |
| `frontend/src/api/index.ts`                       | CREATE | Barrel                                                                                                   |
| `frontend/src/api/fetch.test.ts`                  | CREATE | 13 tests                                                                                                 |
| `frontend/src/api/books.test.ts`                  | CREATE | 6 tests                                                                                                  |
| `frontend/src/lib/query/client.ts`                | CREATE | `queryClient` singleton + `setUnauthenticatedHandler`                                                    |
| `frontend/src/lib/query/keys.ts`                  | CREATE | Key factory                                                                                              |
| `frontend/src/lib/query/devtools.tsx`             | CREATE | Lazy dev-only devtools panel                                                                             |
| `frontend/src/lib/query/client.test.ts`           | CREATE | 4 tests                                                                                                  |
| `frontend/src/routes/production.ts`               | CREATE | Lazy route declarations                                                                                  |
| `frontend/src/routes/library.tsx`                 | CREATE | `loader` + `Component` re-export                                                                         |
| `frontend/src/routes/book.tsx`                    | CREATE | `loader` + `Component` re-export                                                                         |
| `frontend/src/routes/library-params.ts`           | CREATE | URL → API params parser (split out to satisfy `react-refresh/only-export-components`)                    |
| `frontend/src/pages/library/LibraryPage.tsx`      | CREATE | Grid + list + Load more + empty state + skeleton fallback                                                |
| `frontend/src/pages/library/LibraryPage.test.tsx` | CREATE | 7 RTL tests                                                                                              |
| `frontend/src/pages/book/BookPage.tsx`            | CREATE | Sticky cover + Tabs (Overview / Versions / Activity)                                                     |
| `frontend/src/pages/book/BookPage.test.tsx`       | CREATE | 7 RTL tests                                                                                              |
| `frontend/src/App.test.tsx`                       | CREATE | 2 auth-boundary tests                                                                                    |

---

## Deviations from Plan

1. **`/login` SPA route does not exist** — the plan example wrote
   `navigate("/login")`. There is no SPA route at `/login`, and Vite's
   proxy only forwards `/api`, `/auth`, `/opds` to the backend. A
   client-side `navigate()` would never reach the OIDC initiator at
   `/auth/login`. Switched to `window.location.assign("/auth/login")`
   for full-page navigation that exits the SPA and hits the backend
   OIDC flow. Test asserts on the `assign` call rather than a
   memory-router `/login` element. Captured in `App.tsx` docstring.

2. **`paramsFromSearch` lives in `routes/library-params.ts`, not in
   `routes/library.tsx`** — ESLint's `react-refresh/only-export-
components` rule forbids the route module exporting both a
   `Component` and a non-component helper. The route file still
   exports `loader` (necessary, framework contract) with a
   file-scoped `eslint-disable react-refresh/only-export-components`
   comment; the parser was moved out to keep the disable to the
   single load-bearing case.

3. **Versions tab renders summary only** — Plan task 15 says read-only
   metadata version list. Backend currently surfaces only
   `metadata_version_summary { pending, accepted }` on
   `BookDetail`; a per-row version list endpoint is not in scope
   for 11a. Per-row review (accept / reject / revert) lands in
   sub-phase 11c. Versions tab shows the counts plus an explicit
   "lands in 11c" note.

4. **Backend status enum values match production code, not the plan
   example** — plan's `BookListItem` interface listed
   `ingestion_status: "pending" | "staged" | "managed" | "failed"`,
   but the merged backend models (`models/ingestion_status.rs`,
   `models/enrichment_status.rs`) use `pending | processing | complete |
failed | skipped` / `pending | in_progress | complete | failed |
skipped`. Frontend types follow the backend reality. Validation
   status remains a raw DB string per backend docstring.

---

## Outstanding (deferred to follow-ups, NOT to 11a-A.5)

- **Live browser QA**: backend was not running in the workspace at
  implementation time. Plan §"Exit Criteria (11a)" requires
  `localhost:5173/library renders real books from the dev DB with
covers`. The route serves the SPA shell (`200 OK` from Vite), and
  unit tests cover the data-flow shape end-to-end with seeded
  caches, but a screenshot vs the dev hero needs a running backend.
  Captured as a PR-body manual-QA checklist item.

- **Step 10 hero pages still link to `/design/hero/book?id=…`**
  rather than the production `/b/:id`. Out of scope for the scaffold;
  the hero pages remain the dev visual target.

- **No infinite-query route-loader pre-flight beyond page 1.** Loader
  only seeds page 1; pagination after that runs from the component
  via `fetchNextPage`. Plan's `prefetchInfiniteQuery({ pages: N })`
  approach was avoided because react-router's loader has no signal
  for "user is on page N" — the seed is purely page 1 and Load
  more drives subsequent pages.

---

## Issues Encountered

1. **`tsconfig` `erasableSyntaxOnly`** rejects TypeScript parameter
   properties (`constructor(public readonly status: number, ...)`).
   Rewrote `ApiError` with explicit field declarations + a plain
   constructor body — passes both rules and lint.

2. **`react-refresh/only-export-components`** vs react-router data
   route modules that must export both `loader` and `Component`.
   Disabled at file scope on `routes/library.tsx` and `routes/book.tsx`
   with a docstring justification.

3. **jsdom `window.location.assign` cannot be `vi.spyOn`'d** — the
   property is read-only on the JSDOM Location class. Mocked via
   `Object.defineProperty(window, "location", { value: mock })` and
   restored in `afterEach`.

---

## Tests Written

| Test File                                | Cases                                                                                                                                      |
| ---------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------ |
| `src/api/fetch.test.ts`                  | 13 — credentials + method, CSRF injection (5 verb cases), RFC 7807 parsing (3), csrf-mismatch retry (3), 204 body handling                 |
| `src/api/books.test.ts`                  | 6 — `listBooks` (URL building, filter serialisation, signal forwarding), `getBook` (percent-encoding), `getWork`                           |
| `src/lib/query/client.test.ts`           | 4 — `QueryCache.onError` 401 → handler, 500 / TypeError do not fire, handler replacement                                                   |
| `src/pages/library/LibraryPage.test.tsx` | 7 — heading + count, grid render, Load more visibility (×2), list view via `?view=list`, empty state, card link target                     |
| `src/pages/book/BookPage.test.tsx`       | 7 — title + author, series label, Tabs present, Overview description, Versions count badge, null-description placeholder, back link target |
| `src/App.test.tsx`                       | 2 — 401 → `window.location.assign("/auth/login")`, 500 does not redirect                                                                   |

39 new tests; total suite 156 passing (16 files).

---

## Next Steps

- [ ] Browser QA against live backend (`docker compose up -d` + `cargo run -p reverie_api`), screenshot `/library` and `/b/:id`, attach to PR.
- [ ] Open PR: `gh pr create` against `main` with title `feat(frontend): library UI scaffold (UNK-80, 11a-A.5)`.
- [ ] After merge, sub-phase 11b (search + filters) is unblocked.
