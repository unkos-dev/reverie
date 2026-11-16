---
type: REQ
profile-version: 1
id: "REV-REQ-0080"
title: "Library return restores browsing context"
---

# Library return restores browsing context

## Statement

WHEN a reader returns from a book to a Library visit, the Library MUST restore that visit's loaded rows and browsing
position according to its saved anchor and scroll offset. The anchor is the first visible book and its position within
the viewport when the reader leaves. Restoration MUST yield to a newer filter, sort, view, navigation or user scroll.

## Rationale

Opening a book should not lose the reader's place in a large collection. Independent visits can use the same filters and
sort while retaining different amounts of the collection. Returning to either visit must preserve its own context while
current results load.

## Acceptance criteria

- R1: Returning to an unchanged visit restores its loaded rows, anchor position and scroll offset. Another visit with
  identical filters and sort can load a different extent and complete its own return check without changing the earlier
  visit's retained rows or geometry. Intermediate return pages preserve the displayed rows and position until the
  replacement is ready, including on repeated returns.
- R2: If the anchor remains listed but has moved, restoration places it at its saved position within the viewport.
- R3: If the anchor has left the list, restoration uses the previous absolute scroll offset.
- R4: If the anchor has left the list and its maximum scroll offset is less than the saved offset, restoration returns
  to the top. This also applies when the maximum is greater than zero, so clamping to the bottom does not satisfy this
  criterion.
- A newer filter, sort, view, navigation or user scroll prevents a delayed completion from restoring the old position or
  replacing the newer intent.
