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
readonly PAGE="docs/src/content/docs/reference/just.md"

render() {
  # Modules carry their own recipes; the root module's are listed first so the
  # aggregate entry points appear before the per-plane detail.
  just --dump --dump-format json | jq -r '
    def params($r):
      if ($r.parameters | length) == 0 then ""
      else " " + ([$r.parameters[] | "<" + .name + ">"] | join(" "))
      end;

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
              + ((.doc // "") | gsub("\\|"; "\\|"))

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

HEADER
  render
}

if [ "${1:-}" = "--check" ]; then
  if ! diff -u "$PAGE" <(write_page) > /dev/null 2>&1; then
    echo "::error::${PAGE} is stale. Run 'just infra::just-reference' and commit the result." >&2
    diff -u "$PAGE" <(write_page) >&2 || true
    exit 1
  fi
  echo "${PAGE} is up to date"
else
  write_page > "$PAGE"
  echo "wrote ${PAGE}"
fi
