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

- Sibling edits made in the same turn both take effect.
- Blur followed by Clear does not bring back the cleared condition.
- Clear removes every filter condition and leaves view and sort unchanged.
- Clearing one section leaves sibling sections unchanged.
- Entered text and its selected operator are kept, and remain editable, while they contribute no effective predicate.
