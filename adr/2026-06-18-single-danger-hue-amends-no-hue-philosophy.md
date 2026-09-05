---
status: "accepted"
date: 2026-06-18
supersedes: []
decision-makers: "John Unkovich"
consulted: []
informed: "Reverie contributors"
---

# A single danger hue amends the no-hue-states policy

## Context and Problem Statement

Reverie expresses UI state through typography weight, surface opacity, motion,
and a single gold accent, never a state-coded hue. There are deliberately no
success, warning, info, or danger color tokens: an error reads as weighted text
plus a gold recovery action, an explicit success as a transient gold note,
selection as a gold-soft background, and so on. This keeps the interface quiet
and the gold accent singular.

That policy serves one class of moments poorly: **irreversible-destructive
actions and unrecoverable system errors.** A user about to permanently delete
data, or facing a state where continuing risks silent data loss, should register
"stop" without having to parse weight and copy alone. Color-blind-safe
reinforcement is required regardless (WCAG 1.4.1: color is never the sole
signal), but the total absence of a danger hue removes a channel that
destructive/error semantics conventionally and effectively use.

Should Reverie introduce a state hue for destructive/error semantics, and if so,
how is it bounded so it does not reopen the door to a general state-color palette?

## Decision Drivers

- Destructive and unrecoverable-error moments need an unmistakable, conventional
  signal; weight and copy alone under-communicate "stop."
- WCAG 1.4.1: color must never be the sole signal; any state hue must always
  pair with an icon, weight, or text label.
- The single-accent, quiet-interface philosophy must stay intact; this can be a
  bounded exception, not the first step of a state-color gradient.
- The hue must be a generated, AA-correct member of the token system in both
  themes, not an ad-hoc hex.

## Considered Options

- Introduce exactly one state hue, `--danger`, reserved for destructive/error.
- Keep zero state hues; express destructive/error with weight plus a gold
  recovery action only.
- Introduce a conventional state palette (success / warning / info / danger).

## Decision Outcome

Chosen option: **introduce exactly one state hue, `--danger`, reserved strictly
for destructive and unrecoverable-error semantics**, amending the no-hue-states
policy with a single bounded exception. `--danger` resolves to a generated red
scale (solid `#B91C1C` at step 9, with theme-tuned text and border steps), in
both themes; `--fg-on-danger` is white on the solid (6.47:1). It appears as a
fill, border, or icon, never as body text, and per WCAG 1.4.1 always pairs
with an icon, weight, or text label, so color alone never carries the meaning.
The shadcn `destructive` alias routes to `--danger` (previously aliased to
`--fg` under the no-hue policy).

No other state hue is introduced: success, warning, and info remain hue-less and
continue to speak through weight, opacity, motion, and the gold accent.
Decorative and categorical tones live in the atmosphere layer, never as a second
accent or a state color.

### Consequences

- Good, because destructive and unrecoverable-error intent gains the
  unmistakable, conventional signal those moments require, while every other
  state stays quiet.
- Good, because the exception is bounded and named (one hue, one purpose) so
  the policy does not erode into a general state palette.
- Good, because `--danger` is a generated, AA-correct scale in both themes,
  consistent with the rest of the token system.
- Bad, because the philosophy is no longer "zero state hues", the simplest
  possible statement, and contributors must learn the one exception and its
  reservation rule.
- Neutral, because `--color-destructive` changes meaning (a real red now, `--fg`
  before); destructive UI re-skins accordingly.

### Confirmation

`--danger` resolves to the generated `--danger-*` scale; a contract test asserts
`--danger` maps to `danger-9` and that `--color-destructive` routes to `--danger`
rather than `--fg`. No `--color-success`, `--color-warning`, or `--color-info`
tokens exist. The reservation (destructive/error only, never decorative, always
paired with a non-color signal) is enforced in review.

## Pros and Cons of the Options

### One reserved danger hue (chosen)

- Good, because it solves the one case the no-hue policy served poorly without
  opening the rest.
- Good, because "one hue, one purpose, always paired" is a teachable, enforceable
  boundary.
- Bad, because it is a genuine exception to an otherwise absolute rule.

### Zero state hues (status quo)

- Good, because it is the simplest, most absolute statement of the philosophy.
- Bad, because destructive confirmation and unrecoverable-error states
  under-communicate when the only signals are weight and copy.

### Conventional state palette (success / warning / info / danger)

- Good, because it matches mainstream convention and needs no per-state argument.
- Bad, because it reintroduces the severity-ladder and "soft alarm" problems the
  no-hue policy exists to avoid, and dilutes the single gold accent. Rejected.

## More Information

The no-hue-states policy this amends is documented in the design philosophy
reference (`docs/design/philosophy.md`, "State without hue") and
the color token reference (`docs/design/color-tokens.md`). The
`--danger` scale is one of the generated families established by the token
architecture decision (`docs/adr/0026-radix-generated-three-tier-dual-theme-color-tokens.md`). The
brand identity reference (`reverie-branding/identity.md`) carries the Reverie
Danger anchor and its reservation rules. Revisit only if a second state hue is
ever proposed, which would mean reopening the no-hue policy, not extending this
exception.
