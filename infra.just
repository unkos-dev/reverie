# Infra plane: whole-tree repo-lint gates. Runs from the repo root (git ls-files is
# repo-relative), so no working-directory. Tools are system-PATH / image-baked.
# pipefail in `set shell` is load-bearing: without it a failing `git ls-files` in a
# pipe would be masked by a passing xargs.

set shell := ["bash", "-ueo", "pipefail", "-c"]

# Shellcheck every tracked shell script.
[group('infra')]
shellcheck:
    git ls-files -z '*.sh' | xargs -0 -r shellcheck

# Hadolint every tracked Dockerfile.
[group('infra')]
hadolint:
    git ls-files -z '*Dockerfile*' | xargs -0 -r hadolint

# Spell check the tree (honours .gitignore and _typos.toml excludes).
[group('infra')]
typos:
    typos

# yamllint every tracked YAML file.
[group('infra')]
yamllint:
    git ls-files -z '*.yml' '*.yaml' | xargs -0 -r yamllint

# --config is explicit because sqruff also auto-discovers `.sqlfluff`. That file
# belongs to Codacy, and any key `.sqruff` leaves unset would otherwise be taken
# from it.
# Lint every tracked SQL file against `.sqruff`.
[group('infra')]
sqruff:
    git ls-files -z '*.sql' | xargs -0 -r sqruff lint --config .sqruff

# Lint GitHub Actions workflow files.
[group('infra')]
actionlint:
    actionlint -color

# `.` matches the zizmor-action default input and .github/zizmor.yml is
# auto-discovered from the repo root, so no --config flag is needed. Exit
# code and default persona (regular) already match the CI job's gate
# semantics: any finding is nonzero. Without a GH_TOKEN/GITHUB_TOKEN/
# ZIZMOR_GITHUB_TOKEN in the environment zizmor degrades to offline mode on
# its own (a WARN, not a failure) and skips five audits that need the GitHub
# API: impostor-commit, known-vulnerable-actions, ref-confusion,
# ref-version-mismatch, stale-action-refs. CI's run carries the ephemeral
# job token, so it always has that coverage; export one of those variables
# locally for full parity.
#
# Audit workflow files for GitHub Actions supply-chain and security issues (matches the CI zizmor gate).
[group('infra')]
zizmor:
    zizmor .

# --offline is the strict form of the two network-disabling flags (it also
# forbids fetching a remote `user/repo` input, not just skipping audits), so
# it is the one that gives an unconditional offline guarantee regardless of
# an ambient token in the calling shell. That guarantee is why this variant
# exists separately from `just infra::zizmor` instead of just leaving the
# token unset: `check` must never depend on network state to stay green.
# Coverage gap versus the full run: the same five token-gated audits listed
# on `just infra::zizmor` never run here, token or not.
#
# Offline-only workflow security audit; skips network-dependent audits (see just infra::zizmor for full CI parity).
[group('infra')]
zizmor-offline:
    zizmor --offline .

# Vale prose linter over all in-scope Markdown (docs/src, adr, repo-root).
[group('infra')]
prose:
    vale docs/src adr *.md

# Self-test the private-plan reference guard.
[group('infra')]
no-plan-refs-selftest:
    scripts/no-plan-refs-selftest.sh

# Self-test the issue-tracker reference guard's whole-tree enforcement.
[group('infra')]
no-issue-refs-selftest:
    scripts/no-issue-refs-selftest.sh

# Regenerate the just recipe reference from the justfiles. WRITES.
[group('infra')]
just-reference:
    scripts/gen-just-reference.sh

# Assert the committed just reference still matches the justfiles.
[group('infra')]
just-reference-drift:
    scripts/gen-just-reference.sh --check

# Assert no workflow re-pins a mise-managed tool; mise.toml is the single source.
[group('infra')]
tool-pin-drift:
    scripts/tool-pin-drift.sh

# Exercise the tool-pin guard against planted pins before trusting it.
[group('infra')]
tool-pin-drift-selftest:
    scripts/tool-pin-drift-selftest.sh

# Assert every vite-plus pin agrees: both package.json files, the alias spec, the override, and each workflow.
[group('infra')]
vp-pin-drift:
    scripts/vp-pin-drift.sh

# Exercise the vp-pin guard against planted drift before trusting it.
[group('infra')]
vp-pin-drift-selftest:
    scripts/vp-pin-drift-selftest.sh

# Assert the npm pin agrees between package.json devEngines and mise.toml.
[group('infra')]
npm-pin-drift:
    scripts/npm-pin-drift.sh

# Exercise the npm-pin guard against planted drift before trusting it.
[group('infra')]
npm-pin-drift-selftest:
    scripts/npm-pin-drift-selftest.sh

# Assert Renovate's custom manager still sees the devEngines npm pin.
[group('infra')]
renovate-npm-manager-selftest:
    scripts/renovate-npm-manager-selftest.sh

# Validate canonical ADR frontmatter and lifecycle index coverage.
[group('infra')]
adr-frontmatter-selftest:
    scripts/adr-frontmatter-selftest.sh

# Self-test the SARIF severity sanitizer both Snyk upload jobs run.
[group('infra')]
sanitize-sarif-severities-selftest:
    scripts/sanitize-sarif-severities-selftest.sh

# Self-test the Snyk Code test-variant rule filter, including that an
# ordinary rule reported in a test file still reaches the dashboard.
[group('infra')]
filter-sarif-test-rules-selftest:
    scripts/filter-sarif-test-rules-selftest.sh

# Self-test the base-OS finding filter, including that a distro CVE with
# a fix available is never withheld.
[group('infra')]
filter-sarif-unfixable-os-selftest:
    scripts/filter-sarif-unfixable-os-selftest.sh

# Validate greptile.json parses and every pinned context path resolves.
[group('infra')]
greptile-config:
    scripts/validate-greptile-config.sh

# Dev-DB compose invariants: loopback bind, digest pin, parent volume mount.
[group('infra')]
compose-dev-invariants:
    scripts/compose-dev-invariants.sh

# Self-test both CI postgres pins against isolated workflow fixtures.
[group('infra')]
compose-dev-invariants-selftest:
    scripts/compose-dev-invariants-selftest.sh

# Overridden DB credentials never echo into recipe logs (dry-run assertion).
[group('infra')]
recipe-secret-echo-test:
    scripts/recipe-secret-echo-test.sh

# Exercise the dev-server lifecycle script against a fake server on a scratch port.
[group('infra')]
dev-server-selftest:
    scripts/dev-server-selftest.sh

# Assert the backend dev recipes resolve REVERIE_DEV_ENV without overriding an already-set variable.
[group('infra')]
backend-dev-env-selftest:
    scripts/backend-dev-env-selftest.sh

# Self-test `just doctor` (missing-binary, fully-stubbed happy path, warn-only path).
[group('infra')]
doctor-selftest:
    scripts/doctor-selftest.sh

# Self-test the db-up socket readiness probe against a stubbed psql.
[group('infra')]
db-ready-selftest:
    scripts/db-ready-selftest.sh

# Self-test the shared tmpfs/ramfs filesystem guard `just worktree` and `just doctor` both call.
[group('infra')]
require-disk-backed-selftest:
    scripts/require-disk-backed-selftest.sh

# Self-test the worktree recipe's Cargo isolation and operator-owned overlays.
[group('infra')]
worktree-selftest:
    scripts/worktree-selftest.sh

# Self-test the new-branch base resolver: origin/main, local-main and HEAD fallbacks, and the explicit-base override/rejection paths.
[group('infra')]
worktree-base-selftest:
    scripts/worktree-base-selftest.sh

# Self-test the preflight scoper's lane mapping, escalation, and fail-closed paths.
[group('infra')]
preflight-scope-selftest:
    scripts/preflight-scope-selftest.sh

# Self-test the gate runner's verdict line, run record, status reporting, and record pruning.
[group('infra')]
gate-run-selftest:
    scripts/gate-run-selftest.sh

# Every dependency-resolving cargo invocation in the just recipes must pass --locked; the package-ingress ADR records why.
[group('infra')]
cargo-locked-guard:
    scripts/cargo-locked-guard.sh

# Self-test scripts/cargo-locked-guard.sh against clean and violating fixtures, then the real recipes.
[group('infra')]
cargo-locked-guard-selftest:
    scripts/cargo-locked-guard-selftest.sh

# Self-test the detached-gate script `just preflight-detach` wraps: a passing and a failing run each need a log and a gate-status-readable record.
[group('infra')]
gate-detach-selftest:
    scripts/gate-detach-selftest.sh

# lefthook is a node-local devDep, not on PATH, so it runs via npx. Validation
# is cheap and offline, so it joins the lint aggregate.
#
# Validate the lefthook.yml hook configuration.
[group('infra')]
lefthook-validate:
    npx --no-install lefthook validate

# Whole-tree: scripts/no-issue-refs.sh ignores any arguments and always
# checks every tracked in-scope file, so the *files parameter only controls
# whether a caller bothers invoking the recipe at all.
#
# Reject issue-tracker references anywhere in the gated tree.
[group('infra')]
[positional-arguments]
no-issue-refs *files:
    scripts/no-issue-refs.sh "$@"

# Args go through "$@" (shell-quoted) so a path with a space or a crafted name
# cannot split or inject.
#
# Reject plan-artifact references in a passed file list.
[group('infra')]
[positional-arguments]
no-plan-refs *files:
    scripts/no-plan-refs.sh "$@"

# The two ref guards take a file list and are driven per-commit by hooks, so they
# are excluded here.
#
# Whole repo-lint surface (mirrors the CI repo-lint job).
[group('infra')]
lint: shellcheck hadolint typos yamllint sqruff actionlint prose tool-pin-drift vp-pin-drift npm-pin-drift renovate-npm-manager-selftest just-reference-drift adr-frontmatter-selftest greptile-config compose-dev-invariants recipe-secret-echo-test cargo-locked-guard lefthook-validate

# Self-tests of this repository's own scripts, invoked by hand while editing
# one. This recipe gates nothing: it is absent from LANE_ORDER, lanes_for(),
# ALWAYS_LANES and preflight-full, and CI never runs it.
#
# Nothing here has a gate's subject. Ten entries cover local-only developer
# tooling (doctor, dev-server, worktree, the gate runner, the preflight scoper)
# that CI has no stake in whatsoever. The other ten cover guards CI already runs
# for real on every pull request; what those guards need is an assertion inside
# them that fires on a real invocation, not a fixture-driven test that fires
# only where the test runs.
#
# The split is what a check takes as its subject, not what it is called.
# `lint` above keeps everything whose subject is committed configuration, which
# an unrelated change can break: tool-pin-drift, vp-pin-drift, npm-pin-drift,
# just-reference-drift, greptile-config, compose-dev-invariants,
# cargo-locked-guard, lefthook-validate, and three that only look test-shaped,
# renovate-npm-manager-selftest, adr-frontmatter-selftest and
# recipe-secret-echo-test. A new config validator belongs there. Only a check
# whose subject is one of this repository's own scripts belongs here.
#
# Self-tests for this repository's own scripts; gates nothing, run it by hand.
[group('infra')]
selftests: no-plan-refs-selftest no-issue-refs-selftest tool-pin-drift-selftest vp-pin-drift-selftest npm-pin-drift-selftest sanitize-sarif-severities-selftest filter-sarif-test-rules-selftest filter-sarif-unfixable-os-selftest compose-dev-invariants-selftest dev-server-selftest backend-dev-env-selftest doctor-selftest db-ready-selftest require-disk-backed-selftest worktree-selftest worktree-base-selftest preflight-scope-selftest gate-run-selftest gate-detach-selftest cargo-locked-guard-selftest

# Infra has no formatter (oxfmt owns formatting via the JS plane), so check == lint.
[group('infra')]
check: lint
