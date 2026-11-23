---
type: REQ
profile-version: 1
id: "REV-REQ-0083"
title: "Editing baselines form only from complete values of one revision"
governed-by:
  - "REV-ADR-0084"
---

# Editing baselines form only from complete values of one revision

## Statement

WHEN the browser forms an editing baseline for book metadata or reading state, it MUST pair every value the edit
requires with the [validator](0082-graphql-edit-acknowledgements-carry-the-validator-of-their-values.md#statement) of
the same [revision](0082-graphql-edit-acknowledgements-carry-the-validator-of-their-values.md#statement). It MUST
exclude data from responses that carry execution errors. It MAY combine error-free fields that identify the same
immutable revision. It MUST form a baseline only after all required fields are available, including every page from that
revision for a whole-list replacement. An editing baseline is the set of values an edit starts from together with the
validator its save sends.

## Rationale

A baseline whose validator belongs to other values authorises an overwrite of changes the reader never saw. Adopting a
baseline while required values are missing, or saving a whole-list replacement built from missing pages or pages mixed
across revisions, silently deletes entries the reader never saw.

## Acceptance criteria

- Error-free responses that supply different fields of the same immutable revision can together form one complete
  baseline. Publishing a newer revision's validator cannot pair it with an earlier revision's values. Checked by
  `metadata and reading edits keep their baseline` in `frontend/tests/browser/api/fetch.test.tsx`.
- A missing validator or required value prevents an editing baseline until complete data from that revision is
  available. Library editing reads report missing required data through `@throwOnFieldError`. Checked by
  `metadata and reading edits keep their baseline` in `frontend/tests/browser/api/fetch.test.tsx`.
- Data from a response carrying execution errors cannot form or advance an editing baseline. Checked by
  `GraphQL network preserves failures, CSRF recovery and account isolation` in
  `frontend/tests/browser/api/graphql.test.tsx`.
- A complete multi-page contributor list collected from one revision forms a valid whole-list editing baseline. Checked
  by `metadata and reading edits keep their baseline` in `frontend/tests/browser/api/fetch.test.tsx`.
- A multi-page contributor list with a missing page or with pages from different revisions cannot form an editing
  baseline. Checked by `metadata and reading edits keep their baseline` in `frontend/tests/browser/api/fetch.test.tsx`.
