// actionlint version pinned to v1.7.12 — keep in lockstep with the
// `workflow-lint` job in .github/workflows/ci.yml so local pre-commit
// and CI never drift. See CONTRIBUTING.md for install instructions.
module.exports = {
  "*.md": "markdownlint-cli2",
  "*.{ts,tsx,js,jsx,json,yaml,yml,css,md}": "prettier --write",
  ".github/workflows/**/*.{yml,yaml}": "actionlint -color",
  "*.sh": "shellcheck",
  "Dockerfile*": "hadolint",
  "*.{md,rs,ts,tsx,js,jsx,toml}": "typos",
  // impeccable runs a full scan of frontend/src rather than scanning only
  // staged files: cross-file checks (e.g. neighbours of a touched component)
  // can change verdict, and the wall-time delta is ~400ms. Function form
  // ignores the staged-file list so the same command runs locally and in
  // CI (`npm --prefix frontend run detect`).
  //
  // Advisory posture until the deferred `bg-black` findings in stock
  // shadcn overlays are addressed — see memory entry
  // `project_bg_black_overlays_deferred`. The shell wrapper (`sh -c`)
  // is load-bearing: lint-staged's function form runs the returned
  // string via execa without a shell, so a bare `||` would reach
  // `npm` as an argv token (impeccable then receives `||` and `true`
  // as positional paths and reports "cannot access ||"). Wrapping in
  // `sh -c '...'` restores shell semantics. Strip the wrapper and the
  // `|| true` at the same time as the bg-black fix lands, alongside
  // flipping the CI `continue-on-error` flag.
  "frontend/src/**/*.{ts,tsx,html,css}": () => "sh -c 'npm --prefix frontend run detect || true'",
};
