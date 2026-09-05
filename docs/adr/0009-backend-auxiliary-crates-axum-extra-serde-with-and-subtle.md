---
type: ADR
profile-version: 1
id: "REV-ADR-0009"
title: "Backend auxiliary crates: axum-extra, serde_with, and subtle"
status: "accepted"
recorded-on: "2026-09-04"
decided-on: "2026-05-22"
decision-makers:
  - "John Unkovich"
---

# Backend auxiliary crates: axum-extra, serde_with, and subtle

## Context and problem statement

The JSON REST API conventions work needed three pieces of capability the backend did not yet have: query-string
extraction that decodes a repeated key (`?tag=a&tag=b`) into a `Vec<String>` for filter parameters, a way to decode an
RFC 7396 JSON Merge Patch body so a `PATCH` handler can tell a field left out of the body apart from one set to
`null`, and a constant-time comparison for a presented secret against a stored one so a CSRF token check does not leak
timing information. Which crates should serve those three needs?

## Decision drivers

- The built-in `axum::Query` extractor decodes repeated query keys into only the last value, not a `Vec`, so filter
  parameters need a different extractor.
- Bare serde's `Option<Option<T>>` collapses "field absent" and "field present as `null`" into the same `None` value,
  which breaks Merge Patch semantics.
- A timing difference in a secret-vs-presented comparison lets an attacker who can issue many requests narrow the
  valid-token search space byte by byte; Reverie's threat model is a multi-user, network-exposed instance.
- Preferring an existing dependency's feature flag over a new crate, and an audited crate over a hand-rolled
  primitive, keeps the dependency surface and the maintenance burden small.

## Considered options

- `axum-extra` query feature, `serde_with`, and `subtle`
- `serde_qs` as a new top-level dependency instead of the `axum-extra` `"query"` feature
- Hand-rolled `Option<Option<T>>` Merge Patch decode
- Replace `subtle` with a hand-rolled constant-time compare
- A third-party query-string extractor crate

## Decision outcome

Chosen option: **`axum-extra` query feature, `serde_with`, and `subtle`**, because each is the smallest addition that
meets one of the three needs, and two of the three were already dependencies before this decision.

`axum-extra`'s existing entry in `backend/Cargo.toml` gains the `"query"` feature, alongside the `"cookie"` feature it
already carried, at the version already pinned to match the `axum` line in use. `axum_extra::extract::Query` decodes
a repeated query key into a `Vec`, which the built-in `axum::Query` (backed by `serde_urlencoded`) does not.
Because `axum-extra` is already compiled into the tree, enabling the feature costs no new dependency, unlike adding
`serde_qs` as a sibling top-level crate for substantially the same surface.

`serde_with` becomes a new top-level dependency, with its default features disabled and only `"std"` enabled: the
helper this decision needs, `serde_with::rust::double_option`, does not require the `"macros"` feature or the
proc-macro crate it pulls in. A `PATCH` handler that implements Merge Patch semantics annotates each optional field
with `#[serde(default, with = "::serde_with::rust::double_option")]`, which distinguishes the three states a Merge
Patch field can take: absent from the body decodes to `None` and leaves the field unchanged; present as `null`
decodes to `Some(None)` and clears the field; present with a value decodes to `Some(Some(value))` and sets it. Bare
`Option<Option<T>>` cannot make this distinction under serde's default decoding, and `serde_with::rust::double_option`
is the established serde-ecosystem helper for it, so a per-field hand-rolled `Visitor` is not worth writing.

`subtle` was already in the tree as the constant-time comparison used to verify a device token's hash; this decision
documents its second, non-test consumer: a CSRF check compares the incoming header token to the session-stored token
via `subtle::ConstantTimeEq`, so the comparison takes the same wall-clock time regardless of where the two byte
slices first differ. There is no constant-time compare in the standard library; the textbook hand-rolled equivalent,
folding an XOR-and-OR across zipped byte slices, is exactly what `subtle::ConstantTimeEq` already provides, with the
addition of an optimisation barrier that stops LLVM from rewriting the fold into an early-exit comparison. Reverie
applies this defence to every comparison of a presented secret against a stored one; the device-token verifier is the
existing precedent.

### Consequences

- Positive: the `axum-extra` feature addition is the minimum diff that supports repeated-key filter parameters, with
  no new dependency and no version churn.
- Positive: `serde_with` is the canonical solution to the Merge Patch decode problem, is widely used in the serde
  ecosystem, is clean under `cargo audit`, and its `double_option` helper has been stable across major versions.
- Positive: documenting `subtle`'s second consumer here keeps the constant-time-compare convention discoverable
  alongside its first use, rather than buried in a handler docstring.
- Negative: `serde_with` is a large crate carrying many helpers this decision does not use; the compile-time and
  binary-size cost is accepted in exchange for not hand-rolling a Merge Patch decoder. Enabling further `serde_with`
  features, such as `"macros"`, needs its own justification.
- Negative: the `axum-extra` `"query"` feature pulls in `serde_qs` transitively, growing the lockfile by one
  indirect dependency.

## Pros and cons of the options

### `axum-extra` query feature, `serde_with`, and `subtle`

- Positive: two of the three additions are feature flags or an existing dependency, not new crates.
- Positive: each crate is narrowly scoped to the need it serves and is already established in the serde or Rust
  cryptography ecosystem.
- Negative: `serde_with`'s default feature set is broader than what this decision uses, so the dependency must be
  configured explicitly to avoid it.

### `serde_qs` as a new top-level dependency

- Negative: `axum-extra` is already in tree; enabling its `"query"` feature is the smaller diff, and adding
  `serde_qs` as a sibling top-level dependency would cover substantially the same surface at the cost of a new
  crate.

### Hand-rolled `Option<Option<T>>` Merge Patch decode

- Negative: `serde_with::rust::double_option` already is this, written, tested, and audited, and reused across the
  serde ecosystem; hand-rolling a `Visitor` per field is not a decode primitive Reverie should own.

### Replace `subtle` with a hand-rolled constant-time compare

- Negative: `subtle` is already in tree, and reinventing its optimisation-barrier protection opens the comparison
  to the same class of compiler-rewrite bug the crate explicitly defends against.

### A third-party query-string extractor crate

- Negative: rejected on the same "already in tree" grounds as `serde_qs`; the `axum-extra` `"query"` feature is the
  path of least resistance.

## More information

Standing principle: default to standard-library and audited-crate idioms over hand-rolled primitives.

Sibling ADR: [JSON API conventions](./0011-json-api-conventions-for-the-browser-facing-rest-surface.md) (the RFC 7396 Merge Patch decision
and the CSRF synchroniser-token decision).

Sibling ADR: [Frontend data layer dependencies](./0010-frontend-data-layer-dependencies-react-query-and-dnd-kit.md) (the frontend
dependency adoptions for the same API conventions work).

Dependency declarations: [`backend/Cargo.toml`](../../backend/Cargo.toml).
