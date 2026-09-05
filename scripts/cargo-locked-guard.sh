#!/usr/bin/env bash
# Guard: every dependency-resolving cargo invocation on a build or gate path
# passes --locked, so nothing can take a version the lockfile never recorded
# (docs/adr/0045-package-ingress-default-deny-controls-no-per-package-allowances.md, the frozen-installs
# control).
#
# Polarity: any `cargo <subcommand>` is a violation unless it carries
# --locked, its subcommand is one of the named non-resolving few, or it is
# the one documented exemption. The non-resolving set is the closed,
# knowable one; the resolving set is unbounded (cargo grows subcommands), so
# an unknown subcommand fails loudly and gets a deliberate ruling instead of
# passing silently.
#
# Granularity: decisions are per command segment, not per line. A line is
# split on &&, ||, ; and | and each segment is judged alone, so `--locked`
# on one invocation cannot vouch for another, and an `echo` naming cargo in
# its message exempts only its own segment. A trailing comment starts at
# whitespace-then-#, so `${VAR#pattern}` expansions survive into the code.
#
# Inputs are derived, not hand-listed, so a new just module or workflow is
# scanned by construction: the root justfile, every root *.just module, the
# workflow files, lefthook.yml, and the Dockerfile. In the image build,
# `chef prepare` reads manifests without resolving; `chef cook` and
# `auditable build` resolve and are locked like everything else, which
# matters most for auditable: it embeds the resolved dependency list into
# the shipped binary as SBOM provenance.
#
# Failure discipline: a guard that did no work must not read as clean. An
# unreadable input aborts (exit 2), and grep exit statuses above 1 abort
# rather than being swallowed; only "no matches at all" (exit 1) is clean.
#
# Usage: cargo-locked-guard.sh [file...]   defaults to the derived set
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

if [ "$#" -gt 0 ]; then
  files=("$@")
else
  files=(justfile lefthook.yml Dockerfile)
  for f in *.just .github/workflows/*.yml; do
    files+=("$f")
  done
fi

for f in "${files[@]}"; do
  if [ ! -r "$f" ]; then
    echo "cargo-locked-guard: cannot read input ${f}; refusing to report a clean tree" >&2
    exit 2
  fi
done

# fmt reads the tree; machete and deny read the lockfile without resolving;
# `sqlx migrate` replays SQL files through sqlx-cli; `chef prepare` reads
# manifests into a recipe. Everything else cargo can do is presumed to
# resolve until ruled non-resolving here.
is_exempt_sub() { # <subcommand> <next-word>
  case "$1" in
    fmt | machete | deny) return 0 ;;
    sqlx) [ "${2:-}" = "migrate" ] && return 0 ;;
    chef) [ "${2:-}" = "prepare" ] && return 0 ;;
  esac
  return 1
}

# `cargo ` followed by a subcommand or a +toolchain prefix, so
# `cargo-machete` the binary name and `scripts/cargo-locked-guard.sh` the
# path never match. /dev/null forces the file:line: prefix even for a
# single input.
rc=0
matches="$(grep -nE 'cargo[[:space:]]+[+a-z]' "${files[@]}" /dev/null)" || rc=$?
if [ "$rc" -gt 1 ]; then
  echo "cargo-locked-guard: grep failed (exit ${rc}); refusing to report a clean tree" >&2
  exit "$rc"
fi

fail=0
while IFS= read -r match; do
  [ -n "$match" ] || continue
  text="${match#*:*:}"
  # A line that is a comment from its first non-blank character never runs.
  trimmed="${text#"${text%%[![:space:]]*}"}"
  case "$trimmed" in
    '#'*) continue ;;
  esac
  # Trailing comments start at whitespace-then-#; a bare # glued to text is
  # parameter-expansion syntax, not a comment.
  code="${text%%[[:space:]]#*}"
  # Judge each command segment alone.
  segs="${code//&&/$'\n'}"
  segs="${segs//||/$'\n'}"
  segs="${segs//;/$'\n'}"
  segs="${segs//|/$'\n'}"
  while IFS= read -r seg; do
    read -ra words <<< "$seg" || true
    [ "${#words[@]}" -gt 0 ] || continue
    # An echo or printf segment cannot invoke cargo: its mention is message
    # prose (a workflow error message, a fail_text hint).
    case "${words[0]}" in
      echo | printf) continue ;;
    esac
    sub='' nxt=''
    for i in "${!words[@]}"; do
      if [ "${words[$i]}" = "cargo" ]; then
        sub="${words[$((i + 1))]:-}"
        case "$sub" in
          +*) # toolchain prefix: the subcommand is the word after it
            sub="${words[$((i + 2))]:-}"
            nxt="${words[$((i + 3))]:-}"
            ;;
          *)
            nxt="${words[$((i + 2))]:-}"
            ;;
        esac
        break
      fi
    done
    [ -n "$sub" ] || continue
    # The ADR's one named gap: sqlx-cli's own dependency tree resolves
    # fresh at install; the version pin is held in lockstep with the sqlx
    # crate by the pin-match CI step. Exempted by exact content so any
    # other `cargo install` still fails.
    case "$seg" in
      *'cargo install sqlx-cli'*) continue ;;
      *'--locked'*) continue ;;
    esac
    if is_exempt_sub "$sub" "$nxt"; then
      continue
    fi
    printf 'cargo-locked-guard: missing --locked: %s\n' "$match" >&2
    fail=1
    break # one report per line is enough
  done <<< "$segs"
done <<< "$matches"

if [ "$fail" -ne 0 ]; then
  echo 'cargo-locked-guard: add --locked, or extend the non-resolving exemption in this script if the subcommand genuinely resolves nothing' >&2
  exit 1
fi
echo 'cargo-locked-guard: every resolving cargo invocation passes --locked'
