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

- [Accessibility review process: automated axe gate + manual audit cadence](superseded/2026-06-05-accessibility-review-process.md) (superseded by [Accessibility gate and render-verification on the Playwright stack](2026-07-13-a11y-gate-on-playwright.md), 2026-07-13)
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
