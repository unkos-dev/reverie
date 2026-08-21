---
status: accepted
date: 2026-05-13
supersedes: []
decision-makers: "John Unkovich"
consulted: []
informed: "Reverie contributors"
---

# GHA build cache + cargo-chef Dockerfile layering for Docker publish

## Context and Problem Statement

The predecessor ADR
[`2026-05-12-platform-matrix-via-native-runners.md`](2026-05-12-platform-matrix-via-native-runners.md)
established the current build shape (native per-arch runners, dynamic
matrix, manifest merge) and explicitly flagged build-cache strategy as
the next ADR. Empirical baseline 2026-05-12 main-push (arm64-only,
post-native-runners ADR): prepare 3s + build 3m55s + merge 22s = **4m35s wall**,
all cold every run. Backend has 70 direct Cargo dependencies, and that
number will climb as feature work lands (Step 7 enrichment, OIDC, etc.).
Cold rebuild cost compounds across two arches on every `v*` tag push
and across every main push.

There is no on-disk persistence between runs: GitHub-hosted runners
are ephemeral. Without an explicit cache backend, every `cargo build`
re-fetches and re-compiles all 70 deps. The current Dockerfile also
puts dep compilation and app compilation in the same layer, so even if
some external cache existed, layer-keyed reuse wouldn't help when only
app code changes.

This ADR records the decision on cache shape, the alternatives that
were considered, and the conditions that would force a future
reversal. It does not change the build shape decided by the
predecessor: per-arch native runners, manifest merge, the two
publication channels, and the `:latest`-not-auto-assigned policy all
remain in force.

## Decision

Reverie's Docker publish workflow uses **GitHub Actions cache
(`type=gha`)** as the buildkit cache backend, with a **cargo-chef
4-stage Dockerfile** ensuring dependency compilation lands in a
dedicated cacheable layer.

1. **GHA cache backend, per-arch scope.** `docker/build-push-action@v7`
   gains:

   ```yaml
   cache-from: type=gha,scope=buildcache-${{ matrix.arch }}
   cache-to: type=gha,scope=buildcache-${{ matrix.arch }},mode=max
   ```

   `mode=max` exports intermediate layers so partial-hit scenarios still
   benefit. The scope key partitions amd64 and arm64 caches; the key is
   **branch-agnostic by design** (see GHA branch scoping below).

2. **cargo-chef 4-stage Dockerfile.** The backend section splits into
   `chef` (shared base with pinned `cargo-chef@0.1.77 --locked`),
   `planner` (emits `recipe.json`), `cooker` (compiles deps only from
   `recipe.json`), and `backend-builder` (real build atop the warm dep
   layer). The cooker layer is the cache target: warm hits skip ~3min
   of dep compilation when `Cargo.lock` is unchanged.

3. **Frontend buildkit package-store cache mount.**
   `RUN --mount=type=cache,id=pnpm,target=/pnpm/store pnpm install`.
   Survives within a single build (buildkit-scoped, not layer-scoped).
   Cross-run reuse comes from the gha layer cache for the install layer
   when `pnpm-lock.yaml` is unchanged; the mount avoids tarball re-fetch
   when the layer cache invalidates for unrelated reasons.

4. **Tier 1 observability only.** Post-build steps emit `docker buildx
du` (this runner's local buildkit content store, ephemeral) and a
   `$GITHUB_STEP_SUMMARY` pointer directing operators to inspect the
   `Build and push by digest` step log for per-stage `CACHED` lines.
   Tier 2 (cache-inventory cron against the persistent gha pool) and
   Tier 3 (OTLP traces, sccache stats) are deferred to revisit
   conditions; the persistent cache pool is inspected on demand via
   `gh api .../actions/caches`.

5. **`workflow_dispatch` trigger added.** Manual trigger enables
   ad-hoc rebuilds and feature-branch verification of workflow changes
   that wouldn't otherwise fire on a non-main push. Permanent addition;
   same write-perm boundary as push-to-main.

GHA cache **branch scoping** is the partitioning key, not the
buildkit scope name. The same `scope=buildcache-${{ matrix.arch }}`
string across branches is the correct shape:

- Each run writes cache entries under its own `GITHUB_REF`.
- A run reads from its own ref's entries first, falling back read-only
  to the base ref (typically `main`).
- Embedding `${{ github.ref_name }}` in the scope would **defeat**
  base-ref fallback and force every branch to start cold.

## Consequences

- Good: **warm builds skip the cooker layer.** Empirically verified
  on the implementation branch:
  [run 25771083286](https://github.com/unkos-dev/reverie/actions/runs/25771083286)
  cold = 6m32s; [run 25771327467](https://github.com/unkos-dev/reverie/actions/runs/25771327467)
  warm = 2m38s (whitespace-only src edit, same branch). The warm run
  log shows `importing cache manifest from gha:...` (cache-from hit)
  and 11+ `CACHED` lines.
- Good: **cache miss is never a correctness risk.** `cache-from`
  miss → cold build. `cache-to` failure → silent fallthrough. No
  partial-state corruption surface; rollback is a single-commit
  revert with no data migration or external state to unwind.
- Good: **mode=max preserves intermediate layers.** Partial-hit
  scenarios (e.g. a single dep version bump on a single arch) still
  reuse what they can.
- Good: **per-arch scope isolation.** amd64 and arm64 caches don't
  compete for entries; LRU eviction is local to each arch's pool.
- Neutral: **GHA cache pool capped at 10GB per repo.** Currently
  ~1GB per arch after a full build; plenty of headroom. LRU eviction
  is silent; degradation is perf-only and surfaces as colder warm
  builds. Tier 1 obs catches this manually (operator inspects
  `actions/caches` API on demand).
- Neutral: **`workflow_dispatch` adds a manual trigger surface on
  main.** Same write-perm boundary as push-to-main; no new privilege
  escalation path.
- Bad: **cargo-chef adds a build-time dependency.** Pinned
  `0.1.77 --locked`. Bump requires lockstep across the shared `chef`
  base (single line). Supply-chain risk is bounded to one
  version-pinned crate.
- Bad: **chef-layer rebuild cost on base-image churn.** When
  `rust:1-slim` ships a patch update (auto-pulled), the
  `cargo install cargo-chef` re-runs (~30–60s tax) and cascades to
  cooker + backend-builder. Pre-existing pattern (this PR doesn't
  introduce floating tags), but the cache wiring makes the cost
  visible as a cold-day event rather than baseline.
- Bad: **first main-push after merge is cold.** Cache writes from
  feature-branch verification go under `refs/heads/feat/...`; main's
  first cache-from miss is expected. Subsequent main-pushes warm.
- Unknown: **tag-push (`refs/tags/v*`) cache hit behaviour.** Tag
  refs read with `base = main` fallback per GHA scoping rules.
  Documented behaviour for tag refs is ambiguous in practice. Both
  outcomes are functionally correct, only perf differs. Action: record
  actual tag-push wall-clock on the first `v*` after merge; if
  consistently cold, add a `debt/` entry and consider explicit
  cross-ref hydration.

## Alternatives Considered

- **Buildkit cache mounts on `~/.cargo/registry` + `target/`, no
  cargo-chef.** Considered. Approach:
  `RUN --mount=type=cache,target=/usr/local/cargo/registry --mount=type=cache,target=/build/target cargo build --release`.
  Cross-run reuse via `type=gha,mode=max` works the same way.
  **Rejected**: no dedicated layer for deps vs app code, so any
  app-only edit still re-links every dep. Cargo's incremental
  compilation helps but doesn't match the layer-cache hit rate of a
  dedicated cooker stage. The alternative is also less portable:
  cache-mount semantics on non-GHA buildkit drivers vary, while
  cargo-chef's layer pattern is portable to any buildkit-compatible
  builder.
- **`type=registry` cache backend (push cache layers to ghcr.io as
  separate OCI artifacts).** Considered. Unbounded retention (no
  10GB cap), survives across all refs (no branch-scope partitioning
  to work around). **Rejected for now**: extra package surface in
  ghcr.io (a `:buildcache` tag alongside `:main` / `:vX.Y.Z` is
  operationally noisy for a pre-v1.0 project), no measurable upside
  while the 10GB pool is 0% utilised. Listed as a revisit condition
  if 10GB cap eviction becomes visible.
- **sccache (sccache-action) atop or instead of cargo-chef.** Considered.
  sccache provides function-level cache reuse across builds, finer
  grain than layer reuse. **Rejected for now**: complexity without
  observable need at 70 deps + ~4min cold backend builds. Listed as
  a revisit condition if backend dep count crosses ~150 or cold
  cooker rebuild crosses ~8min.
- **`Swatinem/rust-cache` on the CI workflow (not the Dockerfile).**
  Considered. The repo already uses this on the `Backend` CI job
  outside Docker. **Not applicable here**: the Docker publish path
  builds the binary inside the container, where the action can't
  reach. Mentioning it for completeness because it might confuse
  readers familiar with the non-Docker CI path.
- **Status quo: cold builds every time.** Acceptance was "build still
  green". **Rejected as a forward-looking choice**: dep count climbs
  with feature work; tag-push runs both arches; main-push cadence is
  release-driven. The compounding cold cost crosses pain threshold
  before v1.0.

## Revisit Conditions

Open a superseding or amending ADR if any of the following happen:

- **Cache hit rate visibly degrades.** Cooker layer not `CACHED`
  across consecutive src-only main-pushes (no `Cargo.lock` churn).
  Investigate 10GB cap eviction or recipe.json hash drift. Most
  likely shape: move to `type=registry,ref=ghcr.io/unkos-dev/reverie:buildcache`
  for unbounded retention.
- **Backend dep count crosses ~150 direct deps OR cooker cold-rebuild
  crosses ~8min.** Add sccache layer atop cargo-chef for finer-grained
  reuse.
- **Multi-arch builds become per-PR (not just main + tag).** Per-PR
  cache scopes pollute the 10GB pool fast under the current shape.
  Re-evaluate scope key partitioning and possibly switch to registry
  cache.
- **buildkit/GHA cache backend deprecated or pricing changes.** Forced
  revisit; registry cache is the obvious fallback.
- **External contributors arrive in volume.** CI cost visibility
  matters more; Tier 2 (weekly cache-inventory cron) becomes worth
  building.
- **Tag-push consistently runs cold despite a warm main cache.**
  Confirms tag/base-ref fallback doesn't work as hoped; add explicit
  cross-ref cache hydration on tag-push (e.g. seed cooker layer from
  main before build).
- **Image signing / SLSA attestation changes pipeline shape.** If
  signing introduces steps that change which layers benefit from cache
  reuse, revisit cooker layer composition.

## More Information

- MADR 4.0: <https://adr.github.io/madr/>
- Predecessor:
  [`adr/2026-05-12-platform-matrix-via-native-runners.md`](2026-05-12-platform-matrix-via-native-runners.md):
  established build shape (per-arch native runners, manifest merge);
  flagged build-cache strategy as the next ADR. This ADR resolves
  that flag. Build-shape decisions from the predecessor remain in
  force.
- Related:
  [`adr/2026-05-05-single-image-distribution-central-csp.md`](2026-05-05-single-image-distribution-central-csp.md):
  defines image contents; this ADR decides how those contents are
  cached during build. No interaction with runtime image surface.
- Tracker: the Linear ticket commissioning this work
- Implementation PR:
  [#233](https://github.com/unkos-dev/reverie/pull/233), merged
  2026-05-13 as commit
  [`2f62f65`](https://github.com/unkos-dev/reverie/commit/2f62f65)
- Adversarial review of the implementation surfaced two MEDIUM
  findings folded into this ADR:
  - D2 (cargo-chef alternative not recorded) → Alternatives Considered
  - S1 (tag-push perf unverified) → Consequences + Revisit Conditions
- Empirical baseline pre-cache: 4m35s arm64-only main-push (run
  [25744557285](https://github.com/unkos-dev/reverie/actions/runs/25744557285),
  commit `ba65bdf`, 2026-05-12)
- Empirical post-cache cold: 6m32s feature-branch first push
  (run [25771083286](https://github.com/unkos-dev/reverie/actions/runs/25771083286))
- Empirical post-cache warm: 2m38s feature-branch same-branch reuse
  (run [25771327467](https://github.com/unkos-dev/reverie/actions/runs/25771327467))
- Code references:
  - `Dockerfile`: cargo-chef split decided by this ADR
  - `.github/workflows/docker-publish.yml`: cache wiring + Tier 1
    obs + workflow_dispatch decided by this ADR
  - The implementation plan for this work: its carry-over section
    enumerated the points folded into this ADR
