---
type: ADR
profile-version: 1
id: "REV-ADR-0060"
title: "Use React Aria for the interactive UI foundation"
status: "proposed"
recorded-on: "2026-10-20"
decision-makers:
  - "John Unkovich"
---

# Use React Aria for the interactive UI foundation

## Context and problem statement

Reverie's command palette and vocabulary pickers depend on `cmdk`, whose release inactivity and unresolved behaviour
issues prompted evaluation of a maintained replacement. The application also owns focus, keyboard and selection code
around its existing primitives. With no published release or users, replacing this foundation carries less migration
cost than maintaining parallel interaction systems after release.

A replacement needs to serve the complete interactive foundation. The global palette can open over route dialogs, and
filter pickers open inside the Refine drawer. React Aria and Radix maintain independent overlay stacks; mixing a React
Aria popup with a Radix dialog can dismiss both layers on one Escape press.

## Decision drivers

- Delegate supported interaction and accessibility behaviour to a maintained library.
- Keep one overlay foundation for nested focus and dismissal.
- Retain Reverie's colours, typography and Tailwind styling.
- Preserve domain values, API contracts and shared state ownership.
- Measure production integration cost rather than infer it from a styled demonstration.

## Considered options

- React Aria Components with Reverie's styling and staged Radix UI retirement.
- Replace only `cmdk` while retaining Radix-owned overlays.
- Adopt React Spectrum's packaged component library.
- Retain the existing libraries and patch `cmdk` locally.

## Decision outcome

Chosen option: **React Aria Components with Reverie's styling and staged Radix UI retirement**, because it supplies a
shared interaction foundation while allowing the application to retain its visual language and domain integration.

React Aria owns interacting overlays as a coherent boundary. Remaining controls migrate separately before the Radix
runtime is removed. Radix Colors and the generated CSS primitives remain independent of this runtime choice. Native
component defaults are preferred; custom interaction behaviour needs a demonstrated problem and an explicit decision.
The grid engine and its editing, sorting and loading model are outside this choice.

### Consequences

- Positive: keyboard, selection, focus and dismissal behaviour can use supported native composition.
- Positive: a single overlay foundation avoids maintaining a bridge between independent dismissal stacks.
- Positive: generated colours and semantic tokens remain usable without adopting Adobe's visual system.
- Negative: application styling wrappers, API integration and domain-state ownership remain local responsibilities.
- Negative: the initial overlay boundary spans several routes, and all touched states need rendered acceptance.
- Negative: bundle size may increase; production builds and actual source deletion determine the adoption cost.
- Negative: both runtime packages remain installed temporarily while non-overlay controls migrate.

## Pros and cons of the options

### Replace only `cmdk` while retaining Radix-owned overlays

- Positive: the initial component migration is narrower.
- Negative: React Aria input and collection behaviour needs composition inside Radix-owned overlays rather than using
  the evaluated native overlay composition, retaining more local integration code.

### Adopt React Spectrum's packaged component library

- Positive: packaged presentation reduces local styling responsibility.
- Negative: its Adobe design system would require a broader visual adoption decision.

### Retain the existing libraries and patch `cmdk` locally

- Positive: existing presentation and integration remain available.
- Negative: the project owns the patch and its maintenance until an upstream fix or replacement.

## More information

- [React Aria Components](https://react-aria.adobe.com/)
- [React Aria modal composition](https://react-aria.adobe.com/Modal)
- [Generated colour contract](./0026-radix-generated-three-tier-dual-theme-color-tokens.md)
- [Refine drawer semantics](./0041-library-filters-in-a-drawer-with-removable-chips.md)
