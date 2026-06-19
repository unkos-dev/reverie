---
title: Color Token System
description: The three-tier editorial color contract — generated Radix primitives, reverie semantic roles, and the sealed atmosphere layer.
---

Reverie's color system is a **three-tier contract**: raw color exists in exactly
one generated layer, UI chrome reads only named semantic roles, and
art-directed "atmosphere" is a sealed parallel namespace chrome may not consume.
This page is the canonical reference for that contract. The wider visual surface
(type scale, motion, theme lifecycle) lives in the Visual Identity reference,
and the
[brand identity](https://github.com/unkos-dev/reverie-branding/blob/main/identity.md)
remains the source of truth for the palette.

## The three tiers

| Tier           | Where                             | Contents                                                                             | Raw color?                         |
| -------------- | --------------------------------- | ------------------------------------------------------------------------------------ | ---------------------------------- |
| 1 — Primitives | `themes/primitives.generated.css` | `--sand-*` / `--gold-*` / `--danger-*` 12-step ramps (+ alpha, P3, contrast, `--bg`) | Yes — generated, never hand-edited |
| 2 — Semantic   | `themes/index.css`                | `--canvas` / `--fg` / `--accent` / `--danger` / `--border` … + shadcn aliases        | No — `var()` references only       |
| 3 — Atmosphere | `themes/atmosphere.css`           | `--atm-*` / `--cover-*` art-directed editorial constants                             | Yes — art-directed                 |

The rule in one line: **components → semantic → primitive; raw color literals
live only in Tier 1 (and the named exceptions). Atmosphere is a sealed parallel
namespace chrome may not read.**

## Primitive step → role legend

The 12-step Radix ramps follow the standard role assignment:

| Step | Role                      | Step | Role                        |
| ---- | ------------------------- | ---- | --------------------------- |
| 1    | app background            | 7    | element border / focus ring |
| 2    | subtle background         | 8    | hovered border              |
| 3    | UI element background     | 9    | solid (purest anchor)       |
| 4    | hovered element bg        | 10   | hovered solid               |
| 5    | active / selected bg      | 11   | low-contrast text (≥ 4.5:1) |
| 6    | subtle border / separator | 12   | high-contrast text          |

## Semantic roles

Tier 2 names the roles chrome consumes: surfaces (`--canvas`, `--surface*`),
lines (`--border`, `--border-strong`, `--border-control`), text (`--fg`,
`--fg-muted`, `--fg-faint`), the single gold accent (`--accent*`,
`--fg-on-accent`), and the danger family below. Each resolves to a Tier 1
primitive; none carry raw color.

## Danger is the one sanctioned state hue

`--danger` (`#B91C1C`, Radix step 9) is the **only** state color. It amends the
otherwise no-hue-states philosophy and is reserved strictly for
destructive / error semantics — **never decorative**. Per WCAG 1.4.1 color is
never the sole signal: danger always pairs with an icon, weight, or text label.
The rationale is recorded in the single-danger-hue ADR
(`adr/2026-06-18-single-danger-hue-amends-no-hue-philosophy.md`).

## Focus indicator (WCAG 1.4.11)

A universal `:focus-visible` rule applies a single 2px `gold-11` outline at a
2px offset (`--focus-ring: var(--accent-text)`), with `box-shadow: none`.
`gold-11` carries the ≥ 3:1 non-text-contrast floor against the page unaided
(≈ 7:1 light / ≈ 10:1 dark), so no outer halo is needed. An earlier design
padded a `gold-9` ring with a high-contrast `sand-12` halo to rescue its
2.8:1 on parchment; moving the ring to `gold-11` removes that need. The
`box-shadow: none` is load-bearing: the rule is unlayered and suppresses the
component-level `focus-visible:ring-*` box-shadows that would otherwise paint a
second concentric ring.

## Governance

- **Hex confinement.** Raw hex is allowed only in the generated primitive file
  (excluded from lint) and the art-directed atmosphere file; the Tier 2
  semantic file is hex-banned by stylelint.
- **Single accent.** Gold is the sole UI accent. Terracotta and all
  categorical / decorative tones live in atmosphere, never as a second accent.
- **Atmosphere review gate.** `--atm-*` additions are art-directed and reviewed;
  UI chrome may not consume them.
- **Finite primitive set.** Adding a primitive means regenerating the Radix
  ramps — a reviewed change, never an ad-hoc hex.

The dual-theme three-tier architecture is recorded in the
token-architecture ADR (`adr/2026-06-18-radix-three-tier-dual-theme-tokens.md`).
