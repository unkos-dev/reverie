---
type: ADR
profile-version: 1
id: "REV-ADR-0082"
title: "Expected GraphQL mutation outcomes as union results"
status: "accepted"
recorded-on: "2026-11-19"
decided-on: "2026-11-17"
decision-makers:
  - "John Unkovich"
---

# Expected GraphQL mutation outcomes as union results

## Context and problem statement

A Reverie mutation can end in an expected domain outcome such as a validation refusal, an immutable system shelf or a
missing target. It can also fail unexpectedly because of a transport fault, an internal error or a refused credential.
Callers need expected domain outcomes and unexpected execution failures kept distinct so each class is handled without
ambiguity.

Execution errors with a classified `extensions.code` previously carried both classes. Relay's `commitMutation` passes
response errors to `onCompleted` beside the data, the browser network refuses any response with errors, and Relay's
error guide recommends modelling expected error information as part of the schema. How should a mutation report an
expected outcome?

## Decision drivers

- Expected outcomes are typed in the schema and in generated client types, not recovered from string codes.
- Expected domain refusals and unexpected execution failures stay clearly distinguishable.
- An outcome carries the data its recovery needs, such as a message or the current state of the target.
- A success result carries the changed records, so the store updates without a refetch.
- The convention follows published GraphQL and Relay guidance rather than a Reverie-specific protocol.

## Considered options

- Union results
- Classified error extensions
- A success payload with a list of user errors
- One generic refusal type

## Decision outcome

Chosen option: **Union results**, because it is the only option in which each expected outcome is a distinct schema type
that generated client types branch on, while every execution error remains in the unexpected class.

### Consequences

- Positive: clients branch on `__typename` with type-checked members instead of inspecting string error codes.
- Positive: the success member returns changed records and edges, so Relay publishes updates through normalisation and
  declarative connection directives.
- Positive: an expected refusal arrives as normal data, so whole-response execution-error rejection does not discard its
  recovery fields.
- Negative: each mutation adds a union and usually a dedicated success type to the schema.
- Negative: generated union types include a future-member case that every client must handle.

## Pros and cons of the options

### Classified error extensions

- Positive: no extra schema types, and it is how the mutations reported refusals before this decision.
- Negative: expected and unexpected outcomes share one channel, so every client classifies string codes to decide on
  Retry.
- Negative: an error makes the network refuse the whole response, so a refusal cannot also carry data.

### A success payload with a list of user errors

- Positive: a common pattern that keeps one payload type per mutation.
- Negative: success fields become nullable, and nothing in the type ties a user error to the absence of a result.
- Negative: error codes stay strings inside the list.

### One generic refusal type

- Positive: fewer schema types: `Success | MutationRefused { code, message }`.
- Negative: clients branch on a string code again, and a refusal cannot carry outcome-specific recovery data.

## More information

- Relay's error states guide: "If you wish to access error information in your application to display user friendly
  messages, the recommended approach is to model and expose the error information as part of your GraphQL schema." The
  guide ships with `relay-runtime` at `llm-docs/guided-tour/rendering/error-states.mdx`.
