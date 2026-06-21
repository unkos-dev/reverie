---
status: "accepted"
date: 2026-06-09
supersedes: []
decision-makers: "John Unkovich"
consulted: "—"
informed: "Reverie contributors"
---

# Adopt a declarative configuration stack (figment + serde + validator + schemars)

## Context and Problem Statement

Reverie's runtime configuration is loaded by a hand-rolled imperative reader in
`backend/src/config.rs`: `Config::from_source` threads
~40 environment variables across six structs (`Config` plus `EnrichmentConfig`,
`CoverConfig`, `WritebackConfig`, `OpdsConfig`, `SecurityConfig`) as a long ladder
of `get("KEY")` / `parse_*(get, "KEY", default)` calls, each with bespoke
`unwrap_or_else` defaults, range checks, and error mapping. The file is ~1370
lines (about half of that tests) and grew from ~15 vars at scaffold time to its
current size without the loading approach ever being revisited.

The immediate trigger is the docs-as-done effort
([`.claude/PRPs/plans/docs-as-done.plan.md`](../.claude/PRPs/plans/docs-as-done.plan.md),
the configuration-reference generation task): hard rule 10 requires a generated **configuration reference** from
`config.rs`. An adversarial review of that plan found the proposed generator (which would
parse the source with `syn` and read each field's `///` doc prose) to be structurally
unsound. The config contract lives in imperative call-site code plus doc-comment
prose, so nothing can introspect it: not docs tooling, not a schema emitter, not
a config-validation artifact. The docs gap is a symptom; the cause is that the
configuration is not a declarative, machine-introspectable structure.

The deeper observation: the imperative reader reimplements, by hand, a subset of
what the standard Rust layered-config stack provides declaratively (layering,
typed deserialization, validation, test injection). It meets ordinary
requirements in a non-standard way, and that non-standardness is what blocks
every introspection tool.

Should Reverie keep meeting these requirements imperatively, or adopt the
standard declarative configuration stack: making the struct the single source
of truth and letting documentation, validation, and testing fall out of it?

This decision interacts with
[`2026-06-02-hybrid-migration-entrypoints-and-role.md`](2026-06-02-hybrid-migration-entrypoints-and-role.md)
(the conditional-required `DATABASE_URL_MIGRATION` behaviour the loader must
preserve) and with
[`2026-06-08-standards-first-integrations.md`](2026-06-08-standards-first-integrations.md)
(the "prefer the boring open standard" philosophy this decision extends to the
config layer). It is distinct from
[`2026-05-26-persisted-settings.md`](2026-05-26-persisted-settings.md), which
governs DB-backed _runtime_ settings, a separate surface from environment
configuration.

## Decision Drivers

- **Introspectability.** The configuration reference (the configuration-reference generation task) requires the
  config to be a machine-readable structure, not prose to be regex-scraped.
- **Single source of truth.** Each variable's name, type, default, and required-ness
  should be declared once on the field, so documentation cannot drift from the
  loader.
- **Standards-first.** Prefer the conventional, well-vetted layered-config stack
  over hand-rolled primitives, consistent with the project's standing posture.
- **Preserve the security-relevant surface.** The loader handles secrets (by
  name/shape only), the CSP-report-endpoint header-injection check, role-scoped
  DSN separation, and conditional-required migration credentials. None of these
  may regress.
- **Preserve the test seam.** Config is unit-tested by injecting environment via a
  closure so tests never mutate process env (for parallel test environment isolation). The replacement must keep
  hermetic, parallel-safe config tests.
- **Operator-facing errors.** Failures must continue to name the offending
  variable and give an actionable reason.
- **Pre-v1.0 latitude.** Breaking changes to the developer environment are
  acceptable now; there is no external compatibility contract on env-var names yet.

## Considered Options

- **A: Keep the imperative reader (status quo).** Solve the docs need some other
  way (e.g. a `syn` source parser, or a hand-maintained reference table).
- **B: Minimal declarative-for-docs ("light path").** Derive `schemars::JsonSchema`
  on the _existing_ structs, annotate each field's env-name and default, and render
  the reference from the emitted schema. Extend the existing default-assertion and
  call-site-coverage tests to close drift. **Leave `from_source` untouched**, no
  figment, no validator.
- **C: Full declarative stack (chosen).** Replace `from_source` with a
  `figment` + `serde` load pipeline, `validator` for validation, and `schemars`
  (scoped to the config crate) for the reference and a reusable JSON Schema.

Sub-choices within C:

- **Loader: `figment` over `config` (config-rs).** figment carries richer error
  metadata (key path + source), maps cleanly to the var-named `ConfigError`
  requirement, and its per-key `.map()` gives a flexible escape hatch for the
  awkward env-name mappings; config-rs's `_`-separator nesting collides with
  snake_case field names and offers less remapping. figment's provider model also
  replaces the `EnvGet` closure with an in-memory provider for tests.
- **Validation: `validator` over `garde`.** On a security-relevant surface,
  breadth of deployment is the stronger scrutiny signal than release recency:
  `validator` has roughly 10× the adoption of `garde` (≈9M vs ≈0.8M recent
  downloads), in a stable problem domain where a quiet release cadence reads as
  mature rather than abandoned. `garde` has the cleaner cross-field API and more
  recent releases, but fewer independent eyes.
- **Reference emission: `schemars`, scoped to the config crate, over reusing
  utoipa or a custom harvester.** A custom harvester would either reintroduce the
  brittle `syn` source-walk this decision exists to eliminate or require owning a
  bespoke proc-macro. Reusing utoipa (the API-side OpenAPI emitter) for config is
  off-label, awkward to introspect, and would couple the config refactor's
  sequencing to docs-as-done. schemars is the purpose-built struct→JSON-Schema
  tool, keeps config self-contained, and yields a reusable JSON Schema artifact.

Option B is a genuine contender, not a strawman: it solves the docs trigger and
every S1 sub-problem (non-`REVERIE_` vars, nested structs, the log cascade) by
making each fact a declared annotation rather than parsed prose, at a fraction of
the surface area and risk. It is rejected only because the larger declarative
gains (a true single source of truth, validation aggregation, and config-file
readiness via figment's layering) were judged worth the larger change now,
pre-v1.0, rather than deferred. The honest tradeoff recorded here is **B's
drift-risk (test-closable) versus C's regression-risk on a working,
secret-handling, well-tested subsystem.**

## Decision Outcome

Chosen option: **C (the full declarative stack)**, because it makes the config
struct the single introspectable source of truth, which resolves the configuration-reference generation task
generator soundly and removes the class of hand-rolled drift the imperative reader
invites. The stack:

- **`figment`**: layered loading (struct defaults → environment), nested-struct
  deserialization, and an in-memory provider for hermetic tests (replacing the
  `EnvGet` closure seam).
- **`serde`**: typed deserialization into the config structs (already in tree).
- **`validator`**: declarative range checks plus framework-hosted custom and
  cross-field validators (conditional-required, header-injection), with
  field-attributed error aggregation.
- **`schemars`**: `JsonSchema` derive on the config structs, scoped to the config
  crate; the configuration reference and a reusable JSON Schema artifact are
  rendered from it.

Crate versions are pinned in `backend/Cargo.toml` at implementation time and pass
the `cargo audit` gate; the implementation plan (prp-plan output), not this ADR,
owns the task sequence, the offload boundary, and the verification checklist.

### Consequences

- Good, because each variable's name, type, default, and required-ness is declared
  once on the field; the reference generator reads the structure instead of parsing
  prose, and documentation cannot silently drift from the loader.
- Good, because validation gains a consistent, well-vetted framework with
  field-attributed, aggregated errors: an improvement over fail-fast `if`-ladders
  on a surface that includes a security-relevant injection check.
- Good, because figment's layering leaves the door open to optional config-file
  support later without another rewrite (enabled, not pursued here).
- Good, because the configuration becomes a `backend/src/config/` module with one
  file per subsystem struct, replacing a ~1370-line monolith; the split falls out
  of the declarative shapes along the existing sub-struct seams.
- Bad, because it touches every field of a working, secret-handling,
  security-relevant subsystem. **The implementation carries a security review
  (hard rule 6)** covering secret handling (name/shape only), the CSP-report-endpoint
  injection check, role-scoped DSN separation, and conditional-required migration
  credentials. Secrets are represented by name/shape only in every emitted
  artifact, including the schemars JSON Schema, which must never carry a default
  _value_ for a secret-bearing field.
- Bad, because the declarative path deserializes `migration_database_url` from
  `DATABASE_URL_MIGRATION` unconditionally whenever it is set; the `auto_migrate`
  gate must be reapplied as a post-deserialize step, else the long-lived server
  re-acquires the migrator credential-in-memory that
  [`2026-06-02-hybrid-migration-entrypoints-and-role.md`](2026-06-02-hybrid-migration-entrypoints-and-role.md)
  deliberately eliminated.
- Bad (smaller than feared), because some of `from_source`'s complexity survives
  as custom code in new shapes: the `REVERIE_LOG_LEVEL > RUST_LOG > "info"`
  cascade, the conditional-required migration DSN, and the ingestion-DSN fallback
  (all post-deserialize), plus two custom field deserializers, `format_priority`
  (bare CSV→`Vec<enum>`) and `csp_report_endpoint` (raw-string injection guard).
  Implementation prototyping established that figment does **not** coerce
  `Str→num`/`Str→bool` from a raw-string provider, its own `Env` provider parses
  each value via `Value`'s `FromStr` first. Mirroring that parse in `EnvProvider`
  makes numeric coercion native and the strict-bool contract (which requires only
  lowercase `true`/`false`, rejecting `1`/`yes`) native too, so the per-field
  bool/number deserializers first anticipated are unnecessary; enum, `url::Url`,
  and `PathBuf` deserialize natively. The surviving custom surface is therefore
  narrower than a hand-rolled reader's, concentrated on the two
  non-standard fields.
- Neutral, because the environment-variable-name-to-nested-struct mapping is not
  solved declaratively by serde for sub-struct fields (a uniform `_` split cannot
  serve both flat snake_case fields like `db_max_connections` and nested fields
  like `enrichment.concurrency`). It is carried by a small custom
  `figment::Provider` (`EnvProvider`, ~60 lines) holding an explicit per-key
  var→dotted-field map; the `REVERIE_LOG_LEVEL > RUST_LOG` cascade is resolved
  inside that provider. The provider, rather than figment's lighter `Env::map()`,
  is justified less by the mapping than by the **test seam**: its in-memory
  `from_pairs` constructor keeps config tests parallel-safe without mutating
  process env (for parallel test environment isolation), which stock `figment::Env` (process-env-only) cannot do
  without `Jail`'s global-env lock and the `getenv`/`setenv` race. The map doubles
  as the introspectable var↔field registry the reference generator consumes. The
  revisit trigger below was evaluated against this provider and did not fire.
- Neutral, because the operator env-var surface is deliberately mixed: bare
  ecosystem-canonical names (`DATABASE_URL`, `OIDC_*`, `RUST_LOG`) alongside
  `REVERIE_`-namespaced app-specific knobs: rather than a uniform scheme.
  Regularizing every var to mirror the struct nesting (e.g. `__`-separated,
  `REVERIE_OPDS__PUBLIC_URL`) would let stock `figment::Env::split("__")` drop
  most of the per-key map, but was rejected: it spends pre-v1.0 latitude to
  degrade operator ergonomics (longer, `__`-typo-prone names) and to make the
  var↔field registry implicit. The bare/namespaced split matches mature
  self-hosted peers and is the intended contract. (`OIDC_*` staying bare, which
  risks collision on a shared host running another OIDC app, is flagged for
  separate reconsideration, not settled here.)
- Neutral, because the backend then runs two schema systems on disjoint surfaces:
  utoipa for the HTTP API and schemars for config. No type is described by both, so
  there is no duplication, only two purpose-built tools on separate surfaces.
- Bad (accepted), because developer environments keyed on the current env-var
  layout may need adjustment; acceptable pre-v1.0, where no external env-var
  contract exists.

### Confirmation

Configuration loads through figment's declarative pipeline with no hand-rolled
`env::var` / `get("KEY")` ladder remaining in the loader; every field's env
binding and default are declared in structured form (field metadata or an
explicit provider map), not parsed from prose, the `REVERIE_LOG_LEVEL` /
`RUST_LOG` cascade being the named multi-source exception. The behavioural config
tests (defaults and the process-env-free injection seam) stay green against the
new pipeline; the staging-example coverage test is rewritten against the
declarative structs, since it is coupled to the removed `get("KEY")` form.

## Pros and Cons of the Options

### A: Keep the imperative reader

- Good, because zero change to a working, well-tested, security-relevant subsystem.
- Bad, because it does not make the config introspectable; the docs generator stays
  unsound (prose-scraping) and drift remains structurally possible.
- Bad, because it entrenches a non-standard hand-rolled reader as the pattern.

### B: Minimal declarative-for-docs (light path)

- Good, because it solves the docs trigger and every S1 sub-problem at a fraction of
  the surface area: `from_source` is untouched, so the security-relevant load path
  carries no regression risk.
- Good, because the annotation↔loader drift gap is largely closable with a test
  extending the existing default assertions.
- Neutral, because it adds only `schemars` plus field annotations.
- Bad, because the env-name and default are still declared in two places (annotation
  and loader); it is a partial single-source, not a true one.
- Bad, because it leaves the imperative reader, and the broader "config is not
  declarative" problem, in place, foregoing validation aggregation and
  config-file readiness.

### C: Full declarative stack (chosen)

- Good, because the struct is the single source of truth; docs, validation, and a
  JSON Schema artifact all derive from it.
- Good, because it adopts standard, vetted crates over hand-rolled primitives.
- Bad, because it is the largest change, on a security-relevant surface, with a
  meaningful regression-risk surface and a security review attached.
- Neutral, because several custom behaviours (cascade, conditional-required,
  fallback, enum parses, env-name mapping) survive as code regardless of the stack.

## More Information

- Driver: [`.claude/PRPs/plans/docs-as-done.plan.md`](../.claude/PRPs/plans/docs-as-done.plan.md)
  (the configuration-reference generation task): the configuration-reference requirement and the adversarial-review
  finding (S1) that surfaced the imperative-config problem. The docs-as-done
  configuration-reference page lands after this refactor, not within it.
- Related: [`2026-06-02-hybrid-migration-entrypoints-and-role.md`](2026-06-02-hybrid-migration-entrypoints-and-role.md)
  (conditional-required `DATABASE_URL_MIGRATION`),
  [`2026-06-08-standards-first-integrations.md`](2026-06-08-standards-first-integrations.md)
  (standards-first philosophy),
  [`2026-05-22-backend-aux-crates.md`](2026-05-22-backend-aux-crates.md)
  (backend dependency-adoption precedent),
  [`2026-05-26-persisted-settings.md`](2026-05-26-persisted-settings.md)
  (distinct DB-backed runtime-settings surface).
- The imperative reader was tracked as accepted technical debt with this refactor
  as its lift condition; that entry is purged by the implementing PR (see `debt/`
  git history).
- Implementation plan, task sequence (including the `config/` module split as the
  closing move), and verification live in prp-plan output
  (`.claude/PRPs/plans/`), not here. The implementation epic is tracked as
  the configuration refactor epic.
- Revisit trigger: if implementation prototyping shows figment's env→nested-struct
  mapping cannot serve the existing variable layout without an outsized custom
  adapter, reconsider B (minimal light path) before committing the loader rewrite.
