#!/usr/bin/env bash
# Validate the canonical ADR metadata shape and lifecycle index coverage.
set -euo pipefail

fail=0
while IFS= read -r file; do
  case "$file" in
    adr/AGENTS.md | adr/CLAUDE.md | adr/README.md | adr/TEMPLATE.md) continue ;;
  esac

  frontmatter="$(awk '
    NR == 1 && $0 != "---" { exit 2 }
    NR > 1 && $0 == "---" { closed = 1; exit }
    NR > 1 { print }
    END { if (!closed) exit 2 }
  ' "$file")" || {
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

  status="$(sed -n 's/^status:[[:space:]]*//p' <<<"$frontmatter" | head -n 1 | tr -d '"')"
  case "$file" in
    adr/superseded/*)
      if [ "$status" != "superseded" ]; then
        echo "FAIL ${file}: records under superseded/ must carry status superseded" >&2
        fail=1
      fi
      if ! grep -Eq '^superseded-by: \[".+"\]$' <<<"$frontmatter"; then
        echo "FAIL ${file}: superseded records must link superseded-by" >&2
        fail=1
      fi
      ;;
    *)
      if [ "$status" = "superseded" ]; then
        echo "FAIL ${file}: status superseded requires the superseded/ directory" >&2
        fail=1
      fi
      ;;
  esac

  # Active entries render as "(<status>, <date>)"; superseded entries render as
  # "(superseded by [replacement](...), <date>)".
  expected_marker="(${status},"
  if [ "$status" = "superseded" ]; then
    expected_marker="(superseded"
  fi
  index_path="${file#adr/}"
  index_line="$(grep -F "(${index_path})" adr/README.md || true)"
  if [ -z "$index_line" ]; then
    echo "FAIL ${file}: missing from adr/README.md" >&2
    fail=1
  elif ! grep -Fq "$expected_marker" <<<"$index_line"; then
    echo "FAIL ${file}: adr/README.md entry does not carry status ${status}" >&2
    fail=1
  fi
done < <(git ls-files 'adr/*.md' 'adr/superseded/*.md')

[ "$fail" -eq 0 ] || exit 1
echo "ADR frontmatter and index coverage passed"
