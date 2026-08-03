#!/usr/bin/env bash
# Shared filesystem-type guard: a RAM-backed filesystem (tmpfs/ramfs) does
# not survive a reboot, and building on one spends memory rather than disk.
# `just worktree` refuses to create a worktree there, and `just doctor`'s
# low-disk warning names the same condition when it fires on one; both reuse
# this one detector rather than keeping their own copies of the `stat`
# invocation in step.
#
# Usage:
#   scripts/require-disk-backed.sh <path>            enforce: exit 1 with a
#                                                      message when <path> is
#                                                      tmpfs/ramfs, else 0
#   scripts/require-disk-backed.sh --fstype <path>    detect only: print the
#                                                      filesystem type (empty
#                                                      if it cannot be read),
#                                                      always exit 0
#
# `stat -f -c %T` is GNU-only; BSD stat (macOS) rejects the flag combination.
# Both modes degrade the same way on a non-GNU stat: print a warning and
# treat the check as skipped rather than as a pass, so a caller cannot read
# "we could not tell" as "we checked and it is fine". Enforcement mode's exit
# status alone cannot carry that distinction (skipped and confirmed-safe are
# both 0), which is why the warning goes to stderr rather than being dropped;
# a caller that discards stderr accepts the same silent-skip risk the
# original inline check already carried.
set -ueo pipefail

detect_fstype() { # <path> -- prints the filesystem type, or nothing if unreadable
  stat -f -c %T "$1" 2>/dev/null || true
}

if [ "${1:-}" = '--fstype' ]; then
  path="${2:?usage: scripts/require-disk-backed.sh --fstype <path>}"
  detect_fstype "$path"
  exit 0
fi

path="${1:?usage: scripts/require-disk-backed.sh <path>}"

if fstype="$(stat -f -c %T "$path" 2>/dev/null)"; then
  case "$fstype" in
    tmpfs | ramfs)
      echo "require-disk-backed: ${path} is on ${fstype}, a RAM-backed filesystem" >&2
      exit 1
      ;;
  esac
else
  echo "warning: cannot read the filesystem type of ${path} (non-GNU stat); the disk-backed check did not run" >&2
fi
