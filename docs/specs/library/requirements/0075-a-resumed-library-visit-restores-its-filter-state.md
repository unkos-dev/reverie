---
type: REQ
profile-version: 1
id: "REV-REQ-0075"
title: "A resumed Library visit restores its filter state"
governed-by:
  - "REV-ADR-0066"
---

# A resumed Library visit restores its filter state

## Statement

A Library visit belongs to one
[session history entry](https://html.spec.whatwg.org/multipage/browsing-the-web.html#session-history-entry). Reloading
that entry, or reaching it again through any history traversal, resumes the same visit. Other navigations to Library
start a fresh visit, including replacement navigation to the same URL. WHEN a Library visit resumes, the Library MUST
restore the filter state last applied in that visit before it issues its first books list request, and MUST restore no
filters when that visit's saved state is absent or malformed. A visit's saved state holds its filter conditions, its
view and its saved position. It is malformed when any part of it fails to decode, including a view or position, or when
it holds a field the Library does not define; a malformed saved state restores none of its filters, even those that
decode. Restoration excepts a restored author, series or shelf selection once it is established that the viewer can no
longer see it: the first books request and the restored controls omit that selection, and keep every other restored
filter. While that cannot be established, the visit keeps every restored selection.

## Rationale

A reader who reloads the page or returns from a book depends on recovering the narrowing context they left. Restoring it
before the first books list request avoids fetching and showing an unfiltered list that is replaced a moment later.
Restoring part of a malformed saved state would show the reader a combination of conditions they never chose.

## Acceptance criteria

- Reload after rapid edits restores their final state before the first books list request. Checked by
  `rapid edits reload with their first request and no URL grammar` in
  `frontend/tests/journeys/library-navigation.spec.ts`.
- Library to book to Back restores the visit's filters and chips, and its next books request carries them. Checked by
  `book Back and Forward restore a visit; Back past Library does not undo edits` in
  `frontend/tests/journeys/library-navigation.spec.ts`.
- Forward restores its destination visit. Checked by
  `fresh same-route entry and direct context start clean; traversing restores each destination` in
  `frontend/tests/journeys/library-navigation.spec.ts`.
- Traversing to an earlier entry restores that entry's visit, including a traversal between two Library entries with no
  other page between them. Checked by
  `fresh same-route entry and direct context start clean; traversing restores each destination` in
  `frontend/tests/journeys/library-navigation.spec.ts`.
- A resumed visit whose saved state is absent, holds an invalid filter beside valid ones, or holds an invalid view
  starts with no filters, restoring none of its saved conditions. Checked by
  `REV-REQ-0075 AC5: unusable saved state restores no filters: %j` in
  `frontend/tests/browser/use-library-filters.test.tsx`.
- A resumed visit whose saved state holds valid filters and an invalid saved position starts with no filters. Checked by
  `REV-REQ-0075 AC5: unusable saved state restores no filters: %j` in
  `frontend/tests/browser/use-library-filters.test.tsx`.
- A restored selection the viewer can no longer see is absent from the first books request and from the restored
  controls, and every other restored filter, including other selections in the same facet, is present in both. Checked
  by `rapid edits reload with their first request and no URL grammar` in
  `frontend/tests/journeys/library-navigation.spec.ts`.
- When the viewer's visibility of its selections cannot be established, the visit's saved state keeps every restored
  selection. Checked by `rapid edits reload with their first request and no URL grammar` in
  `frontend/tests/journeys/library-navigation.spec.ts`.
