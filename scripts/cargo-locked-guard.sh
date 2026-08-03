#!/usr/bin/env bash
# Guard: every dependency-resolving cargo invocation on a build or gate path
# passes --locked, so nothing can take a version the lockfile never recorded
# (adr/2026-08-03-package-ingress-default-deny.md, the frozen-installs
# control).
#
# Polarity: any `cargo <subcommand>` line is a violation unless it carries
# --locked, its subcommand is one of the named non-resolving few, or it is
# the one documented exemption. The non-resolving set is the closed,
# knowable one; the resolving set is unbounded (cargo grows subcommands), so
# an unknown subcommand fails loudly and gets a deliberate ruling instead of
# passing silently.
#
# Inputs are derived, not hand-listed, so a new just module or workflow is
# scanned by construction: the root justfile, every root *.just module, the
# workflow files, and lefthook.yml. The Dockerfile is deliberately not an
# input: the image build copies a committed manifest-and-lockfile pair, so
# nothing can drift inside a build, and its cargo lines answer to the image
# lanes rather than this lint.
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
  files=(justfile lefthook.yml)
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
# `sqlx migrate` replays SQL files through sqlx-cli. Everything else cargo
# can do is presumed to resolve until ruled non-resolving here.
is_exempt_sub() { # <subcommand> <next-word>
  case "$1" in
    fmt | machete | deny) return 0 ;;
    sqlx) [ "${2:-}" = "migrate" ] && return 0 ;;
  esac
  return 1
}

# `cargo ` followed by a word, so `cargo-machete` the binary name and
# `scripts/cargo-locked-guard.sh` the path never match. /dev/null forces the
# file:line: prefix even for a single input.
rc=0
matches="$(grep -nE 'cargo[[:space:]]+[a-z]' "${files[@]}" /dev/null)" || rc=$?
if [ "$rc" -gt 1 ]; then
  echo "cargo-locked-guard: grep failed (exit ${rc}); refusing to report a clean tree" >&2
  exit "$rc"
fi

fail=0
while IFS= read -r match; do
  [ -n "$match" ] || continue
  text="${match#*:*:}"
  # Skip lines that are comments from their first non-blank character; a
  # trailing comment does not exempt the code before it, so --locked and the
  # subcommand are read from the pre-# portion only.
  trimmed="${text#"${text%%[![:space:]]*}"}"
  case "$trimmed" in
    '#'*) continue ;;
  esac
  code="${text%%#*}"
  case "$code" in
    *cargo*) ;;
    *) continue ;; # the cargo word sat inside the trailing comment
  esac
  # An echo line cannot invoke cargo: its mention is message prose (a
  # workflow error message naming `cargo auditable`, a fail_text hint).
  case "$trimmed" in
    'echo '* | 'printf '*) continue ;;
  esac
  # The ADR's one named gap: sqlx-cli's own dependency tree resolves fresh
  # at install; the version pin is held in lockstep with the sqlx crate by
  # the pin-match CI step. Exempted by exact content so any other
  # `cargo install` still fails.
  case "$code" in
    *'cargo install sqlx-cli'*) continue ;;
    *'--locked'*) continue ;;
  esac
  read -ra words <<< "$code"
  sub='' nxt=''
  for i in "${!words[@]}"; do
    if [ "${words[$i]}" = "cargo" ]; then
      sub="${words[$((i + 1))]:-}"
      nxt="${words[$((i + 2))]:-}"
      break
    fi
  done
  [ -n "$sub" ] || continue
  if is_exempt_sub "$sub" "$nxt"; then
    continue
  fi
  printf 'cargo-locked-guard: missing --locked: %s\n' "$match" >&2
  fail=1
done <<< "$matches"

if [ "$fail" -ne 0 ]; then
  echo 'cargo-locked-guard: add --locked, or extend the non-resolving exemption in this script if the subcommand genuinely resolves nothing' >&2
  exit 1
fi
echo 'cargo-locked-guard: every resolving cargo invocation passes --locked'
