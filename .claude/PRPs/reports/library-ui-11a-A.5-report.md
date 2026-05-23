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
| Unit tests  | ✅     | 183 passed (20 files); +66 new tests for this sub-phase                       |
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

66 new tests; total suite 183 passing (20 files).

---

## Review-round-2 fixes (post-CodeRabbit / Greptile / adversarial)

PR #311 picked up 16 actionable CodeRabbit comments, 4 Greptile P1/P2 findings, and an independent adversarial review. Convergent findings resolved in a follow-up commit on the same branch:

1. **Skeleton fallback was dead code** (Greptile P1, adversarial P3-X-3). `LibraryPage` used `useInfiniteQuery` (non-suspense) wrapped in a `<Suspense>` boundary — the boundary never fired. Switched to `useSuspenseInfiniteQuery`; `LibrarySkeleton` is now the live cache-miss fallback.

2. **No Zod validation on API responses** (Greptile P1, CodeRabbit on `books.ts:172`, adversarial P1-D-1). Violated `frontend/CLAUDE.md`'s boundary-validation rule. Added Zod schemas for `IngestionStatus`, `EnrichmentStatus`, `SeriesRef`, `BookListItem`, `BookListResponse`, `MetadataVersionSummary`, `BookDetail`, `WorkManifestation`, `WorkDetail`; types are now `z.infer` derivations. `listBooks` / `getBook` / `getWork` parse before returning.

3. **No `errorElement` on the root route** (adversarial P2-C-1, CodeRabbit on `BookPage`, Greptile P2). `throw new Response(404)` from the loader landed on react-router's default unstyled UI. Added `src/components/RootErrorBoundary.tsx` and wired it at the root route. Distinguishes `isRouteErrorResponse` / `ApiError` / generic-error branches.

4. **`204`/`205` on the CSRF-retry success path** (CodeRabbit). The retry branch unconditionally called `.json()`. Extracted a shared `decodeSuccess<T>` helper used by both the main path and the retry path; both honour the empty-body status codes.

5. **`BookPage` `{ id = "" }` default unreachable** (CodeRabbit, adversarial P3-X-4). Replaced with an explicit guard that throws when the id is missing — the unreachable default could otherwise propagate to an empty `getBook("")` call in a non-routed test.

6. **Removed `afterRefresh` dead arg** (adversarial P2-C-4, Greptile). Pure cosmetic dead parameter; deleted along with the `void afterRefresh;` discard.

7. **Explicit return types on key factory** (CodeRabbit). Added named tuple types (`BooksAllKey`, `BooksListKey`, `BookDetailKey`, `WorkDetailKey`) and annotated each builder function. Improves IDE narrowing and surfaces shape drift faster.

8. **New tests** (CodeRabbit + adversarial P3-X-1/2/3/4):
   - `src/routes/library-params.test.ts` — 7 cases (parser, unknown-key drop, sort enum guard, empty-string forwarding).
   - `src/routes/library.test.ts` — 4 cases (seed cache, cursor-strip, sort preservation, null return).
   - `src/routes/book.test.ts` — 4 cases (seed cache, 404 throw on missing/empty id, null return).
   - `src/lib/query/keys.test.ts` — 6 cases (root namespace, structural equality, distinct slots, ordering).
   - `src/api/books.test.ts` — +4 cases (`getWork` percent-encoding, Zod-violation rejection for missing fields / bad enum / drifted shape).
   - `src/api/fetch.test.ts` — +2 cases (`205` main path, `204` on csrf-retry path).
   - `src/pages/book/BookPage.test.tsx` — Versions tab now asserts the accepted count in addition to pending.

Acknowledged but deferred (out of scope for 11a-A.5):

- **`P2-C-5` `ReadableStream` body replay** — first multipart/streaming caller will need to clone or buffer; tracked in the file-upload PR.
- **`P3-X-5` Versions badge layout at `pending > 99`** — deferred to 11c when per-row review ships and the badge becomes load-bearing.
- **`P3-X-6` `Link` header surfacing** — `apiFetch` still reads cursor from body only; flag for observer-driven scroll if/when 11b needs it.

---

## Next Steps

- [ ] Browser QA against live backend (`docker compose up -d` + `cargo run -p reverie_api`), screenshot `/library` and `/b/:id`, attach to PR.
- [ ] Open PR: `gh pr create` against `main` with title `feat(frontend): library UI scaffold (UNK-80, 11a-A.5)`.
- [ ] After merge, sub-phase 11b (search + filters) is unblocked.
