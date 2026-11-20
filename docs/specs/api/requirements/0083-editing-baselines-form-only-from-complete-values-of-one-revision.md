---
type: REQ
profile-version: 1
id: "REV-REQ-0083"
title: "Editing baselines form only from complete values of one revision"
---

# Editing baselines form only from complete values of one revision

## Statement

WHEN the browser client adopts an editing baseline for book metadata or reading state, it MUST take the values and the
validator from one revision and from a complete response, so that values and a validator from different revisions, a
response that carries errors or lacks data, and a replacement list with a missing page or with pages from different
revisions never become one. An editing baseline is the set of values an edit starts from together with the validator its
save sends. A validator is the strong entity-tag that identifies one revision, a revision is the resource state a
validator identifies, and a null value is a value.

## Rationale

A baseline whose validator belongs to other values authorises an overwrite of changes the reader never saw. A
replacement list adopted with a missing page, or with pages from different revisions, deletes entries the reader never
saw when it is saved.

## Acceptance criteria

- Values from one metadata revision never pair with another revision's validator, and a validator published apart from
  its values cannot give earlier stored values a replacement baseline. Checked by
  `metadata and reading edits keep their baseline` in `frontend/tests/browser/api/fetch.test.tsx`.
- A response that carries errors or lacks data forms no baseline. Checked by
  `GraphQL network preserves failures, CSRF recovery and account isolation` in
  `frontend/tests/browser/api/graphql.test.tsx`.
- A complete selected list at one revision can supply a replacement-list baseline. A missing page or pages from
  different revisions cannot supply one. Checked by `metadata and reading edits keep their baseline` in
  `frontend/tests/browser/api/fetch.test.tsx`.
