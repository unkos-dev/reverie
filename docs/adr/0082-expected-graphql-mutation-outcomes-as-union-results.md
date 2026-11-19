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

A Reverie mutation can end in an outcome the caller should expect and handle: a name that fails validation, a system
shelf that cannot change, a target that is missing or not the caller's. It can also fail unexpectedly: a lost
connection, an internal fault, a refused credential. The editing contract needs the two classes kept apart, because an
unexpected failure can offer Retry with the original input while an expected refusal must not offer an unchanged resend.

Execution errors with a classified `extensions.code` carried both classes. Relay's `commitMutation` passes response
errors to `onCompleted` beside the data, the browser network refuses any response with errors, and Relay's error guide
recommends modelling error information that the application displays as part of the schema. How should a mutation report
an expected outcome?

## Decision drivers

- Expected outcomes are typed in the schema and in generated client types, not recovered from string codes.
- The unexpected class stays recognisable, so Retry applies only to it.
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
that generated client types branch on, while every execution error remains the unexpected class.

Each mutation returns a union of one success member and the expected outcomes for that mutation. Shared outcome types,
such as `NotFound`, are reused across mutations. `forbidden`, `internal` and `malformed-header` remain execution errors.
A caller-invisible or malformed target is a `NotFound` member, matching how reads treat it.

### Consequences

- Positive: clients branch on `__typename` with type-checked members. A refusal never reaches `onError`, so anything in
  `onError` is a failure that may offer Retry.
- Positive: the success member returns the changed records and edges, so Relay publishes them through normalisation and
  the declarative connection directives.
- Positive: a refusal arrives as data, so the network's refusal of responses with errors no longer blocks it.
- Negative: each mutation adds a union and usually a success type to the schema.
- Negative: generated union types include a future-member case that every client must handle.
- Negative: mutations move to unions one workflow at a time, so error codes and union results coexist until the last
  mutation moves.

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
