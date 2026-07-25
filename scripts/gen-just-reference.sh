#!/usr/bin/env bash
#
# Generate the just recipe reference from `just --dump-format json`.
#
# The task graph is the source of truth: recipe names, doc comments, groups,
# parameters and dependencies are read from just itself, so the page cannot
# describe a recipe that no longer exists or miss one that was added. Prose
# that is not derivable from the justfiles belongs in the page's front matter
# section here, not hand-edited into the output.
#
# Usage: gen-just-reference.sh            # write the page
#        gen-just-reference.sh --check    # exit 1 if the committed page is stale
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"
# .mdx, not .md: the page uses Starlight's <Aside> component, and the
# standard Markdown loader would render the import line as visible text
# and drop the component. The neighbouring configuration.mdx does the same.
readonly PAGE="docs/src/content/docs/reference/just.mdx"

render() {
  # Modules carry their own recipes; the root module's are listed first so the
  # aggregate entry points appear before the per-plane detail.
  just --dump --dump-format json | jq -r '
    def params($r):
      if ($r.parameters | length) == 0 then ""
      else " " + ([$r.parameters[] | "<" + .name + ">"] | join(" "))
      end;

    # MDX parses a bare `<` as JSX and `{` as an expression, so either one in
    # prose aborts the docs build. Escape both, but only outside code spans:
    # inside backticks the backslash would render literally, and the parameter
    # column is already a code span. Even-indexed segments of a backtick split
    # are the prose.
    def mdx_escape:
      split("`")
      | to_entries
      | map(if (.key % 2) == 0
            then (.value | gsub("<"; "\\<") | gsub("\\{"; "\\{"))
            else .value
            end)
      | join("`");

    # Dependencies are deliberately not rendered: the JSON dump flattens
    # cross-module deps to bare recipe names, so `check: js::check rust::check`
    # arrives as three indistinguishable "check" entries. Listing those would
    # be actively misleading. The doc comments carry the fan-out where it
    # matters.
    def table($mod; $prefix):
      [ $mod.recipes | to_entries[] | .value
        | select(.private | not)
      ] as $rs
      | if ($rs | length) == 0 then empty
        else
          ($rs | sort_by(.name)[]
            | "| `just " + $prefix + .name + params(.) + "` | "
              + ((.doc // "") | gsub("\n"; " ") | mdx_escape | gsub("\\|"; "\\|"))
              + " |")
        end;

    "| Recipe | Purpose |",
    "| ------ | ------- |",
    table(.; ""),
    "",
    ( .modules | to_entries | sort_by(.key)[]
      | "## " + .key,
        "",
        "| Recipe | Purpose |",
        "| ------ | ------- |",
        table(.value; .key + "::"),
        ""
    )
  '
}

write_page() {
  # Each module block emits a trailing blank, so the last one lands at end of
  # file where markdownlint's MD012 rejects it. `cat -s` squeezes runs to one
  # and command substitution drops every trailing newline; printf restores the
  # single one POSIX wants.
  printf '%s\n' "$(_write_page_raw | cat -s)"
}

_write_page_raw() {
  cat <<'HEADER'
---
title: just recipes
description: Every task-runner recipe in the Reverie repository, generated from the justfiles.
---

import { Aside } from '@astrojs/starlight/components';

<Aside type="caution" title="Generated file">
  This page is generated from the justfiles by `just infra::just-reference`.
  Edit the recipe doc comments, not this page.
</Aside>

`just` is the task runner for every plane of the repository. Run `just` with no
arguments for the same list at a terminal, or `just --list` to include groups.

Recipes are namespaced by module: `just rust::check` runs the backend gate,
`just js::check` the frontend one. The unprefixed aggregates at the top fan out
across every plane.

`just preflight` and `just preflight-scoped` end with one machine-readable
verdict line, `GATE: PASS ...` or `GATE: FAIL ... at <lane>`, so a run read back
through a pipe, a truncated log, or a detached shell still says whether it
passed. `just gate-status` replays the last recorded run with its per-lane
timings, warns when the checkout has moved past the commit that run was
recorded on or picked up uncommitted changes it never saw, and exits nonzero
unless that run finished green: distinct statuses tell a failed run, a killed
one, one still in progress, and no recorded run at all apart.

HEADER
  render
}

# oxfmt owns the committed bytes: the pre-commit hook and the CI fmt gate both
# format .mdx, so the generator formats its own output and the drift check
# compares formatted forms. Raw generator output is never canonical; without
# this the formatter and the drift check would each reject the other's bytes.
format_page() {
  npx --no-install vp fmt --write "$1" > /dev/null
}

if [ "${1:-}" = "--check" ]; then
  # The temp copy stays inside the repo so the root vite.config.ts fmt config
  # governs it, but outside docs/src/content so a leftover file could never
  # build as a site page. It is gitignored under the same reasoning.
  TMP_PAGE="docs/gen-just-reference.check.mdx"
  trap 'rm -f "$TMP_PAGE"' EXIT
  write_page > "$TMP_PAGE"
  format_page "$TMP_PAGE"
  if ! diff -u "$PAGE" "$TMP_PAGE" > /dev/null 2>&1; then
    echo "::error::${PAGE} is stale. Run 'just infra::just-reference' and commit the result." >&2
    diff -u "$PAGE" "$TMP_PAGE" >&2 || true
    exit 1
  fi
  echo "${PAGE} is up to date"
else
  write_page > "$PAGE"
  format_page "$PAGE"
  echo "wrote ${PAGE}"
fi
