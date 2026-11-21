---
type: REQ
profile-version: 1
id: "REV-REQ-0088"
title: "A Library batch Retry repeats only the writes that failed"
---

# A Library batch Retry repeats only the writes that failed

## Statement

WHEN a Library batch action's write fails for some of its selected books, the batch Retry MUST resend exactly the books
whose write [failed temporarily](0081-failed-library-saves-support-explicit-retry.md#statement), each with its own
original input and
[editing baseline](../../api/requirements/0082-graphql-edit-acknowledgements-carry-the-validator-of-their-values.md#statement).
A batch action is one action the reader applies to every selected book in the Library, such as setting a reading status.

## Rationale

One batch can succeed for some books, be refused for others and fail temporarily for the rest. Sending a book's write
again after it succeeded repeats it, and sending a refused book's write again fails again or overwrites a change the
reader never saw. Each book's own baseline keeps a resend from authorising a write against another book's state.

## Acceptance criteria

- Each book's first write in a batch carries that book's own baseline. Checked by
  `Library Retry sends the retained edit again` in `frontend/tests/browser/LibraryPage.test.tsx`.
- In a batch where one book fails temporarily and another is refused as stale, batch Retry sends again only the write of
  the book that failed, the same write as its first attempt. Checked by `Library Retry sends the retained edit again` in
  `frontend/tests/browser/LibraryPage.test.tsx`.
- A book refused as stale keeps its own feedback after the batch, and the batch offers no Reload control of its own. The
  per-book feedback is checked by `Library Retry sends the retained edit again` in
  `frontend/tests/browser/LibraryPage.test.tsx`; no automated check asserts the absence of a batch Reload control.
