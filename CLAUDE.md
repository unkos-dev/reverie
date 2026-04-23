# Reverie — AI Workflow Instructions

Reverie is a self-hosted ebook library manager. This repo is a monorepo with `backend/`
(Rust + Axum) and `frontend/` (React + Vite + TypeScript).

---

## Git Conventions

### Branching: GitHub Flow

`main` is the only long-lived branch. All work happens on short-lived feature branches.

- Branch from `main`, merge back to `main` via PR
- **PRs require explicit user approval to merge** — agents must never merge without
  human confirmation
- Branch prefixes: `feat/`, `fix/`, `refactor/`, `docs/`, `chore/`, `test/`
- Include Linear issue ID when applicable: `feat/unk-42-epub-import`

### Commits: Conventional Commits

Every commit message follows the
[Conventional Commits](https://www.conventionalcommits.org/) specification:

```text
<type>(<scope>): <description>

[optional body]

[optional footer(s)]
```

**Types:** `feat`, `fix`, `refactor`, `docs`, `chore`, `test`, `perf`

**Scope** is optional but encouraged. Use the subsystem name: `api`, `parser`, `ui`,
`db`, `auth`, `config`, `ci`, `docker`.

**Breaking changes** use a `!` suffix: `feat(config)!: switch to TOML config format`
and include a `BREAKING CHANGE:` footer explaining migration steps.

**Examples:**

```text
feat(parser): add EPUB 3.0 metadata extraction

fix(ui): correct z-index on reader toolbar

refactor(db): replace raw SQL queries with sqlx query macros

feat(config)!: migrate settings from JSON to TOML

BREAKING CHANGE: existing config.json files must be converted.
Run `reverie migrate-config` to convert automatically.
```

Commit messages should explain **why**, not just **what**. The diff shows what changed;
the message explains the motivation.

### Versioning: SemVer

Versions follow [Semantic Versioning](https://semver.org/). Managed by `release-please`
— do not manually edit version numbers.

- `0.x.y` — pre-v1.0, API is unstable. Bump MINOR for features, PATCH for fixes.
- `v1.0.0` — deliberate decision meaning "API is stable." Not an accident.
- Breaking changes post-v1.0 require MAJOR bump.

### Release workflow

`release-please` maintains an open Release PR on `main`. When the user merges the
Release PR:

1. Version is bumped in `Cargo.toml` and `package.json`
2. `CHANGELOG.md` is updated
3. Git tag `vX.Y.Z` is created
4. GitHub Release is published
5. Docker image is built and pushed to `ghcr.io/unkos-dev/reverie:X.Y.Z`

---

## Hard Rules

1. **Never merge to `main` without explicit user approval.** Present the PR, wait for
   the human to approve and merge. This is non-negotiable.
2. **Never commit secrets** — no `.env` files, no tokens, no API keys. Use `.env.example`
   for templates.
3. **Conventional Commits are mandatory** — non-conforming commit messages break
   automated changelog generation.
4. **Match existing patterns** — before creating a new file or module, check how similar
   things are structured in the codebase. Follow the established pattern.
5. **Test-Driven Development is mandatory.** No feature or bug fix is complete without
   tests. Write the failing test first, then implement. Include:
   - Happy path tests (expected behaviour works)
   - Negative tests (invalid input is rejected, error cases are handled)
   - Edge cases where the behaviour is non-obvious
   A PR with untested code will not be approved.

---

## Project Structure

- `backend/` — Rust + Axum API server. See `backend/CLAUDE.md` for Rust-specific rules.
- `frontend/` — React + Vite + TypeScript UI. See `frontend/CLAUDE.md` for frontend rules.
- `docs/` — Starlight documentation site.
- `Dockerfile` — Multi-stage production build.

---

## Linear Integration

This project is tracked in Linear under the **Unkos** team, **Reverie** project.

- Include issue IDs in branch names: `feat/unk-42-epub-import`
- Include issue IDs in commit messages where relevant
- When work is deferred or blocked, create a Linear issue

---

## Planning Artifact Locations

Two distinct planning artifact types live in two distinct locations:

- **`/plans/`** (gitignored, local scratch space):
  - Project-wide reference docs (BLUEPRINT.md, DESIGN_BRIEF.md)
  - Design specs and brainstorming outputs (pre-implementation decisions + rationale)
  - The `superpowers:brainstorming` skill MUST write its spec output here as
    `YYYY-MM-DD-<topic>-design.md`. This overrides the skill's documented
    default of `docs/superpowers/specs/` (which the skill explicitly invites
    overriding via "User preferences for spec location override this default").
- **`.claude/PRPs/plans/`** (committed):
  - Implementation plans, one per feature/PR
  - Output from `prp-core:prp-plan` and related planning skills
  - Filename pattern: `<topic>.plan.md` (matching the feature branch name)

**Workflow:** `superpowers:brainstorming` → spec lands in `/plans/` →
ingested by `prp-core:prp-plan` → implementation plan committed to
`.claude/PRPs/plans/`.

When invoking `superpowers:brainstorming`, explicitly pass the spec
location alongside the topic (belt-and-suspenders alongside this
convention) — agents that read CLAUDE.md will honor this section, but the
SKILL.md default is not enforced automatically.
