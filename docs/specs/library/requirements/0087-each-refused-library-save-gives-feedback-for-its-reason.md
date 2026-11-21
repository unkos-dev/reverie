---
type: REQ
profile-version: 1
id: "REV-REQ-0087"
title: "Each refused Library save gives feedback for its reason"
---

# Each refused Library save gives feedback for its reason

## Statement

WHEN the server refuses a [Library save](0081-failed-library-saves-support-explicit-retry.md#statement) for a reason
other than a stale baseline, the Library MUST show feedback for that reason and MUST NOT offer to send the save again
unchanged. A refusal for validation keeps the reader's submitted input available in the editor for correction. A refusal
because a field is locked, or because the reader lacks permission, names that reason.

## Rationale

Each refusal needs a different response from the reader: a validation refusal needs a corrected value, a locked field
needs unlocking, and a permission refusal cannot be resolved from the Library. Generic feedback, or an offer to send the
same save again, gives the reader a control that cannot succeed. Keeping the input after a validation refusal lets the
reader correct it rather than enter it again.

## Acceptance criteria

- After a refusal for validation, the Edit outcomes region shows the submitted value, and reopening the cell's editor
  shows the submitted input. Checked by `Library Retry sends the retained edit again` in
  `frontend/tests/browser/LibraryPage.test.tsx`.
- After a refusal because a field is locked, the Edit outcomes region names the locked-field refusal and offers neither
  Retry nor Reload. Checked by `Library Retry sends the retained edit again` in
  `frontend/tests/browser/LibraryPage.test.tsx`.
- After a refusal because the reader lacks permission, the Edit outcomes region names the permission refusal and offers
  neither Retry nor Reload.
