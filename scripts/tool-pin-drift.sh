#!/usr/bin/env bash
# Keep repository-local lint tool pins aligned with CI installers.
set -euo pipefail

pin() {
  awk -F ' *= *' -v key="$1" '$1 == key { gsub(/"/, "", $2); print $2 }' mise.toml
}

require_ci() {
  local tool="$1" version="$2" pattern="$3"
  [ -n "$version" ] || { echo "missing ${tool} pin in mise.toml" >&2; return 1; }
  grep -Fq "$pattern" .github/workflows/ci.yml || {
    echo "${tool} ${version} does not match its CI pin" >&2
    return 1
  }
}

require_ci actionlint "$(pin actionlint)" "version: \"$(pin actionlint)\""
require_ci hadolint "$(pin hadolint)" "version: \"$(pin hadolint)\""
require_ci just "$(pin just)" "just@$(pin just)"
require_ci shellcheck "$(pin shellcheck)" "version: \"$(pin shellcheck)\""
require_ci typos "$(pin typos)" "typos@$(pin typos)"
require_ci vale "$(pin vale)" "version: \"$(pin vale)\""
require_ci yamllint "$(pin yamllint)" "yamllint==$(pin yamllint)"

echo "repository tool pins match CI"
