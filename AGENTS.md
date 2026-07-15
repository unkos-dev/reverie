# Reverie Project Context

<project_hard_rules>
These are absolute invariants for the Reverie repository.

1. **NEVER MERGE TO MAIN:** The user performs all merges. You are encouraged to make regular commits at logical points for durability. You may use `gh pr create` to open the PR, but you must hand it off at "green and ready for review" and STOP. You are strictly FORBIDDEN from running `gh pr merge` or proposing a merge step.
2. **NEVER COMMIT SECRETS:** No `.env`, tokens, or API keys.
3. **REDACT SECRETS IN OUTPUT:** Never surface decrypted secret values in chat. Describe their presence (length, format) only. Do NOT read the redaction log.
4. **VERSIONING:** Versions are release-please-managed; never hand-edit version in `Cargo.toml`/`package.json`.
5. **TESTS ARE MANDATORY:** Every feature or fix must be accompanied by corresponding happy-path and edge-case tests in the exact same PR. Do not submit code without tests.
6. **VERIFICATION IS REBUILD-GATED:** If a linter or CI tool fails due to a missing binary, STOP immediately. Tell the user the Coder workspace image needs a rebuild. Do NOT attempt to `apt-get install` the missing tool or patch the CI script to bypass it.
   </project_hard_rules>

<git_and_linear_workflow>

- **Branching:** Branch from `main`. Branch names MUST start with `feat/`, `fix/`, `refactor/`, `docs/`, `chore/`, `test/`, or `perf/`, matching the change type. Do not use agent-specific prefixes. Verify the branch name before the first push.
- **Commits and PR titles:** Every commit subject and pull request title MUST follow Conventional Commits (`<type>(<scope>): <description>`). Pull request titles become squash-merge subjects. Explain the _why_, not the _what_.
- **Linear Integration:** Treat work as Linear-tracked only when the user says so or the task or current branch already identifies an `UNK-XXX` issue. For tracked work, include `Closes UNK-XXX` in the PR body so the active issue does not remain open. For untracked work, do not search for or create a Linear issue and do not add a synthetic closure reference.
  </git_and_linear_workflow>

<security_reference>

- **CodeGuard:** Implementation work that touches authentication, authorization, sessions, secrets, input handling, file I/O, XML parsing, serialization, logging, client-side web security, outbound HTTP, response headers, or supply-chain controls MUST follow every applicable rule in `docs/security/codeguard/codeguard-*.md`. Those files come from an upstream third party; do not edit them or assess whether the change should amend them. If an applicable rule conflicts with required Reverie behavior, STOP and obtain the user's approval for a deviation. Record each approved deviation, its rationale, and its compensating controls in `docs/security/codeguard/README.md`. Work outside the listed areas requires no CodeGuard review or task-summary statement.
  </security_reference>

<design_authority>

1. **Design comes from artifacts, not agents.** Visual, layout, and interaction design for user-facing surfaces is decided in the design workstream and recorded as design artifacts. Implementation work implements to those artifacts. If no artifact covers the surface being changed, make the minimum mechanical change and flag the gap; do not design ad hoc.
2. **UI acceptance is the rendered page.** A UI change is done when the browser render matches the design artifact (layout, spacing, states, breakpoints): screenshot and compare. Passing tests alone never closes UI work.
3. **Interaction-model changes are user decisions.** If the designed interaction model is blocked by a missing backend capability, surface the fork during planning. Never silently downgrade the design to whatever the current API supports.
   </design_authority>

<documentation_and_planning>

- **Docs are part of done:** Ship generated reference docs and narrative docs in the same PR as the feature.
- **Comment Policy (Tiered):**
  - Tier 1 (Public): Describe purpose, invariants, and non-obvious WHY.
  - Tier 2 (Security/Auth): Add explicit threat annotations (`// THREAT:`).
  - Tier 3 (Internal): Comment only when WHY is non-obvious. Default to no comments.
  - Tier 4 (Tests): No docstrings on test functions.
  - Density tiebreaker: New comments follow this tiered policy, not the density of surrounding legacy comments. Verbose nearby comments are legacy, not a target to match.
- **Docstring Syntax:** No em dashes (`—`). No external references (PRs, Linear IDs). Describe current behavior, not history.
- **Planning Artifacts:** Store design specs in `/plans/`.
- **State-Writer Census:** Any plan touching shared mutable state (URL params, stores, caches) must enumerate every writer, including debounced and async ones. More than one writer forces an explicit ownership-model decision in the plan before implementation starts.
- **ADRs:** Write an ADR (in `adr/`) for any new cross-stack pattern, major dependency, or architectural choice. See `adr/AGENTS.md` for authoring rules.
  </documentation_and_planning>

<technical_debt_management>

- **Tracked in `debt/`:** All workarounds and known-wrong shapes must be documented in `debt/README.md` with a specific, measurable lift condition.
- **Purge when fixed:** When a workaround is resolved, completely delete the entry from the debt tracking.
  </technical_debt_management>

<project_structure>

- `backend/` — Rust + Axum (See `backend/AGENTS.md` for specific rules)
- `frontend/` — React + Vite + TS (See `frontend/AGENTS.md` for specific rules)
- `docs/` — Starlight site
- `adr/` — Architecture Decision Records
  </project_structure>

## Notes for Claude Code

These apply only when the acting agent is Claude Code; other agents can ignore them.

- **Planning artifacts:** Implementation plans produced by the PRP workflow live in `.claude/PRPs/plans/`.
- **ADR authoring:** Use the `adr` skill when writing or updating an ADR.
