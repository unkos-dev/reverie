# Infra plane: whole-tree repo-lint gates. Runs from the repo root (git ls-files is
# repo-relative), so no working-directory. Tools are system-PATH / image-baked.
# pipefail in `set shell` is load-bearing: without it a failing `git ls-files` in a
# pipe would be masked by a passing xargs.

set shell := ["bash", "-ueo", "pipefail", "-c"]

# renovate: datasource=pypi depName=sqlfluff
sqlfluff_version := "4.5.0"

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

# Lint every non-ignored tracked SQL file, including parser failures.
[group('infra')]
sqlfluff:
    git ls-files -z --cached --ignored --exclude-from=.sqlfluffignore '--format=:(exclude,literal)%(path)' -- '*.sql' | { \
        mapfile -d '' -t excludes; \
        git ls-files -z -- '*.sql' "${excludes[@]}" | { \
            mapfile -d '' -t files; \
            test "${#files[@]}" -gt 0; \
            uvx --no-build sqlfluff@{{sqlfluff_version}} lint --ignore-local-config --config .sqlfluff "${files[@]}"; \
        }; \
    }

# Lint GitHub Actions workflow files.
[group('infra')]
jactionlint:
    jactionlint --color=always

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

# Lint the OpenAPI contract with the CI security rules and baseline.
[group('infra')]
openapi:
    vacuum lint --ruleset .vacuum.yaml --ignore-file .vacuum-ignore.yaml --fail-severity info --details backend/openapi.json

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

# Every dependency-resolving cargo invocation in the just recipes must pass --locked; the package-ingress ADR records why.
[group('infra')]
cargo-locked-guard:
    scripts/cargo-locked-guard.sh

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
lint: shellcheck hadolint typos sqlfluff jactionlint prose specful filter-sarif-unfixable-os-selftest cargo-locked-guard

# Infra has no formatter (oxfmt owns formatting via the JS plane), so check == lint.
[group('infra')]
check: lint
