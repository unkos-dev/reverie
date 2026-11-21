---
type: REQ
profile-version: 1
id: "REV-REQ-0019"
title: "Browser edits retain the validator of their editing baseline"
---

# Browser edits retain the validator of their editing baseline

## Statement

WHEN the browser client sends a save of book metadata or reading state, the save MUST carry the
[validator](0082-graphql-edit-acknowledgements-carry-the-validator-of-their-values.md#statement) of the
[editing baseline](0082-graphql-edit-acknowledgements-carry-the-validator-of-their-values.md#statement) the edit started
from, whatever responses the client has received since, until a new editing decision or a successful matched reload or
reset replaces the baseline's values and validator together. A new editing decision is the reader opening an editor on
the values the client displays. A matched reload or reset is a read that the reader requests, through Reload or a reset
control, and that returns current values with the validator of their own revision. A reload or reset that fails, or that
returns values without their validator, replaces nothing.

## Rationale

A validator from another response can authorise an overwrite of values the reader never saw. Keeping the validator with
the edit prevents unrelated reads, refreshes of the displayed values and error responses from advancing a retained
draft's write precondition.

## Acceptance criteria

- Metadata and reading-state saves send the validator of their editing baseline, as HTTP `If-Match` or as the GraphQL
  baseline input. Checked by `metadata and reading edits keep their baseline` in
  `frontend/tests/browser/api/fetch.test.tsx`.
- A read of another resource, a refresh of the displayed values or a refusal of the save as stale does not change the
  validator that the edit's next save sends. Checked by `metadata and reading edits keep their baseline` in
  `frontend/tests/browser/api/fetch.test.tsx` and `a table edit keeps the validator its editor opened with` in
  `frontend/tests/browser/LibraryPage.test.tsx`.
- After a new editing decision, the next save sends the validator of the values that decision started from. Checked by
  `metadata and reading edits keep their baseline` in `frontend/tests/browser/api/fetch.test.tsx`.
- After a successful matched reload or reset, the edit shows the reloaded values and its next save sends their
  validator.
- After a reload or reset that fails, or that returns values without their validator, the edit keeps its values and its
  next save sends the validator it held before.
