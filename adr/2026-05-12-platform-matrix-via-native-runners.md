---
status: accepted
date: 2026-05-12
decision-makers: john
supersedes:
  - "superseded/2026-05-12-decouple-staging-image-from-semver-releases.md"
---

# Per-architecture native runners with manifest-list merge for Docker publish

## Context and Problem Statement

The predecessor ADR
[`2026-05-12-decouple-staging-image-from-semver-releases.md`](superseded/2026-05-12-decouple-staging-image-from-semver-releases.md)
established two publication channels (`main`-push → `:main` + `:sha-<7>`;
`v*`-tag push → `:vX.Y.Z` + `:X.Y`). It flagged multi-arch readiness as a
later concern. Subsequent work (`UNK-241`, PR #223, merged 2026-05-12)
added a trigger-driven platform matrix and pulled in
`docker/setup-qemu-action` so the amd64 GitHub-hosted runner could
produce arm64 layers via `binfmt_misc`. That landed the arm64 image on
`:main` and unblocked the homelab staging deploy (`UNK-230`).

It also left QEMU emulation as a permanent cost on every build.

Empirical confirmation 2026-05-12: post-merge run
[25731891335](https://github.com/unkos-dev/reverie/actions/runs/25731891335)
was still mid-build at the 30-minute mark on the single arm64-via-QEMU
job. Rust compilation under emulation is the dominant cost. The arm64
image on `:main` is staging-critical and currently fails the "CI runtime
does not regress materially" acceptance criterion the predecessor's
trigger split was meant to preserve.

GitHub now offers free ARM64 hosted runners for public repositories:
`runs-on: ubuntu-24.04-arm`
([docs](https://docs.github.com/en/actions/reference/runners/github-hosted-runners#standard-github-hosted-runners-for-public-repositories)).
With native runners on both architectures available, the build shape can
move from "emulate the foreign arch on one runner" to "build each arch
on its native runner in parallel, then merge a manifest list".

This ADR records the decision to make that move, the alternatives that
were considered, and the conditions that would force a future reversal.
It supersedes the predecessor's CI-shape section — the
two-publication-channels decision and the `:latest`-policy decision from
that ADR remain in force and are restated here for the steady-state
record.

## Decision

Reverie's Docker publish workflow builds each architecture on a native
GitHub-hosted runner and assembles the manifest list as a final step.

1. **One or two build jobs** (dynamic matrix) — the matrix `include`
   list is emitted per trigger by the `prepare-matrix` job described
   in item 3 below. Each instance differs on runner and target
   platform:
   - `build (amd64)` on `ubuntu-latest`, platform `linux/amd64`
   - `build (arm64)` on `ubuntu-24.04-arm`, platform `linux/arm64`

   Each build uses `docker/build-push-action@v7` with
   `outputs: type=image,name=<registry>/<image>,push-by-digest=true,name-canonical=true,push=true`,
   sets `provenance: mode=max` and `sbom: true`, and uploads its
   per-arch digest as a workflow artifact. Each build also runs
   `docker/metadata-action@v6` to emit OCI labels
   (`org.opencontainers.image.*`) and wires them into
   `build-push-action`'s `labels:` input. `imagetools create` in the
   `merge` job cannot back-fill labels into already-pushed image
   configs, so labels must be set at per-arch build time.

2. **One `merge` job** depending on `build`. It downloads the digest
   artifacts, runs `docker/metadata-action@v6` (tags are properties of
   the final manifest list, not the per-arch images), and assembles the
   manifest with `docker buildx imagetools create -t <tag> ... <digest> ...`.
   `imagetools inspect` verifies the resulting list.

3. **Trigger-driven matrix filter** via a small `prepare-matrix` job
   that emits the build matrix `include` list as JSON based on
   `github.ref_type`. The `build` job's strategy reads
   `matrix: ${{ fromJSON(needs.prepare-matrix.outputs.matrix) }}`.
   Tag-push emits both legs; main-push emits only arm64 (sole consumer
   is `oci-compute-1`, an Ampere A1 arm64 instance). Job-level `if:`
   cannot reference the `matrix` context per [GitHub Actions context
   availability](https://docs.github.com/en/actions/learn-github-actions/contexts#context-availability),
   so the per-trigger filter lives in the matrix shape rather than as
   a job gate.

4. **No QEMU.** `docker/setup-qemu-action` is removed. The release
   boundary (`v*` tag) is fully native on both legs.

5. **Tag set, concurrency, sha-prefix gating preserved.** The
   metadata-action tag block keeps the predecessor's pattern:
   `type=semver` × 2, `type=ref,event=branch`, and
   `type=sha,prefix=sha-,enable=${{ github.ref_type != 'tag' }}`. The
   `concurrency` group keyed on `github.ref` with
   `cancel-in-progress: true` is unchanged.

The two-channel publication model and `:latest`-not-auto-assigned
decisions from the predecessor remain in force; only the build-execution
shape changes.

## Consequences

- Good — **wall-clock = `max(amd64, arm64)`**, not sum, on tag pushes.
  Per-arch builds run in parallel on native runners.
- Good — **`main`-push arm64 build runs natively**, not emulated.
  Eliminates the 30+ min QEMU baseline observed on run
  `25731891335`. Staging image cadence becomes acceptable.
- Good — **no QEMU dependency**. `docker/setup-qemu-action` and its
  `binfmt_misc` fragility are gone from the workflow. The "Compute
  build platforms" shell step is replaced by a `prepare-matrix` job
  emitting the include list as JSON; per-trigger filtering moves from
  a workflow-step expression to a matrix-shape expression.
- Good — **release boundary fully native on both architectures**.
  Self-hosters pulling a `v*` tag receive images built without
  emulation on either leg.
- Good — **attestations preserved**. `provenance: mode=max` and
  `sbom: true` on each per-arch build; `imagetools create` propagates
  per-platform attestations onto the resulting manifest list. This is
  the standard pattern and keeps the path to future image signing
  (Sigstore / cosign) unblocked.
- Neutral — **four jobs per publish run instead of one**. One
  `prepare-matrix` setup job (seconds), two `build` jobs (one or two
  matrix instances depending on trigger), one `merge` job. Each is
  short on its own; total runner minutes consumed on tag pushes are
  similar to the QEMU baseline (the amd64 leg was always fast; the
  arm64 leg dominates either way), and total wall-clock drops because
  the two build legs are concurrent.
- Bad — **digest plumbing via `actions/upload-artifact` +
  `download-artifact`**. Per-arch builds and the merge job don't share
  a workspace, so digests cross job boundaries as artifacts. This is
  the canonical pattern but adds two steps per build and one per
  merge. Acceptable cost.
- Bad — **dependency on GitHub free-tier ARM runner availability**.
  If GitHub changes free-tier ARM pricing or imposes sustained queue
  contention, builds slow or break. Revisit conditions below cover
  this.

## Alternatives Considered

- **Option A: swap `runs-on: ubuntu-24.04-arm` for the arm64 leg, keep
  QEMU for tag-push amd64.** Rejected — puts QEMU on the public
  release path. Tags are the worst place to absorb emulation cost.
  Half-measure that doesn't remove the QEMU dependency.
- **Self-hosted ARM runner on `oci-compute-1`.** Rejected — self-hosted
  runners on a public repo are a documented GitHub security
  anti-pattern: forks can inject workflow code that runs on the
  self-hosted machine. Adds operational surface (runner agent, OS
  patching, network segmentation, ephemeral isolation harness). GH-hosted
  ARM is free, ephemeral, and isolated.
- **Status quo: keep QEMU on every build.** Rejected — empirically
  blocked by run `25731891335` (30+ min). The predecessor ADR's
  acceptance criterion that CI runtime does not regress materially
  fails today; this is the regression.
- **Arm64-only on `v*` tags (inverse of the chosen split).** Rejected
  — homelab staging consumes `:main` arm64. Making arm64 a release-only
  artefact regresses staging.
- **Full multi-arch (amd64 + arm64) on both triggers.** Rejected —
  amd64 has no consumer today. The sole arm64 consumer is
  `oci-compute-1`; the predecessor's trigger split already decided
  amd64 is a release-only platform. This ADR preserves that decision
  and just changes how it's built.

## Revisit Conditions

Open a superseding ADR if any of the following happen:

- **Second consumer or dev architecture emerges.** A developer
  workstation, a CI gate, or a second deploy target on amd64 changes
  the trigger split — main-push may need both arches again, or arm64
  may need to remain on tag-push only depending on demand. The
  build-shape decision in this ADR is stable across that change, but
  the `prepare-matrix` job's emitted JSON would update.
- **GitHub changes free-tier ARM runner pricing or availability.** If
  `ubuntu-24.04-arm` becomes paid for public repos, or capacity caps
  appear, the trade-off vs self-hosted runners flips. Today the
  security cost of self-hosted on a public repo dominates; if the
  hosted-runner cost rises enough, ephemeral self-hosted on
  `oci-compute-1` (with a fork-safe harness) may become defensible.
- **Sustained queue contention on free-tier ARM runners.** Today no
  evidence of capacity issues. If observed queue waits creep above
  the QEMU baseline this ADR replaces, the change is a net loss and
  warrants reversal or self-hosted introduction.
- **Image signing / SLSA attestation introduces additional steps.**
  Adding Sigstore / cosign / SLSA L3 may change where signing happens
  in the pipeline (per-arch build vs after merge). If the signing
  step has cost characteristics that make the matrix shape suboptimal
  (e.g. signing each per-arch image is wasteful versus signing only
  the final manifest list), revisit job topology.
- **Attestation propagation through `imagetools create` regresses.**
  The standard pattern preserves per-platform provenance/SBOM blobs on
  the manifest list. If a future buildx version changes that
  behaviour, signing-flow plans break and the merge step needs
  alternative handling (e.g. explicit `cosign sign` on the manifest).

## Implementation Plan

- **Affected paths**: `.github/workflows/docker-publish.yml` only;
  Dockerfile and app code unchanged. ADR work: this file plus a
  `status: superseded` flip and `superseded-by:` cross-reference on
  the predecessor, plus a one-line update in `adr/README.md`.
- **No new dependencies**. `docker/build-push-action@v7`,
  `docker/metadata-action@v6`, `docker/login-action@v4`,
  `docker/setup-buildx-action@v3`, `actions/upload-artifact@v4`, and
  `actions/download-artifact@v4` are all already used in this
  repository or are the canonical companions of actions already used.
  `docker/setup-qemu-action` is removed.
- **Verification** (lifted from `UNK-244` acceptance criteria):
  - [ ] Tag-push (`v*`) produces a multi-arch manifest list
        (`linux/amd64`, `linux/arm64`) where each per-arch image was
        built on a native runner (not via QEMU).
  - [ ] Main-push produces a single-platform manifest (`linux/arm64`
        only) where the arm64 image was built on `ubuntu-24.04-arm`.
  - [ ] Main-push wall-clock runtime drops materially vs current state
        (target: well under 30 min for the arm64 leg; baseline run
        [25731891335](https://github.com/unkos-dev/reverie/actions/runs/25731891335)).
  - [ ] Tag-push wall-clock ≈ `max(amd64-native, arm64-native)`, down
        from `amd64 + arm64-via-QEMU`.
  - [ ] `docker pull && docker run` on `oci-compute-1` continues to
        succeed without `--platform`.
  - [ ] Attestation/provenance present on the final manifest list
        (verify via `docker buildx imagetools inspect --raw`).
  - [ ] OCI labels (`org.opencontainers.image.revision`,
        `org.opencontainers.image.created`,
        `org.opencontainers.image.version`) present on per-arch
        image configs; verify via `docker buildx imagetools inspect
--raw <ref>` and inspect each entry under `manifests[]`.
  - [ ] `concurrency` cancel-in-progress + `sha-` prefix gating
        behaviour preserved.
  - [ ] No `docker/setup-qemu-action` reference remains in the
        workflow.

## More Information

- MADR 4.0: <https://adr.github.io/madr/>
- Supersedes:
  [`adr/2026-05-12-decouple-staging-image-from-semver-releases.md`](superseded/2026-05-12-decouple-staging-image-from-semver-releases.md)
  — two-channel publication and `:latest`-not-auto-assigned decisions
  remain in force; build-shape decision is replaced
- Related:
  [`adr/2026-05-05-single-image-distribution-central-csp.md`](2026-05-05-single-image-distribution-central-csp.md)
  — upstream invariant. The image contents are decided by that ADR;
  this ADR decides how the image is built and tagged
- Tracker: [UNK-244](https://linear.app/unkos/issue/UNK-244) — the
  Linear ticket commissioning this ADR and the corresponding workflow
  PR; folds in [UNK-242](https://linear.app/unkos/issue/UNK-242)
  (originally tracked a superseding ADR for the QEMU-intermediate step
  that this ADR removes wholesale)
- Related: [UNK-241](https://linear.app/unkos/issue/UNK-241) —
  immediate predecessor (introduced QEMU and the trigger-driven
  platform matrix); the trigger split is preserved by this ADR, the
  QEMU dependency is removed
- Related: [UNK-156](https://linear.app/unkos/issue/UNK-156) /
  [UNK-230](https://linear.app/unkos/issue/UNK-230) — homelab Phase 3
  staging deploy that consumes the `:main` arm64 image
- Empirical baseline: [run 25731891335](https://github.com/unkos-dev/reverie/actions/runs/25731891335)
  — 30+ min arm64-via-QEMU build on main-push, the regression that
  prompted this change
- Code references:
  - `.github/workflows/docker-publish.yml` — single workflow changed
    by this decision
  - `release-please-config.json` — release-please configuration,
    unchanged by this decision but provides the `v*` tag flow this
    ADR's tag-push leg consumes
