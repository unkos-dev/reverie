---
severity: low
surfaces: [ci, developer]
adopted: 2026-07-26
adopted-because: actions/cache derives its version from the requested path set, so a fork pull request asking for target/ cannot restore the registry-only tarball the default branch now saves
lift-when-class: infra-gap-closes
lift-when: fork pull requests restore a compilation cache again, whether by a fork-safe read path to the object store, a separate default-branch save that includes target/, or a measurement showing forks are cold rarely enough not to matter
---

# Fork pull requests lose their compilation cache

## Constraint

`backend / checks` asks `Swatinem/rust-cache` for the registry only
wherever the object store is reachable, and for `target/` as well where
it is not:

```yaml
cache-targets: ${{ secrets.R2_ACCOUNT_ID == '' }}
```

A fork pull request cannot read repository secrets, so it takes the
second branch and requests `target/`. That was intended to leave forks
exactly where they were before the object store existed.

It does not. `actions/cache` derives a cache version from the set of
paths requested, and only restores an entry saved under the same
version. The default branch, which has the secret, now saves
registry-only entries. A fork asking for `target/` computes a different
version and matches none of them.

Saving does not compensate: `save-if` is restricted to the default
branch, so no fork run has ever written an entry of its own.

Full-target entries saved before the object store landed are still
restorable, which is why this has not yet been observed. They expire
seven days after last access.

## Workaround

None. Fork pull requests compile cold once the last full-target
generation expires, at roughly 786s for `backend / checks` against 194s
warm.

## Why this isn't the right shape

External contributors get the slowest possible CI, and the cause is a
detail of how `actions/cache` versions entries rather than a decision
anyone made. The condition is also invisible: the job passes, no warning
is emitted, and the only symptom is duration.

This repository accepts external contributions and documents the process
in `.github/CONTRIBUTING.md`, so the affected path is one the project
intends to support.

## Lift conditions

Any of:

- fork runs reach the object store through a fork-safe read path;
- the default branch additionally saves a full-target entry that fork
  runs can match;
- measurement shows fork pull requests are rare enough that a cold
  compile is acceptable, recorded here and this entry purged.

## Related

- `adr/2026-07-26-remote-build-cache-on-r2.md`
