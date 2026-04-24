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

Branch names use the same type prefix: `feat/`, `fix/`, `refactor/`, `docs/`, `chore/`, `test/`, `perf/`. See [CLAUDE.md](CLAUDE.md) for the full specification, examples, and breaking-change conventions.

## Development setup

Simplest path — full stack in Docker:

```bash
git clone https://github.com/unkos-dev/reverie.git
cd reverie
docker compose up
```

Backend only (requires Rust toolchain):

```bash
cd backend && cargo run
```

Frontend only (requires Node.js 22+):

```bash
cd frontend && npm install && npm run dev
```

See [backend/CLAUDE.md](backend/CLAUDE.md) and [frontend/CLAUDE.md](frontend/CLAUDE.md) for subsystem-specific conventions (database roles, testing helpers, linting rules).

## Testing requirements

**Tests are mandatory.** No feature or bug fix is complete without tests. Follow the test-first pattern:

- **Happy path** — expected behaviour works
- **Negative cases** — invalid input is rejected, error paths are exercised
- **Edge cases** — where the behaviour is non-obvious

PRs without tests will not be approved. See [CLAUDE.md](CLAUDE.md) Hard Rule 5 for the full policy.

## Pull request process

1. Create a feature branch from `main` using the appropriate prefix
2. Write tests for your changes (see above)
3. Ensure all CI checks pass locally (`cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`, `npm run lint`, `npm test`, `npm run build` as applicable)
4. Open the PR — it will load a template; fill in **Summary**, **Why** (if motivation isn't obvious from the diff), and **Test plan**
5. Labels auto-apply based on paths touched — no manual labelling needed
6. Wait for maintainer review and approval

## Dependencies

Dependency updates are managed by [Renovate](https://docs.renovatebot.com/) on a weekly schedule. **Don't file separate PRs for dependency bumps** unless you're patching a security advisory that Renovate hasn't yet flagged. Security-related dependency updates bypass the weekly schedule and land whenever the advisory is published.
