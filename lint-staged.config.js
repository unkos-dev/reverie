// actionlint version pinned to v1.7.12 — keep in lockstep with the
// `workflow-lint` job in .github/workflows/ci.yml so local pre-commit
// and CI never drift. See CONTRIBUTING.md for install instructions.
module.exports = {
  "*.md": "markdownlint-cli2",
  // The glob covers every type oxfmt formats so the local pre-commit pass
  // matches the authoritative CI gate (`oxfmt --check .` over the whole tree);
  // a type formatted in CI but skipped here would pass commit and fail CI.
  "*.{ts,tsx,js,jsx,mjs,cjs,mts,cts,json,jsonc,css,md,mdx,html,toml}": "oxfmt --write",
  // YAML is split out of the general oxfmt glob into its own sequential
  // task array: lint-staged runs different glob keys concurrently, so leaving
  // `yamllint` on a separate key could race it against `oxfmt --write` and
  // lint pre-/mid-format YAML (flaky local failures). An array runs in order,
  // guaranteeing the formatter completes before the linter reads the file.
  // yamllint mirrors the `repo-lint` CI job against the same ruleset
  // (.yamllint.yaml, auto-discovered from the repo root); actionlint below is
  // workflow-syntax specific, yamllint is the general YAML semantic check.
  // Version pinned in the workspace image (hard-rule-8).
  "*.{yml,yaml}": ["oxfmt --write", "yamllint"],
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
  // Reject issue-tracker references (UNK- + digits) and plan-artifact labels
  // ((S2), decision/invariant numbering, Phase N) in public-facing source —
  // both describe planning artifacts, not the codebase. lint-staged passes only
  // the staged files, so these are changed-files only; each script owns its
  // in-scope policy (no-issue-refs covers source + Markdown; no-plan-refs covers
  // source only, since plans/ADRs/docs legitimately cite plan steps).
  "backend/src/**/*.rs": ["scripts/no-issue-refs.sh", "scripts/no-plan-refs.sh"],
  "frontend/src/**/*.{ts,tsx,js,jsx,css}": ["scripts/no-issue-refs.sh", "scripts/no-plan-refs.sh"],
  // Markdown gets two prose passes: the issue-ref guard, then the Vale prose
  // linter. vale-lint.sh owns the in-scope set (docs/src, adr, repo-root
  // Markdown, minus agent-process files) and runs the same locally and in CI.
  // Hard gate: every ReverieProse rule emits at `error`, so Vale exits non-zero
  // on any finding and blocks the commit (CI gates the same way). Vale is pinned
  // in the workspace image (hard-rule-8).
  "**/*.{md,mdx}": ["scripts/no-issue-refs.sh", "scripts/vale-lint.sh"],
};
