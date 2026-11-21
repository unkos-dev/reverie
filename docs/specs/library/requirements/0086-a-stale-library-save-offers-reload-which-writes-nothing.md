---
type: REQ
profile-version: 1
id: "REV-REQ-0086"
title: "A stale Library save offers Reload, which writes nothing"
---

# A stale Library save offers Reload, which writes nothing

## Statement

WHEN the server refuses a [Library save](0081-failed-library-saves-support-explicit-retry.md#statement) because its
[editing baseline](../../api/requirements/0082-graphql-edit-acknowledgements-carry-the-validator-of-their-values.md#statement)
is stale, the Library MUST keep the reader's submitted input visible and offer, in place of Retry, a Reload that reads
the current revision and sends no write. A baseline is stale when its validator no longer identifies the current
revision.

## Rationale

A stale refusal means another change reached the book after the reader's edit began. The same save sent again would be
refused again, and saving over the newer state without showing it would discard a change the reader never saw. Reading
the current revision lets the reader decide again against what is now stored.

## Acceptance criteria

- A metadata save refused as stale offers Reload and no Retry control. Checked by
  `Library Retry sends the retained edit again` in `frontend/tests/browser/LibraryPage.test.tsx`.
- A reading-state save refused as stale offers Reload. Checked by `Library Retry sends the retained edit again` in
  `frontend/tests/browser/LibraryPage.test.tsx`.
- Activating Reload sends no write, and the Edit outcomes region keeps the refused submitted value visible. Checked by
  `Library Retry sends the retained edit again` in `frontend/tests/browser/LibraryPage.test.tsx`.
- An edit started after Reload sends the validator of the revision Reload read.
