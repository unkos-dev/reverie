# JS plane: frontend lint/test/build plus whole-tree formatting and Markdown lint.
# JS tools are node-local (frontend/node_modules/.bin or the root devDeps), NOT on
# system PATH, so recipes delegate through `npm run <script>` or `npx --no-install`,
# never a bare binary. The module runs from the repo root (flat layout); frontend
# recipes opt into frontend/ via [working-directory], whole-tree recipes stay at root.

set shell := ["bash", "-ueo", "pipefail", "-c"]

# Runs from the repo root with repo-root-relative paths so the root vite.config.ts
# lint overrides (scoped to frontend/**) match the linted paths literally, instead
# of relying on oxlint resolving a frontend/-CWD-relative path against the root
# config. A bare `vp lint frontend` lints the whole package. Under GitHub
# Actions, --format=github emits workflow annotations so findings land inline
# on the PR diff.
#
# oxlint over the frontend, or a passed file list (--max-warnings 0, matching CI).
[group('js')]
[positional-arguments]
oxlint *files:
    #!/usr/bin/env bash
    set -ueo pipefail
    cd "$(git rev-parse --show-toplevel)"
    if [ "$#" -gt 0 ]; then
        npx --no-install vp lint --max-warnings 0 ${GITHUB_ACTIONS:+--format=github} "$@"
    else
        npx --no-install vp lint --max-warnings 0 ${GITHUB_ACTIONS:+--format=github} frontend
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
markdownlint *files: markdownlint-selftest
    #!/usr/bin/env bash
    set -ueo pipefail
    if [ "$#" -gt 0 ]; then
        npx --no-install markdownlint-cli2 "$@"
    else
        git ls-files -z '*.md' ':!:.claude/**' | xargs -0 -r npx --no-install markdownlint-cli2
    fi

# Exercise YAML loading through the installed Markdownlint CLI before linting the tree.
[group('js')]
markdownlint-selftest:
    scripts/markdownlint-selftest.sh

# Advisory (CI runs it continue-on-error), so deliberately not in `lint`/`check`.
#
# Run the impeccable design anti-pattern detector over the frontend.
[group('js')]
[working-directory('frontend')]
detect:
    npm run detect

# Runs at the repo root over every oxfmt-owned type; never a dependency of
# check/lint. vp's bundled oxfmt is the one formatter; config is the root
# vite.config.ts fmt block.
#
# Format the whole tree in place (TS/JS/JSON/CSS/MD/HTML/TOML/YAML). WRITES.
[group('js')]
fmt:
    npx --no-install vp fmt --write --no-error-on-unmatched-pattern .

# Verify whole-tree formatting without writing (the authoritative CI gate form).
[group('js')]
fmt-check:
    npx --no-install vp fmt --check .

# Frontend lint surface: oxlint, stylelint, then root-CWD markdownlint.
[group('js')]
lint: oxlint stylelint markdownlint

# WCAG axe-core scan against the dev-only design showcase. Playwright's
# webServer owns the dev-server lifecycle (start, wait, teardown), and locally
# reuses an already-running dev server on port 5173, so no hand-rolled server
# block is needed here. Not part of `check`: it boots a browser and a server.
#
# WCAG axe-core scan of the design showcase; boots a browser and a dev server.
[group('js')]
[working-directory('frontend')]
a11y:
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

# Frontend production build (vp build; typecheck rides js::oxlint via tsgo).
[group('js')]
[working-directory('frontend')]
build:
    npm run build

# The CI frontend gate form; cheap and offline, so locally runnable on its own.
#
# Verify the committed Fontshare woff2 files against their pinned checksums.
[group('js')]
[working-directory('frontend/public/fonts/fontshare/files')]
font-integrity:
    sha256sum -c SHA256SUMS

# Locally-runnable verify set: lint + format check (a11y and coverage excluded).
[group('js')]
check: lint fmt-check

# Vite's default port. strictPort is unset in vite.config.ts, so a server
# started while the port is busy would silently shift to the next free one;
# dev-start refuses that case instead so status/stop always describe the
# server actually reachable here.
dev_port := "5173"

# Run the Vite dev server in the foreground (Ctrl-C stops it).
[group('js')]
[working-directory('frontend')]
dev:
    npm run dev

# The server runs in its own session (setsid) so the pidfile names a process
# group covering the whole npm/node tree; dev-stop signals the group, because
# killing only the npm wrapper would orphan the actual Vite process. The
# leader writes its own pid before exec'ing npm, so the pidfile is correct
# whether or not setsid had to fork.
#
# Start the Vite dev server in the background; logs to frontend/.dev-server.log.
[group('js')]
[working-directory('frontend')]
dev-start:
    #!/usr/bin/env bash
    set -ueo pipefail
    pidfile=.dev-server.pid
    log=.dev-server.log
    if [ -s "$pidfile" ] && kill -0 -- "-$(cat "$pidfile")" 2>/dev/null; then
        echo "dev server already running (pid $(cat "$pidfile"), http://localhost:{{ dev_port }}/)"
        exit 0
    fi
    rm -f "$pidfile"
    # Port probe via the bash /dev/tcp builtin: no lsof/ss dependency.
    if (exec 3<> /dev/tcp/127.0.0.1/{{ dev_port }}) 2>/dev/null; then
        echo "port {{ dev_port }} is already serving but was not started by dev-start; stop that process first" >&2
        exit 1
    fi
    setsid bash -c 'echo "$$" > .dev-server.pid; exec npm run dev' > "$log" 2>&1 < /dev/null &
    for _ in $(seq 1 60); do
        if [ -s "$pidfile" ] && ! kill -0 -- "-$(cat "$pidfile")" 2>/dev/null; then
            echo "dev server exited during startup; last log lines:" >&2
            tail -n 20 "$log" >&2
            rm -f "$pidfile"
            exit 1
        fi
        if (exec 3<> /dev/tcp/127.0.0.1/{{ dev_port }}) 2>/dev/null; then
            echo "dev server running (pid $(cat "$pidfile"), http://localhost:{{ dev_port }}/, log frontend/$log)"
            exit 0
        fi
        sleep 0.5
    done
    echo "dev server did not start listening on port {{ dev_port }} within 30s; see frontend/$log" >&2
    exit 1

# Stop the background Vite dev server started by dev-start.
[group('js')]
[working-directory('frontend')]
dev-stop:
    #!/usr/bin/env bash
    set -ueo pipefail
    pidfile=.dev-server.pid
    if [ ! -f "$pidfile" ]; then
        echo "no dev server started by dev-start (no $pidfile); a foreground 'just js::dev' stops with Ctrl-C"
        exit 0
    fi
    pid="$(cat "$pidfile")"
    if ! kill -0 -- "-$pid" 2>/dev/null; then
        rm -f "$pidfile"
        echo "removed stale pidfile (process group $pid is gone)"
        exit 0
    fi
    kill -TERM -- "-$pid"
    for _ in $(seq 1 20); do
        if ! kill -0 -- "-$pid" 2>/dev/null; then
            rm -f "$pidfile"
            echo "dev server stopped (pid $pid)"
            exit 0
        fi
        sleep 0.5
    done
    # Escalate only after TERM had 10s: Vite normally exits promptly, so a
    # survivor here is wedged, and leaving it would strand the port.
    kill -KILL -- "-$pid" 2>/dev/null || true
    rm -f "$pidfile"
    echo "dev server killed after TERM timeout (pid $pid)"

# Distinguishes a dev-start-owned server from any other listener on the port
# (a foreground 'just js::dev', or Playwright's a11y webServer reusing 5173)
# so stop advice matches how the server was started. Exits nonzero when down,
# so the recipe works as a scriptable probe.
#
# Report whether the Vite dev server is up.
[group('js')]
[working-directory('frontend')]
dev-status:
    #!/usr/bin/env bash
    set -ueo pipefail
    pidfile=.dev-server.pid
    if [ -s "$pidfile" ] && kill -0 -- "-$(cat "$pidfile")" 2>/dev/null; then
        echo "dev server running via dev-start (pid $(cat "$pidfile"), http://localhost:{{ dev_port }}/, log frontend/.dev-server.log)"
        exit 0
    fi
    if (exec 3<> /dev/tcp/127.0.0.1/{{ dev_port }}) 2>/dev/null; then
        echo "port {{ dev_port }} is serving, but not via dev-start (foreground 'just js::dev' or another process)"
        exit 0
    fi
    if [ -f "$pidfile" ]; then
        echo "dev server is down (stale pidfile from pid $(cat "$pidfile"); dev-start will clean it up)"
    else
        echo "dev server is down"
    fi
    exit 1
