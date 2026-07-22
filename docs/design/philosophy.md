# Reverie Design Philosophy

Reverie is a self-hosted ebook library for people who want their library to
look like a library. Its design register is **boutique cinematic**: warm
canvases, type-led hierarchy, and a single decisive accent (Reverie Gold).
This document captures the conceptual rules; concrete tokens, type scale,
and motion are codified in [Visual Identity](./visual-identity.md) and
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
  (`#A77C00`, `gold-9`) on Light. The accent expresses the most-important
  action on a surface, the focus ring, and the "selected" highlight.
  Nothing else is gold.
- **Warm neutrals.** Ink (`#0E0D0A`), Cream (`#E8E0D0`), and Parchment
  (`#E8DCC2`) anchor the palette. No pure black, no pure white.

## State without hue

State communicates through **typography weight, surface opacity, motion,
and the gold accent** (never a state-coded hue, with a single bounded
exception, **danger** (below)). This is a load-bearing brand invariant:

| State                             | Expression                                                                                                                                                                                                     |
| --------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Default / idle                    | `text-fg`, `bg-surface` (or unchanged)                                                                                                                                                                         |
| Hover (surface lift)              | `translate-y-[-1px]` + `border-border-strong`                                                                                                                                                                  |
| Hover (in-list item)              | `bg-hover` (= `bg-surface-2`); brand gold is reserved for primary affordances and is never a hover treatment                                                                                                   |
| Active / pressed                  | `bg-accent` or `bg-accent-strong`                                                                                                                                                                              |
| Selected                          | `bg-accent-soft` background + `text-fg`                                                                                                                                                                        |
| Disabled                          | `opacity-50` + `text-fg-muted`                                                                                                                                                                                 |
| Loading                           | opacity pulse 0.85 ↔ 1.0, ~1.6s, on the region                                                                                                                                                                 |
| Error (recoverable)               | `text-fg font-semibold` + gold recovery action                                                                                                                                                                 |
| Destructive / unrecoverable error | `--danger` fill, border, or icon (white text on the fill), always paired with an icon, weight, or label. The one state hue (below)                                                                             |
| Success (explicit)                | gold inline note (`text-fg-on-accent` on full `bg-accent` fill); fades after ~3s                                                                                                                               |
| Link                              | underline + `text-accent` on hover; no permanent colour difference                                                                                                                                             |
| Focus (keyboard)                  | universal `:focus-visible`, single 2px `gold-11` outline + 2px offset, no halo; `gold-11` (`--accent-text`) carries the ≥ 3:1 non-text boundary unaided on both themes (see [Color Tokens](./color-tokens.md)) |

`--color-danger` is the **one** sanctioned state hue: reserved for
destructive and unrecoverable-error semantics, never decorative, and always
paired with an icon, weight, or text label (WCAG 1.4.1). The rationale is
recorded in the single-danger-hue decision
(`adr/2026-06-18-single-danger-hue-amends-no-hue-philosophy.md`); see also
[Color Tokens](./color-tokens.md). The canonical token set still
deliberately excludes `--color-success`, `--color-warning`, `--color-info`,
and `--color-neutral`. Adding any further hue-coded state token requires a
separate brand-aligned decision; do not "harmlessly add" them on the
assumption they'll be useful later. Charts and code blocks are scoped
exceptions; when they ship, the deviation is documented in
[Visual Identity](./visual-identity.md) and constrained to the
surface that requires it.

`--color-fg-faint` is **decorative-only**: breadcrumb separators,
ornamental dividers, and similar tertiary glyphs. It is never a
functional-state colour, because `opacity-50 × text-fg-faint` falls
below AA in both themes; that's why the disabled-state mapping above
uses `text-fg-muted` instead.

Brand `--color-accent` (Reverie Gold) is the signature affordance for
primary actions, focus rings, and recovery actions only. It is **not**
a hover treatment. shadcn primitives that ship with `bg-accent` for
hover/focus (dropdown items, select items) bind to `--color-hover`
(= `--color-surface-2`) instead, so the gold register stays
unambiguous.

### Light-theme accent: documented axe deviation

On Parchment the light accent (`#A77C00`, `gold-9`) is ≈ 2.8:1, below the
WCAG 2.2 1.4.11 3:1 floor as a line or as text; darkening it far enough to
clear 3:1 would stop reading as gold. The contrast is carried by _how the
accent is used_: solid fills (large CTAs, primary actions) carry **ink
text** at ≈ 5:1, which clears 1.4.3; the fill, not the gold edge, does the
work. Focus rings sidestep the deviation entirely by using `gold-11`
(`--accent-text`), the text-grade gold that clears the ≥ 3:1 boundary on its
own; no halo required (see [Color Tokens](./color-tokens.md)).

axe-core surfaces 1.4.11 / 1.4.3 violations on any Light surface that uses
the `gold-9` edge or `gold-9` text outside those mitigated cases. The
restriction (use the light accent (`gold-9`) only as a fill (with ink
text) or a recovery action, with focus rings reserved for the compliant
`gold-11`) is the brand's mitigation, and it leaves the accent needing no
accessibility exception: the `/design/system` gallery scans clean, and the
axe gate carries no contrast carve-out for gold. Introducing gold as a
line, an icon stroke, or normal-size text on _new_ Light surfaces is a
brand violation, not axe noise, and reviewers should reject it.

## Motion

Motion is a co-equal axis with colour and typography. The timing budget
sits in the 200–300ms range for interaction feedback and ≤300ms for
page transitions. Reduced-motion preferences disable the loading pulse,
status-dot pulse, and hover lift; they're feedback affordances, not
content.

## Theming

Three preferences (`system` / `light` / `dark`), one cookie, one FOUC
script. The cookie (`reverie_theme`) survives logout by design; it is
device state, not session state. The FOUC pre-paint script reads the
cookie synchronously, sets `<html data-theme>` before React hydrates,
and the canonical theme tree's `[data-theme="dark"]` / `light`
selectors swap palette runtime variables. See
[Visual Identity § Theme Architecture](./visual-identity.md#theme-architecture)
for the cross-stack contract.

## What we don't build

- A framework. Reverie's design system is intentionally narrow; it
  serves Reverie, not arbitrary downstream consumers.
- A token-name framework. The token names map 1:1 onto the brand
  identity's palette. No automation is required to keep them in sync;
  any palette change is a deliberate brand-spec edit.
- Component-level theming knobs. Operators can theme through CSS
  variables (`--canvas`, `--accent`, etc.) at deployment time;
  Reverie does not expose a runtime theme editor.
