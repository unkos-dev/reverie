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

1. **The manager is pinned and enforced.** npm through `devEngines` and
   `mise.toml` held in lockstep; the Rust toolchain and MSRV through CI. An
   unpinned manager means an unknown set of security defaults.
2. **Installs are frozen.** `npm ci` and `cargo --locked`. A resolving install
   on a build or deploy path can take a version the lockfile never recorded.
3. **No install-time code execution.** `allowScripts` denies by default, and
   `strict-allow-scripts` fails the install on anything unreviewed. The policy
   sits in the root `.npmrc` and root `package.json` because
   [2026-06-30-adopt-vite-plus-monorepo-toolchain.md](2026-06-30-adopt-vite-plus-monorepo-toolchain.md)
   put one lockfile and one hoisted `node_modules` at the root.
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
intent while rotting silently. An `allowScripts` entry is therefore name-only
unless this repo pins that package's version itself.

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
- Bad, because `npx <tool>` inside the checkout fails for a tool carrying an
  install script. `npm exec` ignores the project `allowScripts` by design while
  still reading strict mode from `.npmrc`, so the gate applies with no policy
  behind it; `--allow-scripts=<pkg>` covers the one-off.
- Bad, because a Renovate refresh introducing a script-bearing package turns
  that pull request red until someone rules on it. That signal is the point,
  and it still adds a step to an otherwise automerged lane.
- Neutral, because the image build is untouched: it copies the manifests and
  lockfile alone, and every install it runs passes `--ignore-scripts`.
- Neutral, because cargo has no install-script equivalent, so control 3 reads
  as npm-specific while the other four span both stacks.
- Neutral, because control 2 is not yet uniform: the CI steps that
  `cargo install sqlx-cli` resolve that tool's own dependencies fresh instead
  of from its lockfile, and the coverage and doctest recipes omit `--locked`.
  Both are narrower than the npm path, which no longer has a resolving install
  anywhere, and both are named here so the gap is a known one.

### Confirmation

`strict-allow-scripts` in the root `.npmrc` fails any install whose
dependencies carry an install script not covered by `allowScripts`. Every
`allowScripts` entry is name-only, or pinned to a version this repo declares
itself; a pinned entry naming a transitive package is the defect this record
exists to prevent.

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
