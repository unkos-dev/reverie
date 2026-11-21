---
type: REQ
profile-version: 1
id: "REV-REQ-0085"
title: "The Library never sends a save again on its own"
---

# The Library never sends a save again on its own

## Statement

WHEN a [Library save](0081-failed-library-saves-support-explicit-retry.md#statement) has been sent and the server may
have executed it, the Library MUST NOT send that save again unless the reader activates Retry. A request that the server
refuses for its CSRF token is refused before it executes, so the single resend of that request with a renewed token is
outside this obligation.

## Rationale

A save whose acknowledgement was lost may already have committed. Sending it again without the reader's decision can
apply a change twice or overwrite a newer value, and the reader would not know that a second write happened. Leaving
each resend to the reader keeps every write one the reader chose.

## Acceptance criteria

- After a temporary failure, the Library sends no further write for that save until the reader activates Retry. Checked
  by `Library Retry sends the retained edit again` in `frontend/tests/browser/LibraryPage.test.tsx`.
- A save that was sent and whose response never arrived leads to no further write until the reader activates Retry.
  Checked by `Library Retry sends the retained edit again` in `frontend/tests/browser/LibraryPage.test.tsx`.
