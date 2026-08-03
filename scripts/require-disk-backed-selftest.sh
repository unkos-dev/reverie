#!/usr/bin/env bash
# Self-test scripts/require-disk-backed.sh: the shared tmpfs/ramfs guard
# `just worktree` and `just doctor` both call. Exercises the real script
# against real filesystems (a tmpfs specimen and a disk-backed one), plus a
# stubbed non-GNU `stat` for the warn-but-continue path no real host on this
# repo's CI runner can reach on demand.
set -ueo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd -P)"
guard="${repo_root}/scripts/require-disk-backed.sh"

failures=0
pass() { printf 'ok   %s\n' "$1"; }
fail() {
  printf 'FAIL %s\n  %s\n' "$1" "$2"
  failures=$((failures + 1))
}

# --- a disk-backed path is accepted -------------------------------------

# The checkout itself is never tmpfs: nothing about this repo's own
# development story works from one (see `just worktree`'s own guard, which
# this script now implements). It is therefore a reliable disk-backed
# specimen on every machine this test runs on.
rc=0
out="$("$guard" "$repo_root" 2>&1)" || rc=$?
if [ "$rc" -ne 0 ]; then
  fail 'a disk-backed path is accepted' "exited ${rc}: ${out}"
elif [ -n "$out" ]; then
  fail 'a disk-backed path produces no warning' "output: ${out}"
else
  pass 'a disk-backed path is accepted, silently'
fi

fstype_out="$("$guard" --fstype "$repo_root")"
if [ -z "$fstype_out" ]; then
  fail '--fstype reports a type for a disk-backed path' 'got empty output'
elif [ "$fstype_out" = tmpfs ] || [ "$fstype_out" = ramfs ]; then
  fail '--fstype does not report the checkout as RAM-backed' "got: ${fstype_out}"
else
  pass "--fstype reports the checkout's real filesystem type (${fstype_out})"
fi

# --- a tmpfs/ramfs path is rejected --------------------------------------

# /dev/shm is tmpfs on every Linux CI runner and dev machine this repo
# targets; a path under /tmp is the documented fallback specimen when it is
# not (the two are never both absent on a POSIX host). A machine where
# neither is tmpfs (non-Linux, or a real-disk-backed /tmp) has nothing to
# reject against, so the assertion is skipped rather than forced, with a
# note explaining why: forcing it would fabricate a condition that does not
# hold on this host, and skipping it is loud, not silent.
tmpfs_specimen=''
for candidate in /dev/shm /tmp; do
  if [ -d "$candidate" ]; then
    candidate_fstype="$(stat -f -c %T "$candidate" 2>/dev/null || true)"
    case "$candidate_fstype" in
      tmpfs | ramfs)
        tmpfs_specimen="$candidate"
        break
        ;;
    esac
  fi
done

if [ -z "$tmpfs_specimen" ]; then
  printf 'skip %s\n' 'a tmpfs/ramfs path is rejected -- no tmpfs specimen at /dev/shm or /tmp on this host'
else
  rc=0
  out="$("$guard" "$tmpfs_specimen" 2>&1)" || rc=$?
  if [ "$rc" -eq 0 ]; then
    fail 'a tmpfs/ramfs path is rejected' "exited 0 for ${tmpfs_specimen}: ${out}"
  elif ! printf '%s' "$out" | grep -qF 'RAM-backed filesystem'; then
    fail 'the rejection names the RAM-backed condition' "output: ${out}"
  else
    pass "a tmpfs/ramfs path (${tmpfs_specimen}) is rejected with a clear message"
  fi

  fstype_out="$("$guard" --fstype "$tmpfs_specimen")"
  case "$fstype_out" in
    tmpfs | ramfs) pass "--fstype reports the tmpfs specimen's real type (${fstype_out})" ;;
    *) fail '--fstype reports the tmpfs specimen as tmpfs or ramfs' "got: ${fstype_out}" ;;
  esac
fi

# --- a non-GNU stat degrades to a warning, not a silent pass -------------

# BSD/macOS stat rejects `-f -c`, the exact combination this guard relies
# on. A stub reproducing that failure exercises the fallback without
# needing a non-GNU host: real stat is passed through for every other
# invocation the test harness itself makes, and only the guard's own call
# (recognized by its exact argument shape) fails.
stub_dir="$(mktemp -d)"
trap 'rm -rf "$stub_dir"' EXIT
cat > "${stub_dir}/stat" << 'EOF'
#!/usr/bin/env bash
if [ "$1" = '-f' ] && [ "$2" = '-c' ] && [ "$3" = '%T' ]; then
  echo "stat: illegal option -- f" >&2
  exit 1
fi
exec "$(command -v -p stat)" "$@"
EOF
chmod +x "${stub_dir}/stat"

rc=0
out="$(PATH="${stub_dir}:${PATH}" "$guard" "$repo_root" 2>&1)" || rc=$?
if [ "$rc" -ne 0 ]; then
  fail 'a non-GNU stat degrades to a warning, not a failure' "exited ${rc}: ${out}"
elif ! printf '%s' "$out" | grep -qF 'the disk-backed check did not run'; then
  fail 'the skipped check says so, so it cannot read as a pass' "output: ${out}"
else
  pass 'a non-GNU stat warns and continues, rather than silently passing'
fi

fstype_out="$(PATH="${stub_dir}:${PATH}" "$guard" --fstype "$repo_root" 2>/dev/null)"
if [ -n "$fstype_out" ]; then
  fail '--fstype prints nothing when stat cannot answer' "got: ${fstype_out}"
else
  pass '--fstype prints nothing (not a guess) when stat cannot answer'
fi

if [ "$failures" -ne 0 ]; then
  printf '\n%d require-disk-backed assertion(s) failed\n' "$failures" >&2
  exit 1
fi
echo 'OK: require-disk-backed rejects tmpfs/ramfs, accepts disk-backed paths, and degrades cleanly on non-GNU stat'
