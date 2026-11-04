---
type: REQ
profile-version: 1
id: "REV-REQ-0077"
title: "Library filter edits add no history entries"
governed-by:
  - "REV-ADR-0066"
---

# Library filter edits add no history entries

## Statement

WHEN the reader edits Library filters, the Library MUST NOT add a
[session history entry](https://html.spec.whatwg.org/multipage/browsing-the-web.html#session-history-entry) for the
edit.

## Rationale

Filter edits refine one visit; they are not places the reader navigated to. If each edit added a history entry, Back
would replay the reader's narrowing steps instead of returning them to where they came from.

## Acceptance criteria

- After filter edits, Back reaches the entry that preceded the edited visit, which may itself be a Library visit, rather
  than stepping through earlier filter states.
