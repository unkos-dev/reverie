---
status: active
severity: low
surfaces: [ci, security]
adopted: 2026-05-30
adopted-because: PR #370 (Greptile flagged the `typos` install-action step as lacking a SHA pin); deferred because it is a whole-workflow posture, not a one-step fix
lift-when-class: internal-refactor
lift-when: all third-party GitHub Actions across every workflow pinned to full commit SHAs (Linear ticket pending — workspace currently at the free-tier issue cap)
lifted: ~
superseded-by: ~
---

# GitHub Actions referenced by mutable tags, not commit SHAs

## Constraint

Every third-party GitHub Action across the repo's workflows is referenced
by a mutable tag (`@v6`, `@v2`, `@stable`, …) rather than a pinned commit
SHA. In `.github/workflows/ci.yml` alone:

- `actions/checkout@v6`
- `actions/cache@v5`
- `actions/setup-node@v6`
- `dorny/paths-filter@v4`
- `Swatinem/rust-cache@v2`
- `dtolnay/rust-toolchain@stable`
- `taiki-e/install-action@v2` (used for both `cargo-nextest` and `typos`)

The other workflows (`codeql.yml`, `docker-publish.yml`, `docs.yml`,
`label.yml`, `release-please.yml`) carry the same posture.

A mutable tag is a moving target: the tag owner (or anyone who compromises
the action repo) can force-push it to point at different code, and a
workflow run picks up the new code with no integrity alarm. Pinning to a
full commit SHA freezes the referenced tree.

## Workaround

Accept mutable tags as the standing convention. This was the existing
state before PR #370; that PR added the `repo-lint` job consistent with it
rather than diverging.

Note the risk is narrower than "no integrity checking anywhere":

- `taiki-e/install-action` **does** verify each downloaded tool binary
  (e.g. `typos`) against a SHA256 hash committed in its manifest
  (`manifests/typos.json` carries per-platform `hash` fields). The binary
  is integrity-checked; what is unpinned is the action's own code.
- The `shellcheck` and `hadolint` steps in `repo-lint` pin their release
  artefacts by SHA256 directly, so those two tools are covered at the
  binary layer regardless of action pinning.

The uncovered surface is therefore the **action code itself** — the
JavaScript/composite steps that run with access to the workflow token —
across all `uses:` references.

## Why this isn't the right shape

OpenSSF Scorecard's "Pinned-Dependencies" check and GitHub's own
hardening guidance both call for SHA-pinned actions. For an open-source,
self-hosted project whose threat model is a multi-user exposed instance,
the CI supply chain is part of the attack surface: a compromised action
runs with the workflow's `GITHUB_TOKEN` and can tamper with build output,
publish artefacts, or exfiltrate secrets.

The reason it is debt and not simply "do it now" is scope: pinning only
the one step Greptile flagged (`typos`) would make `ci.yml` _less_
internally consistent, not more. The correct fix is a single deliberate
repo-wide pass across all workflows, paired with a SHA-aware update tool
(Renovate/Dependabot both bump pinned SHAs while preserving a `# vX.Y.Z`
comment) so the pins don't rot into staleness.

## Lift conditions

A PR that:

1. Pins every third-party action in every workflow to a full commit SHA,
   each with a trailing `# vX.Y.Z` comment for readability.
2. Confirms the dependency-update tool (Renovate/Dependabot) is configured
   to bump SHA-pinned actions, so the pins stay current.

When that lands:

1. Flip this entry to `status: lifted`, set `lifted`, set `superseded-by`.
2. Move it from "Active" to "Lifted" in `debt/README.md`.

## Related

- PR #370 — surfaced the finding (added `repo-lint`; left action pinning
  out of scope).
- A Linear tracking ticket for the repo-wide pass is pending: the Unkos
  workspace is at its free-tier issue cap, so the ticket could not be
  created at adoption time. File it when the cap lifts and reference it
  here as the lift trigger.
- `.github/workflows/*.yml` — the workaround surface.
