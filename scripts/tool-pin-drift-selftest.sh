#!/usr/bin/env bash
set -euo pipefail

root="$(git rev-parse --show-toplevel)"
checker="${root}/scripts/tool-pin-drift.sh"
tmp="$(mktemp -d)"
trap 'rm -rf "${tmp}"' EXIT

mkdir -p "${tmp}/.github/workflows"
printf '[tools]\nactionlint = "1.7.12"\njust = "1.56.0"\nyamllint = "1.38.0"\n' >"${tmp}/mise.toml"

fail=0
expect() {
  local name="$1" want="$2" text="$3" got=0
  printf '%s\n' "$text" >"${tmp}/.github/workflows/ci.yml"
  (cd "$tmp" && "$checker") >/dev/null 2>&1 || got=$?
  if [ "$got" -ne "$want" ]; then
    echo "FAIL ${name}: expected exit ${want}, got ${got}"
    fail=1
  else
    echo "ok   ${name}"
  fi
}

expect "clean workflow passes" 0 'run: echo just testing things'
expect "inline at-pin rejected" 1 'tool: just@1.53.0'
expect "inline pip pin rejected" 1 'run: pipx install yamllint==1.33.0'
expect "annotation with split version line rejected" 1 '# renovate: datasource=github-releases depName=rhysd/actionlint extractVersion=^v(?<version>.+)$'
expect "bare-name annotation rejected" 1 '# renovate: datasource=github-releases depName=just'
expect "unrelated annotation passes" 0 '# renovate: datasource=github-releases depName=aquasecurity/trivy extractVersion=^v(?<version>.+)$'
expect "prose tool name without version passes" 0 '# actionlint and CodeQL cover different classes'

cat >>"${tmp}/mise.toml" <<'EOF'

[tools."github:owner/filter-tool"]
version = "1.2.3"
filter_bins = "widget"

[tools."github:owner/bin-tool"]
version = "4.5.6"
bin = "gadget"

[tools."cargo:cargo-test-tool"]
version = "1.0.0"
EOF
expect "github backend filter_bins pin rejected" 1 'tool: widget@1.2.3'
expect "github backend bin pin rejected" 1 'tool: gadget@4.5.6'
expect "cargo backend pin rejected" 1 'tool: cargo-test-tool@1.0.0'

# An inline entry can carry a backend prefix too. Until the census stripped it
# the derived patterns held the literal `npm:npm`, which cannot match the bare
# tool name a workflow would pin, so the tool was silently uncovered.
printf '[tools]\n"npm:npm" = "11.18.0"\n' >"${tmp}/mise.toml"
expect "inline backend-prefixed inline pin rejected" 1 'run: npm install -g npm@11.19.0'
expect "inline backend-prefixed annotation rejected" 1 '# renovate: datasource=npm depName=npm'
expect "tool name without a version still passes" 0 'run: npm ci'

# node is a mise-managed tool, so a workflow line re-pinning it must be
# rejected like any other, in both shapes this guard matches: the depName
# annotation and a literal at-pin.
printf '[tools]\nnode = "24.18.1"\n' >"${tmp}/mise.toml"
expect "node annotation pin rejected" 1 '# renovate: datasource=node-version depName=node'
expect "node inline at-pin rejected" 1 'tool: node@24'

exit "$fail"
