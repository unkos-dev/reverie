---
type: DESIGN
profile-version: 1
id: "REV-DESIGN-0001"
title: "Library filter and sort state"
satisfies:
  - "REV-REQ-0001"
  - "REV-REQ-0002"
  - "REV-REQ-0003"
governed-by:
  - "REV-ADR-0041"
  - "REV-ADR-0047"
---

# Library filter and sort state

This Design covers how the `/library` route represents, writes, and transmits its two independent axes of query
state: the filter grammar (search, vocabulary, text, range, date, status, series, and shelf conditions) and the sort
stack. The two axes live in different media with different lifetimes and different write paths, and this Design
explains both and the one point where they rejoin: the books list request.

## Purpose and boundaries

This subject owns the representation of "what the reader is currently looking at and in what order" for the library
surface: the client-side codec and writer for filter state, the client-side resolution and writer for the sort
preference, the editing surfaces that dispatch to each, and the projection of both onto the `GET /api/v1/books`
request. It does not own the typed filter grammar's column set or operator vocabulary, the sort column whitelist, the
keyset cursor mechanics, or the server-side query construction; those are the backend's list-endpoint contract, covered
by the ADRs this Design links rather than restated here. It does not own the library's other per-user display
preferences (density, hidden columns, view), except where their resolution pattern (override else installation
default) is identical to sort's and worth naming once.

Depends on: the `/auth/me/preferences` resource for the sort preference; `GET /api/v1/books` for both the wire filter
grammar and the `sort` parameter; the browser's `history` API, which the filter writer drives directly.

Depended on by: every library view (grid, list, table), the filter rail, the quick-search input, the filter chips row,
the table's sortable column headers, and the route loader that prefetches the first page.

## Structure

### State-writer census

| State item                                                                                | Where it lives                                                                                                                                               | Single owner                                                                                                       |
| ----------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------ |
| Library filter URL params (`q`, vocabulary, text, range, date, status, `series`, `shelf`) | Flat keys in the route's `URLSearchParams`, declared by `LIBRARY_PARSERS` in `frontend/src/lib/hooks/use-library-filters.ts`                                 | `useLibraryFilters()` in that module; the `q` key also through `useQuickSearchFilter()` in the same module         |
| `cursor` URL parameter                                                                    | `LIBRARY_PARSERS.cursor`, same module                                                                                                                        | `useLibraryFilters()` and `useQuickSearchFilter()`, which drop it with every filter write                          |
| `view` URL parameter                                                                      | `LIBRARY_PARSERS.view`, same module                                                                                                                          | `useLibraryFilters().setView`                                                                                      |
| `sort_stack` account preference                                                           | `text` column on `user_preferences`, exposed through `/auth/me/preferences`; resolved client-side in `frontend/src/pages/library/use-library-preferences.ts` | `useLibraryPreferences().setSortLevels`                                                                            |
| First-paint display-preference mirror (`density`, `hiddenColumns`, `view`, `sortStack`)   | `localStorage`, one key per account, `frontend/src/pages/library/display-storage.ts`                                                                         | The effect inside `useLibraryPreferences()` that calls `writeDisplayPreferences` whenever a resolved value changes |
| Other `/auth/me/preferences` groups (`density`, `hidden_columns`, `view`)                 | Same `user_preferences` row and response shape as `sort_stack`                                                                                               | `useLibraryPreferences().setDensity`, `setHiddenColumns`, `resetColumns`, `setView`                                |

Call sites that dispatch to those owners:

- Filter params: every section of `frontend/src/components/shell/FilterRail.tsx` through `commitSlice` and
  `commitTyped`; `frontend/src/pages/library/LibraryPage.tsx` through `commitAll` (the chips row's per-chip remove),
  `clearAll` (the clear-all affordance and the filtered empty state), `revertTypedEdits` (the filter drawer's Escape
  and cancel path), and `setView`; the page toolbar's search input through `useQuickSearchFilter()`.
- `cursor`: cleared inside every filter-slice commit and quick-search write, because a changed condition invalidates
  the keyset boundary the cursor names.
- `view`: `setView` in `LibraryPage.tsx`, from the view switcher and the table-chunk-load-failure fallback.
- `sort_stack`: `handleSortChange` in `LibraryPage.tsx`, the library's one sort intent handler, called from the rail's
  `SortSection` (add, remove, reorder, flip, reset) and from the column-header click and ctrl-click handler in
  `frontend/src/pages/library/table/LibraryTableView.tsx`.
- First-paint mirror: no other call site writes it; it is read once at mount and otherwise only overwritten whole.
- Other preference groups: `toggleColumn`, `setView`, and the toolbar's density control in `LibraryPage.tsx`, merged
  through the same delayed patch queue as the sort write.

No item above has more than one writer. `useLibraryFilters` and `useLibraryPreferences` are themselves the two owners
the census resolves to; every editing surface is a caller of one or the other, never a second URL or preference writer.

### Component relationships

- `frontend/src/routes/library-params.ts` is the one module that parses and serialises the filter grammar: `parseFilterParams`
  reads a `URLSearchParams` into the grouped `FilterState` the rail renders, `serializeFilterParams` writes a
  `FilterState` back, and `filterStateToParams` / `paramsFromSearch` project it onto the flat `ListBooksParams` the API
  client sends. Every reader and writer of filter state routes through this module; nothing hand-assembles a filter
  query string elsewhere.
- `frontend/src/lib/hooks/use-library-filters.ts` wraps that codec in a `nuqs`-backed writer (`useLibraryFilters`) that
  applies updates through `history.replaceState`, plus a narrower `useQuickSearchFilter` for the `q` key's delayed
  typing path. `useLibrarySearchParams` (also here) is the one sanctioned read of the applied URL; React Router's own
  `useSearchParams` is not interchangeable with it because it does not observe `history.replaceState` writes.
- `frontend/src/pages/library/use-library-preferences.ts` (`useLibraryPreferences`) owns the sort preference alongside
  the library's other per-user display preferences. It resolves each group from up to three media (a same-session
  local write, the server response, the first-paint mirror), delays and merges writes into one PATCH, and exposes
  `sortLevels` (the one resolved stack every surface renders) and `setSortLevels` (the one sort intent handler).
- `frontend/src/components/shell/FilterRail.tsx`, rendered inside the library page's filter drawer, is the primary
  filter- and sort-editing surface: one collapsible section per filterable field plus a sort section, each dispatching to `useLibraryFilters`'s slice commits or to the
  page's `onSortChange` prop. The chips row (`frontend/src/pages/library/FilterChips.tsx`) and the quick-search input
  are the two other editing surfaces; both dispatch through the same writer.
- `frontend/src/pages/library/LibraryPage.tsx` composes the above: it resolves the sort stack via
  `useLibraryPreferences`, the filter state via `paramsFromSearch`/`parseFilterParams`, builds the `GET /api/v1/books`
  request, and supplies `handleSortChange` (the one sort intent handler) to both the rail and the table view's column
  headers.
- Backend-side, `backend/src/routes/library/filters.rs` and `backend/src/routes/sort_spec.rs` are the sole consumers
  of the filter and sort query parameters respectively; both resolve client input against a closed, index-backed
  column set before any SQL is built.

## Interfaces and dependencies

- `GET /api/v1/books` (`backend/src/routes/library/mod.rs`) is the endpoint this state ultimately targets. It accepts
  the flat suffix-operator filter parameters, the `sort` parameter, and `cursor`; the wire grammars for the first two
  are covered by the ADRs linked in More information, not restated here. Filter and sort parameters are documented in
  `ListParams`'s OpenAPI schema (`#[utoipa::path]` on the `list` handler).
- `GET /auth/me/preferences` and `PATCH /auth/me/preferences` (`backend/src/routes/preferences/mod.rs`) are the
  interface for the sort preference (and the library's other per-user display preferences). The response carries the
  caller's overrides (`sort_stack: Option<String>`, `null` meaning inherit) and the installation `defaults` together,
  so the client never holds a second copy of the defaults.
- `frontend/src/api/index.ts` and the API client modules under `frontend/src/api/` are the typed request/response
  boundary the above two interfaces cross: `ListBooksParams`, `getPreferences`, `updatePreferences`,
  `parseSortParam`/`serializeSortParam`.
- The write authority for filter URL state is `nuqs`'s `useQueryStates`, configured with `limitUrlUpdates` per call
  (`debounce(FILTER_DEBOUNCE_MS)` for typed slices, the library default rate limit otherwise) and applied through
  `history.replaceState`.

## Data and state

- **Filter URL params.** Live in the route's URL search string for the lifetime of that browser tab's navigation
  history within the app: they survive a page refresh and in-app navigation (because they are part of the URL), but do
  not survive into a fresh visit, because nothing mirrors them to any longer-lived store. Filters are not durable
  across visits by design.
- **`sort_stack` preference.** Persisted server-side on the `user_preferences` row (one row per account), reached
  through `/auth/me/preferences`. Its lifetime is the account's: it survives reload, re-login, and a change of device,
  and is written under last-write-wins semantics with no precondition header. A `null` value means "inherit the
  installation default", which is itself carried in the same response's `defaults.sort_stack` field rather than
  hardcoded on the client.
- **First-paint mirror.** `localStorage`, keyed per account (`reverie_library_display:<user id>`), holding the last
  known _effective_ value of `density`, `hiddenColumns`, `view`, and `sortStack` (the override, or `null` when
  inheriting). Read once at mount to avoid a first-paint flash of installation defaults while the preferences request
  is in flight; overwritten on every render where the resolved values change; never treated as authoritative once a
  server response has arrived.
- **Session-local override state.** `useLibraryPreferences` also holds `useState` values (`localDensity`,
  `localColumns`, `localSort`, `localView`) that pin a group to what the reader just did for the rest of the session,
  so a slow-to-arrive server response cannot overwrite an edit made while it was in flight. These are in-memory only
  and do not outlive the component tree.

## Runtime behaviour

**A filter change**, for example typing into the Pages range editor's lower bound in the rail:

1. `RangeSection` calls the `pages` slice's `onChange`, which is `commitTyped("pages", ...)` in `FilterRail.tsx`.
2. `commitTyped` calls `useLibraryFilters().commitSlice` for the `pages` slice with the patch function and the
   delayed option set.
3. `commitSlice` serialises the patched `FilterState` through `serializeFilterParams`, reads back only the `pages`
   slice's keys (`pages_gte`, `pages_lte`, `pages_empty`), and calls `nuqs`'s `setParams` with those keys plus
   `cursor: null`, held via `debounce(FILTER_DEBOUNCE_MS)` (250 ms).
4. After the hold settles, `nuqs` applies the update via `history.replaceState`. `useLibrarySearchParams`
   (`useOptimisticSearchParams`) reflects it immediately to every reader on the next render.
5. `LibraryContent` in `LibraryPage.tsx` derives `params` from the updated search via `paramsFromSearch`, which feeds
   the `GET /api/v1/books` query key and request; a changed key triggers React Query to refetch with the new
   condition.

Before the hold settles, the rail itself already renders the typed value: `useLibraryFilters().filters` is derived
from the _pending_ `nuqs` values (`toSearch(values)`), not from the applied URL, so the input never appears to lag
behind the keystroke even though the network-visible URL and the list request do.

**A sort change**, for example a ctrl-click on the "Pages" column header in the table view:

1. The table view's header handler computes the next stack (toggling or adding the `pages` level) and calls
   `onSortChange`, which is `handleSortChange` in `LibraryPage.tsx`.
2. `handleSortChange` calls `preferences.setSortLevels(levels)`.
3. `setSortLevels` serialises the levels to the `?sort=`-shaped wire string via `serializeSortParam`, sets
   `localSort` immediately (so every reader of `sortLevels` sees the new stack on the next render, before the network
   round-trip), and queues a `sort_stack` field carrying that serialised string, or `null` for an empty stack, onto
   the delayed preference writer (`PREFERENCE_WRITE_DEBOUNCE_MS`, 400 ms).
4. `LibraryPage.tsx` derives `sortOverride` from `preferences.sortOverride` and assigns it to the request's `sort`
   field only when non-empty (`if (deferredSortOverride !== "") params.sort = deferredSortOverride`); an inheriting
   reader's request carries no `sort` parameter at all.
5. The delayed patch queue flushes to `PATCH /auth/me/preferences` after 400 ms of inactivity (or when the page is
   left), merging with any other pending preference field into one request.
6. The list request's query key includes `sort` (via `deferredSortOverride`, a `useDeferredValue` of the resolved
   override), so the sort change alone is enough to trigger a refetch without a matching filter change; the current
   rows stay visible with `aria-busy` set until the resorted page lands, rather than falling back to the route's
   Suspense skeleton.

## Failure and recovery

- **An invalid URL filter.** The client-side codec (`routes/library-params.ts`) is tolerant by construction: a
  malformed value (a non-integer, an ill-formed date, an unrecognised status token) is dropped while parsing the URL
  into `FilterState`, rather than surfaced as an error or sent to the server. A hand-crafted or stale URL therefore
  never reaches the wire with a value the server would reject; the affected condition is simply absent from the
  filter rail and from the request. If a filter value reaches the server without going through this client-side
  parsing regardless (a non-browser client, or a value the client-side codec does not yet police), the server rejects
  the request: a type-level decode failure
  (bad UUID, non-integer, non-ISO-date) returns `400 Bad Request` via `AppError::MalformedQuery`, and a value that
  decodes but violates a semantic bound (over-cap value list, over-long text, out-of-range rating, negative page
  bound, unrecognised status token) returns `422 Unprocessable Entity` via `AppError::Validation`
  (`backend/src/routes/library/filters.rs::validate`). Neither path silently narrows or widens the result set.
- **A cursor invalidated by a filter or sort change.** Every filter-slice write and the quick-search write drop
  `cursor` in the same update, because a changed condition invalidates the keyset boundary a stale cursor names. If a
  cursor is replayed against a different filter set or sort stack regardless, the server rejects it with `422` (the
  keyset and sort-stack cursor contracts, covered by their own ADRs, not restated here).
- **A failed preference write.** `useLibraryPreferences`'s mutation `onError` handler logs the failure to the console
  (`QueryCache.onError` only routes 401s) and otherwise does nothing: the local value (`localSort`, or the equivalent
  field for another group) stays applied for the session, so the reader's chosen sort keeps working even though it
  was not durably saved. No retry is attempted automatically; the next successful write (any further edit to any
  preference group) carries the merged pending patch, including the field that previously failed only if the reader
  edits that field again. A write that never succeeds again means the override is lost on the next device or after
  the browser storage is cleared, since the mirror and the session-local state are the only places it was ever held
  outside the server.
- **A preferences response arriving late.** The route loader can only seed the list query's cache key from the
  first-paint mirror or the bare URL; if the account has an override the mirror does not yet know about (a new
  device), the first page renders in the seeded (installation or stale-mirror) order and then re-sorts once the
  response reveals the override, producing exactly one additional list request rather than a discarded duplicate of
  the seeded one.

## Security and operations

The `sort_stack` preference (and the other display-preference groups it shares a row and endpoint with) is per-user
data reached only through `/auth/me/preferences`, which is scoped to the caller's own account: the resource never
accepts a user id from the request, and both handlers run inside `crate::db::acquire_with_rls`, so the
`user_preferences_owner` row-level-security policy confines every statement to the caller's own row
(`backend/src/routes/preferences/mod.rs`). The value itself is re-validated server-side against the same sort-column
whitelist and level cap the list endpoint enforces (`validate_sort_stack` in `backend/src/models/user_preferences.rs`),
so a tampered or malformed preference write cannot smuggle an unwhitelisted sort column onto a later list request.

Filter values reach the database only as parameter-bound query arguments; no filter or sort value is ever
interpolated into SQL text (`backend/src/routes/library/filters.rs` and `backend/src/routes/sort_spec.rs`).

Not applicable: this subject exposes no operational surface of its own (no service to run, restart, or scale). The
list and preferences endpoints it depends on are covered by the operational documentation for the backend service as
a whole, not by this Design.

## More information

- [Multi-column sort stack on the keyset list contract](../../../adr/0037-multi-column-sort-stack-on-the-keyset-list-contract.md)
- [Typed filter grammar on list endpoints](../../../adr/0038-typed-filter-grammar-on-list-endpoints.md)
- [No unbounded queries: keyset pagination as the default list contract](../../../../docs/adr/0019-keyset-pagination-as-the-default-list-contract.md)
