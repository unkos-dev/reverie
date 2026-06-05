# Contributing to Reverie

Thanks for your interest in contributing. Reverie is a self-hosted ebook library manager built for the open-source self-hosting community. The project is pre-v1.0 and opinionated — not every proposal will fit the direction, and the maintainer may close issues or PRs that are outside scope. If you're unsure whether an idea fits, open a discussion or a lightweight issue before sinking time into code.

## Community standards

This project follows the project [Code of Conduct](CODE_OF_CONDUCT.md). Participation in issues, PRs, and discussions is expected to meet its standards.

**Security issues are reported privately, not through issues.** Use [GitHub Security Advisories](https://github.com/unkos-dev/reverie/security/advisories/new). See [SECURITY.md](SECURITY.md) for scope, response timeframes, and the project's threat model.

## Contributor License Agreement

By submitting a pull request, you agree to assign copyright of your contribution to the project maintainer (John Unkovich). This preserves the option to dual-license in the future while keeping the project AGPL-3.0 for the community. Acceptance is implicit by the act of submitting a PR — no separate signature needed.

## Commit messages and branches

This project uses [Conventional Commits](https://www.conventionalcommits.org/). All commit messages follow:

```text
<type>(<scope>): <description>
```

Branch names use the same type prefix: `feat/`, `fix/`, `refactor/`, `docs/`, `chore/`, `test/`, `perf/`. See [CLAUDE.md](../CLAUDE.md) for the full specification, examples, and breaking-change conventions.

## Development setup

Simplest path — full stack in Docker:

```bash
git clone https://github.com/unkos-dev/reverie.git
cd reverie
docker compose up
```

Backend only (requires Rust toolchain — MSRV **1.91**, declared as `rust-version` in `backend/Cargo.toml` and enforced in CI):

```bash
cd backend && cargo run
```

> Run `cargo run -- migrate` once to initialise the schema before the first
> `cargo run` — the server verifies the schema and refuses to start if it is
> fresh or behind.

Frontend only (requires Node.js >=24.15.0):

```bash
cd frontend && npm install && npm run dev
```

See [backend/CLAUDE.md](../backend/CLAUDE.md) and [frontend/CLAUDE.md](../frontend/CLAUDE.md) for subsystem-specific conventions (database roles, testing helpers, linting rules).

### Pre-commit prerequisites

The lint-staged pre-commit hook runs [`actionlint`](https://github.com/rhysd/actionlint) on changed GitHub Actions workflow files. Install it once before your first commit (version pinned to **v1.7.12** in [`lint-staged.config.js`](../lint-staged.config.js) and [`.github/workflows/ci.yml`](workflows/ci.yml)):

```bash
# Linux + macOS — pinned binary (Homebrew's formula is not version-pinned,
# so it can drift from the v1.7.12 lint chain enforced in CI; download the
# release tarball directly to guarantee parity).
curl -fsSL "https://github.com/rhysd/actionlint/releases/download/v1.7.12/actionlint_1.7.12_$(uname -s | tr 'A-Z' 'a-z')_$(uname -m | sed 's/x86_64/amd64/; s/aarch64/arm64/').tar.gz" \
  | tar -xz -C "$HOME/.local/bin" actionlint
```

The hook also runs [`yamllint`](https://www.yamllint.com/) on changed `*.{yml,yaml}` files (version pinned to **1.33.0** in CI). It is pip-installable:

```bash
pipx install yamllint==1.33.0
```

If `actionlint` or `yamllint` is not on `PATH`, the pre-commit hook fails with a clear `command not found`. CI re-runs the same checks, so a bypass (`--no-verify` or missing-binary skip) is still caught before merge.

Workflow files are additionally scanned in CI by [zizmor](https://github.com/zizmorcore/zizmor) (the merge-blocking `workflow-security` job) for GitHub Actions security issues — credential persistence, template injection, cache poisoning, and dangerous triggers. It is a CI-only tool, so there is nothing to install locally; documented suppressions and their justifications live in [`.github/zizmor.yml`](zizmor.yml).

## Testing requirements

**Tests are mandatory.** No feature or bug fix is complete without tests. Follow the test-first pattern:

- **Happy path** — expected behaviour works
- **Negative cases** — invalid input is rejected, error paths are exercised
- **Edge cases** — where the behaviour is non-obvious

PRs without tests will not be approved. See [CLAUDE.md](../CLAUDE.md) Hard Rule 5 for the full policy.

## Accessibility

Reverie targets **WCAG 2.2 Level AA** as a design invariant. The process — what the automated gate covers, what the manual audit owns, the audit cadence, and how the one accepted brand carve-out (Reverie Gold on large CTAs) is documented — is recorded in [`adr/2026-06-05-accessibility-review-process.md`](../adr/2026-06-05-accessibility-review-process.md).

For frontend changes, the `a11y` CI job runs axe-core against the design showcase and fails on any WCAG 2.2 AA violation outside the documented allowlist (`frontend/scripts/a11y/allowlist.mjs`). Run it locally with `npm run a11y` (from `frontend/`, with the dev server up). UI-touching PRs also carry an accessibility checklist in the PR template.

## Pull request process

1. Create a feature branch from `main` using the appropriate prefix
2. Write tests for your changes (see above)
3. Ensure all CI checks pass locally (`cargo fmt --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo test`, `npm run lint`, `npm test`, `npm run build` as applicable)
4. Open the PR — it will load a template; fill in **Summary**, **Why** (if motivation isn't obvious from the diff), and **Test plan**
5. Labels auto-apply based on paths touched — no manual labelling needed
6. Wait for maintainer review and approval

## Third-party AI code review

This repository uses third-party AI code reviewers that auto-comment on pull requests. By opening a PR you accept that the diff and surrounding repository context will be sent to the active reviewers for analysis.

Active reviewers:

- [Greptile](https://www.greptile.com) — graph-based codebase context. See [security disclosures](https://www.greptile.com/security)
- [CodeRabbit](https://www.coderabbit.ai) — line-level inline review with formal GitHub PR Review status. See [security and trust](https://www.coderabbit.ai/trust-center)

General data handling (both reviewers):

- Both are managed SaaS providers; inference runs through third-party LLM platforms (OpenAI, Anthropic, Google). Repository code is cached on their infrastructure while their GitHub Apps have access; cache is deleted on App uninstall per each provider's retention policy
- Reverie is AGPL-3.0 and the code these reviewers receive is already public, so the marginal exposure is near-zero — these disclosures exist for transparency, not because anything sensitive is being shared

Reviewer-specific notes:

- **Greptile AI-training opt-in.** Reverie uses Greptile under their "free for open-source" arrangement, and as a token form of reciprocity, this repository has training-data use enabled at the account level. Per Greptile's policy this means de-identified, aggregated repository data may be used to monitor, improve, or expand their services. PII and customer-specific references are stripped per their disclosure
- **CodeRabbit AI-training default.** CodeRabbit's OSS terms do not enable training on repository data by default. Reverie does not change that default
- **External contributions.** If active external contributions start arriving, the Greptile training opt-in is reconsidered with those contributors in the loop. Reverie remains AGPL-3.0

Reviewer findings are advisory: address actionable ones in follow-up commits, dismiss the rest with a brief note. Maintainer review remains the only merge gate.

## Dependencies

Dependency updates are managed by [Renovate](https://docs.renovatebot.com/) on a weekly schedule. **Don't file separate PRs for dependency bumps** unless you're patching a security advisory that Renovate hasn't yet flagged. Security-related dependency updates bypass the weekly schedule and land whenever the advisory is published.

New Rust dependencies must satisfy the supply-chain policy in [`backend/deny.toml`](../backend/deny.toml): a crate whose license is outside the permissive allowlist (any GPL/LGPL/AGPL or otherwise unlisted license) or that resolves to a git source rather than crates.io will fail the `cargo deny check` run in the `audit` CI job. If you have a legitimate need for such a dependency, raise it in the PR so the policy exception can be reviewed.
