---
type: REQ
profile-version: 1
id: "REV-REQ-0081"
title: "Failed Library saves support explicit Retry"
---

# Failed Library saves support explicit Retry

## Statement

WHEN a Library save fails temporarily or ends with an uncertain outcome, the Library MUST offer Retry of that save,
keeping the reader's submitted input for it. A Library save is a book metadata or reading-state edit that the reader
commits in the Library table. A save fails temporarily when it receives no response, an HTTP status of `408`, `429` or
`500` and above, or a GraphQL error with the code `internal`. A save ends with an uncertain outcome when a response
arrives that the Library cannot use as an acknowledgement, such as a GraphQL error with no code, or an acknowledgement
whose values or validator are missing or malformed. A save that the server refuses for any other reason has neither
failed temporarily nor ended with an uncertain outcome.

## Rationale

A reader whose save fails because of the network or the server, or whose acknowledgement cannot be used, must be able to
recover it without typing the value again. The outcome of such a save is unknown, and sending it again may succeed. A
save the server refused would fail again unchanged, so Retry belongs only to temporary failures and uncertain outcomes.

## Acceptance criteria

- After a temporary failure of a metadata save or a reading-state save, the Edit outcomes region offers a Retry control
  for that save. Checked by `Library Retry sends the retained edit again` in
  `frontend/tests/browser/LibraryPage.test.tsx`.
- Activating Retry after a temporary failure sends the reader's submitted input without the reader entering it again.
  Checked by `Library Retry sends the retained edit again` in `frontend/tests/browser/LibraryPage.test.tsx`.
- After a save whose acknowledgement arrives unusable, the Edit outcomes region offers a Retry control for that save.
- While a Retry is pending, its control reads "Retrying…". Checked by `Library Retry sends the retained edit again` in
  `frontend/tests/browser/LibraryPage.test.tsx`.
- A save that the server refuses for a stale baseline or a locked field offers no Retry control. Checked by
  `Library Retry sends the retained edit again` in `frontend/tests/browser/LibraryPage.test.tsx`.
