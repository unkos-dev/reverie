---
type: ADR
profile-version: 1
id: "REV-ADR-0078"
title: "Use Juniper as the GraphQL server library"
status: "proposed"
recorded-on: "2026-11-18"
decision-makers:
  - "John Unkovich"
---

# Use Juniper as the GraphQL server library

## Context and problem statement

Reverie serves browser application data from a code-first GraphQL schema written in Rust. The schema was built on
async-graphql 7.2.1. That project's last merged pull request landed on 2026-04-12 and its last release shipped on
2026-04-21. It has 31 open pull requests, including a fix for a parser stack overflow on deeply nested input values.

The server library parses and validates documents, executes their fields and exports the schema that the Relay compiler
reads. Which Rust library should own that work for Reverie's GraphQL API?

## Decision drivers

- An upstream that still ships releases, so parser and execution defects get fixed outside Reverie.
- A code-first Rust schema whose exported SDL stays the contract for the Relay compiler and trusted documents.
- Reverie keeps its own HTTP envelope, trusted-document admission, root-field scope checks, cost and depth limits, and
  caller-scoped batched reads.
- The amount of code and the number of dependencies Reverie must maintain itself.
- Support for September 2025 specification features such as `@oneOf` and `@specifiedBy`.

## Considered options

- Stay on async-graphql 7.2.1
- Fork async-graphql
- Migrate to Juniper 0.18

## Decision outcome

Chosen option: **Migrate to Juniper 0.18**, because it is the maintained Rust GraphQL server that keeps the schema
contract, and Reverie then owns only the few layers Juniper lacks instead of an entire server library.

### Consequences

- Positive: parser, validation and execution fixes come from an upstream that released 0.18.0 on 2026-10-30.
- Positive: Juniper's parser bounds document depth, token count and field count before validation runs.
- Negative: Reverie owns its query cost and depth analysis, per-request batching, Relay connection types and the `JSON`
  scalar, which async-graphql provided.
- Negative: Juniper cannot attach applied directives to object types. Relay paging fragments refetch through `node(id:)`
  rather than concrete `fetch__*` roots.
- Negative: Juniper's look-ahead resolves a response key repeated directly in one selection set to the first
  occurrence's children. Fields that read look-ahead depend on documents not repeating such keys.
- Negative: Juniper 0.18 refuses a variable's input object when a non-null field with a default value is absent. Reverie
  fills those defaults before execution until upstream validation applies them.
- Negative: recent Juniper activity rests mainly on one maintainer, and a breaking major release is pending.
- Neutral: neither library implements `onError`, so the browser keeps refusing responses that carry errors.

## Pros and cons of the options

### Stay on async-graphql 7.2.1

- Positive: no code changes.
- Negative: known defects, including the parser stack overflow, stay unfixed unless a dormant upstream resumes.
- Negative: future specification work such as `onError` depends on the same dormant upstream.

### Fork async-graphql

- Positive: the schema code, built-in cost analysis and batching stay unchanged.
- Negative: Reverie would own about 48,000 lines across the core, derive, parser, value and Axum crates, plus 49 core
  dependencies. More than a quarter of recent upstream commits were dependency and toolchain maintenance.

### Migrate to Juniper 0.18

- Positive: maintained upstream with the September 2025 specification features.
- Positive: the port touches only Reverie's GraphQL module, about 3,000 production lines.
- Negative: Reverie writes and maintains the layers listed under Consequences.

## More information

REV-ADR-0077 chose GraphQL and Relay for browser application data and named async-graphql as the library for its
code-first schema. That transport and client decision is unchanged. Revisit this decision if Juniper stops releasing, or
if a release adds applied directives on object types, input-field defaults in variable validation or `onError`.
