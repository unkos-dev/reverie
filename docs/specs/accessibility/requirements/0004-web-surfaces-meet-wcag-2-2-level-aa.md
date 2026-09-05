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

Every web surface Reverie ships MUST conform to WCAG 2.2 Level AA. No element, role, or success criterion is exempt.

## Rationale

A self-hosted reading application has no vendor to fall back on for remediation: a reader who depends on sufficient
colour contrast, keyboard operability, or screen-reader semantics is served by the surface Reverie ships or not served
at all. Binding the obligation to the full WCAG 2.2 Level AA tag set, rather than to a subset chosen for convenience,
is what keeps the target from narrowing quietly over time; the `wcag22aa` tag alone would pass trivially, since it
selects only the rules new in 2.2 and excludes `color-contrast`.

The brand's accent colour needs no exception: Reverie Gold clears the contrast thresholds as a fill with ink-on-gold
text, and wherever gold must read as text or as a hairline the design system uses the darker `accent-text` step, so
the restriction on where gold appears is a brand rule rather than an accessibility concession.

## Acceptance criteria

- Every web surface Reverie ships has no failing WCAG 2.2 Level AA success criterion, as observed by the automated gate
  for the routes it scans and by the manual audit at each release tag for every surface the gate does not reach.
- Every route the automated gate scans passes the full WCAG 2.2 Level AA tag set (2.0 A, 2.0 AA, 2.1 A, 2.1 AA, 2.2 AA)
  with an empty allowlist.
- No accepted exception to this Requirement exists. An allowlist entry that subtracts a violation from the gate's
  verdict is a defect in this Requirement, not a valid application of it, and needs this Requirement rewritten before
  it can stand.
