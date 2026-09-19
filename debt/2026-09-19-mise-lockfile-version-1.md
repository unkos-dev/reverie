---
severity: low
surfaces: [developer, ci]
adopted: 2026-09-19
adopted-because: Renovate's mise manager commits only mise.lock after running mise lock, so a version-2 lockfile's yamllint dependency sidecar under .mise/locks/ would go stale on every Renovate yamllint update and fail the locked CI install
lift-when-class: dep-unblocks
lift-when: the Renovate release Mend runs commits the .mise/locks/ sidecar directories its mise lock run writes, at which point `mise lock --upgrade` moves the lockfile to version 2 and the sidecar is committed
---

# mise lockfile held at version 1

`mise.lock` is a version-1 lockfile. The current mise writes version 2 for a new lockfile and keeps an older format
through ordinary updates, so the file was seeded with `lockfile_version = 1` before the first `mise lock` and stays at
that version until `mise lock --upgrade` is run deliberately.

Version 1 records a checksum and download URL for every tool whose backend publishes an artifact, and a version only for
`cargo:cargo-mutants` and `pypi:yamllint`. CI installs with `--locked`, which `jdx/mise-action` adds whenever a lockfile
is present, so every artifact download is verified and yamllint resolves its own Python dependencies fresh on each
install, as it did before the lockfile existed.

Version 2 would freeze yamllint's dependency graph in `.mise/locks/yamllint/<version>/` and reference it from
`mise.lock` by path and digest. Renovate runs `mise lock` when it updates a tool but commits only `mise.lock`, so each
yamllint update would arrive without its sidecar and the locked install would fail until someone ran `mise lock` on the
branch by hand.

Lift this entry by running `mise lock --upgrade`, committing `mise.lock` and `.mise/locks/`, and confirming the next
Renovate yamllint update carries a regenerated sidecar.
