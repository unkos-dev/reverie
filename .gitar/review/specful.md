# Specful review context

Review changed behaviour against the committed Specful corpus as well as the applicable `AGENTS.md` instructions. Start
at `docs/specs/index.md` and `.specful/generated/catalog.json` to locate the relevant Requirements, Designs and ADRs.
Read the documents themselves and follow their requirement, design and decision references across affected scopes before
judging compliance.

Requirements define obligations, Designs describe how a subject works, and accepted ADRs record decisions. Check
document status and supersession before relying on a decision. When the pull request changes a specification, compare
the base and proposed versions and assess whether the implementation and tests satisfy the proposed contract. Do not
treat a proposed contract change as an already accepted decision.

For each specification mismatch, cite the artifact ID, section and relevant obligation alongside the affected code and
observable consequence. Distinguish implementation defects from missing, ambiguous or conflicting documentation. Report
unavailable context as a review limitation; do not invent requirements or claim compliance with documents you have not
read. A schema validation pass establishes structure and references, not behavioural correctness.
