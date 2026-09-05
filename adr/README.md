# Architecture Decision Records (ADR)

An Architecture Decision Record (ADR) captures an important architecture decision along with its context and consequences.

## Conventions

- Directory: `adr`
- Naming: `YYYY-MM-DD-short-kebab-slug.md` (date-prefixed, no numeric prefixes)
- Shape: canonical [MADR 4.0](https://adr.github.io/madr/). Copy [TEMPLATE.md](TEMPLATE.md).
- Sections (in order): Context and Problem Statement, Decision Drivers (optional), Considered Options, Decision Outcome (Consequences and Confirmation), Pros and Cons of the Options (optional), More Information (optional)
- Status values: `proposed`, `accepted`, `rejected`, `deprecated`, `superseded`
- Supersession (header fields, not status prose): the replacement ADR carries `supersedes: ["superseded/<old>.md"]`; the replaced ADR carries `status: superseded` + `superseded-by: ["../<new>.md"]` and is moved into `adr/superseded/`. Paths are relative to the file.
- **Not an implementation plan.** ADRs record the decision and rationale. Build steps, file lists, and verification checklists belong in private `/plans/` artifacts. See [AGENTS.md](AGENTS.md).

## Workflow

- Create a new ADR as `proposed`.
- Discuss and iterate.
- When the team commits: mark it `accepted` (or `rejected`).
- If replaced later: create the replacement with `supersedes: ["superseded/<old>.md"]`; on the old ADR set `status: superseded` + `superseded-by: ["../<new>.md"]` and `git mv` it into `adr/superseded/`.

## ADRs

- [Adopt tower-sessions-sqlx-store for Postgres-backed sessions](superseded/2026-05-08-tower-sessions-sqlx-store.md) (superseded by [First-party session layer on tower-sessions core](2026-06-04-first-party-session-layer.md), 2026-06-04)
- [Frontend docstring linting via `eslint-plugin-jsdoc`](superseded/2026-05-22-frontend-docstring-tooling.md) (superseded by [Adopt oxlint, replacing the ESLint toolchain](2026-06-27-adopt-oxlint-toolchain.md), 2026-06-27)
- [JSON API conventions for Reverie's browser-facing REST surface](2026-05-22-json-api-conventions.md) (accepted, 2026-05-22)
- [Persist operator-tunable settings to database with live reload](2026-05-26-persisted-settings.md) (accepted, 2026-05-26)
- [Auto-migrate database on startup with all-or-nothing batch transactions](superseded/2026-05-26-auto-migration-on-startup.md) (superseded by [Database migration model: hybrid entrypoints, least-privilege role, all-or-nothing batch](2026-06-02-hybrid-migration-entrypoints-and-role.md), 2026-05-26)
- [Reconcile `validation_status` vocabulary and introduce a typed `ValidationStatus` enum](2026-05-28-validation-status-vocabulary.md) (accepted, 2026-05-28)
- [Database migration model: hybrid entrypoints, least-privilege role, all-or-nothing batch](2026-06-02-hybrid-migration-entrypoints-and-role.md) (accepted, 2026-06-02)
- [First-party session layer on tower-sessions core; drop axum-login and tower-sessions-sqlx-store](2026-06-04-first-party-session-layer.md) (accepted, 2026-06-04)
- [Accessibility review process: automated axe gate + manual audit cadence](superseded/2026-06-05-accessibility-review-process.md) (superseded by [Accessibility gate and render-verification on the Playwright stack](2026-07-13-a11y-gate-on-playwright.md), 2026-07-13)
- [Connection pooling: in-process sqlx PgPool as the sole pooling layer](2026-06-08-connection-pooling.md) (accepted, 2026-06-08)
- [Durable, crash-safe state in Postgres via atomic transactions](2026-06-08-postgres-backed-crash-safe-state.md) (accepted, 2026-06-08)
- [Scale stance: stateless application, operator-enabled HA, no first-party distributed infrastructure](2026-06-08-scale-stance-stateless-enable-not-own.md) (accepted, 2026-06-08)
- [Standards-first integrations: expose open interfaces, bundle no adjacent services](2026-06-08-standards-first-integrations.md) (accepted, 2026-06-08)
- [Durable job queue: Postgres-backed, SKIP-LOCKED, crash-only via restart-bounded reclaim](2026-06-08-durable-job-queue-crash-only.md) (accepted, 2026-06-08)
- [API versioning via URL path and OpenAPI 3.1 as the generated API contract](2026-06-08-api-versioning-openapi.md) (accepted, 2026-06-08)
- [No unbounded queries: keyset pagination as the default list contract](2026-06-08-keyset-pagination-list-contract.md) (accepted, 2026-06-08)
- [Adopt a declarative configuration stack (figment + serde + validator + schemars)](2026-06-09-declarative-config-stack.md) (accepted, 2026-06-09)
- [Rasterize SVG-declared EPUB covers to PNG via resvg](2026-06-13-svg-cover-rasterization.md) (accepted, 2026-06-13)
- [Cover cache headers, ingest pre-warm, and JPEG thumbnails](2026-06-14-cover-cache-headers-and-thumbnail-encoding.md) (accepted, 2026-06-14)
- [Radix-generated three-tier dual-theme color tokens](2026-06-18-radix-three-tier-dual-theme-tokens.md) (accepted, 2026-06-18)
- [A single danger hue amends the no-hue-states policy](2026-06-18-single-danger-hue-amends-no-hue-philosophy.md) (accepted, 2026-06-18)
- [Authentication and identity model: unified identity with pluggable providers](2026-06-23-auth-identity-pluggable-providers.md) (accepted, 2026-06-23)
- [API authorization model: orthogonal scope, role, and ownership axes enforced server-side](2026-06-23-api-authorization-orthogonal-axes.md) (accepted, 2026-06-23)
- [Adopt oxlint, replacing the ESLint toolchain](2026-06-27-adopt-oxlint-toolchain.md) (accepted, 2026-06-27)
- [Adopt oxfmt, replacing Prettier](2026-06-28-adopt-oxfmt-formatter.md) (accepted, 2026-06-28)
- [Adopt lefthook, replacing husky and lint-staged](2026-06-28-adopt-lefthook-git-hooks.md) (accepted, 2026-06-28)
- [Adopt the Vite+ (vp) monorepo toolchain](2026-06-30-adopt-vite-plus-monorepo-toolchain.md) (accepted, 2026-06-30)
- [Password strength policy: zxcvbn floor plus a fail-open HIBP breach check](2026-06-30-password-policy-hibp-zxcvbn.md) (accepted, 2026-06-30)
- [Lint suppressions must be self-purging: #[expect] with a reason, never #[allow]](2026-07-04-expect-over-allow-lint-suppressions.md) (accepted, 2026-07-04)
- [Library data-grid stack: two-way bake-off behind a local adapter](2026-07-04-library-grid-stack-bakeoff.md) (accepted, 2026-07-04)
- [Multi-column sort stack on the keyset list contract](2026-07-07-multi-column-sort-stack.md) (accepted, 2026-07-07)
- [Typed filter grammar on list endpoints](2026-07-07-typed-filter-grammar-list-endpoints.md) (accepted, 2026-07-07)
- [Single filter home in the library right rail](2026-07-10-library-filter-home-right-rail.md) (accepted, 2026-07-10)
- [Accessibility gate and render-verification on the Playwright stack](2026-07-13-a11y-gate-on-playwright.md) (accepted, 2026-07-13)
- [Code-scanning ingestion policy: scan everything, ingest what is actionable](2026-07-23-code-scanning-ingestion-policy.md) (accepted, 2026-07-23)
- [Remote Rust build cache on object storage, alongside the tarball cache](2026-07-26-remote-build-cache-on-r2.md) (accepted, 2026-07-26)
- [Match-time accent folding via unaccent expression indexes](2026-08-02-match-time-accent-folding.md) (accepted, 2026-08-02)
- [Package ingress: default-deny controls, never per-package allowances](2026-08-03-package-ingress-default-deny.md) (accepted, 2026-08-03)
- [chrono is the first-party datetime crate](2026-08-05-first-party-datetime-crate.md) (accepted, 2026-08-05)
- [Library sort is a per-user preference, resolved client-side, never URL state](2026-08-08-library-sort-per-user-preference.md) (accepted, 2026-08-08)
