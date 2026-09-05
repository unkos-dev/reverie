---
type: ADR
profile-version: 1
id: "REV-ADR-0002"
title: "Strict lint policy: pedantic clippy and strict frontend lint"
status: "accepted"
recorded-on: "2026-09-04"
decided-on: "2026-05-03"
decision-makers:
  - "John Unkovich"
---

# Strict lint policy: pedantic clippy and strict frontend lint

## Context and problem statement

`backend/CLAUDE.md` and `frontend/CLAUDE.md` document hard rules: no `unwrap()`/`expect()` in non-test code, no
`let _ = <Result>`, no wildcard imports, no `println!`/`eprintln!`, no `any`, no `!` non-null assertions, no `enum`,
typed catch blocks, `import type` separation, and others. These rules were enforced by code review only. Reviewer
attention is finite, and violations slip in: three real `unwrap()` violations existed in `backend/src/main.rs` that
the rules forbade but no automated check caught.

A separate trial of a third-party AI code reviewer was also underway. Tightening machine-enforced rules first reduces
the territory that reviewer can claim with style-level commentary and shifts its review focus toward logic-level
concerns, where AI review adds the most signal.

Which lint tier should each stack run, and how strict should the machine-enforced floor be?

## Decision drivers

- Review-only enforcement misses violations; reviewer attention is finite and the rules had already been breached
  undetected.
- Fast local feedback matters: a violation should surface at the developer's terminal, before push and before
  reviewer time.
- Machine-enforced style rules narrow the territory available to AI-assisted review, improving its signal-to-noise
  during the trial.
- `backend/CLAUDE.md` and `frontend/CLAUDE.md` should be a single source of truth: lint configuration should match
  what they document, not drift from it.

## Considered options

- Enable strict lint tiers on both stacks (pedantic and nursery clippy groups plus project-specific hard rules on the
  backend, a strict type-aware tier plus the CLAUDE.md-mapped rules on the frontend).
- Status quo: review-only enforcement.
- Pedantic and nursery clippy groups as `deny` instead of `warn`.
- Clippy's `restriction` group, blanket-enabled.
- Per-file `#[allow]` for the noisy pedantic lints instead of a crate-level allow-list.
- Adopt all clippy lints, including the doc-comment pedantic lints.
- The same strict tier on the frontend without a React-specific lint plugin.
- Two separate ADRs, one per stack.

## Decision outcome

Chosen option: **enable strict lint tiers on both stacks**, because review-only enforcement had already let unwrap()
violations reach `main.rs` undetected, and a machine-enforced floor gives fast local feedback while narrowing the
style-level territory available to AI-assisted review.

On the backend, `backend/Cargo.toml` carries a `[lints.clippy]` table: the `clippy::pedantic` and `clippy::nursery`
groups as `warn`, and the project-specific hard rules from `backend/CLAUDE.md` as `deny`: `unwrap_used`,
`expect_used`, `let_underscore_must_use`, `print_stdout`, `print_stderr`, `dbg_macro`, `undocumented_unsafe_blocks`.
`todo` and `unimplemented` stay `warn`, allowed during development and visible at PR time. Third-party APIs the
project bans outright become `disallowed-methods` / `disallowed-types` entries in `backend/clippy.toml`, each
carrying its reason; a ban whose subject is a resolved path belongs there, while a ban a text search can express
belongs in `scripts/backend-guards.sh`, which runs before any toolchain install and is correspondingly cheaper. Four
pedantic lints are allow-listed because they target library API hygiene rather than application correctness:
`module_name_repetitions` (renaming `WritebackOrchestrator` to `Orchestrator` would make re-exports ambiguous and
break IDE jump-to-definition), `missing_errors_doc` and `missing_panics_doc` (the `# Errors` / `# Panics` boilerplate
duplicates what `thiserror` already types, with near-zero reader value in an application crate), and
`must_use_candidate` (near-zero value where call sites are internal). Tokio, ripgrep, rust-analyzer, axum, sqlx,
hyper, and tower all allow-list the same lints. Test code is excluded from the strictest deny rules through a
crate-root `#![cfg_attr(test, allow(...))]`, matching `backend/CLAUDE.md`'s "tests may use them freely" clause.

On the frontend, the same strict tier applies: type-aware strict linting, stable and unique list keys with no
array-index keys, `import type` separated from value imports, no `enum`, and no inline style objects in JSX, each
mapped from a `frontend/CLAUDE.md` rule to a lint rule. The frontend lint engine is oxlint (see
[Adopt oxlint](../../adr/2026-06-27-adopt-oxlint-toolchain.md) for that engine decision).

Both stacks already gated CI on their respective lint commands; the strict tier needed no further CI change beyond
the configuration itself.

### Consequences

- Positive: `backend/CLAUDE.md` and `frontend/CLAUDE.md` rules become CI-gated rather than review-gated, and lint
  configuration matches what the documents state.
- Positive: the policy surfaced real bugs hidden behind unenforced rules: the three `unwrap()` calls in
  `backend/src/main.rs` violated the existing rule but had slipped past review.
- Positive: fast local feedback; `cargo clippy` at the developer's terminal catches violations before push, before
  CI, before reviewer time.
- Positive: less style territory for AI-assisted review to claim, for a cleaner signal-to-noise ratio during that
  trial.
- Negative: a one-time cleanup cost across both stacks to clear the newly-enforced backlog.
- Negative: pedantic and nursery may fire on legitimate patterns; a PR may need a targeted, justified per-line
  suppression. Accepted as a recurring but bounded cost.
- Negative: the CI clippy step runs slower with pedantic and nursery enabled.

## Pros and cons of the options

### Enable strict lint tiers on both stacks

- Positive: closes the gap between documented hard rules and what CI actually checks.
- Negative: a one-time cleanup cost, and an ongoing cost of targeted suppressions for legitimate patterns pedantic or
  nursery flags.

### Status quo: review-only enforcement

- Negative: reviewer drift and missed violations, already evidenced by the `main.rs` unwraps; no fast local feedback;
  `backend/CLAUDE.md` and `frontend/CLAUDE.md` as a source of truth is undermined when their rules are not
  machine-checked.

### Pedantic and nursery clippy groups as deny instead of warn

- Negative: too aggressive for an evolving codebase. `warn` combined with CI's `-D warnings` achieves equivalent
  gating while letting developers see warnings during development without blocking incremental progress.

### Clippy's restriction group, blanket-enabled

- Negative: restriction is an opt-in menu of lints with mutually exclusive goals (for example `shadow_unrelated`
  versus `shadow_reuse`), so it is not a coherent group to enable wholesale. Individual restriction lints
  (`unwrap_used`, `expect_used`, and the rest) are picked deliberately instead.

### Per-file allow for the noisy pedantic lints instead of a crate-level allow-list

- Negative: scatters rationale across the codebase. A crate-level allow-list keeps the policy in one place where it
  can be audited and revisited.

### Adopt all clippy lints, including the doc-comment pedantic lints

- Negative: `missing_errors_doc` and `missing_panics_doc` generate boilerplate doc blocks that restate the error type
  without adding reader value, when error variants are already typed via `thiserror`. Cost-benefit fails for an
  application crate.

### The same strict tier on the frontend without a React-specific lint plugin

- Negative: `frontend/CLAUDE.md`'s rules for stable list keys and the ban on array-index keys need a React-aware lint
  rule to enforce mechanically. The cost of enabling one is trivial next to the enforcement gap of leaving it out.

### Two separate ADRs, one per stack

- Negative: the policy is cross-stack: both stacks adopt the strictest practical lint tier with project-rule
  overlays. Splitting the record would duplicate rationale and risk drift between two records.

## More information

- Clippy lint groups documentation: <https://rust-lang.github.io/rust-clippy/master/>
- Tokio `Cargo.toml` (precedent for an application-crate lint allow-list):
  <https://github.com/tokio-rs/tokio/blob/master/tokio/Cargo.toml>
- Related: `backend/CLAUDE.md` "Conventions" and "Rust Code Rules"; `frontend/CLAUDE.md` "TypeScript" and "Hooks"
  sections.
- The four pedantic lints allow-listed for application-crate reasons (`module_name_repetitions`,
  `missing_errors_doc`, `missing_panics_doc`, `must_use_candidate`) exist for library API hygiene; revisit the
  allow-list if Reverie ever publishes a library crate to crates.io.
- [Tiered comment policy for an open-source codebase](./0004-tiered-comment-policy-for-an-open-source-codebase.md)
  narrows this allow-list: after the library split, `missing_errors_doc` is re-enabled once the per-module docstring
  backfill completes, and `missing_panics_doc` stays allow-listed only until that lands.
