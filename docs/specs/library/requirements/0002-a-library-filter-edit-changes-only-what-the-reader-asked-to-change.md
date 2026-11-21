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

- Two edits to different filter conditions made in one reader action both take effect.
- An edit that a field publishes when it loses focus, followed by Clear in the same reader action, does not bring back
  the cleared condition.
- Clear removes every filter condition and leaves view and sort unchanged.
- Clearing one filter section, or removing one filter chip, leaves every other section unchanged.
- Entered text and its selected operator are kept, and remain editable, while they restrict no results. Text restricts
  no results when it is blank, or when its operator is "is empty", which ignores the text.
- After an edit, the books request filters by exactly the conditions that restrict results, including the conditions the
  edit did not touch, and by no text that restricts no results.
