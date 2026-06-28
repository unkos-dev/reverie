# Docs plane: Starlight site build and internal-link check. Runs from docs/, which
# is a separate npm tree from frontend/. Build needs docs/node_modules present
# (run `just docs::install` first on a cold checkout).

set shell := ["bash", "-ueo", "pipefail", "-c"]
set working-directory := 'docs'

# Install docs dependencies (separate npm tree from the frontend).
[group('docs')]
install:
    npm ci

# Build the Starlight site (= astro build).
[group('docs')]
build:
    npm run build

# Stages dist under the Pages base path so root-relative /reverie/... links
# resolve, mirroring the CI gate. Run on demand after a build, not in `check`.
#
# Internal-link check over the built site.
[group('docs')]
links:
    #!/usr/bin/env bash
    set -ueo pipefail
    if [ ! -f dist/index.html ]; then
        echo "docs/dist/index.html missing; run 'just docs::build' first" >&2
        exit 1
    fi
    linkroot="$(mktemp -d)"
    trap 'rm -rf "$linkroot"' EXIT
    ln -sfn "$PWD/dist" "$linkroot/reverie"
    lychee --offline --no-progress --root-dir "$linkroot" "$linkroot/reverie"

# Locally-runnable docs gate: build only. Run `just docs::links` on demand after.
[group('docs')]
check: build
