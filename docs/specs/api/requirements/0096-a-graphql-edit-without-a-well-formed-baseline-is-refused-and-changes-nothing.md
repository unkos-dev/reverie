---
type: REQ
profile-version: 1
id: "REV-REQ-0096"
title: "A GraphQL edit without a well-formed baseline is refused and changes nothing"
---

# A GraphQL edit without a well-formed baseline is refused and changes nothing

## Statement

WHEN a GraphQL `updateMetadata` or `updateReadingState` mutation omits its baseline, supplies `null` for it, or supplies
a value that is not one strong entity-tag, the server MUST refuse the mutation with an error and leave stored state
unchanged. The baseline is the
[validator](0082-graphql-edit-acknowledgements-carry-the-validator-of-their-values.md#statement) of the values the
caller's edit was made from. A strong entity-tag is a single `entity-tag` as
[RFC 9110 §8.8.3](https://www.rfc-editor.org/rfc/rfc9110#section-8.8.3) defines it, without the `W/` weak indicator, so
the `*` wildcard, a weak tag and an unquoted value are not strong entity-tags.

## Rationale

An edit without a baseline gives the server nothing to compare with the target's current state, so the server cannot
tell a fresh edit from one that would overwrite a change the caller never saw. Accepting it would reopen the lost update
that [RFC 9110 §13.1.1](https://www.rfc-editor.org/rfc/rfc9110#section-13.1.1) preconditions exist to prevent. A weak
tag or the wildcard matches more than one revision, so it cannot stand in for the baseline either.

## Acceptance criteria

- An `updateMetadata` whose baseline is `null`, omitted or unquoted produces an error, and a read afterwards returns the
  values and validator from before the attempts. Checked by `graphql_edit_baselines` in `backend/src/graphql/mod.rs`.
- An `updateReadingState` whose baseline is `null`, omitted or weak produces an error, and a save made afterwards with
  the baseline read before the attempts succeeds, which shows the attempts changed nothing. Checked by
  `graphql_edit_baselines`.
