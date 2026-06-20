---
status: accepted
date: 2026-05-03
supersedes: []
decision-makers: "John Unkovich"
consulted: "—"
informed: "Reverie contributors"
---

# Strict lint policy: clippy pedantic + ESLint strict-tier

## Context and Problem Statement

`backend/CLAUDE.md` and `frontend/CLAUDE.md` document hard rules — no
`unwrap()`/`expect()` in non-test code, no `let _ = <Result>`, no
wildcard imports, no `println!`/`eprintln!`, no `any`, no `!` non-null
assertions, no `enum`, typed catch blocks, `import type` separation,
and others. These rules are currently enforced by code review only.
Reviewer attention is finite; violations slip in. Three real `unwrap()`
violations exist today in `backend/src/main.rs` (lines 216, 218, 226)
that the rules forbid but no automated check catches.

A separate trial of Greptile (third-party AI code reviewer) is also
underway. Tightening machine-enforced rules first reduces the territory
Greptile can claim with style-level commentary and shifts its review
focus toward logic-level concerns where AI review adds the most signal.

## Decision

Enable strict lint tiers on both stacks.

### Backend (Rust)

Add `[lints.clippy]` block to `backend/Cargo.toml`:

- `clippy::pedantic` group as `warn`
- `clippy::nursery` group as `warn` (mature experimental lints —
  `use_self`, `option_if_let_else`, `redundant_pub_crate`, etc.)
- Project-specific rules from `backend/CLAUDE.md` as `deny`:
  `unwrap_used`, `expect_used`, `let_underscore_must_use`,
  `print_stdout`, `print_stderr`, `dbg_macro`,
  `undocumented_unsafe_blocks`
- `todo` and `unimplemented` as `warn` (allowed during dev,
  visible at PR time)
- 4 pedantic lints allow-listed because they target library API
  hygiene, not application correctness:
  - `module_name_repetitions` — would require renaming
    `WritebackOrchestrator` to `Orchestrator` inside `mod writeback`,
    making re-exports ambiguous and breaking IDE jump-to-definition
  - `missing_errors_doc` / `missing_panics_doc` — `# Errors` /
    `# Panics` doc blocks add ~80 boilerplate sections describing
    error variants already typed via `thiserror`; reader value is
    near-zero for an application crate
  - `must_use_candidate` — `#[must_use]` matters for libraries where
    callers might forget to consume a result; near-zero value for
    application crates where call sites are internal

Tokio, ripgrep, rust-analyzer, axum, sqlx, hyper, and tower all
allow-list these same lints.

Test code is excluded from the strictest deny rules via
`#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used,
clippy::print_stdout, clippy::print_stderr))]` at the crate root,
matching `backend/CLAUDE.md`'s "Tests may use them freely" clause.

### Frontend (TypeScript)

Update `frontend/eslint.config.js`:

- Swap `tseslint.configs.recommended` → `tseslint.configs.strictTypeChecked`.
  This enables `no-non-null-assertion`, `no-misused-promises`,
  `use-unknown-in-catch-callback-variable`, and the `no-unsafe-*` family
  that map directly to `frontend/CLAUDE.md` rules.
- Install `eslint-plugin-react`. Enable `react/no-array-index-key` and
  `react/jsx-key` per CLAUDE.md "List `key` values must be stable and
  unique. Never use array index."
- Add `@typescript-eslint/consistent-type-imports` as `error` per
  CLAUDE.md "`import type` separate from value imports."
- Add `no-restricted-syntax` for `TSEnumDeclaration` per CLAUDE.md
  "No `enum` — prefer `as const` objects + union types."
- Add `no-restricted-syntax` for inline `style={{ ... }}` JSX
  attributes per CLAUDE.md "No inline style objects (except for
  genuinely dynamic values)."

Existing carve-outs (Lockup hex literals, shadcn/ui type assertions,
test fixture casts) remain.

### Enforcement

Both stacks already gate `cargo clippy -- -D warnings` and
`npm run lint` in CI. No CI changes needed beyond the config blocks.

### Test code exclusions

- Backend: `#![cfg_attr(test, allow(...))]` at crate root.
- Frontend: `**/*.test.{ts,tsx}` keeps the existing
  `consistent-type-assertions: 'off'` override; strict-tier rules
  apply otherwise (intentional — test code should still avoid `any`,
  `!`, and misused promises).

## Consequences

- Good — `backend/CLAUDE.md` and `frontend/CLAUDE.md` rules become
  CI-gated rather than review-gated. Single source of truth for hard
  rules: lint config matches CLAUDE.md.
- Good: surfaces real bugs hidden behind unenforced rules: 3
  `unwrap()` calls in `backend/src/main.rs` violate the existing rule
  but slipped past review.
- Good — fast local feedback. `cargo clippy` at the dev's terminal
  catches violations before push, before CI, before reviewer time.
- Good: less style territory for Greptile to claim during the trial.
  Cleaner Greptile signal-to-noise.
- Bad: one-time cleanup cost: ~150 backend warnings (~50–60 manual
  after auto-fixes), ~50–100 frontend warnings.
- Bad: pedantic and nursery may fire on legitimate patterns; future
  PRs may need targeted per-line `#[allow(clippy::specific_lint)]`
  with a justification comment. Acceptable cost.
- Bad: CI clippy step ~10% slower with pedantic + nursery enabled.
- Neutral: the 4 noisy-pedantic allow-listed lints could be revisited
  if Reverie ever publishes a library crate to crates.io. Those lints
  exist for library API hygiene and are appropriate there.

## Alternatives Considered

- **Status quo (review-only enforcement).** Rejected: reviewer drift,
  missed violations (already evidenced by main.rs unwraps), no fast
  local feedback. CLAUDE.md as a source of truth is undermined when
  rules aren't machine-checked.
- **Pedantic + nursery as `deny` instead of `warn`.** Rejected — too
  aggressive for an evolving codebase. `warn` + CI's `-D warnings`
  achieves equivalent gating while letting devs see warnings during
  development without blocking incremental progress.
- **Restriction group blanket-enable.** Rejected: restriction is an
  opt-in menu of ~80 lints with mutually-exclusive goals (e.g.,
  `shadow_unrelated` vs `shadow_reuse`). Not a coherent group.
  Individual restriction lints (`unwrap_used`, `expect_used`, etc.)
  are picked deliberately above.
- **Per-file `#[allow]` for the 4 noisy lints instead of crate-level
  allow-list.** Rejected: scatters rationale across the codebase.
  Crate-level allow keeps the policy in one place where it can be
  audited and revisited.
- **Adopt all clippy lints including `missing_errors_doc` /
  `missing_panics_doc`.** Rejected — generates ~80 boilerplate doc
  blocks ("Returns an error if the underlying database operation
  fails") that don't add reader value when error variants are already
  typed via `thiserror`. Cost-benefit fails for an application crate.
- **Same lint set on frontend without `eslint-plugin-react`.**
  Rejected — `frontend/CLAUDE.md` rules for stable list keys and ban
  on array-index keys require `react/no-array-index-key` from
  `eslint-plugin-react`. The cost of one more dev-dep is trivial.
- **Two separate ADRs (one per stack).** Rejected: the policy is
  cross-stack ("both stacks adopt the strictest practical lint tier
  with project-rule overlays"). Splitting would duplicate rationale
  and risk drift between the two ADRs.

## More Information

- MADR 4.0: <https://adr.github.io/madr/>
- Clippy lint groups documentation:
  <https://rust-lang.github.io/rust-clippy/master/>
- typescript-eslint configs:
  <https://typescript-eslint.io/users/configs>
- Tokio Cargo.toml (precedent for application-crate lint allow-list):
  <https://github.com/tokio-rs/tokio/blob/master/tokio/Cargo.toml>
- Related: `backend/CLAUDE.md` "Conventions" + "Rust Code Rules"
- Related: `frontend/CLAUDE.md` "TypeScript" + "Hooks" sections
- Future ADR planned: Greptile AI code review trial (separate decision
  with its own context, consequences, and revisit gate)
