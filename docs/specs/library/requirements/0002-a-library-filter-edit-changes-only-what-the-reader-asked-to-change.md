---
type: REQ
profile-version: 1
id: "REV-REQ-0002"
title: "A Library filter edit changes only what the reader asked to change"
governed-by:
  - "REV-ADR-0066"
---

# A Library filter edit changes only what the reader asked to change

## Statement

WHEN the reader edits Library filters, the Library MUST apply exactly the requested change: the edited condition takes
the requested value and keeps it until the reader changes it again within the visit, and every other filter condition,
entered text, selected operator, view and sort keeps its value.

## Rationale

A reader narrowing the Library builds one query from several conditions. An edit that drops a sibling condition, brings
back a cleared one, or discards text the reader entered changes the results without the reader asking, and the reader
may not notice which condition moved. View and sort are standing choices, so clearing filters leaves them alone.

## Acceptance criteria

- Two edits to different filter conditions made in one reader action both take effect. Checked by
  `same-turn sibling edits compose and preserve unrelated native entry data` in
  `frontend/tests/browser/use-library-filters.test.tsx`.
- An edit that a field publishes when it loses focus, followed by Clear in the same reader action, does not bring back
  the cleared condition. Checked by `blur, Clear and subsequent typing use the current intent without resurrection` in
  `frontend/tests/browser/use-library-filters.test.tsx`.
- Clear removes every filter condition and leaves view and sort unchanged. Checked by
  `clearing all filters preserves the visit view and the stored sort` in `frontend/tests/browser/LibraryPage.test.tsx`.
- Clearing one filter section, or removing one filter chip, leaves every other section unchanged. Checked by
  `a chip's remove control drops exactly that filter param` in `frontend/tests/browser/LibraryPage.test.tsx`.
- Entered text and its selected operator are kept, and remain editable, while they restrict no results. Text restricts
  no results when it is blank, or when its operator is "is empty", which ignores the text. Checked by
  `raw text and its operator stay editable while they contribute no predicate` in
  `frontend/tests/browser/FilterRail.test.tsx`.
- After an edit, the books request filters by exactly the conditions that restrict results, including the conditions the
  edit did not touch, and by no text that restricts no results.
