#!/usr/bin/env bash
# Append the concrete install directories of the mise tools active in this
# repository to GITHUB_PATH, so tools resolve to real binaries rather than
# to mise shims.
#
# jdx/mise-action computes exactly this PATH and then drops it: it exports
# every variable from `mise env --json` *except* PATH, and adds only the
# shims directory. A shim re-resolves its version on each exec from the mise
# config discovered at the caller's working directory. That is invisible for
# a step whose working directory is the checkout, and fatal for a program the
# toolchain spawns from somewhere else: rustc links each unit from that
# unit's package root, so linking a registry dependency runs the linker from
# ~/.cargo/registry, where no mise.toml is discoverable and the shim fails
# with "No version is set for shim".
#
# Resolving once, here, also drops a mise process spawn from every single
# linker invocation.
#
# Usage: scripts/ci/mise-path.sh   (run from the repository root, in CI)
set -euo pipefail

if [ -z "${GITHUB_PATH:-}" ]; then
  echo "$0: GITHUB_PATH is unset; this script is for GitHub Actions" >&2
  exit 2
fi

# `mise env` reports the PATH mise would set for the tools this repository's
# config selects. Only the tool install directories are wanted: the rest of
# the entries are the ambient PATH, and re-appending those would shuffle the
# job's own precedence.
install_dirs="$(mise env --json | jq -r '.PATH' | tr ':' '\n' | grep '/installs/')"

if [ -z "$install_dirs" ]; then
  echo "$0: mise reported no tool install directories; expected at least one" >&2
  echo "$0: mise env PATH was:" >&2
  mise env --json | jq -r '.PATH' >&2
  exit 1
fi

printf '%s\n' "$install_dirs" >>"$GITHUB_PATH"
echo "Added to PATH:"
printf '  %s\n' "$install_dirs"
