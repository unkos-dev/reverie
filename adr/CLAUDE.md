# ADRs — Authoring Rules

Architecture Decision Records, MADR 4.0 shape. See [README.md](README.md) for
the index + lifecycle, [TEMPLATE.md](TEMPLATE.md) for the canonical skeleton.

## An ADR records a decision, not a plan

ADRs capture **what was decided and why** — context, drivers, options, outcome,
consequences. They do **not** double as implementation plans.

- No `Implementation Plan`, `Verification`, file-by-file task lists, or build
  checklists. Those belong in `prp-plan` output (`.claude/PRPs/plans/`).
- Decision-bearing invariants worth keeping ("no raw SQL outside the
  data-access layer") go in `### Confirmation` under Decision Outcome — one to
  three lines, not a checklist.
- Revisit triggers fold into `## More Information`.

The bundled `adr` skill (vendored from `vercel/ai`) frames ADRs as "executable
specifications" carrying an implementation plan. This repo overrides that:
follow TEMPLATE.md, drop the skill's `Implementation Plan` / `Verification`
sections.

## Canonical sections (in order)

Front matter: `status`, `date`, `decision-makers`, `consulted`, `informed`.

1. `## Context and Problem Statement`
2. `## Decision Drivers` (optional)
3. `## Considered Options`
4. `## Decision Outcome` → `### Consequences` → `### Confirmation`
5. `## Pros and Cons of the Options` (optional)
6. `## More Information` (optional)

Statuses: `proposed`, `accepted`, `rejected`, `deprecated`, `superseded`.
