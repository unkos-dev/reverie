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

# `.` matches the zizmor-action default input. --config is passed explicitly
# rather than relying on discovery: from a linked worktree zizmor does not find
# .github/zizmor.yml, so every suppression in it silently stops applying and
# the run fails on a finding the file accepts with a written justification.
# That is the checkout `just worktree` creates, so discovery cannot be trusted
# where this recipe is most often run. Exit
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
    zizmor --config .github/zizmor.yml .

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
    zizmor --offline --config .github/zizmor.yml .

# index --check runs first so a stale generated view is reported as drift, not
# as a validation failure of the artifacts it was derived from.
#
# Check the specification corpus (generated views current, artifacts valid).
[group('infra')]
specful:
    specful index --check
    specful validate

# Every tracked Markdown file, so nothing rots unlinted. The lefthook job runs
# vale over staged files whatever their path, so a narrower gate than this one
# does not spare anyone the findings: it only defers them to whoever next edits
# a file, as a surprise their change did not cause. Scope is taken from git
# rather than a directory list for the same reason.
#
# The two exclusions are carved in .vale.ini, not here, so the hook honours them
# too: agent-process instructions, and the third-party CodeGuard rules that
# AGENTS.md forbids editing.
#
# Vale prose linter over every tracked Markdown file.
[group('infra')]
prose:
    git ls-files -z '*.md' | xargs -0 -r vale

# Self-test the base-OS finding filter, including that a distro CVE with
# a fix available is never withheld.
[group('infra')]
filter-sarif-unfixable-os-selftest:
    scripts/filter-sarif-unfixable-os-selftest.sh

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

# Every dependency-resolving cargo invocation in the just recipes must pass --locked; the package-ingress ADR records why.
[group('infra')]
cargo-locked-guard:
    scripts/cargo-locked-guard.sh

# Self-test the detached-gate script `just preflight-detach` wraps: a passing and a failing run each need a log and a gate-status-readable record.
[group('infra')]
gate-detach-selftest:
    scripts/gate-detach-selftest.sh

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
# Whole repo-lint surface (the non-JS half of CI's lint concern).
[group('infra')]
lint: shellcheck hadolint typos yamllint sqruff actionlint prose specful filter-sarif-unfixable-os-selftest preflight-scope-selftest cargo-locked-guard

# Everything left here covers local-only developer tooling, and that is now the
# entry criterion. CI has no stake in `just doctor`, `just worktree`, or the
# dev-server lifecycle: nothing CI does breaks when one of them regresses. So
# this recipe gates nothing, and is absent from LANE_ORDER, lanes_for(),
# ALWAYS_LANES and preflight-full. The developer editing one of these scripts
# runs it by hand, which is the only moment its verdict is worth anything.
#
# What used to sit here alongside them were self-tests of guards CI runs for
# real on every pull request. A fixture beside a guard proves the guard passes
# against a planted case; an assertion inside it fires on every real invocation,
# which is strictly more coverage for a tenth of the lines. Those guards carry
# their own assertions now and the fixtures are gone.
#
# A check earns a place in `lint` by catching a failure that has actually
# happened here, not one that could. Config validators added alongside the
# config they validate do not qualify. cargo-locked-guard is the exception, and
# carries ADR authority rather than a fire history; the other two answer for
# themselves below.
#
# filter-sarif-unfixable-os-selftest guards a suppression predicate that decides
# which container CVEs reach the code-scanning dashboard. The script fails
# closed against upstream rewording, but nothing stops an edit that widens the
# match from withholding findings that do have fixes, and the assertion that
# would catch that is the predicate itself, so it cannot live inside the script.
#
# preflight-scope-selftest guards the scoper that decides which lanes a local
# gate runs, including the lane holding this check. Break the `scripts/*` arm of
# its escalates() and a change to the scoper itself narrows its own gate from
# sixteen lanes to one, prints nothing, and exits clean. No assertion inside the
# scoper can catch that, because what needs asserting is which lanes were
# selected and the selector is the code under suspicion. Gate placement is
# circular for the same reason, so CI's repo-lint job is what actually holds it:
# that job is unconditional and never consults the scoper.
#
# Self-tests for this repository's local developer tooling; gates nothing, run it by hand.
[group('infra')]
selftests: dev-server-selftest backend-dev-env-selftest doctor-selftest require-disk-backed-selftest worktree-selftest worktree-base-selftest gate-detach-selftest

# Infra has no formatter (oxfmt owns formatting via the JS plane), so check == lint.
[group('infra')]
check: lint
