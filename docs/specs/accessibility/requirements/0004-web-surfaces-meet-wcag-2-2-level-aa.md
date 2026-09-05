---
type: REQ
profile-version: 1
id: "REV-REQ-0004"
title: "Web surfaces meet WCAG 2.2 Level AA"
governed-by:
  - "REV-ADR-0040"
---

# Web surfaces meet WCAG 2.2 Level AA

## Statement

Every web surface Reverie ships MUST conform to WCAG 2.2 Level AA, with exactly one accepted carve-out: Reverie Gold on
light surfaces, restricted to focus rings, large calls to action, and recovery actions, MAY fail a Level AA
colour-contrast success criterion (1.4.3 Contrast (Minimum) or 1.4.11 Non-text Contrast) that a strict, unqualified
application of the target would otherwise require it to meet. No other element, role, or success criterion MAY fail
the target.

## Rationale

A self-hosted reading application has no vendor to fall back on for remediation: a reader who depends on sufficient
colour contrast, keyboard operability, or screen-reader semantics is served by the surface Reverie ships or not served
at all. Binding the obligation to the full WCAG 2.2 Level AA tag set, rather than to a subset chosen for convenience,
is what keeps the target from narrowing quietly over time; the `wcag22aa` tag alone would pass trivially, since it
selects only the rules new in 2.2 and excludes `color-contrast`.
The one carve-out exists because Reverie Gold is the brand's identifying accent colour, and confining it to focus
rings, large calls to action, and recovery actions is what stops the exception from spreading to body text or
incidental UI, where a contrast failure would harm ordinary reading and navigation rather than only a bounded set of
high-visibility controls.

## Acceptance criteria

- Every web surface Reverie ships is checked against the full WCAG 2.2 Level AA tag set (2.0 A, 2.0 AA, 2.1 A, 2.1 AA,
  2.2 AA) and has no failing success criterion, except as the carve-out below permits.
- A failing success criterion satisfies this Requirement only when all three of the following hold: the criterion is
  1.4.3 Contrast (Minimum) or 1.4.11 Non-text Contrast; the failing colour is Reverie Gold; and the failing element is
  a focus ring, a large call to action, or a recovery action. A failure meeting only some of these conditions does not
  qualify.
- Reverie Gold used at a Level AA colour-contrast shortfall on any element other than a focus ring, a large call to
  action, or a recovery action (for example body text, a hairline border, or an incidental badge) does not satisfy
  this Requirement.
- No accepted exception to this Requirement exists beyond the one carve-out stated above: an accepted accessibility
  exception that does not meet every condition of that carve-out is a defect in this Requirement, not a valid
  application of it, and needs this Requirement rewritten before it can stand.
