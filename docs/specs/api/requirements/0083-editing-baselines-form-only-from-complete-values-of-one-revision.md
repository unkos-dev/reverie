---
type: REQ
profile-version: 1
id: "REV-REQ-0083"
title: "Editing baselines form only from complete values of one revision"
---

# Editing baselines form only from complete values of one revision

## Statement

WHEN the browser client starts an edit of book metadata or reading state, it MUST start it only from an
[editing baseline](0082-graphql-edit-acknowledgements-carry-the-validator-of-their-values.md#statement) whose values all
belong to the [revision](0082-graphql-edit-acknowledgements-carry-the-validator-of-their-values.md#statement) that its
validator identifies, with each list value holding every entry that revision has.

## Rationale

A baseline whose validator belongs to other values authorises an overwrite of changes the reader never saw. A list value
saved from a partial list deletes the entries the reader was never shown. Starting every edit from complete values of
one revision means a save changes only what the reader saw and chose to change.

## Acceptance criteria

- A response that carries an error, even beside data, or that carries no data, supplies no values to an edit. Checked by
  `GraphQL network preserves failures, CSRF recovery and account isolation` in
  `frontend/tests/browser/api/graphql.test.tsx`.
- A response whose data omits a field the edit selected supplies no values to that edit.
- After a read that returns only a newer revision's validator, the client holds no values for that revision, so no edit
  starts from the earlier values with the newer validator. Checked by `metadata and reading edits keep their baseline`
  in `frontend/tests/browser/api/fetch.test.tsx`.
- Every page of a list, all read at one revision, supplies a list edit's starting values. Checked by
  `metadata and reading edits keep their baseline` in `frontend/tests/browser/api/fetch.test.tsx`.
- A list with a missing page, or with pages read at different revisions, supplies no starting values to a list edit.
  Checked by `metadata and reading edits keep their baseline` in `frontend/tests/browser/api/fetch.test.tsx`.
