---
severity: medium
surfaces: [developer, security]
adopted: 2026-06-09
adopted-because: pre-existing scaffold-era shape, recognised during the docs-as-done (UNK-370) adversarial review; decision to replace it recorded in adr/2026-06-09-declarative-config-stack.md
lift-when-class: internal-refactor
lift-when: the adr/2026-06-09-declarative-config-stack.md implementation ships — config loads via a figment declarative pipeline, no `get("KEY")` ladder remains in the loader, and the configuration reference generates from schemars
---

# Imperative hand-rolled config reader

## Constraint

Runtime configuration is loaded by a hand-rolled imperative reader in
[`backend/src/config.rs`](../backend/src/config.rs). `Config::from_source`
threads ~40 environment variables across six structs (`Config` plus
`EnrichmentConfig`, `CoverConfig`, `WritebackConfig`, `OpdsConfig`,
`SecurityConfig`) as a long ladder of `get("KEY")` / `parse_*(get, "KEY",
default)` calls, each with a bespoke `unwrap_or_else` default, range check, and
error map. The file is ~1370 lines (about half tests) and grew from ~15 vars at
scaffold time to its current size without the loading approach being revisited.

This shape was reasonable at ~15 flat vars — hand-rolling beat pulling a config
framework. It became a liability as the surface grew to ~40 vars across six
structs.

## Workaround

The reader works and is well-tested (defaults, the staging-example coverage
test, and a process-env-free injection seam — UNK-100). It is not broken; it is
the wrong _shape_: the configuration contract lives in imperative call-site code
plus doc-comment prose rather than in a declarative, machine-introspectable
structure.

## Why this isn't the right shape

1. **Nothing can introspect it.** Because the contract is imperative code +
   prose, no tool can read it: not documentation tooling, not a schema emitter,
   not a config-validation artifact. This is what blocks the generated
   configuration reference that hard rule 10 requires (UNK-370) — the docs gap
   is a _symptom_ of the imperative shape, not an independent problem.
2. **It reimplements a standard stack by hand.** Layering, typed
   deserialization, validation, and test injection are all provided
   declaratively by the conventional Rust config stack. The hand-rolled reader
   meets ordinary requirements in a non-standard way.
3. **Hand-rolled validation on a security-relevant surface.** Validation is
   scattered fail-fast `if`-checks rather than a consistent framework — on a
   surface that includes the CSP-report-endpoint header-injection check, secret
   handling, role-scoped DSN separation, and conditional-required migration
   credentials. Ad-hoc predicates are easier to get subtly wrong than a vetted
   validation layer with aggregated, field-attributed errors.
4. **Drift is structurally possible.** Each variable's name and default live in
   the imperative call site; documentation of them lives in field doc-comments.
   Nothing binds the two, so they can diverge silently.

## Lift conditions

This debt lifts when the decision recorded in
[`adr/2026-06-09-declarative-config-stack.md`](../adr/2026-06-09-declarative-config-stack.md)
is implemented: configuration loads through a figment + serde declarative
pipeline with `validator` for validation and `schemars` for the reference, no
hand-rolled `env::var` / `get("KEY")` ladder remains in the loader, and each
field's env binding + default are declared as structured metadata.

On resolution, **purge this entry** (delete the file, remove the README line);
the purge commit names the resolving PR.

Partial progress (e.g. the lighter schemars-only "light path" documented as the
ADR's rejected option B) would _not_ lift this entry — option B leaves the
imperative reader in place. Only the full declarative replacement removes the
wrong shape.

## Related

- [`adr/2026-06-09-declarative-config-stack.md`](../adr/2026-06-09-declarative-config-stack.md)
  — the decision to replace this reader; this entry is its accepted-debt
  companion.
- [`backend/src/config.rs`](../backend/src/config.rs) — the workaround site.
- Driver: docs-as-done (UNK-370) — the configuration-reference requirement that
  surfaced this shape.
- Linear: the config-refactor implementation epic (to be filed) carries the
  scheduled work; until it exists, this entry is the canonical record.
