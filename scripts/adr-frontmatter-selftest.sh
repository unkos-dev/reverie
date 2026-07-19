#!/usr/bin/env bash
# Validate the canonical ADR metadata shape and lifecycle index coverage.
set -euo pipefail

fail=0
while IFS= read -r file; do
  case "$file" in
    adr/AGENTS.md | adr/CLAUDE.md | adr/README.md | adr/TEMPLATE.md) continue ;;
  esac

  frontmatter="$(awk 'NR == 1 && $0 != "---" { exit 2 } NR > 1 && $0 == "---" { exit } NR > 1 { print }' "$file")" || {
    echo "FAIL ${file}: invalid frontmatter boundary" >&2
    fail=1
    continue
  }

  for field in status date supersedes decision-makers consulted informed; do
    if ! grep -Eq "^${field}:" <<<"$frontmatter"; then
      echo "FAIL ${file}: missing ${field}" >&2
      fail=1
    fi
  done
  if ! grep -Eq '^consulted: (\[\]|\[".+"\]|".+")$' <<<"$frontmatter"; then
    echo "FAIL ${file}: consulted must be [] or a non-empty quoted value" >&2
    fail=1
  fi
  if ! grep -Fqx 'decision-makers: "John Unkovich"' <<<"$frontmatter"; then
    echo "FAIL ${file}: non-canonical decision-makers value" >&2
    fail=1
  fi

  index_path="${file#adr/}"
  if ! grep -Fq "(${index_path})" adr/README.md; then
    echo "FAIL ${file}: missing from adr/README.md" >&2
    fail=1
  fi
done < <(git ls-files 'adr/*.md' 'adr/superseded/*.md')

[ "$fail" -eq 0 ] || exit 1
echo "ADR frontmatter and index coverage passed"
