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

# Lint GitHub Actions workflow files.
[group('infra')]
actionlint:
    actionlint -color

# Vale prose linter over all in-scope Markdown (docs/src, adr, repo-root).
[group('infra')]
prose:
    scripts/vale-lint.sh --all

# Self-test the Vale ruleset (rules fire on bad prose, stay silent on clean).
[group('infra')]
vale-selftest:
    scripts/vale-selftest.sh

# Self-test the DCO gate (dco-check.sh verdicts on fixture histories).
[group('infra')]
dco-selftest:
    scripts/dco-selftest.sh

# Self-test the private-plan reference guard.
[group('infra')]
no-plan-refs-selftest:
    scripts/no-plan-refs-selftest.sh

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

# Validate canonical ADR frontmatter and lifecycle index coverage.
[group('infra')]
adr-frontmatter-selftest:
    scripts/adr-frontmatter-selftest.sh

# Self-test the SARIF severity sanitizer both Snyk upload jobs run.
[group('infra')]
sanitize-sarif-severities-selftest:
    scripts/sanitize-sarif-severities-selftest.sh

# Test the vale-lint.sh scope policy (include/exclude classification).
[group('infra')]
vale-scope-test:
    scripts/vale-scope-test.sh

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

# Assert the backend dev recipes defer to `.env` instead of overriding it.
[group('infra')]
backend-dev-env-selftest:
    scripts/backend-dev-env-selftest.sh

# Self-test `just doctor` (missing-binary, fully-stubbed happy path, warn-only path).
[group('infra')]
doctor-selftest:
    scripts/doctor-selftest.sh

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
lint: shellcheck hadolint typos yamllint actionlint prose vale-selftest vale-scope-test dco-selftest no-plan-refs-selftest tool-pin-drift tool-pin-drift-selftest just-reference-drift adr-frontmatter-selftest sanitize-sarif-severities-selftest greptile-config compose-dev-invariants compose-dev-invariants-selftest recipe-secret-echo-test dev-server-selftest backend-dev-env-selftest doctor-selftest lefthook-validate

# Infra has no formatter (oxfmt owns formatting via the JS plane), so check == lint.
[group('infra')]
check: lint
