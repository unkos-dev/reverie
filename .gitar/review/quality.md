# Review quality

Review the current pull request head and trace findings to reachable behaviour. Identify the affected code, triggering
conditions and concrete consequence. Distinguish verified behaviour from inference and state material uncertainty.
Reassess earlier findings against the current diff; do not repeat resolved findings or formatter output.

Report actionable findings with a concrete correctness, security, interoperability, accessibility, performance or
maintenance consequence. Omit cosmetic nits, preferred idioms, speculative problems and generic best-practice advice
without demonstrated applicability. Consolidate findings with the same root cause. A review with no findings is valid.

Assess migrations and changed state interpretation against existing data as well as empty databases. Check whether data
migrations preserve the required behaviour for existing records. Evaluate whether tests demonstrate the claimed
behaviour and meaningful failure cases; passing CI alone does not establish correctness.

## Standards and best practice

Check changed behaviour against applicable industry standards and authoritative best-practice sources. RFCs, OWASP
guidance and W3C standards are examples, not an exhaustive list. Verify the source and cite its title, version or date,
specific section and link. Explain why it applies to the protocol, feature or boundary under review and identify the
concrete consequence of the deviation.

Distinguish normative requirements such as MUST from recommendations and optional techniques. Report a best-practice
recommendation as such rather than presenting it as mandatory compliance. Check applicable specifications and accepted
decisions; surface conflicts for a project decision rather than silently overriding either source. If a reference cannot
be verified, state the uncertainty instead of claiming established non-compliance.

## Unnecessary complexity

Flag complexity when a materially simpler alternative preserves the required behaviour, security properties,
compatibility and relevant performance characteristics. Identify the unnecessary complexity, describe the alternative
and explain its trade-offs. Check applicable specifications and accepted decisions before recommending it. Distinguish
actionable simplifications from stylistic preferences; fewer lines alone do not establish a better solution.

## Dependency updates

For dependency updates, including Renovate pull requests, assess compatibility with actual call sites and configuration.
Check breaking API changes, runtime and compiler requirements, peer dependencies, changed defaults and required
migration steps against authoritative upstream release notes and documentation. Identify behaviour changes that existing
tests may miss and explain their effect on this project.

Do not repeat changelogs, speculate that an upgrade might break something, or duplicate dependency-scanner alerts
without additional project-specific evidence. Routine version bumps may require no findings. Review permission does not
authorise automatic fixes, approval or merging.
