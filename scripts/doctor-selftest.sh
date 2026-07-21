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

# Non-empty sqlx offline cache with one entry that actually parses as JSON.
echo '{}' >"${fixture}/backend/.sqlx/query-fixture.json"

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
for real in env bash git jq ls df date dirname cat tail tr; do
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
# role must independently fail the run. ---
export DOCTOR_STUB_APP_LOGIN=0
expect_exit "broken app-role login fails the run" 1 "${stub_bin}"
expect_contains "app-role login failure is reported" "FAIL reverie_app role authenticates"
export DOCTOR_STUB_APP_LOGIN=1
expect_exit "healthy app-role login passes" 0 "${stub_bin}"
expect_contains "app-role login success is reported" "PASS reverie_app role authenticates"

# --- sqlx cache: a zero-byte leftover file must not satisfy "non-empty
# directory"; only an entry that actually parses as JSON does. ---
rm -f "${fixture}/backend/.sqlx/query-fixture.json"
: >"${fixture}/backend/.sqlx/query-truncated.json"
expect_exit "zero-byte-only sqlx cache fails the run" 1 "${stub_bin}"
expect_contains "zero-byte sqlx cache failure is reported" "FAIL sqlx offline cache"
echo '{}' >"${fixture}/backend/.sqlx/query-fixture.json"
rm -f "${fixture}/backend/.sqlx/query-truncated.json"
expect_exit "a valid sqlx cache entry passes" 0 "${stub_bin}"
expect_contains "valid sqlx cache is reported" "PASS sqlx offline cache"

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

exit "${fail}"
