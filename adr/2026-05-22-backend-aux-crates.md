---
status: accepted
date: 2026-05-22
supersedes: []
decision-makers: "John Unkovich"
consulted: "—"
informed: "Reverie contributors"
---

# Backend auxiliary crates for Step 11

## Context and Problem Statement

Step 11 of the Reverie blueprint (the API conventions work) adds a JSON REST API
surface (see sibling
[`2026-05-22-json-api-conventions.md`](2026-05-22-json-api-conventions.md))
across six sub-phases (11a–11f). The implementation requires
two changes to `backend/Cargo.toml`:

1. A feature-flag addition to the existing `axum-extra` entry
   to enable repeated-key query parameter decoding (e.g.
   `?tag=a&tag=b` → `Vec<String>`), which Sub-phase 11b's filter
   parameters need.
2. A new top-level dependency on `serde_with` for RFC 7396 JSON
   Merge Patch sparse-update plumbing, which Sub-phase 11c's
   `PATCH /api/books/{id}/metadata` endpoint needs.

A third crate, `subtle`, is already in tree at version `2.6.1`
([`backend/Cargo.toml`](../backend/Cargo.toml)) but Step 11
introduces the first non-test consumer: Sub-phase 11a Task 1c
imports `subtle::ConstantTimeEq` to compare the CSRF token in
constant time. No `Cargo.toml` change is needed for `subtle`;
this ADR documents the new consumer pattern only.

CLAUDE.md's proactive trigger requires this ADR ("write ADR
before new crate"). The three crates are bundled because they
share an adoption shape (small, well-maintained, narrow purpose,
chosen for a specific spec-compliance requirement) and shipping
three micro-ADRs would impose ceremony disproportionate to the
decision.

The frontend-side dep adoptions sit in
[`2026-05-22-frontend-data-layer-deps.md`](2026-05-22-frontend-data-layer-deps.md);
together the two ADRs cover the full Step 11 dependency surface.

## Decision

### `axum-extra`: add `"query"` to the existing features list

Existing entry in `backend/Cargo.toml`:

```toml
axum-extra = { version = "0.12.6", features = ["cookie"] }
```

After this PR:

```toml
axum-extra = { version = "0.12.6", features = ["cookie", "query"] }
```

The version stays pinned at `0.12.6`, this version was selected
to match axum 0.8.9's compatibility requirements; downgrading
breaks the build. EDIT the existing line; do NOT add a duplicate
`axum-extra` entry. Sub-phase 11a Task 4 (committed in this
PR's stack) is the first consumer; it imports
`axum_extra::extract::Query` in the new
`backend/src/routes/library.rs` for sort-aware cursor pagination
parameters (and forward-compatibility with 11b's repeated-key
filter params).

Why this feature flag and not the built-in `axum::Query`: the
built-in extractor uses `serde_urlencoded`, which does not
deserialise repeated keys into `Vec`. 11b's `?tag=a&tag=b`
parameter needs `serde_qs`-style decoding; `axum_extra::extract::Query`
provides this. Adopting `axum-extra` in 11a (instead of 11b)
avoids a refactor of the `ListParams` extractor when filter
params land.

Why `axum-extra` over `serde_qs`: `axum-extra` is already in
tree; adding a feature flag costs zero dependencies (the crate
is already compiling). `serde_qs` would be a net-new dep with
substantially the same surface.

### `serde_with`: new top-level dependency (Sub-phase 11c)

Add to `backend/Cargo.toml`:

```toml
serde_with = "3"
```

First consumer is Sub-phase 11c's
`backend/src/routes/manifestations.rs::update_metadata` handler.
Specifically, the `UpdateMetadataRequest` struct uses
`#[serde(default, with = "::serde_with::rust::double_option")]`
to distinguish three cases per RFC 7396 JSON Merge Patch:

- Field absent in the body → `None` → leave unchanged.
- Field present as `null` → `Some(None)` → clear.
- Field present with a value → `Some(Some(value))` → set.

Why `serde_with` and not hand-rolled `Option<Option<T>>` decode:
`Option<Option<T>>` with bare serde collapses
"absent" and "null" into the same `None` value, which breaks
Merge Patch semantics. The `serde_with::rust::double_option`
helper is the canonical solution in the serde ecosystem
documented in `serde_with`'s docs. Hand-rolling the decode in
each PATCH request would mean a custom `Visitor` per field; not
worth it for an established crate that does precisely this.

Why `serde_with` v3 specifically: v3 is the current stable line,
React-compat with the existing `serde 1.0.228` pin. v2 is
end-of-life.

Note on `serde_with` surface area: the crate is large (it
contains many helpers we will not use). The default feature set
is `["std", "macros"]`; `macros` pulls in the `serde_with_macros`
proc-macro crate, which `double_option` does not require. The
correct entry for 11c is therefore:

```toml
serde_with = { version = "3", default-features = false, features = ["std"] }
```

This drops the proc-macro compile-time cost while keeping the
helper accessible. If a future use needs `serde_with_macros`
(e.g. the `#[serde_as]` attribute), add `"macros"` back with an
updated ADR or per-PR justification.

### `subtle`: first non-test consumer documented (no Cargo.toml change)

`subtle = "2.6.1"` is already in tree (was added when
`auth/token.rs` adopted constant-time SHA-256 hex compare for
the device-token verifier: see
[`backend/src/auth/token.rs:50-56`](../backend/src/auth/token.rs)).

Sub-phase 11a Task 1c introduces the **second** consumer:
`backend/src/security/csrf.rs::csrf_required` middleware
compares the incoming `X-CSRF-Token` header byte-slice to the
session-stored token byte-slice via `subtle::ConstantTimeEq`.
The compare is constant-time relative to the token length;
mismatches at byte position 0 take the same wall-clock time as
mismatches at byte position 31.

Why constant-time matters here (per the OWASP CSRF cheat sheet
and Reverie's threat model
[`project_open_source_security_stance`](../.claude/projects/-home-coder-reverie/memory/project_open_source_security_stance.md)):
a timing-leak in CSRF compare lets an attacker who can issue
many requests against the gateway narrow the valid-token search
space byte by byte, in the same way they can attack a
non-constant-time HMAC verifier. Reverie pins this defense for
every secret-vs-presented comparison; the precedent is
`auth/token.rs::verify_device_token`.

No `Cargo.toml` change. This ADR documents the new consumer
pattern so the convention is searchable from the ADR index
rather than buried in a per-handler docstring.

## Consequences

- **Good**: `axum-extra` feature flip is the minimum diff that
  forward-compatibly supports 11b filter params. No dependency
  count delta, no version churn.
- **Good**: `serde_with` is the canonical solution for the
  Merge Patch decode problem. Adopting it in 11c (when the first
  PATCH endpoint lands) avoids hand-rolling a fragile
  three-state decoder. Maintainability cost is low: the crate
  is widely used in the serde ecosystem, RustSec-clean, and the
  `double_option` helper has been stable across v2 and v3.
- **Good**: documenting `subtle`'s second consumer in this ADR
  keeps the constant-time-compare convention discoverable. New
  contributors who need to compare a presented secret can find
  the prior art (device-token + CSRF-token) without grep
  archaeology.
- **Bad**: `serde_with` is a large crate (many helpers,
  generated macro expansion is non-trivial). We accept the
  compile-time + binary-size cost in exchange for not
  hand-rolling Merge Patch decoders. Mitigation: never enable
  optional `serde_with` features without an updated entry in
  this ADR or a successor.
- **Bad**: `axum-extra` `"query"` feature pulls in `serde_qs`
  transitively. Reverie's `Cargo.lock` will grow by ~1 indirect
  dependency. Acceptable cost.
- **Neutral**: `subtle` is already cargo-cached and audited.
  Adding a second consumer changes nothing at the build-graph
  level.

## Alternatives Considered

### `serde_qs` (top-level instead of `axum-extra` "query")

Add `serde_qs = "0.13"` as a new top-level dep. Use
`serde_qs::axum::QsQuery<T>` extractor.

Rejected: `axum-extra` is already in tree. Enabling its
`"query"` feature is the smaller diff; adding `serde_qs` as a
sibling top-level adds a new dependency for substantially the
same surface.

### Hand-roll `Option<Option<T>>` Merge Patch decode

Write a per-field `Visitor` impl that distinguishes absent /
null / value. Cheapest in dependencies.

Rejected: `serde_with::rust::double_option` is exactly this,
written, tested, audited, and reused across the serde
ecosystem. Hand-rolling is not the kind of thing Reverie should
own forever.

### Replace `subtle` with the standard library

There is no `std` constant-time-compare. Hand-rolled
`bytes.iter().zip(other.iter()).fold(0, |acc, (a, b)| acc | (a
^ b))` is the textbook pattern. `subtle::ConstantTimeEq`
encapsulates this with optimisation-barrier protection (LLVM
will not reorder the fold into a short-circuit short of the
crate's volatile-read barriers).

Rejected: `subtle` is already in tree; reinventing the barrier
opens us to subtle compiler-rewrite bugs that the crate
explicitly defends against. Use the audited crate.

### `axum-form` or third-party query-string extractors

Various community crates exist for repeated-key decode.

Rejected on "already in tree" grounds, `axum-extra` `"query"`
is the path of least resistance.

## More Information

- [`feedback_industry_standard_default`](../.claude/projects/-home-coder-reverie/memory/feedback_industry_standard_default.md):
  defaults to standard-library / audited-crate idioms over
  hand-rolled primitives.
- [`feedback_audit_ignores`](../.claude/projects/-home-coder-reverie/memory/feedback_audit_ignores.md),
  handling of `cargo audit` findings on these new packages.
- Sibling ADR:
  [`2026-05-22-json-api-conventions.md`](2026-05-22-json-api-conventions.md)
  (RFC 7396 Merge Patch decision; CSRF synchronizer-token
  decision).
- Sibling ADR:
  [`2026-05-22-frontend-data-layer-deps.md`](2026-05-22-frontend-data-layer-deps.md)
  (frontend dependency adoptions for Step 11).
- Implementation plan: `.claude/PRPs/plans/library-ui.plan.md`
  (Sub-phase 11a Tasks 1c + 4; 11c Task 1).
- Tracker: the Step 11 API conventions work.
