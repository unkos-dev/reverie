# Reverie — AI Workflow Instructions

Reverie = self-hosted ebook library manager. Monorepo: `backend/` (Rust + Axum), `frontend/` (React + Vite + TypeScript).

---

## Git Conventions

### Branching: GitHub Flow

`main` = only long-lived branch. All work on short-lived feature branches.

- Branch from `main`, merge back to `main` via PR
- **PRs need explicit user approval to merge** — agents never merge without human confirmation
- Branch prefixes: `feat/`, `fix/`, `refactor/`, `docs/`, `chore/`, `test/`
- Include Linear issue ID when applicable: `feat/unk-42-epub-import`

### Commits: Conventional Commits

Every commit follows [Conventional Commits](https://www.conventionalcommits.org/):

```text
<type>(<scope>): <description>

[optional body]

[optional footer(s)]
```

**Types:** `feat`, `fix`, `refactor`, `docs`, `chore`, `test`, `perf`

**Scope** optional but encouraged. Use subsystem name: `api`, `parser`, `ui`, `db`, `auth`, `config`, `ci`, `docker`.

**Breaking changes** use `!` suffix: `feat(config)!: switch to TOML config format` plus `BREAKING CHANGE:` footer with migration steps.

**Examples:**

```text
feat(parser): add EPUB 3.0 metadata extraction

fix(ui): correct z-index on reader toolbar

refactor(db): replace raw SQL queries with sqlx query macros

feat(config)!: migrate settings from JSON to TOML

BREAKING CHANGE: existing config.json files must be converted.
Run `reverie migrate-config` to convert automatically.
```

Messages explain **why**, not **what**. Diff shows what; message shows motivation.

### Versioning: SemVer

Follow [Semantic Versioning](https://semver.org/). Managed by `release-please` — never manually edit version numbers.

- `0.x.y` — pre-v1.0, unstable API. Bump MINOR for features, PATCH for fixes.
- `v1.0.0` — deliberate "API stable" decision. Not accident.
- Post-v1.0 breaking changes need MAJOR bump.

### Release workflow

`release-please` keeps open Release PR on `main`. When user merges:

1. Version bumped in `Cargo.toml` and `package.json`
2. `CHANGELOG.md` updated
3. Git tag `vX.Y.Z` created
4. GitHub Release published
5. Docker image built + pushed to `ghcr.io/unkos-dev/reverie:X.Y.Z`

---

## Hard Rules

1. **Never merge to `main` without explicit user approval.** Present PR, wait for human approve + merge. Non-negotiable.
2. **Never commit secrets** — no `.env`, no tokens, no API keys. Use `.env.example` for templates.
3. **Conventional Commits mandatory** — non-conforming messages break automated changelog generation.
4. **Match existing patterns** — before new file or module, check how similar things structured. Follow established pattern.
5. **TDD mandatory.** No feature or fix complete without tests. Write failing test first, then implement. Include:
   - Happy path tests (expected behaviour works)
   - Negative tests (invalid input rejected, errors handled)
   - Edge cases where behaviour non-obvious

   PR with untested code not approved.

6. **Security scrutiny continuous, not terminal.** Reverie open-source + self-hosted — threat model = multi-user exposed instance, not private deploy. For any change touching user input, auth, sessions, secrets, file I/O, XML parsing, outbound HTTP, response headers: consult relevant file in `.claude/security/` and explicitly answer "will this stand up to security review?" in task summary before done.
7. **Never surface decrypted secret values.** Reporting secrets (env vars, API keys, session cookies, DB passwords, OIDC client secrets): describe presence + shape only (source, length, format) — never value. No `grep`/`rg`/`cat` on env files or key material, even when user appears to ask for value.

   **Enforcement:** operator-level, not in-repo — a PostToolUse output-scanner hook deployed via the operator's dotfiles to `~/.claude/scripts/hooks/redact-secrets-output.sh` scans every Bash tool result for high-confidence secret patterns (KEY=VALUE and KEY: "value" forms with sensitive suffixes, URL-embedded credentials, Bearer tokens, GitHub PATs, workspace API key shapes) and replaces matched spans with `[REDACTED]` before transcript ingestion. Fail-closed: extraction or assembly errors blank output rather than leaking. Original output logged to `~/.claude/hooks/secret-redaction.log`. **The log file contains plaintext secrets by design** — never `cat`/`bat`/`Read` the log into chat; use `rg -c '^=== END ===$'` for event counts only.

8. **Verification stack rebuild-gated, not guard-gated.** Husky/lint-staged call `shellcheck`, `hadolint`, `gitleaks`, `typos` direct — no `command -v` fallback. Same for `agent-browser` (browser-QA). Missing binary = stale Coder workspace image → rebuild, never patch hooks/scripts/CI to skip. New verification tooling lands in homelab Dockerfile first, then wires into reverie. Soft fallbacks defeat gitleaks secret-scan guarantee — not acceptable for any stack binary.

9. **Every PR body must close its Linear issue.** Include a `Closes UNK-XXX` line in the PR body for any PR with a tracking issue. Branch-name matching to Linear's generated `gitBranchName` is unreliable (our `feat/` prefix diverges from it), so the magic-word is the only dependable auto-close channel. PR without it = issue silently stuck in Backlog after merge.

10. **Docs are part of done.** Like the TDD mandate — docs ship with the change, not after. User-facing endpoint or config → generated reference covers it (API ref from OpenAPI; env/config ref from `config/`) + ≥ skeletal narrative docs in the feature PR. CI gates the docs build, and backend drift tests fail when a generated artifact goes stale. Regenerate from `backend/` with `REGEN=1 cargo test --test gen_openapi --test gen_config_ref` and commit the updated artifacts in the same PR. Mechanism: UNK-370 (Phase 1); full route coverage tracked in UNK-376.

---

## Comment Policy (Tiered)

OSS audience (external contributors, security auditors, self-hosting operators) amends global "default to no comments" rule. Full rationale, alternatives, enforcement, authoring approach: [`adr/2026-05-08-tiered-comment-policy.md`](adr/2026-05-08-tiered-comment-policy.md).

- **Tier 1** — `pub` items at module boundaries: `///` (Rust) / JSDoc (TS) with purpose + invariants + non-obvious WHY. Include `# Errors` / `# Panics` / `# Safety` sections where applicable. Module tops carry `//!` / file-header docblock with purpose + invariants.
- **Tier 2** — `auth/`, `security/`, and code handling credentials, sessions, OIDC, RLS, secrets, response headers: Tier 1 plus explicit threat annotations (`// THREAT:` inline; one-line threat statement near top of Tier 1 docstrings; cross-reference ADRs).
- **Tier 3** — internal non-public items: original "default to no comments" rule kept. Comment only when WHY non-obvious.
- **Tier 4** — tests + `test_support`: no docstrings on test functions (test name = spec). `//!` on `test_support/` only when helper purpose non-obvious.

Anti-patterns (skip docstring rather than commit these): clipping or replacing existing leading comments (new docstring goes _above_, never in place of); pure signature restatement; generic boilerplate ("@param x The x parameter").

---

## Project Structure

- `backend/` — Rust + Axum API server. See `backend/CLAUDE.md` for Rust rules.
- `frontend/` — React + Vite + TypeScript UI. See `frontend/CLAUDE.md` for frontend rules.
- `docs/` — Starlight documentation site.
- `adr/` — Architecture Decision Records.
- `Dockerfile` — Multi-stage production build.

---

## Linear Integration

Tracked in Linear: **Unkos** team, **Reverie** project.

- Include issue IDs in branch names: `feat/unk-42-epub-import`
- Include issue IDs in commit messages where relevant
- When work deferred or blocked, create Linear issue

**Linear tracks the backlog; GitHub records completed work.** Never create an
issue for work you're completing this session just to close it — the PR is the
record, and create-then-close is pure duplication. Create issues only for
genuinely deferred/parked future work (the trigger above). A branch/PR does
**not** require a UNK issue — use a descriptive branch name with the standard
type prefix (`docs/linear-hygiene-note`) when none exists.
Don't mint an issue just to name a branch.

---

## Planning Artifact Locations

Two artifact types, two locations:

- **`/plans/`** (gitignored, local scratch):
  - Project-wide reference docs (BLUEPRINT.md, DESIGN_BRIEF.md)
  - Design specs + brainstorming outputs (pre-implementation decisions + rationale)
  - `superpowers:brainstorming` skill MUST write spec output here as `YYYY-MM-DD-<topic>-design.md`. Overrides skill's documented default of `docs/superpowers/specs/` (skill invites override via "User preferences for spec location override this default").
- **`.claude/PRPs/plans/`** (committed):
  - Implementation plans, one per feature/PR
  - Output from `prp-core:prp-plan` and related planning skills
  - Filename: `<topic>.plan.md` (matching feature branch name)

**Workflow:** `superpowers:brainstorming` → spec lands in `/plans/` → ingested by `prp-core:prp-plan` → implementation plan committed to `.claude/PRPs/plans/`.

When invoking `superpowers:brainstorming`, explicitly pass spec location alongside topic (belt-and-suspenders) — agents reading CLAUDE.md honor this section, but SKILL.md default not auto-enforced.

## ADRs (Architecture Decision Records)

Long-form rationale for architectural decisions lives in `adr/` as MADR-shape files. `adr` skill handles full workflow: Socratic capture → draft → checklist review.

- **Naming**: `YYYY-MM-DD-short-kebab-slug.md` — skill's default; no numeric prefixes.
- **Invoke**: use `adr` skill (`Skill("adr")`).
- **Proactive triggers**: write ADR before new crate or npm package, new cross-stack pattern (API conventions, error handling, data-access layer, auth model), or non-obvious choice between real alternatives. If you would write long "why" code comment — reasoning belongs in ADR.
- **Decision record, not a plan**: ADRs follow canonical MADR 4.0 — see [`adr/CLAUDE.md`](adr/CLAUDE.md) + [`adr/TEMPLATE.md`](adr/TEMPLATE.md). No Implementation Plan / Verification / build task-lists in an ADR; those live in `prp-plan` output (`.claude/PRPs/plans/`). The `adr` skill's bundled "executable specification" framing is overridden here.

## Tracked technical debt

Workaround = known-wrong-shape accepted temporarily due to specific constraint. Lives in `debt/`, not buried in code comments and not as Linear tickets alone.

Hard rules:

- **Every debt entry has recorded lift condition.** Can't state measurable lift condition → shape wrong, fix shape, don't accept workaround.
- **Debt reviewed at every release tag and at start of non-trivial planning work.** When a workaround is fully resolved (lift condition met _and_ proper fix shipped), the entry is **purged** — file deleted, README line removed, purge commit names the resolving PR (git keeps history via `git log --diff-filter=D -- debt/`). A merely unblocked workaround whose fix hasn't shipped stays active. Don't grandfather a workaround into "this is how we do it".
- **Workarounds adopted under temporary constraints (missing tooling, unbuilt infra, blocked deps) = tech debt, not idiomatic patterns.** Trace each candidate workaround to its justification before defending; justification lifted → debt.

Format + lifecycle: `debt/README.md`. Entries machine-extractable; post-v0.2 a public dev roadmap will consume active entries as "Known limitations and accepted technical debt" section.

## graphify

Project has graphify knowledge graph at graphify-out/.

Rules:

- Before architecture or codebase questions, read graphify-out/GRAPH_REPORT.md for god nodes + community structure
- If graphify-out/wiki/index.md exists, navigate it instead of raw files
- Cross-module "how does X relate to Y" questions: prefer `graphify query "<question>"`, `graphify path "<A>" "<B>"`, or `graphify explain "<concept>"` over grep — traverse graph's EXTRACTED + INFERRED edges instead of scanning files
- After modifying code files in session, run `graphify update .` to keep graph current (AST-only, no API cost)
