#!/usr/bin/env bash
#
# Consumer self-test for Markdownlint's YAML configuration path. It keeps both
# valid and deliberately malformed fixtures outside the repository tree.
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
cli="${repo_root}/node_modules/.bin/markdownlint-cli2"

if [ ! -x "${cli}" ]; then
  echo "markdownlint self-test: missing node_modules binary; run npm ci" >&2
  exit 1
fi

tmp="$(mktemp -d)"
trap 'rm -rf "${tmp}"' EXIT
mkdir -p "${tmp}/valid" "${tmp}/malformed"

printf '%s\n' \
  'config:' \
  '  default: true' \
  '  MD013: false' \
  '  MD025: false' \
  >"${tmp}/valid/.markdownlint-cli2.yaml"
printf '%s\n' \
  '---' \
  'title: Valid document' \
  'items:' \
  '  - first' \
  '---' \
  '' \
  '# Valid document' \
  '' \
  'Body text.' \
  >"${tmp}/valid/document.md"

(cd "${tmp}/valid" && "${cli}" document.md) >/dev/null
echo "ok   valid YAML configuration and front matter"

printf '%s\n' \
  'config:' \
  '  default: true' \
  '  MD013: [' \
  >"${tmp}/malformed/.markdownlint-cli2.yaml"
printf '%s\n' '# Malformed configuration fixture' >"${tmp}/malformed/document.md"

if (cd "${tmp}/malformed" && "${cli}" document.md) >/dev/null 2>&1; then
  echo "FAIL malformed YAML configuration was accepted" >&2
  exit 1
fi
echo "ok   malformed YAML configuration is rejected"

echo "markdownlint self-test: passed"
