---
type: REQ
profile-version: 1
id: "REV-REQ-0083"
title: "Editing baselines form only from complete values of one revision"
---

# Editing baselines form only from complete values of one revision

## Statement

WHEN the browser client forms an editing baseline for book metadata or reading state, it MUST pair values only with the
[validator](0082-graphql-edit-acknowledgements-carry-the-validator-of-their-values.md#statement) of the same
[revision](0082-graphql-edit-acknowledgements-carry-the-validator-of-their-values.md#statement) from a complete,
error-free response, and for a whole-list replacement edit it MUST include every page of that list from that single
revision. An editing baseline is the set of values an edit starts from together with the validator its save sends.

## Rationale

A baseline whose validator belongs to other values authorises an overwrite of changes the reader never saw. Adopting a
baseline from a failed or incomplete response, or saving a whole-list replacement built from missing pages or pages
mixed across revisions, silently deletes entries the reader never saw.

## Acceptance criteria

- A response carrying values from one revision and a validator from another revision, or a validator delivered without
  its matching values, cannot form an editing baseline for a subsequent save. Checked by
  `metadata and reading edits keep their baseline` in `frontend/tests/browser/api/fetch.test.tsx`.
- A response that carries execution errors or omits required data cannot form an editing baseline. Checked by
  `GraphQL network preserves failures, CSRF recovery and account isolation` in
  `frontend/tests/browser/api/graphql.test.tsx`.
- A complete multi-page contributor list collected from one revision forms a valid whole-list editing baseline. Checked
  by `metadata and reading edits keep their baseline` in `frontend/tests/browser/api/fetch.test.tsx`.
- A multi-page contributor list with a missing page or with pages from different revisions cannot form an editing
  baseline. Checked by `metadata and reading edits keep their baseline` in `frontend/tests/browser/api/fetch.test.tsx`.
