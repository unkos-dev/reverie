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

- [chrono is the first-party datetime crate](2026-08-05-first-party-datetime-crate.md) (accepted, 2026-08-05)
- [Library sort is a per-user preference, resolved client-side, never URL state](2026-08-08-library-sort-per-user-preference.md) (accepted, 2026-08-08)
