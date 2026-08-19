#!/usr/bin/env bash
# Fast, read-only environment self-check: "is this machine ready to develop
# Reverie?" Prints one PASS/WARN/FAIL line per check plus a summary, and
# exits nonzero iff any check FAILed. No writes, no network calls beyond the
# already-running local docker daemon this repo's dev stack owns, the dev
# cluster's own unix socket, and the kache build cache's own unix socket.
set -ueo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd -P)"
cd "$repo_root"

pass_count=0
warn_count=0
fail_count=0

pass() { printf 'PASS %s\n' "$1"; pass_count=$((pass_count + 1)); }
warn() { printf 'WARN %s -- fix: %s\n' "$1" "$2"; warn_count=$((warn_count + 1)); }
fail() { printf 'FAIL %s -- fix: %s\n' "$1" "$2"; fail_count=$((fail_count + 1)); }
info() { printf 'INFO %s\n' "$1"; }

# 1. Required binaries resolve on PATH. jq is included because checks 2, 2b,
# and 7 below all shell out to it; without this, a missing jq shows up
# only as a confusing "(missing: unknown)" deep in another check's output.
for bin in git just docker cargo rustc node npm npx mise jq; do
  if command -v "$bin" >/dev/null 2>&1; then
    pass "binary '${bin}' resolves on PATH"
  else
    fail "binary '${bin}' resolves on PATH" "install ${bin}, then run 'mise install'"
  fi
done

# 1b. vp resolves on PATH. Separate from the loop above because it is a
# standalone binary rather than a mise pin, so `mise install` cannot supply it
# and the loop's fix text would send a contributor somewhere that never helps.
# Every js recipe and four lefthook jobs invoke it directly, so a machine
# without it fails at commit time.
if command -v vp >/dev/null 2>&1; then
  pass "binary 'vp' resolves on PATH"
else
  fail "binary 'vp' resolves on PATH" "see the vite-plus install in .github/CONTRIBUTING.md"
fi

# 2. mise pins installed for this directory's toolset. `ls --current` scopes
# to the config active here (root mise.toml), and `--missing` reports only
# tools that are pinned but not yet installed.
#
# Fail closed: a query that itself errors, or that returns output jq cannot
# parse, must never read as "zero missing". Capturing the mise exit status
# separately from stdout is what makes a future flag rename or output-shape
# change surface as a FAIL instead of silently forging a PASS forever.
if command -v mise >/dev/null 2>&1; then
  mise_rc=0
  missing_json="$(mise ls --current --missing -J 2>/dev/null)" || mise_rc=$?
  missing_count="$(printf '%s' "${missing_json}" | jq 'length' 2>/dev/null)" || missing_count=""
  if [ "${mise_rc}" -ne 0 ] || [ -z "${missing_count}" ]; then
    fail "mise-pinned tools are installed (mise query failed)" "run 'mise ls --current --missing -J' manually and investigate"
  elif [ "${missing_count}" = "0" ]; then
    pass "mise-pinned tools are installed"
  else
    missing_names="$(printf '%s' "${missing_json}" | jq -r 'keys | join(", ")' 2>/dev/null || echo unknown)"
    fail "mise-pinned tools are installed (missing: ${missing_names})" "mise install"
  fi
else
  fail "mise-pinned tools are installed" "install mise, then run 'mise install'"
fi

# 2b. npm resolved on PATH matches the devEngines.packageManager pin in
# package.json. devEngines is what npm itself enforces: every direct npm
# invocation compares the running binary's version against it and hard-fails
# with EBADDEVENGINES on a mismatch, and lefthook's git hooks invoke bare
# npx/npm from the caller's PATH, so this breaks every commit, not just an
# explicit `npm install`. scripts/npm-pin-drift.sh checks the declared pin
# agrees with mise.toml; this checks the binary actually resolved on PATH
# agrees with the declared pin, which mise alone cannot guarantee once a
# stale npm sits ahead of mise's shims.
jq_rc=0
declared_npm="$(jq -r '.devEngines.packageManager.version // ""' package.json 2>/dev/null)" || jq_rc=$?
if [ "${jq_rc}" -ne 0 ]; then
  # Fail closed and name the real fault: a failed jq collapsed into an empty
  # string is indistinguishable from a genuinely absent pin, and "restore
  # the pin" is the wrong fix when jq is missing or package.json unreadable.
  fail "package.json devEngines pin is readable (jq failed)" "run 'jq .devEngines package.json' manually and investigate"
elif command -v npm > /dev/null 2>&1; then
  resolved_npm="$(npm --version 2>/dev/null || true)"
  if [ -z "${resolved_npm}" ]; then
    fail "npm on PATH reports a version" "run 'npm --version' manually and investigate"
  elif [ -z "${declared_npm}" ]; then
    fail "package.json declares devEngines.packageManager.version" "restore the npm pin in package.json devEngines"
  elif [ "${resolved_npm}" != "${declared_npm}" ]; then
    fail "npm on PATH (${resolved_npm}) matches the devEngines.packageManager pin (${declared_npm})" "run 'mise install', and confirm mise's shims precede any other npm on PATH"
  else
    pass "npm on PATH matches the devEngines.packageManager pin (${resolved_npm})"
  fi
else
  fail "npm on PATH matches the devEngines.packageManager pin" "run 'mise install', and confirm mise's shims precede any other npm on PATH"
fi

# 3. Docker daemon reachable.
docker_up=0
if command -v docker >/dev/null 2>&1 && docker info --format '{{.ServerVersion}}' >/dev/null 2>&1; then
  docker_up=1
  pass "docker daemon reachable"
else
  fail "docker daemon reachable" "start the Docker daemon/Docker Desktop"
fi

# 4 & 5. Dev Postgres container health, connection acceptance, and runtime
# role login. pg_isready only proves the server accepts a connection as a
# protocol matter; it says nothing about whether the reverie_app runtime
# role itself can authenticate, so a missing or broken role (per
# docker/init-roles.sql, credentials from backend/README.md) would
# otherwise still read as healthy right up until the backend fails to
# start. SELECT 1 as reverie_app is read-only and mirrors the backend's own
# runtime identity.
container="reverie-postgres"
if [ "${docker_up}" -eq 1 ]; then
  state_status="$(docker inspect --format '{{.State.Status}}' "${container}" 2>/dev/null || true)"
else
  state_status=""
fi

case "${state_status}" in
  running)
    health="$(docker inspect --format '{{if .State.Health}}{{.State.Health.Status}}{{else}}none{{end}}' "${container}" 2>/dev/null || echo none)"
    if [ "${health}" = "healthy" ]; then
      pass "dev Postgres container (${container}) is running and healthy"
    else
      fail "dev Postgres container (${container}) health is '${health}'" "docker logs ${container} (or 'just db-down && just db-up')"
    fi
    if docker exec "${container}" pg_isready -U reverie -d reverie_dev >/dev/null 2>&1; then
      pass "dev Postgres accepts connections (pg_isready)"
    else
      fail "dev Postgres accepts connections (pg_isready)" "just db-down && just db-up"
    fi
    if docker exec -e PGPASSWORD=reverie_app "${container}" psql -h localhost -U reverie_app -d reverie_dev -Atc 'SELECT 1' >/dev/null 2>&1; then
      pass "reverie_app role authenticates (psql SELECT 1)"
    else
      # This probe only knows the dev default password; docker/init-roles.sql
      # reads REVERIE_APP_PASSWORD to seed a non-default one (the same
      # mechanism docker/compose.staging.yml requires), and docker/.env is
      # this compose project's own dotenv file for such overrides. Their
      # presence means a login failure may just be an unprobed custom
      # credential, not a broken role, so degrade to WARN rather than
      # sending an operator into a needless (and data-erasing) `db-reset`.
      if [ -n "${REVERIE_APP_PASSWORD:-}" ] || [ -f docker/.env ]; then
        warn "reverie_app role authenticates (psql SELECT 1)" "custom dev credentials in effect; verify the reverie_app password manually"
      else
        fail "reverie_app role authenticates (psql SELECT 1)" "just db-reset (re-seeds docker/init-roles.sql)"
      fi
    fi
    ;;
  "")
    warn "dev Postgres container (${container}) exists" "just db-up"
    warn "dev Postgres accepts connections (pg_isready)" "just db-up"
    warn "reverie_app role authenticates (psql SELECT 1)" "just db-up"
    ;;
  *)
    warn "dev Postgres container (${container}) is ${state_status}, not running" "just db-up"
    warn "dev Postgres accepts connections (pg_isready)" "just db-up"
    warn "reverie_app role authenticates (psql SELECT 1)" "just db-up"
    ;;
esac

# 5b. Host-side unix-socket reachability. The DB-backed just recipes'
# default DSNs connect over the socket docker/compose.dev.yml bind-mounts
# to the host (see rust.just), so a healthy container whose socket is not
# reachable from the host still strands every DB recipe: exactly the state
# of a container created before the socket mount existed, which db-up
# fixes by recreating it. Probed from the host with psql because that is
# the path the recipes actually take; the in-container checks above cannot
# see a missing host mount. A host without psql skips the probe as INFO
# rather than warning: db-up itself degrades gracefully without psql, so
# absence only reduces diagnostic coverage.
sock_dir="${XDG_STATE_HOME:-$HOME/.local/state}/reverie/pgsock"
if [ "${state_status}" = "running" ]; then
  if command -v psql >/dev/null 2>&1; then
    if psql "postgres:///reverie_dev?host=${sock_dir}&user=reverie&password=reverie&connect_timeout=2" -Atc 'SELECT 1' >/dev/null 2>&1; then
      pass "dev Postgres reachable over the unix socket (${sock_dir})"
    else
      fail "dev Postgres reachable over the unix socket (${sock_dir})" "just db-up (recreates the container with the socket mount)"
    fi
  else
    info "host psql not on PATH; unix-socket probe skipped"
  fi
else
  warn "dev Postgres reachable over the unix socket" "just db-up"
fi

# 6. node_modules present at the workspace root, and not stale against the
# committed lockfile. npm workspaces hoist frontend and docs dependencies
# into the one root node_modules (adr/2026-06-30-adopt-vite-plus-monorepo-toolchain.md),
# so a healthy checkout has no frontend/node_modules or docs/node_modules of
# its own to check.
#
# Every unhealthy state here gets the same advice. `just install` runs
# `npm ci`, which is lockfile-exact and bootstraps from nothing, so it repairs
# an absent tree, an interrupted one, and a stale one alike, and none of the
# three needs a binary a broken install may not have left behind.
if [ -d node_modules ]; then
  pass "root node_modules present"
else
  warn "root node_modules present" "just install"
fi

if [ -f package-lock.json ] && [ -f node_modules/.package-lock.json ]; then
  if [ package-lock.json -nt node_modules/.package-lock.json ]; then
    warn "node_modules stale against package-lock.json" "just install"
  else
    pass "node_modules matches package-lock.json"
  fi
else
  # The install marker (node_modules/.package-lock.json) is missing even
  # though node_modules exists: an incomplete or interrupted install.
  warn "node_modules install incomplete (marker missing)" "just install"
fi

# 7. sqlx offline cache present, with at least one entry that actually
# looks like a query cache record. Any parseable JSON (`jq empty`) would
# accept an unrelated or gutted file, so require the two keys every real
# sqlx query-*.json carries (verified against a committed entry): "query",
# the cached SQL text, and "hash", the cache key sqlx looks entries up by.
sqlx_cache_ok=0
if [ -d backend/.sqlx ]; then
  for f in backend/.sqlx/query-*.json; do
    if [ -s "${f}" ] && jq -e 'has("query") and has("hash")' "${f}" >/dev/null 2>&1; then
      sqlx_cache_ok=1
      break
    fi
  done
fi
if [ "${sqlx_cache_ok}" -eq 1 ]; then
  pass "sqlx offline cache (backend/.sqlx) present"
else
  fail "sqlx offline cache (backend/.sqlx) present" "cd backend && DATABASE_URL=<schema-owner DSN> cargo sqlx prepare -- --tests"
fi

# 8. Git state: branch, ahead/behind vs origin/main from local refs only
# (no fetch), and staleness of the local origin/main ref.
branch="$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo unknown)"
if git rev-parse --verify -q refs/remotes/origin/main >/dev/null; then
  read -r behind ahead <<<"$(git rev-list --left-right --count origin/main...HEAD 2>/dev/null || echo '? ?')"
  pass "git: on branch '${branch}', ${ahead} ahead / ${behind} behind origin/main"

  main_epoch="$(git log -1 --format=%ct refs/remotes/origin/main)"
  now_epoch="$(date +%s)"
  age_days=$(((now_epoch - main_epoch) / 86400))
  if [ "${age_days}" -gt 7 ]; then
    warn "local origin/main ref is ${age_days} days old" "git fetch origin"
  else
    pass "local origin/main ref is ${age_days} days old"
  fi
else
  warn "local origin/main ref exists" "git fetch origin"
fi

# 9. CARGO_TARGET_DIR / CARGO_BUILD_TARGET_DIR override of a worktree-local
# cargo target dir. `just worktree` writes .cargo/config.toml pinning
# `[build] target-dir` to this checkout's own target/ so concurrent worktree
# builds never thrash a shared target directory. Cargo resolves both
# CARGO_TARGET_DIR and CARGO_BUILD_TARGET_DIR (its generic
# CARGO_<SECTION>_<KEY> mapping for [build] target-dir) before it ever reads
# that config key, so either one, exported by a shell profile or inherited
# CI env, silently defeats the isolation while the worktree still looks
# isolated. Verified empirically: with both set, CARGO_TARGET_DIR is the one
# cargo actually honors, so that is the variable named when both are
# present. Only checked when the pin is actually present -- the main
# checkout was never given one, so there is no isolation to defeat there.
if [ -f .cargo/config.toml ] && grep -q 'target-dir' .cargo/config.toml; then
  if [ -n "${CARGO_TARGET_DIR:-}" ] && [ -n "${CARGO_BUILD_TARGET_DIR:-}" ]; then
    warn "CARGO_TARGET_DIR overrides this worktree's isolated cargo target dir (CARGO_BUILD_TARGET_DIR is also set but is shadowed by CARGO_TARGET_DIR)" "unset both CARGO_TARGET_DIR and CARGO_BUILD_TARGET_DIR, or set both to ${repo_root}/target"
  elif [ -n "${CARGO_TARGET_DIR:-}" ]; then
    warn "CARGO_TARGET_DIR overrides this worktree's isolated cargo target dir" "unset CARGO_TARGET_DIR, or set it to ${repo_root}/target"
  elif [ -n "${CARGO_BUILD_TARGET_DIR:-}" ]; then
    warn "CARGO_BUILD_TARGET_DIR overrides this worktree's isolated cargo target dir" "unset CARGO_BUILD_TARGET_DIR, or set it to ${repo_root}/target"
  else
    pass "CARGO_TARGET_DIR and CARGO_BUILD_TARGET_DIR do not override the worktree-local cargo target dir"
  fi
fi

# 10. Informational: list git worktrees.
info "git worktrees:"
git worktree list | while IFS= read -r line; do
  info "  ${line}"
done

# 11. Disk space on the filesystem holding the repo. tmpfs-aware: a low
# reading on a RAM-backed filesystem means building there spends memory, not
# disk, so the warning says that explicitly and points at a disk-backed
# alternative rather than reading like an ordinary low-disk warning. Reuses
# scripts/require-disk-backed.sh's own `stat -f -c %T` detection (its
# --fstype mode) rather than a second copy of that invocation.
#
# DOCTOR_STUB_DISK_AVAIL_BYTES and DOCTOR_STUB_DISK_FSTYPE exist only for
# doctor-selftest.sh: free disk space and filesystem type are host facts a
# fixture cannot control, so the selftest overrides both to reach every
# branch deterministically instead of depending on where CI or a developer's
# machine happens to run it.
five_gib=$((5 * 1024 * 1024 * 1024))
if [ -n "${DOCTOR_STUB_DISK_AVAIL_BYTES:-}" ]; then
  avail_bytes="${DOCTOR_STUB_DISK_AVAIL_BYTES}"
  disk_probe_ok=1
elif avail_bytes="$(df -B1 --output=avail "${repo_root}" 2>/dev/null | tail -n 1 | tr -d ' ')" && [ -n "${avail_bytes}" ]; then
  disk_probe_ok=1
else
  disk_probe_ok=0
fi

if [ "${disk_probe_ok}" -eq 1 ]; then
  avail_gib=$((avail_bytes / 1024 / 1024 / 1024))
  if [ "${avail_bytes}" -ge "${five_gib}" ]; then
    pass "disk space: ${avail_gib} GiB free"
  else
    if [ -n "${DOCTOR_STUB_DISK_FSTYPE+set}" ]; then
      fstype="${DOCTOR_STUB_DISK_FSTYPE}"
    else
      fstype="$("${repo_root}/scripts/require-disk-backed.sh" --fstype "${repo_root}" 2>/dev/null || true)"
    fi
    case "${fstype}" in
      tmpfs | ramfs)
        warn "disk space: only ${avail_gib} GiB free on ${fstype} (RAM-backed)" "building here spends memory, not disk; use a disk-backed path (see 'just worktree', WORKTREE_ROOT)"
        ;;
      *)
        warn "disk space: only ${avail_gib} GiB free" "free up space on the filesystem holding ${repo_root}"
        ;;
    esac
  fi
else
  warn "disk space check" "cannot determine free space (non-GNU df); check manually"
fi

# 12. kache build-cache binary resolves on PATH. It is configured as the
# cargo rustc-wrapper outside this repo; absence only means local builds
# fall back to uncached compiles, not a broken toolchain, so this warns
# rather than fails.
kache_present=0
if command -v kache >/dev/null 2>&1; then
  kache_present=1
  pass "binary 'kache' resolves on PATH"
else
  warn "binary 'kache' resolves on PATH" "mise install"
fi

# 13. kache daemon reachable. The daemon owns the store's only automatic
# eviction (one sweep on startup, then every six hours), so a stopped daemon
# is the usual reason the size check below eventually fires. Local cache hits
# and misses work without it, which is why this warns rather than fails.
#
# `kache daemon status` exits 0 whether or not the daemon is up, so the state
# has to be read out of its output rather than its exit code. Two properties
# shape the match: the state is wrapped in ANSI colour that no NO_COLOR
# setting suppresses, and "not running" contains "running". Hence globs that
# tolerate the escape sequences, with the negative case tested first.
# Anything neither pattern recognises is reported as indeterminate: an
# upstream change to this output must surface as a warning, never keep
# forging a PASS.
if [ "${kache_present}" -eq 1 ]; then
  daemon_line="$(kache daemon status 2>/dev/null | grep 'Daemon:' | head -n 1 || true)"
  case "${daemon_line}" in
    *not*running*)
      warn "kache daemon is running" "kache daemon start"
      ;;
    *running*)
      pass "kache daemon is running"
      ;;
    *)
      warn "kache daemon is running" "cannot determine daemon state (unrecognized 'kache daemon status' output); run it manually"
      ;;
  esac
fi

# 14. kache content-addressed store size, checked against the cap kache
# itself enforces rather than against free disk. Eviction only happens while
# the daemon runs (check 13), and it holds the store under
# `cache.local_max_size`, so a store found above that ceiling means eviction
# is not happening: a dead daemon, a failing sweep, or a machine whose
# configured cap no longer matches this threshold. Sizing the check to the cap
# rather than to a smaller number of its own keeps the two from disagreeing,
# which is the state that made an earlier 20 GiB threshold fire permanently
# against a 50 GiB cap on a disk with hundreds of gigabytes free.
#
# A store that has never been populated (kache has
# never run here, or the resolved directory does not exist on this
# platform) is not a problem to report on, so an absent directory degrades
# silently rather than warning or failing.
#
# The remedy has to be an age sweep, not a size one. `kache gc` with no age
# evicts only down to the configured cap, which defaults far above this
# threshold, and KACHE_MAX_SIZE in the invoking environment does not reach
# that sweep: measured against a 31 GiB store, a 29 GiB cap evicted nothing
# while an age sweep over the same store reclaimed 6.7 GiB. The age also has
# to be short enough to match something. Entries turn over in days here, not
# weeks, so the 30d this once advised could evict nothing on a store that had
# just tripped the threshold.
#
# Directory resolution follows kache's own documented precedence (kache's
# configuration reference, v0.11.0): the KACHE_CACHE_DIR environment
# variable overrides everywhere it is set, and otherwise the platform
# default applies: ~/Library/Caches/kache on macOS, ~/.cache/kache
# everywhere else. kache's docs describe XDG_CACHE_HOME as affecting only
# where kache looks for its *config* file, never the cache directory
# itself, so it is not treated as authoritative here and never substitutes
# for the documented default when that default exists. It is still probed
# as a last-resort fallback, after both documented sources, only when the
# platform default directory is absent: Rust's common cache-dir resolution
# libraries honor $XDG_CACHE_HOME as the Linux cache root when it is set,
# so a machine that relies on that convention is still found rather than
# silently going unreported. Probing this extra candidate can only add a
# location to check; it never suppresses the documented default when that
# one is actually present.
platform="$(uname -s 2>/dev/null || echo unknown)"
if [ -n "${KACHE_CACHE_DIR:-}" ]; then
  kache_store="${KACHE_CACHE_DIR}"
elif [ "${platform}" = "Darwin" ]; then
  kache_store="${HOME}/Library/Caches/kache"
else
  kache_store="${HOME}/.cache/kache"
  if [ ! -d "${kache_store}" ] && [ -n "${XDG_CACHE_HOME:-}" ]; then
    kache_store="${XDG_CACHE_HOME}/kache"
  fi
fi

# `du -sk` (1024-byte blocks) rather than GNU-only `du -sb` (bytes, via
# `--apparent-size`): `-b` has no BSD/macOS equivalent, so it either fails
# or reports wildly different units on a non-GNU userland. `-sk` is
# supported by both and, as a real-disk-usage measurement rather than an
# apparent-size one, is the more honest number for a disk-space check.
store_cap_kib=$((50 * 1024 * 1024))
if [ -d "${kache_store}" ]; then
  if store_kib="$(du -sk "${kache_store}" 2>/dev/null | cut -f1)" && [ -n "${store_kib}" ]; then
    # Round to the nearest GiB rather than truncating: truncation alone
    # would display a 20.9 GiB store as "20 GiB", reading as though the
    # warning fired under its own stated threshold.
    store_gib=$(( (store_kib + 524288) / 1048576 ))
    if [ "${store_kib}" -ge "${store_cap_kib}" ]; then
      warn "kache store size: ${store_gib} GiB" "kache gc --max-age 7d"
    else
      pass "kache store size: ${store_gib} GiB"
    fi
  else
    warn "kache store size" "cannot determine store size (du failed); check manually"
  fi
fi

echo "----"
echo "doctor: ${pass_count} pass, ${warn_count} warn, ${fail_count} fail"
if [ "${fail_count}" -gt 0 ]; then
  exit 1
fi
exit 0
