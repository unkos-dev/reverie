---
status: "{proposed | accepted | rejected | deprecated | superseded by [title](YYYY-MM-DD-title.md)}"
date: { YYYY-MM-DD }
decision-makers: "{everyone who owns the decision}"
consulted: "{everyone whose expertise was sought}"
informed: "{everyone kept up-to-date}"
---

# {short title representing the decision}

## Context and Problem Statement

{Background and the problem, framed as a question where possible. Link
issues/tickets/prior ADRs. Enough context that a first-time reader needs no
follow-up.}

<!-- Optional — remove if unused -->

## Decision Drivers

- {constraint, requirement, or force}

## Considered Options

- {option 1}
- {option 2}

## Decision Outcome

Chosen option: "{option}", because {justification — reference drivers and tradeoffs}.

### Consequences

- Good, because {positive consequence}
- Bad, because {negative consequence}
- Neutral, because {consequence}

### Confirmation

{How compliance with this decision is or was confirmed — the load-bearing
invariant(s), not a build checklist. One to three lines, e.g. "Enforced by
clippy lint X" or "No raw SQL outside `src/db/`." Remove if none.}

<!-- Optional — remove if unused -->

## Pros and Cons of the Options

### {option 1}

- Good, because {argument}
- Neutral, because {argument}
- Bad, because {argument}

<!-- Optional — remove if unused -->

## More Information

{Cross-links to related ADRs, revisit triggers, follow-up notes. Implementation
work is tracked in prp-plan output (`.claude/PRPs/plans/`), not here.}
