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

Branch names use the same type prefix: `feat/`, `fix/`, `refactor/`, `docs/`, `chore/`, `test/`, `perf/`. Breaking changes append `!` after the type or scope and explain the break in a `BREAKING CHANGE:` footer.

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
> (Compose prefixes volume names with the checkout directory name; if yours
> differs, `docker volume ls | grep pgdata` finds the actual name.)

Backend only (requires the Rust toolchain; the minimum supported version is declared as `rust-version` in [`backend/Cargo.toml`](../backend/Cargo.toml) and enforced in CI):

```bash
cd backend && cargo run
```

> Run `cargo run -- migrate` once to initialise the schema before the first
> `cargo run`; the server verifies the schema and refuses to start if it is
> fresh or behind.

Frontend only (Node.js at or above the `engines.node` floor in `package.json`):

```bash
cd frontend && npm install && npm run dev
```

Subsystem conventions (database roles, testing helpers, linting rules) are documented in [backend/AGENTS.md](../backend/AGENTS.md) and [frontend/AGENTS.md](../frontend/AGENTS.md).

Agent conventions live in [`AGENTS.md`](../AGENTS.md) files (the [agents.md](https://agents.md) standard), so any coding agent picks them up. The `CLAUDE.md` files are one-line import shims that point Claude Code at the same content.

### Pre-commit prerequisites

Git hooks are managed by lefthook and installed through the `prepare` npm script, so a fresh clone wires them on `npm ci`.

The lefthook pre-commit hook runs [`actionlint`](https://github.com/rhysd/actionlint) on changed GitHub Actions workflow files. Install it once before your first commit; CI pins the enforced version in [`ci.yml`](workflows/ci.yml), and the command below installs that pin:

```bash
# Linux + macOS. Download the release tarball directly: Homebrew's formula is
# not version-pinned, so it can drift from the lint chain enforced in CI.
curl -fsSL "https://github.com/rhysd/actionlint/releases/download/v1.7.12/actionlint_1.7.12_$(uname -s | tr 'A-Z' 'a-z')_$(uname -m | sed 's/x86_64/amd64/; s/aarch64/arm64/').tar.gz" \
  | tar -xz -C "$HOME/.local/bin" actionlint
```

The hook also runs [`yamllint`](https://www.yamllint.com/) on changed `*.{yml,yaml}` files (version pinned in CI). It is pip-installable:

```bash
pipx install yamllint==1.33.0
```

The hook runs the frontend linters [`oxlint`](https://oxc.rs) and [`stylelint`](https://stylelint.io) through [`just`](https://just.systems), the repository's task runner, so the [`js.just`](../js.just) recipes stay the single source of truth for how each linter runs. Install `just` once (it is not version-pinned):

```bash
cargo install just   # or: brew install just, your distro package manager, https://just.systems
```

If `actionlint`, `yamllint`, or `just` is not on `PATH`, the pre-commit hook fails with a clear `command not found`. CI re-runs the same checks, so a bypass (`--no-verify` or missing-binary skip) is still caught before merge.

Workflow files are additionally scanned in CI by [zizmor](https://github.com/zizmorcore/zizmor) (the merge-blocking `workflow-security` job) for GitHub Actions security issues: credential persistence, template injection, cache poisoning, and dangerous triggers. It is a CI-only tool, so there is nothing to install locally; documented suppressions and their justifications live in [`.github/zizmor.yml`](zizmor.yml).

### CI toolchain pins

CI installs vp and node through [`voidzero-dev/setup-vp`](https://github.com/voidzero-dev/setup-vp), reading two workflow env vars: `VP_VERSION` (the global vp) and `NODE_VERSION`. Both carry `# renovate:` annotations, so Renovate raises bump PRs; `VP_VERSION` and the npm `vite-plus` devDependency share the grouped `vite-plus` PR, and the `repo-lint` drift guard fails the build if they diverge. On a `vite-plus` bump, re-check the `dependency-review` `allow-ghsas` list in [`ci.yml`](workflows/ci.yml) against new advisories for the aliased vite package; a vp bump is the deliberate review trigger recorded in [`debt/2026-06-30-vite-plus-alias-dependency-review.md`](../debt/2026-06-30-vite-plus-alias-dependency-review.md).

## Testing requirements

**Tests are mandatory.** No feature or bug fix is complete without tests. Follow the test-first pattern:

- **Happy path**: expected behaviour works
- **Negative cases**: invalid input is rejected, error paths are exercised
- **Edge cases**: where the behaviour is non-obvious

PRs without tests will not be approved.

## Accessibility

Reverie targets **WCAG 2.2 Level AA** as a design invariant. The review process is recorded in [`adr/2026-06-05-accessibility-review-process.md`](../adr/2026-06-05-accessibility-review-process.md): what the automated gate covers, what the manual audit owns, the audit cadence, and how the one accepted brand carve-out (Reverie Gold on large CTAs) is documented.

For frontend changes, the `a11y` CI job runs axe-core against the design showcase and fails on any WCAG 2.2 AA violation outside the documented allowlist (`frontend/scripts/a11y/allowlist.mjs`). Run it locally with `npm run a11y` (from `frontend/`, with the dev server up). UI-touching PRs also carry an accessibility checklist in the PR template.

## Pull request process

1. Create a feature branch from `main` using the appropriate prefix
2. Write tests for your changes (see above)
3. Ensure all CI checks pass locally (`cargo fmt --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo test`, `npm run lint`, `npm test`, `npm run build` as applicable)
4. Open the PR and fill in the template's **Summary**, **Why** (if motivation isn't obvious from the diff), and **Test plan**
5. Labels auto-apply based on paths touched; no manual labelling needed
6. Wait for maintainer review and approval

## Third-party AI code review

This repository uses third-party AI code reviewers that auto-comment on pull requests. By opening a PR you accept that the diff and surrounding repository context will be sent to the active reviewers for analysis.

Active reviewers:

- [Greptile](https://www.greptile.com): graph-based codebase context. See [security disclosures](https://www.greptile.com/security)
- [CodeRabbit](https://www.coderabbit.ai): line-level inline review with formal GitHub PR Review status. See [security and trust](https://www.coderabbit.ai/trust-center)

General data handling (both reviewers):

- Both are managed SaaS providers; inference runs through third-party LLM platforms (OpenAI, Anthropic, Google). Repository code is cached on their infrastructure while their GitHub Apps have access; cache is deleted on App uninstall per each provider's retention policy
- Reverie is AGPL-3.0 and the code these reviewers receive is already public, so the marginal exposure is near zero; the disclosures exist for transparency

Reviewer-specific notes:

- **Greptile AI-training opt-in.** Reverie uses Greptile under their "free for open-source" arrangement, and as a token form of reciprocity, this repository has training-data use enabled at the account level. Per Greptile's policy this means de-identified, aggregated repository data may be used to monitor, improve, or expand their services. PII and customer-specific references are stripped per their disclosure
- **CodeRabbit AI-training default.** CodeRabbit's OSS terms do not enable training on repository data by default. Reverie does not change that default
- **External contributions.** If active external contributions start arriving, the Greptile training opt-in is reconsidered with those contributors in the loop. Reverie remains AGPL-3.0

Reviewer findings are advisory: address actionable ones in follow-up commits, dismiss the rest with a brief note. Maintainer review remains the only merge gate.

## Dependencies

Dependency updates are managed by [Renovate](https://docs.renovatebot.com/) on a weekly schedule. **Don't file separate PRs for dependency bumps** unless you're patching a security advisory that Renovate hasn't yet flagged. Security-related dependency updates bypass the weekly schedule and land whenever the advisory is published.

New Rust dependencies must satisfy the supply-chain policy in [`backend/deny.toml`](../backend/deny.toml): a crate whose license is outside the permissive allowlist (any GPL/LGPL/AGPL or otherwise unlisted license) or that resolves to a git source rather than crates.io will fail the `cargo deny check` run in the `audit` CI job. If you have a legitimate need for such a dependency, raise it in the PR so the policy exception can be reviewed.
