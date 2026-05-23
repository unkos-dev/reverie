# Tracked technical debt

This directory tracks accepted technical debt with explicit lift
conditions. Each entry is a known-wrong-shape the project carries
temporarily because of a specific constraint, with a recorded plan to
remove it.

`debt/` is sister to `adr/`, not a subset:

| Artefact | Purpose                                                        | Lifecycle                              |
| -------- | -------------------------------------------------------------- | -------------------------------------- |
| `adr/`   | Decisions ("we chose X over Y, here's why")                    | proposed → accepted → maybe superseded |
| `debt/`  | Concessions ("we know this is wrong, accepting until Y lifts") | active → lifted (kept for audit)       |

If you're recording a deliberate choice, write an ADR. If you're
recording a constraint you intend to remove, write a debt entry.

## Hard rules

- **Every entry has a measurable lift condition.** If you cannot
  articulate one, the shape is wrong — fix the shape, do not accept
  the workaround. "When we have time" is not a lift condition.
- **Sweep `debt/` at every release tag and at the start of any
  non-trivial planning conversation.** When a constraint lifts, the
  entry is flipped to `status: lifted`, not deleted. Historical record
  matters; future contributors and outside readers benefit from
  seeing what was carried, why, and how it was removed.
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
status: active # active | lifted
severity: low|medium|high
surfaces: [developer, server-operator, end-user, security, ci]
adopted: 2026-05-05 # when accepted (or recognised, if pre-existing)
adopted-because: <ticket / PR / inline rationale>
lift-when-class: dep-unblocks | internal-refactor | external-standard | feature-flag | release-tag | infra-gap-closes
lift-when: <specific measurable condition>
lifted: ~ # YYYY-MM-DD if status: lifted, else ~
superseded-by: ~ # PR / commit / ADR link if lifted, else ~
---
```

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

- Every release tag — before bumping the version, walk active entries
  and check if any constraint has lifted. Promote lift-ready ones to
  PRs.
- Start of non-trivial planning conversations — same sweep, applied
  to whatever subsystem the planning touches.

### Lift

When the constraint lifts:

1. The PR that removes the workaround flips the entry's frontmatter:
   `status: lifted`, `lifted: <date>`, `superseded-by: <PR url>`.
2. The entry stays in place. Do not delete.
3. The README index moves the entry from "Active" to "Lifted".

## Why entries are machine-extractable

The frontmatter spec exists in this shape because a future consumer
(post-v0.2 public dev roadmap) will read these entries to populate a
"Known limitations and accepted technical debt" section, filtered and
grouped by `severity`, `surfaces`, and `lift-when-class`. Write
entries assuming an outside-the-team reader (a self-hoster considering
deployment, an OSS contributor evaluating the project) will eventually
see them. No private references; Linear ticket IDs are fine.

The roadmap consumer is the second consumer. The agent (and any
contributor) is the first. Today, only the first reader uses the
entries — the structure is in place so the second consumer requires no
translation pass when it joins.

## Active

<!-- listed most-stale first; new entries go to the top -->

- [`validation_status` ships as raw String, not a typed enum](2026-05-23-validation-status-untyped.md) — adopted because DB enum and frontend contract vocabularies disagree; lifts on UNK-276 reconciliation PR
- [tower-sessions pinned to 0.14 (axum-login peer pin)](2026-05-21-tower-sessions-0-14-pin.md) — adopted because axum-login 0.18.0 peer-pins tower-sessions 0.14; lifts on axum-login release > 0.18.0 (UNK-101)
- [Staging compose has no automated CI smoke test](2026-05-08-staging-compose-no-ci-smoke.md) — adopted because CI smoke for `compose.staging.yml` is real eng work; lifts on UNK-185
- [MemoryStore for production sessions](2026-05-05-memory-store-sessions.md) — adopted because PostgresStore not wired at scaffold; lifts on UNK-163
- [Vite allowedHosts permissive in dev](2026-05-05-vite-allowed-hosts-permissive.md) — adopted because workspace hostnames unenumerable for static allowlist; lifts on UNK-168
- [Dev postgres host port 5433](2026-05-05-dev-postgres-port-5433.md) — adopted because Coder workspace's shared-postgres on 5432; lifts on UNK-169
- [chrono in OIDC test mock](2026-05-05-chrono-in-oidc-mock.md) — adopted because openidconnect v4 forces chrono types in test setup; lifts on dep-unblock or wrapper

## Lifted

<!-- empty at first land; entries move here on lift, never deleted -->

- [Runtime sqlx queries instead of compile-time macros](2026-05-05-runtime-sqlx-queries.md) — lifted 2026-05-06; superseded by PR series #157–#163
- [ENV_LOCK + unsafe env mutation in config tests](2026-05-05-env-lock-config-tests.md) — lifted 2026-05-06; superseded by PR #168
