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
`parse_*(get, "KEY", default)` calls, each with bespoke `unwrap_or_else` defaults, range checks, and error mapping.
The file was around 1370 lines (about half of that tests) and had grown from around 15 variables at scaffold time to
its current size without the loading approach ever being revisited.

The trigger was a generated configuration reference: the config contract lived in imperative call-site code plus
doc-comment prose, so nothing could introspect it, not a documentation tool, not a schema emitter, not a
config-validation artifact. A prior review of the proposed reference generator, which would have parsed the source
with `syn` and read each field's `///` doc prose, found that approach structurally unsound for the same reason: the
docs gap was a symptom, and the cause was that the configuration was not a declarative, machine-introspectable
structure.

The deeper observation: the imperative reader reimplements, by hand, a subset of what the standard Rust
layered-config stack provides declaratively (layering, typed deserialization, validation, test injection). It met
ordinary requirements in a non-standard way, and that non-standardness is what blocked every introspection tool.

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

- Introspectability: the configuration reference must be generated from a machine-readable structure, not prose
  scraped by a parser.
- Single source of truth: each variable's name, type, default, and required-ness should be declared once on the
  field, so documentation cannot drift from the loader.
- Standards-first: prefer the conventional, well-vetted layered-config stack over hand-rolled primitives, consistent
  with the project's standing posture.
- Preserve the security-relevant surface: the loader handles secrets (by name/shape only), the CSP-report-endpoint
  header-injection check, role-scoped DSN separation, and conditional-required migration credentials. None of these
  may regress.
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

Chosen option: **Full declarative stack**, because it makes the config struct the single introspectable source of
truth, which resolves the configuration-reference generator soundly and removes the class of hand-rolled drift the
imperative reader invites. The stack:

- `figment`: layered loading (struct defaults to environment), nested-struct deserialization, and an in-memory
  provider for hermetic tests, replacing the closure-based test seam. Chosen over `config` (config-rs): figment
  carries richer error metadata (key path plus source), maps cleanly to the var-named `ConfigError` requirement, and
  its per-key `.map()` gives a flexible escape hatch for the awkward env-name mappings; config-rs's `_`-separator
  nesting collides with snake_case field names and offers less remapping.
- `serde`: typed deserialization into the config structs (already in tree).
- `validator`: declarative range checks plus framework-hosted custom and cross-field validators
  (conditional-required, header-injection), with field-attributed error aggregation. Chosen over `garde`: on a
  security-relevant surface, breadth of deployment is the stronger scrutiny signal than release recency, and
  `validator` has roughly ten times the adoption of `garde` (about 9M vs about 0.8M recent downloads) in a stable
  problem domain where a quiet release cadence reads as mature rather than abandoned. `garde` has the cleaner
  cross-field API and more recent releases, but fewer independent eyes.
- `schemars`: `JsonSchema` derive on the config structs, scoped to the config crate; the configuration reference and
  a reusable JSON Schema artifact are rendered from it. Chosen, scoped to the config crate, over reusing utoipa or a
  custom harvester: a custom harvester would either reintroduce the brittle `syn` source-walk this decision exists
  to eliminate or require owning a bespoke proc-macro, and reusing utoipa (the API-side OpenAPI emitter) for config
  is off-label, awkward to introspect, and would couple the config refactor's sequencing to the docs effort.
  `schemars` is the purpose-built struct-to-JSON-Schema tool, keeps config self-contained, and yields a reusable
  JSON Schema artifact.

Crate versions are pinned in `backend/Cargo.toml` at implementation time and pass the `cargo audit` gate.

### Consequences

- Positive: each variable's name, type, default, and required-ness is declared once on the field; the reference
  generator reads the structure instead of parsing prose, and documentation cannot silently drift from the loader.
- Positive: validation gains a consistent, well-vetted framework with field-attributed, aggregated errors, an
  improvement over fail-fast `if`-ladders on a surface that includes a security-relevant injection check.
- Positive: figment's layering leaves the door open to optional config-file support later without another rewrite
  (enabled, not pursued here).
- Positive: the configuration becomes a `backend/src/config/` module with one file per subsystem struct, replacing
  a roughly 1370-line monolith; the split falls out of the declarative shapes along the existing sub-struct seams.
- Positive: the environment-variable-name-to-nested-struct mapping is carried by a small custom `figment::Provider`
  (`EnvProvider`, around 60 lines) with an in-memory `from_pairs` constructor, which keeps config tests
  parallel-safe without mutating process env; stock `figment::Env` is process-env-only and cannot do this without
  `Jail`'s global-env lock and the `getenv`/`setenv` race. The map doubles as the introspectable var-to-field
  registry the reference generator consumes.
- Positive: the operator env-var surface stays deliberately mixed, bare ecosystem-canonical names (`DATABASE_URL`,
  `OIDC_*`, `RUST_LOG`) alongside `REVERIE_`-namespaced app-specific knobs, matching mature self-hosted peers.
  Regularising every var to mirror the struct nesting (for example `__`-separated, `REVERIE_OPDS__PUBLIC_URL`) would
  let stock `figment::Env::split("__")` drop most of the per-key map, but was rejected: it spends pre-v1.0 latitude
  to degrade operator ergonomics (longer, typo-prone names) and to make the var-to-field registry implicit.
- Positive: the backend runs two schema systems on disjoint surfaces, utoipa for the HTTP API and schemars for
  config, with no type described by both, so there is no duplication, only two purpose-built tools on separate
  surfaces.
- Negative: it touches every field of a working, secret-handling, security-relevant subsystem, so the
  implementation carries a security review covering secret handling (name/shape only), the CSP-report-endpoint
  injection check, role-scoped DSN separation, and conditional-required migration credentials. Secrets are
  represented by name/shape only in every emitted artifact, including the schemars JSON Schema, which must never
  carry a default value for a secret-bearing field.
- Negative: the declarative path deserializes `migration_database_url` from `DATABASE_URL_MIGRATION` unconditionally
  whenever it is set; the `auto_migrate` gate must be reapplied as a post-deserialize step, else the long-lived
  server re-acquires the migrator credential in memory that
  [Migration model: hybrid entrypoints and a least-privilege role](./0014-migration-model-hybrid-entrypoints-and-a-least-privilege-role.md)
  deliberately eliminated.
- Negative (smaller than feared): some of `from_source`'s complexity survives as custom code in new shapes: the
  `REVERIE_LOG_LEVEL` > `RUST_LOG` > `"info"` cascade, the conditional-required migration DSN, and the
  ingestion-DSN fallback (all post-deserialize), plus two custom field deserializers, `format_priority` (bare
  CSV to `Vec<enum>`) and `csp_report_endpoint` (raw-string injection guard). Prototyping established that figment
  does not coerce `Str` to `num`/`bool` from a raw-string provider on its own; its `Env` provider parses each value
  via `Value`'s `FromStr` first. Mirroring that parse in `EnvProvider` makes numeric coercion native and the
  strict-bool contract (only lowercase `true`/`false`, rejecting `1`/`yes`) native too, so the per-field bool/number
  deserializers first anticipated are unnecessary; enum, `url::Url`, and `PathBuf` deserialize natively. The
  surviving custom surface is therefore narrower than a hand-rolled reader's, concentrated on the two non-standard
  fields.
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
reference from the emitted schema. Extend the existing default-assertion and call-site-coverage tests to close
drift. Leave `from_source` untouched: no figment, no validator.

This option is a genuine contender, not a strawman: it solves the docs trigger and every sub-problem the review
identified (non-`REVERIE_` vars, nested structs, the log cascade) by making each fact a declared annotation rather
than parsed prose, at a fraction of the surface area and risk. It is rejected only because the larger declarative
gains (a true single source of truth, validation aggregation, and config-file readiness via figment's layering) were
judged worth the larger change now, pre-v1.0, rather than deferred. The honest trade-off is this option's
test-closable drift risk against the full stack's regression risk on a working, secret-handling, well-tested
subsystem.

- Positive: it solves the docs trigger and every identified sub-problem at a fraction of the surface area:
  `from_source` is untouched, so the security-relevant load path carries no regression risk.
- Positive: the annotation-to-loader drift gap is largely closable with a test extending the existing default
  assertions.
- Neutral: it adds only `schemars` plus field annotations.
- Negative: the env-name and default are still declared in two places (annotation and loader); it is a partial
  single source, not a true one.
- Negative: it leaves the imperative reader, and the broader "config is not declarative" problem, in place,
  foregoing validation aggregation and config-file readiness.

### Full declarative stack

- Positive: the struct is the single source of truth; docs, validation, and a JSON Schema artifact all derive from
  it.
- Positive: it adopts standard, vetted crates over hand-rolled primitives.
- Negative: it is the largest change, on a security-relevant surface, with a meaningful regression-risk surface and
  a security review attached.
- Neutral: several custom behaviours (cascade, conditional-required, fallback, enum parses, env-name mapping)
  survive as code regardless of the stack.

## More information

Related: [Migration model: hybrid entrypoints and a least-privilege role](./0014-migration-model-hybrid-entrypoints-and-a-least-privilege-role.md)
(conditional-required `DATABASE_URL_MIGRATION`),
[Standards-first integrations over bundled adjacent services](./0022-standards-first-integrations-over-bundled-adjacent-services.md)
(standards-first philosophy),
[Backend auxiliary crates: axum-extra, serde_with, and subtle](./0009-backend-auxiliary-crates-axum-extra-serde-with-and-subtle.md)
(backend dependency-adoption precedent),
[Persist operator-tunable settings to database with live reload](./0012-persist-operator-tunable-settings-to-database-with-live-reload.md)
(distinct database-backed runtime-settings surface).
