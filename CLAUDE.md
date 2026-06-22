# Reverie Project Context

<project_hard_rules>
These are absolute invariants for the Reverie repository.

1. **NEVER MERGE TO MAIN:** The user performs all merges. You are encouraged to make regular commits at logical points for durability. You may use `gh pr create` to open the PR, but you must hand it off at "green and ready for review" and STOP. You are strictly FORBIDDEN from running `gh pr merge` or proposing a merge step.
2. **NEVER COMMIT SECRETS:** No `.env`, tokens, or API keys.
3. **REDACT SECRETS IN OUTPUT:** Never surface decrypted secret values in chat. Describe their presence (length, format) only.
4. **SECURITY PROCESS:** Consult `docs/security/codeguard/` and answer 'will this change require an update here?' before coding. Do NOT read the redaction log.
5. **VERSIONING:** Versions are release-please-managed; never hand-edit version in `Cargo.toml`/`package.json`.
6. **TESTS ARE MANDATORY:** Every feature or fix must be accompanied by corresponding happy-path and edge-case tests in the exact same PR. Do not submit code without tests.
7. **VERIFICATION IS REBUILD-GATED:** If a linter or CI tool fails due to a missing binary, STOP immediately. Tell the user the Coder workspace image needs a rebuild. Do NOT attempt to `apt-get install` the missing tool or patch the CI script to bypass it.
   </project_hard_rules>

<git_and_linear_workflow>

- **Branching:** Branch from `main` (e.g., `feat/unk-42-epub-import`).
- **Commits:** Conventional Commits are MANDATORY (`<type>(<scope>): <description>`). Explain the _why_, not the _what_.
- **Linear Integration:** PR bodies MUST include `Closes UNK-XXX` to auto-close the tracking issue. Linear tracks the backlog; do not mint an issue just to name a branch or close work done in this session.
  </git_and_linear_workflow>

<documentation_and_planning>

- **Docs are part of done:** Ship generated reference docs and narrative docs in the same PR as the feature.
- **Comment Policy (Tiered):**
  - Tier 1 (Public): Describe purpose, invariants, and non-obvious WHY.
  - Tier 2 (Security/Auth): Add explicit threat annotations (`// THREAT:`).
  - Tier 3 (Internal): Comment only when WHY is non-obvious. Default to no comments.
  - Tier 4 (Tests): No docstrings on test functions.
- **Docstring Syntax:** No em dashes (`—`). No external references (PRs, Linear IDs). Describe current behavior, not history.
- **Planning Artifacts:** Store design specs in `/plans/`. Implementation plans go in `.claude/PRPs/plans/`.
- **ADRs:** Write an ADR (in `adr/`) for any new cross-stack pattern, major dependency, or architectural choice. Use the `adr` skill.
  </documentation_and_planning>

<technical_debt_management>

- **Tracked in `debt/`:** All workarounds and known-wrong shapes must be documented in `debt/README.md` with a specific, measurable lift condition.
- **Purge when fixed:** When a workaround is resolved, completely delete the entry from the debt tracking.
  </technical_debt_management>

<project_structure>

- `backend/` — Rust + Axum (See `backend/CLAUDE.md` for specific rules)
- `frontend/` — React + Vite + TS (See `frontend/CLAUDE.md` for specific rules)
- `docs/` — Starlight site
- `adr/` — Architecture Decision Records
  </project_structure>
