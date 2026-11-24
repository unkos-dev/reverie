---
type: DESIGN
profile-version: 1
id: "REV-DESIGN-0001"
title: "Library filter and sort state"
satisfies:
  - "REV-REQ-0001"
  - "REV-REQ-0002"
  - "REV-REQ-0075"
  - "REV-REQ-0076"
  - "REV-REQ-0077"
  - "REV-REQ-0080"
  - "REV-REQ-0092"
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
projection. The route loader in `frontend/src/routes/library.tsx` prepares each Library destination's
`LibraryVisitOwner` (`frontend/src/lib/relay/library-visit.ts`), its restored visit and its screen query reference. The
page calls `useLibraryFilters` (`frontend/src/lib/hooks/use-library-filters.ts`) once with that owner's `LibraryIntent`
and the loader's visit. The toolbar, filter panel, chips and empty-state actions receive state and dispatch functions
from that owner. They do not instantiate another filter owner.

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
feedback. Each typeahead reads the `TypeaheadMultiSelect_suggestions` fragment on the screen query. A search runs while
the popover is open, the control is enabled and the trimmed term has from 2 to 100 Unicode scalar values. It fetches
that vocabulary's suggestions with `fetchQuery`, then refetches the fragment with `store-only`, which reads the fetched
records without suspending. Starting a search, closing the popover or leaving the term ineligible unsubscribes the
previous search, so only the latest term can update the list or its error. Suggestions return up to ten ranked choices.

Shelf and series selections are GraphQL ID arrays, matching ANY within a facet and AND between facets. A native multiple
ListBox lists the owned shelves from the screen query's shelf list, which the page loads to completion. Toggle selection
and select-all publish actual IDs; empty selection is unrestricted, including unassigned books. Oversized selection
remains editable with associated feedback while list requests stay blocked. Section Clear and per-ID chip removal use
the same visit owner.

Series uses the multiple ComboBox and TagGroup composition with GraphQL ID identity. Native `allowsCustomValue` retains
free search text on commit and dismissal; only selected identified items publish predicates. Controlled query text
remains after selection and does not depend on narrowed Library results. One label read, the `LibraryPage_labels`
fragment on the screen query, names the chips, the rail's Series and Authors selections and the shelf header. It selects
each selected author's and series' ID and name, and each selected shelf's ID, name and item count. No response writes
filter intent.

`useLibraryPreferences` reads the preferences fragment on `Viewer.libraryPreferences` and resolves each group as the
reader's override, else the installation default. It resolves the sort override separately from the effective stack. The
page supplies its one sort intent handler to the filter panel, Covers sort control and table headers. `useLibraryBooks`
owns one displayed Relay source and a candidate replacement through the account-scoped `LibraryVisitOwner`. Native
fragments render the displayed connection and read complete metadata and reading revisions for edits.

## Interfaces and dependencies

The visit schema validates owned entry data at restoration. The Navigation API provides `currentEntry`, its `id` and
`getState()`, `updateCurrentEntry`, `activation`, the `navigate` event and entry disposal; React Router owns route
navigation and runs the Library loader. The browser baseline requires recent Navigation-capable browsers. The owner does
not patch classic history.

The visit projects to `ListBooksParams`, then `libraryVariables` builds typed GraphQL filters and sort levels. Plain
calendar dates become local-midnight `created_at_gte` and next-local-midnight `created_at_lt` RFC 3339 instants through
`@internationalized/date`. The upper bound adds one calendar day before timezone conversion, including ordinary
month/year rollovers and daylight-saving changes.

`Viewer.libraryPreferences` and `updateLibraryPreferences` provide the account overrides and installation defaults
through `services::preferences`. The HTTP preference endpoints share that service. Each gesture commits one
`updateLibraryPreferences` mutation with an optimistic store update that changes only that gesture's preference field.
Sort gestures write the structured `sort`, or `null` to reset it; a column toggle writes `columns` with that one column;
the Columns reset writes `resetHiddenColumns`. The Library sends its books requests through GraphQL. A request carries
the reader's non-empty override as ordered typed `sort` levels, and with no override it omits `sort` or supplies `null`,
so the server applies the installation default. It never serialises the installation-default levels, so an inheriting
reader's list-query key depends only on the absent override, never on server defaults the client cannot see. `listBooks`
(`frontend/src/api/books.ts`) is an HTTP adapter that serialises the parameters it is given and omits any that are
absent, including `sort`. Native query references retain connection membership. Each source keeps its operation,
variables and generation key. The editing owner retains submitted values and their starting validators.

## Data and state

| State                               | Owner                                       | Lifetime                      |
| ----------------------------------- | ------------------------------------------- | ----------------------------- |
| Published filters and optional view | The entry's `LibraryVisitOwner`             | Navigation entry              |
| Unfinished native input             | Edited React Aria control                   | Mounted control               |
| Sort and display overrides          | Relay record `Viewer.libraryPreferences`    | Account preference            |
| Labels and the owned shelf list     | Relay, retained by the bootstrap and screen | Account and Navigation entry  |
| Loaded connection and editing pairs | Relay and displayed source                  | Account and Navigation entry  |
| Selected rows                       | Library page                                | Current intent and projection |

The `reverieLibrary` entry field holds the owned visit; writes preserve unrelated entry data. Filter state keeps raw
text and its selected operator even when the text is blank. Chips and counts describe effective predicates. Requests
retain raw ISBN-13 contains text, so the server applies its raw length cap before separator removal; the client uses the
same raw request value for eligibility and source identity. Complete inverted bounds remain entered. Only the entry's
visit owner writes its entry state: dispatched edits, captured positions and the route load's reset when a restored
selection is no longer visible. Query completion and suggestion responses never write filter intent.

Callers include quick search, every rail section, section clear, chip removal, global and empty-state clear, view
gestures and the table-chunk fallback. View gestures also write the account preference; chunk failure changes only the
visit override. Filter clear preserves both view and sort. No filter value enters a preference write. The installation
default hides Subtitle; an override that hides nothing shows every column.

## Runtime behaviour

A native edit dispatches to the visit owner, which applies each functional patch to its current visit, so same-turn
patches compose, and persists the result through `updateCurrentEntry`. An edit adds no history entry. An edit that
changes the author, series or shelf selections refetches the label read with `store-or-network` in the same update. The
selected records are already in the store, so the refetch does not suspend. Browser query parameters carry no filter or
view state.

Each Library route load resolves its destination before the page renders. A traversal whose destination is the current
entry, or the document's first load of its activation entry, settles on that entry when the entry's path and query match
the load: it reuses or creates the entry's owner and restores the entry's saved visit. Any other load, including a
same-route push or replacement, gets a provisional owner with an empty visit, which the page binds to the current entry
when it mounts. A push runs its loader before the browser records the new entry, so a traversal settles only the one
load it reached. The owner rejects an intent write, a position write and a books-list activation once its entry is no
longer current, so a departing page cannot write into the destination's visit during a pending traversal.

The loader confirms the account, then reads `libraryBootstrapQuery`: the account's preferences and lookups of the
restored author, series and shelf IDs, each sent once. Genre, mood and tag values are names and are not looked up. When
any lookup omits a restored ID, the loader resets the visit's filters and saved position to empty while keeping any view
override. Only a successful bootstrap establishes that the viewer can no longer see a selection; a failed bootstrap
resets nothing. The owner writes the reset visit to its entry only while that entry is current. The loader then starts
`LibraryPageQuery` with the resolved visit's selections, and the page reads it under Suspense with LibrarySkeleton as
the fallback. The first eligible document load sends the bootstrap and the screen query, which includes books. A push
with stored preferences sends the screen query alone. A label lookup is answered from the store when every requested
record is already there, as it is for a pick from suggestions or the shelf list.

A bootstrap answered from the store keeps its selections until a deletion invalidates a selected record or a fetch
occurs. Deleting a shelf invalidates that shelf's record, so a return to a visit that selected it fetches the bootstrap
and resets the visit's filters. Authors and series leave the visible set only through edits and enrichment, which
invalidate no record, so a retained visit keeps such a selection until its bootstrap is next fetched.

The requested source follows published filter intent immediately. A request is eligible only when each multi-value
predicate contains at most 20 values and every active text predicate contains at most 200 Unicode scalar values after
trimming surrounding whitespace. Blank optional text has no predicate; text retained under an `empty` operator is
dormant. SearchField and TextField use controlled `isInvalid` and associated FieldError feedback immediately, without
native `maxLength` truncation. Raw overlong text and oversized selections remain published and persisted, including
across reload and traversal. An ineligible visit prevents the whole list request, pagination and source refetches. The
results region explains that results are paused. Correction or Clear restores request eligibility. Sort controls resolve
at gesture time while a replacement query loads. Initial uncached loading uses LibrarySkeleton. Later changes retain the
controls with visible “Updating results…” and busy treatment during fetching. Pending recovery without previous pages
uses a skeleton inside the results region; neither empty state appears until the request settles. Retained results
cannot load more while a replacement is pending. Successful responses adopt their source, including real empty results
for contradictory bounds. Range explanations appear against settled current results, rather than describing retained
rows as the current result set.

Each Navigation entry owns distinct connection generations through `dynamicKey_UNSTABLE: $sourceKey`. A provisional
owner allocates a distinct key for a push before the browser assigns the entry ID. The owner fetches the replacement's
first page while the displayed reader uses `store-only`. The candidate reader mounts once that page is available and
extends it through `usePaginationFragment` in a separate Suspense boundary. Pagination and adoption use ordinary
updates, so the shell's document view transition does not capture pointer input for those updates. Relay retains the
displayed connection and its record data. The editing owner commits saves above the virtualised rows.

The synchronous Navigation `navigate` event captures an anchor, the first visible book and its position within the
viewport, together with the loaded extent and absolute scroll position. Entry state contains only serialisable geometry
and visit intent. Relay retains references in memory. A fresh return reuses the retained connection without a request.
Relay marks a query stale after five minutes. A committed edit invalidates each retained connection whose sort or filter
depends on the changed field. A work-level Library save also invalidates connections containing another edition of that
work. A stale cell refusal invalidates connections containing the book or a sibling edition for metadata edits. Active
connections rebuild and inactive connections rebuild on return. A stale return renders the retained source while the
candidate loads both the saved extent and anchor, or reaches the connection end. If a later rebuild page fails with the
`invalid-cursor` execution error code, the owner restarts the rebuild once from page one. Intermediate pages preserve
the displayed books and position until the complete replacement is ready, including on repeated returns. The search for
an absent anchor exhausts the connection before applying the fallback. The owner adopts its ready reference once. A
moved anchor restores at its saved viewport position as far as the scroll range allows. An absent anchor uses the saved
absolute offset, or zero when that offset exceeds the new range. Account, entry, source and generation checks reject
obsolete completion. New intent and user scroll cancel delayed restoration. Entry disposal and account loss release
retained references.

Selection clears at filter, sort or view intent and displayed-source replacement. Pagination retains selection. A cell
editor captures the opened value and its validator. A failed save shows one dismissible notice above the table.

The batch bar captures selected loaded IDs once. Add to shelf runs serial membership writes. Set status uses each book's
matched reading baseline and reports success and failure counts. Refresh metadata schedules enrichment for non-child
callers and reports scheduling separately from completed enrichment. Successful IDs leave selection.

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
projection uses `shelf_any` and `series_any`; the scalar `shelf` and `series` API parameters serve other callers. The
bootstrap lookups and the label read send each selected ID once; displayed rows retain their owning source.

A failed bootstrap reaches the route error page before the loader writes anything, so the saved visit keeps every
restored selection, and a later load with the server healthy restores them. A failed screen query also reaches the route
error page; by then the loader has already persisted any filter reset. A failed continuation page of the shelf list
stops loading shelves, keeps the loaded ones and gives the Shelf section and batch bar their degraded state; a failed
first page fails the screen query. A failed suggestion search logs the failure and shows "Couldn't load suggestions. Try
again." in the popover while the input keeps its element, focus and text. A term longer than 100 Unicode scalar values
sends no request and shows the same text.

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
Library visit. Switch to grid view selects the visit's grid override without rewriting the account preference. A
preference gesture shows its optimistic value at once. A refused or failed write logs the failure, and Relay rolls the
value back to the stored one.

## Security and operations

Visit state is plain schema-validated data and contains no credentials. The server validates API inputs independently,
uses bound SQL parameters and scopes results through row-level security. Shelf ownership and sort whitelisting are
server responsibilities. Preference requests identify the caller through the authenticated account, and the
`user_preferences_owner` row-level policy scopes them to that account's row.

Native year editing can publish intermediate calendar-valid prefixes and therefore intermediate requests and results.
This subject adds no service or operational setting.
