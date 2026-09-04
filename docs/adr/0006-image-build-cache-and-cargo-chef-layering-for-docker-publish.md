---
type: ADR
profile-version: 1
id: "REV-ADR-0006"
title: "Image build cache and cargo-chef layering for Docker publish"
status: "accepted"
recorded-on: "2026-09-04"
decided-on: "2026-05-13"
decision-makers:
  - "John Unkovich"
informed:
  - "Reverie contributors"
---

# Image build cache and cargo-chef layering for Docker publish

## Context and problem statement

Reverie's Docker publish workflow had no on-disk cache persistence between runs: GitHub-hosted runners are ephemeral,
so without an explicit cache backend every `cargo build` re-fetched and recompiled all of the backend's direct
dependencies. The Dockerfile also placed dependency compilation and application compilation in the same layer, so
even an external cache backend would not help layer-keyed reuse when only application code changed.

The predecessor record ([per-architecture native runners with a manifest-list merge](./0005-per-architecture-native-runners-with-a-manifest-list-merge.md))
established the current build shape (native per-arch runners, a dynamic matrix, manifest-list merge) and flagged
build-cache strategy as the next decision. An empirical baseline taken after that record, on a main push building
arm64 only: prepare 3s + build 3m55s + merge 22s = 4m35s wall, all cold on every run. The backend had 70 direct Cargo
dependencies at the time, a count expected to climb as feature work landed. Cold rebuild cost compounds across two
architectures on every `v*` tag push and across every main push.

What cache backend and Dockerfile layering should the Docker publish workflow use, and what conditions would justify
revisiting that choice?

## Decision drivers

- GitHub-hosted runners are ephemeral, so nothing persists between runs without an explicit cache backend.
- Dependency and application compilation shared a single Dockerfile layer, so layer-keyed cache reuse could not help
  when only application code changed.
- Cold-build cost was expected to compound as the dependency count grew and as the two-architecture build ran on
  every main push and every `v*` tag push.
- A chosen approach should stay portable across buildkit-compatible builders rather than depend on driver-specific
  cache-mount semantics.

## Considered options

- GitHub Actions cache backend with cargo-chef layering
- Buildkit cache mounts on the cargo registry and target directories, without cargo-chef
- `type=registry` cache backend (cache layers pushed to the registry as separate OCI artifacts)
- sccache atop or instead of cargo-chef
- `Swatinem/rust-cache` on the CI workflow instead of the Dockerfile
- Status quo: cold builds every time

## Decision outcome

Chosen option: **GitHub Actions cache backend with cargo-chef layering**, because it removes the ephemeral-runner cold
rebuild on both the dependency and application layers while keeping the cache pattern portable to any
buildkit-compatible builder.

The Docker publish workflow uses GitHub Actions cache (`type=gha`) as the buildkit cache backend, with a cargo-chef
four-stage Dockerfile ensuring dependency compilation lands in a dedicated cacheable layer.

`docker/build-push-action` carries a `cache-from` and a `cache-to` input, both `type=gha` and both scoped by the
matrix architecture, with `mode=max` added to `cache-to`:

```yaml
cache-from: type=gha,scope=buildcache-<arch>
cache-to: type=gha,scope=buildcache-<arch>,mode=max
```

`mode=max` exports intermediate layers so partial-hit scenarios still benefit, and the scope key partitions the
amd64 and arm64 caches. The scope key is
branch-agnostic by design: each run writes cache entries under its own ref and reads from that ref's entries first,
falling back read-only to the base ref (typically main). Embedding the branch name in the scope would defeat that
base-ref fallback and force every branch to start cold.

The backend section of the Dockerfile splits into a `chef` stage (a shared base with a pinned, locked cargo-chef
install), a `planner` stage (emits `recipe.json`), a `cooker` stage (compiles dependencies only, from
`recipe.json`), and a `backend-builder` stage (the real build atop the warm dependency layer). The cooker layer is
the cache target: a warm hit skips the dependency compilation when `Cargo.lock` is unchanged.

`pnpm fetch` populates the frontend package store from the lockfile before the frontend build and SBOM stages
diverge, in an ordinary Dockerfile layer rather than a buildkit cache mount. Both stages inherit that store, and the
frozen offline install consumes it. The dependency layer is therefore complete when restored from the GHA cache and
remains valid even though an ephemeral builder holds no cache-mount state. The build invocation disables pnpm's
automatic dependency repair only after the explicit frozen offline install has already succeeded, so source-layer
timestamps cannot trigger an unscoped install.

Post-build steps emit the runner's local buildkit content-store usage and a summary pointer directing operators to
the build-and-push step log for per-stage cache-hit lines. Deeper observability, such as a scheduled inventory of
the persistent GHA cache pool or build traces, is a revisit condition; the persistent pool can be inspected on
demand through the GitHub API.

A `workflow_dispatch` trigger was added permanently, under the same write-permission boundary as a push to main, to
allow ad-hoc rebuilds and feature-branch verification of workflow changes that would not otherwise run.

### Consequences

- Positive: warm builds skip the cooker layer. On the implementation branch, a whitespace-only source edit measured
  roughly 6m32s cold against 2m38s warm on the same branch, with the warm run showing a cache-manifest import and
  more than ten cached layers.
- Positive: a cache miss is never a correctness risk. A `cache-from` miss falls back to a cold build, and a
  `cache-to` failure falls through silently; there is no partial-state corruption surface, and rollback is a
  single-commit revert with no data migration or external state to unwind.
- Positive: frontend dependency state survives an external cache restore, because `pnpm fetch` writes the store into
  an ordinary layer and the offline bundle install fails immediately if that layer is incomplete.
- Positive: `mode=max` preserves intermediate layers, so a partial hit, such as a single dependency bump on a single
  architecture, still reuses what it can.
- Positive: per-arch scope isolation means the amd64 and arm64 caches do not compete for entries; eviction is local
  to each architecture's pool.
- Positive: the `workflow_dispatch` trigger adds a manual entry point under the same write-permission boundary as
  push-to-main, with no new privilege-escalation path.
- Negative: cargo-chef adds a pinned build-time dependency; a version bump requires a lockstep update across the
  shared `chef` base.
- Negative: chef-layer rebuild cost recurs on base-image churn. When the pinned Rust base image ships a patch
  update, the cargo-chef install re-runs and cascades into the cooker and backend-builder stages.
- Negative: the first main push after a feature branch merges is cold, because cache writes from feature-branch
  verification land under that branch's ref, and main's first cache-from read misses; subsequent main pushes warm.
- Negative: `pnpm fetch` is an experimental pnpm command, though it is pnpm's documented recommendation for Docker
  builds on ephemeral CI workers, and the pinned pnpm image digest bounds its behaviour from changing without review.
- Negative: the GHA cache pool is capped at 10GB per repository. Usage was roughly 1GB per architecture after a full
  build, well under the cap, but eviction under that cap is silent and would surface only as slower warm builds.
- Negative: tag-push (`refs/tags/v*`) cache-hit behaviour was unverified at the time of the decision. GHA's
  base-ref fallback should apply to tag refs, but documented behaviour for tag refs is ambiguous, and both outcomes
  are functionally correct with only performance differing.

### Confirmation

The cargo-chef layering is in `Dockerfile`: the `chef` stage installs a pinned, locked `cargo-chef`, the `planner`
stage emits `recipe.json`, the `cooker` stage runs `cargo chef cook` against that recipe, and the `backend-builder`
stage builds the real binary on top. The GHA cache backend and the `workflow_dispatch` trigger are in
`.github/workflows/docker-publish.yml`, whose `cache-from`/`cache-to` lines pass `type=gha` scoped by
`buildcache-<arch>` with `mode=max` on `cache-to`, and whose `on:` block carries `workflow_dispatch`.

## Pros and cons of the options

### GitHub Actions cache backend with cargo-chef layering

- Positive: cargo-chef gives dependency compilation a dedicated cacheable layer, so an application-only edit does
  not re-link every dependency.
- Positive: the cargo-chef layer pattern is portable to any buildkit-compatible builder, unlike cache-mount
  semantics, which vary by driver.
- Neutral: adds a pinned build-time dependency and a lockstep bump requirement across the shared `chef` base stage.

### Buildkit cache mounts without cargo-chef

- Positive: a simpler Dockerfile, with no additional build-time dependency.
- Negative: no dedicated layer separates dependency compilation from application compilation, so an application-only
  edit still re-links every dependency; cargo's incremental compilation helps but does not match a dedicated
  cooker-stage hit rate.
- Negative: cache-mount semantics vary across buildkit drivers, so this approach is less portable than the
  cargo-chef layer pattern.

### `type=registry` cache backend

- Positive: unbounded retention, with no 10GB cap and no branch-scope partitioning to work around.
- Negative: publishes cache layers as a separate `:buildcache` OCI artifact in the registry, operationally noisy for
  a pre-v1.0 project.
- Negative: no measurable upside while the 10GB GHA cache pool was unused.

### sccache atop or instead of cargo-chef

- Positive: function-level cache reuse, finer-grained than layer reuse.
- Negative: adds complexity without an observable need at the dependency count and cold-build time in force at the
  time of the decision.

### `Swatinem/rust-cache` on the CI workflow

- Negative: not applicable to the Docker publish path, because the binary is built inside the container, where the
  action cannot reach.

### Status quo: cold builds every time

- Negative: dependency count and build cadence were both expected to climb with feature work, so the compounding
  cold-build cost was expected to cross the pain threshold before v1.0.

## More information

Related: [single-image distribution with central CSP enforcement](./0003-single-image-distribution-with-central-csp-enforcement.md)
defines image contents; this record decides how those contents are cached during build, with no interaction with the
runtime image surface.

Open a superseding record if any of the following happen:

- The cooker layer stops showing a cache hit across consecutive source-only main pushes with no `Cargo.lock` churn.
  The likely cause is 10GB cap eviction or `recipe.json` hash drift, and the likely resolution is a registry cache
  backend for unbounded retention.
- The backend's direct dependency count crosses roughly 150, or a cold cooker rebuild crosses roughly 8 minutes. The
  likely resolution is an sccache layer atop cargo-chef for finer-grained reuse.
- Multi-arch builds run on every pull request rather than only on main and tags. Per-PR cache scopes would pollute
  the 10GB pool quickly under the current shape, forcing a re-evaluation of scope-key partitioning and possibly a
  move to a registry cache.
- The buildkit or GHA cache backend is deprecated or its pricing changes; a registry cache is the obvious fallback.
- External contributors arrive in volume, making CI cost visibility matter enough to justify a scheduled cache
  inventory.
- Tag pushes consistently run cold despite a warm main cache, confirming that tag and base-ref fallback does not work
  as expected; the likely resolution is explicit cross-ref cache hydration on tag push.
- Image signing or attestation changes the pipeline shape in a way that changes which layers benefit from cache
  reuse, which would call for revisiting the cooker layer's composition.

Re-recorded from adr/2026-05-13-image-build-cache.md (decided 2026-05-13); history holds the original.
