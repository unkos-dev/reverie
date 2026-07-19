# ADR Authoring Rules

Architecture Decision Records, MADR 4.0 shape. See [README.md](README.md) for
the index + lifecycle, [TEMPLATE.md](TEMPLATE.md) for the canonical skeleton.

## An ADR records a decision, not a plan

ADRs capture **what was decided and why**: context, drivers, options, outcome,
consequences. They do **not** double as implementation plans.

- No `Implementation Plan`, `Verification`, file-by-file task lists, or build
  checklists. Those belong in implementation-plan artifacts, not the ADR.
- Decision-bearing invariants worth keeping ("no raw SQL outside the
  data-access layer") go in `### Confirmation` under Decision Outcome: one to
  three lines, not a checklist.
- Revisit triggers fold into `## More Information`.

Follow `TEMPLATE.md`: an ADR carries no `Implementation Plan` or `Verification`
section.

## Canonical sections (in order)

Front matter: `status`, `date`, `supersedes` (paths replaced; `[]` if none), `decision-makers`, `consulted`, `informed`. A superseded ADR also carries `superseded-by` and moves to `superseded/`.

Canonical field values:

- `decision-makers: "John Unkovich"`: always quoted full name, no variations (`john`, `junkovich`)
- `consulted: []`: use an empty list when no outside expertise was sought
- `informed: "Reverie contributors"`: or a named list when relevant

1. `## Context and Problem Statement`
2. `## Decision Drivers` (optional)
3. `## Considered Options`
4. `## Decision Outcome` → `### Consequences` → `### Confirmation`
5. `## Pros and Cons of the Options` (optional)
6. `## More Information` (optional)

Statuses: `proposed`, `accepted`, `rejected`, `deprecated`, `superseded`.

**Status lifecycle:** an ADR shipping with its implementation PR flips `proposed` to `accepted`
in that same PR; the README index entry updates in lockstep.
