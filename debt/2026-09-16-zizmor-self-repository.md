---
severity: low
surfaces: [developer, security, ci]
adopted: 2026-09-16
adopted-because: the pinned workflow linter rejects GitHub's self-repository syntax
lift-when-class: dep-unblocks
lift-when: a released version of the pinned workflow linter parses $/ references and passes the workflow tree after every workflow self-reference and the README example intended for copying are migrated
---

# Zizmor self-repository audit disabled

Reverie keeps 36 executable self-references in the workspace-relative `./` form because `actionlint` 1.7.12 rejects
GitHub's `$/` self-repository syntax. Zizmor's `self-repository` audit is disabled so later zizmor releases can retain
their other audits without dropping `actionlint` checks for expressions, workflow contracts, and inline shell scripts.

While the audit is disabled, zizmor cannot report a local action loaded from changed runner workspace state or a
self-reference that prevents GitHub's SHA-pin enforcement. Current workflow checkouts do not replace `.github/actions`,
and every third-party action is pinned to a full commit SHA.

Lift this entry by replacing every `./.github/actions/...` and `./.github/workflows/...` reference in the workflow tree
and the example intended for copying in `.github/workflows/README.md` with the equivalent `$/` reference, then remove
the zizmor rule disable. Verify the migrated tree with `just infra::actionlint` and `just infra::zizmor`.
