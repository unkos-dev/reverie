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
filters when that visit's saved state is absent or malformed. WHEN a Library visit resumes in a new document load and
the viewer can no longer see an author, series or shelf selected in that saved state, the Library MUST restore no
filters; an in-session traversal whose cached records have not been invalidated MAY keep those selections until the next
document load or record invalidation. A visit's saved state holds its filter conditions, its view and its saved
position. It is malformed when any part of it fails to decode, including a view or position, or when it holds a field
the Library does not define; a malformed saved state restores none of its filters, even those that decode. While the
viewer's visibility of the restored selections cannot be established, the visit MUST keep its saved state intact.

## Rationale

A reader who reloads the page or returns from a book depends on recovering the narrowing context they left. Restoring it
before the first books list request avoids fetching and showing an unfiltered list that is replaced a moment later.
Restoring part of a malformed saved state, or applying the remaining filters after one selected author, series or shelf
disappears, would show the reader a combination of conditions they never chose. Keeping the saved state intact while
visibility cannot be established ensures a temporary outage does not erase a valid visit.

## Acceptance criteria

- Reload after rapid edits restores their final state before the first books list request. Checked by
  `rapid edits reload with their first request and no URL grammar` in
  `frontend/tests/journeys/library-navigation.spec.ts`.
- Library to book to Back restores filters, chips and results. Checked by
  `book Back and Forward restore a visit; Back past Library does not undo edits` in
  `frontend/tests/journeys/library-navigation.spec.ts`.
- Forward restores its destination visit. Checked by
  `book Back and Forward restore a visit; Back past Library does not undo edits` in
  `frontend/tests/journeys/library-navigation.spec.ts`.
- Traversing to an earlier entry restores that entry's visit, including when the Library page component is reused.
  Checked by `traversing between two Library entries restores each visit` in
  `frontend/tests/journeys/library-navigation.spec.ts`.
- A resumed visit whose saved state is absent or malformed, including valid filters with an invalid saved position,
  starts with no filters, restoring none of its saved conditions rather than part of them. Checked by
  `REV-REQ-0075 AC5: unusable saved state restores no filters: %j` in
  `frontend/tests/browser/use-library-filters.test.tsx`.
- When a visit resumes in a new document load and any selected author, series or shelf is no longer visible to the
  viewer, the first books list request carries no filters and the restored controls show no active filters. Checked by
  `rapid edits reload with their first request and no URL grammar` in
  `frontend/tests/journeys/library-navigation.spec.ts`.
- When the viewer's visibility of the restored selections cannot be established, the visit's saved state keeps every
  restored filter intact. Checked by `rapid edits reload with their first request and no URL grammar` in
  `frontend/tests/journeys/library-navigation.spec.ts`.
