# Feature: Declarative Configuration Stack (figment + serde + validator + schemars)

## Summary

Replace the hand-rolled imperative env reader in `backend/src/config.rs`
(`Config::from_source`, a ~1370-line ladder of `get("KEY")` /
`parse_*(get, "KEY", default)` calls) with a declarative stack: **figment**
loads layered defaults→env into the structs, **serde** deserializes, **validator**
runs declarative + cross-field validation with field-attributed errors, and
**schemars** emits a JSON Schema artifact (the introspectable source the UNK-370
config-reference page will later render from). Implements
`adr/2026-06-09-declarative-config-stack.md` (option C). The keystone is a
**single custom `figment::Provider`** that owns the env-var→field map, the
empty-as-unset filter, and the `REVERIE_LOG_LEVEL > RUST_LOG` cascade — the one
introspectable artifact that production load, the hermetic test seam, var-named
errors, and the staging-coverage test all read from. The `config.rs` monolith
splits into a `backend/src/config/` module as the closing move.

## User Story

As a **Reverie maintainer / external contributor**
I want **the configuration contract to live in one declarative, machine-introspectable structure**
So that **documentation, validation, and a JSON Schema fall out of the struct instead of being hand-maintained against an imperative reader that nothing can read — unblocking the UNK-370 config reference and removing a class of silent drift.**

## Problem Statement

The config contract lives in imperative call-site code plus doc-comment prose, so
no tool can introspect it (docs generator, schema emitter, config-validation
artifact). This blocks the generated configuration reference hard rule 10 / UNK-370
requires, and makes annotation↔loader drift structurally possible. The reader is
not broken — it is the wrong _shape_. Testable success: config loads through a
figment declarative pipeline, no `env::var`/`get("KEY")` ladder remains in the
loader, every field's env binding + default is declared as structured metadata,
a schemars JSON Schema emits, and every behavioural config test stays green
(coverage test rewritten against the declarative structs).

## Solution Statement

Layer a figment pipeline:
`Serialized::defaults(Config::default())` → `.merge(EnvProvider)` → `.extract()`
→ post-deserialize gates (`auto_migrate`, ingestion-DSN fallback) → `validate()`.
The custom `EnvProvider` takes `(key, value)` string pairs (production:
`std::env::vars()`; tests: an explicit slice), applies the env-name→dotted-field
remap + empty-as-unset filter + log cascade, and yields a string-valued figment
`Dict`. Its embedded map is the single source for: production load, the hermetic
in-memory test seam (no `Jail` — see GOTCHA-TESTSEAM), the field→var-name lookup
that keeps `ConfigError` var-named, and the staging-coverage test's allow-set.
validator handles range + cross-field checks; the CSP header-injection guard
stays at **deserialize time on the raw string** (custom `deserialize_with`, not a
validator on the parsed `Url` — see GOTCHA-CSPRAW). schemars derives the JSON
Schema with secret-bearing fields carrying no default value. `validate()` errors
aggregate into a new `ConfigError::Multiple` variant; deserialize-phase errors
(figment `extract`) remain fail-fast, one at a time (see GOTCHA-AGG).

## Metadata

| Field            | Value                                                                                                                                                                                                                                                       |
| ---------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Type             | REFACTOR                                                                                                                                                                                                                                                    |
| Complexity       | HIGH                                                                                                                                                                                                                                                        |
| Systems Affected | `backend/src/config.rs` (→ `config/`), `Cargo.toml`, `backend/CLAUDE.md`, `models/manifestation_format.rs`, bin entry (`print-config-schema`), `backend/config.schema.json` + CI drift-check, `docker/staging.env.runtime.example` (coverage test), `debt/` |
| Dependencies     | figment, validator, schemars (versions pinned at impl, pass `cargo audit`); `url` `serde` feature                                                                                                                                                           |
| Estimated Tasks  | 14                                                                                                                                                                                                                                                          |

---

## UX Design

This is an internal refactor — no end-user UI. "User" = operator (env vars) +
maintainer (struct + emitted schema). The observable contract (var names,
defaults, error messages) must not regress.

### Before State

```text
┌──────────────┐    ┌──────────────────────────────┐    ┌──────────────┐
│  process env │──▶ │ Config::from_source(&EnvGet) │──▶ │   Config     │
│  / .env      │    │  ~40 × get("KEY")            │    │  (+ 5 sub-   │
└──────────────┘    │  parse_* + bespoke defaults  │    │   structs)   │
                    │  scattered if-range checks   │    └──────────────┘
   tests inject ───▶│  EnvGet = Fn(&str)->Option   │
   HashMap closure  └──────────────────────────────┘
                                  │
                    PAIN: contract lives in imperative code; nothing can
                    introspect it → UNK-370 generator unsound; drift possible;
                    validation = ad-hoc if-ladders on a security surface.
```

### After State

```text
┌──────────────┐
│  process env │──┐
│  / .env      │  │  EnvProvider (custom figment::Provider)  ◀── SINGLE SOURCE
└──────────────┘  │   env-name→dotted-field map + empty-filter + log cascade
                  ▼
   Serialized::defaults(Config::default())            ┌──────────────┐
            │  .merge(EnvProvider)  .extract()    ──▶  │   Config     │
            ▼                                          │ #[derive(    │
   post-deserialize gates (auto_migrate, ing-DSN)      │  Deserialize,│
            │                                          │  JsonSchema, │
            ▼                                          │  Validate)]  │
   Config::validate()  (range + cross-field schema)   └──────────────┘
   [CSP injection guard runs earlier, at deser time on raw string]
            │                                                  │
   tests: EnvProvider::from_pairs(&[..])  (in-memory, no Jail) │
            │                                          schema_for!(Config)
            ▼                                                  ▼
   var-named ConfigError (field→var map)        config.schema.json artifact
                                                 (UNK-370 reads this later)
```

### Interaction Changes

| Location                   | Before              | After                                                                                               | User Impact                                     |
| -------------------------- | ------------------- | --------------------------------------------------------------------------------------------------- | ----------------------------------------------- |
| `Config::from_env()`       | imperative ladder   | figment pipeline                                                                                    | none — same vars, defaults, errors              |
| Config tests               | `&EnvGet` closure   | `EnvProvider::from_pairs` (strings)                                                                 | hermetic + parallel-safe preserved              |
| `ConfigError`              | hand-built per call | from validator + figment, mapped to var name; new `Multiple` variant aggregates `validate()` errors | same per-error shape; multi-error on validation |
| (new) `config.schema.json` | —                   | emitted JSON Schema                                                                                 | unblocks UNK-370 config-ref page                |

---

## Mandatory Reading

| Priority | File                                                      | Lines            | Why Read This                                                                                                             |
| -------- | --------------------------------------------------------- | ---------------- | ------------------------------------------------------------------------------------------------------------------------- |
| P0       | `adr/2026-06-09-declarative-config-stack.md`              | all              | The decision + every constraint to preserve; the revisit trigger                                                          |
| P0       | `backend/src/config.rs`                                   | 1-792            | The reader being replaced — every var, default, range, error                                                              |
| P0       | `backend/src/config.rs`                                   | 793-1373         | The test suite that must stay green (defaults, cascade, gates, security) + the staging-coverage test (854-912) to rewrite |
| P0       | `debt/2026-06-09-imperative-config-reader.md`             | all              | Lift condition — purge on merge                                                                                           |
| P1       | `adr/2026-06-02-hybrid-migration-entrypoints-and-role.md` | all              | The `auto_migrate`/`DATABASE_URL_MIGRATION` credential-in-memory trap                                                     |
| P1       | `backend/CLAUDE.md`                                       | 160-180, 330-340 | Operator namespacing rule (line 168) + project-structure tree (line 335) — both edited this PR (APPROVAL GATE)            |
| P1       | `backend/src/lib.rs`                                      | 165-230, 480-520 | `run()` consumes `Config::from_env()` (line 223) + log-level/migration usage                                              |
| P1       | `backend/src/test_support.rs`                             | 1-70, 400-410    | `test_config()` struct-literal builder — must still compile                                                               |
| P2       | `backend/src/security/headers.rs`                         | 420-640          | 11 `SecurityConfig { .. }` literal sites — must still compile                                                             |
| P2       | `docker/staging.env.runtime.example`                      | all              | The subset the coverage test scans                                                                                        |

**External Documentation:**
| Source | Section | Why Needed |
|--------|---------|------------|
| [figment /sergiobenitez/figment](https://docs.rs/figment) | custom `Provider` + `Serialized::defaults` + `Env` | The provider keystone + defaults layer |
| [validator /keats/validator](https://github.com/keats/validator) | `schema(function=...)`, `custom(function=...)`, `nested`, `range` | Cross-field + range + injection checks |
| [schemars /gresau/schemars](https://github.com/gresau/schemars) | `JsonSchema` derive, `schemars(skip)`, default attrs | Schema emit; secret-field default omission |

---

## Patterns to Mirror

**ERROR_HANDLING (var-named — preserve the two variants, ADD `Multiple` for aggregation):**

```rust
// SOURCE: backend/src/config.rs:331-347 (extend, do not replace)
pub enum ConfigError {
    #[error("missing required environment variable: {0}")]
    MissingVar(String),                                 // unset — distinct from Invalid
    #[error("invalid value for {var}: {reason}")]
    Invalid { var: String, reason: String },            // set-but-bad
    #[error("{0} configuration error(s):\n{}", .0.iter().map(ToString::to_string).collect::<Vec<_>>().join("\n"))]
    Multiple(Vec<ConfigError>),                          // NEW — validate() aggregation (GOTCHA-AGG)
}
// MissingVar vs Invalid is operator-meaningful (set the var vs fix the value) — do NOT
// collapse the two by defaulting required fields to "" then length-validating (GOTCHA-REQUIRED).
```

**CSP HEADER-INJECTION CHECK (security-load-bearing — RAW-STRING guard at DESERIALIZE time, NOT a validator on the parsed Url — GOTCHA-CSPRAW):**

```rust
// SOURCE: backend/src/config.rs:682-707  (order is load-bearing: check RAW chars BEFORE parse)
// Implement as #[serde(deserialize_with = "de_csp_endpoint")] producing Option<url::Url>:
if s.chars().any(|c| matches!(c, '"' | ';' | '\r' | '\n')) { /* reject — raw string */ }
let parsed = url::Url::parse(&s)?;          // url parse percent-encodes/normalizes —
if !matches!(parsed.scheme(), "http" | "https") { /* reject */ }
// A validator on the parsed Url sees `as_str()` (normalized): `"`→`%22` so the guard
// silently passes, and CR/LF make url::parse error with the WRONG message. Both break
// `security_report_endpoint_injection_chars_errors` (config.rs:1280-1294).
```

**STRICT BOOL (UNK-106/110 — only lowercase true/false; see GOTCHA-BOOL):**

```rust
// SOURCE: backend/src/config.rs:760-773
"true" => Ok(true), "false" => Ok(false),
_ => Err(ConfigError::Invalid { .. }), // "1"/"yes" rejected
```

**MIGRATION GATE (security trap — reapply post-deserialize):**

```rust
// SOURCE: backend/src/config.rs:416-425
let auto_migrate = parse_bool(get, "REVERIE_AUTO_MIGRATE", false)?;
let migration_database_url = if auto_migrate {
    let url = get("DATABASE_URL_MIGRATION").filter(|s| !s.trim().is_empty());
    if url.is_none() { return Err(ConfigError::MissingVar("DATABASE_URL_MIGRATION".into())); }
    url
} else { None }; // <-- MUST stay None when !auto_migrate even if env is set
```

**TEST SEAM (replace closure with string-pair provider, NOT Jail):**

```rust
// SOURCE: backend/src/config.rs:802-808 (the pattern to evolve)
fn env_for(vars: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> { .. }
// AFTER: EnvProvider::from_pairs(&[("REVERIE_PORT","8080"), ..]) merged into Figment
```

**STAGING-COVERAGE TEST (rewrite against the map — see Task 11):**

```rust
// SOURCE: backend/src/config.rs:869-912
// BEFORE: textual scan of config.rs for get("KEY") / get, "KEY"
// AFTER: example keys ⊆ EnvProvider's env-name map keys
```

---

## Files to Change

| File                                                                    | Action            | Justification                                                                                   |
| ----------------------------------------------------------------------- | ----------------- | ----------------------------------------------------------------------------------------------- |
| `backend/Cargo.toml`                                                    | UPDATE            | Add figment, validator, schemars; add `url` `serde` feature                                     |
| `backend/src/config.rs`                                                 | UPDATE→split      | Add derives + validator attrs; build figment pipeline + EnvProvider; later split into `config/` |
| `backend/src/config/mod.rs`                                             | CREATE (closing)  | Re-export all public types so `crate::config::*` paths survive                                  |
| `backend/src/config/provider.rs`                                        | CREATE (closing)  | The `EnvProvider` keystone + env-name map                                                       |
| `backend/src/config/{core,enrichment,cover,writeback,opds,security}.rs` | CREATE (closing)  | One file per subsystem struct                                                                   |
| `backend/src/models/manifestation_format.rs`                            | UPDATE            | Add `Deserialize` + `JsonSchema` (currently `FromStr` only)                                     |
| `backend/config.schema.json`                                            | CREATE            | Committed generated JSON Schema artifact; CI drift-checked (Task 10); UNK-370 consumes it       |
| `backend/src/main.rs` (or bin entry)                                    | UPDATE            | Add `print-config-schema` subcommand (Task 10)                                                  |
| `.github/workflows/*` (CI)                                              | UPDATE            | Add schema-drift `--check` job (Task 10)                                                        |
| `backend/CLAUDE.md`                                                     | UPDATE            | Line 168 cascade rule → figment mechanism; line 335 tree → `config/` module (APPROVAL GATE)     |
| `debt/2026-06-09-imperative-config-reader.md`                           | DELETE (on merge) | Lift condition met; remove `debt/README.md` line too                                            |
| `debt/README.md`                                                        | UPDATE (on merge) | Drop the purged entry's line                                                                    |

**No field-shape changes** to the six structs → the ~15 struct-literal sites
(test_support.rs, security/headers.rs, oidc.rs, writeback/ingestion orchestrators)
compile unchanged. Adding derives is additive. Verify, don't edit them.

---

## NOT Building (Scope Limits)

- **The UNK-370 config-reference doc page.** This PR emits the JSON Schema
  artifact + declarative structs; the rendered reference page lands in UNK-370.
- **Config-file (TOML/YAML) support.** figment's layering enables it; not pursued
  here (ADR: "enabled, not pursued").
- **Env-var renames / namespace changes.** Same var names, defaults, and
  precedence as today (pre-v1.0 latitude exists but is not spent here).
- **The OpenAPI/utoipa half of UNK-370.** Independent; unaffected.
- **garde / config-rs.** Rejected in ADR; do not introduce.

---

## Step-by-Step Tasks

Execute in order. Behaviour-preserving tasks 1–10 first; coverage-test rewrite 11;
docs/debt 12–13; module split 14 last (keeps the diff reviewable).

### Task 1: UPDATE `backend/Cargo.toml` — add dependencies

- **ACTION**: Add `figment`, `validator` (derive feature), `schemars`; add `serde` feature to `url`.
- **IMPLEMENT**: `figment = { version = "<pin>", features = ["env"] }`; `validator = { version = "<pin>", features = ["derive"] }`; `schemars = { version = "<pin>" }`; change `url = "2.5.8"` → `url = { version = "2.5.8", features = ["serde"] }`.
- **GOTCHA (GOTCHA-VERSION)**: Confirm schemars major — **0.8 and 1.0 differ in trait shape (`gen` vs `SchemaGenerator`) and attribute syntax**. Pin one, write all attrs against it. Confirm validator ≥ 0.18 for `schema(function = path)` (bare-path, not string) form. Context7 may blend versions — validate against the pin.
- **GOTCHA**: schemars may need its own `url` integration feature for `url::Url` to impl `JsonSchema`; check the pinned version's feature list.
- **VALIDATE**: `cargo audit` (each new dep passes) && `cargo build -p reverie-api`.

### Task 2: UPDATE `backend/src/models/manifestation_format.rs` — add derives

- **ACTION**: Add `serde::Deserialize` + `schemars::JsonSchema` to `ManifestationFormat` (keep `FromStr`).
- **GOTCHA**: `format_priority` is CSV (`epub,pdf,mobi`), NOT figment array syntax `[a,b]`. Keep a custom field deserializer (CSV split → `FromStr` per token, lowercased) — see Task 6. The derive here is for the element type + schema.
- **MIRROR**: existing `#[serde(rename_all="lowercase")]` enum at `config.rs:300-309` (`CleanupMode`).
- **VALIDATE**: `cargo build -p reverie-api`.

### Task 3: Add derives + serde attrs to the six config structs

- **ACTION**: Add `#[derive(Deserialize, JsonSchema, Validate)]` (alongside existing `Debug, Clone`) to `Config`, `EnrichmentConfig`, `CoverConfig`, `WritebackConfig`, `OpdsConfig`, `SecurityConfig`.
- **IMPLEMENT**: `#[serde(default)]` at struct level (defaults come from the `Default` impl in Task 4). Field renames where the serde/dotted name diverges from the Rust field (the EnvProvider maps env→dotted; serde sees dotted).
- **GOTCHA (GOTCHA-SECCONFIG)**: `SecurityConfig.csp_html_header` / `csp_api_header` are `HeaderValue`, run-computed, never env-sourced → `#[serde(skip)]` + `#[schemars(skip)]` (they default to `None`, finalised by `run()`). `frontend_dist_path` (`PathBuf`) and `public_url`/`csp_report_endpoint` (`url::Url`) need their types to impl `Deserialize`+`JsonSchema` (url via Task 1 feature).
- **GOTCHA (GOTCHA-CSPRAW)**: `csp_report_endpoint` gets `#[serde(default, deserialize_with = "de_csp_endpoint")]` (Task 7 defines the fn) — the header-injection guard must run on the **raw string before `url::Url::parse`**, so it lives in the deserializer, NOT in a `#[validate(custom)]` on the parsed `Url`.
- **GOTCHA (GOTCHA-SECRET)**: secret-bearing fields (`oidc_client_secret`, `googlebooks_api_key`, `hardcover_api_token`) carry **no** `#[schemars(default = "...")]` — no secret value in the emitted schema. Required secrets stay required (no default); optional ones are `Option` (no value leaks).
- **VALIDATE**: `cargo build -p reverie-api` (struct-literal sites still compile — additive derives).

### Task 4: Add `Default` impl carrying every current default

- **ACTION**: `impl Default` for each struct with the exact defaults from `from_source` (port 3000, db_max_connections 10, format_priority `epub,pdf,mobi,azw3,cbz,cbr`, cleanup `all`, enrichment concurrency 2, cover max_bytes 10_485_760, etc.).
- **GOTCHA (GOTCHA-REQUIRED — preserve `MissingVar`)**: required fields (`database_url`, `oidc_issuer_url`, `oidc_client_id`, `oidc_client_secret`, `oidc_redirect_uri`) must **NOT** be defaulted to `""`. Defaulting them collapses _unset_ (`MissingVar`, recovery: set the var) into _empty/invalid_ (`Invalid`), degrading the most common misconfiguration's operator message — and the existing `from_env_missing_database_url` test (substring-only assert) would NOT catch the regression. Two sound options: (a) model required fields as the deserialize target so figment yields a genuine **missing-key** error → map to `MissingVar`; or (b) keep them out of the `Serialized::defaults` layer and emit `MissingVar` explicitly in the post-extract required-check (Task 6). Do **not** use `length(min=1)` as the required-check — that yields `Invalid`, not `MissingVar`.
- **GOTCHA**: this `Default` is the `Serialized::defaults` base layer for the **optional** fields — the single source for every default value, replacing the scattered `unwrap_or_else(|| "X".into())`.
- **VALIDATE**: `cargo build`; scratch assert `Config::default().port == 3000`; a test asserting an unset `DATABASE_URL` yields the `MissingVar` **variant** (not just a substring).

### Task 5: CREATE the `EnvProvider` keystone (custom `figment::Provider`)

- **ACTION**: Implement `figment::Provider` for a struct holding `Vec<(String,String)>` pairs.
- **IMPLEMENT**:
  - `EnvProvider::from_process_env()` → `std::env::vars().collect()`.
  - `EnvProvider::from_pairs(&[(&str,&str)])` → for tests (strings, exercising the coerce path).
  - `data()`: for each pair, look up the **env-name → dotted-field** map (the embedded `const`/fn — see below), drop empties (empty-as-unset, GOTCHA-EMPTY), build a nested figment `Dict` (`Profile::Default`). Unmapped keys ignored (operator vars like `PATH`).
  - The **env-name map** is the introspectable artifact: `&[("REVERIE_PORT","port"), ("DATABASE_URL","database_url"), ("REVERIE_ENRICHMENT_CONCURRENCY","enrichment.concurrency"), ("OIDC_ISSUER_URL","oidc_issuer_url"), ...]` — every var, including non-`REVERIE_` (`DATABASE_URL`, `OIDC_*`) and nested (`enrichment.*`, `cover.*`, `writeback.*`, `opds.*`, `security.*`).
- **GOTCHA (GOTCHA-SPLIT)**: do NOT use `Env::split("_")` — it collides snake_case flat fields (`db_max_connections` → `db.max.connections`) with genuinely nested ones (`enrichment.concurrency`). The explicit per-key map is why a custom provider exists (ADR neutral consequence).
- **GOTCHA (GOTCHA-CASCADE)**: the `REVERIE_LOG_LEVEL > RUST_LOG > "info"` cascade lives here as a one-field pre-pass: if `REVERIE_LOG_LEVEL` absent but `RUST_LOG` present, emit `log_level` from `RUST_LOG`. The `"info"` floor is the `Default`.
- **VALIDATE**: `cargo build`; unit test that `from_pairs` yields the expected Dict for a flat + a nested key.

### Task 6: Build the figment pipeline in `Config::from_figment` + rewire `from_env`/`from_source`

- **ACTION**: Core fn `Config::from_figment(figment: Figment) -> Result<Self, ConfigError>`: `extract()` → post-deserialize gates + required-check → `validate()` (aggregated). `from_env()` = `dotenvy::dotenv().ok();` then `from_figment(Figment::from(Serialized::defaults(Config::default())).merge(EnvProvider::from_process_env()))`. Keep `from_source(&EnvGet)` as a thin shim OR migrate callers (oidc.rs:193) to `from_pairs` — prefer migrating; see Task 8.
- **IMPLEMENT custom field deser**: `format_priority` via `#[serde(deserialize_with)]` splitting CSV; `CleanupMode` via existing serde rename; `csp_report_endpoint` via `de_csp_endpoint` (GOTCHA-CSPRAW); bools via GOTCHA-BOOL handling.
- **GOTCHA (GOTCHA-MIGGATE — security-load-bearing)**: figment deserializes `migration_database_url` from `DATABASE_URL_MIGRATION` **unconditionally**. Reapply the gate post-extract: `if !auto_migrate { migration_database_url = None } else if migration_database_url.as_deref().map_or(true, |s| s.trim().is_empty()) { return Err(MissingVar("DATABASE_URL_MIGRATION")) }`. Without this the long-lived server re-acquires the migrator credential-in-memory that `2026-06-02-hybrid-migration-entrypoints-and-role.md` eliminated.
- **GOTCHA**: ingestion-DSN fallback (`DATABASE_URL_INGESTION` → `database_url`) is also post-deserialize (empty/absent → clone `database_url`).
- **GOTCHA (required-check → `MissingVar`)**: after extract, any required field still empty/absent (`database_url`, `oidc_*`) → `ConfigError::MissingVar(<env name>)`, NOT `Invalid` (GOTCHA-REQUIRED). This is the deserialize-phase, fail-fast on the first missing required var (matches current behaviour).
- **GOTCHA (GOTCHA-SECRET-ERR — hard rule 7)**: mapping a figment `Error` → `ConfigError::Invalid{var, reason}` via `reason: e.to_string()` can echo the offending **value**. For secret-bearing fields (`oidc_client_secret`, `googlebooks_api_key`, `hardcover_api_token`) the reason must be value-free (name/shape only). Keep a secret-field set beside the env-name map; when the figment error's key path hits one, emit a scrubbed reason. The current loader never leaks a secret (secrets only ever `MissingVar`) — do not regress that.
- **GOTCHA**: map figment `Error` → `ConfigError::Invalid{var, reason}` using the error's key path through the **reverse** field→env-name map (so the var name, not the dotted field, surfaces).
- **VALIDATE**: `cargo build`; the `from_env_*` happy-path + defaults + `from_env_missing_database_url` (variant = `MissingVar`) tests pass.

### Task 7: Add validator attributes (range + cross-field) + the raw-string CSP deserializer + error aggregation

- **ACTION**: Replace the scattered `if`-range checks with `#[validate(...)]`; define the CSP raw-string deserializer; aggregate `validate()` errors into `ConfigError::Multiple`.
- **IMPLEMENT**:
  - Range: `#[validate(range(min=1, max=10))]` on enrichment/writeback `concurrency`; `range(min=1, max=500)` on opds `page_size`.
  - **`#[validate(nested)]`** on every sub-struct field of `Config` (`enrichment`, `cover`, `writeback`, `opds`, `security`) — **without it the sub-struct range checks silently do not run** (GOTCHA-NESTED, validation-that-doesn't-fire on a security surface).
  - Cross-field via `#[validate(schema(function = ...))]`: OPDS `enabled ⇒ public_url.is_some()`; HSTS `include_subdomains ⇒ behind_https`, `preload ⇒ include_subdomains`; OPDS `realm` excludes `"`.
  - **CSP injection — NOT a validator** (GOTCHA-CSPRAW): define `fn de_csp_endpoint<'de, D>(d) -> Result<Option<url::Url>, D::Error>` that deserializes the **raw string**, runs the char guard (`"` `;` CR LF) **before** `url::Url::parse`, then the scheme allowlist — mirroring `config.rs:682-707` order exactly. A `#[validate(custom)]` on the parsed `Url` sees the normalized `as_str()` (`"`→`%22`, CR/LF already rejected by parse with the wrong message) and silently passes (or emits the wrong message for) 2 of the 3 `security_report_endpoint_injection_chars_errors` cases (config.rs:1280-1294).
- **GOTCHA (GOTCHA-ERRNAME — traverse the tree, don't flatten)**: `validate()` returns a **nested** `ValidationErrors` tree — sub-struct failures are `ValidationErrorsKind::Struct` under the parent key (`enrichment` → `{concurrency}`), `Vec` fields are `…Kind::List` keyed by index. `field_errors()` only flattens the top level and will **miss** `enrichment.concurrency`. Walk the tree to assemble the dotted path (`parent.child`), THEN reverse-map it through the env-name map to the var (`REVERIE_ENRICHMENT_CONCURRENCY`). A naive `field_errors()` yields the wrong/generic var name.
- **GOTCHA (GOTCHA-AGG)**: collect **all** validate() errors (the tree, traversed) into `ConfigError::Multiple(Vec<ConfigError::Invalid{..}>)` so the validation phase delivers the ADR's promised aggregation. (Deserialize-phase / extract errors remain fail-fast, one at a time — figment stops at the first; state this so the "aggregated" acceptance criterion is honestly scoped to validation, not deserialize.)
- **VALIDATE**: `cargo build`; range/HSTS/OPDS/CSP-injection rejection tests pass with the env-var name in the message; a multi-violation input surfaces `Multiple`.

### Task 8: Migrate the test seam to `EnvProvider::from_pairs`

- **ACTION**: Rewrite the `#[cfg(test)]` helpers (`env_for`, `env_for_owned`, `with_overrides`, `without_keys`) to build pair slices fed to `EnvProvider::from_pairs`, merged onto `Serialized::defaults`. Update the external caller `auth/oidc.rs:193` (`Config::from_source(&|k| ...)`) to the pair form.
- **GOTCHA (GOTCHA-TESTSEAM)**: do **NOT** use figment's `Jail` — it mutates _real_ process env and holds a global lock, **serializing tests and regressing UNK-100's parallel-safe property** (the entire reason the seam exists). `from_pairs` is in-memory, no process env, parallel-safe.
- **GOTCHA (GOTCHA-TESTFIDELITY)**: inject **strings** through the provider's coerce path. Do NOT inject a pre-typed `Serialized(struct)` for overrides — that bypasses the parse/coerce path where the bugs live.
- **VALIDATE**: `cargo test -p reverie-api config::` — all behavioural tests green.

### Task 9: Preserve strict-bool + empty-as-unset semantics

- **ACTION**: Confirm/force the two coercion contracts.
- **GOTCHA (GOTCHA-BOOL)**: current contract rejects `"1"`/`"yes"` (UNK-106/110; tests `security_parse_bool_rejects_legacy_truthy`, `from_env_auto_migrate_invalid_value_rejected`). If figment/serde bool coercion accepts more than `true`/`false`, add `#[serde(deserialize_with = strict_bool)]` on every bool field (`auto_migrate`, `behind_https`, hsts\__, `_\_enabled`). Security-relevant gates.
- **GOTCHA (GOTCHA-EMPTY)**: `""` must equal unset (tests `from_env_empty_migration_url...`, every `Option`-secret `.filter(!is_empty)`). The EnvProvider's empty-drop (Task 5) covers this; guard with the existing tests.
- **VALIDATE**: `cargo test config::` — bool + empty tests green.

### Task 10: Emit the JSON Schema artifact (committed + CI-drift-checked)

- **ACTION**: Add a deterministic `reverie-api print-config-schema` subcommand that writes `schema_for!(Config)` as pretty JSON to stdout; commit the rendered output as `backend/config.schema.json`; gate drift in CI.
- **IMPLEMENT (decide the home — don't leave it ambiguous)**: mirror the existing `.sqlx` cache pattern in `backend/CLAUDE.md` — the schema is a **committed generated artifact**, regenerated by the subcommand, and CI runs a `--check` (regenerate to a temp file, `diff` against committed; fail on drift) so a stale schema can't merge. A pure-CLI-only artifact is rejected: a stale committed file is exactly the drift class this refactor exists to kill. UNK-370's config-ref generator consumes `backend/config.schema.json`.
- **GOTCHA (security-acceptance, not cosmetic)**: the schema-emit test asserts **no default value for any secret-bearing field** (GOTCHA-SECRET) — `port` has default `3000`; `oidc_client_secret` / `*_api_key` / `*_api_token` have **no** `default`.
- **VALIDATE**: `cargo test` schema-emit + no-secret-default test; `reverie-api print-config-schema | diff - backend/config.schema.json` clean; CI drift-check job added.

### Task 11: Rewrite the staging-coverage test against the declarative map

- **ACTION**: Replace `staging_runtime_example_keys_are_read_by_config` (config.rs:869-912, which scans `get("KEY")`/`get, "KEY"` text — now gone).
- **IMPLEMENT**: new test asserts every `KEY=` in `docker/staging.env.runtime.example` is a key in the EnvProvider env-name map (`example keys ⊆ map keys`). Same one-way UNK-250 guarantee, now reading the structured map instead of source text.
- **VALIDATE**: `cargo test staging` green; flip a key in the example to confirm it fails.

### Task 12: UPDATE `backend/CLAUDE.md` (APPROVAL GATE)

- **ACTION**: Edit line ~168 (cascade rule: "Resolve cascade once in `config.rs`" → reference the figment cascade in the EnvProvider + `config/` module) and line ~335 (tree: `config.rs # Environment-based configuration` → `config/ # Declarative config module (figment+serde+validator+schemars)`).
- **GOTCHA**: CLAUDE.md edits carry a **broad-blast-radius approval gate** — surface the proposed text to the user and get explicit approval before writing (per global feedback rule). Do not blow through.
- **VALIDATE**: user approves the diff.

### Task 13: Purge the debt entry (on merge readiness)

- **ACTION**: Delete `debt/2026-06-09-imperative-config-reader.md`; remove its line from `debt/README.md`. Purge commit names this PR.
- **GOTCHA**: only when the lift condition is fully met (no `get("KEY")` ladder remains, schema emits). Verify against the debt file's lift-when clause.
- **VALIDATE**: `rg -l 'imperative-config-reader' debt/` returns nothing.

### Task 14: Split `config.rs` → `backend/src/config/` module (CLOSING MOVE)

- **ACTION**: Mechanical move: `mod.rs` (re-exports + `Config` + `ConfigError` + pipeline), `provider.rs` (EnvProvider + env-name map), one file per sub-struct (`enrichment.rs`, `cover.rs`, `writeback.rs`, `opds.rs`, `security.rs`).
- **GOTCHA**: `mod.rs` must `pub use` every type so `crate::config::OpdsConfig` / `crate::config::SecurityConfig` (referenced in orchestrators, headers.rs, test_support) keep resolving. No public-path changes.
- **GOTCHA**: do this **last**, after behaviour is green — a no-behaviour-change move keeps the review diff legible (ADR closing move).
- **VALIDATE**: `cargo build && cargo test -p reverie-api` — full suite green; `cargo clippy --workspace --all-targets --locked -- -D warnings`.

---

## Testing Strategy

### Tests that MUST stay green (behaviour contract — config.rs:914-1372)

Defaults (`from_env_with_defaults`), log cascade (3 tests), migration gate (5
tests incl. `from_env_missing_migration_url_ok_when_auto_migrate_off`,
`from_env_auto_migrate_true_requires_migration_url`,
`from_env_empty_migration_url_treated_as_none...`), ingestion/format-priority,
OPDS (page-size range + boundary, realm quote, public_url-required), security
(HSTS preconditions, CSP injection ×3 forms, bad scheme, malformed URL,
strict-bool legacy-truthy), port/cleanup parse errors, `user_agent`.

### New tests

| Test                                         | Validates                                            |
| -------------------------------------------- | ---------------------------------------------------- |
| `env_provider_maps_flat_and_nested_key`      | EnvProvider remap (GOTCHA-SPLIT)                     |
| `env_provider_drops_empty_as_unset`          | GOTCHA-EMPTY                                         |
| `config_schema_has_no_secret_defaults`       | GOTCHA-SECRET (security acceptance)                  |
| `nested_range_validation_fires`              | GOTCHA-NESTED (else silent pass)                     |
| `missing_database_url_is_MissingVar_variant` | GOTCHA-REQUIRED (variant, not substring)             |
| `csp_endpoint_raw_quote_and_crlf_rejected`   | GOTCHA-CSPRAW (the 2 cases a validator-on-Url drops) |
| `secret_field_deser_error_has_no_value`      | GOTCHA-SECRET-ERR (hard rule 7)                      |
| `multi_violation_yields_Multiple`            | GOTCHA-AGG (validation aggregation)                  |
| `nested_validate_error_names_env_var`        | GOTCHA-ERRNAME (tree traversal, not flatten)         |
| rewritten `staging_*_keys_in_map`            | Task 11                                              |

### Edge Cases Checklist

- [ ] `auto_migrate=false` + `DATABASE_URL_MIGRATION` set → `migration_database_url` is `None` (GOTCHA-MIGGATE)
- [ ] `REVERIE_BEHIND_HTTPS=1` rejected (GOTCHA-BOOL)
- [ ] empty `REVERIE_GOOGLEBOOKS_API_KEY` → `None` (GOTCHA-EMPTY)
- [ ] `db_max_connections` does not get split into `db.max.connections` (GOTCHA-SPLIT)
- [ ] validator error for `enrichment.concurrency=11` names `REVERIE_ENRICHMENT_CONCURRENCY` (GOTCHA-ERRNAME, via tree traversal)
- [ ] unset `DATABASE_URL` → `MissingVar` variant, not `Invalid` (GOTCHA-REQUIRED)
- [ ] `REVERIE_CSP_REPORT_ENDPOINT=https://x/"q` and `…/\r\n…` both ERROR with "must not contain" (GOTCHA-CSPRAW)
- [ ] deserialize failure on a secret field surfaces no value (GOTCHA-SECRET-ERR)
- [ ] emitted schema: `port` default present, `oidc_client_secret` default absent

---

## Validation Commands

### Level 1: STATIC_ANALYSIS

```bash
cd backend && cargo fmt --all -- --check && cargo clippy --workspace --all-targets --locked -- -D warnings
```

**EXPECT**: exit 0.

### Level 2: UNIT_TESTS

```bash
cd backend && cargo test -p reverie-api config::
```

**EXPECT**: all config tests pass.

### Level 3: FULL_SUITE

```bash
cd backend && cargo test -p reverie-api && cargo build -p reverie-api
```

**EXPECT**: full suite green, build succeeds.

### Level 4: SUPPLY-CHAIN

```bash
cd backend && cargo audit
```

**EXPECT**: figment, validator, schemars introduce no advisory (per ADR gate).

### Level 5: SQLX CACHE (only if any query touched — unlikely here)

```bash
cd backend && cargo sqlx prepare --check -- --tests
```

### Level 6: SECURITY REVIEW (hard rule 6 — explicit, before merge)

- [ ] **Secrets name/shape only — schema** — emitted JSON Schema carries no `default` value for any secret-bearing field (concrete schema-inspection step, not "tests pass").
- [ ] **Secrets name/shape only — errors** — no figment/serde error maps a secret field's value into `ConfigError` (GOTCHA-SECRET-ERR, hard rule 7).
- [ ] **CSP-report-endpoint injection check** preserved, on the **raw string before parse** (`"` `;` CR LF + scheme allowlist) — NOT a validator on the parsed `Url` (GOTCHA-CSPRAW) — verified by the 3 injection-form tests.
- [ ] **Role-scoped DSN separation** — ingestion DSN fallback intact; no DSN cross-contamination.
- [ ] **`auto_migrate` post-deserialize null-out** (GOTCHA-MIGGATE) — security-load-bearing mutation, the migration-ADR credential-in-memory trap. Verified by the 5 migration tests.
- [ ] Answer in task summary: "will this stand up to security review?"

---

## Acceptance Criteria

- [ ] Config loads via figment; no `env::var`/`get("KEY")` ladder remains in the loader.
- [ ] Each field's env binding + default declared as structured metadata (EnvProvider map + `Default` impl).
- [ ] validator-based validation; var-named errors via tree traversal; `validate()` errors aggregate into `ConfigError::Multiple` (deserialize-phase stays fail-fast).
- [ ] `MissingVar` vs `Invalid` distinction preserved (unset required var → `MissingVar` variant).
- [ ] CSP injection guard runs on the raw string at deserialize time (not a validator on the parsed `Url`).
- [ ] No secret value reaches `ConfigError` or the emitted schema (name/shape only).
- [ ] schemars JSON Schema emitted to committed `backend/config.schema.json`, CI drift-checked; unblocks UNK-370 config-ref.
- [ ] All behavioural config tests green; staging-coverage test rewritten.
- [ ] Security review (Level 6) complete.
- [ ] `backend/CLAUDE.md` lines 168/335 updated (with approval).
- [ ] On merge: `debt/2026-06-09-imperative-config-reader.md` purged + README line removed.
- [ ] `config.rs` → `config/` module; `crate::config::*` paths unchanged.

---

## Risks and Mitigations

| Risk                                                                            | Likelihood | Impact     | Mitigation                                                                                                                                                                                                              |
| ------------------------------------------------------------------------------- | ---------- | ---------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| figment env→nested mapping needs an outsized custom adapter                     | MED        | HIGH       | The EnvProvider is exactly that adapter, kept thin (string map). **ADR revisit trigger**: if it balloons, reconsider option B (light path) before committing the loader rewrite — raise to user, do not silently pivot. |
| `auto_migrate` gate forgotten post-deserialize                                  | MED        | HIGH (sec) | GOTCHA-MIGGATE called out; 5 migration tests guard; Level-6 checkbox                                                                                                                                                    |
| `Jail` chosen for tests → parallel regression                                   | LOW        | HIGH       | GOTCHA-TESTSEAM forbids Jail; `from_pairs` is the seam                                                                                                                                                                  |
| CSP guard placed on parsed `Url` → silent injection-guard bypass + broken tests | MED        | HIGH (sec) | GOTCHA-CSPRAW: raw-string `deserialize_with`, not a validator; 3 injection tests guard                                                                                                                                  |
| Required-field empty-default erases `MissingVar`/`Invalid` distinction          | MED        | HIGH       | GOTCHA-REQUIRED: required fields out of defaults; variant-asserting test                                                                                                                                                |
| Secret value leaks via figment error → `reason`                                 | MED        | HIGH (sec) | GOTCHA-SECRET-ERR: scrub reason for secret-field key paths                                                                                                                                                              |
| schemars 0.8/1.0 attribute-syntax mismatch                                      | MED        | MED        | GOTCHA-VERSION: pin one major, write attrs against it, ignore Context7 blend                                                                                                                                            |
| validator errors name Rust fields not env vars (nested tree)                    | HIGH       | MED        | GOTCHA-ERRNAME: traverse tree to dotted path, then reverse-map                                                                                                                                                          |
| "Aggregated errors" claimed but unrealized                                      | MED        | LOW        | GOTCHA-AGG: `ConfigError::Multiple`; deserialize-phase honestly scoped as fail-fast                                                                                                                                     |
| Strict-bool relaxed by serde coercion                                           | MED        | MED (sec)  | GOTCHA-BOOL: custom `deserialize_with` if needed                                                                                                                                                                        |
| Schema artifact stale (committed but not regenerated)                           | MED        | MED        | Task 10: CI `--check` drift gate (`.sqlx`-cache pattern)                                                                                                                                                                |
| Struct-literal sites break on field changes                                     | LOW        | MED        | Keep field shape; derives additive; ~15 sites verified compile                                                                                                                                                          |

---

## Notes

- **One mid-execution pivot max** (global rule): if the EnvProvider proves
  unworkable, stop, commit-durable, raise the ADR revisit trigger (option B), end
  session — do not stack pivots.
- **Approval gates**: stop-and-show after the plan, and again before the
  `backend/CLAUDE.md` edit (Task 12).
- **Branch**: `refactor/unk-375-declarative-config-stack` (our `refactor/` prefix;
  Linear's generated `feature/...` name diverges — use `Closes UNK-375` in the PR
  body per hard rule 9).
- **time crate**, not chrono (project rule) — no date handling here anyway.
- **Confidence**: validator/schemars version-shape (GOTCHA-VERSION) and figment
  bool/empty coercion (GOTCHA-BOOL/EMPTY) are the two empirical unknowns to settle
  early at impl with a throwaway spike before committing the full struct surface.

```

```
