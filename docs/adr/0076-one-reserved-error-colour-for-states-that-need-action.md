---
type: ADR
profile-version: 1
id: "REV-ADR-0076"
title: "One reserved error colour for states that need action"
status: "accepted"
recorded-on: "2026-11-14"
decided-on: "2026-11-14"
decision-makers:
  - "John Unkovich"
supersedes:
  - "REV-ADR-0027"
---

# One reserved error colour for states that need action

## Context and problem statement

Reverie has one state colour, the red `--danger` scale. It is limited to three contexts: an irreversible destructive
confirmation, an unrecoverable system error, and the border and ring of an invalid input. It never appears as text.
Every other state speaks through weight, opacity, motion and the accent colour.

Two things have changed. The interface now edits in place and saves automatically, so a failed save is a routine error
that the person must notice and retry. It falls outside the three contexts, so it must look calmer than an invalid
field. The product has also committed to conventional interface structure, and red error text beside a control is the
conventional pattern.

Which errors may use the reserved colour, and in what form?

## Decision drivers

- An error that needs action should be unmistakable, in the form people already know.
- Colour must never be the only signal (WCAG 1.4.1), and text must meet its contrast minimum (WCAG 1.4.3).
- The interface stays quiet: one state colour, not a severity ladder.
- The rule should be short enough to apply without a list of approved contexts.
- The colour is a generated member of the token system in both themes.

## Considered options

- **One error colour for any state that needs action.** The reserved red marks every error the person must act on. It
  may be text. No other state colour exists.
- **Keep the three-context limit.** Red stays reserved for destructive confirmation, unrecoverable errors and invalid
  input borders, and is never text.
- **Conventional state palette.** Separate colours for error, warning, success and information.

## Decision outcome

Chosen option: **one error colour for any state that needs action**, because it gives every actionable error the
conventional signal while the interface keeps a single state colour.

The reserved red marks an error that the person must act on, including a failed save and an invalid input, and it keeps
its use on an irreversible destructive confirmation. It may appear as a fill, a border, an icon or text. Red text meets
the WCAG AA contrast minimum on its background. An icon or a text label always carries the same meaning as the colour.

No other state colour is introduced. Success, warning and information stay without a colour of their own and speak
through weight, opacity, motion and the accent colour.

### Consequences

- Positive: a failed save, an invalid field and a system error share one recognisable signal.
- Positive: the rule is one sentence, with no list of approved contexts to maintain.
- Positive: error messages follow the conventional pattern that the brand commitment asks for.
- Negative: red appears more often, so each appearance carries less weight than under the three-context limit.
- Negative: "an error the person must act on" needs judgement, and a contributor can stretch it to a mild warning.
- Negative: red text adds contrast pairs that must be checked in every theme.

## More information

The `--danger` scale is one of the generated families established by REV-ADR-0026. Reconsider this decision if a second
state colour is proposed.
