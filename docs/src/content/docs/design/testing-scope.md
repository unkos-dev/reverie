---
title: Testing Scope for the Design System
description: How the design-system work is tested, and what is deliberately exempt.
---

The Step 10 design system is tested at two distinct bars:

- **Deterministic logic (unit tests, mandatory):** theme provider (initial
  resolution from cookie/DB/`prefers-color-scheme`, persistence, API sync),
  cookie helpers, the custom ESLint hex-literal rule fixtures, and the
  route-gating production-build structural assertion.
- **Visual / composition work (exempt from unit tests):** verified by
  `@axe-core/cli` against `/design/system`, manual Dark/Light toggle, and
  the `/crosscheck` dual-model review gate at D5. Applied-page Lighthouse
  gates are Step 11's responsibility.

This split is deliberate. Snapshotting visual output on per-component unit
tests locks styling into a pixel-brittle contract that Step 11 cannot move.
The design system's acceptance bar is the axe-core hard gate plus a human
review — per-component visual regression tooling is not adopted.
