---
type: REQ
profile-version: 1
id: "REV-REQ-0081"
title: "Failed Library saves support explicit Retry"
---

# Failed Library saves support explicit Retry

## Statement

WHEN a Library save fails temporarily, the Library MUST offer Retry of that save. A Library save is a book metadata or
reading-state edit that the reader commits in the Library table. A save fails temporarily when it receives no response,
an HTTP status of `408`, `429` or `500` and above, or a GraphQL error with the code `internal`. A save that the server
refuses for any other reason has not failed temporarily.

## Rationale

A reader whose save fails because of the network or the server must be able to recover it without typing the value
again. The outcome of such a save is unknown, and sending it again may succeed. A save the server refused would fail
again unchanged, so Retry belongs only to temporary failures.

## Acceptance criteria

- After a temporary failure of a metadata save or a reading-state save, the Edit outcomes region offers a Retry control
  for that save. Checked by `Library Retry sends the retained edit again` in
  `frontend/tests/browser/LibraryPage.test.tsx`.
- While a Retry is pending, its control reads "Retrying…". Checked by `Library Retry sends the retained edit again` in
  `frontend/tests/browser/LibraryPage.test.tsx`.
- A save that the server refuses for a stale baseline or a locked field offers no Retry control. Checked by
  `Library Retry sends the retained edit again` in `frontend/tests/browser/LibraryPage.test.tsx`.
