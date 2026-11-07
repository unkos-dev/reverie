---
type: REQ
profile-version: 1
id: "REV-REQ-0019"
title: "Browser edits retain the validator belonging to their values"
---

# Browser edits retain the validator belonging to their values

## Statement

WHEN the browser client saves book metadata or reading state, it MUST send the `If-Match` validator belonging to the
validated values from which that edit began. It MUST retain that baseline through errors and CSRF retry until a new
editing decision adopts validated values and their matching validator together.

## Rationale

A validator from another response can authorise an overwrite of values the reader never saw. Keeping the validator with
the editing values prevents unrelated reads, background refresh and error responses from advancing a retained draft's
write precondition.

## Acceptance criteria

- Metadata and reading saves send the validator belonging to their editing values, including across a CSRF retry.
- A read of another resource, a background refresh or a 412 response does not replace a retained edit's validator.
- Missing or malformed values or validators do not publish a replacement editing baseline or authorise another write.
- A new editing decision adopts validated values and their matching validator together.

Checked by `metadata and reading edits keep their baseline` in `frontend/tests/browser/api/fetch.test.tsx`.
