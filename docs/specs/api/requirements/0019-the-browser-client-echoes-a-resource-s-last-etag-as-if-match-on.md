---
type: REQ
profile-version: 1
id: "REV-REQ-0019"
title: "Browser edits retain the validator belonging to their values"
---

# Browser edits retain the validator belonging to their values

## Statement

WHEN the browser client saves book metadata or reading state, it MUST send the validator of the revision from which that
edit's values came. A revision is the resource state a validator identifies. Values and a validator from different
revisions MUST NOT form an editing baseline, including a validator published after and apart from the values. A selected
value absent from a response MUST NOT form an editing baseline, while a null value remains a value. A replacement-list
edit MUST adopt every selected page of that field from one revision. Background reads, refreshes and failed writes MUST
NOT advance a retained edit's baseline, and the client MUST retain that baseline through errors and CSRF retry until a
new editing decision adopts values and their matching validator together.

## Rationale

A validator from another response can authorise an overwrite of values the reader never saw. Keeping the validator with
the editing values prevents unrelated reads, background refresh and error responses from advancing a retained draft's
write precondition. A replacement list adopted with a missing page, or with pages from different revisions, would delete
entries the reader never saw when it is saved.

## Acceptance criteria

- Metadata and reading saves send the validator belonging to their editing values as HTTP `If-Match` or an explicit
  GraphQL baseline, including across a CSRF retry. Checked by `metadata and reading edits keep their baseline` in
  `frontend/tests/browser/api/fetch.test.tsx`.
- A read of another resource, a background refresh or a stale-baseline refusal does not replace a retained edit's
  validator. Checked by `metadata and reading edits keep their baseline` in `frontend/tests/browser/api/fetch.test.tsx`
  and `a table edit keeps the validator its editor opened with` in `frontend/tests/browser/LibraryPage.test.tsx`.
- A validator published apart from its values cannot give earlier stored values a replacement baseline, and values from
  one metadata revision never pair with another revision's validator. Checked by
  `metadata and reading edits keep their baseline` in `frontend/tests/browser/api/fetch.test.tsx`.
- A response without its selected values forms no baseline. Checked by
  `GraphQL network preserves failures, CSRF recovery and account isolation` in
  `frontend/tests/browser/api/graphql.test.tsx`.
- A new editing decision adopts values and their matching validator together. Checked by
  `metadata and reading edits keep their baseline` in `frontend/tests/browser/api/fetch.test.tsx`.
- A complete selected list at one revision can supply a replacement-list baseline. Missing pages or pages from different
  revisions cannot supply one. Checked by `metadata and reading edits keep their baseline` in
  `frontend/tests/browser/api/fetch.test.tsx`.
