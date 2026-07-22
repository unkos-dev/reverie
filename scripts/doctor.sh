#!/usr/bin/env bash
# Fast, read-only environment self-check: "is this machine ready to develop
# Reverie?" Prints one PASS/WARN/FAIL line per check plus a summary, and
# exits nonzero iff any check FAILed. No writes, no network calls beyond the
# already-running local docker daemon this repo's dev stack owns.
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

# 1. Required binaries resolve on PATH. jq is included because check 2 and
# check 7 below both shell out to it; without this, a missing jq shows up
# only as a confusing "(missing: unknown)" deep in another check's output.
for bin in git just docker cargo rustc node npm npx mise jq; do
  if command -v "$bin" >/dev/null 2>&1; then
    pass "binary '${bin}' resolves on PATH"
  else
    fail "binary '${bin}' resolves on PATH" "install ${bin}, then run 'mise install'"
  fi
done

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

# 6. node_modules present at the workspace root, and not stale against the
# committed lockfile. npm workspaces hoist frontend and docs dependencies
# into the one root node_modules (adr/2026-06-30-adopt-vite-plus-monorepo-toolchain.md),
# so a healthy checkout has no frontend/node_modules or docs/node_modules of
# its own to check.
if [ -d node_modules ]; then
  pass "root node_modules present"
else
  # npx --no-install cannot bootstrap an empty tree: with no node_modules at
  # all there is no local vp binary for it to fall back to, so it refuses to
  # fetch one and errors out. npm install is the one command that works from
  # nothing; it also gets vp into node_modules/.bin for the staleness checks
  # below to use afterward.
  warn "root node_modules present" "npm install"
fi

if [ -f package-lock.json ] && [ -f node_modules/.package-lock.json ]; then
  if [ package-lock.json -nt node_modules/.package-lock.json ]; then
    # Advice is keyed on whether node_modules/.bin/vp actually exists, not
    # inferred from marker files: an install that predates vite-plus
    # entering the lockfile can leave the marker present and the lockfile
    # newer with no vp binary ever having been installed, and npx
    # --no-install has nothing to fall back to in that case either.
    if [ -x node_modules/.bin/vp ]; then
      warn "node_modules matches package-lock.json" "npx --no-install vp install"
    else
      warn "node_modules matches package-lock.json" "npm install"
    fi
  else
    pass "node_modules matches package-lock.json"
  fi
else
  # The install marker (node_modules/.package-lock.json) is missing even
  # though node_modules exists: an incomplete or interrupted install, where
  # node_modules/.bin/vp may itself be missing. npx --no-install has
  # nothing to fall back to there, so advise the same npm install bootstrap
  # as the absent-tree branch above rather than a command that may not run.
  warn "node_modules matches package-lock.json" "npm install"
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

# 11. Disk space on the filesystem holding the repo.
five_gib=$((5 * 1024 * 1024 * 1024))
if avail_bytes="$(df -B1 --output=avail "${repo_root}" 2>/dev/null | tail -n 1 | tr -d ' ')" && [ -n "${avail_bytes}" ]; then
  avail_gib=$((avail_bytes / 1024 / 1024 / 1024))
  if [ "${avail_bytes}" -ge "${five_gib}" ]; then
    pass "disk space: ${avail_gib} GiB free"
  else
    warn "disk space: only ${avail_gib} GiB free" "free up space on the filesystem holding ${repo_root}"
  fi
else
  warn "disk space check" "cannot determine free space (non-GNU df); check manually"
fi

echo "----"
echo "doctor: ${pass_count} pass, ${warn_count} warn, ${fail_count} fail"
if [ "${fail_count}" -gt 0 ]; then
  exit 1
fi
exit 0
