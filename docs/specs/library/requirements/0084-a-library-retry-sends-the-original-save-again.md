---
type: REQ
profile-version: 1
id: "REV-REQ-0084"
title: "A Library Retry sends the original save again"
---

# A Library Retry sends the original save again

## Statement

WHEN the reader activates Retry on a [Library save](0081-failed-library-saves-support-explicit-retry.md#statement), the
Library MUST send that save again exactly as it was first sent: the same submitted values, the same
[editing baseline](../../api/requirements/0082-graphql-edit-acknowledgements-carry-the-validator-of-their-values.md#statement)
validator and, for a metadata save, the same request ID. A request ID is the identifier the Library assigns to one
metadata save so that the server can recognise a repeat of it.

## Rationale

The reader decided on the failed save against the values they saw. A Retry that rebuilt the save from later reads could
send values the reader never chose, or a validator that authorises overwriting changes the reader never saw. Keeping the
request ID lets the server recognise the Retry as the same save rather than as a second one.

## Acceptance criteria

- A metadata Retry sends the same patch, baseline and request ID as the failed save, including when the Library list
  refreshes between the failure and the Retry. Checked by `Library Retry sends the retained edit again` in
  `frontend/tests/browser/LibraryPage.test.tsx`.
- A reading-state Retry sends the same patch and baseline as the failed save. Checked by
  `Library Retry sends the retained edit again` in `frontend/tests/browser/LibraryPage.test.tsx`.
- A corrected value is a new save, not a Retry: it carries a new request ID and the baseline of the edit it corrects.
  Checked by `Library Retry sends the retained edit again` in `frontend/tests/browser/LibraryPage.test.tsx`.
