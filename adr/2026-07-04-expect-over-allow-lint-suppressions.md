---
status: accepted
date: 2026-07-04
supersedes: []
decision-makers: "John Unkovich"
consulted: []
informed: "Reverie contributors"
---

# Lint suppressions must be self-purging: #[expect] with a reason, never #[allow]

## Context and Problem Statement

The strict lint policy ([Strict lint policy: pedantic clippy and strict frontend lint](../docs/adr/0002-strict-lint-policy-pedantic-clippy-and-strict-frontend-lint.md))
denies whole lint classes and forces violations to be suppressed at the site.
Those suppressions were written as `#[allow(...)]`, which is invisible to every
tool once the suppressed lint stops firing: nothing reports a stale allow, so
suppressions outlive the code they excused.

The rot was measured, not hypothetical. Converting the backend's suppressions
surfaced roughly 50 fossils out of ~120 sites:

- Every `#[allow(dead_code)]` on a `pub` item predating the lib/bin split went stale the day the crate became a library, because `dead_code`
  cannot fire on public API items. Two of them sat on `rematch_on_isbn_change`
  and its outcome enum, which had long since gained production callers.
- Every `unwrap_used`/`expect_used` suppression in test code was redundant from
  the moment `clippy.toml` pinned `allow-unwrap-in-tests` /
  `allow-expect-in-tests`.
- A `too_many_lines` allow on `update_book_metadata` survived a refactor that
  shrank the function below the threshold, and was caught only by a reviewer
  asking about it.

How do we keep suppressions honest without banning suppression itself, which
the sqlx/utoipa idioms legitimately require?

## Decision Drivers

- A suppression whose lint no longer fires must be impossible to keep silently.
- Every suppression must carry its justification at the site, not in a nearby
  comment that drifts.
- Suppression must remain available: `query!`-per-column match arms and
  `#[utoipa::path]` blocks mandate structural repetition and length that some
  lints flag.
- Enforcement must be mechanical (compiler/CI), not reviewer memory.

## Considered Options

- Deny the `#[allow(...)]` form; require `#[expect(..., reason = "...")]`
- Keep `#[allow(...)]` but require a `reason` string
- Ban suppressing specific lints (e.g. `too_many_lines`) outright

## Decision Outcome

Chosen option: "Deny the `#[allow]` form; require `#[expect(..., reason)]`",
because `#[expect]` is the only variant the compiler polices in both
directions: the suppressed lint firing is expected, and the expectation going
unfulfilled is itself a deny-level signal (`unfulfilled_lint_expectations`)
that evicts the suppression the moment it goes stale. A reason-bearing
`#[allow]` documents intent but still rots silently. Banning specific lints
from suppression forces bad decomposition on idiom-mandated code, the same
miscalibration this repo rejected for the duplication gate.

Two `[lints.clippy]` entries in `backend/Cargo.toml` enforce it:
`allow_attributes = "deny"` and `allow_attributes_without_reason = "deny"`
(the latter also covers reason-less `#[expect]`).

One escape hatch exists: a lint that legitimately fires in only one `cfg`
cannot be an unconditional `#[expect]`, because the expectation is unfulfilled
in the other configuration. Those sites use
`#[cfg_attr(test, allow(..., reason = "..."))]` (the crate root does this for
`unwrap_used`-family lints in test builds). `cfg_attr`-gated allows are the
only sanctioned `allow` form.

### Consequences

- Good, because stale suppressions are now compile errors, not archaeology:
  the migration itself deleted ~50 of them.
- Good, because every remaining suppression states its justification inline in
  a compiler-checked position.
- Bad, because refactors that shrink a function or add a caller now also have
  to delete the corresponding `#[expect]` in the same change. That friction is
  the feature, but it does add a step.
- Neutral, because test-file `unwrap`/`expect` suppression disappears entirely;
  the `clippy.toml` test carve-outs already cover it.

### Confirmation

Enforced by `allow_attributes = "deny"` and
`allow_attributes_without_reason = "deny"` in `backend/Cargo.toml`
`[lints.clippy]`; staleness is enforced by the default-warn
`unfulfilled_lint_expectations` under CI's `-D warnings`.

## More Information

Amends the enforcement mechanics of
[Strict lint policy: pedantic clippy and strict frontend lint](../docs/adr/0002-strict-lint-policy-pedantic-clippy-and-strict-frontend-lint.md) without
changing which lints are denied. Revisit if a future Rust release changes
`#[expect]` semantics across `cfg` boundaries, which would allow retiring the
`cfg_attr` escape hatch.
