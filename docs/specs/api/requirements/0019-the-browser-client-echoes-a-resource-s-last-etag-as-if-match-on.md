---
type: REQ
profile-version: 1
id: "REV-REQ-0019"
title: "Browser edits retain the validator belonging to their values"
---

# Browser edits retain the validator belonging to their values

## Statement

WHEN the browser client saves book metadata or reading state, it MUST send the validator of the revision its edited
values came from, and MUST keep that validator through errors, CSRF retry, background reads, refreshes and refused
writes until a new editing decision replaces the values and their validator together. A validator is the strong
entity-tag that identifies one revision, a revision is the resource state a validator identifies, and a new editing
decision is the reader starting an edit from the values on screen or an explicit matched reload or reset.

## Rationale

A validator from another response can authorise an overwrite of values the reader never saw. Keeping the validator with
the edited values prevents unrelated reads, background refresh and error responses from advancing a retained draft's
write precondition.

## Acceptance criteria

- Metadata and reading saves send the validator belonging to their edited values as HTTP `If-Match` or an explicit
  GraphQL baseline, including across a CSRF retry. Checked by `metadata and reading edits keep their baseline` in
  `frontend/tests/browser/api/fetch.test.tsx`.
- A read of another resource, a background refresh or a stale-baseline refusal does not replace a retained edit's
  validator. Checked by `metadata and reading edits keep their baseline` in `frontend/tests/browser/api/fetch.test.tsx`
  and `a table edit keeps the validator its editor opened with` in `frontend/tests/browser/LibraryPage.test.tsx`.
- A new editing decision replaces the values and their matching validator together. Checked by
  `metadata and reading edits keep their baseline` in `frontend/tests/browser/api/fetch.test.tsx`.
