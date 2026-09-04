---
type: ADR
profile-version: 1
id: "REV-ADR-0005"
title: "Per-architecture native runners with a manifest-list merge"
status: "accepted"
recorded-on: "2026-09-04"
decided-on: "2026-05-12"
decision-makers:
  - "John Unkovich"
informed:
  - "Reverie contributors"
---

# Per-architecture native runners with a manifest-list merge

## Context and problem statement

Reverie's Docker publish workflow built and published images from a single amd64 GitHub-hosted runner. The earlier
record that decoupled the staging image from semver releases established two publication channels (a `main`-branch
push emits `:main` and `:sha-<7>`; a `v*`-tag push emits `:vX.Y.Z` and `:X.Y`) and flagged multi-arch readiness as a
later concern. Subsequent work added a trigger-driven platform matrix and `docker/setup-qemu-action`, so the amd64
runner could produce arm64 layers via `binfmt_misc`. That put the arm64 image on `:main` and unblocked the homelab
staging deploy, but left QEMU emulation as a permanent cost on every build.

A post-merge run was still mid-build at the 30-minute mark on the single arm64-via-QEMU job, with Rust compilation
under emulation as the dominant cost. The arm64 image on `:main` is staging-critical, and a build that slow fails the
"CI runtime does not regress materially" acceptance criterion the earlier record's trigger split was meant to
preserve.

GitHub now offers free ARM64 hosted runners for public repositories (`runs-on: ubuntu-24.04-arm`). With native
runners on both architectures available, the build shape can move from emulating the foreign architecture on one
runner to building each architecture on its own native runner in parallel, then merging a manifest list.

How should the Docker publish workflow build and publish multi-architecture images without QEMU emulation, while
keeping the two-channel publication policy and the CI runtime the earlier record established?

## Decision drivers

- CI runtime for the arm64 build must not regress materially, the acceptance criterion the earlier record's trigger
  split was meant to preserve.
- The `:main` arm64 image is staging-critical and must build promptly on every push.
- GitHub's free ARM64 hosted runners for public repositories remove the previous rationale for emulation.
- The release boundary (`v*` tag) should be fully native on both architectures.

## Considered options

- Native runners per architecture, merged into a manifest list.
- Native runner for the arm64 leg only, keep QEMU for tag-push amd64.
- Self-hosted ARM runner on the homelab arm64 compute node.
- Status quo: keep QEMU on every build.
- Arm64-only on `v*` tags, the inverse of the chosen trigger split.
- Full multi-arch build (amd64 and arm64) on both triggers.

## Decision outcome

Chosen option: **native runners per architecture, merged into a manifest list**, because it removes QEMU emulation from
every build, keeps the `main`-push arm64 build and the `v*`-tag release boundary fully native, and lets wall-clock
time on tag pushes drop to the slower of the two per-architecture builds instead of their sum.

The publish workflow builds each architecture on a native runner and merges a manifest list as a final step:

- A `prepare-matrix` job emits the build matrix as JSON based on `github.ref_type`: a tag push includes both
  `build (amd64)` on `ubuntu-latest` and `build (arm64)` on `ubuntu-24.04-arm`; a `main` push includes only the arm64
  leg, whose sole consumer is the homelab arm64 staging host. A job-level `if:` cannot read the `matrix` context, so
  the per-trigger filter lives in the matrix shape rather than as a job gate.
- Each `build` job uses `docker/build-push-action` with `push-by-digest=true` and `name-canonical=true`, sets
  `provenance: mode=max` and `sbom: true`, emits OCI labels via `docker/metadata-action` at build time (labels cannot
  be back-filled onto an already-pushed image config), and uploads its digest as a workflow artifact.
- A single `merge` job depending on `build` downloads the digest artifacts, computes tags with
  `docker/metadata-action` (tags are a property of the final manifest list, not the per-arch images), and assembles
  the manifest with `docker buildx imagetools create`.
- `docker/setup-qemu-action` is removed; both legs of the release boundary (`v*` tag) build natively.
- The tag set, the `concurrency` group keyed on `github.ref`, and the sha-prefix gating from the earlier record are
  preserved.

The two-channel publication policy from the earlier record remains in force: a `main`-branch push emits `:main` and
`:sha-<7>`; a `v*`-tag push emits `:vX.Y.Z` and `:X.Y`; `:latest` remains deliberately unassigned until the first
semver release. Only the build-execution shape changes.

### Consequences

- Positive: wall-clock time on tag pushes is `max(amd64, arm64)`, not their sum, since the per-arch builds run in
  parallel on native runners.
- Positive: the `main`-push arm64 build runs natively rather than under QEMU emulation, eliminating the 30-minute-plus
  baseline and making the staging image cadence acceptable.
- Positive: `docker/setup-qemu-action` and its `binfmt_misc` fragility are gone from the workflow; the per-trigger
  filter moves from a workflow-step shell expression to a matrix-shape expression emitted by the `prepare-matrix` job.
- Positive: the release boundary (`v*` tag) is fully native on both architectures, so self-hosters pulling a
  versioned tag receive images built without emulation on either leg.
- Positive: `provenance: mode=max` and `sbom: true` on each per-arch build carry through `imagetools create` onto the
  resulting manifest list, keeping a path to future image signing open.
- Negative: digests cross the `build`/`merge` job boundary as workflow artifacts, since the two jobs do not share a
  workspace; this is the canonical pattern but adds upload and download steps.
- Negative: four jobs run per publish instead of one (`prepare-matrix`, one or two `build` jobs, `merge`); total
  runner-minutes on tag pushes stay close to the QEMU baseline, since the arm64 leg dominates either way.
- Negative: the workflow depends on continued GitHub free-tier ARM64 runner availability; a pricing or capacity
  change would slow or break the build.

### Confirmation

`.github/workflows/docker-publish.yml` implements this decision: the `prepare-matrix` job emits `ubuntu-24.04-arm`
as the arm64 leg's runner and `ubuntu-latest` for amd64, gated on `github.ref_type`; a `build` job runs once per
matrix entry with `docker/build-push-action` pushing by digest; a `merge` job downloads the digests and runs
`docker buildx imagetools create`. `docker/setup-qemu-action` does not appear anywhere in the workflow. The
`docker/metadata-action` tag block in the `merge` job matches the two-channel policy (`type=semver` twice,
`type=ref,event=branch`, `type=sha,prefix=sha-,enable=...`), though the sha-prefix condition today also requires
`github.ref_name == 'main'`, narrower than the `github.ref_type != 'tag'` condition this record states. The
`concurrency` group is keyed on `github.ref` as described, but the workflow no longer sets
`cancel-in-progress: true`; the workflow's own comment now states that an in-flight publish is serialized and never
cancelled, a later change than this decision.

## Pros and cons of the options

### Native runners per architecture, merged into a manifest list

- Positive: wall-clock on tag pushes is the slower of the two per-arch builds, not their sum.
- Positive: no QEMU dependency; the `main`-push arm64 build runs natively, eliminating the 30-minute-plus baseline.
- Negative: depends on continued GitHub free-tier ARM64 runner availability.

### Native runner for the arm64 leg only, keep QEMU for tag-push amd64

- Negative: leaves QEMU on the public release path, the worst place to absorb emulation cost; a half-measure that
  does not remove the dependency.

### Self-hosted ARM runner on the homelab arm64 compute node

- Negative: self-hosted runners on a public repository are a documented security anti-pattern, since a fork can
  inject workflow code that runs on the self-hosted machine.
- Negative: adds operational surface (runner agent, OS patching, network segmentation, ephemeral isolation) that a
  free, ephemeral, isolated GitHub-hosted runner avoids.

### Status quo: keep QEMU on every build

- Negative: empirically blocked by a build still running past 30 minutes; fails the earlier record's "CI runtime
  does not regress materially" acceptance criterion.

### Arm64-only on `v*` tags, the inverse of the chosen trigger split

- Negative: the homelab staging deploy consumes the `:main` arm64 image; making arm64 a release-only artefact
  regresses staging.

### Full multi-arch build (amd64 and arm64) on both triggers

- Negative: amd64 has no consumer today; the sole arm64 consumer is the homelab compute node, and the earlier
  record's trigger split already decided amd64 is a release-only platform.

## More information

This record replaces the earlier record's build-shape decision. That record's two-channel publication policy and
its decision to leave `:latest` unassigned remain in force here.

### Publication-channel alternatives

These were the alternatives considered when the two-channel publication policy was set:

- Kick release-please early to force a first semver release: rejected, it burns the first semver tag on a
  scaffolding release with no functional milestone behind it, which is permanent low-signal noise in the changelog.
- Manual local `docker build && docker push`: rejected, it breaks the invariant that every published image is
  reproducible from a workflow run and a commit SHA, and it would need maintainer credentials with package-write
  scope outside GitHub Actions OIDC.
- Auto-assign `:latest` to `main` HEAD: rejected, `:latest` is a contract meaning "the most recent stable release",
  and pointing it at an unreleased build breaks that contract for the most natural pull command a new user types.
- A separate staging workflow file: rejected, it duplicates the whole pipeline (login, metadata, buildx setup,
  build-push) for what is a one-line trigger difference, and doubles the action-version upgrade path to track.
- A `workflow_dispatch` manual button as the primary trigger: rejected, it defeats CI-driven deploy automation by
  gating staging image production on a maintainer clicking a button.
- A different registry for staging (Docker Hub, ECR, a separate GHCR namespace): rejected, it adds infrastructure
  (credentials, retention policies, audit scope) for no benefit over hosting both channels as tags in one package.

Related: [Single-image distribution with central CSP enforcement](./0003-single-image-distribution-with-central-csp-enforcement.md),
an upstream invariant. That record decides the image's contents; this record decides how the image is built and
tagged.

Open a superseding record if any of the following happen:

- A second consumer or developer architecture emerges. A developer workstation, a CI gate, or a second deploy target
  on amd64 changes the trigger split: a `main` push may need both architectures again, or arm64 may need to stay on
  tag push only, depending on demand. The build-shape decision is stable across that change, but the
  `prepare-matrix` job's emitted JSON would update.
- GitHub changes free-tier ARM runner pricing or availability. If `ubuntu-24.04-arm` becomes paid for public
  repositories, or capacity caps appear, the trade-off against self-hosted runners flips. Today the security cost of
  a self-hosted runner on a public repository dominates; if the hosted-runner cost rises enough, an ephemeral
  self-hosted runner on the homelab arm64 node, with a fork-safe harness, may become defensible.
- Sustained queue contention on free-tier ARM runners. There is no evidence of capacity issues today. If observed
  queue waits creep above the QEMU baseline this record replaced, the change is a net loss and warrants reversal or a
  self-hosted runner.
- Image signing or SLSA attestation introduces additional steps. Sigstore, cosign, or SLSA level 3 may change where
  signing happens in the pipeline (per-architecture build or after the merge). If signing each per-architecture image
  is wasteful next to signing only the final manifest list, revisit the job topology.
- Attestation propagation through `imagetools create` regresses. The standard pattern preserves per-platform
  provenance and SBOM blobs on the manifest list. If a future buildx version changes that behaviour, the merge step
  needs alternative handling, such as an explicit `cosign sign` on the manifest.

Code references: `.github/workflows/docker-publish.yml` is the workflow this decision changed;
`release-please-config.json` is unchanged by this decision but drives the `v*` tag push this record's release leg
consumes.
