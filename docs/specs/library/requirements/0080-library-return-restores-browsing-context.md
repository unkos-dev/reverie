---
type: REQ
profile-version: 1
id: "REV-REQ-0080"
title: "Library return restores browsing context"
---

# Library return restores browsing context

## Statement

WHEN a reader returns to a Library visit through browser history, the Library MUST restore that visit's loaded books and
the reader's place. Restoration MUST NOT override a newer filter, sort, view, navigation or user scroll.

## Rationale

Opening a book should not lose the reader's place in a large collection. Independent visits can use the same filters and
sort while retaining different amounts of the collection. Returning to either visit must preserve its own context.

## Acceptance criteria

- R1a: Returning to an unchanged visit restores its loaded books, with the top book at its previous position within 1
  CSS pixel.
- R1b: Two visits with identical filters and sort each restore their own loaded books and place, including when their
  loaded extents differ.
- R2: If the top book remains listed but has moved, restoration places it at its saved position within the viewport,
  within 1 CSS pixel, as far as the scroll range allows.
- Y1: A newer filter, sort, view, navigation or user scroll prevents a delayed completion from restoring the old
  position or replacing the newer intent.

Checked by `unchanged returns preserve each visit's books and known-book position` for R1a and R1b,
`a moved book returns to its saved position` for R2, and `a user scroll wins during a held return` for Y1 in
`frontend/tests/journeys/library-navigation.spec.ts`.
