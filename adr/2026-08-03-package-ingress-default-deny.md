---
status: accepted
date: 2026-08-03
supersedes: []
decision-makers: "John Unkovich"
consulted: []
informed: "Reverie contributors"
---

# Package ingress: default-deny controls, never per-package allowances

## Context and Problem Statement

Reverie takes foreign code from four registries: npm for the frontend and docs
workspaces, crates.io for the backend, the mise backends for the pinned CLI
tools, and a container registry for the runtime base image. Every install is a
point where someone else's code and data enter the tree, and under npm it is
also a point where someone else's code runs.

The controls over that boundary were written one package at a time, and two of
the three `allowScripts` entries had stopped matching anything:

- `puppeteer@25.3.0` was pinned while the tree resolved 25.4.0. npm treats an
  entry that matches nothing as unreviewed rather than denied, and npm 11 ran
  unreviewed scripts, so that postinstall fetched a browser on every install.
  The entry read as a control while acting as none.
- `esbuild@0.28.1` has not failed yet but is the same shape. esbuild appears in
  no manifest except that key, resolving through three ranges the repo does not
  own, so the next lockfile refresh that moves it voids the entry.

Both are keyed on a version the repo does not control, and both stop applying
without saying so. The boundary needs controls that cannot lapse this way.

## Decision Drivers

- A control that stops applying must fail loudly rather than decay into no
  control.
- No control may rest on per-package bookkeeping keyed on a version this repo
  does not pin.
- Enforcement is mechanical, not reviewer memory.
- One policy across ecosystems, so a contributor learns the boundary once.
- Legitimate needs stay reachable: a dependency that cannot work without its
  install script must remain installable.

## Considered Options

- Global default-deny controls, with per-package exceptions only where a
  package cannot function without one
- Per-package allowlists pinned to reviewed versions
- The package managers' own defaults, unmodified

## Decision Outcome

Chosen option: "global default-deny controls", because a default is the only
form no version can void by moving underneath it. Pinned per-package
allowlists were the status quo and produced both failures above. Bare manager
defaults come close under npm 12, which blocks unreviewed scripts, but they
block in silence, so a script some package needed resurfaces later as an
unrelated runtime fault.

Five controls govern package ingress. Each is global, and each is enforced by
the toolchain rather than at review.

1. **The manager is pinned and enforced.** pnpm through `packageManager`,
   which a disagreeing pnpm re-executes rather than ignores; the JavaScript
   runtime through `devEngines.runtime`, which pnpm and vp both read and which
   `pnpm-lock.yaml` carries per-platform checksums for; the Rust toolchain and
   MSRV through CI. An unpinned manager means an unknown set of security
   defaults.
2. **Installs are frozen.** `pnpm install --frozen-lockfile` and
   `cargo --locked`. A resolving install
   on a build or deploy path can take a version the lockfile never recorded.
3. **No install-time code execution.** `allowBuilds` records a decision per
   package, and `strictDepBuilds` fails the install on anything undecided. The
   policy sits in `pnpm-workspace.yaml` because
   [2026-06-30-adopt-vite-plus-monorepo-toolchain.md](2026-06-30-adopt-vite-plus-monorepo-toolchain.md)
   put one lockfile and one workspace declaration at the root.
4. **New versions wait.** Renovate holds an update for `minimumReleaseAge`,
   currently 3 days. A compromised release is usually yanked soon after
   discovery, so waiting removes most of that exposure and asks nothing of
   per-package judgement.
5. **Every ingested tree is audited.** `cargo deny` over `Cargo.lock`, the Snyk
   lanes and licence policy over npm, and a CycloneDX SBOM shipped inside the
   image.

Entries take the shape the tooling can police, following
[2026-07-04-expect-over-allow-lint-suppressions.md](2026-07-04-expect-over-allow-lint-suppressions.md):
a version-pinned allow entry is the `#[allow]` of package policy, documenting
intent while rotting silently. `allowBuilds` keys its entries by package name,
so a decision cannot quietly stop matching when a version range moves.

No package is allowed to run an install script. Six packages in the tree
declare one, three of them `fsevents` builds that install on macOS alone. Of
the remainder, esbuild and lefthook ship their real binaries as platform
`optionalDependencies` and both run correctly with every script blocked, and
puppeteer's script fetches a browser for a URL-scanning path this repo never
invokes. All three are denied by name.

### Consequences

- Good, because no entry is keyed on a version the repo does not own, so a
  lockfile refresh can no longer void the policy.
- Good, because a dependency that starts shipping an install script fails the
  install instead of running, or being skipped with nobody the wiser.
- Bad, because a one-off `vp dlx <tool>` fetches a package this policy has not
  ruled on. It runs outside the workspace install, so `allowBuilds` does not
  cover it; treat an ad-hoc fetch as its own decision.
- Bad, because a Renovate refresh introducing a script-bearing package turns
  that pull request red until someone rules on it. That signal is the point,
  and it still adds a step to an otherwise automerged lane.
- Neutral, because the image build is untouched: it copies the manifests and
  lockfile alone, and every install it runs passes `--ignore-scripts`.
- Neutral, because cargo has no install-script equivalent, so control 3 reads
  as npm-specific while the other four span both stacks.
- Neutral, because control 2 keeps one named gap: the CI steps that
  `cargo install sqlx-cli` resolve that tool's own dependencies fresh instead
  of from its lockfile, with the version pin held in lockstep with the sqlx
  crate by its own CI check. Every other resolving cargo invocation across
  the just recipes, the git hooks, the workflow files, and the image build
  passes `--locked`, and `scripts/cargo-locked-guard.sh` fails the lint gate
  when one arrives without it, treating unknown cargo subcommands as
  resolving until ruled otherwise. The remaining gap is narrower than the npm path, which no
  longer has a resolving install anywhere.

### Confirmation

`strictDepBuilds` fails any install whose dependencies carry a build script
not ruled on in `allowBuilds`. Entries are keyed by package name rather than by
resolved version, so a decision cannot silently stop matching when a range
moves. On the cargo side, `scripts/cargo-locked-guard.sh` fails
the lint gate for any resolving invocation without `--locked` across the
recipes, hooks, workflows, and image build, treating unknown subcommands as
resolving.

## More Information

Applies the self-purging shape from
[2026-07-04-expect-over-allow-lint-suppressions.md](2026-07-04-expect-over-allow-lint-suppressions.md)
to package ingress, and inherits its policy location from
[2026-06-30-adopt-vite-plus-monorepo-toolchain.md](2026-06-30-adopt-vite-plus-monorepo-toolchain.md).
Corrects the supply-chain consequence in
[2026-05-21-impeccable-adoption.md](2026-05-21-impeccable-adoption.md), which
named a `skipDownload` field no manifest carries.

Revisit if npm gains per-workspace script policy, or if a dependency arrives
that cannot work without its install script: a non-empty allowlist makes the
name-only rule load-bearing rather than theoretical.
