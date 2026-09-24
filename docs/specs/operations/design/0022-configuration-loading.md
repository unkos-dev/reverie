---
type: DESIGN
profile-version: 1
id: "REV-DESIGN-0022"
title: "Configuration loading"
satisfies:
  - "REV-REQ-0060"
  - "REV-REQ-0061"
  - "REV-REQ-0064"
governed-by:
  - "REV-ADR-0023"
---

# Configuration loading

This Design covers how Reverie turns the process environment into a validated `Config`: the declarative figment pipeline
in `backend/src/config/mod.rs`, the operator-facing variable registry (`ENV_MAP`) and its two completeness checks, the
post-deserialise startup gates, the secret-scrubbing behaviour of `ConfigError`, and the generator that renders the
configuration reference and `backend/config.schema.json` from the same schema.

## Purpose and boundaries

This subject owns the mechanism that gets a variable from the environment into a typed, validated field of `Config`: the
custom `figment::Provider` (`EnvProvider` in `backend/src/config/provider.rs`), the `ENV_MAP` registry it and the
reference generator both read, `Config::from_figment`'s six post-deserialise gates, the `ConfigError` shape and its
var-name and secret-scrubbing behaviour, and `reference_markdown`/`config_schema_json`, the two renderers that turn the
same `schemars` schema into the committed configuration reference and JSON Schema artifacts. It does not own the meaning
or defaults of any individual domain's fields (enrichment, cover, writeback, OPDS, security headers, password policy,
OIDC): those belong to the subjects that consume them, and this Design names the consuming code paths without restating
their behaviour.

`ENV_MAP` is the registry for every schema-visible field of `Config` (every field not marked `#[schemars(skip)]`), not
for every environment variable Reverie reads. Three fields are computed after deserialisation rather than
environment-sourced, and are excluded from both the schema and `ENV_MAP` for that reason: `ingestion_dsn_defaulted`,
`security.csp_html_header` and `security.csp_api_header`. Two call sites deliberately read the environment directly,
outside this pipeline, each for a stated reason: `run_migrate` in `backend/src/lib.rs` (the `reverie migrate`
subcommand) reads only `DATABASE_URL_MIGRATION` through `resolve_migration_dsn`, because a migrate invocation has no
business holding the OIDC secret or the application DSN that building a full `Config` would require;
`read_bootstrap_seed`, also in `backend/src/lib.rs`, reads `REVERIE_BOOTSTRAP_EMAIL`, `REVERIE_BOOTSTRAP_DISPLAY_NAME`
and `REVERIE_BOOTSTRAP_PASSWORD` directly because the seed password is a one-shot startup credential that must never be
retained on the long-lived `Config`. Neither path is gated by this subject's Gates 1-6, and a malformed value on either
surfaces as whatever error the consuming code produces, not a `ConfigError`.

Depends on: the process environment, populated by the operator before the binary starts (a container's declared
environment, or a sourced dev env file); `backend/src/models/manifestation_format.rs` for the `ManifestationFormat` enum
`format_priority` deserialises into; `backend/src/security/dist_validation.rs` and `backend/src/security/csp.rs`, which
`crate::run` calls after this pipeline to finalise the two CSP header fields this subject leaves `None`.

Depended on by: `crate::run`, which builds the primary, ingestion and writeback pools, the OIDC client and the router
from the loaded `Config`, and every subsystem reachable from `AppState::config` (`backend/src/state.rs`) or a config
sub-struct threaded into a service constructor; `backend/src/lib.rs`'s `build_router_with_session_store`, which reads
`config.security.behind_https` to decide the session cookie's `Secure` attribute (`backend/src/routes/auth.rs` only
tests this behaviour); `backend/src/security/headers.rs`, which reads the same field for HSTS emission; `run_bootstrap`,
`run_reset_password` and `run_unlock_account` (`backend/src/lib.rs`), CLI subcommands that call `Config::from_env`
directly rather than through `crate::run`; the `reverie print-config-schema` CLI subcommand and the two drift tests
(`backend/tests/gen_config_ref.rs`, `backend/tests/gen_config_schema.rs`) that gate the generated artifacts this subject
renders.

## Structure

`backend/src/config/mod.rs` declares `Config` as a flat struct with five nested subsystem structs (`EnrichmentConfig`,
`CoverConfig`, `WritebackConfig`, `OpdsConfig`, `SecurityConfig`, one file each under `backend/src/config/`), deriving
`serde::Deserialize`, `schemars::JsonSchema` and `validator::Validate` with `#[serde(default)]` on every struct so each
field's `Default` impl supplies its value when the environment supplies none. `Config::from_env` is the production entry
point; it builds a `Figment` from `EnvProvider::from_process_env()` and calls `Config::from_figment`.

`EnvProvider` (`backend/src/config/provider.rs`) is the `figment::Provider` this subject substitutes for the stock
`figment::providers::Env`. Its `data()` method walks a list of raw `(key, value)` pairs (`from_process_env` for
production, `from_pairs` for tests), drops any pair whose value is empty, looks each key up in `ENV_MAP`, parses the raw
string into a typed `figment::Value` the same way stock `Env` does, and nests it onto a dotted path. Two behaviours are
specific to this provider rather than inherited from figment: the `RUST_LOG`/`REVERIE_LOG_LEVEL` cascade (both map to
`log_level`; a present `REVERIE_LOG_LEVEL` pair causes the `RUST_LOG` pair to be skipped, independent of pair order),
and the flat-versus-nested split driven entirely by `ENV_MAP`'s explicit dotted paths rather than a separator
convention, because `REVERIE_DB_MAX_CONNECTIONS` must land on the flat `db_max_connections` field while
`REVERIE_ENRICHMENT_CONCURRENCY` must nest under `enrichment.concurrency`, and no single splitting rule produces both.

Four registries in `backend/src/config/mod.rs` and `provider.rs` together decide what varies and what is required, and
each has a distinct shape and a distinct completeness check:

- `ENV_MAP` (`provider.rs`): every operator-facing variable name paired with the dotted field path it feeds. This is the
  widest registry; the reference generator and `EnvProvider::data` both iterate it, and it is the only one of the four
  checked in both directions (see Failure and recovery).
- `REQUIRED_FIELDS` (`mod.rs`): variable name paired with a `RequiredFieldAccessor` function reading the resolved field
  back off a built `Config`, the single entry `("DATABASE_URL", |c| c.database_url.as_str())`. Pairing the name and its
  field reader in one tuple, rather than keeping two parallel lists, is what the module comment calls structurally
  impossible to get out of step: adding a required field is one new entry, not two lists kept aligned by hand.
  `reference.rs::required_label` reads the same list to render the reference's "Required" column, so Gate 3's startup
  contract and the documented contract cannot diverge.
- `OIDC_FIELDS` and `RESOURCE_SERVER_FIELDS` (`mod.rs`): the same name-plus-accessor shape as `REQUIRED_FIELDS`, but
  conditionally required together once a trigger field (`oidc_issuer_url`, `resource_server_issuer`) is non-blank.
  `OIDC_FIELDS` lists the issuer field first deliberately, so an issuer-only block reports the next missing field rather
  than reporting the issuer itself as missing when it is in fact the one field already present.
- `SECRET_FIELDS` (`mod.rs`): six dotted paths (`database_url`, `migration_database_url`, `ingestion_database_url`,
  `oidc_client_secret`, `googlebooks_api_key`, `hardcover_api_token`) consulted only by `map_figment_error`, to scrub a
  deserialise-phase error before it can echo a secret's value.

A fifth required-together rule exists outside this list-and-loop pattern entirely: `OpdsConfig`
(`backend/src/config/opds.rs`) defaults `enabled` to `true` and requires `public_url` whenever it is, enforced by
`validate_opds_config`, a `#[validate(schema(function = ...))]` struct-level function that runs during the final
`cfg.validate()` call rather than as one of `Config::from_figment`'s six numbered gates. Because `enabled` defaults to
`true`, this rule fires on a load that sets no OPDS variable at all, not only on one that sets
`REVERIE_OPDS_ENABLED=true` explicitly (see Runtime behaviour).

`reference.rs` and the `config_schema_json` function in `backend/src/lib.rs` both start from the same
`schemars::schema_for!(Config)` value, so a field's doc comment, default and range constraint are declared once and
consumed by two renderers rather than duplicated: `reference_markdown` walks `ENV_MAP` and, for each variable, resolves
its dotted path to a schema node (descending through `$ref`-linked sub-struct definitions at each segment) to render the
Markdown table row; `config_schema_json` serialises the schema directly. `backend/tests/gen_config_ref.rs` and
`backend/tests/gen_config_schema.rs` are the drift gates comparing a fresh render against the committed
`website/src/content/docs/reference/configuration.mdx` and `backend/config.schema.json`.

## Interfaces and dependencies

- `Config::from_env() -> Result<Config, ConfigError>` (`backend/src/config/mod.rs`) is the sole production entry point;
  callers that reach it through `crate::run` never call it directly.
- `Config::from_figment(&Figment) -> Result<Config, ConfigError>` is the pipeline embedding callers and tests drive
  directly, taking a caller-built `Figment` so tests can inject `EnvProvider::from_pairs` instead of the process
  environment.
- `figment::Provider` is the trait `EnvProvider` implements; `EnvProvider::from_process_env()` and
  `EnvProvider::from_pairs(&[(&str, &str)])` are its two constructors.
- `reference_markdown() -> anyhow::Result<String>` and `config_schema_json() -> anyhow::Result<String>` are the two
  generator entry points; the latter also backs the `reverie print-config-schema` CLI subcommand (`backend/src/lib.rs`),
  which reads no environment and opens no database.
- `ConfigError` (`MissingVar(String)`, `Invalid { var, reason }`, `Multiple(Vec<ConfigError>)`) is the error type every
  gate and every `validate()` failure maps onto; callers match it to build an operator-facing message.

## Data and state

`Config` is built once, at process startup, and is not reloaded while the process runs; operator-tunable settings that
change at runtime live in a separate database-backed surface this subject does not own. Within that one build, three
fields are written after `figment.extract()` inside `Config::from_figment` itself: Gate 1 forces
`migration_database_url` to `None` whenever `auto_migrate` is false, regardless of what `DATABASE_URL_MIGRATION`
supplied; Gate 2 clones `database_url` into `ingestion_database_url` whenever the latter is blank and sets
`ingestion_dsn_defaulted` to `true` to record that the fallback fired. After `from_figment` returns, exactly one further
writer touches the built value: `crate::run` (`backend/src/lib.rs`) sets `security.csp_api_header` and, when
`security.frontend_dist_path` is set, `security.csp_html_header`, from the finalisation pass the Design "Response
security headers and CSP" covers; both fields stay `None` on a `Config` produced by `from_env` or `from_figment` alone,
so a caller embedding the library and skipping `crate::run` must perform that finalisation itself or ship a server that
emits no `Content-Security-Policy` header. Nothing else in the process mutates a built `Config`; `AppState::config`
(`backend/src/state.rs`) holds a clone no later request handler writes back.

## Runtime behaviour

**A minimal load: `DATABASE_URL` set, `REVERIE_OPDS_ENABLED=false`, nothing else**, driven by `Config::from_env`:

1. `EnvProvider::from_process_env` collects every process environment variable into raw pairs.
2. `EnvProvider::data` drops any pair whose value is empty, keeps only pairs whose key appears in `ENV_MAP`, parses each
   surviving value into a typed `figment::Value` (a numeric string becomes `Num`, exactly `true`/`false` becomes `Bool`,
   everything else stays `Str`), and nests each onto the dotted path `ENV_MAP` names.
3. `figment.extract()` deserialises the accumulated dict into `Config`; every field this load supplies no value for
   takes the value the container's `#[serde(default)]` reads off `Config::default()` (and each sub-struct's own
   `Default`).
4. Gates 1 and 2 run: `auto_migrate` is `false` by default, so Gate 1 forces `migration_database_url` to `None`;
   `ingestion_database_url` is blank by default, so Gate 2 clones `database_url` into it and sets
   `ingestion_dsn_defaulted`.
5. Gate 3 checks `REQUIRED_FIELDS`; `database_url` is non-blank because the operator set it, so it passes.
6. Gate 4 checks `oidc_configured()` (a non-blank `oidc_issuer_url`); by default it is blank, so the OIDC-fields loop is
   skipped, and the `local_auth_enabled`-or-`oidc_configured()` check passes because `local_auth_enabled` defaults to
   `true`.
7. Gate 5 checks `resource_server_configured()`; blank by default, so it is skipped. Gate 6 builds the `User-Agent`
   string from `operator_contact` (`None` by default) and confirms it parses as a valid header value.
8. `cfg.validate()` runs the `#[validate(range(...))]` attributes and the struct-level cross-field functions
   (`validate_security_config`, `validate_opds_config`); every range-checked field's default satisfies its own range,
   `validate_security_config` passes because every security default is `false`/`None`, and `validate_opds_config` passes
   because this load explicitly set `enabled` to `false`.
9. `Config::from_env` returns `Ok(cfg)`.

Omitting `REVERIE_OPDS_ENABLED=false` from that load changes the outcome at step 8 alone: `OpdsConfig::default()` sets
`enabled` to `true`, so `validate_opds_config` finds `enabled` true and `public_url` still `None`, and
`Config::from_env` instead returns `ConfigError::Invalid` naming `REVERIE_PUBLIC_URL` with the reason "required when
`REVERIE_OPDS_ENABLED=true`". A load naming only `DATABASE_URL` therefore never reaches `Ok`: the operator must also
either disable OPDS or supply `REVERIE_PUBLIC_URL`.

**An issuer-only OIDC block**, `OIDC_ISSUER_URL` set with the other three `OIDC_*` variables unset: `extract()`
deserialises `oidc_issuer_url` to the supplied value and the other three OIDC fields to their blank `String::new()`
defaults. Gate 4's `oidc_configured()` reads `true` because the issuer is non-blank, so the `OIDC_FIELDS` loop runs;
because the loop iterates `OIDC_FIELDS` in the order the constant declares it and the issuer field is listed first, the
loop's own field (the issuer) is checked and passes immediately, and the loop reports `MissingVar("OIDC_CLIENT_ID")` on
the next entry rather than naming the issuer that is in fact present.

**`REVERIE_LOCAL_AUTH_ENABLED=false` with no OIDC configured and no resource server configured**: Gate 4's OIDC branch
is skipped (`oidc_configured()` is `false`), but the following check,
`!cfg.local_auth_enabled && !cfg.oidc_configured()`, is true, so `from_figment` returns
`ConfigError::Invalid { var: "REVERIE_LOCAL_AUTH_ENABLED", reason: "at least one auth provider must be enabled: ..." }`.
Configuring `resource_server_issuer` and `resource_server_audience` in the same scenario does not change the outcome:
Gate 5 (resource-server-fields-required-together) is a separate check from the interactive-provider guard above it,
because a resource-server JWT authenticates an API caller without ever establishing a session, so it cannot itself
satisfy "at least one usable auth provider".

**Reference and schema generation**: `reference_markdown` builds the schema once, then for each `(var, path)` pair in
`ENV_MAP`, calls `node_for_path` to resolve `path` (following `$ref` links at each dotted segment) to a schema node,
renders that node's type, required-ness (from `REQUIRED_FIELDS`/`OIDC_FIELDS`/the literal `DATABASE_URL_MIGRATION`
check), default and description into one Markdown table row, and joins the rows under a fixed frontmatter and intro.
`config_schema_json` serialises the same schema directly to pretty-printed JSON. Both are exercised by
`backend/tests/gen_config_ref.rs` and `backend/tests/gen_config_schema.rs`, which compare a fresh render against the
committed artifact and fail if they differ; running either test with the `REGEN` environment variable set rewrites the
artifact instead of asserting.

## Failure and recovery

Deserialise-phase failure (`figment.extract()`) is fail-fast: the first field that cannot deserialise into its typed
slot stops the pipeline, and `map_figment_error` turns the underlying `figment::Error` into one `ConfigError::Invalid`
by reverse-mapping the error's dotted key path back to an operator-facing variable name through `ENV_MAP`. The
declarative `validate()` phase that runs after every gate is different: it aggregates every range and cross-field
failure across the whole struct tree into `ConfigError::Multiple`, via `collect_validation_errors` walking the nested
`ValidationErrors` tree and reverse-mapping each field-level failure the same way; a struct-level failure (from
`validate_security_config` or `validate_opds_config`) instead carries its variable name as an explicit `"var"` parameter
on the `ValidationError`, because those failures key under the tree's synthetic `"__all__"` entry, which no dotted path
resolves.

A required variable left unset is always `ConfigError::MissingVar`, never `Invalid`: Gate 3, the OIDC-fields loop and
the resource-server-fields loop each check this before any other gate can turn the same absence into a different error,
so the operator-facing message says "set the variable" rather than "fix the value" for every field these three gates
cover.

The `ENV_MAP`-to-schema completeness the module's doc comment describes is checked in two different ways with two
different failure signatures, not by one shared assertion. `reference.rs`'s `every_schema_field_has_an_env_map_entry`
test walks the leaf fields of the schema and asserts each has an `ENV_MAP` entry (schema ⊆ `ENV_MAP`); the reverse
direction is not a dedicated assertion but a side effect of `node_for_path` returning `None` for an `ENV_MAP` entry
whose dotted path resolves to nothing, which `reference_markdown` turns into an `anyhow` error that the
`config_reference_matches_committed_artifact` drift test then panics on via `.expect(...)`.

`REVERIE_AUTO_MIGRATE=true` with `DATABASE_URL_MIGRATION` unset, or set to a whitespace-only string, both fail Gate 1
with `ConfigError::MissingVar("DATABASE_URL_MIGRATION")`; the value is trimmed before the blank check specifically so a
whitespace-only export cannot boot the server carrying a credential no query can actually use.

## Security and operations

`ConfigError::Invalid`'s `reason` field never echoes the offending value for a field whose dotted path appears in
`SECRET_FIELDS`, on the deserialise phase: `map_figment_error` checks `SECRET_FIELDS` membership before building the
error and substitutes a fixed value-free reason whenever it matches. This defends against a value-shaped coercion
failure that would otherwise echo the raw string, for example `OIDC_CLIENT_SECRET=true`, which lands as a `Value::Bool`
and fails the `String` field with a message quoting `true`. The same guarantee does not extend to the `validate()`
phase: `map_validation_errors` renders a validator's own `message` field directly, with no `SECRET_FIELDS` check. No
field in `SECRET_FIELDS` carries a `#[validate(...)]` attribute, so that unguarded path has no secret-bearing field to
reach; a `#[validate(...)]` attribute on one would not be scrubbed by anything this pipeline does. Separately, the
schema-emitted default for a secret-bearing field is checked against real credential leakage by only three of the six
`SECRET_FIELDS` entries (`oidc_client_secret`, `googlebooks_api_key`, `hardcover_api_token`, asserted in
`config_schema_has_no_secret_default_values`); the other three, the DSN fields, are safe only through their defaults,
not because a test confirms it: `database_url` and `ingestion_database_url` default to an empty string, and
`migration_database_url`, which is optional, to `None`.

Gate 1 keeps the migration DSN out of the `Config` the server runs on: forcing `migration_database_url` to `None`
whenever `auto_migrate` is false runs on every load, not only when the operator remembers to omit the variable, so an
operator who exports `DATABASE_URL_MIGRATION` for convenience without also setting `REVERIE_AUTO_MIGRATE=true` does not
thereby put that credential into the configuration every pool and startup branch reads. The gate does not reach the
process environment, which this pipeline never modifies: the variable stays where the operator put it, and keeping it
off the serving process is a property of the deployment rather than of this subject.

## More information

- [Configuration reference](../../../../website/src/content/docs/reference/configuration.mdx): the generated,
  never-hand-edited artifact this subject renders.
