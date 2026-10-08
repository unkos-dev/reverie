---
type: ADR
profile-version: 1
id: "REV-ADR-0054"
title: "Non-serialising credential model redaction"
status: "accepted"
recorded-on: "2026-10-09"
decided-on: "2026-10-08"
decision-makers:
  - "John Unkovich"
---

# Non-serialising credential model redaction

## Context and problem statement

`LocalCredential` and `PasswordResetPin` contain password or recovery-PIN hashes that Debug and tracing must conceal.
Both models need ordinary String fields, Clone and SQLx row decoding, while deliberately exposing no serialisation
implementation. Their formatting policy needs one declaration of each sensitive field without losing useful non-secret
fields in diagnostics.

## Decision drivers

- Conceal only `password_hash` and `pin_hash`, including alternate Debug and tracing of clones.
- Preserve every other field in diagnostics and keep the existing database representation.
- Add no `Serialize` implementation or field-type change.
- Use a maintained derive compatible with the pinned Rust compiler.

## Considered options

- Retain handwritten Debug implementations.
- Use `redactable` SensitiveDisplay with explicit field annotations.
- Build a local redaction derive.

## Decision outcome

Chosen option: **use `redactable` SensitiveDisplay with explicit field annotations**, because the actual models prove
that the redaction-only feature set produces safe Debug without adding serialisation or changing SQLx decoding.

The dependency is pinned to 0.14.0 with default features disabled and only `redaction` enabled. Each model's template
names all its fields; the hash alone uses `#[sensitive(redactable::Secret)]`, and every other field uses
`#[not_sensitive]`. String fields, Clone and FromRow remain. Neither model derives `Serialize`. The decision applies
only to these two credential models; it does not establish PII masking or a project-wide logging convention.

Handwritten formatting preserves complete control and adds no dependency, but duplicates the redaction mechanism. A
local derive could centralise that mechanism, at the cost of owning macro parsing, expansion and compatibility.

### Consequences

- Positive: each sensitive field declares its formatting policy beside its database field.
- Positive: existing model types and row decoding remain compatible with their consumers.
- Negative: the dependency adds a procedural macro and transitive packages to the build.
- Negative: new fields require explicit annotations and a template update; formatting and tracing tests must retain
  coverage of every non-secret field.
