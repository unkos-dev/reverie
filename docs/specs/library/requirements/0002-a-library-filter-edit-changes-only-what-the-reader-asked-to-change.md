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

- Sibling edits made in the same turn both take effect. Checked by
  `same-turn sibling edits compose and preserve unrelated native entry data` in
  `frontend/tests/browser/use-library-filters.test.tsx`.
- Blur followed by Clear does not bring back the cleared condition. Checked by
  `blur, Clear and subsequent typing use the current intent without resurrection` in
  `frontend/tests/browser/use-library-filters.test.tsx`.
- Clear removes every filter condition and leaves view and sort unchanged. Checked by
  `clearing all filters preserves the visit view and the stored sort` in `frontend/tests/browser/LibraryPage.test.tsx`.
- Clearing one section or removing one chip leaves sibling sections unchanged. Checked by
  `a chip's remove control drops exactly that filter param` in `frontend/tests/browser/LibraryPage.test.tsx`.
- Entered text and its selected operator stay editable while they contribute no effective predicate. Checked by
  `raw text and its operator stay editable while they contribute no predicate` in
  `frontend/tests/browser/FilterRail.test.tsx`.
- The books request's effective filter arguments represent exactly the published edit and preserve sibling conditions,
  while dormant entered text and operators contribute no effective predicate. Checked by
  `frontend/tests/journeys/library-navigation.spec.ts`.
