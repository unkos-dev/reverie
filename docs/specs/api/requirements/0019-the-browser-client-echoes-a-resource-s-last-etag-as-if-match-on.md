---
type: REQ
profile-version: 1
id: "REV-REQ-0019"
title: "Browser edits retain the validator belonging to their values"
---

# Browser edits retain the validator belonging to their values

## Statement

WHEN the browser client saves book metadata or reading state, it MUST send the validator of the revision from which that
edit's validated values came. A revision is the resource state a validator identifies. Values and a validator from
different revisions MUST NOT form an editing baseline. Background reads, refreshes and failed writes MUST NOT advance a
retained edit's baseline. The client MUST retain that baseline through errors and CSRF retry until a new editing
decision adopts validated values and their matching validator together. A replacement-list edit MUST adopt all pages of
that selected field at one revision; it MUST NOT adopt an incomplete list or pages from different revisions.

## Rationale

A validator from another response can authorise an overwrite of values the reader never saw. Keeping the validator with
the editing values prevents unrelated reads, background refresh and error responses from advancing a retained draft's
write precondition.

## Acceptance criteria

- Metadata and reading saves send the validator belonging to their editing values as HTTP `If-Match` or an explicit
  GraphQL baseline argument, including across a CSRF retry.
- A read of another resource, a background refresh or a 412 response does not replace a retained edit's validator.
- Missing or malformed values or validators do not publish a replacement editing baseline or authorise another write.
- A new editing decision adopts validated values and their matching validator together.
- GraphQL edits retain their selected values and explicit baseline through background record merges, CSRF resend and
  precondition refusal. A later ETag-only publication cannot give an earlier stored value a replacement baseline.
- A complete selected list at one revision can supply a replacement-list baseline. Missing pages or different page
  revisions cannot supply one. Unrelated metadata fields are not required to validate that selection.

Checked by `metadata and reading edits keep their baseline` in `frontend/tests/browser/api/fetch.test.tsx`.
