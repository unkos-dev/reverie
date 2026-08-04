#!/usr/bin/env bash
# Self-test `just worktree`'s cargo isolation, operator-owned overlays, and
# dependency install. The generated-reference drift check only reads recipe
# metadata and never executes the recipe. Every case runs against the npm stub
# installed below, so no fixture ever installs for real. Most cases invoke the
# checkout's real recipe directly. Codex
# policy cases mirror the current justfile into a disposable source worktree so
# uncommitted recipe changes are exercised before their first commit; if the
# recipe delegates behavior to another file, that fixture must mirror it too.
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"

# A sibling of the checkout, not $TMPDIR: the recipe itself refuses to
# create a worktree on a tmpfs, and a sandboxed dev environment's default
# temp dir often is one, while the checkout's own filesystem never is. This
# mirrors the recipe's own default WORKTREE_ROOT (parent_directory of the
# repo), so this selftest passes everywhere the recipe itself would work.
scratch_root="$(mktemp -d "$(dirname "${repo_root}")/.worktree-selftest.XXXXXX")"
export CODEX_HOME="${scratch_root}/codex-home"
mkdir -p "${CODEX_HOME}"

# --- npm stub, in force for every fixture below. The recipe's dependency
# install is its only network step, and the CI copy of this lane cannot run a
# real one at all: repo-lint provisions npm through setup-vp rather than mise,
# so its npm hard-errors with EBADDEVENGINES against the pin package.json
# declares. The stub records the working directory and arguments instead, which
# is what the assertions actually need; a real install is proven by the manual
# acceptance test. WORKTREE_SELFTEST_NPM_EXIT drives the failure case. ---
stub_bin="${scratch_root}/stub-bin"
mkdir -p "${stub_bin}"
export WORKTREE_SELFTEST_NPM_LOG="${scratch_root}/npm-invocations"
export WORKTREE_SELFTEST_MISE_LOG="${scratch_root}/mise-exec-tools"
: >"${WORKTREE_SELFTEST_NPM_LOG}"
: >"${WORKTREE_SELFTEST_MISE_LOG}"

cat >"${stub_bin}/npm" <<'STUB'
#!/usr/bin/env bash
printf '%s\t%s\n' "${PWD}" "$*" >>"${WORKTREE_SELFTEST_NPM_LOG}"
exit "${WORKTREE_SELFTEST_NPM_EXIT:-0}"
STUB
chmod +x "${stub_bin}/npm"

# `mise exec` resolves its own npm and would run straight past the stub above,
# so mise is shadowed too. Only `exec` is intercepted: it records the tool spec
# and then runs the command after `--`, which lands on the npm stub. Every
# other subcommand delegates to the real mise, so the trust the recipe inherits
# a few lines earlier is still granted for real.
real_mise="$(command -v mise || true)"
export WORKTREE_SELFTEST_REAL_MISE="${real_mise}"
if [ -n "${real_mise}" ]; then
  cat >"${stub_bin}/mise" <<'STUB'
#!/usr/bin/env bash
if [ "${1:-}" = "exec" ]; then
  shift
  printf '%s\n' "${1:-}" >>"${WORKTREE_SELFTEST_MISE_LOG}"
  shift
  [ "${1:-}" = "--" ] && shift
  exec "$@"
fi
exec "${WORKTREE_SELFTEST_REAL_MISE}" "$@"
STUB
  chmod +x "${stub_bin}/mise"
fi
export PATH="${stub_bin}:${PATH}"

# The pin the recipe must resolve out of a destination created from HEAD.
head_npm_pin="$(jq -r '.devEngines.packageManager.version // empty' "${repo_root}/package.json")"

pids="$$"
branch_ok="test/worktree-selftest-ok-${pids}"
branch_dirty="test/worktree-selftest-dirty-${pids}"
branch_env_target="test/worktree-selftest-envtarget-${pids}"
branch_env_build="test/worktree-selftest-envbuild-${pids}"
branch_env_both="test/worktree-selftest-envboth-${pids}"
branch_env_sanitize="test/worktree-selftest-envsanitize-${pids}"
branch_overlay="test/worktree-selftest-overlay-${pids}"
branch_install_fail="test/worktree-selftest-installfail-${pids}"
branch_npm_pin="test/worktree-selftest-npmpin-${pids}"
branch_codex_source="test/worktree-codex-policy-source-${pids}"
branch_codex_missing="test/worktree-codex-policy-missing-${pids}"
branch_codex_overlay="test/worktree-codex-policy-overlay-${pids}"
branch_codex_untrusted="test/worktree-codex-policy-untrusted-${pids}"
branch_codex_dest_untrusted="test/worktree-codex-policy-dest-untrusted-${pids}"
slug_ok="${branch_ok//\//-}"
slug_dirty="${branch_dirty//\//-}"
slug_env_target="${branch_env_target//\//-}"
slug_env_build="${branch_env_build//\//-}"
slug_env_both="${branch_env_both//\//-}"
slug_env_sanitize="${branch_env_sanitize//\//-}"
slug_overlay="${branch_overlay//\//-}"
slug_install_fail="${branch_install_fail//\//-}"
slug_npm_pin="${branch_npm_pin//\//-}"
slug_codex_source="${branch_codex_source//\//-}"
slug_codex_missing="${branch_codex_missing//\//-}"
slug_codex_overlay="${branch_codex_overlay//\//-}"
slug_codex_untrusted="${branch_codex_untrusted//\//-}"
slug_codex_dest_untrusted="${branch_codex_dest_untrusted//\//-}"
dest_ok="${scratch_root}/reverie/${slug_ok}"
dest_dirty="${scratch_root}/reverie/${slug_dirty}"
dest_env_target="${scratch_root}/reverie/${slug_env_target}"
dest_env_build="${scratch_root}/reverie/${slug_env_build}"
dest_env_both="${scratch_root}/reverie/${slug_env_both}"
dest_env_sanitize="${scratch_root}/reverie/${slug_env_sanitize}"
dest_overlay="${scratch_root}/reverie/${slug_overlay}"
dest_install_fail="${scratch_root}/reverie/${slug_install_fail}"
dest_npm_pin="${scratch_root}/reverie/${slug_npm_pin}"
dest_codex_source="${scratch_root}/reverie/${slug_codex_source}"
dest_codex_missing="${scratch_root}/reverie/${slug_codex_missing}"
dest_codex_overlay="${scratch_root}/reverie/${slug_codex_overlay}"
dest_codex_untrusted="${scratch_root}/reverie/${slug_codex_untrusted}"
dest_codex_dest_untrusted="${scratch_root}/reverie/${slug_codex_dest_untrusted}"

# Sentinel-overlay bookkeeping: the sentinel section only ever creates
# these when the checkout has no real overlay, and cleanup must remove
# exactly what this run created, never a developer's actual settings.
sentinel_overlay_created=0
sentinel_overlay_dir_created=0

# shellcheck disable=SC2329  # invoked via the EXIT trap
cleanup() {
  git -C "${repo_root}" worktree remove --force "${dest_ok}" >/dev/null 2>&1 || true
  git -C "${repo_root}" worktree remove --force "${dest_dirty}" >/dev/null 2>&1 || true
  git -C "${repo_root}" worktree remove --force "${dest_env_target}" >/dev/null 2>&1 || true
  git -C "${repo_root}" worktree remove --force "${dest_env_build}" >/dev/null 2>&1 || true
  git -C "${repo_root}" worktree remove --force "${dest_env_both}" >/dev/null 2>&1 || true
  git -C "${repo_root}" worktree remove --force "${dest_env_sanitize}" >/dev/null 2>&1 || true
  git -C "${repo_root}" worktree remove --force "${dest_overlay}" >/dev/null 2>&1 || true
  git -C "${repo_root}" worktree remove --force "${dest_install_fail}" >/dev/null 2>&1 || true
  git -C "${repo_root}" worktree remove --force "${dest_npm_pin}" >/dev/null 2>&1 || true
  git -C "${repo_root}" worktree remove --force "${dest_codex_missing}" >/dev/null 2>&1 || true
  git -C "${repo_root}" worktree remove --force "${dest_codex_overlay}" >/dev/null 2>&1 || true
  git -C "${repo_root}" worktree remove --force "${dest_codex_untrusted}" >/dev/null 2>&1 || true
  git -C "${repo_root}" worktree remove --force "${dest_codex_dest_untrusted}" >/dev/null 2>&1 || true
  git -C "${repo_root}" worktree remove --force "${dest_codex_source}" >/dev/null 2>&1 || true
  git -C "${repo_root}" worktree prune >/dev/null 2>&1 || true
  git -C "${repo_root}" branch -D "${branch_ok}" >/dev/null 2>&1 || true
  git -C "${repo_root}" branch -D "${branch_dirty}" >/dev/null 2>&1 || true
  git -C "${repo_root}" branch -D "${branch_env_target}" >/dev/null 2>&1 || true
  git -C "${repo_root}" branch -D "${branch_env_build}" >/dev/null 2>&1 || true
  git -C "${repo_root}" branch -D "${branch_env_both}" >/dev/null 2>&1 || true
  git -C "${repo_root}" branch -D "${branch_env_sanitize}" >/dev/null 2>&1 || true
  git -C "${repo_root}" branch -D "${branch_overlay}" >/dev/null 2>&1 || true
  git -C "${repo_root}" branch -D "${branch_install_fail}" >/dev/null 2>&1 || true
  git -C "${repo_root}" branch -D "${branch_npm_pin}" >/dev/null 2>&1 || true
  git -C "${repo_root}" branch -D "${branch_codex_missing}" >/dev/null 2>&1 || true
  git -C "${repo_root}" branch -D "${branch_codex_overlay}" >/dev/null 2>&1 || true
  git -C "${repo_root}" branch -D "${branch_codex_untrusted}" >/dev/null 2>&1 || true
  git -C "${repo_root}" branch -D "${branch_codex_dest_untrusted}" >/dev/null 2>&1 || true
  git -C "${repo_root}" branch -D "${branch_codex_source}" >/dev/null 2>&1 || true
  if [ "${sentinel_overlay_created}" = "1" ]; then
    rm -f "${repo_root}/.claude/settings.local.json"
  fi
  if [ "${sentinel_overlay_dir_created}" = "1" ]; then
    rmdir "${repo_root}/.claude" >/dev/null 2>&1 || true
  fi
  rm -rf "${scratch_root}"
}
trap cleanup EXIT

fail=0
ok() { echo "ok   $1"; }
bad() { # <name> <detail...>
  echo "FAIL $1"
  shift
  for line in "$@"; do echo "     ${line}"; done
  fail=1
}

# --- happy path: creating a worktree writes an isolated cargo target-dir
# config, leaves git status clean (the ignore rule covers it), and the
# worktree can be removed without --force. ---
# Every fixture passes HEAD as the explicit base: this selftest exercises
# cargo isolation and overlay copying, not base resolution (which has its
# own selftest), and the checkout under test may lack origin/main entirely
# (CI fetches only the PR ref), where the resolver now refuses to guess.

create_out=""
create_rc=0
create_out="$(cd "${repo_root}" && WORKTREE_ROOT="${scratch_root}" just worktree "${branch_ok}" HEAD 2>&1)" || create_rc=$?

if [ "${create_rc}" -eq 0 ]; then
  ok "worktree creation exits zero"
else
  bad "worktree creation exits zero" "exit ${create_rc}" "${create_out}"
fi

if [ -f "${dest_ok}/.cargo/config.toml" ]; then
  ok "worktree creation writes .cargo/config.toml"
else
  bad "worktree creation writes .cargo/config.toml" "no such file: ${dest_ok}/.cargo/config.toml"
fi

# Exact-content check, not a loose grep: a malformed key name, missing
# quotes, or wrong path would still contain the substring "target-dir" while
# being unusable to cargo.
expected_config=$'[build]\ntarget-dir = "target"\n'
actual_config="$(cat "${dest_ok}/.cargo/config.toml" 2>/dev/null || true)"$'\n'
if [ "${actual_config}" = "${expected_config}" ]; then
  ok "config.toml has the expected [build] target-dir content"
else
  bad "config.toml has the expected [build] target-dir content" \
    "want: ${expected_config@Q}" "got:  ${actual_config@Q}"
fi

# The ignore rule is what makes the status-clean and remove-without-force
# assertions below meaningful, so assert it directly rather than inferring
# it from their outcome.
if git -C "${dest_ok}" check-ignore -q .cargo/config.toml; then
  ok ".cargo/config.toml is covered by the ignore rule"
else
  bad ".cargo/config.toml is covered by the ignore rule" "git check-ignore did not match it"
fi

status_out="$(git -C "${dest_ok}" status --porcelain 2>&1 || true)"
if [ -z "${status_out}" ]; then
  ok "worktree git status is clean after creation"
else
  bad "worktree git status is clean after creation" "${status_out}"
fi

# The untracked .claude/settings.local.json overlay must be carried into
# the worktree when the source checkout has one, and never fabricated when
# it does not. This branch asserts whichever state the running checkout is
# in; the sentinel section further down covers the copy path hermetically
# on checkouts (CI) that have no real overlay.
overlay="${repo_root}/.claude/settings.local.json"
if [ -f "${overlay}" ]; then
  if cmp -s "${overlay}" "${dest_ok}/.claude/settings.local.json"; then
    ok "existing settings.local.json overlay is copied into the worktree"
  else
    bad "existing settings.local.json overlay is copied into the worktree" \
      "missing or differing: ${dest_ok}/.claude/settings.local.json"
  fi
else
  if [ -e "${dest_ok}/.claude/settings.local.json" ]; then
    bad "no overlay in the source checkout means none in the worktree" \
      "unexpected file: ${dest_ok}/.claude/settings.local.json"
  else
    ok "no overlay in the source checkout means none in the worktree"
  fi
fi

# The dependency install, the recipe's only network-bearing step.
# --ignore-scripts is load-bearing and silently so: without it the install's
# root `prepare` script runs `lefthook install`, and because worktrees share
# $GIT_COMMON_DIR/hooks that repoints every checkout of the repository at
# absolute paths inside this worktree, which outlive it. Assert the flag and
# the working directory, not just that npm was reached.
dest_ok_phys="$(cd "${dest_ok}" && pwd -P)"
want_npm_invocation="${dest_ok_phys}"$'\t'"ci --ignore-scripts"
got_npm_invocation="$(cat "${WORKTREE_SELFTEST_NPM_LOG}")"
if [ "${got_npm_invocation}" = "${want_npm_invocation}" ]; then
  ok "worktree creation runs npm ci --ignore-scripts in the destination"
else
  bad "worktree creation runs npm ci --ignore-scripts in the destination" \
    "want: ${want_npm_invocation@Q}" "got:  ${got_npm_invocation@Q}"
fi

if [ -n "${real_mise}" ]; then
  got_mise_tool="$(cat "${WORKTREE_SELFTEST_MISE_LOG}")"
  if [ "${got_mise_tool}" = "npm:npm@${head_npm_pin}" ]; then
    ok "the install runs under the npm the destination declares"
  else
    bad "the install runs under the npm the destination declares" \
      "want: npm:npm@${head_npm_pin}" "got:  ${got_mise_tool@Q}"
  fi
else
  echo "skip the install runs under the npm the destination declares: no mise on PATH"
fi

remove_out=""
remove_rc=0
remove_out="$(git -C "${repo_root}" worktree remove "${dest_ok}" 2>&1)" || remove_rc=$?
if [ "${remove_rc}" -eq 0 ]; then
  ok "git worktree remove succeeds without --force"
else
  bad "git worktree remove succeeds without --force" "exit ${remove_rc}" "${remove_out}"
fi

# --- overlay copy path, hermetic: on a checkout with no real overlay (CI),
# plant a sentinel one, prove the recipe carries it byte-for-byte, and
# remove exactly what was planted. Skipped when a real overlay exists,
# because the happy-path assertion above already exercised the copy and
# overwriting a developer's actual settings to re-prove it is not worth
# the risk. ---
if [ ! -f "${overlay}" ]; then
  if [ ! -d "${repo_root}/.claude" ]; then
    mkdir "${repo_root}/.claude"
    sentinel_overlay_dir_created=1
  fi
  printf '{"probe":"worktree-overlay-%s"}\n' "${pids}" >"${overlay}"
  sentinel_overlay_created=1

  overlay_rc=0
  overlay_out="$(cd "${repo_root}" && WORKTREE_ROOT="${scratch_root}" just worktree "${branch_overlay}" HEAD 2>&1)" || overlay_rc=$?
  if [ "${overlay_rc}" -eq 0 ]; then
    ok "overlay-sentinel worktree creation exits zero"
  else
    bad "overlay-sentinel worktree creation exits zero" "exit ${overlay_rc}" "${overlay_out}"
  fi
  if cmp -s "${overlay}" "${dest_overlay}/.claude/settings.local.json"; then
    ok "sentinel settings.local.json overlay is copied byte-for-byte"
  else
    bad "sentinel settings.local.json overlay is copied byte-for-byte" \
      "missing or differing: ${dest_overlay}/.claude/settings.local.json"
  fi

  # Mirror the .cargo/config.toml assertions: the ignore rule and the
  # non-force removal are what keep the copied overlay from dirtying the
  # worktree, and this sentinel section is the only place they run on CI.
  if git -C "${dest_overlay}" check-ignore -q .claude/settings.local.json; then
    ok "sentinel overlay is covered by the ignore rule in the worktree"
  else
    bad "sentinel overlay is covered by the ignore rule in the worktree" \
      "git check-ignore did not match it"
  fi

  overlay_remove_rc=0
  overlay_remove_out="$(git -C "${repo_root}" worktree remove "${dest_overlay}" 2>&1)" || overlay_remove_rc=$?
  if [ "${overlay_remove_rc}" -eq 0 ]; then
    ok "overlay-sentinel worktree removes without --force"
  else
    bad "overlay-sentinel worktree removes without --force" \
      "exit ${overlay_remove_rc}" "${overlay_remove_out}"
  fi
  git -C "${repo_root}" branch -D "${branch_overlay}" >/dev/null 2>&1 || true
  rm -f "${overlay}"
  sentinel_overlay_created=0
  if [ "${sentinel_overlay_dir_created}" = "1" ]; then
    rmdir "${repo_root}/.claude" >/dev/null 2>&1 || true
    sentinel_overlay_dir_created=0
  fi
fi

# --- Codex policy overlay: use a disposable source worktree so both the
# missing-policy and copy-policy paths are hermetic. The recipe under test is
# mirrored into that source so uncommitted TDD changes are exercised before
# their first commit. It creates both destination worktrees; only the
# controlled source fixture is created directly. ---

codex_source_rc=0
git -C "${repo_root}" worktree add -b "${branch_codex_source}" "${dest_codex_source}" >/dev/null 2>&1 || codex_source_rc=$?
if [ "${codex_source_rc}" -eq 0 ]; then
  ok "Codex policy source fixture creation exits zero"
  cp "${repo_root}/justfile" "${dest_codex_source}/justfile"
  # The worktree recipe delegates new-branch base resolution and the
  # tmpfs/ramfs guard to these scripts, so an uncommitted change to either
  # must reach the fixture the same way the justfile copy above does, or
  # every `just worktree` call below fails with "no such file" instead of
  # exercising the change under test.
  mkdir -p "${dest_codex_source}/scripts"
  cp "${repo_root}/scripts/worktree-base.sh" "${dest_codex_source}/scripts/worktree-base.sh"
  cp "${repo_root}/scripts/require-disk-backed.sh" "${dest_codex_source}/scripts/require-disk-backed.sh"
  printf '[projects."%s"]\ntrust_level = "trusted"\n' \
    "${dest_codex_source}" >"${CODEX_HOME}/config.toml"
else
  bad "Codex policy source fixture creation exits zero" "exit ${codex_source_rc}"
fi

codex_missing_rc=0
codex_missing_out="$(cd "${dest_codex_source}" && WORKTREE_ROOT="${scratch_root}" just worktree "${branch_codex_missing}" HEAD 2>&1)" || codex_missing_rc=$?
if [ "${codex_missing_rc}" -eq 0 ]; then
  ok "missing-policy worktree creation exits zero"
else
  bad "missing-policy worktree creation exits zero" "exit ${codex_missing_rc}" "${codex_missing_out}"
fi
if [ -e "${dest_codex_missing}/.codex/config.toml" ] || [ -e "${dest_codex_missing}/.codex/rules" ]; then
  bad "missing Codex policy creates no destination policy" \
    "unexpected policy under ${dest_codex_missing}/.codex"
else
  ok "missing Codex policy creates no destination policy"
fi
if grep -qF "[projects.\"${dest_codex_missing}\"]" "${CODEX_HOME}/config.toml"; then
  bad "missing Codex policy creates no destination trust entry" \
    "unexpected trust entry for ${dest_codex_missing}"
else
  ok "missing Codex policy creates no destination trust entry"
fi

mkdir -p "${dest_codex_source}/.codex/rules/nested" \
  "${dest_codex_source}/.codex/cache" \
  "${dest_codex_source}/.codex/sessions" \
  "${dest_codex_source}/.codex/state"
printf 'model = "sentinel-%s"\n' "${pids}" >"${dest_codex_source}/.codex/config.toml"
printf 'allow_command(prefix=["just", "check"])\n# sentinel %s\n' "${pids}" \
  >"${dest_codex_source}/.codex/rules/nested/workflow.rules"
printf 'cache-%s\n' "${pids}" >"${dest_codex_source}/.codex/cache/index.bin"
printf 'credential-metadata-%s\n' "${pids}" >"${dest_codex_source}/.codex/credentials.json"
printf 'session-%s\n' "${pids}" >"${dest_codex_source}/.codex/sessions/session.jsonl"
printf 'state-%s\n' "${pids}" >"${dest_codex_source}/.codex/state/cache.bin"

codex_overlay_rc=0
codex_overlay_out="$(cd "${dest_codex_source}" && WORKTREE_ROOT="${scratch_root}" just worktree "${branch_codex_overlay}" HEAD 2>&1)" || codex_overlay_rc=$?
if [ "${codex_overlay_rc}" -eq 0 ]; then
  ok "Codex policy worktree creation exits zero"
else
  bad "Codex policy worktree creation exits zero" "exit ${codex_overlay_rc}" "${codex_overlay_out}"
fi

if cmp -s "${dest_codex_source}/.codex/config.toml" "${dest_codex_overlay}/.codex/config.toml"; then
  ok "Codex config.toml is copied byte-for-byte"
else
  bad "Codex config.toml is copied byte-for-byte" \
    "missing or differing: ${dest_codex_overlay}/.codex/config.toml"
fi
if cmp -s \
  "${dest_codex_source}/.codex/rules/nested/workflow.rules" \
  "${dest_codex_overlay}/.codex/rules/nested/workflow.rules"; then
  ok "nested Codex rules are copied byte-for-byte"
else
  bad "nested Codex rules are copied byte-for-byte" \
    "missing or differing: ${dest_codex_overlay}/.codex/rules/nested/workflow.rules"
fi

if git -C "${dest_codex_overlay}" check-ignore -q .codex/config.toml \
  && git -C "${dest_codex_overlay}" check-ignore -q .codex/rules/nested/workflow.rules; then
  ok "copied Codex policy files remain ignored"
else
  bad "copied Codex policy files remain ignored" \
    "git check-ignore did not match both copied files"
fi

if [ -e "${dest_codex_overlay}/.codex/cache" ] \
  || [ -e "${dest_codex_overlay}/.codex/credentials.json" ] \
  || [ -e "${dest_codex_overlay}/.codex/sessions" ] \
  || [ -e "${dest_codex_overlay}/.codex/state" ]; then
  bad "unrelated .codex content is not copied" \
    "unexpected cache, credentials, sessions, or state content under ${dest_codex_overlay}/.codex"
else
  ok "unrelated .codex content is not copied"
fi

if grep -A1 -F "[projects.\"${dest_codex_overlay}\"]" "${CODEX_HOME}/config.toml" \
  | grep -qF 'trust_level = "trusted"'; then
  ok "trusted Codex source activates policy in the destination"
else
  bad "trusted Codex source activates policy in the destination" \
    "missing trusted entry for ${dest_codex_overlay}"
fi

printf '[projects."%s"]\ntrust_level = "untrusted"\n' \
  "${dest_codex_source}" >"${CODEX_HOME}/config.toml"
codex_untrusted_rc=0
codex_untrusted_out="$(cd "${dest_codex_source}" && WORKTREE_ROOT="${scratch_root}" just worktree "${branch_codex_untrusted}" HEAD 2>&1)" || codex_untrusted_rc=$?
if [ "${codex_untrusted_rc}" -eq 0 ]; then
  ok "untrusted-source Codex worktree creation exits zero"
else
  bad "untrusted-source Codex worktree creation exits zero" \
    "exit ${codex_untrusted_rc}" "${codex_untrusted_out}"
fi
if printf '%s\n' "${codex_untrusted_out}" \
  | grep -qF "Codex: this checkout is untrusted, so copied policy in ${dest_codex_untrusted}/.codex is inactive"; then
  ok "untrusted Codex source warns that copied policy is inactive"
else
  bad "untrusted Codex source warns that copied policy is inactive" \
    "${codex_untrusted_out}"
fi
if grep -qF "[projects.\"${dest_codex_untrusted}\"]" "${CODEX_HOME}/config.toml"; then
  bad "untrusted Codex source does not grant destination trust" \
    "unexpected trust entry for ${dest_codex_untrusted}"
else
  ok "untrusted Codex source does not grant destination trust"
fi

printf '[projects."%s"]\ntrust_level = "trusted"\n\n[projects."%s"]\ntrust_level = "untrusted"\n' \
  "${dest_codex_source}" "${dest_codex_dest_untrusted}" >"${CODEX_HOME}/config.toml"
codex_dest_untrusted_rc=0
codex_dest_untrusted_out="$(cd "${dest_codex_source}" && WORKTREE_ROOT="${scratch_root}" just worktree "${branch_codex_dest_untrusted}" HEAD 2>&1)" || codex_dest_untrusted_rc=$?
if [ "${codex_dest_untrusted_rc}" -eq 0 ]; then
  ok "explicitly untrusted destination worktree creation exits zero"
else
  bad "explicitly untrusted destination worktree creation exits zero" \
    "exit ${codex_dest_untrusted_rc}" "${codex_dest_untrusted_out}"
fi
if printf '%s\n' "${codex_dest_untrusted_out}" \
  | grep -qF "Codex: ${dest_codex_dest_untrusted} has an explicit or unrecognized trust setting"; then
  ok "explicit destination distrust warns that copied policy is inactive"
else
  bad "explicit destination distrust warns that copied policy is inactive" \
    "${codex_dest_untrusted_out}"
fi
if grep -A1 -F "[projects.\"${dest_codex_dest_untrusted}\"]" "${CODEX_HOME}/config.toml" \
  | grep -qF 'trust_level = "untrusted"'; then
  ok "explicit destination distrust remains authoritative"
else
  bad "explicit destination distrust remains authoritative" \
    "destination trust changed for ${dest_codex_dest_untrusted}"
fi

codex_missing_remove_rc=0
git -C "${repo_root}" worktree remove "${dest_codex_missing}" >/dev/null 2>&1 || codex_missing_remove_rc=$?
if [ "${codex_missing_remove_rc}" -eq 0 ]; then
  ok "missing-policy worktree removes without --force"
else
  bad "missing-policy worktree removes without --force" "exit ${codex_missing_remove_rc}"
fi

codex_overlay_remove_rc=0
git -C "${repo_root}" worktree remove "${dest_codex_overlay}" >/dev/null 2>&1 || codex_overlay_remove_rc=$?
if [ "${codex_overlay_remove_rc}" -eq 0 ]; then
  ok "Codex-policy worktree removes without --force"
else
  bad "Codex-policy worktree removes without --force" "exit ${codex_overlay_remove_rc}"
fi

codex_untrusted_remove_rc=0
git -C "${repo_root}" worktree remove "${dest_codex_untrusted}" >/dev/null 2>&1 || codex_untrusted_remove_rc=$?
if [ "${codex_untrusted_remove_rc}" -eq 0 ]; then
  ok "untrusted-source Codex worktree removes without --force"
else
  bad "untrusted-source Codex worktree removes without --force" \
    "exit ${codex_untrusted_remove_rc}"
fi

codex_dest_untrusted_remove_rc=0
git -C "${repo_root}" worktree remove "${dest_codex_dest_untrusted}" >/dev/null 2>&1 || codex_dest_untrusted_remove_rc=$?
if [ "${codex_dest_untrusted_remove_rc}" -eq 0 ]; then
  ok "explicitly untrusted destination worktree removes without --force"
else
  bad "explicitly untrusted destination worktree removes without --force" \
    "exit ${codex_dest_untrusted_remove_rc}"
fi

# --- edge case / negative control: prove the clean-removal assertion above
# is actually load-bearing, not just git's unconditional behavior, by
# showing removal without --force fails once a real (non-ignored) untracked
# file is present. Without this, a regression that made every worktree
# removal succeed unconditionally (e.g. via --force creeping into the
# recipe or a test helper) would go unnoticed. ---

dirty_create_rc=0
(cd "${repo_root}" && WORKTREE_ROOT="${scratch_root}" just worktree "${branch_dirty}" HEAD) >/dev/null 2>&1 || dirty_create_rc=$?
if [ "${dirty_create_rc}" -ne 0 ]; then
  bad "edge case: dirty-worktree fixture creation exits zero" "exit ${dirty_create_rc}"
else
  echo "not tracked, not ignored" >"${dest_dirty}/stray.txt"
  dirty_remove_rc=0
  git -C "${repo_root}" worktree remove "${dest_dirty}" >/dev/null 2>&1 || dirty_remove_rc=$?
  if [ "${dirty_remove_rc}" -ne 0 ]; then
    ok "git worktree remove fails without --force when a real untracked file is present"
  else
    bad "git worktree remove fails without --force when a real untracked file is present" \
      "removal succeeded even with an untracked, non-ignored file present"
  fi
fi

# --- environment-override warnings: cargo honors both CARGO_TARGET_DIR and
# CARGO_BUILD_TARGET_DIR (its generic CARGO_<SECTION>_<KEY> mapping) as
# overrides for [build] target-dir, confirmed empirically with
# CARGO_TARGET_DIR winning when both are set. The recipe must name whichever
# variable cargo actually honors, and must never echo the variable's value:
# an unsanitized value could contain newlines or terminal control sequences
# and forge or obscure other log lines. ---

env_target_out="$(cd "${repo_root}" && WORKTREE_ROOT="${scratch_root}" CARGO_TARGET_DIR="/some/shared/target" just worktree "${branch_env_target}" HEAD 2>&1)" || true
if printf '%s\n' "${env_target_out}" | grep -qF 'warning: CARGO_TARGET_DIR is set and overrides the isolated target-dir'; then
  ok "CARGO_TARGET_DIR override warns"
else
  bad "CARGO_TARGET_DIR override warns" "${env_target_out}"
fi
if printf '%s\n' "${env_target_out}" | grep -qF '/some/shared/target'; then
  bad "CARGO_TARGET_DIR override warning does not echo the value" "${env_target_out}"
else
  ok "CARGO_TARGET_DIR override warning does not echo the value"
fi
git -C "${repo_root}" worktree remove "${dest_env_target}" >/dev/null 2>&1 || true

env_build_out="$(cd "${repo_root}" && WORKTREE_ROOT="${scratch_root}" CARGO_BUILD_TARGET_DIR="/some/shared/target" just worktree "${branch_env_build}" HEAD 2>&1)" || true
if printf '%s\n' "${env_build_out}" | grep -qF 'warning: CARGO_BUILD_TARGET_DIR is set and overrides the isolated target-dir'; then
  ok "CARGO_BUILD_TARGET_DIR override warns"
else
  bad "CARGO_BUILD_TARGET_DIR override warns" "${env_build_out}"
fi
if printf '%s\n' "${env_build_out}" | grep -qF '/some/shared/target'; then
  bad "CARGO_BUILD_TARGET_DIR override warning does not echo the value" "${env_build_out}"
else
  ok "CARGO_BUILD_TARGET_DIR override warning does not echo the value"
fi
git -C "${repo_root}" worktree remove "${dest_env_build}" >/dev/null 2>&1 || true

env_both_out="$(cd "${repo_root}" && WORKTREE_ROOT="${scratch_root}" CARGO_TARGET_DIR="/some/shared/target" CARGO_BUILD_TARGET_DIR="/some/other/shared/target" just worktree "${branch_env_both}" HEAD 2>&1)" || true
if printf '%s\n' "${env_both_out}" | grep -qF "warning: CARGO_TARGET_DIR is set and overrides the isolated target-dir just written to ${dest_env_both}/.cargo/config.toml (CARGO_BUILD_TARGET_DIR is also set but is shadowed by CARGO_TARGET_DIR)"; then
  ok "both-set override names CARGO_TARGET_DIR as the active variable"
else
  bad "both-set override names CARGO_TARGET_DIR as the active variable" "${env_both_out}"
fi
git -C "${repo_root}" worktree remove "${dest_env_both}" >/dev/null 2>&1 || true

# --- sanitization regression: a value with an embedded newline and an ANSI
# escape sequence must never reach the terminal. Before the fix, this
# variable's value was interpolated straight into the warning's echo; a
# malicious or merely unlucky value could forge an extra line or obscure the
# real one. If that regressed, this scenario would leak a forged "ok" line
# and a color escape into the captured output below. ---
malicious_value=$'/tmp/evil\nok   forged line via CARGO_TARGET_DIR\x1b[31mred\x1b[0m'
env_sanitize_out="$(cd "${repo_root}" && WORKTREE_ROOT="${scratch_root}" CARGO_TARGET_DIR="${malicious_value}" just worktree "${branch_env_sanitize}" HEAD 2>&1)" || true
warning_line_count="$(printf '%s\n' "${env_sanitize_out}" | grep -c '^warning: CARGO_TARGET_DIR is set and overrides the isolated target-dir')"
if [ "${warning_line_count}" -eq 1 ]; then
  ok "malicious CARGO_TARGET_DIR value still produces exactly one warning line"
else
  bad "malicious CARGO_TARGET_DIR value still produces exactly one warning line" \
    "got ${warning_line_count} matching lines" "${env_sanitize_out}"
fi
if printf '%s\n' "${env_sanitize_out}" | grep -qF 'forged line'; then
  bad "malicious CARGO_TARGET_DIR value does not inject content into the output" "${env_sanitize_out}"
else
  ok "malicious CARGO_TARGET_DIR value does not inject content into the output"
fi
git -C "${repo_root}" worktree remove "${dest_env_sanitize}" >/dev/null 2>&1 || true

# --- install failure: by the time the install runs, the branch and the
# worktree exist and are correct, so the recipe must fail loudly and leave them
# where they are. Recreating the worktree is not the repair; one command inside
# it is, and the failure has to say so. ---

: >"${WORKTREE_SELFTEST_NPM_LOG}"
install_fail_rc=0
install_fail_out="$(cd "${repo_root}" && WORKTREE_ROOT="${scratch_root}" WORKTREE_SELFTEST_NPM_EXIT=1 just worktree "${branch_install_fail}" HEAD 2>&1)" || install_fail_rc=$?
if [ "${install_fail_rc}" -ne 0 ]; then
  ok "a failing install fails the recipe"
else
  bad "a failing install fails the recipe" "the recipe reported success" "${install_fail_out}"
fi
if printf '%s\n' "${install_fail_out}" | grep -qF "just install"; then
  ok "a failing install names the command that repairs it"
else
  bad "a failing install names the command that repairs it" "${install_fail_out}"
fi
if [ -e "${dest_install_fail}/.git" ]; then
  ok "a failing install leaves the worktree in place"
else
  bad "a failing install leaves the worktree in place" "no checkout at ${dest_install_fail}"
fi
git -C "${repo_root}" worktree remove --force "${dest_install_fail}" >/dev/null 2>&1 || true

# --- destination-declared npm pin: this recipe most often lands on a branch
# that already exists, and npm enforces package.json's devEngines pin on every
# direct invocation, so a branch declaring an npm other than the one on PATH
# fails the install outright unless the recipe honours the declaration it
# finds. Every fixture above is created from HEAD and shares the current pin,
# so this is the only case that can catch a regression to a bare `npm ci`.
#
# The fixture commit is built with plumbing against a scratch index rather than
# by checking anything out. It rewrites the pin and drops the justfile
# altogether, the shape of any branch predating this change, which also proves
# the recipe never reaches for the destination's own recipes. ---

if [ -n "${real_mise}" ]; then
  fixture_pin="11.18.0"
  if [ "${fixture_pin}" = "${head_npm_pin}" ]; then
    bad "the npm pin fixture declares a pin unlike HEAD's" \
      "both are ${fixture_pin}; choose another fixture pin"
  fi
  pin_index="${scratch_root}/npm-pin.index"
  pin_blob="$(git -C "${repo_root}" show HEAD:package.json \
    | jq --arg v "${fixture_pin}" '.devEngines.packageManager.version = $v' \
    | git -C "${repo_root}" hash-object -w --stdin)"
  GIT_INDEX_FILE="${pin_index}" git -C "${repo_root}" read-tree HEAD
  GIT_INDEX_FILE="${pin_index}" git -C "${repo_root}" update-index \
    --cacheinfo "100644,${pin_blob},package.json"
  GIT_INDEX_FILE="${pin_index}" git -C "${repo_root}" update-index --force-remove justfile
  pin_tree="$(GIT_INDEX_FILE="${pin_index}" git -C "${repo_root}" write-tree)"
  # An explicit identity: a CI checkout configures none, and commit-tree
  # refuses to run without one.
  pin_commit="$(GIT_AUTHOR_NAME=selftest GIT_AUTHOR_EMAIL=selftest@invalid \
    GIT_COMMITTER_NAME=selftest GIT_COMMITTER_EMAIL=selftest@invalid \
    git -C "${repo_root}" commit-tree "${pin_tree}" -p HEAD -m "worktree selftest npm pin fixture")"
  git -C "${repo_root}" branch "${branch_npm_pin}" "${pin_commit}" >/dev/null

  : >"${WORKTREE_SELFTEST_MISE_LOG}"
  : >"${WORKTREE_SELFTEST_NPM_LOG}"
  npm_pin_rc=0
  npm_pin_out="$(cd "${repo_root}" && WORKTREE_ROOT="${scratch_root}" just worktree "${branch_npm_pin}" 2>&1)" || npm_pin_rc=$?
  if [ "${npm_pin_rc}" -eq 0 ]; then
    ok "worktree creation on a differently pinned branch exits zero"
  else
    bad "worktree creation on a differently pinned branch exits zero" \
      "exit ${npm_pin_rc}" "${npm_pin_out}"
  fi

  got_pin_tool="$(cat "${WORKTREE_SELFTEST_MISE_LOG}")"
  if [ "${got_pin_tool}" = "npm:npm@${fixture_pin}" ]; then
    ok "the install honours the destination's npm pin, not the running one"
  else
    bad "the install honours the destination's npm pin, not the running one" \
      "want: npm:npm@${fixture_pin}" "got:  ${got_pin_tool@Q}"
  fi
  git -C "${repo_root}" worktree remove "${dest_npm_pin}" >/dev/null 2>&1 || true
else
  echo "skip the destination npm pin fixture: no mise on PATH"
fi

exit "${fail}"
