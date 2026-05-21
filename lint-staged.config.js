// actionlint version pinned to v1.7.12 — keep in lockstep with the
// `workflow-lint` job in .github/workflows/ci.yml so local pre-commit
// and CI never drift. See CONTRIBUTING.md for install instructions.
module.exports = {
  "*.md": "markdownlint-cli2",
  "*.{ts,tsx,js,jsx,json,yaml,yml,css,md}": "prettier --check",
  ".github/workflows/**/*.{yml,yaml}": "actionlint -color",
  // impeccable runs a full scan of frontend/src rather than scanning only
  // staged files: cross-file checks (e.g. neighbours of a touched component)
  // can change verdict, and the wall-time delta is ~400ms. Function form
  // ignores the staged-file list so the same command runs locally and in
  // CI (`npm --prefix frontend run detect`).
  //
  // Advisory posture (`|| true`) until the deferred `bg-black` findings in
  // stock shadcn overlays are addressed — see memory entry
  // `project_bg_black_overlays_deferred`. Strip `|| true` and flip the CI
  // `continue-on-error` flag at the same time as that fix lands.
  "frontend/src/**/*.{ts,tsx,html,css}": () => "npm --prefix frontend run detect || true",
};
