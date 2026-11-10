---
type: ADR
profile-version: 1
id: "REV-ADR-0070"
title: "Library table uses react-data-grid directly"
status: "superseded"
recorded-on: "2026-11-09"
decided-on: "2026-11-09"
decision-makers:
  - "John Unkovich"
supersedes:
  - "REV-ADR-0036"
superseded-by:
  - "REV-ADR-0071"
---

# Library table uses react-data-grid directly

## Context and problem statement

The Library table is Reverie's only data grid. It renders on react-data-grid (RDG), chosen through the two-way bake-off
recorded in
[Library data-grid stack: two-way bake-off behind a local adapter](./0036-library-data-grid-stack-two-way-bake-off-behind-a-local-adapter.md).
That record also required production code to reach the grid only through a local, vendor-neutral contract, so that a
later grid swap would change one binding rather than its callers.

The contract has one binding and one caller. It renames the column, editor, sort and selection types of RDG without
adding behaviour, and it still carries RDG concepts such as the cell tab index and the editor options. Around it sit
replacements for things RDG already does: a two-state wrap over the RDG header sort cycle, a copied checkbox renderer
and a guard for multi-row edits that the table never produces.

Should the Library table keep reaching RDG through the local contract, or use the interfaces RDG ships?

## Decision drivers

- The frontend uses its chosen tools through their documented interfaces and keeps custom code only where a tool has a
  demonstrated gap.
- A layer earns its place by isolating real variation. With one grid and one caller, the contract isolates none.
- The RDG beta line ships breaking changes, so the cost of each upgrade matters.
- The bake-off evidence for RDG over AG Grid Community and a TanStack Table build stays valid.

## Considered options

- **Use react-data-grid directly**: the Library table imports RDG and uses its types, with no local contract.
- **Keep the local adapter**: the Library table stays on the vendor-neutral contract and its RDG binding.
- **AG Grid Community**: replace RDG with the bake-off's other candidate.
- **TanStack Table with a virtualiser**: replace RDG with the bake-off's fallback build.

## Decision outcome

Chosen option: **Use react-data-grid directly**, because the contract isolates no variation that exists today, and
removing it lets the table use the sort cycle, checkbox renderer and edit lifecycle as RDG ships them. The bake-off
result still holds, so the grid stays RDG.

### Consequences

- Positive: the table's grid code reads as the documented RDG API, so the RDG README and types describe it directly.
- Positive: replacements for RDG behaviour, and the tests that only proved them, go away with the contract.
- Negative: an RDG breaking change now lands in `frontend/src/pages/library/table/LibraryTableView.tsx` instead of in
  one binding file, and a future grid swap rewrites the table rather than a binding.
- Neutral: a second grid surface, if one appears, starts from the RDG API rather than from a shared local contract.

## Pros and cons of the options

### Use react-data-grid directly

- Positive: no translation layer between the table and the grid it renders.
- Negative: the table is coupled to the RDG beta API.

### Keep the local adapter

- Positive: a grid swap would stay a binding change.
- Negative: the swap it prepares for has no planned trigger, and the contract already leaks RDG concepts, so a swap
  would still change the caller.

### AG Grid Community

- Negative: it did not clear the bake-off's scroll budget, and its cell editing did not work under test.

### TanStack Table with a virtualiser

- Negative: ARIA grid semantics, the spreadsheet keyboard model and the editor lifecycle would all be built by hand.

## More information

The bake-off budgets, measurements and decision rule are in the superseded record and are not repeated here.

Revisit trigger: if RDG breaks its API or stalls maintenance so that an upgrade becomes infeasible, reopen the grid
choice with a new ADR.
