---
type: REQ
profile-version: 1
id: "REV-REQ-0019"
title: "Browser edits retain the validator belonging to their values"
---

# Browser edits retain the validator belonging to their values

## Statement

WHEN the browser client saves book metadata or reading state, it MUST send the
[validator](0082-graphql-edit-acknowledgements-carry-the-validator-of-their-values.md#statement) of the
[revision](0082-graphql-edit-acknowledgements-carry-the-validator-of-their-values.md#statement) its edited values came
from, and MUST keep that validator across transport retries, errors, later reads of the same or another resource, and
refused writes until the reader starts a new edit from refreshed values on screen or triggers an explicit reload.

## Rationale

A validator from a newer or unrelated response can authorise an overwrite of values the reader never saw. Keeping the
starting validator paired with an active edit ensures that concurrent reads, transient retries and refused writes never
silently advance a draft's write precondition.

## Acceptance criteria

- Metadata and reading saves send the validator belonging to their edited values as HTTP `If-Match` or a GraphQL
  baseline, keeping the same validator across a transparent transport retry. Checked by
  `metadata and reading edits keep their baseline` in `frontend/tests/browser/api/fetch.test.tsx`.
- A read of another resource, a newer refresh of the same resource, or a stale-baseline refusal does not replace an open
  edit's starting validator. Checked by `metadata and reading edits keep their baseline` in
  `frontend/tests/browser/api/fetch.test.tsx` and `a table edit keeps the validator its editor opened with` in
  `frontend/tests/browser/LibraryPage.test.tsx`.
- Starting a new edit from refreshed values, or activating an explicit reload, replaces the values and their matching
  validator together. Checked by `metadata and reading edits keep their baseline` in
  `frontend/tests/browser/api/fetch.test.tsx` and `a stale table save refreshes before the next edit` in
  `frontend/tests/browser/LibraryPage.test.tsx`. The Book dialog Reload is checked by
  `retains refused input and reloads the Book dialog onto the current revision` in
  `frontend/tests/browser/EditMetadataDialog.test.tsx`.
