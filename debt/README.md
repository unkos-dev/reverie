# Tracked technical debt

This directory tracks accepted technical debt with explicit lift
conditions. Each entry is a known-wrong-shape the project carries
temporarily because of a specific constraint, with a recorded plan to
remove it.

`debt/` is sister to `adr/`, not a subset:

| Artefact | Purpose                                                        | Lifecycle                              |
| -------- | -------------------------------------------------------------- | -------------------------------------- |
| `adr/`   | Decisions ("we chose X over Y, here's why")                    | proposed → accepted → maybe superseded |
| `debt/`  | Concessions ("we know this is wrong, accepting until Y lifts") | active → purged on resolution          |

If you're recording a deliberate choice, write an ADR. If you're
recording a constraint you intend to remove, write a debt entry.

`debt/` holds only what the project **currently** owes. A resolved
workaround is purged, not archived — git retains the full history and
the purge commit names the resolving PR (see [Lifecycle → Resolve](#resolve)).

## Hard rules

- **Every entry has a measurable lift condition.** If you cannot
  articulate one, the shape is wrong — fix the shape, do not accept
  the workaround. "When we have time" is not a lift condition.
- **Sweep `debt/` at every release tag and at the start of any
  non-trivial planning conversation.** Walk the entries, check whether
  any constraint has lifted, and promote lift-ready ones to PRs.
- **Purge on resolution, not on unblock.** When a workaround is fully
  resolved — lift condition met _and_ the proper fix shipped (the
  wrong-shape is gone from the code) — delete the entry file and remove
  its README line. Do not leave a tombstone. The purge commit message
  **must name the resolving PR** (e.g.
  `chore(debt): purge X — resolved by #209`) so the history stays
  discoverable via `git log --diff-filter=D -- debt/`. A merely
  _unblocked_ workaround whose fix has not yet shipped stays active.
- **Workarounds adopted under temporary constraints (missing tooling,
  unbuilt infra, blocked deps) are tech debt, not idiomatic
  patterns.** Trace each candidate workaround to its justification
  before defending it; if the justification has lifted, it's debt.
- **Reference the corresponding Linear ticket as the lift trigger.**
  Debt entries describe the invariant; the Linear ticket carries the
  scheduled work.

## Frontmatter

Every entry has YAML frontmatter:

```yaml
---
severity: low|medium|high
surfaces: [developer, server-operator, end-user, security, ci]
adopted: 2026-05-05 # when accepted (or recognised, if pre-existing)
adopted-because: <ticket / PR / inline rationale>
lift-when-class: dep-unblocks | internal-refactor | external-standard | feature-flag | release-tag | infra-gap-closes
lift-when: <specific measurable condition>
---
```

There is no `status` field and no `lifted` / `superseded-by` fields: an
entry exists only while the debt is live, and resolution **deletes** the
entry rather than flipping a flag. The resolving PR and date live in the
purge commit, not in frontmatter.

### Field meanings

- **`severity`** — impact score. Used by future tooling (post-v0.2
  public roadmap) to filter what surfaces to outside readers. `low` =
  paper cut affecting only contributors; `medium` = real cost to one
  audience (operators / developers / CI); `high` = security smell,
  unsafe code, or a footgun that has caused or could cause incidents.
- **`surfaces`** — who notices this debt. Multi-valued list from:
  `developer` (only contributors hit it), `server-operator` (people
  running Reverie in production), `end-user` (browser users of a
  Reverie instance), `security` (defensive posture), `ci`
  (continuous integration).
- **`lift-when-class`** — bucketed reason for blockage:
  - `dep-unblocks` — waiting on an upstream dependency to ship X
  - `internal-refactor` — needs work in this repo to lift
  - `external-standard` — waiting on an external standard / convention
  - `feature-flag` — gated on a project-internal feature flag flip
  - `release-tag` — gated on a release version
  - `infra-gap-closes` — waiting on adjacent infrastructure (homelab,
    deployment surface) being in place
- **`lift-when`** — specific, measurable condition. Free text.
  Examples: "UNK-167 merged to main", "openidconnect v5 stable
  release ships with chrono decoupled", "v0.2 release tag cut".

## Lifecycle

### Adopt

Write the entry **alongside** (or before) the code change that
introduces the workaround. The act of writing the lift condition
forces an honest evaluation: if you can't state one, the shape is
wrong and you fix the code instead.

### Sweep

The agent (or any contributor) runs through `debt/` at:

- Every release tag — before bumping the version, walk the entries
  and check if any constraint has lifted. Promote lift-ready ones to
  PRs.
- Start of non-trivial planning conversations — same sweep, applied
  to whatever subsystem the planning touches.

### Resolve

When a workaround is fully resolved — lift condition met _and_ the
proper fix shipped (the wrong-shape is gone from the code):

1. The PR that removes the workaround **deletes** the entry file and
   removes its line from the [Active](#active) list. Nothing is archived
   in-tree; there is no "Lifted" section.
2. Name the link on both sides so it stays recoverable: reference the
   entry file in the PR body, and name the resolving PR in the **purge
   commit message**.
3. `git log --diff-filter=D -- debt/` recovers any purged entry with its
   full text and the commit that removed it. That is the audit trail —
   the live tree only carries current debt.

A workaround that is merely _unblocked_ (the blocking constraint
lifted) but whose proper fix has not yet shipped is **not** resolved —
it stays active, with its `lift-when` updated to reflect what now
remains.

## Why entries are machine-extractable

The frontmatter spec exists in this shape because a future consumer
(post-v0.2 public dev roadmap) will read these entries to populate a
"Known limitations and accepted technical debt" section, filtered and
grouped by `severity`, `surfaces`, and `lift-when-class`. Write
entries assuming an outside-the-team reader (a self-hoster considering
deployment, an OSS contributor evaluating the project) will eventually
see them. No private references; Linear ticket IDs are fine.

The roadmap consumer is the second consumer. The agent (and any
contributor) is the first. Because every live entry is by definition
active debt, the roadmap reads the directory as-is — no status filter,
no translation pass.

## Active

<!-- listed most-stale first; new entries go to the top -->

- [a11y gate advisory until showcase baseline clean](2026-06-05-a11y-gate-advisory.md) — adopted from UNK-268; the gate fails on a known default-Badge contrast violation (UNK-345) deliberately not allowlisted, so it ships `continue-on-error` to avoid blocking its own + all frontend PRs; lifts when UNK-345 ships and the gate flips to blocking
- [Publisher and pub_date missing from metadata edit UI](2026-05-26-publisher-pubdate-ui-gap.md) — adopted from 11c; BookDetail doesn't carry those columns yet; lifts when API + UI extended
- [Dev postgres host port 5433](2026-05-05-dev-postgres-port-5433.md) — adopted because Coder workspace's shared-postgres on 5432; lifts on UNK-169
- [chrono in OIDC test mock](2026-05-05-chrono-in-oidc-mock.md) — adopted because openidconnect v4 forces chrono types in test setup; lifts on dep-unblock or wrapper
