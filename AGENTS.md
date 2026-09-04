# Reverie Project Context

<project_hard_rules>
These are absolute invariants for the Reverie repository.

1. **Merges belong to the maintainer:** Commit freely at logical points for durability, and open the pull request with `gh pr create`. Hand off at "green and ready for review". Do not run `gh pr merge` and do not propose a merge step.
2. **Never commit secrets:** No `.env`, tokens, or API keys.
3. **Redact secrets in output:** Never surface a decrypted secret value. Describe its presence (length, format) only. Do not read the redaction log.
4. **Versioning:** Versions are release-please-managed; never hand-edit version in `Cargo.toml`/`package.json`.
5. **Tests are mandatory for the shipped product:**
   - Every feature or fix in `backend/` or `frontend/` ships with happy-path and edge-case tests in the same PR.
   - Do not submit product code without tests.
   - Executable tooling anywhere else in the tree (`scripts/`, the justfiles, `.github/`, `docker/`, the root configs) is judged on one question: can it fail quietly?
   - Prose and generated documentation are not in scope here; "Docs are part of done" below governs them.
   - A guard that a pull request exercises and that fails loudly needs no self-test.
   - A check that can pass while matching nothing needs an assertion inside it, not a fixture-driven test outside it.
6. **Verification prerequisites:** Restore a declared project dependency only through the repository's documented, lockfile-backed setup command. If a system prerequisite or CI-only binary is missing, stop the affected verification and report the exact missing command. Never install system packages, weaken checks, or patch around a missing tool without the maintainer's explicit approval.
   </project_hard_rules>

<git_and_linear_workflow>

- **Branching:** Branch from `main`. Branch names MUST start with a commitlint-accepted type as prefix (`build/`, `chore/`, `ci/`, `docs/`, `feat/`, `fix/`, `perf/`, `refactor/`, `revert/`, `style/`, `test/`), matching the change type. Do not use agent-specific prefixes. Verify the branch name before the first push.
- **Commits and PR titles:** Every commit subject and pull request title MUST follow Conventional Commits (`<type>(<scope>): <description>`). Pull request titles become squash-merge subjects. Explain the _why_, not the _what_.
- **PR descriptions:** Treat the pull request description as the durable squash commit body. Describe the final change in the project's voice. Keep session handoffs, review requests, next-step instructions, and other conversational material out of the description.
- **Sign-off:** Every commit MUST carry a `Signed-off-by` trailer, so commit with `git commit -s`. The `commit-msg` hook rejects an unsigned commit locally, and the DCO app fails its required check on the pull request. `.github/CONTRIBUTING.md` carries the Developer Certificate of Origin text and the rest of the contributor process.
- **Linear Integration:** Treat work as Linear-tracked only when the maintainer says so or the task or current branch already identifies an `UNK-XXX` issue.
  - For tracked work, include `Closes UNK-XXX` in the PR body; omitting it leaves the issue open.
  - Linear's own GitHub app attaches the pull request and transitions the issue: In Progress on open, Done on merge.
  - Nothing in this repository closes issues; there is no workflow or credential behind the transition.
  - The app matches the phrase anywhere in the body, even in prose that only quotes it, and writes on open regardless of the issue's state.
  - A body that discusses a closing line without meaning it must write the identifier without the keyword.
  - For untracked work, do not search for or create a Linear issue and do not add a synthetic closure reference.
    </git_and_linear_workflow>

<security_reference>

- **CodeGuard:** Implementation work in any covered area MUST follow every applicable rule in `docs/security/codeguard/codeguard-*.md`.
  - Covered areas: authentication, authorization, sessions, secrets, input handling, file I/O, XML parsing, serialization, logging, client-side web security, outbound HTTP, response headers, and supply-chain controls.
  - Those files come from an upstream third party. Do not edit them, and do not assess whether a change should amend them.
  - If an applicable rule conflicts with required Reverie behavior, stop and obtain the maintainer's approval for a deviation.
  - Record each approved deviation, its rationale, and its compensating controls in `docs/security/codeguard/README.md`.
  - Work outside the covered areas requires no CodeGuard review or task-summary statement.
    </security_reference>

<design_authority>

1. **Design comes from artifacts, not agents.** Visual, layout, and interaction design for user-facing surfaces is decided in the design workstream and recorded as design artifacts. Implementation work implements to those artifacts. If no artifact covers the surface being changed, make the minimum mechanical change and flag the gap; do not design ad hoc.
2. **UI acceptance is the rendered page.** A UI change is done when the browser render matches the design artifact (layout, spacing, states, breakpoints): screenshot and compare. Passing tests alone never closes UI work.
3. **Interaction-model changes are maintainer decisions.** If the designed interaction model is blocked by a missing backend capability, surface the fork during planning. Never silently downgrade the design to whatever the current API supports.
   </design_authority>

<documentation_and_planning>

- **Docs are part of done:** Ship generated reference docs and narrative docs in the same PR as the feature.
- **Comment Policy (Tiered):**
  - Tier 1 (Public): Describe purpose, invariants, and non-obvious WHY.
  - Tier 2 (Security/Auth): Add explicit threat annotations (`// THREAT:`).
  - Tier 3 (Internal): Comment only when WHY is non-obvious. Default to no comments.
  - Tier 4 (Tests): No docstrings on test functions.
  - Density tiebreaker: New comments follow this tiered policy, not the density of surrounding legacy comments. Verbose nearby comments are legacy, not a target to match.
- **Docstring Syntax:** No external references (PRs, Linear IDs). Describe current behavior, not history.
- **Private Artifacts:** Store design specifications, implementation plans, and evaluation records in ignored `/plans/`. Never reference private artifact paths from public source, documentation, commits, or pull requests.
- **State-Writer Census:** Any plan touching shared mutable state (URL params, stores, caches) must enumerate every writer, including debounced and async ones. More than one writer forces an explicit ownership-model decision in the plan before implementation starts.
- **ADRs:** Write an ADR for any new cross-stack pattern, major dependency, or architectural choice. Create it with `specful new adr --title <title>` under `docs/adr/`; never hand-allocate an identifier. Records under `adr/` predate that profile and stay there, authored to `adr/AGENTS.md`, until they are reviewed.
- **Specifications:** Requirements (what the system must do) and Designs (how a subject currently works) live under `docs/specs/`, one directory per architectural scope. Start at `docs/specs/index.md`. Create an artifact with `specful new requirement|design --title <title> --scope <scope>`; never hand-allocate an identifier. Write current state only: no history, no transition narrative. After editing, run `specful index` then `specful validate`, and commit the regenerated views with the change. A Design records how a subject works; it is not a visual or interaction design artifact, which `<design_authority>` governs separately.
  </documentation_and_planning>

<technical_debt_management>

- **Tracked in `debt/`:** All workarounds and known-wrong shapes must be documented in `debt/README.md` with a specific, measurable lift condition.
- **Purge when fixed:** When a workaround is resolved, completely delete the entry from the debt tracking.
  </technical_debt_management>

<project_structure>

- `backend/` — Rust + Axum (See `backend/AGENTS.md` for specific rules)
- `frontend/` — React + Vite + TS (See `frontend/AGENTS.md` for specific rules)
- `docs/` — Repository documentation (deployment, design, security, schema)
- `docs/specs/` — Requirements and Designs, navigated from `docs/specs/index.md`
- `website/` — Starlight site
- `adr/` — Architecture Decision Records

Files that gate a change and are easy to miss:

- `.github/CONTRIBUTING.md`: contributor process, DCO sign-off, CI toolchain pins.
- `.github/workflows/`: the CI jobs the local `just` gates mirror.
- `lefthook.yml`: the pre-commit, commit-msg, and pre-push hooks that run on every commit.
- `.vale.ini` with `styles/`: prose linting over Markdown, ADRs, and docs.
- `_typos.toml` and `.markdownlint-cli2.jsonc`: spelling and Markdown lint configuration.
- `.specful/config.yaml`: the project key and identifier counters; only `specful new` writes it. `.specful/generated/` and every `docs/specs/**/index.md` are generated by `specful index` and never hand-edited.
- `.env.example`: the documented environment-variable surface.
- `backend/deny.toml`: the supply-chain policy `cargo deny` enforces.
- `backend/guards/`: allowlists for the static guards in `scripts/backend-guards.sh`.

`.github/` is a dot directory, so ripgrep and similar tools skip it by default.
The repository `.ignore` re-includes it for tools that honour ignore files;
anything else needs an explicit `--hidden` flag or a direct path.
</project_structure>

<task_runner>

`just` is the task runner for every plane. Run `just --list` before
hand-rolling a command: the lint, format, test, and build definitions
CI uses live in the justfiles, so invoking a tool directly can apply different
flags than the gate that will judge the change.

Recipes are namespaced by module (`rust::check`, `js::check`, `website::build`);
the unprefixed aggregates fan out across planes. Each recipe's doc comment is
its documentation; `just --list` renders them.

Use `just worktree <branch>` to create a worktree. It places the checkout
outside the repository, where it cannot enter the Docker build context or
cargo's workspace discovery, and refuses to create one on a temporary
filesystem where unpushed commits would not survive a reboot. It installs the
node dependencies in the new checkout, under the pnpm that branch's
`package.json` declares, so the worktree is ready for the JS plane on its first
turn; that install is the one step in the recipe that needs network access. It
also writes a worktree-local cargo target dir, so concurrent worktree builds
cannot thrash a shared target cache. `CARGO_TARGET_DIR` or `CARGO_BUILD_TARGET_DIR`
in the environment overrides that per-worktree config (cargo gives both
variables precedence over `[build] target-dir`, and CARGO_TARGET_DIR wins
when both are set), so anyone who sets either, for this worktree or one
created by other means, should point it at that worktree's own `target/`.

Two aggregates anchor the local loop and should be the default reflex:

- `just doctor` answers "is this machine ready to develop Reverie?" in
  seconds: required binaries, mise pins, docker daemon, dev Postgres health,
  runtime-role login and host unix-socket reachability, node_modules
  freshness, the sqlx offline cache,
  a CARGO_TARGET_DIR or CARGO_BUILD_TARGET_DIR override of a worktree's
  isolated target dir, git sync state, disk space, and the kache build
  cache's presence, daemon state, and store size. Every warning and failure
  names the exact fixing command. Run it first whenever the environment
  might have changed or a failure looks environmental rather than caused by
  the change.
- `just preflight` runs only the preflight lanes this branch's changed paths
  require, deciding from `.github/path-filters.yml` (the file CI's `changes`
  job feeds to dorny/paths-filter, so the two cannot drift). A docs-only or
  frontend-only branch skips the database, the Rust rebuild, and the
  dependency audit. Changes to the verification machinery itself (the
  justfiles, `scripts/`, `mise.toml`, that filter file) escalate to the full
  lane set, and the whole-tree repo-lint mirror always runs. This is the
  default gate for both iteration and pre-push verification.
- `just preflight-full` runs everything the CI gate runs that is locally
  runnable, unconditionally: the DB-backed backend test suite, the sqlx
  cache check, the backend static guards, cargo-machete, cargo-deny, the
  frontend build, and the zizmor workflow-security audit
  (online audits included when a GitHub token is in the environment,
  offline-degraded otherwise). It brings the dev database up itself. Do not run
  it without the maintainer's prior approval. Before seeking approval, explain
  why the scoped gate is insufficient for the change. What it cannot run stays
  remote. The
  MSRV (minimum supported Rust version) check, coverage, the docker image build,
  and the IaC, SAST (static application security testing), and secret scans each
  need a runner, an image, or a token no workstation has.
- Two recipes gate nowhere and are invoked by hand when you are working on what
  they cover. `just js::a11y` reuses an already-running dev server without
  checking who owns it, so from a checkout that does not hold port 5173 it scans
  a different tree; a clean CI runner cannot be ambiguous that way, so CI owns
  the gate and you run the recipe while fixing violations.
  `just infra::selftests` covers this repository's local developer tooling
  against stubbed fixtures: `doctor`, `worktree`, the dev-server lifecycle, the
  detached gate. CI has no stake in any of it. Run it while editing one of
  those scripts. The self-tests that used to sit beside it, covering guards CI
  runs for real on every pull request, are gone; those guards assert their own
  non-degeneracy instead, which fires on every real invocation rather than only
  where a fixture runs.
  Two self-tests are not in that recipe and do gate, in `infra::lint` and in
  CI's repo-lint job. `filter-sarif-unfixable-os-selftest`, because its subject
  decides which container CVEs reach the code-scanning dashboard and an edit
  that widens the match hides fixable findings with nothing else to notice.
  `preflight-scope-selftest`, because its subject decides which lanes a local
  gate runs, so an edit that narrows the gate can deselect the very check that
  would catch it. Neither failure can be asserted from inside its own subject.
- `just check` remains the fast offline subset for mid-task iteration;
  it includes zizmor's offline-only audits but not the token-gated ones.
- `just preflight-detach [scoped|full]` runs either gate detached from the
  terminal (setsid), so a long run survives a session or turn boundary
  without a hand-rolled setsid-plus-log-file pipeline. It prints the log
  path immediately and returns; the log lands under the same
  `$XDG_STATE_HOME/reverie/gate/` area the lane records use, and `just
gate-status` replays the verdict once the run finishes.

Both gates end with a single verdict line, `GATE: PASS <label> (...)` or
`GATE: FAIL <label> at <lane> (...)`. Read that line, not the tail of the last
lane's output: piping a gate run replaces its exit status with the pipe's, and
a truncated or detached log drops the status entirely, so the final lane's
build log looks the same whether the lanes before it passed or failed. Each run
is also recorded per lane under `$XDG_STATE_HOME/reverie/gate/`, outside the
checkout and keyed per worktree; `just gate-status` replays the last one with
its timings and tells every outcome apart by exit status: 1 it failed, 2 it
died unfinished, 3 it is still in progress, 4 nothing is recorded. It also
warns when the checkout has moved past the commit the run was recorded on or
picked up uncommitted changes the run never saw. The
`GATE:` line in the captured output stays the authority: a sandboxed or
otherwise write-blocked run leaves no record, so `gate-status` can only answer
for the last run that managed to write one.
</task_runner>
