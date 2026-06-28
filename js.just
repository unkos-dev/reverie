# JS plane: frontend lint/test/build plus whole-tree formatting and Markdown lint.
# JS tools are node-local (frontend/node_modules/.bin or the root devDeps), NOT on
# system PATH, so recipes delegate through `npm run <script>` or `npx --no-install`,
# never a bare binary. The module runs from the repo root (flat layout); frontend
# recipes opt into frontend/ via [working-directory], whole-tree recipes stay at root.

set shell := ["bash", "-ueo", "pipefail", "-c"]

# A passed file list is stripped of its leading `frontend/` so repo-root-relative
# staged paths (the hook hand-off) resolve against this recipe's frontend/ CWD.
#
# oxlint over the frontend (delegates to the package.json `lint` script).
[group('js')]
[positional-arguments]
[working-directory('frontend')]
oxlint *files:
    #!/usr/bin/env bash
    set -ueo pipefail
    if [ "$#" -gt 0 ]; then
        args=()
        for f in "$@"; do args+=("${f#frontend/}"); done
        npm run lint -- "${args[@]}"
    else
        npm run lint
    fi

# With an explicit file list, scopes to those paths (leading `frontend/` stripped so
# repo-root-relative staged paths resolve against this frontend/ CWD); otherwise the
# full src glob.
#
# stylelint over frontend CSS (--max-warnings 0, matching CI).
[group('js')]
[positional-arguments]
[working-directory('frontend')]
stylelint *files:
    #!/usr/bin/env bash
    set -ueo pipefail
    if [ "$#" -gt 0 ]; then
        args=()
        for f in "$@"; do args+=("${f#frontend/}"); done
        npx --no-install stylelint --max-warnings 0 "${args[@]}"
    else
        npm run stylelint
    fi

# With an explicit file list, scopes to those paths; otherwise whole-tree
# (archival .claude excluded to match CI).
#
# markdownlint over tracked Markdown at the repo root.
[group('js')]
[positional-arguments]
markdownlint *files:
    #!/usr/bin/env bash
    set -ueo pipefail
    if [ "$#" -gt 0 ]; then
        npx --no-install markdownlint-cli2 "$@"
    else
        git ls-files -z '*.md' ':!:.claude/**' | xargs -0 -r npx --no-install markdownlint-cli2
    fi

# Advisory (CI runs it continue-on-error), so deliberately not in `lint`/`check`.
#
# impeccable design anti-pattern detector.
[group('js')]
[working-directory('frontend')]
detect:
    npm run detect

# Runs at the repo root over every oxfmt-owned type; never a dependency of
# check/lint.
#
# Format the whole tree in place (TS/JS/JSON/CSS/MD/HTML/TOML/YAML). WRITES.
[group('js')]
fmt:
    npx --no-install oxfmt --write --no-error-on-unmatched-pattern .

# Verify whole-tree formatting without writing (the authoritative CI gate form).
[group('js')]
fmt-check:
    npx --no-install oxfmt --check .

# Frontend lint surface: oxlint, stylelint, then root-CWD markdownlint.
[group('js')]
lint: oxlint stylelint markdownlint

# Boots `npm run dev`, waits for readiness, then scans. CI-shaped: not part of
# check (needs a server, and port 5173 collides with the always-on dev server).
#
# WCAG axe-core scan against the dev-only design showcase.
[group('js')]
[working-directory('frontend')]
a11y:
    #!/usr/bin/env bash
    set -ueo pipefail
    # mktemp avoids a predictable /tmp path (no symlink-overwrite surface); setsid +
    # process-group kill tears down the vite child too (not just the npm wrapper), so
    # a re-run does not collide on port 5173 with a leaked server.
    log="$(mktemp)"
    setsid npm run dev > "$log" 2>&1 &
    DEV_PID=$!
    trap 'kill -- -"$DEV_PID" 2>/dev/null || true; rm -f "$log"' EXIT
    for i in $(seq 1 60); do
        if curl -sf http://localhost:5173/design/system > /dev/null; then
            echo "dev server ready"
            break
        fi
        if [ "$i" -eq 60 ]; then
            echo "dev server did not become ready in 60s" >&2
            cat "$log" >&2
            exit 1
        fi
        sleep 1
    done
    npm run a11y

# Frontend unit tests.
[group('js')]
[working-directory('frontend')]
test:
    npm run test

# Frontend unit tests with coverage (the CI form).
[group('js')]
[working-directory('frontend')]
test-coverage:
    npm run test:coverage

# Frontend production build (tsc -b && vite build).
[group('js')]
[working-directory('frontend')]
build:
    npm run build

# Locally-runnable verify set: lint + format check (a11y and coverage excluded).
[group('js')]
check: lint fmt-check
