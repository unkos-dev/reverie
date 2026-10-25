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
  - "REV-ADR-0066"
  - "REV-ADR-0062"
  - "REV-ADR-0047"
---

# Library filter and sort state

## Purpose and boundaries

The Library surface owns the reader's current filter intent, visit view override and resolved account sort. It projects
those values onto the books-list request and shares the resulting QueryClient pages with grid and table views. The
server's predicate, cursor and ownership contracts belong to the Books list query contract Design. Display preferences
retain their account lifetime independently of the visit.

## Structure

`frontend/src/routes/library-params.ts` defines the serialisable visit schema, grouped filter model and effective API
projection. `useLibraryFilters()` in `frontend/src/lib/hooks/use-library-filters.ts` is mounted once by the page after
Router setup. The toolbar, rail, chips and empty-state actions receive state and dispatch functions from that owner.
They do not instantiate another filter owner.

Native NumberFields publish numeric values with snapping, steppers and intrinsic domains. Independently optional
DatePickers provide keyboard segments and calendars. SearchField and TextField publish raw text immediately. Native
controls own unfinished input; the application holds no separate draft or delayed filter queue.

`useLibraryPreferences()` resolves sort and display preferences. The page supplies its one sort intent handler to the
rail and table headers. `useInfiniteQuery` observes the current filter request, retaining previous pages through
`keepPreviousData` while a changed request loads.

## Interfaces and dependencies

The visit schema validates owned entry data at restoration. The Navigation API provides `currentEntry.getState()`,
`updateCurrentEntry` and `currententrychange`; React Router owns route navigation. The browser baseline requires recent
Navigation-capable browsers. The owner starts after Router initialisation and does not patch classic history.

The API projection produces `ListBooksParams` for `GET /api/v1/books`. Plain calendar dates become local-midnight
`created_at_gte` and next-local-midnight `created_at_lt` RFC 3339 instants through `@internationalized/date`. The upper
bound adds one calendar day before timezone conversion, including ordinary month/year rollovers and daylight-saving
changes.

`GET /auth/me/preferences` and `PATCH /auth/me/preferences` provide the account preferences and installation defaults.
Sort gestures write `sort_stack`; a non-empty override goes on list requests explicitly, while inheritance omits sort.
The shared QueryClient and structured list keys retain the `InfiniteData` pages, page parameters and consumed abort
signals used by pagination and row actions.

## Data and state

| State                               | Owner                          | Lifetime                      |
| ----------------------------------- | ------------------------------ | ----------------------------- |
| Published filters and optional view | Page-level `useLibraryFilters` | Navigation entry              |
| Unfinished native input             | Edited React Aria control      | Mounted control               |
| Sort and display overrides          | `useLibraryPreferences`        | Account preference            |
| First-paint display mirror          | Preference owner               | Per-user localStorage         |
| Loaded pages and row patches        | Shared QueryClient             | Query cache                   |
| Selected rows                       | Library page                   | Current intent and projection |

The `reverieLibrary` entry field holds the owned visit; writes preserve unrelated entry data. Filter state keeps raw
text and its selected operator even when the text is blank. Chips, counts and requests derive only effective predicates.
Complete inverted bounds remain entered. The native owner keeps immediate dispatch composition aligned with the
persisted snapshot; query completion and suggestion responses never write filter intent.

Callers include quick search, every rail section, section clear, chip removal, global and empty-state clear, view
gestures and the table-chunk fallback. View gestures also write the existing preference; chunk failure changes only the
visit override. Filter clear preserves both view and sort. The preference owner's 400 ms patch queue and local mirror
retain their existing lifetime; no filter value enters that queue or mirror.

## Runtime behaviour

A native edit dispatches to the current React visit snapshot. Same-turn functional patches compose before the matching
snapshot is persisted through `updateCurrentEntry`. An edit adds no history entry. Reload and traversal restore the
destination visit; a fresh push or replacement starts clean, including same-route navigation that reuses the mounted
page. Library has no prefetch loader, so the restored owner supplies the first list request. Browser query parameters
carry no filter or view state and have no compatibility decoder.

The list request uses current filter intent immediately. Sort retains `useDeferredValue` so its controls resolve at
gesture time while the request catches up. Initial uncached loading uses LibrarySkeleton. Later changes retain the
controls and previous pages with visible “Updating results…” and busy treatment. Placeholder pages cannot load more.
Successful responses replace placeholders, including real empty results for contradictory bounds. Range explanations
appear against settled current results, rather than describing retained rows as the current result set.

Displayed placeholder pages carry their originating query key. A second infinite-query observer reads that source cache
while placeholders are displayed, sharing the same query function and pagination options. It is enabled only during
placeholder display and uses an infinite stale time, so retention alone does not refetch the source. Cache patches and
undo update the visible rows, and conflict invalidation refetches the enabled source query. Row actions retain their
origin even when intent changes during a mutation. No placeholder data enters the destination cache.

API cursors remain page parameters; changing intent starts another list key, so a late old page cannot append to it.
Selection clears at filter, sort or view intent and when the displayed source key changes. Paging within the same query
retains selection.

Uniform Escape, close and outside dismissal retains published edits and returns focus through native overlay handling.
Closure creates no separate commit or rollback transaction.

## Failure and recovery

An absent or malformed owned payload starts a clean visit as a unit. Restoration preserves unrelated entry data and adds
no recovery write or UI; the next ordinary owner write replaces the owned payload. Intrinsic numeric domains and
calendar values are checked before restored data drives controls or requests.

Malformed API timestamps, numbers, `UUID` and boolean values retain `400` decode errors. Domain, cap, status and cursor
errors retain `422`; contradictory complete intervals return successful empty data. Network and server errors remain
errors. Initial or changed-query failures reach the route error boundary. Failed load-more requests retain loaded pages
and provide the existing Retry action.

A table chunk failure selects the visit's grid override without rewriting the account preference. A failed preference
write keeps the local choice for the current session and logs the failure; a late preference response cannot overwrite a
local choice. A missing or stale first-paint mirror can cause one correction when account preferences arrive.

## Security and operations

Visit state is plain schema-validated data and contains no credentials. The server validates API inputs independently,
uses bound SQL parameters and scopes results through row-level security. Shelf ownership and sort whitelisting remain
server responsibilities. Preference requests identify the caller through the authenticated account and retain their
existing row-level policies.

Native year editing can publish intermediate calendar-valid prefixes and therefore intermediate requests/results.
Browser component tests cover editor mapping; standalone Chromium and patched Firefox journeys cover bootstrap,
restoration, fresh entries, dismissal and request/result integration. Intercepted synthetic APIs do not establish real
authentication or database behaviour. This subject adds no service or operational setting.
