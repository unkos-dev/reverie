#!/usr/bin/env bash
# Self-test scripts/doctor.sh against an isolated fixture repo and a stubbed
# PATH, so the assertions never touch the real checkout's toolchain, docker
# daemon, or dev database state.
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
doctor="${repo_root}/scripts/doctor.sh"

tmp="$(mktemp -d)"
trap 'rm -rf "${tmp}"' EXIT

fixture="${tmp}/fixture"
mkdir -p "${fixture}/scripts" "${fixture}/backend/.sqlx"
cp "${doctor}" "${fixture}/scripts/doctor.sh"
chmod +x "${fixture}/scripts/doctor.sh"

# A minimal, real git repo so git-derived checks (branch, ahead/behind,
# origin/main staleness) exercise real git plumbing rather than a stub.
git -C "${fixture}" init -q -b main
git -C "${fixture}" config user.name "Doctor Selftest"
git -C "${fixture}" config user.email "selftest@example.invalid"
git -C "${fixture}" config commit.gpgsign false
echo x >"${fixture}/README.md"
git -C "${fixture}" add README.md
git -C "${fixture}" commit -q -m "chore: fixture root"
git -C "${fixture}" update-ref refs/remotes/origin/main HEAD

# A root node_modules (the ADR-mandated hoist point) with a lockfile that
# does not outrun the install marker. Writing the lockfile before the
# marker (rather than backdating it with a GNU-only `touch -d`) is
# portable: on any real filesystem the second write's mtime is never
# earlier than the first's, and a same-second tie still reads as "not
# stale" since the check only fires on strictly-newer.
mkdir -p "${fixture}/node_modules"
echo '{}' >"${fixture}/package-lock.json"
echo '{}' >"${fixture}/node_modules/.package-lock.json"

# An executable node_modules/.bin/vp, the actual precondition the staleness
# advice checks for (an install that predates vite-plus entering the
# lockfile can have the marker and no vp binary at all).
install_vp_stub() {
  mkdir -p "${fixture}/node_modules/.bin"
  printf '#!/usr/bin/env bash\nexit 0\n' >"${fixture}/node_modules/.bin/vp"
  chmod +x "${fixture}/node_modules/.bin/vp"
}
install_vp_stub

# Non-empty sqlx offline cache with one entry shaped like a real sqlx
# query-*.json (query + hash keys), not just any parseable JSON.
echo '{"query": "SELECT 1", "hash": "deadbeef"}' >"${fixture}/backend/.sqlx/query-fixture.json"

# Stub PATH: real coreutils/git/jq passed through by absolute-path symlink,
# the remaining required binaries stubbed as no-op executables (doctor.sh
# only checks their resolvability, never runs them), and docker/mise
# stubbed with controllable behavior via environment variables so every
# branch of the container-health, app-login, and mise-pin checks is
# reachable without a real daemon.
link_real() { # <dir> <name>
  local dir="$1" name="$2"
  ln -s "$(command -v "${name}")" "${dir}/${name}"
}

noop_stub() { # <dir> <name>
  local dir="$1" name="$2"
  printf '#!/usr/bin/env bash\nexit 0\n' >"${dir}/${name}"
  chmod +x "${dir}/${name}"
}

stub_bin="${tmp}/bin"
mkdir -p "${stub_bin}"
for real in env bash git jq ls df date dirname cat tail tr grep; do
  link_real "${stub_bin}" "${real}"
done
for tool in just cargo rustc node npm npx; do
  noop_stub "${stub_bin}" "${tool}"
done

cat >"${stub_bin}/mise" <<'MISE_STUB'
#!/usr/bin/env bash
# Fixture stub: asserts the exact invocation doctor.sh makes (ls --current
# --missing -J) and exits 2 on anything else, so a future argument
# regression in doctor.sh fails this selftest instead of the stub silently
# accepting whatever it was called with. DOCTOR_STUB_MISE_ERROR simulates
# the query itself failing (a renamed flag, a crashed mise, ...),
# independent of DOCTOR_STUB_MISE_MISSING, so the fail-closed path is
# reachable without needing a real broken mise.
set -euo pipefail
if [ "$#" -eq 4 ] && [ "$1" = "ls" ] && [ "$2" = "--current" ] && [ "$3" = "--missing" ] && [ "$4" = "-J" ]; then
  if [ -n "${DOCTOR_STUB_MISE_ERROR:-}" ]; then
    exit 2
  fi
  if [ -z "${DOCTOR_STUB_MISE_MISSING:-}" ]; then
    echo '{}'
  else
    jq -n --arg names "${DOCTOR_STUB_MISE_MISSING}" \
      '$names | split(" ") | map({(.): true}) | add'
  fi
  exit 0
fi
exit 2
MISE_STUB
chmod +x "${stub_bin}/mise"

cat >"${stub_bin}/docker" <<'DOCKER_STUB'
#!/usr/bin/env bash
# Fixture stub: only supports the four invocations doctor.sh makes (info,
# inspect --format ..., exec ... pg_isready ..., exec ... psql ...), all
# controlled by DOCTOR_STUB_* environment variables so every branch is
# independently reachable without a real daemon or container.
set -euo pipefail
case "$1" in
  info)
    [ "${DOCTOR_STUB_DOCKER_UP:-1}" = "1" ]
    ;;
  inspect)
    fmt="$3"
    if [ -z "${DOCTOR_STUB_CONTAINER_STATUS:-}" ]; then
      exit 1
    fi
    case "${fmt}" in
      *Health*) printf '%s' "${DOCTOR_STUB_CONTAINER_HEALTH:-none}" ;;
      *) printf '%s' "${DOCTOR_STUB_CONTAINER_STATUS}" ;;
    esac
    ;;
  exec)
    case "$*" in
      *psql*) [ "${DOCTOR_STUB_APP_LOGIN:-1}" = "1" ] ;;
      *pg_isready*) [ "${DOCTOR_STUB_PG_READY:-1}" = "1" ] ;;
      *) exit 1 ;;
    esac
    ;;
  *)
    exit 1
    ;;
esac
DOCKER_STUB
chmod +x "${stub_bin}/docker"

fail=0

run_doctor() { # runs the fixture doctor.sh with the given PATH, capturing output+exit code
  local test_path="$1"
  output=""
  rc=0
  output="$(PATH="${test_path}" "${fixture}/scripts/doctor.sh" 2>&1)" || rc=$?
}

expect_exit() { # <name> <want-exit> <path>
  local name="$1" want="$2" test_path="$3"
  run_doctor "${test_path}"
  if [ "${rc}" -ne "${want}" ]; then
    echo "FAIL ${name}: expected exit ${want}, got ${rc}"
    echo "${output}"
    fail=1
  else
    echo "ok   ${name}"
  fi
}

expect_contains() { # <name> <needle>
  local name="$1" needle="$2"
  if printf '%s' "${output}" | grep -qF "${needle}"; then
    echo "ok   ${name}"
  else
    echo "FAIL ${name}: expected output to contain '${needle}'"
    echo "${output}"
    fail=1
  fi
}

expect_not_contains() { # <name> <needle>
  local name="$1" needle="$2"
  if printf '%s' "${output}" | grep -qF "${needle}"; then
    echo "FAIL ${name}: expected output to NOT contain '${needle}'"
    echo "${output}"
    fail=1
  else
    echo "ok   ${name}"
  fi
}

# --- fully-stubbed happy path: every check passes, exit 0 ---
export DOCTOR_STUB_DOCKER_UP=1
export DOCTOR_STUB_CONTAINER_STATUS=running
export DOCTOR_STUB_CONTAINER_HEALTH=healthy
export DOCTOR_STUB_PG_READY=1
export DOCTOR_STUB_MISE_MISSING=""
expect_exit "happy path exits zero" 0 "${stub_bin}"
expect_not_contains "happy path has no FAIL lines" "FAIL "
expect_not_contains "happy path has no WARN lines" "WARN "

# --- WARN-only run: dev DB absent, mise pin missing -> still exit 0 ---
export DOCTOR_STUB_CONTAINER_STATUS=""
export DOCTOR_STUB_CONTAINER_HEALTH=""
expect_exit "warn-only run exits zero" 0 "${stub_bin}"
expect_contains "warn-only run reports the absent dev DB" "WARN dev Postgres container"
expect_not_contains "warn-only run has no FAIL lines" "FAIL "
# restore for later assertions
export DOCTOR_STUB_CONTAINER_STATUS=running
export DOCTOR_STUB_CONTAINER_HEALTH=healthy

# --- a real missing mise pin is reported by name and fails the run ---
export DOCTOR_STUB_MISE_MISSING="cargo-nextest"
expect_exit "missing mise pin fails the run" 1 "${stub_bin}"
expect_contains "missing mise pin is named in the output" "cargo-nextest"
export DOCTOR_STUB_MISE_MISSING=""

# --- the mise query itself erroring must fail closed, not read as "zero
# missing": this is the regression test for the check silently forging a
# PASS if a future mise renames --missing or -J. ---
export DOCTOR_STUB_MISE_ERROR=1
expect_exit "mise query failure fails closed" 1 "${stub_bin}"
expect_contains "mise query failure is reported, not silently passed" "FAIL mise-pinned tools are installed (mise query failed)"
unset DOCTOR_STUB_MISE_ERROR

# --- pg_isready succeeding is not enough: a broken/missing reverie_app
# role must independently fail the run when no override signal explains it. ---
export DOCTOR_STUB_APP_LOGIN=0
expect_exit "broken app-role login fails the run" 1 "${stub_bin}"
expect_contains "app-role login failure is reported" "FAIL reverie_app role authenticates"
export DOCTOR_STUB_APP_LOGIN=1
expect_exit "healthy app-role login passes" 0 "${stub_bin}"
expect_contains "app-role login success is reported" "PASS reverie_app role authenticates"

# --- a login failure with an override signal present (REVERIE_APP_PASSWORD
# set, the env var docker/init-roles.sql reads for a non-default password)
# degrades to WARN instead of FAIL: the default-credential probe cannot
# tell a broken role from an intentionally customized one. ---
export DOCTOR_STUB_APP_LOGIN=0
export REVERIE_APP_PASSWORD=custom-secret
expect_exit "custom REVERIE_APP_PASSWORD degrades login failure to warn" 0 "${stub_bin}"
expect_contains "custom-password warn is reported" "WARN reverie_app role authenticates"
expect_not_contains "custom-password path has no FAIL lines" "FAIL "
unset REVERIE_APP_PASSWORD

# --- the same degrade-to-WARN applies for the other documented override
# signal: a docker/.env file, the dotenv file this compose project loads. ---
mkdir -p "${fixture}/docker"
: >"${fixture}/docker/.env"
expect_exit "docker/.env presence degrades login failure to warn" 0 "${stub_bin}"
expect_contains "docker/.env warn is reported" "WARN reverie_app role authenticates"
rm -rf "${fixture}/docker"
export DOCTOR_STUB_APP_LOGIN=1

# --- sqlx cache: a zero-byte leftover file must not satisfy "non-empty
# directory", and a parseable-but-unrelated JSON file must not satisfy
# "looks like a query cache entry"; only a query+hash-shaped entry does. ---
rm -f "${fixture}/backend/.sqlx/query-fixture.json"
: >"${fixture}/backend/.sqlx/query-truncated.json"
expect_exit "zero-byte-only sqlx cache fails the run" 1 "${stub_bin}"
expect_contains "zero-byte sqlx cache failure is reported" "FAIL sqlx offline cache"
rm -f "${fixture}/backend/.sqlx/query-truncated.json"

echo '{"unrelated": true}' >"${fixture}/backend/.sqlx/query-wrongshape.json"
expect_exit "parseable but wrong-shape JSON fails the run" 1 "${stub_bin}"
expect_contains "wrong-shape sqlx cache failure is reported" "FAIL sqlx offline cache"
rm -f "${fixture}/backend/.sqlx/query-wrongshape.json"

echo '{"query": "SELECT 1", "hash": "deadbeef"}' >"${fixture}/backend/.sqlx/query-fixture.json"
expect_exit "a valid sqlx cache entry passes" 0 "${stub_bin}"
expect_contains "valid sqlx cache is reported" "PASS sqlx offline cache"

# --- node_modules advice: each branch's fix must actually be runnable from
# that branch's starting state, not just present as text. ---

# (a) node_modules absent entirely: npx --no-install has no local binary to
# fall back to, so the advice must be npm install, the one command that
# bootstraps from nothing.
rm -rf "${fixture}/node_modules"
expect_exit "absent node_modules warns" 0 "${stub_bin}"
expect_contains "absent node_modules advises npm install" "WARN root node_modules present -- fix: npm install"
mkdir -p "${fixture}/node_modules"
install_vp_stub
echo '{}' >"${fixture}/package-lock.json"
echo '{}' >"${fixture}/node_modules/.package-lock.json"

# (b) node_modules present but the install marker missing: an incomplete or
# interrupted install, where node_modules/.bin/vp may itself be missing, so
# this must also advise npm install rather than npx --no-install.
rm -f "${fixture}/node_modules/.package-lock.json"
expect_exit "missing install marker warns" 0 "${stub_bin}"
expect_contains "missing install marker advises npm install" "WARN node_modules matches package-lock.json -- fix: npm install"
echo '{}' >"${fixture}/node_modules/.package-lock.json"

# (c) both files present but the lockfile is strictly newer than the
# marker: a genuine staleness. With node_modules/.bin/vp present (a
# completed install), npx --no-install can repair it. Writing the marker
# before the lockfile is the same portable ordering trick used to set up
# the fixture originally, run in the other direction; unlike the tie-safe
# "not stale" setup, this assertion needs a real, not just a same-second,
# gap, so a one-second sleep (portable, unlike a GNU-only `touch -d`
# backdate) sits between the two writes.
echo '{}' >"${fixture}/node_modules/.package-lock.json"
sleep 1
echo '{}' >"${fixture}/package-lock.json"
expect_exit "stale lockfile warns" 0 "${stub_bin}"
expect_contains "stale lockfile advises npx --no-install vp install" "WARN node_modules matches package-lock.json -- fix: npx --no-install vp install"

# (d) the same staleness, but node_modules/.bin/vp does not exist: an
# install that predates vite-plus entering the lockfile, so the marker is
# present and the lockfile is newer with no vp binary ever having been
# installed. npx --no-install has nothing to fall back to here either, so
# this must also advise npm install. This is the regression test for the
# advice being keyed on the actual binary rather than inferred from
# marker files.
rm -f "${fixture}/node_modules/.bin/vp"
expect_exit "stale lockfile with no vp binary warns" 0 "${stub_bin}"
expect_contains "stale lockfile with no vp binary advises npm install" "WARN node_modules matches package-lock.json -- fix: npm install"
install_vp_stub

# restore the happy-path ordering (lockfile written before the marker) for
# any assertions that follow.
echo '{}' >"${fixture}/package-lock.json"
echo '{}' >"${fixture}/node_modules/.package-lock.json"

# --- missing-binary detection: PATH with one required binary removed ---
stub_bin_missing="${tmp}/bin-missing"
mkdir -p "${stub_bin_missing}"
for f in "${stub_bin}"/*; do
  name="$(basename "${f}")"
  [ "${name}" = "cargo" ] && continue
  cp -P "${f}" "${stub_bin_missing}/${name}"
done
expect_exit "missing binary fails closed" 1 "${stub_bin_missing}"
expect_contains "missing binary is named in the output" "FAIL binary 'cargo' resolves on PATH"

# --- CARGO_TARGET_DIR / CARGO_BUILD_TARGET_DIR override check: only
# reachable once the fixture has the worktree-local pin doctor.sh looks
# for. Cargo honors both as environment overrides for [build] target-dir
# (CARGO_BUILD_TARGET_DIR via cargo's generic CARGO_<SECTION>_<KEY>
# mapping), confirmed empirically with CARGO_TARGET_DIR winning when both
# are set, so both are exercised individually and together below. ---

# (a) no .cargo/config.toml at all (an ordinary, non-worktree checkout): the
# check must not fire either way, since there is no isolation to defeat.
export DOCTOR_STUB_MISE_MISSING=""
expect_exit "no cargo pin: happy path still exits zero" 0 "${stub_bin}"
expect_not_contains "no cargo pin: check does not fire" "cargo target dir"

# (b) the worktree-local pin is present and neither variable is set: passes.
mkdir -p "${fixture}/.cargo"
printf '%s\n' '[build]' 'target-dir = "target"' >"${fixture}/.cargo/config.toml"
expect_exit "worktree pin with no override passes" 0 "${stub_bin}"
expect_contains "worktree pin with no override is reported" "PASS CARGO_TARGET_DIR and CARGO_BUILD_TARGET_DIR do not override the worktree-local cargo target dir"

# (c) the worktree-local pin is present and CARGO_TARGET_DIR is exported:
# this is the regression test for the original review finding -- an
# environment override silently defeating the isolation must be surfaced,
# not silent.
export CARGO_TARGET_DIR="/some/shared/target"
expect_exit "worktree pin with a CARGO_TARGET_DIR override warns, not fails" 0 "${stub_bin}"
expect_contains "override warning names the offending variable" "WARN CARGO_TARGET_DIR overrides this worktree's isolated cargo target dir"
expect_contains "override warning names the fix" "unset CARGO_TARGET_DIR, or set it to"
unset CARGO_TARGET_DIR

# (d) the worktree-local pin is present and only CARGO_BUILD_TARGET_DIR is
# exported: this is the regression test for the second override variable --
# cargo's generic CARGO_<SECTION>_<KEY> mapping also defeats the isolation
# and must be surfaced under its own name, not silently treated as healthy.
export CARGO_BUILD_TARGET_DIR="/some/shared/target"
expect_exit "worktree pin with a CARGO_BUILD_TARGET_DIR override warns, not fails" 0 "${stub_bin}"
expect_contains "CARGO_BUILD_TARGET_DIR override warning names the offending variable" "WARN CARGO_BUILD_TARGET_DIR overrides this worktree's isolated cargo target dir"
expect_contains "CARGO_BUILD_TARGET_DIR override warning names the fix" "unset CARGO_BUILD_TARGET_DIR, or set it to"
unset CARGO_BUILD_TARGET_DIR

# (e) both variables are exported: the warning must name the one cargo
# actually honors (CARGO_TARGET_DIR, per the measured precedence) rather
# than either an arbitrary choice or both interchangeably.
export CARGO_TARGET_DIR="/some/shared/target"
export CARGO_BUILD_TARGET_DIR="/some/other/shared/target"
expect_exit "worktree pin with both overrides set warns, not fails" 0 "${stub_bin}"
expect_contains "both-set warning names CARGO_TARGET_DIR as active" "WARN CARGO_TARGET_DIR overrides this worktree's isolated cargo target dir (CARGO_BUILD_TARGET_DIR is also set but is shadowed by CARGO_TARGET_DIR)"
expect_contains "both-set warning's fix covers both variables" "unset both CARGO_TARGET_DIR and CARGO_BUILD_TARGET_DIR, or set both to"
unset CARGO_TARGET_DIR
unset CARGO_BUILD_TARGET_DIR
rm -rf "${fixture}/.cargo"

exit "${fail}"
