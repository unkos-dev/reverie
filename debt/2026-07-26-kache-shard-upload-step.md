---
severity: low
surfaces: [ci]
adopted: 2026-07-26
adopted-because: kache-action is a JavaScript action, so its post step runs from $GITHUB_WORKSPACE while the cargo workspace lives in backend/; shard upload is conditional on finding a Cargo.lock and is skipped there
lift-when-class: upstream
lift-when: kache-action accepts a working directory for its post step, or resolves the manifest path itself, at which point the explicit step is deleted
---

# Prefetch shards uploaded by a hand-written step

## Constraint

`kunobi-ninja/kache-action` runs `kache save-manifest` in its post step to
upload the content-addressed shards prefetch reads. Shard upload is
conditional on a `Cargo.lock` being present in the working directory.

The action is a JavaScript action, so its post step executes from
`$GITHUB_WORKSPACE`. A job's `defaults.run.working-directory` applies to
`run` steps only and does not move it. This repository's cargo workspace
lives in `backend/`, so the post step finds no lockfile at the repository
root and logs `No Cargo.lock found, skipping shard upload`.

The namespace input reaches kache correctly and the wrapper reports
`shard context: available`, so nothing in the configuration is wrong. The
upload never happens.

## Workaround

`.github/workflows/ci.yml` runs `kache save-manifest` as an ordinary step
after the compiling steps, where it inherits `backend/` from the job's
working directory. It passes no arguments: the action's setup step
exports `KACHE_NAMESPACE` and the remote settings into the job
environment, so the namespace stays declared once. The step is confined
to the default branch, matching every other write to the store.

The action's own post step still runs and still skips, harmlessly.

## Why this isn't the right shape

Two steps now invoke the same command for the same purpose in the same
job, and only one of them accomplishes it. A reader has to know why the
second one exists to understand why the first is left in place. The step
also depends on the action continuing to export `KACHE_NAMESPACE` under
that name, which is not part of any documented contract.

## Lift conditions

Upstream exposes a working directory or manifest path for the post step,
or resolves the workspace root itself. The explicit step is then deleted
and the action's post step does the upload.

## Related

- `adr/2026-07-26-remote-build-cache-on-r2.md`
- `debt/2026-07-26-kache-action-digest-pinned-past-tag.md`
