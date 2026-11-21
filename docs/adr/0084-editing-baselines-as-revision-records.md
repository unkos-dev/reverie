---
type: ADR
profile-version: 1
id: "REV-ADR-0084"
title: "Editing baselines as revision records"
status: "accepted"
recorded-on: "2026-11-21"
decided-on: "2026-11-17"
decision-makers:
  - "John Unkovich"
---

# Editing baselines as revision records

## Context and problem statement

An edit of book metadata starts from values and the validator of the revision those values belong to, and its save sends
that validator as its precondition. In the browser, Relay normalises every response into one store. An object with an
`id` becomes its own record. An object without one is stored under its parent record and field, so every selection of
that field, however sparse, writes into the same record. A read that selects only the validator then places a newer
validator beside older values, and an edit started from that record could overwrite changes the reader never saw.

How should the GraphQL schema carry editing values and their validator, so that the client store can never pair one
revision's values with another revision's validator?

## Decision drivers

- Values and the validator of one revision stay together in the client store, whatever other selections write.
- The guarantee holds for every selection, without a rule that the Relay compiler cannot check.
- Library rows and editors select only the fields they show.
- Live work and manifestation records stay the display source, so an edit reaches every view of the book.
- The mechanism is Relay's documented normalisation, not client identity code.

## Considered options

- Revision records with their own `id`
- A validator field on the live metadata record, which has no `id`
- One complete fragment for every selection of editing values
- A client `getDataID` keyed on the manifestation and the validator

## Decision outcome

Chosen option: **Revision records with their own `id`**, because Relay normalises each revision into a record of its
own, so a new revision is a new record and no selection, however sparse, can pair a new validator with older values.

### Consequences

- Positive: a store merge cannot mix revisions. A sparse read of a new revision reads as missing data, never as a
  mismatched pair.
- Positive: Library rows and editors select only the fields they need.
- Negative: the schema carries revision types and their relation entry types beside the live records.
- Negative: past revisions have no `Node` lookup, so a client cannot refetch an earlier revision by its ID.
- Negative: relation pages are reached through the current revision, so a list whose pages span a revision change forms
  no baseline.
- Negative: an optimistic updater must never write a revision record, because an optimistic value beside an unchanged
  validator recreates the mixed pair.

## Pros and cons of the options

### A validator field on the live metadata record, which has no `id`

- Positive: no new schema types, and the validator sits beside the values it describes.
- Negative: every selection of the field writes into one store record, so a sparse read pairs a newer validator with
  older values.

### One complete fragment for every selection of editing values

- Positive: no schema change.
- Negative: the Relay compiler cannot enforce that every selection uses the complete fragment, so one partial selection
  brings back the mixed pair.
- Negative: Library rows would carry every editable field.

### A client `getDataID` keyed on the manifestation and the validator

- Positive: no schema change, and record identity stays in one client function.
- Negative: the Relay compiler cannot add the manifestation ID and validator that such a `getDataID` needs to each
  selection, so a selection without them merges into the shared record without any error.
