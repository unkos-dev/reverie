# Website plane: Starlight site build and internal-link check. Runs from website/, an
# npm workspace of the repo root. Deps install once at the root (`just install`)
# and hoist into the root node_modules.

set shell := ["bash", "-ueo", "pipefail", "-c"]
set working-directory := 'website'

# --filter, not a bare `vp build`: `build` is a vp built-in, and a built-in
# cannot be overridden by a same-named script. A bare `vp build` here would run
# Vite against the docs directory and exit 0 having built nothing Astro.
# --fail-if-no-match because a filter matching nothing is also a silent exit 0,
# so a renamed or relocated website package would leave `website::check` green having
# built nothing.
#
# Build the Starlight site (= astro build).
[group('website')]
build:
    vp run --filter website --fail-if-no-match build

# Stages dist under the Pages base path so root-relative /reverie/... links
# resolve, mirroring the CI gate. Run on demand after a build, not in `check`.
#
# Internal-link check over the built site.
[group('website')]
links:
    #!/usr/bin/env bash
    set -ueo pipefail
    if [ ! -f dist/index.html ]; then
        echo "website/dist/index.html missing; run 'just website::build' first" >&2
        exit 1
    fi
    linkroot="$(mktemp -d)"
    trap 'rm -rf "$linkroot"' EXIT
    ln -sfn "$PWD/dist" "$linkroot/reverie"
    lychee --offline --no-progress --root-dir "$linkroot" "$linkroot/reverie"

# Locally-runnable website gate: build only. Run `just website::links` on demand after.
[group('website')]
check: build
