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
workaround is purged, not archived. Git retains the full history and
the purge commit names the resolving PR (see [Lifecycle → Resolve](#resolve)).

## Hard rules

- **Every entry has a measurable lift condition.** If you cannot
  articulate one, the shape is wrong. Fix the shape, do not accept
  the workaround. "When we have time" is not a lift condition.
- **Sweep `debt/` at every release tag and at the start of any
  non-trivial planning conversation.** Walk the entries, check whether
  any constraint has lifted, and promote lift-ready ones to PRs.
- **Purge on resolution, not on unblock.** When a workaround is fully
  resolved, lift condition met _and_ the proper fix shipped (the
  wrong-shape is gone from the code), delete the entry file and remove
  its README line. Do not leave a tombstone. The purge commit message
  **must name the resolving PR** (e.g.
  `chore(debt): purge X — resolved by #209`) so the history stays
  discoverable via `git log --diff-filter=D -- debt/`. A merely
  _unblocked_ workaround whose fix has not yet shipped stays active.
- **Workarounds adopted under temporary constraints (missing tooling,
  unbuilt infra, blocked deps) are tech debt, not idiomatic
  patterns.** Trace each candidate workaround to its justification
  before defending it; if the justification has lifted, it's debt.
- **Never reference an issue tracker.** `scripts/no-issue-refs.sh`
  gates every Markdown file, `debt/` included, and rejects tracker
  references outright. Entries are read by outside contributors and
  self-hosters who cannot open a private tracker, so state the lift
  condition in codebase terms. A PR number is acceptable where the
  provenance matters; the tracker carries the scheduled work and stays
  out of the tree.

## Frontmatter

Every entry has YAML frontmatter:

```yaml
---
severity: low|medium|high
surfaces: [developer, server-operator, end-user, security, ci]
adopted: 2026-05-05 # when accepted (or recognised, if pre-existing)
adopted-because: <PR / inline rationale>
lift-when-class: dep-unblocks | internal-refactor | external-standard | feature-flag | release-tag | infra-gap-closes
lift-when: <specific measurable condition>
---
```

There is no `status` field and no `lifted` / `superseded-by` fields: an
entry exists only while the debt is live, and resolution **deletes** the
entry rather than flipping a flag. The resolving PR and date live in the
purge commit, not in frontmatter.

### Field meanings

- **`severity`**: impact score. Used by future tooling (post-v0.2
  public roadmap) to filter what surfaces to outside readers. `low` =
  paper cut affecting only contributors; `medium` = real cost to one
  audience (operators / developers / CI); `high` = security smell,
  unsafe code, or a footgun that has caused or could cause incidents.
- **`surfaces`**: who notices this debt. Multi-valued list from:
  `developer` (only contributors hit it), `server-operator` (people
  running Reverie in production), `end-user` (browser users of a
  Reverie instance), `security` (defensive posture), `ci`
  (continuous integration).
- **`lift-when-class`**: bucketed reason for blockage:
  - `dep-unblocks`: waiting on an upstream dependency to ship X
  - `internal-refactor`: needs work in this repo to lift
  - `external-standard`: waiting on an external standard / convention
  - `feature-flag`: gated on a project-internal feature flag flip
  - `release-tag`: gated on a release version
  - `infra-gap-closes`: waiting on adjacent infrastructure (homelab,
    deployment surface) being in place
- **`lift-when`**: specific, measurable condition. Free text.
  Examples: "the recovery-pin index ships on main", "openidconnect v5 stable
  release ships with chrono decoupled", "v0.2 release tag cut".

## Lifecycle

### Adopt

Write the entry **alongside** (or before) the code change that
introduces the workaround. The act of writing the lift condition
forces an honest evaluation: if you can't state one, the shape is
wrong and you fix the code instead.

### Sweep

The agent (or any contributor) runs through `debt/` at:

- Every release tag, before bumping the version, walk the entries
  and check if any constraint has lifted. Promote lift-ready ones to
  PRs.
- Start of non-trivial planning conversations: same sweep, applied
  to whatever subsystem the planning touches.

### Resolve

When a workaround is fully resolved, lift condition met _and_ the
proper fix shipped (the wrong-shape is gone from the code):

1. The PR that removes the workaround **deletes** the entry file and
   removes its line from the [Active](#active) list. Nothing is archived
   in-tree; there is no "Lifted" section.
2. Name the link on both sides so it stays recoverable: reference the
   entry file in the PR body, and name the resolving PR in the **purge
   commit message**.
3. `git log --diff-filter=D -- debt/` recovers any purged entry with its
   full text and the commit that removed it. That is the audit trail. The live tree only carries current debt.

A workaround that is merely _unblocked_ (the blocking constraint
lifted) but whose proper fix has not yet shipped is **not** resolved;
it stays active, with its `lift-when` updated to reflect what now
remains.

## Why entries are machine-extractable

The frontmatter spec exists in this shape because a future consumer
(post-v0.2 public dev roadmap) will read these entries to populate a
"Known limitations and accepted technical debt" section, filtered and
grouped by `severity`, `surfaces`, and `lift-when-class`. Write
entries assuming an outside-the-team reader (a self-hoster considering
deployment, an OSS contributor evaluating the project) will eventually
see them. No private references and no tracker IDs, per the hard rule
above.

The roadmap consumer is the second consumer. The agent (and any
contributor) is the first. Because every live entry is by definition
active debt, the roadmap reads the directory as-is: no status filter,
no translation pass.

## Active

<!-- listed most-stale first; new entries go to the top -->

- [Pre-migration manifestations have no embedded-cover flag](2026-07-31-embedded-cover-flag-not-backfilled.md): adopted 2026-07-31; fixing the dashboard cover-coverage metric to count embedded EPUB covers required a new has_embedded_cover column set at ingestion, and no data already stored can reconstruct that flag for existing rows without re-reading each file; lifts when a backfill pass re-runs the cover check against every manifestation with format='epub' and has_embedded_cover IS NULL
- [ISBNs live beside the identifier registry, not in it](2026-07-25-isbn-outside-identifier-registry.md): adopted 2026-07-25; the external-identifier registry shipped additive-only because rematch, the value indexes, OPF writeback, and search are wired to the manifestations ISBN columns; lifts when a feature needs a uniform identifier model, at which point the columns fold into the registry as an `isbn` scheme
- [Snyk sees no Rust dependencies](2026-07-20-snyk-rust-dependency-blind-spot.md): adopted 2026-07-20; Snyk supports Cargo only via `snyk sbom test`, which creates no monitored target and emits no SARIF, so the advisory baseline covers npm, sources, and the image but not crates; cargo-deny scans Cargo.lock directly in the meantime; lifts when `snyk test`/`snyk monitor` support Cargo natively
- [cssMinify forced to esbuild while lightningcss corrupts light-dark()](2026-07-05-cssminify-esbuild-lightdark.md): adopted from PR review; rolldown-vite's default minifier miscompiles light-dark() and every grid color token uses it; lifts when the pinned vite-plus ships the lightningcss fix and the default minifier passes the built-CSS check
- [dependency-review allow-ghsas suppresses vite-alias false positives](2026-06-30-vite-plus-alias-dependency-review.md): adopted from PR #558; vp's `vite` npm alias makes the fork match real-vite advisories, so the gate suppresses 14 GHSAs; lifts when vp drops the alias or dependency-review resolves npm aliases
- [Fork pull requests lose their compilation cache](2026-07-26-fork-pr-cache-fallback.md): adopted with the remote build cache; actions/cache versions entries by path set, so a fork asking for target/ cannot restore the registry-only tarball the default branch saves; lifts when forks reach a cache again or measurement shows they need not
- [kache-action digest pinned past its only tag](2026-07-26-kache-action-digest-pinned-past-tag.md): adopted because the v1 tag predates the namespace and pr-comment inputs and GitHub silently discards undeclared ones; lifts when upstream tags a release containing a257c05
- [Prefetch shards uploaded by a hand-written step](2026-07-26-kache-shard-upload-step.md): adopted because the action's JavaScript post step runs from the repository root where this workspace has no Cargo.lock; lifts when upstream accepts a working directory for that step
- [Self-registration ships dormant (endpoint off by default, screen unrouted)](2026-06-30-self-registration-dormant.md): adopted from the S3 review; the register endpoint mints immediately-active accounts, unsafe for a reachable multi-user instance; lifts when registration becomes a moderated request-access flow with a disabled-on-register invariant test
- [Forwarded client-IP header trusted by name, not proxy identity](2026-06-27-forwarded-header-trust-unbound.md): adopted from PR #511 review; the login/recovery limiter honors trusted_client_ip_header without a peer-CIDR binding; lifts when forwarded-IP trust is bound to an allow-listed reverse-proxy CIDR
- [Publisher and pub_date missing from metadata edit UI](2026-05-26-publisher-pubdate-ui-gap.md): adopted from 11c; BookDetail doesn't carry those columns yet; lifts when API + UI extended
- [chrono in OIDC test mock](2026-05-05-chrono-in-oidc-mock.md): adopted because openidconnect v4 forces chrono types in test setup; lifts on dep-unblock or wrapper
