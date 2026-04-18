# Contributing to Reverie

Thank you for your interest in contributing to Reverie.

## Contributor License Agreement

By submitting a pull request, you agree to assign copyright of your contribution to
the project maintainer (John Unkovich). This is required to maintain the option of
dual-licensing in the future while keeping the project AGPL-3.0 for the community.

A CLA bot will be added to automate this process once the project accepts external
contributions.

## Commit Messages

This project uses [Conventional Commits](https://www.conventionalcommits.org/).
All commit messages must follow this format:

```text
<type>(<scope>): <description>
```

See [CLAUDE.md](CLAUDE.md) for the full specification and examples.

## Development Setup

```bash
# Clone the repo
git clone https://github.com/unkos-dev/reverie.git
cd reverie
```

```bash
# Backend (requires Rust toolchain)
cd backend && cargo run
```

```bash
# Frontend (requires Node.js 22+)
cd frontend && npm install && npm run dev
```

```bash
# Full stack via Docker
docker compose up
```

## Pull Request Process

1. Create a feature branch from `main` using the appropriate prefix
2. Write tests for your changes
3. Ensure all CI checks pass
4. Submit a PR with a clear description of the changes
5. Wait for maintainer review and approval
