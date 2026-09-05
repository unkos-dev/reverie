---
type: ADR
profile-version: 1
id: "REV-ADR-0043"
title: "Remote Rust build cache on object storage"
status: "accepted"
recorded-on: "2026-09-06"
decided-on: "2026-07-26"
decision-makers:
  - "John Unkovich"
---

# Remote Rust build cache on object storage

## Context and problem statement

CI caches Rust compilation with `Swatinem/rust-cache`, which archives the workspace `target/` directory and the
cargo registry into one tarball per job, keyed on the toolchain and the `Cargo.lock` hash. It degrades in two steps
rather than one. A lockfile change misses the exact key but still restores the previous tarball through a prefix
restore-key, leaving cargo to rebuild only what moved. A toolchain, profile, or `RUSTFLAGS` change rotates the key
prefix itself, and nothing restores.

Measured on the `Backend checks` job:

| Run                                  | Total | Lint | Doctests |
| ------------------------------------ | ----- | ---- | -------- |
| Exact key hit                        | 194s  | 79s  | 26s      |
| Lockfile bump, restored by prefix    | 252s  | 87s  | 57s      |
| Key prefix rotated, nothing restored | 786s  | 342s | 349s     |

The recurring event is the middle row, not the bottom one. Renovate raises dependency pull requests continuously,
and each costs an extra minute or so rather than a full rebuild. The bottom row is a toolchain or profile change,
which a content-addressed store cannot improve either: the compiler version is part of every cache key it computes.

The pressure is therefore storage, not compilation. GitHub allows 10 GiB of Actions cache per repository. This
repository sits at 9.75 GiB across 69 entries, with three Rust tarball streams competing for it, and each lockfile
bump mints a new generation of each. Restore-keys only work while an older generation survives eviction, so the
prefix restore that makes a lockfile bump cheap is exactly what the quota threatens.

Can CI keep compiled artifacts outside the repository quota, so the tarball streams stop competing for eviction,
without making the common runs slower than the tarball already makes them?

## Decision drivers

- Compiled artifacts should not compete with unrelated caches for a fixed repository quota.
- Cache granularity should survive eviction, not depend on an older generation surviving alongside the current one.
- A pull request must not be able to poison the cache the default branch reads.
- A cache fault must degrade to a slow build, never a wrong or failing one.
- Reverting must be cheap if the remote store does not pay for itself.

## Considered options

- Keep `Swatinem/rust-cache` alone
- Replace it with `kache` backed by the GitHub Actions cache
- Add `kache` backed by S3-compatible object storage, keeping the tarball cache for the registry only
- Move CI to a third-party runner provider with a larger or regional cache

## Decision outcome

Chosen option: **Add `kache` backed by S3-compatible object storage, keeping the tarball cache for the registry
only**, because it is the only option that addresses granularity and the quota together while leaving a one-step
revert.

`kache` is a content-addressed `rustc` wrapper already used for local builds. It keys each compilation on the
compiler version, the crate source, its dependencies' content hashes, and the normalised flags, so a bumped
dependency invalidates that crate and its dependents rather than the graph. The store lives in a Cloudflare R2
bucket, which has no repository quota and no egress charge.

The two caches divide cleanly rather than overlapping. `Swatinem/rust-cache` drops to `cache-targets: false` and
carries only the registry, which is small enough to sit inside the quota comfortably. `kache` owns compiled
artifacts.

Write access is gated on the default branch. Pull request runs receive object-read credentials and run with the
remote in read-only mode; only pushes to `main` receive read-write credentials. This mirrors the reasoning already
recorded on the container build's cache scopes, where a pull-request-controlled write into a scope a privileged job
later reads is treated as a poisoning surface.

The rollout starts with `Backend checks`, the job with the largest non-instrumented compile. Coverage stays on the
tarball cache for now because `-C instrument-coverage` puts it in a separate key space, so including it would
roughly double the stored artifacts for a compile phase that is a minority of that job's runtime.

### Consequences

- Positive: a dependency bump invalidates only the crates that depend on it, and that granularity survives
  eviction, where the tarball's prefix restore does not.
- Positive: object storage has no repository quota, so one of the three Rust tarball streams leaves the 10 GiB pool
  entirely and stops competing for eviction with the other two.
- Positive: relieving the tarball of `target/` leaves the registry cache well inside the 10 GiB quota, which stops
  unrelated caches being evicted.
- Positive: the gap against an exact tarball hit was one defect rather than the design. A zero-byte `.rmeta` emitted
  under `cargo clippy --all-targets` was rejected by the kache 0.11.0 store, leaving the workspace crate to
  recompile as the last serial unit in two steps, at roughly 300s against the tarball's 196s. kache 0.12.0 stores
  those units, and a warm default-branch run now measures 191s.
- Negative: pull request runs measure 238s to 242s against that same warm case, so parity is the default branch's
  figure rather than every run's, and the difference is unexplained.
- Negative: a share of every run still compiles uncached. Roughly 220 C translation units bypass the cache on a
  compiler flag kache declines to model, and aggregate remote request latency exceeds transfer time, which together
  account for most of what the compile phase still costs.
- Negative: CI gains a dependency on a third-party action, a young caching tool, and an external storage provider,
  any of which can fail or change. Failures degrade to uncached compilation rather than breaking the build.
- Negative: credentials now exist that can write to a shared build cache. The read-only split limits the blast
  radius to pushes on the default branch.
- Negative: the bucket is same-continent with the runners but not co-located, so restore throughput is a measured
  quantity rather than an assumed one.
- Negative: fork pull requests cannot read repository secrets and so cannot reach the store. They request the
  tarball with `target/` included, but `actions/cache` derives its version from the path set, so they cannot
  restore the registry-only tarball the default branch now saves. Once the last full-target generation expires they
  restore nothing; this gap is tracked as debt separately.
- Positive: artifacts are never shared with developer machines. The compiler flags, linker build, and libc differ
  enough that keys would not match even if credentials were distributed, so local builds keep their own store.

## Pros and cons of the options

### Keep `Swatinem/rust-cache` alone

- Positive: already in place, well understood, and has no external dependency beyond GitHub.
- Positive: a key hit is fast and needs no network beyond GitHub's own.
- Positive: a lockfile bump still restores through a prefix restore-key, so it costs about a minute rather than a
  full rebuild.
- Negative: three Rust tarball streams share a 10 GiB quota already at 9.75 GiB, and that prefix restore only works
  while an older generation survives eviction.

### Replace it with `kache` backed by the GitHub Actions cache

- Positive: brings content addressing without any external storage or new credentials.
- Negative: the store still lands in the same 10 GiB quota, so the constraint that forced `save-if: main` is
  untouched.
- Negative: removes the registry caching the tarball action also provides.

### Add `kache` backed by S3-compatible object storage, keeping the tarball cache for the registry only

- Positive: granularity and the quota are addressed together.
- Positive: the two caches have disjoint responsibilities, so removing the remote leaves a working configuration
  behind.
- Negative: adds an action, a tool, a storage provider, and credentials.
- Negative: restore throughput now depends on a network path outside GitHub.

### Move CI to a third-party runner provider with a larger or regional cache

- Positive: some providers offer regional caches and larger quotas.
- Negative: replaces the entire runner fleet to solve a caching problem.
- Negative: adds recurring cost and a vendor with access to CI execution.

## More information

Revisit if measured restore time on the default branch fails to beat the tarball miss path, if the stored artifacts
outgrow what the retention rule keeps in check, or if the upstream action or tool stops being maintained. Reverting
means removing one action and restoring `cache-targets` on one job.

Object storage has no server-side eviction, so retention is enforced by a bucket lifecycle rule rather than by the
tool.
