# Contributing to Reverie

Thanks for your interest in contributing. Reverie is a self-hosted ebook library manager built for the open-source self-hosting community. The project is pre-v1.0 and opinionated: not every proposal will fit the direction, and the maintainer may close issues or PRs that are outside scope. If you're unsure whether an idea fits, open a discussion or a lightweight issue before sinking time into code.

## Community standards

This project follows the project [Code of Conduct](CODE_OF_CONDUCT.md). Participation in issues, PRs, and discussions is expected to meet its standards.

**Security issues are reported privately, not through issues.** Use [GitHub Security Advisories](https://github.com/unkos-dev/reverie/security/advisories/new). See [SECURITY.md](SECURITY.md) for scope, response timeframes, and the project's threat model.

## Developer Certificate of Origin

Contributions are accepted under the [Developer Certificate of Origin v1.1](https://developercertificate.org/) (DCO). You keep the copyright of your work; every contribution is licensed to the project under the same AGPL-3.0 terms the project ships under (inbound = outbound). Signing off certifies that you wrote the contribution, or otherwise have the right to submit it under that license.

Add the trailer to each commit with `git commit -s` (or `--signoff`):

```text
Signed-off-by: Your Name <your-email@example.com>
```

CI blocks pull requests that contain commits without the trailer, and the repository's commit-msg hook rejects an unsigned commit locally before it is created.

## Commit messages and branches

This project uses [Conventional Commits](https://www.conventionalcommits.org/). All commit messages follow:

```text
<type>(<scope>): <description>
```

The accepted types are the [config-conventional](https://github.com/conventional-changelog/commitlint/tree/master/%40commitlint/config-conventional) set, enforced by commitlint on every commit: `build`, `chore`, `ci`, `docs`, `feat`, `fix`, `perf`, `refactor`, `revert`, `style`, `test`.

Branch names use the same type as their prefix (`feat/`, `fix/`, `refactor/`, and so on through the same list). Breaking changes append `!` after the type or scope and explain the break in a `BREAKING CHANGE:` footer. Reverts use the `revert` type with a footer naming the commit being reverted (`Refs: <sha>`).

Pull request titles follow the same format: squash merging makes the title the commit subject on `main`, and a CI check lints it with the same commitlint config.

## Development setup

Simplest path, full stack in Docker:

```bash
git clone https://github.com/unkos-dev/reverie.git
cd reverie
docker compose -f docker/compose.dev.yml up
```

> Upgrading a dev checkout from before the postgres:18 mount-layout fix? Drop
> the old volume first:
> `docker compose -f docker/compose.dev.yml down && docker volume rm reverie_pgdata`
> (The compose project name is pinned, so the volume is `reverie_pgdata`
> regardless of the checkout directory. A stack created before the pin is
> labelled with a directory-derived project instead: find its volume with
> `docker volume ls | grep pgdata` and take that stack down with
> `docker compose -p <project> -f docker/compose.dev.yml down`.)

Set `REVERIE_COMPOSE_ENV` to run a second, deliberately separate stack with
its own volume and database (`REVERIE_COMPOSE_ENV=stage` gives project
`reverie_stage` and volume `reverie_stage_pgdata`). Export it in your shell or
put it in `docker/.env`, which is the dotenv file Compose loads for this
project; the backend discovers no env file of its own, so a repository
`.env` has no effect on it. Environment stacks still share the published
`127.0.0.1:5432`, so only one can run at a time and the port bind conflict is
the guard.

Backend only (requires the Rust toolchain; the minimum supported version is declared as `rust-version` in [`backend/Cargo.toml`](../backend/Cargo.toml) and enforced in CI):

```bash
just rust::dev
```

> Run `just rust::migrate` once to initialise the schema before the first
> `just rust::dev`; the server verifies the schema and refuses to start if it
> is fresh or behind. Both recipes source the dev env file described below;
> a bare `cd backend && cargo run` has no config unless you export it
> yourself.

Native toolchains, whole stack in the background:

```bash
just dev-up      # dev Postgres, migrations, backend API, Vite
just dev-status  # both planes; nonzero when either is down
just dev-down    # stops both servers, leaves Postgres up
```

`dev-up` is safe to re-run: it is idempotent by construction rather than by
checking what is already running. Each server logs to `.dev-server.log` in its
own plane directory. The database deliberately survives `dev-down`, because it
is stateful and shared with the test suite; stop it with `just db-down`.

The backend recipes supply the dev configuration the server needs when nothing
else does: the RLS-enforced `reverie_app` DSN and the `REVERIE_PUBLIC_URL` that
OPDS requires. They resolve an out-of-tree env file at `~/reverie/dev/env`
(override the location with `REVERIE_DEV_ENV`; copy `.env.example` there to
start one), so a value you set there is the one the server uses.

Frontend only (Node.js at or above the `engines.node` floor in `package.json`; install at the repository root, where npm workspaces hoist every plane's dependencies):

```bash
npm ci && npm run dev --workspace frontend
```

Subsystem conventions (database roles, testing helpers, linting rules) are documented in [backend/AGENTS.md](../backend/AGENTS.md) and [frontend/AGENTS.md](../frontend/AGENTS.md).

Contributor automation conventions live in [`AGENTS.md`](../AGENTS.md) files (the [agents.md](https://agents.md) standard). Compatibility shims may import those files, but they do not define separate policy.

### Pre-commit prerequisites

Git hooks are managed by lefthook and installed through the `prepare` npm script, so a fresh clone wires them on `npm ci`.

Install the repository-pinned hook and local-check tools with [mise](https://mise.jdx.dev/):

```sh
mise install actionlint gitleaks hadolint just node shellcheck typos vale yamllint \
  npm:npm github:nextest-rs/nextest github:taiki-e/cargo-llvm-cov
```

`just` is the task runner for every plane, and the lint, format, test, and build definitions CI uses live in the justfiles rather than inline in the workflows. Run `just --list` for the recipe list, or read the generated [just recipe reference](https://unkos-dev.github.io/reverie/reference/just/) for the full set with descriptions. `just worktree <branch>` creates a worktree outside the checkout, where it cannot enter the Docker build context or cargo's workspace discovery; set `WORKTREE_ROOT` to choose where those live.

[`mise.toml`](../mise.toml) pins those contributor tools and the additional CI-only Rust tools to the versions enforced in CI. The scoped command avoids installing cargo-machete, cargo-deny, and cargo-mutants for contributors because local recipes do not use them. npm is pinned there too, matching the `devEngines.packageManager` version in [`package.json`](../package.json): npm enforces that declaration on every direct invocation and refuses to run when the binary on `PATH` disagrees, so an unpinned npm blocks `npm install` and every hook that shells out to it. `scripts/npm-pin-drift.sh` holds the two copies in lockstep. Node is pinned in `mise.toml` too and installed with the tools above; vite-plus remains a lockfile-managed project dependency. If a declared project dependency is missing, restore it through the documented lockfile-backed setup command. If a system prerequisite is unavailable, stop the affected check and report the missing command; do not bypass the check or install system packages implicitly.

Workflow and infrastructure files are additionally scanned in CI by zizmor, Checkov, Trivy, CodeQL, cargo-audit, cargo-deny, and dependency-review. These are intentionally CI-only scanners. Local installation is not part of contributor setup. Documented zizmor suppressions and their justifications live in [`.github/zizmor.yml`](zizmor.yml).

[Snyk](https://snyk.io) also scans every PR as an advisory (non-blocking) layer: Snyk Code runs static analysis over the Rust and TypeScript sources, and Snyk Open Source scans the npm lockfile for vulnerable or license-incompatible dependencies. Findings surface on the repository's code-scanning dashboard and as PR annotations; they never fail the check. Analysis runs on Snyk's managed SaaS (see their [privacy and data handling terms](https://snyk.io/policies/privacy/)); the code it receives is already public. Like the scanners above, Snyk is CI-only and not part of contributor setup.

### CI toolchain pins

Rust itself is pinned in [`backend/rust-toolchain.toml`](../backend/rust-toolchain.toml). rustup reads that file for every cargo invocation under `backend/`, so a contributor's build and a CI build use the same compiler; before it, each side tracked `stable` on its own schedule and a release could surface new lints on one side weeks before the other. CI installs that same version through [`.github/actions/rust-toolchain`](actions/rust-toolchain/action.yml), a local composite action that reads the channel from the file and calls the rustup every runner preinstalls, so the version is never named a second time where it could drift. It replaced a third-party action that published its releases as long-lived branches upstream rewrites; once the pinned commit was no longer reachable from any branch there, the SHA pin had stopped identifying auditable upstream code. Renovate's `rust-toolchain` manager raises the bump PRs. The pin is not the minimum supported version: `rust-version` in [`backend/Cargo.toml`](../backend/Cargo.toml) stays the supported floor, and the MSRV job overrides the file through `RUSTUP_TOOLCHAIN` so it still compiles against that floor rather than against the pinned version.

CI also keeps a content-addressed Rust build cache in object storage, installed by `kunobi-ninja/kache-action` at the version pinned for `kache` in [`mise.toml`](../mise.toml) and read from there rather than named a second time, so CI and a contributor's machine stay on one version. It runs alongside `Swatinem/rust-cache`, which now carries the cargo registry only. Write credentials are reserved for pushes to `main`; pull requests receive object-read credentials and run with the remote in read-only mode, so a branch cannot write into the store the default branch restores from. The rationale, the alternatives, and the measured numbers behind it are in [`adr/2026-07-26-remote-build-cache-on-r2.md`](../adr/2026-07-26-remote-build-cache-on-r2.md).

CI installs vp through [`voidzero-dev/setup-vp`](https://github.com/voidzero-dev/setup-vp). No workflow pins the vp version: setup-vp resolves it from the root [`package.json`](../package.json)'s `vite-plus` devDependency when the action's `version:` input is omitted, so the npm `vite-plus` bump is the only place that pin moves. Node is pinned once in [`mise.toml`](../mise.toml), the same table that pins npm; every job that installs vp provisions node from that pin through `jdx/mise-action` before the setup-vp step runs, and setup-vp is told `node-manager: false` so it never installs a node of its own. Renovate raises node bump PRs through the same mise manager it uses for every other `mise.toml` pin. On a `vite-plus` bump, re-check the `dependency-review` `allow-ghsas` list in [`ci.yml`](workflows/ci.yml) against new advisories for the aliased vite package; a vp bump is the deliberate review trigger recorded in [`debt/2026-06-30-vite-plus-alias-dependency-review.md`](../debt/2026-06-30-vite-plus-alias-dependency-review.md).

## Testing requirements

**Tests are mandatory for the shipped product.** No feature or bug fix in `backend/` or `frontend/` is complete without tests. Follow the test-first pattern:

- **Happy path**: expected behaviour works
- **Negative cases**: invalid input is rejected, error paths are exercised
- **Edge cases**: where the behaviour is non-obvious

PRs touching product code without tests will not be approved.

Executable tooling elsewhere in the tree (`scripts/`, the justfiles, `.github/`, `docker/`, the root configs) is judged on whether it can fail quietly rather than on a fixed test requirement: a guard a pull request exercises and that fails loudly needs no separate test, while a check that can pass while matching nothing needs an assertion inside it. `AGENTS.md` hard rule 5 is the single source for both halves.

## Accessibility

Reverie targets **WCAG 2.2 Level AA** as a design invariant. The review process is recorded in [`adr/superseded/2026-06-05-accessibility-review-process.md`](../adr/superseded/2026-06-05-accessibility-review-process.md) (superseded on the gate mechanism by [`adr/2026-07-13-a11y-gate-on-playwright.md`](../adr/2026-07-13-a11y-gate-on-playwright.md), which moves the gate onto the Playwright stack): what the automated gate covers, what the manual audit owns, the audit cadence, and how the one accepted brand carve-out (Reverie Gold on large CTAs) is documented.

For frontend changes, the `a11y` CI job runs axe-core against the shipped routes listed in `DEFAULT_TARGETS` (`frontend/scripts/a11y/allowlist.mjs`) and fails on any WCAG 2.2 AA violation outside the documented allowlist in that same file. Run it locally with `npm run a11y` from `frontend/`. Do so with no dev server already on port 5173: Playwright reuses one it finds without checking which checkout owns it, so a server left running from elsewhere gets scanned instead, and the result reads as though it described your branch. To scan a specific server, start it yourself and pass `A11Y_BASE_URL`. UI-touching PRs also carry an accessibility checklist in the PR template.

## Pull request process

1. Create a feature branch from `main` using the appropriate prefix
2. Write tests for your changes (see above)
3. Ensure all CI checks pass locally (`cargo fmt --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo test`, `npm run lint`, `npm test`, `npm run build` as applicable)
4. Open the PR and fill in **Summary** and **Test plan**. Keep **Why**, **Accessibility**, and issue closure sections only when relevant; delete unused sections instead of writing placeholders or `N/A`.
5. Labels auto-apply based on paths touched; no manual labelling needed
6. Wait for maintainer review and approval

## Third-party AI code review

This repository uses a third-party AI code reviewer that auto-comments on pull requests. By opening a PR you accept that the diff and surrounding repository context will be sent to it for analysis.

Active reviewer:

- [Greptile](https://www.greptile.com): graph-based codebase context. See [security disclosures](https://www.greptile.com/security)

Data handling:

- Greptile is a managed SaaS provider; inference runs through OpenAI's and Anthropic's API platforms, per their security disclosure. Repository code is cached on their infrastructure while the GitHub App has access; cache is deleted on App uninstall per their retention policy
- Reverie is AGPL-3.0 and the code the reviewer receives is already public, so the marginal exposure is near zero; the disclosure exists for transparency
- **AI-training opt-in.** Reverie uses Greptile under their "free for open-source" arrangement, and as a token form of reciprocity, this repository has training-data use enabled at the account level. Per Greptile's policy this means de-identified, aggregated repository data may be used to monitor, improve, or expand their services. PII and customer-specific references are stripped per their disclosure
- **External contributions.** If active external contributions start arriving, the training opt-in is reconsidered with those contributors in the loop. Reverie remains AGPL-3.0

Reviewer findings are advisory: address actionable ones in follow-up commits, dismiss the rest with a brief note. Maintainer review remains the only merge gate.

## Dependencies

Dependency updates are managed by [Renovate](https://docs.renovatebot.com/) on a weekly schedule. **Don't file separate PRs for dependency bumps** unless you're patching a security advisory that Renovate hasn't yet flagged. Security-related dependency updates bypass the weekly schedule and land whenever the advisory is published.

New Rust dependencies must satisfy the supply-chain policy in [`backend/deny.toml`](../backend/deny.toml): a crate whose license is outside the permissive allowlist (any GPL/LGPL/AGPL or otherwise unlisted license) or that resolves to a git source rather than crates.io will fail the `cargo deny check` run in the `audit` CI job. If you have a legitimate need for such a dependency, raise it in the PR so the policy exception can be reviewed.

No npm dependency runs an install script. `allowScripts` in the root `package.json` denies every package that ships one, and `strict-allow-scripts` in the root `.npmrc` fails the install on anything unreviewed, so a dependency that starts shipping a script stops CI until someone rules on it. [The package-ingress ADR](../adr/2026-08-03-package-ingress-default-deny.md) records why.

To clear one: read what the script does, then run `npm install-scripts deny <pkg>` if the package works without it. That is the usual answer, because native tooling now ships its binaries as platform `optionalDependencies` and keeps the script only as a fallback. If the package cannot work without it, `npm install-scripts approve <pkg>` records that instead, but approve pins to the resolved version, so reserve it for a package this repo pins itself: on a transitive one the pin stops matching the first time the range moves, and a pinned entry that matches nothing is not a denial. `npm install-scripts ls` shows what is awaiting a decision. Commit the `package.json` change in the same PR.
