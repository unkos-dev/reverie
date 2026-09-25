---
type: ADR
profile-version: 1
id: "REV-ADR-0023"
title: "Declarative configuration stack: figment, validator, schemars"
status: "accepted"
recorded-on: "2026-09-05"
decided-on: "2026-06-09"
decision-makers:
  - "John Unkovich"
---

# Declarative configuration stack: figment, validator, schemars

## Context and problem statement

Reverie's runtime configuration was loaded by a hand-rolled imperative reader in `backend/src/config.rs`:
`Config::from_source` threaded around 40 environment variables across six structs (`Config` plus `EnrichmentConfig`,
`CoverConfig`, `WritebackConfig`, `OpdsConfig`, `SecurityConfig`) as a long ladder of `get("KEY")` /
`parse_*(get, "KEY", default)` calls, each with bespoke `unwrap_or_else` defaults, range checks, and error mapping. The
file was around 1370 lines (about half of that tests) and had grown from around 15 variables at scaffold time to its
current size without the loading approach ever being revisited.

The trigger was a generated configuration reference: the config contract lived in imperative call-site code plus
doc-comment prose, so nothing could introspect it, not a documentation tool, not a schema emitter, not a
config-validation artifact. A prior review of the proposed reference generator, which would have parsed the source with
`syn` and read each field's `///` doc prose, found that approach structurally unsound for the same reason: the docs gap
was a symptom, and the cause was that the configuration was not a declarative, machine-introspectable structure.

The deeper observation: the imperative reader reimplements, by hand, a subset of what the standard Rust layered-config
stack provides declaratively (layering, typed deserialization, validation, test injection). It met ordinary requirements
in a non-standard way, and that non-standardness is what blocked every introspection tool.

Should Reverie keep meeting these requirements imperatively, or adopt the standard declarative configuration stack:
making the struct the single source of truth and letting documentation, validation, and testing fall out of it?

This decision interacts with
[Migration model: hybrid entrypoints and a least-privilege role](./0014-migration-model-hybrid-entrypoints-and-a-least-privilege-role.md)
(the conditional-required `DATABASE_URL_MIGRATION` behaviour the loader must preserve) and with
[Standards-first integrations over bundled adjacent services](./0022-standards-first-integrations-over-bundled-adjacent-services.md)
(the "prefer the boring open standard" philosophy this decision extends to the config layer). It is distinct from
[Persist operator-tunable settings to database with live reload](./0012-persist-operator-tunable-settings-to-database-with-live-reload.md),
which governs database-backed runtime settings, a separate surface from environment configuration.

## Decision drivers

- Introspectability: the configuration reference must be generated from a machine-readable structure, not prose scraped
  by a parser.
- Single source of truth: each variable's name, type, default, and required-ness should be declared once on the field,
  so documentation cannot drift from the loader.
- Standards-first: prefer the conventional, well-vetted layered-config stack over hand-rolled primitives, consistent
  with the project's standing posture.
- Preserve the security-relevant surface: the loader handles secrets (by name/shape only), the CSP-report-endpoint
  header-injection check, role-scoped DSN separation, and conditional-required migration credentials. None of these may
  regress.
- Preserve the test seam: config is unit-tested by injecting environment via a closure so tests never mutate process
  env, keeping config tests hermetic and parallel-safe.
- Operator-facing errors must continue to name the offending variable and give an actionable reason.
- Pre-v1.0 latitude: breaking changes to the developer environment are acceptable now, since there is no external
  compatibility contract on env-var names yet.

## Considered options

- Keep the imperative reader
- Minimal declarative-for-docs light path
- Full declarative stack

## Decision outcome

Chosen option: **Full declarative stack**, because it makes the config struct the single introspectable source of truth,
which resolves the configuration-reference generator soundly and removes the class of hand-rolled drift the imperative
reader invites. The stack:

Figment owns layered configuration loading, serde owns typed deserialisation, validator owns declarative and cross-field
validation, and schemars derives the configuration schema and reference. Together they keep the configuration structs as
the source of truth.

Figment was chosen over config-rs because its errors retain the key path and source, and it can remap environment names
that do not follow struct nesting. Config-rs's underscore-based nesting conflicts with snake_case field names. Validator
was chosen over `garde` because wider adoption offered a stronger scrutiny signal for security-relevant configuration,
despite the latter's cleaner cross-field API.

### Consequences

- Positive: each variable's name, type, default, and required-ness is declared once on the field; the reference
  generator reads the structure instead of parsing prose, and documentation cannot silently drift from the loader.
- Positive: validation gains a consistent, well-vetted framework with field-attributed, aggregated errors, an
  improvement over fail-fast `if`-ladders on a surface that includes a security-relevant injection check.
- Positive: figment's layering leaves the door open to optional config-file support later without another rewrite
  (enabled, not pursued here).
- Positive: the operator env-var surface stays deliberately mixed, bare ecosystem-canonical names (`DATABASE_URL`,
  `OIDC_*`, `RUST_LOG`) alongside `REVERIE_`-namespaced app-specific knobs, matching mature self-hosted peers.
  Regularising every var to mirror the struct nesting (for example `__`-separated, `REVERIE_OPDS__PUBLIC_URL`) would let
  stock `figment::Env::split("__")` drop most of the per-key map, but was rejected: it spends pre-v1.0 latitude to
  degrade operator ergonomics (longer, typo-prone names) and to make the var-to-field registry implicit.
- Positive: the backend runs two schema systems on disjoint surfaces, utoipa for the HTTP API and schemars for config,
  with no type described by both, so there is no duplication, only two purpose-built tools on separate surfaces.
- Negative: it touches every field of a working, secret-handling, security-relevant subsystem, so the implementation
  carries a security review covering secret handling (name/shape only), the CSP-report-endpoint injection check,
  role-scoped DSN separation, and conditional-required migration credentials. Secrets are represented by name/shape only
  in every emitted artifact, including the schemars JSON Schema, which must never carry a default value for a
  secret-bearing field.
- Negative (accepted): developer environments keyed on the current env-var layout may need adjustment; acceptable
  pre-v1.0, where no external env-var contract exists.

## Pros and cons of the options

### Keep the imperative reader

- Positive: zero change to a working, well-tested, security-relevant subsystem.
- Negative: it does not make the config introspectable; the docs generator stays unsound (prose-scraping) and drift
  remains structurally possible.
- Negative: it entrenches a non-standard hand-rolled reader as the pattern.

### Minimal declarative-for-docs light path

Derive `schemars::JsonSchema` on the existing structs, annotate each field's env-name and default, and render the
reference from the emitted schema. Extend the existing default-assertion and call-site-coverage tests to close drift.
Leave `from_source` untouched: no figment, no validator.

This option is a genuine contender, not a strawman: it solves the docs trigger and every sub-problem the review
identified (non-`REVERIE_` vars, nested structs, the log cascade) by making each fact a declared annotation rather than
parsed prose, at a fraction of the surface area and risk. It is rejected only because the larger declarative gains (a
true single source of truth, validation aggregation, and config-file readiness via figment's layering) were judged worth
the larger change now, pre-v1.0, rather than deferred. The honest trade-off is this option's test-closable drift risk
against the full stack's regression risk on a working, secret-handling, well-tested subsystem.

- Positive: it solves the docs trigger and every identified sub-problem at a fraction of the surface area: `from_source`
  is untouched, so the security-relevant load path carries no regression risk.
- Positive: the annotation-to-loader drift gap is largely closable with a test extending the existing default
  assertions.
- Neutral: it adds only `schemars` plus field annotations.
- Negative: the env-name and default are still declared in two places (annotation and loader); it is a partial single
  source, not a true one.
- Negative: it leaves the imperative reader, and the broader "config is not declarative" problem, in place, foregoing
  validation aggregation and config-file readiness.

### Full declarative stack

- Positive: the struct is the single source of truth; docs, validation, and a JSON Schema artifact all derive from it.
- Positive: it adopts standard, vetted crates over hand-rolled primitives.
- Negative: it is the largest change, on a security-relevant surface, with a meaningful regression-risk surface and a
  security review attached.
- Neutral: several custom behaviours (cascade, conditional-required, fallback, enum parses, env-name mapping) survive as
  code regardless of the stack.

## More information

Related:
[Migration model: hybrid entrypoints and a least-privilege role](./0014-migration-model-hybrid-entrypoints-and-a-least-privilege-role.md)
(conditional-required `DATABASE_URL_MIGRATION`),
[Standards-first integrations over bundled adjacent services](./0022-standards-first-integrations-over-bundled-adjacent-services.md)
(standards-first philosophy),
[Backend auxiliary crates: axum-extra, serde_with, and subtle](./0009-backend-auxiliary-crates-axum-extra-serde-with-and-subtle.md)
(backend dependency-adoption precedent),
[Persist operator-tunable settings to database with live reload](./0012-persist-operator-tunable-settings-to-database-with-live-reload.md)
(distinct database-backed runtime-settings surface).
