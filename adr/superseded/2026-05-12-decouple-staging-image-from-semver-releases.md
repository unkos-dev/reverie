---
status: superseded
date: 2026-05-12
supersedes: []
decision-makers: "John Unkovich"
consulted: []
informed: "Reverie contributors"
superseded-by:
  - "../2026-05-12-platform-matrix-via-native-runners.md"
---

# Decouple staging Docker image publication from semver release tags

## Context and Problem Statement

`.github/workflows/docker-publish.yml` triggers exclusively on `push:
tags: v*`. The Docker image at `ghcr.io/unkos-dev/reverie` is therefore
produced **only** when release-please cuts a versioned release. This
shape was inherited from the original CI scaffold, when "publish on
release" was a reasonable default.

As of 2026-05-12 this coupling is actively blocking work:

- Release-please PR
  [#33 "chore(main): release 0.1.0"](https://github.com/unkos-dev/reverie/pull/33)
  has been open since 2026-04-22, held intentionally. `v0.1.0` is a
  semver milestone reserved for the first functional UI cut; merging it
  for scaffolding would burn the marker and pollute the public package
  timeline
- Zero git tags exist on `unkos-dev/reverie`. Zero GitHub releases. The
  GHCR package endpoint returns `404 Package not found`
- Homelab Phase 3 staging deploy (the homelab staging deploy) has nothing to
  `docker pull`. The Incus LXC on `oci-compute-1` is provisioned but
  idle pending an image URL
- `infra/local/reverie-dev/compose.yml` in the homelab repo references
  the placeholder tag `ghcr.io/unkos-dev/reverie:0.0.0-placeholder`:
  intentionally non-functional until a real tag exists

The deeper problem: **the staging image lifecycle is gated on the
public release lifecycle.** These are different concerns operating on
different cadences:

- Public releases are deliberate, semver-meaningful events triggered by
  release-please when functional milestones land. Cadence: weeks to
  months. Audience: self-hosters
- Staging images need to exist whenever the main branch is in a
  testable state. Cadence: every merge. Audience: the staging
  environment and its operators

Tying them together forces premature releases (release v0.1.0 now to
unblock staging) or manual workarounds (local builds pushed by hand,
breaking the CI-is-publisher invariant). Both are wrong shapes.

This ADR records the decision to decouple the two, the alternatives
that were considered, and the conditions that would force a future
reversal, so the next agent maintaining the publish workflow can see
the load-bearing constraints before re-coupling the channels.

## Decision

Reverie's CI publishes Docker images on **two independent triggers**:

1. **`main`-branch push**: emits `ghcr.io/unkos-dev/reverie:main`
   (floating, tracks main HEAD) and `:sha-<7>` (immutable pin to that
   commit). Audience: staging environments and reproducible-pin
   consumers
2. **Version tag push (`v*`)**: emits `:vX.Y.Z` and `:X.Y` (semver
   tags). Audience: self-hosters consuming a released version

The semver tag flow is **unchanged**: release-please continues to
own version bumps and `v*` tag creation, and the existing
`docker/metadata-action` `type=semver` patterns only emit on tag refs.

`:latest` is **deliberately not auto-assigned** to either channel
during this transition. It is reserved for the first semver release
(whenever PR #33 eventually merges) and will track the most recent
semver tag from that point on.

Concretely, the workflow change is single-file, ~10 lines:

```yaml
on:
  push:
    tags: ["v*"]
    branches: [main]

concurrency:
  group: docker-publish-${{ github.ref }}
  cancel-in-progress: true

# ... in docker/metadata-action step:
tags: |
  type=semver,pattern={{version}}
  type=semver,pattern={{major}}.{{minor}}
  type=ref,event=branch
  type=sha,prefix=sha-
```

Operator side, one-time after first post-merge run: GHCR creates the
package as **private** by default. Manually flip to public via
GitHub UI (Packages → reverie → Settings → Change visibility →
Public). Automation of this step is rejected as scope creep: one-time
UI action is acceptable operational cost.

## Consequences

- Good: **release lifecycle stays semver-pure**. PR #33 can be held
  until the app is v0.1.0-worthy. No pressure to merge it
  for unrelated reasons. The first semver release means what it says
- Good: **staging gets continuous images at zero extra release
  cadence**. Every main merge produces a pullable artefact. Homelab
  Phase 3 deploy unblocks immediately
- Good: **reproducible pins via `:sha-<7>`**. Staging operators can
  pin a specific commit when investigating a regression, then move
  back to `:main` when done. This is the conventional pattern for
  ephemeral environments tracking trunk
- Good: **single workflow file maintained**. Both triggers live in
  `docker-publish.yml`; one upgrade path for `docker/login-action`,
  `docker/metadata-action`, `docker/build-push-action`, runner
  ubuntu version, etc.
- Good: **CI-is-publisher invariant preserved**. No local-build
  workarounds, no out-of-band pushes. Every image at `ghcr.io` is
  traceable to a workflow run + commit SHA
- Neutral: **GHCR storage doubles in steady state**. Two long-lived
  tag flows instead of one. GHCR is free for public packages, so
  cost is zero. Storage footprint is the only concern; manifest
  cleanup via retention policy is a future ADR if it matters
- Bad: **`:main` is a footgun for self-hosters who copy the wrong
  tag**. A homelabber following an old blog post might pull `:main`
  and get an unreleased build. Mitigation: documentation in
  `README.md` and `docs/` calls out that `:main` is staging-only;
  `:latest` (once it exists) is the consumer-facing channel
- Bad: **package visibility flip is a manual operator action, not
  a CI step**. The first push after this PR merges creates a private
  package. Until the operator flips it via the GitHub UI (Packages
  → reverie → Settings → Change visibility → Public), homelab pulls
  will 401. CI cannot perform this step. It requires a maintainer
  with admin scope on the package. Mitigation: this is a one-time
  cost documented in the continuous staging images task's acceptance criteria and operator
  handoff
- Bad: **gha layer cache will see two trigger paths**. If a future
  build-cache PR (cargo-chef) lands and pushes large cache layers,
  both `main`-push runs and `v*`-tag runs share the 10 GB cap. Not a
  problem today; flagged as a constraint for the next ADR on
  build-cache strategy
- Neutral: **multi-arch / buildx readiness**. Current workflow is
  single-arch (`linux/amd64`). Adding `linux/arm64` later doesn't
  conflict with the trigger change; it adds a `platforms:` field to
  `build-push-action`. Out of scope here

## Alternatives Considered

- **Kick release-please early to force `v0.1.0`.** Rejected: burns
  the semver milestone on a scaffolding release. The first semver tag
  appears in `CHANGELOG.md` forever; making it "v0.1.0: docker
  scaffolding, no UI" is permanent low-signal noise. Self-hosters who
  later browse releases would have to skip past it. The whole point
  of holding PR #33 is that v0.1.0 should mean something
- **Manual local `docker build && docker push`.** Rejected: breaks
  the CI-is-publisher invariant. Every image at `ghcr.io/unkos-dev/`
  should be reproducible from a workflow run + commit SHA. Manual
  pushes lose that audit trail, can't be re-built deterministically,
  and require maintainer credentials with `write:packages` scope
  outside of GitHub Actions OIDC. Provenance also breaks any future
  Sigstore / cosign signing flow
- **Auto-assign `:latest` to main HEAD.** Rejected: `:latest` is a
  contract with self-hosters that means "the most recent stable
  release I can run". Pointing it at main HEAD makes `docker pull
ghcr.io/unkos-dev/reverie` (the most natural command a curious user
  types) return an unreleased build. Once shipped, this expectation
  is hard to walk back. Better to leave `:latest` unset until the
  first real release defines it
- **Separate `docker-publish-staging.yml` workflow file.** Rejected,
  duplicates the entire pipeline (login, metadata, buildx setup,
  build-push) for a one-line trigger difference. Two upgrade paths
  to track when bumping action versions. The single-workflow
  approach with both triggers in `on:` is cleaner; metadata-action's
  `type=semver` patterns already correctly emit only on tag refs, so
  cross-contamination is impossible
- **`workflow_dispatch` manual button.** Rejected: defeats
  CI-driven deploy automation. Staging should track main
  automatically; gating image production on a maintainer clicking a
  button reintroduces the friction this decision is trying to
  eliminate. `workflow_dispatch` is useful as a fallback for rebuild
  scenarios but not as the primary trigger
- **Push to a different registry for staging (Docker Hub, ECR,
  separate GHCR namespace).** Rejected: adds infrastructure
  (credentials, retention policies, audit scope) for no benefit.
  GHCR can host both channels under one package with different tags,
  which is its intended use

## Revisit Conditions

Open a superseding ADR if any of the following happen:

- **First semver release ships.** Once PR #33 (or its successor)
  merges and `v0.1.0` exists, decide whether `:latest` auto-tracks
  the most recent semver tag (conventional choice) or stays manually
  curated. This ADR explicitly leaves `:latest` policy unanswered for
  the pre-release window
- **Multi-channel staging emerges.** If a `release/X.Y` branch
  strategy is ever adopted (e.g. for patch backports while main moves
  forward), the trigger set becomes: `main` → `:main`, `release/X.Y`
  → `:release-X.Y`, `v*` → `:vX.Y.Z`. The metadata-action tag
  templates need a new conditional. Worth a new ADR because it
  changes the staging-vs-release distinction this decision rests on
- **Image-signing or attestation is adopted.** If Sigstore / cosign /
  SLSA provenance is added, the trigger set may need to split, e.g.
  only `v*` builds get signed because signing has cost and signing
  every main-push isn't worth it. Or, conversely, _all_ publishes
  get signed and the cost calculus changes. Either way, signing
  policy interacts with this decision and deserves its own record
- **Multi-arch builds are added.** `linux/arm64` support (likely
  driven by self-hosters running Raspberry Pi 5, Ampere ARM cloud
  instances, or Apple Silicon homelabs) doubles build time. May force
  re-evaluation of whether every main push warrants a full multi-arch
  build, or whether arm64 only fires on `v*` tags. Different
  trade-off than today's single-arch x86_64
- **GHCR storage / pull cost becomes a real constraint.** Currently
  free for public packages. If GitHub changes pricing or imposes
  pull-rate limits that bite, retention policy / tag cleanup becomes
  a first-class concern. Today it's free, so deferred

## Implementation Plan

- **Affected paths**: `.github/workflows/docker-publish.yml` only
- **Pattern**: keep the existing job structure
  (`docker/login-action` → `docker/metadata-action` → `docker/build-push-action`).
  Add `branches: [main]` to `on: push:`. Add `concurrency` group keyed
  on `github.ref`. Extend the `tags:` block in metadata-action with
  `type=ref,event=branch` and `type=sha,prefix=sha-`. Do not add
  `type=raw,value=latest,enable=...`
- **No new dependencies**. `docker/metadata-action@v6` already
  supports all required tag types
- **No Dockerfile change**, no app-code change, no test change
- **Verification** (lifted from the continuous staging images task's acceptance criteria):
  - [ ] First post-merge workflow run produces both
        `ghcr.io/unkos-dev/reverie:main` and
        `ghcr.io/unkos-dev/reverie:sha-<7>`
  - [ ] After one-time operator visibility flip, `docker pull
ghcr.io/unkos-dev/reverie:main` succeeds from a clean
        unauthenticated context
  - [ ] `:latest` tag does not exist at GHCR after the workflow run
  - [ ] Next `v*` tag push (whenever PR #33 eventually merges)
        still emits `:vX.Y.Z` + `:X.Y` correctly, the semver path is
        not regressed
  - [ ] gha cache usage post-merge stays under a 5 GB observed
        ceiling, leaving headroom for a future build-cache ADR
        (cargo-chef). Observe via repo Settings → Actions → Caches
- **Documentation follow-up**: when the README is next touched, add
  a note distinguishing `:main` (staging, unreleased) from `:latest`
  / `:vX.Y.Z` (once they exist). Not blocking this ADR; tracked as
  part of the continuous staging images task's operator handoff
- **Cross-repo coordination**: once `:main` is published and
  package-visibility is public, homelab repo updates
  `infra/local/reverie-dev/compose.yml` to point at
  `ghcr.io/unkos-dev/reverie:main`. Tracked on the homelab side
  under the homelab staging deploy

## More Information

- MADR 4.0: <https://adr.github.io/madr/>
- Related: [`adr/2026-04-30-adopt-architecture-decision-records.md`](../2026-04-30-adopt-architecture-decision-records.md):
  the meta-ADR that established this format
- Related: [`adr/2026-05-05-single-image-distribution-central-csp.md`](../2026-05-05-single-image-distribution-central-csp.md):
  the upstream invariant. The image _contents_ are decided by that
  ADR; this ADR decides _when_ the image publishes
- Tracker: the continuous staging images task, which is the
  Linear ticket commissioning this ADR and the corresponding workflow PR
- Related: the homelab staging deploy, which is the homelab Phase 3
  staging-deploy work that consumes the `:main` image once published
- Related: [PR #33](https://github.com/unkos-dev/reverie/pull/33),
  the held release-please PR whose intentional delay surfaced this
  coupling
- Code references:
  - `.github/workflows/docker-publish.yml`: single file changed by
    this decision
  - `release-please-config.json`: release-please configuration,
    unchanged by this decision but provides the canonical semver
    flow that `:latest` will eventually track
