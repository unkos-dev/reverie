---
type: DESIGN
profile-version: 1
id: "REV-DESIGN-0001"
title: "Library filter and sort state"
satisfies:
  - "REV-REQ-0001"
  - "REV-REQ-0002"
  - "REV-REQ-0003"
  - "REV-REQ-0075"
  - "REV-REQ-0076"
  - "REV-REQ-0077"
  - "REV-REQ-0080"
  - "REV-REQ-0081"
governed-by:
  - "REV-ADR-0066"
  - "REV-ADR-0075"
  - "REV-ADR-0047"
---

# Library filter and sort state

## Purpose and boundaries

The Library surface owns the reader's current filter intent, visit view override and resolved account sort, and the
persistence and restoration of that visit on its Navigation entry. It projects those values onto the books-list request
and shares one Relay connection with Covers and Table. The server's predicate, cursor and ownership contracts belong to
the Design "Books list query contract". Display preferences have an account lifetime, independent of the visit.

## Structure

`frontend/src/routes/library-params.ts` defines the serialisable visit schema, grouped filter model and effective API
projection. `useLibraryFilters()` in `frontend/src/lib/hooks/use-library-filters.ts` is mounted once by the page after
Router setup. The toolbar, filter panel, chips and empty-state actions receive state and dispatch functions from that
owner. They do not instantiate another filter owner.

Native NumberFields publish numeric values with snapping, steppers and intrinsic domains. Independently optional
DatePickers provide keyboard segments and calendars. SearchField and TextField publish raw text immediately. Native
controls own unfinished input; the application holds no separate draft or delayed filter queue.

Library offers ISBN-13 contains and is-empty filters. The visit retains raw ISBN-13 text, including dormant text under
is-empty; the server normalises search separators. A shared ISBN-13 predicate-presence check treats separator-only
contains text as dormant for chips, section and overall counts, and filtered-empty classification. Is-empty remains
active independently of retained text.

Vocabulary filters compose multiple-selection ComboBoxes with a limit of 20 values per mode. At the limit,
`disabledKeys` disables suggestions outside the current selection while selected options and removal tags remain usable.
Associated description text explains how to make room; oversized selections use controlled invalidity and FieldError
feedback.

Shelf and series selections are GraphQL ID arrays, matching ANY within a facet and AND between facets. A native multiple
ListBox keeps the independently fetched owned shelf options visible. Toggle selection and select-all publish actual IDs;
empty selection is unrestricted, including unassigned books. Oversized selection remains editable with associated
feedback while list requests stay blocked. Section Clear and per-ID chip removal use the same visit owner.

Series uses the multiple ComboBox and TagGroup composition with GraphQL ID identity. Native `allowsCustomValue` retains
free search text on commit and dismissal; only selected identified items publish predicates. Its independent server
suggestions require two query characters and return up to twenty ranked choices. Controlled query text remains after
selection and does not depend on narrowed Library results. `useSeriesLabels` reads the independent bounded
`readerSeriesLabelsQuery` for restored chips and selections; missing or failed labels retain the selected ID with a
fallback showing the first eight characters of its UUID. No response writes filter intent.

`useLibraryPreferences()` resolves sort and display preferences. The page supplies its one sort intent handler to the
filter panel, Covers sort control and table headers. `useLibraryBooks` owns one displayed Relay source and a candidate
replacement through the account-scoped `LibraryVisitOwner`. Native fragments render the displayed connection. Detached
raw list projections retain the metadata and reading values with their matching editing baselines.

## Interfaces and dependencies

The visit schema validates owned entry data at restoration. The Navigation API provides `currentEntry.getState()`,
`updateCurrentEntry` and `currententrychange`; React Router owns route navigation. The browser baseline requires recent
Navigation-capable browsers. The owner starts after Router initialisation and does not patch classic history.

The visit projects to `ListBooksParams`, then `libraryVariables` builds typed GraphQL filters and sort levels. Plain
calendar dates become local-midnight `created_at_gte` and next-local-midnight `created_at_lt` RFC 3339 instants through
`@internationalized/date`. The upper bound adds one calendar day before timezone conversion, including ordinary
month/year rollovers and daylight-saving changes.

`libraryPreferences` and `updateLibraryPreferences` provide the account overrides and installation defaults through
`services::preferences`. The HTTP preference endpoints share that service. Sort gestures write `sortStack`; a non-empty
override becomes ordered typed sort levels. With no override, `sort` is omitted or null and the server uses the
installation default. Native query references retain connection membership. Each source keeps its operation, variables
and detached editing pairs for row actions.

## Data and state

| State                               | Owner                          | Lifetime                      |
| ----------------------------------- | ------------------------------ | ----------------------------- |
| Published filters and optional view | Page-level `useLibraryFilters` | Navigation entry              |
| Unfinished native input             | Edited React Aria control      | Mounted control               |
| Sort and display overrides          | `useLibraryPreferences`        | Account preference            |
| First-paint display mirror          | Preference owner               | Per-user localStorage         |
| Loaded connection and editing pairs | Relay and displayed source     | Account and Navigation entry  |
| Selected rows                       | Library page                   | Current intent and projection |

The `reverieLibrary` entry field holds the owned visit; writes preserve unrelated entry data. Filter state keeps raw
text and its selected operator even when the text is blank. Chips and counts describe effective predicates. Requests
retain raw ISBN-13 contains text, so the server applies its raw length cap before separator removal; the client uses the
same raw request value for eligibility and source identity. Complete inverted bounds remain entered. The native owner
keeps immediate dispatch composition aligned with the persisted snapshot; query completion and suggestion responses
never write filter intent.

Callers include quick search, every rail section, section clear, chip removal, global and empty-state clear, view
gestures and the table-chunk fallback. View gestures also write the account preference; chunk failure changes only the
visit override. Filter clear preserves both view and sort. The preference owner's 400 ms patch queue and local mirror
have an account lifetime; no filter value enters that queue or mirror. Queued preference writes flush on unmount only
while their initiating account epoch remains current. Cold hidden columns contain Subtitle; an explicit empty override
shows every column.

## Runtime behaviour

A native edit dispatches to the current React visit snapshot. Same-turn functional patches compose before the matching
snapshot is persisted through `updateCurrentEntry`. An edit adds no history entry. Reload and traversal restore the
destination visit; a fresh push or replacement starts clean, including same-route navigation that reuses the mounted
page. Library has no prefetch loader, so the restored owner supplies the first list request. Browser query parameters
carry no filter or view state.

The requested source follows published filter intent immediately. A request is eligible only when each multi-value
predicate contains at most 20 values and every active text predicate contains at most 200 Unicode scalar values after
trimming surrounding whitespace. Blank optional text has no predicate; text retained under an `empty` operator is
dormant. SearchField and TextField use controlled `isInvalid` and associated FieldError feedback immediately, without
native `maxLength` truncation. Raw overlong text and oversized selections remain published and persisted, including
across reload and traversal. An ineligible visit prevents the whole list request, pagination and source refetches. The
results region explains that results are paused; correction or Clear restores request eligibility. Sort uses
`useDeferredValue` so its controls resolve at gesture time while the request catches up. Initial uncached loading uses
LibrarySkeleton. Later changes retain the controls with visible “Updating results…” and busy treatment during fetching.
Pending recovery without previous pages uses a skeleton inside the results region; neither empty state appears until the
request settles. Retained results cannot load more while a replacement is pending. Successful responses adopt their
source, including real empty results for contradictory bounds. Range explanations appear against settled current
results, rather than describing retained rows as the current result set.

Each Navigation entry and source generation has a distinct native connection identity through
`dynamicKey_UNSTABLE: $sourceKey`. Pagination preserves that identity. Replacements allocate another generation and keep
the displayed connection intact while bounded reads rebuild the candidate. Native Relay connection handlers assemble
pages; there is no custom merge handler or second reader cache. A replaced source remains retained until its editors,
failed attempts and Undo entries release their uses.

The synchronous Navigation `navigate` event captures an anchor, the first visible book and its position within the
viewport, together with the loaded extent and absolute scroll position. Entry state contains only serialisable geometry
and visit intent. Relay references remain in memory. Return renders the retained source while a candidate reads from
page one through the saved extent and anchor, or the connection end. Intermediate pages preserve the displayed books and
position until the complete replacement is ready, including on repeated returns. The search for an absent anchor
exhausts the connection before applying the fallback. The owner adopts its ready reference once. A moved anchor restores
at its saved viewport position as far as the scroll range allows. An absent anchor uses the saved absolute offset, or
zero when that offset exceeds the new range. Account, entry, source and generation checks reject obsolete completion.
New intent and user scroll cancel delayed restoration. Entry disposal and account loss release retained references.

Selection clears at filter, sort or view intent and displayed-source replacement. Pagination retains selection. The
Library editing owner keeps at most ten exact field operations above conditional results. Filtering, sorting, loading
and empty results preserve Undo. Leaving Library or Table editing clears it.

The batch bar captures selected loaded IDs once. Add to shelf runs serial membership writes. Set status captures one
status and each target's matched reading baseline before sending. Refresh metadata schedules enrichment for non-child
callers and reports scheduling separately from completed enrichment. Each target retains its own outcome; successful IDs
leave selection. Retry includes only temporary or uncertain failures with their retained inputs. Conflicts and
validation refusals keep their per-book recovery in Edit outcomes, without a bulk Reload control.

At viewport widths of at least 1600 px, the filter editor occupies a non-modal panel beside the collection. Opening it
narrows the collection without hiding columns. Its translucent material matches the floating bars, and its top stays
below the sticky navigation. The Filters control toggles it; its close control returns focus to Filters. The collection
stays interactive, and Escape and outside presses leave the panel open. Below 1600 px, a native modal sheet provides
Escape, close and outside dismissal. Both presentations retain published edits. Closure creates no separate commit or
rollback transaction.

Covers uses a native multiple-selection GridList. A card action opens the book page, while its selection checkbox and
native keyboard selection feed the page's batch selection. The Covers sort control dispatches a single-level stack
through the same preference intent handler as the table and full-stack editor. A native slider sets cover width from 120
to 300 px in one-pixel steps. The grid responds during dragging; the final width persists per browser. Its default is
180 px. This display setting does not change filter intent, selection or server preferences.

## Failure and recovery

An absent or malformed owned payload starts a clean visit as a unit. Restoration preserves unrelated entry data and adds
no recovery write or UI; the next ordinary owner write replaces the owned payload. Intrinsic numeric domains and
calendar values are checked before restored data drives controls or requests.

A scalar shelf or series value in the owned payload decodes to a singleton array; an absent value becomes an empty
array. This decoding adds no recovery write. A malformed member rejects the owned payload atomically. The wire
projection uses `shelf_any` and `series_any`; the scalar `shelf` and `series` API parameters serve other callers. Label
requests de-duplicate IDs and stay bounded; displayed rows retain their owning source.

Exceeding a text or selection-count limit does not make an otherwise well-formed visit malformed. Such visits retain all
filters and the view override for correction or clearing, while request eligibility enforces those limits.

The Design "Books list query contract" describes how the server refuses a list request
([Failure and recovery](0019-books-list-query-contract.md#failure-and-recovery)); contradictory complete intervals
return successful empty data. `useLibraryBooks` treats a books-list execution error whose code is `validation` as a
refused request: it stays in the results region with recoverable feedback and no automatic retry. Search, Filters and
Clear remain usable; correction or Clear can request results again. List validation feedback stays with the owning
results region. Other initial or changed-query failures reach the route error boundary. Other failed load-more requests
keep loaded pages and offer Retry.

A table-load failure stays inside the results region. Reload page starts a fresh application load and restores the
Library visit. Switch to grid view selects the visit's grid override without rewriting the account preference. A failed
preference write keeps the local choice for the current session and logs the failure; a late preference response cannot
overwrite a local choice. A missing or stale first-paint mirror can cause one correction when account preferences
arrive.

## Security and operations

Visit state is plain schema-validated data and contains no credentials. The server validates API inputs independently,
uses bound SQL parameters and scopes results through row-level security. Shelf ownership and sort whitelisting are
server responsibilities. Preference requests identify the caller through the authenticated account, and the
`user_preferences_owner` row-level policy scopes them to that account's row.

Native year editing can publish intermediate calendar-valid prefixes and therefore intermediate requests and results.
This subject adds no service or operational setting.
