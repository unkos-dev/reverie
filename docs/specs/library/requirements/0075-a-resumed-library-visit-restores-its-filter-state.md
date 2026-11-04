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
filters when that visit's saved filter state is absent or malformed.

## Rationale

A reader who reloads the page or returns from a book depends on recovering the narrowing context they left. Restoring it
before the first books list request avoids fetching and showing an unfiltered list that is replaced a moment later.
Restoring part of a malformed saved state would show the reader a combination of conditions they never chose.

## Acceptance criteria

- Reload after rapid edits restores their final state before the first books list request.
- Library to book to Back restores filters, chips and results.
- Forward restores its destination visit.
- Traversing to an earlier entry restores that entry's visit, including when the Library page component is reused.
- A resumed visit whose saved filter state is absent or malformed starts with no filters, restoring none of its saved
  conditions rather than part of them.
