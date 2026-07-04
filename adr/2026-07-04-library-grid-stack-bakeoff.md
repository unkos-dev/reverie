---
status: "proposed"
date: 2026-07-04
supersedes: []
decision-makers: "John Unkovich"
consulted: "—"
informed: "Reverie contributors"
---

# Library data-grid stack: two-way bake-off behind a local adapter

## Context and Problem Statement

Reverie commits to a spreadsheet-grade, editable data grid as one of the
library's table views: ARIA `grid` semantics, the full spreadsheet keyboard
model (arrows, Home/End, Ctrl+Home/Ctrl+End, PageUp/PageDown, Enter/Escape edit
lifecycle), in-place cell editors that write through the standard metadata
pipeline, per-cell and per-row locks, and virtualized scrolling over 50K-plus
rows fed by keyset-cursor lazy loading.

Once production code depends on its API, replacing that grid costs a rewrite. So
the choice needs evidence before commitment, and the commitment needs an
abstraction in front of it so gathering that evidence does not itself bind the
codebase to a vendor.

Answer two questions in order: does interaction quality hold at scale for a real
candidate, and then which candidate wins. Landscape review left two MIT-licensed
candidates that each cover the requirement set, plus one lighter DIY path held in
reserve. A single-candidate spike would prove one grid in isolation and leave no
comparative evidence if it flunks; two viable candidates justify a bake-off.

This ADR does not pick the winner. It fixes how the winner gets picked: the
evaluation frame, the pass/fail budgets, and the tiebreak rule, set before any
number exists, so the verdict cannot be back-fitted to a preferred result.

## Decision Drivers

- The grid must clear interaction-latency budgets at 50K rows, not just render
  correctly at small scale.
- License must be permissive (MIT or equivalent); the project ships as open
  source.
- API stability and maintenance cadence outweigh bundle size, because the grid's
  surface is a long-lived integration contract while bytes are amortizable
  through code-splitting and a lazy route chunk.
- Whichever grid wins is reached only through a local `GridAdapter` contract, so
  production code never imports a vendor grid directly and a later swap stays a
  binding change rather than a call-site migration.
- The evaluation rig must ship zero surface to production and must stay in-tree
  as the perf harness for every later grid phase.

## Considered Options

- **react-data-grid** (Comcast): native ARIA grid, first-class editor API, small
  footprint, CSS-variable theming. Beta line; breaking changes have landed
  between beta releases and the React peer requirement has moved mid-line.
- **AG Grid Community**: covers the full requirement set inside the MIT package
  (multi-column sort, cell editing, column show/hide/reorder, an infinite row
  model, a CSS-variable/JS Theming API). Larger default footprint and a default
  look that needs reskinning; active release cadence.
- **TanStack Table + a virtualizer** (DIY): lightest and license-clean, but the
  ARIA grid, the spreadsheet keyboard model, and the editor lifecycle are all
  hand-built. Held as a fallback, not a primary candidate.
- Rejected before the bake-off: glide-data-grid (canvas render with a synthetic
  accessibility layer), MUI X (column reorder and server data source behind a
  paid tier), LyteNyte (server-side loading behind a paid tier). None clear the
  license-plus-accessibility bar for the required feature set.

## Decision Outcome

Chosen option: **run a two-way bake-off (react-data-grid and AG Grid Community)
behind one local `GridAdapter` contract, and commit to a winner strictly by the
pre-registered rule below.** The rule is the decision; the winner is filled in
once the harness produces numbers.

**Hard budgets (a miss flunks the candidate):**

- Keystroke to cell-move: p95 at most 33 ms over a 200-move scripted sequence,
  target 16 ms, measured from keydown to render commit.
- Scroll: sustained wheel scroll and a Ctrl+End jump at 50K rows with no frame
  stall over 100 ms.
- Mount: grid interactive under 1 s from route render, with data pre-generated so
  the number isolates grid initialization.

**Comparative inputs (feed the tiebreak only):** added chunk size from the build
report; theming-bridge effort to apply the Reverie token set (header, row hover,
selection, focus ring, density) in light and dark; accessibility (a clean
axe-core scan on the grid region plus a keyboard-walk checklist).

**Decision rule, fixed before measurement:**

1. Both candidates clear the hard budgets: the tiebreak favors API stability and
   maintenance cadence over bundle size.
2. Exactly one candidate fails a hard budget: the other wins.
3. Both fail: neither is adopted; the TanStack DIY fallback is scoped as its own
   effort with no commitment made here.

The winner, its measured budget numbers, and the tiebreak reasoning are recorded
under More Information when the harness run completes, and this ADR flips to
`accepted` at that point.

### Consequences

- Good, because the verdict criteria are pre-registered, so the choice rests on
  budgets set before any candidate's numbers were visible rather than on a result
  chosen and justified afterward.
- Good, because both candidates are exercised through the same `GridAdapter`
  contract, which validates that abstraction boundary before production code
  leans on it and keeps the eventual loser removable as a binding, not a rewrite.
- Good, because the rig is dev-only and build-excluded, adding zero production
  surface while remaining the reusable perf harness for later grid phases.
- Bad, because carrying both candidate libraries as dev dependencies during the
  bake-off doubles the dev-time install and review surface until the loser is
  removed.
- Neutral, because the hard budgets are read from a browser harness by hand, not
  asserted in CI; per-frame timing is flaky by construction and belongs in a
  measured run, not a gate.

### Confirmation

Production grid usage goes through the local `GridAdapter` contract; no vendor
grid is imported outside a binding that satisfies it. The evaluation harness and
both bindings live under the dev-only design tree and are excluded from
production builds, enforced by the build's no-design-chunk assertion (a grid
library reaching `dist/` fails the build).

## Pros and Cons of the Options

### react-data-grid

- Good, because ARIA grid semantics and the editor lifecycle are native, and
  theming is plain CSS variables that map directly onto the Reverie token set.
- Good, because the footprint is small, easing the eventual production chunk.
- Bad, because the beta line has shipped breaking changes between releases and
  has moved its React peer requirement mid-line, raising upgrade risk over the
  grid's long lifetime.

### AG Grid Community

- Good, because the full required feature set is inside the MIT package with an
  active, predictable release cadence.
- Neutral, because theming is a JS Theming API rather than CSS variables, so the
  token bridge is a parameter object instead of a stylesheet.
- Bad, because the default footprint is large and the default look needs
  reskinning to fit the product.

### TanStack Table + virtualizer (DIY)

- Good, because it is the lightest option and license-clean.
- Bad, because ARIA grid semantics, the spreadsheet keyboard model, and the
  editor lifecycle are all hand-built, which is why it is a fallback rather than
  a primary candidate.

## More Information

Related decisions: the list contract this grid pages over is the keyset
pagination decision; the frontend data layer (React Query) is the data-layer
dependencies decision; the token set the theming bridge targets is the dual-theme
tokens decision; the endpoint shape is the JSON API conventions and API
versioning decisions.

Verdict pending: the winner and its measured numbers are recorded here after the
browser QA run, at which point this ADR moves to `accepted`. Revisit trigger: if
the chosen grid later breaks its API or stalls maintenance such that an upgrade
becomes infeasible, reopen the choice with a fresh ADR that supersedes this one.
