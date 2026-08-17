#!/usr/bin/env bash
# Decide which local verification lanes a working tree actually needs, by
# reading the same .github/path-filters.yml that CI's `changes` job feeds to
# dorny/paths-filter. Prints one `just` recipe name per line, in run order, on
# stdout; `--explain` narrates the decision on stderr.
#
# The point is scope, not leniency: a lane is skipped only when CI would also
# skip it, plus an escalation list for the inputs that change how verification
# itself runs (justfiles, scripts, tool pins) which no CI filter covers because
# CI's repo-lint job is unconditional.
#
# Fails closed everywhere it cannot be certain: an unresolvable base ref, a
# filter with no lane mapping, or a glob shape this matcher cannot translate
# faithfully all abort rather than emit a narrower lane set than the change
# deserves.
set -ueo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd -P)"
cd "$repo_root"

filters_file='.github/path-filters.yml'
base_ref="${PREFLIGHT_BASE:-origin/main}"
files_from=''
explain=0

# Tab separates the two halves of a filter_patterns entry. Held in a variable
# rather than typed inline so a reformat cannot turn the delimiter into spaces
# and silently break every parameter expansion that splits on it.
sep=$'\t'

die() {
  printf 'preflight-scope: %s\n' "$1" >&2
  if [ "$#" -gt 1 ]; then
    printf 'fix: %s\n' "$2" >&2
  fi
  exit 1
}

note() {
  if [ "$explain" -eq 1 ]; then
    printf '%s\n' "$1" >&2
  fi
}

usage() {
  cat >&2 <<'EOF'
usage: scripts/preflight-scope.sh [--base <ref>] [--files-from <path|->] [--explain]

  --base <ref>          compare against <ref> (default: $PREFLIGHT_BASE, else origin/main)
  --files-from <path>   read the changed-path list from a file ("-" for stdin)
                        instead of asking git; used by the self-test
  --filters <path>      read filter definitions from <path> instead of
                        .github/path-filters.yml; used by the self-test
  --explain             narrate the decision on stderr
EOF
  exit 1
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --base)
      [ "$#" -ge 2 ] || usage
      base_ref="$2"
      shift 2
      ;;
    --files-from)
      [ "$#" -ge 2 ] || usage
      files_from="$2"
      shift 2
      ;;
    --filters)
      [ "$#" -ge 2 ] || usage
      filters_file="$2"
      shift 2
      ;;
    --explain)
      explain=1
      shift
      ;;
    -h | --help) usage ;;
    *) die "unknown argument: $1" "run with --help" ;;
  esac
done

# Filter -> local lanes. Every filter defined in the YAML must appear here,
# including the CI-only ones with an empty lane list: an unmapped filter is a
# drift signal (someone added a CI gate without deciding its local story), not
# something to silently ignore.
#
# staging, iac, docker, openapi and npm are deliberately empty. Their CI jobs
# need a built image, a scanner container, or a registry token, so there is no
# faithful local lane to run; `just preflight-full` never ran them either.
lanes_for() {
  case "$1" in
    backend) printf '%s\n' rust::guards db-up rust::check rust::doc-lint rust::test rust::doctests rust::sqlx-check rust::machete ;;
    audit) printf '%s\n' rust::deny ;;
    frontend) printf '%s\n' js::check js::test js::build js::font-integrity ;;
    docs) printf '%s\n' docs::check ;;
    workflows) printf '%s\n' infra::zizmor ;;
    staging | iac | docker | openapi | npm) : ;;
    *) return 1 ;;
  esac
}

# Emission order. Mirrors `just preflight-full`: the static guards that need no
# toolchain, database, or install come first so they fail fastest, db-up
# precedes every DB-backed recipe, and the network-backed audit runs last.
#
# Two recipes are deliberately absent, both because a laptop cannot answer for
# them honestly.
#
# infra::selftests covers this repository's local developer tooling (doctor,
# worktree, the dev-server lifecycle, the detached gate) against stubbed PATH,
# docker, mise, npm, and git fixtures. CI has no stake in any of it. It is a
# recipe to invoke by hand while editing one of those scripts, not a gate.
#
# js::a11y is CI-only for two reasons, and the second is the deciding one. It
# reuses an already-running dev server with no ownership check, so a local run
# scans whichever checkout happens to own port 5173 and reports that verdict as
# this branch's. A dedicated scratch port would fix that much. What it would not
# fix is that a browser and a server do not belong on every frontend branch's
# gate to re-scan routes the branch did not touch. `just js::a11y` remains
# available for the loop that fixes violations, where the caller picks the
# server.
LANE_ORDER=(
  rust::guards
  infra::check
  db-up
  rust::check
  rust::doc-lint
  rust::test
  rust::doctests
  rust::sqlx-check
  rust::machete
  rust::deny
  js::check
  js::test
  js::build
  js::font-integrity
  docs::check
  infra::zizmor
)

# infra::check mirrors CI's repo-lint job, which carries no path filter: it
# lints the whole tree (shellcheck, yamllint, actionlint, prose, and the guards
# whose subject is committed configuration), so any change at all can break it.
ALWAYS_LANES=(infra::check)

# Changes to these reshape verification itself, so a scoped run cannot reason
# about them: the recipe definitions, the scripts those recipes call, the tool
# pins they resolve against, the filter file driving this decision, and the
# hook config. CI has no equivalent filter because its repo-lint job always
# runs; locally the honest answer is to fall back to the full gate.
escalates() {
  case "$1" in
    justfile | mise.toml | lefthook.yml | .github/path-filters.yml) return 0 ;;
    *.just) return 0 ;;
    scripts/*) return 0 ;;
    *) return 1 ;;
  esac
}

[ -f "$filters_file" ] || die "missing ${filters_file}" "restore it from main"

# Strict reader for the restricted YAML shape this file is allowed to use:
# top-level `name:` keys and two-space quoted list items, plus comments and
# blank lines. Anything else is rejected rather than guessed at, so a future
# filter written in a richer YAML dialect fails loudly here instead of being
# silently dropped from the local gate. Both quote styles are accepted because
# the repo formatter rewrites single quotes to double.
filter_names=()
filter_patterns=()  # "name<TAB>pattern"
current=''
lineno=0
while IFS= read -r line || [ -n "$line" ]; do
  lineno=$((lineno + 1))
  if [[ "$line" =~ ^[[:space:]]*(#.*)?$ ]]; then
    continue
  fi
  if [[ "$line" =~ ^([A-Za-z0-9_-]+):$ ]]; then
    current="${BASH_REMATCH[1]}"
    filter_names+=("$current")
    lanes_for "$current" > /dev/null \
      || die "${filters_file}:${lineno}: filter '${current}' has no local lane mapping" \
        "add it to lanes_for() in scripts/preflight-scope.sh (an empty list if it is CI-only)"
    continue
  fi
  if [[ "$line" =~ ^\ \ -\ \'([^\']+)\'$ || "$line" =~ ^\ \ -\ \"([^\"]+)\"$ ]]; then
    [ -n "$current" ] || die "${filters_file}:${lineno}: pattern outside any filter"
    pattern="${BASH_REMATCH[1]}"
    # Translate to the two shapes this matcher implements exactly: a literal
    # path, or a trailing `/**` recursive suffix. Bash pattern matching would
    # happily accept the rest, but its `*` crosses `/` while picomatch's does
    # not, so a pattern like `docs/*.md` would match strictly more here than
    # in CI. Refuse instead of over-matching or under-matching.
    case "$pattern" in
      */'**') : ;;
      *[\*\?\[\]\!]*)
        die "${filters_file}:${lineno}: unsupported glob '${pattern}'" \
          "use a literal path or a trailing '/**', or teach scripts/preflight-scope.sh the new shape"
        ;;
    esac
    filter_patterns+=("${current}${sep}${pattern}")
    continue
  fi
  die "${filters_file}:${lineno}: unparsable line: ${line}"
done < "$filters_file"

[ "${#filter_names[@]}" -gt 0 ] || die "${filters_file} defines no filters"

# Collect changed paths. The local question is "what does my working tree
# change relative to the branch point", so uncommitted and untracked files
# count: preflight runs before the commit that would make them visible to a
# diff against HEAD.
changed=()
if [ -n "$files_from" ]; then
  if [ "$files_from" = '-' ]; then
    mapfile -t changed
  else
    [ -f "$files_from" ] || die "no such file: ${files_from}"
    mapfile -t changed < "$files_from"
  fi
else
  git rev-parse --verify --quiet "${base_ref}^{commit}" > /dev/null \
    || die "cannot resolve base ref '${base_ref}'" \
      "run 'git fetch origin main', or pass --base <ref>"
  merge_base="$(git merge-base "$base_ref" HEAD)" \
    || die "no merge base between '${base_ref}' and HEAD" \
      "fetch the base branch, or pass --base <ref>"
  # --no-renames is load-bearing, not a style choice. With rename detection on,
  # `git diff --name-only` reports a rename as the destination path alone, so
  # moving backend/foo.rs to docs/foo.mdx would select the docs lane and drop
  # every Rust lane the vacated path required. Treating a rename as a delete
  # plus an add names both sides, so no lane a move implicates can be missed.
  #
  # This is parity, not caution: the pinned paths-filter passes --no-renames
  # to every git query it makes, and on pull_request events, where it reads
  # the GitHub API instead, it expands a renamed entry into both its filename
  # and its previous_filename.
  #
  # -z because a path with a space, a quote, or a newline is quoted (and
  # therefore mangled) in git's default line-oriented output. Collected through
  # a file rather than a variable because bash cannot hold the NUL delimiter.
  paths_file="$(mktemp)"
  # shellcheck disable=SC2064  # expand paths_file now: the trap must survive it going out of scope.
  trap "rm -f '${paths_file}'" EXIT
  # A process substitution's exit status is unobservable, so a git failure
  # inside one would surface as a short path list and a confidently narrowed
  # gate. Run each command as its own checkable statement instead.
  git diff --name-only --no-renames -z "$merge_base" > "$paths_file" \
    || die "git diff against ${merge_base} failed" \
      "resolve the repository error above, or pass --files-from"
  git ls-files --others --exclude-standard -z >> "$paths_file" \
    || die "git ls-files failed while listing untracked paths" \
      "resolve the repository error above, or pass --files-from"
  mapfile -d '' -t changed < "$paths_file"
  note "base: ${base_ref} (merge-base ${merge_base})"
fi

# Drop blank entries: a --files-from list may carry them, and an empty string
# would match a `**` prefix test and select lanes for a path that is not there.
filtered=()
for f in "${changed[@]}"; do
  if [ -n "$f" ]; then
    filtered+=("$f")
  fi
done
changed=("${filtered[@]+"${filtered[@]}"}")

if [ "${#changed[@]}" -eq 0 ]; then
  note 'no changed paths; nothing to verify'
  exit 0
fi
note "changed paths: ${#changed[@]}"

emit() {
  # Print the selected lanes in LANE_ORDER, deduplicated.
  local lane
  for lane in "${LANE_ORDER[@]}"; do
    case " ${selected[*]} " in
      *" ${lane} "*) printf '%s\n' "$lane" ;;
    esac
  done
}

for f in "${changed[@]}"; do
  if escalates "$f"; then
    note "escalating to the full gate: '${f}' changes how verification runs"
    selected=("${LANE_ORDER[@]}")
    emit
    exit 0
  fi
done

selected=("${ALWAYS_LANES[@]}")
matched_ci_only=()
for name in "${filter_names[@]}"; do
  hit=''
  for entry in "${filter_patterns[@]}"; do
    [ "${entry%%"$sep"*}" = "$name" ] || continue
    pattern="${entry#*"$sep"}"
    for f in "${changed[@]}"; do
      case "$pattern" in
        */'**')
          prefix="${pattern%'**'}"
          [ "${f##"$prefix"}" != "$f" ] && hit="$f"
          ;;
        *)
          [ "$f" = "$pattern" ] && hit="$f"
          ;;
      esac
      [ -n "$hit" ] && break
    done
    [ -n "$hit" ] && break
  done
  [ -n "$hit" ] || continue
  mapfile -t lanes < <(lanes_for "$name")
  if [ "${#lanes[@]}" -eq 0 ]; then
    matched_ci_only+=("$name")
    note "filter '${name}' matched (${hit}) but has no local lane; CI runs it"
    continue
  fi
  note "filter '${name}' matched (${hit}) -> ${lanes[*]}"
  # The announcement above only fires for a filter with no local lanes at all.
  # frontend has four and still triggers a CI job with no local counterpart, so
  # without this the accessibility gate is the one thing a scoped run silently
  # does not cover.
  if [ "$name" = frontend ]; then
    matched_ci_only+=("frontend's a11y scan")
  fi
  selected+=("${lanes[@]}")
done

if [ "${#matched_ci_only[@]}" -gt 0 ]; then
  printf 'preflight-scope: CI-only gates this change triggers: %s\n' "${matched_ci_only[*]}" >&2
fi

emit
