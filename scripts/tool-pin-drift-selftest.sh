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

[tools."github:nextest-rs/nextest"]
version = "0.9.140"
version_prefix = "cargo-nextest-"
filter_bins = "cargo-nextest"
EOF
expect "github backend binary pin rejected" 1 'tool: cargo-nextest@0.9.140'

exit "$fail"
