---
status: "accepted"
date: 2026-07-26
supersedes: []
decision-makers: "John Unkovich"
consulted: []
informed: "Reverie contributors"
---

# Remote Rust build cache on object storage, alongside the tarball cache

## Context and Problem Statement

CI caches Rust compilation with `Swatinem/rust-cache`, which archives the
workspace `target/` directory and the cargo registry into one tarball per job,
keyed on the toolchain and the `Cargo.lock` hash. That cache is all or nothing:
when the key matches, the job is fast; when any input moves, the job rebuilds
the whole dependency graph.

Measured on the `Backend checks` job:

| Run      | Total | Lint | Doctests |
| -------- | ----- | ---- | -------- |
| Key hit  | 194s  | 79s  | 26s      |
| Key miss | 786s  | 342s | 349s     |

A single Renovate lockfile bump moves that key, so one changed dependency costs
the same as changing all of them. Renovate runs weekly lockfile maintenance and
raises grouped dependency pull requests, so the miss path is a recurring tax
rather than an edge case.

The tarball also has to fit a quota. GitHub allows 10 GiB of Actions cache per
repository, which cannot hold gigabyte-scale per-branch copies, so the caches
are configured `save-if: main`. Pull requests can restore but never contribute,
and each job keeps its own tarball rather than sharing one body of artifacts.

Can CI keep compiled artifacts in a form where one changed dependency
invalidates one crate, without a storage ceiling that forces branches to be
read-only?

## Decision Drivers

- One changed dependency should not rebuild the whole graph.
- Storage must not be capped so low that branches cannot write to it.
- A pull request must not be able to poison the cache the default branch reads.
- A cache fault must degrade to a slow build, never a wrong or failing one.
- Reverting must be cheap if the remote store does not pay for itself.

## Considered Options

- Keep `Swatinem/rust-cache` alone.
- Replace it with `kache` backed by the GitHub Actions cache.
- Add `kache` backed by S3-compatible object storage, keeping the tarball cache
  for the registry only.
- Move CI to a third-party runner provider with a larger or regional cache.

## Decision Outcome

Chosen option: "Add `kache` backed by S3-compatible object storage, keeping the
tarball cache for the registry only", because it is the only option that
addresses granularity and the quota together while leaving a one-step revert.

`kache` is a content-addressed `rustc` wrapper already used for local builds. It
keys each compilation on the compiler version, the crate source, its
dependencies' content hashes, and the normalised flags, so a bumped dependency
invalidates that crate and its dependents rather than the graph. The store lives
in a Cloudflare R2 bucket, which has no repository quota and no egress charge.

The two caches divide cleanly rather than overlapping. `Swatinem/rust-cache`
drops to `cache-targets: false` and carries only the registry, which is small
enough to sit inside the quota comfortably. `kache` owns compiled artifacts.

Write access is gated on the default branch. Pull request runs receive
object-read credentials and run with the remote in read-only mode; only pushes
to `main` receive read-write credentials. This mirrors the reasoning already
recorded on the container build's cache scopes, where a pull-request-controlled
write into a scope a privileged job later reads is treated as a poisoning
surface.

The rollout starts with `Backend checks`, the job with the largest
non-instrumented compile. Coverage stays on the tarball cache for now because
`-C instrument-coverage` puts it in a separate key space, so including it would
roughly double the stored artifacts for a compile phase that is a minority of
that job's runtime.

### Consequences

- Good, because a single dependency bump now invalidates only the crates that
  depend on it instead of the entire tarball.
- Good, because object storage has no repository quota, so branches can
  contribute artifacts rather than only restoring them, and jobs share one body
  of artifacts instead of one tarball each.
- Good, because relieving the tarball of `target/` leaves the registry cache
  well inside the 10 GiB quota, which stops unrelated caches being evicted.
- Bad, because CI gains a dependency on a third-party action, a young caching
  tool, and an external storage provider, any of which can fail or change.
  Failures degrade to uncached compilation rather than breaking the build.
- Bad, because credentials now exist that can write to a shared build cache. The
  read-only split limits the blast radius to pushes on the default branch.
- Neutral, because the bucket is same-continent with the runners but not
  co-located, so restore throughput is a measured quantity rather than an
  assumed one.
- Neutral, because fork pull requests cannot read repository secrets and so
  cannot reach the store at all. They fall back to the tarball carrying
  `target/`, which is what they had before, rather than being left with no
  compilation cache or with a remote they can never reach.
- Neutral, because artifacts are never shared with developer machines. The
  compiler flags, linker build, and libc differ enough that keys would not match
  even if credentials were distributed, so local builds keep their own store.

### Confirmation

Cache faults must never fail or corrupt a build: the wrapper falls back to real
compilation whenever the store or daemon is unreachable. Write credentials must
never reach a pull-request run.

## Pros and Cons of the Options

### Keep `Swatinem/rust-cache` alone

- Good, because it is already in place, well understood, and has no external
  dependency beyond GitHub.
- Good, because a key hit is fast and needs no network beyond GitHub's own.
- Bad, because the key covers the whole tarball, so one dependency bump costs a
  full rebuild.
- Bad, because the quota forces `save-if: main`, leaving branches unable to
  contribute and each job holding a separate copy.

### Replace it with `kache` backed by the GitHub Actions cache

- Good, because it brings content addressing without any external storage or
  new credentials.
- Bad, because the store still lands in the same 10 GiB quota, so the constraint
  that forced `save-if: main` is untouched.
- Bad, because it removes the registry caching the tarball action also provides.

### Add `kache` backed by S3-compatible object storage

- Good, because granularity and the quota are addressed together.
- Good, because the two caches have disjoint responsibilities, so removing the
  remote leaves a working configuration behind.
- Bad, because it adds an action, a tool, a storage provider, and credentials.
- Bad, because restore throughput now depends on a network path outside GitHub.

### Move CI to a third-party runner provider

- Good, because some providers offer regional caches and larger quotas.
- Bad, because it replaces the entire runner fleet to solve a caching problem.
- Bad, because it adds recurring cost and a vendor with access to CI execution.

## More Information

Revisit if measured restore time on the default branch fails to beat the tarball
miss path, if the stored artifacts outgrow what the retention rule keeps in
check, or if the upstream action or tool stops being maintained. Reverting means
removing one action and restoring `cache-targets` on one job.

Object storage has no server-side eviction, so retention is enforced by a bucket
lifecycle rule rather than by the tool.
