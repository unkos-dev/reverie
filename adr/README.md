# Architecture Decision Records (ADR)

An Architecture Decision Record (ADR) captures an important architecture decision along with its context and consequences.

## Conventions

- Directory: `adr`
- Naming:
  - Use date-prefixed files: `YYYY-MM-DD-choose-database.md`
  - If the repo already uses slug-only names, keep that: `choose-database.md`
- Status values: `proposed`, `accepted`, `rejected`, `deprecated`, `superseded`

## Workflow

- Create a new ADR as `proposed`.
- Discuss and iterate.
- When the team commits: mark it `accepted` (or `rejected`).
- If replaced later: create a new ADR and mark the old one `superseded` with a link.

## ADRs

- [Adopt architecture decision records](2026-04-30-adopt-architecture-decision-records.md) (accepted, 2026-04-30)
- [Strict lint policy: clippy pedantic + ESLint strict-tier](2026-05-03-strict-lint-policy.md) (proposed, 2026-05-03)
- [Greptile AI code review: 4-week trial](2026-05-04-greptile-trial.md) (accepted, 2026-05-04)
- [Replace eslint-plugin-react with @eslint-react/eslint-plugin](2026-05-04-replace-eslint-plugin-react.md) (accepted, 2026-05-04)
- [Single-image distribution with backend-served frontend and central CSP enforcement](2026-05-05-single-image-distribution-central-csp.md) (proposed, 2026-05-05)
- [CodeRabbit AI code review: parallel trial alongside Greptile](2026-05-07-coderabbit-parallel-trial.md) (accepted, 2026-05-07)
- [Tiered comment policy for an OSS-released codebase](2026-05-08-tiered-comment-policy.md) (accepted, 2026-05-08)
- [Adopt tower-sessions-sqlx-store for Postgres-backed sessions](2026-05-08-tower-sessions-sqlx-store.md) (proposed, 2026-05-08)
- [Decouple staging Docker image publication from semver release tags](2026-05-12-decouple-staging-image-from-semver-releases.md) (superseded by [Per-architecture native runners with manifest-list merge](2026-05-12-platform-matrix-via-native-runners.md), 2026-05-12)
- [Per-architecture native runners with manifest-list merge for Docker publish](2026-05-12-platform-matrix-via-native-runners.md) (accepted, 2026-05-12)
- [GHA build cache + cargo-chef Dockerfile layering for Docker publish](2026-05-13-image-build-cache.md) (accepted, 2026-05-13)
- [Outbound HTTP clients in Reverie must send an explicit User-Agent](2026-05-18-outbound-http-user-agent.md) (proposed, 2026-05-18)
- [Adopt `impeccable` as the frontend design anti-pattern detector](2026-05-21-impeccable-adoption.md) (proposed, 2026-05-21)
- [Frontend docstring linting via `eslint-plugin-jsdoc`](2026-05-22-frontend-docstring-tooling.md) (accepted, 2026-05-22)
- [JSON API conventions for Reverie's browser-facing REST surface](2026-05-22-json-api-conventions.md) (accepted, 2026-05-22)
- [Frontend data-layer dependencies for Step 11](2026-05-22-frontend-data-layer-deps.md) (accepted, 2026-05-22)
- [Backend auxiliary crates for Step 11](2026-05-22-backend-aux-crates.md) (accepted, 2026-05-22)
- [Persist operator-tunable settings to database with live reload](2026-05-26-persisted-settings.md) (accepted, 2026-05-26)
- [Auto-migrate database on startup with all-or-nothing batch transactions](2026-05-26-auto-migration-on-startup.md) (accepted, 2026-05-26)
- [Reconcile `validation_status` vocabulary and introduce a typed `ValidationStatus` enum](2026-05-28-validation-status-vocabulary.md) (accepted, 2026-05-28)
