---
title: Reverie Design Philosophy
description: The conceptual register, posture, and decision rules behind Reverie's visual identity.
---

Reverie is a self-hosted ebook library for people who want their library to
look like a library. Its design register is **boutique cinematic**: warm
canvases, type-led hierarchy, and a single decisive accent (Reverie Gold).
This document captures the conceptual rules; concrete tokens, type scale,
and motion are codified in [Visual Identity](/design/visual-identity/) and
the canonical theme tree at `frontend/src/styles/themes/`.

## Brand identity is the source of truth

The brand identity at
[unkos-dev/reverie-branding](https://github.com/unkos-dev/reverie-branding)
is the canonical spec for colour, typography, mark, lockup, and tagline.
This site embeds the load-bearing parts inline so contributors can read
them in context, but the branding repo holds the master record. Any drift
is resolved in branding's favour.

## Register

Reverie is opinionated:

- **Quiet over loud.** Surface chrome stays out of the way; the artwork
  and titles do the talking.
- **Type-led hierarchy.** Author Variable for display, Satoshi Variable
  for body. Weight and size carry the structure; colour does not.
- **One accent.** Reverie Gold (`#C9A961`) on Dark, darkened gold
  (`#8E6F38`) on Light. The accent expresses the most-important action
  on a surface, the focus ring, and the "selected" highlight. Nothing
  else is gold.
- **Warm neutrals.** Ink (`#0E0D0A`), Cream (`#E8E0D0`), and Parchment
  (`#E8DCC2`) anchor the palette. No pure black, no pure white.

## State without hue

State communicates through **typography weight, surface opacity, motion,
and the gold accent** — never a state-coded hue. This is a load-bearing
brand invariant:

| State | Expression |
|---|---|
| Default / idle | `text-fg`, `bg-surface` (or unchanged) |
| Hover | `translate-y-[-1px]` + `border-border-strong` |
| Active / pressed | `bg-accent` or `bg-accent-strong` |
| Selected | `bg-accent-soft` background + `text-fg` |
| Disabled | `opacity-50` + `text-fg-faint` |
| Loading | opacity pulse 0.85 ↔ 1.0, ~1.6s, on the region |
| Error | `text-fg font-semibold` + gold recovery action |
| Success (explicit) | gold inline note (`text-fg-on-accent` on full `bg-accent` fill); fades after ~3s |
| Link | underline + `text-accent` on hover; no permanent colour difference |
| Focus (keyboard) | 2px gold outline + 2px offset (`focus-visible:ring-2 focus-visible:ring-accent focus-visible:ring-offset-2`) |

The canonical token set deliberately does **not** include
`--color-success`, `--color-warning`, `--color-danger`, `--color-info`,
or `--color-neutral`. Adding any hue-coded state token requires a
separate brand-aligned decision; do not "harmlessly add" them on the
assumption they'll be useful later. Charts and code blocks are scoped
exceptions — when they ship, the deviation is documented in
[Visual Identity](/design/visual-identity/) and constrained to the
surface that requires it.

## Motion

Motion is a co-equal axis with colour and typography. The timing budget
sits in the 200–300ms range for interaction feedback and ≤300ms for
page transitions. Reduced-motion preferences disable the loading pulse,
status-dot pulse, and hover lift; they're feedback affordances, not
content.

## Theming

Three preferences (`system` / `light` / `dark`), one cookie, one FOUC
script. The cookie (`reverie_theme`) survives logout by design — it is
device state, not session state. The FOUC pre-paint script reads the
cookie synchronously, sets `<html data-theme>` before React hydrates,
and the canonical theme tree's `[data-theme="dark"]` / `light`
selectors swap palette runtime variables. See
[Visual Identity § Theme Architecture](/design/visual-identity/#theme-architecture)
for the cross-stack contract.

## What we don't build

- A framework. Reverie's design system is intentionally narrow — it
  serves Reverie, not arbitrary downstream consumers.
- A token-name framework. The token names map 1:1 onto the brand
  identity's palette. No automation is required to keep them in sync;
  any palette change is a deliberate brand-spec edit.
- Component-level theming knobs. Operators can theme through CSS
  variables (`--canvas`, `--accent`, etc.) at deployment time;
  Reverie does not expose a runtime theme editor.
