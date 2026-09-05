---
type: ADR
profile-version: 1
id: "REV-ADR-0036"
title: "Library data-grid stack: two-way bake-off behind a local adapter"
status: "accepted"
recorded-on: "2026-09-05"
decided-on: "2026-07-04"
decision-makers:
  - "John Unkovich"
---

# Library data-grid stack: two-way bake-off behind a local adapter

## Context and problem statement

Reverie commits to a spreadsheet-grade, editable data grid as one of the library's table views: ARIA `grid`
semantics, the full spreadsheet keyboard model (arrows, Home/End, Ctrl+Home/Ctrl+End, PageUp/PageDown, Enter/Escape
edit lifecycle), in-place cell editors that write through the standard metadata pipeline, per-cell and per-row locks,
and virtualised scrolling over 50K-plus rows fed by keyset-cursor lazy loading.

Once production code depends on its API, replacing that grid costs a rewrite. So the choice needs evidence before
commitment, and the commitment needs an abstraction in front of it so gathering that evidence does not itself bind
the codebase to a vendor.

Two questions need answering in order: does interaction quality hold at scale for a real candidate, and then which
candidate wins. Landscape review left two MIT-licensed candidates that each cover the requirement set, plus one
lighter DIY path held in reserve. A single-candidate spike would prove one grid in isolation and leave no comparative
evidence if it flunks; two viable candidates justify a bake-off between them, decided by a rule fixed before either
candidate's numbers exist, so the verdict cannot be back-fitted to a preferred result.

## Decision drivers

- The grid must clear interaction-latency budgets at 50K rows, not just render correctly at small scale.
- License must be permissive (MIT or equivalent); the project ships as open source.
- API stability and maintenance cadence outweigh bundle size, because the grid's surface is a long-lived integration
  contract while bytes are amortisable through code-splitting and a lazy route chunk.
- Whichever grid wins is reached only through a local `GridAdapter` contract, so production code never imports a
  vendor grid directly and a later swap stays a binding change rather than a call-site migration.

## Considered options

- **react-data-grid** (Comcast): native ARIA grid, first-class editor API, small footprint, CSS-variable theming.
  Beta line; breaking changes have landed between beta releases and the React peer requirement has moved mid-line.
- **AG Grid Community**: covers the full requirement set inside the MIT package (multi-column sort, cell editing,
  column show/hide/reorder, an infinite row model, a CSS-variable/JS Theming API). Larger default footprint and a
  default look that needs reskinning; active release cadence.
- **TanStack Table + a virtualiser** (DIY): lightest and license-clean, but the ARIA grid, the spreadsheet keyboard
  model, and the editor lifecycle are all hand-built. Held as a fallback, not a primary candidate.
- Rejected before comparison: glide-data-grid (canvas render with a synthetic accessibility layer), MUI X (column
  reorder and server data source behind a paid tier), LyteNyte (server-side loading behind a paid tier). None clear
  the license-plus-accessibility bar for the required feature set.

## Decision outcome

Chosen option: **react-data-grid**, because AG Grid Community was the only other candidate to clear the license and
feature bar, and of the two, react-data-grid was the one to clear every hard performance budget the decision rule set
in advance.

**Hard budgets (a miss flunks the candidate):**

- Keystroke to cell-move: p95 at most 33 ms over a 200-move scripted sequence, target 16 ms, measured from keydown to
  render commit.
- Scroll: sustained wheel scroll and a Ctrl+End jump at 50K rows with no frame stall over 100 ms.
- Mount: grid interactive under 1 s from route render, with data pre-generated so the number isolates grid
  initialisation.

**Comparative inputs (feed the tiebreak only):** added chunk size from the build report; theming-bridge effort to
apply the Reverie token set (header, row hover, selection, focus ring, density) in light and dark; accessibility (a
clean axe-core scan on the grid region plus a keyboard-walk checklist).

**Decision rule, fixed before measurement:**

1. Both candidates clear the hard budgets: the tiebreak favours API stability and maintenance cadence over bundle
   size.
2. Exactly one candidate fails a hard budget: the other wins.
3. Both fail: neither is adopted; the TanStack DIY fallback is scoped as its own effort with no commitment made here.

Measured at 50K rows with simulated API latency of 0 ms:

| Budget                              | Threshold         | react-data-grid                               | AG Grid Community                             |
| ----------------------------------- | ----------------- | --------------------------------------------- | --------------------------------------------- |
| Keystroke p95 (200 moves)           | ≤ 33 ms           | 27.0 ms (p50 16.6, max 63.1, 0 dropped): pass | 21.3 ms (p50 16.6, max 28.1, 0 dropped): pass |
| Scroll: max frame, wheel + Ctrl+End | no stall > 100 ms | max frame 50.1 ms, 0 stalls: pass             | not measured: fail                            |
| Mount to interactive                | < 1 s             | 12.1 ms: pass                                 | 25.8 ms: pass                                 |

react-data-grid clears all three budgets. AG Grid clears mount and keystroke but does not clear scroll: its scroll
container selector was stale for the AG Grid version under test, and the pre-registered guardrail treats a
"container not found" scroll run as a failed run, not a pass. That is decision rule case 2, and react-data-grid
wins. The tiebreak was not reached; had both cleared all three (case 1), the tiebreak favours API stability and
maintenance cadence over bundle size, which would have pointed at AG Grid instead.

Two non-budget findings sit alongside the verdict: AG Grid's cell editing did not work under test (its text editor
module was unregistered), which the requirement set needs; and AG Grid's installed footprint is roughly 50 times
react-data-grid's (a tiebreak input only). AG Grid's advantage is theming, where its grid follows dark mode out of
the box, while react-data-grid needs an explicit dark-mode token bridge.

Production grid usage goes through the local `GridAdapter` contract; no vendor grid is imported outside a binding
that satisfies it.

### Consequences

- Positive: the verdict criteria are pre-registered, so the choice rests on budgets set before any candidate's
  numbers were visible rather than on a result chosen and justified afterward.
- Positive: both candidates were exercised through the same `GridAdapter` contract, which validated that
  abstraction boundary before production code leans on it and keeps the loser removable as a binding, not a
  rewrite.
- Negative: react-data-grid's beta line has shipped breaking changes between releases and has moved its React peer
  requirement mid-line, which raises upgrade risk over the grid's long lifetime.
- Neutral: the hard budgets were read from a browser harness by hand, not asserted in CI; per-frame timing is flaky
  by construction and belongs in a measured run, not a gate.

## Pros and cons of the options

### react-data-grid

- Positive: ARIA grid semantics and the editor lifecycle are native, and theming is plain CSS variables that map
  directly onto the Reverie token set.
- Positive: the footprint is small, easing the eventual production chunk.
- Negative: the beta line has shipped breaking changes between releases and has moved its React peer requirement
  mid-line, which raises upgrade risk over the grid's long lifetime.

### AG Grid Community

- Positive: the full required feature set is inside the MIT package with an active, predictable release cadence.
- Neutral: theming is a JS Theming API rather than CSS variables, so the token bridge is a parameter object instead
  of a stylesheet.
- Negative: the default footprint is large and the default look needs reskinning to fit the product.

### TanStack Table + virtualiser (DIY)

- Positive: it is the lightest option and license-clean.
- Negative: ARIA grid semantics, the spreadsheet keyboard model, and the editor lifecycle are all hand-built, which
  is why it is a fallback rather than a primary candidate.

## More information

Related decisions: the list contract this grid pages over is [Keyset pagination as the default list
contract](./0019-keyset-pagination-as-the-default-list-contract.md); the frontend data layer is [Frontend data-layer
dependencies: React Query and dnd-kit](./0010-frontend-data-layer-dependencies-react-query-and-dnd-kit.md); the token
set the theming bridge targets is [Radix-generated three-tier dual-theme color
tokens](./0026-radix-generated-three-tier-dual-theme-color-tokens.md); the endpoint shape is [JSON API conventions for
the browser-facing REST surface](./0011-json-api-conventions-for-the-browser-facing-rest-surface.md) and [API
versioning by URL path with OpenAPI as the contract](./0016-api-versioning-by-url-path-with-openapi-as-the-contract.md).

The local contract this decision names `GridAdapter` is implemented in the codebase as `GridBinding`
(`frontend/src/lib/grid/types.ts`, `frontend/src/lib/grid/ReactDataGridBinding.tsx`); react-data-grid is a current
`frontend/package.json` dependency.

Revisit trigger: if the chosen grid later breaks its API or stalls maintenance such that an upgrade becomes
infeasible, reopen the choice with a fresh ADR that supersedes this one.
